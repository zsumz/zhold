use std::{fs, path::Path};

use zhold_core::ByteSize;

use super::{TrashEntry, TrashOutcome, TrashReport, reconcile::reconcile_locked};
use crate::{
    Store, StoreError,
    inventory::ensure_real_contained_directory,
    io::{measure_tree, read_json, remove_json, remove_tree},
    lock::ExclusiveFileLock,
    manifest::{ArenaManifest, RetirementRecord},
};

pub(crate) fn retry_trash(store: &Store, dry_run: bool) -> Result<TrashReport, StoreError> {
    let _collection_lock = ExclusiveFileLock::acquire(&store.layout.collection_lock())?;
    let reconciliation = reconcile_locked(store, dry_run)?;
    let remaining = if dry_run {
        let eligible = reconciliation
            .entries
            .iter()
            .filter(|entry| {
                entry.path.parent() == Some(store.layout.trash().as_path())
                    && matches!(entry.outcome, TrashOutcome::WouldDelete)
            })
            .fold(ByteSize::ZERO, |sum, entry| sum.saturating_add(entry.size));
        reconciliation.before.saturating_sub(eligible)
    } else {
        reconciliation.remaining
    };
    Ok(TrashReport {
        dry_run,
        before: reconciliation.before,
        reclaimed: reconciliation.reclaimed,
        remaining,
        entries: reconciliation.entries,
        history: crate::HistoryWrite::default(),
    })
}

pub(super) fn resume_retirement(
    store: &Store,
    root: &Path,
    record_path: &Path,
    record: &RetirementRecord,
    dry_run: bool,
) -> Result<(TrashEntry, ByteSize), StoreError> {
    let Some(_arena) = ExclusiveFileLock::try_acquire(&store.layout.arena_lock(record.arena_id()))?
    else {
        return Err(StoreError::ArenaActive(record.arena_id().to_string()));
    };
    let _metadata = ExclusiveFileLock::acquire(&store.layout.metadata_lock(record.arena_id()))?;
    ensure_real_contained_directory(record.original_path(), root)?;
    let manifest_path = store.layout.manifest(record.arena_id());
    let manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.validate(store.marker.store_id, record.arena_id(), manifest_path)?;
    if manifest.retirement_id != Some(record.retirement_id())
        || manifest.revision != record.retired_revision()
    {
        return Err(StoreError::InvalidOwnership {
            path: record_path.to_path_buf(),
            reason: "active arena no longer matches its retirement journal".to_owned(),
        });
    }
    let size = measure_tree(record.original_path())?;
    if dry_run {
        return Ok((
            delete_entry(record.original_path(), size, true),
            ByteSize::ZERO,
        ));
    }
    fs::rename(record.original_path(), record.trash_path()).map_err(|error| {
        StoreError::io(
            "resume atomic arena retirement",
            record.original_path(),
            error,
        )
    })?;
    remove_tree(record.trash_path())?;
    remove_json(record_path)?;
    Ok((delete_entry(record.trash_path(), size, false), size))
}

pub(super) fn delete_retired(
    store: &Store,
    root: &Path,
    record_path: &Path,
    record: &RetirementRecord,
    dry_run: bool,
) -> Result<(TrashEntry, ByteSize), StoreError> {
    let Some(_arena) = ExclusiveFileLock::try_acquire(&store.layout.arena_lock(record.arena_id()))?
    else {
        return Err(StoreError::ArenaActive(record.arena_id().to_string()));
    };
    ensure_real_contained_directory(record.trash_path(), root)?;
    let size = measure_tree(record.trash_path())?;
    if !dry_run {
        remove_tree(record.trash_path())?;
        remove_json(record_path)?;
    }
    Ok((
        delete_entry(record.trash_path(), size, dry_run),
        if dry_run { ByteSize::ZERO } else { size },
    ))
}

fn delete_entry(path: &Path, size: ByteSize, dry_run: bool) -> TrashEntry {
    TrashEntry {
        path: path.to_path_buf(),
        size,
        outcome: if dry_run {
            TrashOutcome::WouldDelete
        } else {
            TrashOutcome::Deleted
        },
    }
}
