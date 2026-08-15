use std::cmp::Reverse;

use thiserror::Error;

use crate::{
    ArenaRecord, ArenaState, BuildOutcome, ByteSize, CollectionPlan, CollectionPolicy, Eviction,
    EvictionReason,
};

/// Invalid collection policy.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PolicyError {
    /// The low watermark must be between 1 and 100 percent.
    #[error("low watermark must be between 1 and 100 percent")]
    InvalidLowWatermark,
}

/// Produces an explainable whole-arena collection plan.
pub fn plan_collection(
    records: &[ArenaRecord],
    policy: CollectionPolicy,
) -> Result<CollectionPlan, PolicyError> {
    plan_collection_with_reservation(records, policy, ByteSize::ZERO)
}

/// Plans collection while preserving capacity reserved by live builds.
pub fn plan_collection_with_reservation(
    records: &[ArenaRecord],
    policy: CollectionPolicy,
    reservation: ByteSize,
) -> Result<CollectionPlan, PolicyError> {
    if !(1..=100).contains(&policy.low_watermark_percent) {
        return Err(PolicyError::InvalidLowWatermark);
    }

    let before = records.iter().fold(ByteSize::ZERO, |total, record| {
        total.saturating_add(record.size)
    });
    let protected = records
        .iter()
        .filter(|record| matches!(record.state(), ArenaState::Active | ArenaState::Pinned))
        .fold(ByteSize::ZERO, |total, record| {
            total.saturating_add(record.size)
        });
    let admission_budget = policy.budget.saturating_sub(reservation);
    let target = policy
        .budget
        .percent(policy.low_watermark_percent)
        .saturating_sub(reservation);

    if before <= admission_budget {
        return Ok(CollectionPlan {
            before,
            target,
            projected: before,
            protected,
            reclaimable: ByteSize::ZERO,
            evictions: Vec::new(),
            budget_met: true,
        });
    }

    let mut candidates = records
        .iter()
        .filter(|record| matches!(record.state(), ArenaState::Orphaned | ArenaState::Idle))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|record| {
        (
            priority(record),
            record.last_used_at,
            Reverse(record.size),
            record.id.clone(),
        )
    });

    let mut projected = before;
    let mut evictions = Vec::new();
    for record in candidates {
        if projected <= target {
            break;
        }

        projected = projected.saturating_sub(record.size);
        evictions.push(Eviction {
            arena_id: record.id.clone(),
            size: record.size,
            reason: reason(record),
        });
    }

    Ok(CollectionPlan {
        before,
        target,
        projected,
        protected,
        reclaimable: before.saturating_sub(projected),
        evictions,
        budget_met: projected <= admission_budget,
    })
}

const fn priority(record: &ArenaRecord) -> u8 {
    if matches!(record.state(), ArenaState::Orphaned) {
        0
    } else if matches!(record.last_outcome, Some(BuildOutcome::Failed(_))) {
        1
    } else {
        2
    }
}

const fn reason(record: &ArenaRecord) -> EvictionReason {
    if matches!(record.state(), ArenaState::Orphaned) {
        EvictionReason::OrphanedWorktree
    } else if matches!(record.last_outcome, Some(BuildOutcome::Failed(_))) {
        EvictionReason::FailedBuild
    } else {
        EvictionReason::LeastRecentlyUsed
    }
}
