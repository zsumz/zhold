use std::fs;

use uuid::Uuid;
use zhold_core::{ByteSize, CollectionPolicy, EvictionReason, plan_collection_with_reservation};

use super::{CollectionReport, CollectionSkip, Retirement, RetirementDisposition};
use crate::{
    Store, StoreError,
    inventory::{ArenaMeasurement, ensure_real_contained_directory, read_arena_snapshot},
    io::{create_json, measure_tree, read_json, remove_json, remove_tree, write_json},
    lock::ExclusiveFileLock,
    manifest::{ArenaManifest, RetirementRecord},
};

pub(crate) fn collect(
    store: &Store,
    policy: CollectionPolicy,
    dry_run: bool,
    measurement: ArenaMeasurement,
) -> Result<CollectionReport, StoreError> {
    let _collection_lock = ExclusiveFileLock::acquire(&store.layout.collection_lock())?;
    collect_locked(store, policy, dry_run, measurement)
}

pub(crate) fn collect_locked(
    store: &Store,
    policy: CollectionPolicy,
    dry_run: bool,
    measurement: ArenaMeasurement,
) -> Result<CollectionReport, StoreError> {
    let inventory = read_arena_snapshot(store, measurement)?;
    if inventory.uncertain_owned > 0 {
        return Err(StoreError::InventoryUncertain {
            count: inventory.uncertain_owned,
        });
    }
    let records = inventory
        .arenas
        .iter()
        .map(|entry| entry.record.clone())
        .collect::<Vec<_>>();
    let plan = plan_collection_with_reservation(&records, policy, inventory.reserved)?;
    if dry_run {
        return Ok(CollectionReport {
            dry_run,
            budget: policy.budget,
            reserved: inventory.reserved,
            reclaimed: ByteSize::ZERO,
            after: plan.projected,
            budget_met: plan.budget_met,
            plan,
            retirements: Vec::new(),
            skipped: Vec::new(),
            history: crate::HistoryWrite::default(),
        });
    }

    let canonical_root =
        store.layout.root().canonicalize().map_err(|error| {
            StoreError::io("canonicalize store root", store.layout.root(), error)
        })?;
    let mut retirements = Vec::new();
    let mut skipped = Vec::new();
    for eviction in &plan.evictions {
        let expected_revision = inventory
            .arenas
            .iter()
            .find_map(|entry| (entry.record.id == eviction.arena_id).then_some(entry.revision));
        let Some(expected_revision) = expected_revision else {
            skipped.push(CollectionSkip {
                arena_id: eviction.arena_id.clone(),
                reason: "arena disappeared from the collection snapshot".to_owned(),
            });
            continue;
        };
        match retire(store, &canonical_root, eviction, expected_revision) {
            Ok(RetirementAttempt::Retired(retirement)) => retirements.push(retirement),
            Ok(RetirementAttempt::Skipped(reason)) => skipped.push(CollectionSkip {
                arena_id: eviction.arena_id.clone(),
                reason,
            }),
            Err(error) => skipped.push(CollectionSkip {
                arena_id: eviction.arena_id.clone(),
                reason: error.to_string(),
            }),
        }
    }
    let (_retired, reclaimed) = accounted_bytes(&retirements);
    let confirmed = read_arena_snapshot(store, measurement)?;
    let after = confirmed.total;
    Ok(CollectionReport {
        dry_run,
        budget: policy.budget,
        reserved: confirmed.reserved,
        plan,
        reclaimed,
        after,
        budget_met: budget_is_confirmed(
            after,
            confirmed.reserved,
            confirmed.uncertain_owned,
            policy.budget,
        ),
        retirements,
        skipped,
        history: crate::HistoryWrite::default(),
    })
}

pub(super) fn budget_is_confirmed(
    total: ByteSize,
    reserved: ByteSize,
    uncertain_owned: u64,
    budget: ByteSize,
) -> bool {
    uncertain_owned == 0 && total.saturating_add(reserved) <= budget
}

pub(super) fn accounted_bytes(retirements: &[Retirement]) -> (ByteSize, ByteSize) {
    retirements.iter().fold(
        (ByteSize::ZERO, ByteSize::ZERO),
        |(retired, reclaimed), retirement| {
            let retired = retired.saturating_add(retirement.size);
            let reclaimed = if matches!(&retirement.disposition, RetirementDisposition::Deleted) {
                reclaimed.saturating_add(retirement.size)
            } else {
                reclaimed
            };
            (retired, reclaimed)
        },
    )
}

#[derive(Debug)]
pub(super) enum RetirementAttempt {
    Retired(Retirement),
    Skipped(String),
}

pub(super) fn retire(
    store: &Store,
    canonical_root: &std::path::Path,
    eviction: &zhold_core::Eviction,
    expected_revision: u64,
) -> Result<RetirementAttempt, StoreError> {
    let Some(_arena_lock) =
        ExclusiveFileLock::try_acquire(&store.layout.arena_lock(&eviction.arena_id))?
    else {
        return Ok(RetirementAttempt::Skipped(
            "arena acquired a live lease after planning".to_owned(),
        ));
    };
    let _metadata_lock =
        ExclusiveFileLock::acquire(&store.layout.metadata_lock(&eviction.arena_id))?;
    let arena = store.layout.arena(&eviction.arena_id);
    if !arena.exists() {
        return Ok(RetirementAttempt::Skipped(
            "arena was already retired after planning".to_owned(),
        ));
    }
    ensure_real_contained_directory(&arena, canonical_root)?;
    let manifest_path = store.layout.manifest(&eviction.arena_id);
    let mut manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.validate(
        store.marker.store_id,
        &eviction.arena_id,
        manifest_path.clone(),
    )?;
    if manifest.revision != expected_revision {
        return Ok(RetirementAttempt::Skipped(
            "arena metadata changed after planning".to_owned(),
        ));
    }
    if manifest.is_pinned_at(crate::time::unix_seconds()?) {
        return Ok(RetirementAttempt::Skipped(
            "arena became pinned after planning".to_owned(),
        ));
    }
    if matches!(eviction.reason, EvictionReason::OrphanedWorktree)
        && manifest.worktree_root.is_dir()
    {
        return Ok(RetirementAttempt::Skipped(
            "orphaned worktree reappeared after planning".to_owned(),
        ));
    }

    let build_dir = store.layout.build_dir(&eviction.arena_id);
    ensure_real_contained_directory(&build_dir, canonical_root)?;
    let trash = store.layout.trash();
    ensure_real_contained_directory(&trash, canonical_root)?;
    let measured = measure_tree(&arena)?;
    let retirement_id = Uuid::new_v4();
    let destination = store
        .layout
        .trash_destination(&eviction.arena_id, retirement_id);
    manifest.prepare_retirement(retirement_id);
    write_json(&manifest_path, &manifest)?;
    let record_path = store.layout.retirement_record(retirement_id);
    let record = RetirementRecord::create(
        store,
        eviction.arena_id.clone(),
        retirement_id,
        manifest.revision,
    );
    if !create_json(&record_path, &record)? {
        return Err(StoreError::InvalidOwnership {
            path: record_path,
            reason: "retirement journal appeared before arena retirement".to_owned(),
        });
    }
    if let Err(error) = fs::rename(&arena, &destination) {
        let _cleanup = remove_json(&record_path);
        return Err(StoreError::io("atomically retire arena", &arena, error));
    }
    let disposition = match remove_tree(&destination) {
        Ok(()) => match remove_json(&record_path) {
            Ok(()) => RetirementDisposition::Deleted,
            Err(error) => RetirementDisposition::PendingDeletion {
                path: record_path,
                error: error.to_string(),
            },
        },
        Err(error) => RetirementDisposition::PendingDeletion {
            path: destination,
            error: error.to_string(),
        },
    };
    Ok(RetirementAttempt::Retired(Retirement {
        arena_id: eviction.arena_id.clone(),
        size: measured,
        reason: eviction.reason,
        disposition,
    }))
}
