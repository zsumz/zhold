use std::path::PathBuf;

use crate::{
    ArenaId, ArenaLiveness, ArenaRecord, BuildOutcome, ByteSize, CollectionPolicy, EvictionReason,
    RepositoryId, ToolchainId, WorkspaceId, WorktreeId,
};

use super::{plan_collection, plan_collection_with_reservation};

#[derive(Clone, Copy, Debug)]
struct RecordSpec<'a> {
    key: &'a str,
    bytes: u64,
    last_used_at: u64,
    liveness: ArenaLiveness,
    pinned: bool,
    worktree_exists: bool,
    last_outcome: Option<BuildOutcome>,
}

impl<'a> RecordSpec<'a> {
    const fn idle(key: &'a str, bytes: u64, last_used_at: u64) -> Self {
        Self {
            key,
            bytes,
            last_used_at,
            liveness: ArenaLiveness::Inactive,
            pinned: false,
            worktree_exists: true,
            last_outcome: None,
        }
    }

    const fn active(mut self) -> Self {
        self.liveness = ArenaLiveness::Active;
        self
    }

    const fn pinned(mut self) -> Self {
        self.pinned = true;
        self
    }

    const fn suspect(mut self) -> Self {
        self.liveness = ArenaLiveness::Suspect;
        self
    }

    const fn orphaned(mut self) -> Self {
        self.worktree_exists = false;
        self
    }

    const fn failed(mut self, code: i32) -> Self {
        self.last_outcome = Some(BuildOutcome::Failed(code));
        self
    }
}

#[test]
fn protects_active_and_pinned_arenas() -> Result<(), Box<dyn std::error::Error>> {
    let records = vec![
        record(RecordSpec::idle("active", 70, 1).active()),
        record(RecordSpec::idle("pinned", 40, 2).pinned()),
    ];

    let plan = plan_collection(
        &records,
        CollectionPolicy {
            budget: ByteSize::from_bytes(50),
            low_watermark_percent: 80,
        },
    )?;

    assert!(plan.evictions.is_empty());
    assert!(!plan.budget_met);
    assert_eq!(plan.protected, ByteSize::from_bytes(110));
    Ok(())
}

#[test]
fn protects_an_unfinished_arena_after_its_lease_disappears()
-> Result<(), Box<dyn std::error::Error>> {
    let suspect = record(RecordSpec::idle("suspect", 70, 1).suspect());

    let plan = plan_collection(
        &[suspect],
        CollectionPolicy {
            budget: ByteSize::from_bytes(1),
            low_watermark_percent: 100,
        },
    )?;

    assert!(plan.evictions.is_empty());
    assert!(!plan.budget_met);
    assert_eq!(plan.protected, ByteSize::from_bytes(70));
    Ok(())
}

#[test]
fn evicts_orphans_then_failed_then_lru() -> Result<(), Box<dyn std::error::Error>> {
    let records = vec![
        record(RecordSpec::idle("warm", 30, 30)),
        record(RecordSpec::idle("failed", 30, 20).failed(1)),
        record(RecordSpec::idle("orphan", 30, 40).orphaned()),
        record(RecordSpec::idle("old", 30, 10)),
    ];

    let plan = plan_collection(
        &records,
        CollectionPolicy {
            budget: ByteSize::from_bytes(100),
            low_watermark_percent: 60,
        },
    )?;

    assert_eq!(plan.evictions.len(), 2);
    assert_eq!(plan.evictions[0].reason, EvictionReason::OrphanedWorktree);
    assert_eq!(plan.evictions[1].reason, EvictionReason::FailedBuild);
    assert_eq!(plan.projected, ByteSize::from_bytes(60));
    Ok(())
}

#[test]
fn rejects_an_invalid_low_watermark() {
    let error = plan_collection(
        &[],
        CollectionPolicy {
            budget: ByteSize::from_bytes(100),
            low_watermark_percent: 0,
        },
    );

    assert!(error.is_err());
}

#[test]
fn reservations_reduce_both_admission_capacity_and_the_total_low_watermark()
-> Result<(), Box<dyn std::error::Error>> {
    let records = vec![
        record(RecordSpec::idle("large", 25, 1)),
        record(RecordSpec::idle("middle", 15, 2)),
        record(RecordSpec::idle("small", 10, 3)),
    ];
    let plan = plan_collection_with_reservation(
        &records,
        CollectionPolicy {
            budget: ByteSize::from_bytes(50),
            low_watermark_percent: 50,
        },
        ByteSize::from_bytes(10),
    )?;

    assert_eq!(plan.target, ByteSize::from_bytes(15));
    assert_eq!(plan.projected, ByteSize::from_bytes(10));
    assert!(plan.budget_met);
    Ok(())
}

#[test]
fn a_reservation_larger_than_the_budget_fails_with_an_empty_inventory()
-> Result<(), Box<dyn std::error::Error>> {
    let plan = plan_collection_with_reservation(
        &[],
        CollectionPolicy {
            budget: ByteSize::from_bytes(100),
            low_watermark_percent: 80,
        },
        ByteSize::from_bytes(120),
    )?;

    assert_eq!(plan.before, ByteSize::ZERO);
    assert_eq!(plan.projected, ByteSize::ZERO);
    assert!(!plan.budget_met);
    Ok(())
}

#[test]
fn deterministic_ties_use_size_then_identity() -> Result<(), Box<dyn std::error::Error>> {
    let large = record(RecordSpec::idle("large", 30, 10));
    let small = record(RecordSpec::idle("small", 10, 10));
    let size_plan = plan_collection(
        &[small, large.clone()],
        CollectionPolicy {
            budget: ByteSize::from_bytes(20),
            low_watermark_percent: 100,
        },
    )?;
    assert_eq!(size_plan.evictions[0].arena_id, large.id);

    let left = record(RecordSpec::idle("left", 30, 10));
    let right = record(RecordSpec::idle("right", 30, 10));
    let expected = std::cmp::min(left.id.clone(), right.id.clone());
    let policy = CollectionPolicy {
        budget: ByteSize::from_bytes(30),
        low_watermark_percent: 100,
    };
    let forward = plan_collection(&[left.clone(), right.clone()], policy)?;
    let reverse = plan_collection(&[right, left], policy)?;

    assert_eq!(forward.evictions[0].arena_id, expected);
    assert_eq!(reverse.evictions[0].arena_id, expected);
    Ok(())
}

fn record(spec: RecordSpec<'_>) -> ArenaRecord {
    let repository_id = RepositoryId::derive("repository");
    let worktree_id = WorktreeId::derive(spec.key);
    let workspace_id = WorkspaceId::derive("workspace");
    let toolchain_id = ToolchainId::derive("toolchain");

    ArenaRecord {
        id: ArenaId::derive(&repository_id, &worktree_id, &workspace_id, &toolchain_id),
        repository_id,
        worktree_id,
        workspace_id,
        toolchain_id,
        worktree_root: PathBuf::from(spec.key),
        build_dir: PathBuf::from(spec.key).join("build"),
        size: ByteSize::from_bytes(spec.bytes),
        size_quality: crate::SizeQuality::Fresh,
        created_at: 1,
        last_used_at: spec.last_used_at,
        liveness: spec.liveness,
        pinned: spec.pinned,
        worktree_exists: spec.worktree_exists,
        last_outcome: spec.last_outcome,
    }
}

#[test]
fn uncertain_sizes_are_protected_and_fail_admission_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let mut stale = record(RecordSpec::idle("stale", 30, 1));
    stale.size_quality = crate::SizeQuality::Stale;
    let plan = plan_collection(
        &[stale],
        CollectionPolicy {
            budget: ByteSize::from_bytes(100),
            low_watermark_percent: 80,
        },
    )?;

    assert!(plan.evictions.is_empty());
    assert_eq!(plan.protected, ByteSize::from_bytes(30));
    assert!(!plan.budget_met);
    Ok(())
}

#[test]
fn durable_cached_sizes_are_trusted_for_collection() -> Result<(), Box<dyn std::error::Error>> {
    let mut cached = record(RecordSpec::idle("cached", 30, 1));
    cached.size_quality = crate::SizeQuality::Cached;
    let plan = plan_collection(
        &[cached],
        CollectionPolicy {
            budget: ByteSize::from_bytes(20),
            low_watermark_percent: 100,
        },
    )?;

    assert_eq!(plan.evictions.len(), 1);
    assert_eq!(plan.protected, ByteSize::ZERO);
    assert!(plan.budget_met);
    Ok(())
}
