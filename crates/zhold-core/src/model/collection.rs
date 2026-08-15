use serde::{Deserialize, Serialize};

use crate::{ArenaId, ByteSize};

/// Collection policy for a bounded managed store.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CollectionPolicy {
    /// Maximum desired store size.
    pub budget: ByteSize,
    /// Percentage of the budget collection should target once triggered.
    pub low_watermark_percent: u8,
}

impl CollectionPolicy {
    /// Creates a policy with the default 80 percent low watermark.
    pub const fn new(budget: ByteSize) -> Self {
        Self {
            budget,
            low_watermark_percent: 80,
        }
    }
}

/// Explainable reason an arena was selected for eviction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvictionReason {
    /// Its owning worktree no longer exists.
    OrphanedWorktree,
    /// Its last managed command failed.
    FailedBuild,
    /// It was the least recently used eligible arena.
    LeastRecentlyUsed,
}

/// One planned whole-arena eviction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Eviction {
    /// Arena selected for eviction.
    pub arena_id: ArenaId,
    /// Measured bytes expected to be reclaimed.
    pub size: ByteSize,
    /// Deterministic policy reason.
    pub reason: EvictionReason,
}

/// Deterministic collection plan before filesystem mutation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CollectionPlan {
    /// Measured bytes before collection.
    pub before: ByteSize,
    /// Low-watermark target when collection was triggered.
    pub target: ByteSize,
    /// Projected bytes after planned evictions.
    pub projected: ByteSize,
    /// Bytes held by active or pinned arenas.
    pub protected: ByteSize,
    /// Bytes expected to be reclaimed.
    pub reclaimable: ByteSize,
    /// Ordered whole-arena evictions.
    pub evictions: Vec<Eviction>,
    /// Whether the projected result is within the requested hard budget.
    pub budget_met: bool,
}
