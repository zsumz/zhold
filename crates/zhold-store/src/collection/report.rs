use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zhold_core::{ArenaId, ByteSize, CollectionPlan, EvictionReason};

/// Result of one deterministic collection attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CollectionReport {
    /// Whether mutation was disabled.
    pub dry_run: bool,
    /// User-requested active storage budget.
    pub budget: ByteSize,
    /// Additional growth headroom held by live build leases.
    pub reserved: ByteSize,
    /// Immutable policy plan computed before mutation.
    pub plan: CollectionPlan,
    /// Bytes confirmed deleted during this attempt.
    pub reclaimed: ByteSize,
    /// Projected size for dry-run, or confirmed active-arena size after retirement.
    pub after: ByteSize,
    /// Whether active bytes plus reservations fit the requested budget.
    pub budget_met: bool,
    /// Arenas moved out of the active index.
    pub retirements: Vec<Retirement>,
    /// Planned candidates skipped after race-safe revalidation.
    pub skipped: Vec<CollectionSkip>,
    /// Nonfatal receipt-publication or retention result.
    #[serde(default)]
    pub history: crate::HistoryWrite,
}

/// One arena retired from the active index.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Retirement {
    /// Retired arena identity.
    pub arena_id: ArenaId,
    /// Bytes measured immediately before retirement.
    pub size: ByteSize,
    /// Policy reason for retirement.
    pub reason: EvictionReason,
    /// Whether bytes were deleted or remain in owned trash for diagnosis.
    pub disposition: RetirementDisposition,
}

/// Final state of a retired arena.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RetirementDisposition {
    /// The no-follow recursive deletion completed.
    Deleted,
    /// The arena was atomically retired but deletion failed closed.
    PendingDeletion {
        /// Owned trash path retaining the arena.
        path: PathBuf,
        /// Failure explanation.
        error: String,
    },
}

/// Candidate skipped because state changed or validation failed after planning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CollectionSkip {
    /// Candidate arena identity.
    pub arena_id: ArenaId,
    /// Explainable skip reason.
    pub reason: String,
}

/// Result of retrying deletion beneath the owned retirement directory.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrashReport {
    /// Whether deletion was disabled.
    pub dry_run: bool,
    /// Bytes present in owned trash before the attempt.
    pub before: ByteSize,
    /// Bytes confirmed deleted during this attempt.
    pub reclaimed: ByteSize,
    /// Bytes still present after the attempt, or projected for dry-run.
    pub remaining: ByteSize,
    /// Per-entry outcomes in stable path order.
    pub entries: Vec<TrashEntry>,
    /// Nonfatal receipt-publication or retention result.
    #[serde(default)]
    pub history: crate::HistoryWrite,
}

/// One owned-trash deletion attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrashEntry {
    /// Retirement directory considered.
    pub path: PathBuf,
    /// Bytes measured before deletion.
    pub size: ByteSize,
    /// Planned, completed, or failed outcome.
    pub outcome: TrashOutcome,
}

/// Outcome of one owned-trash entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TrashOutcome {
    /// Dry-run proved the entry eligible without deleting it.
    WouldDelete,
    /// Recursive no-follow deletion completed.
    Deleted,
    /// Validation or deletion failed closed.
    Skipped {
        /// Failure explanation.
        error: String,
    },
}
