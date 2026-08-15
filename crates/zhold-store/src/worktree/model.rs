use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::{
    HookEvent, HookResult, RepositoryId, WorktreeId, WorktreeIntegrationState, WorktreeKey,
};

/// Optional bounded metadata supplied by a worktree manager.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HookMetadata {
    /// Worktree-manager name.
    pub manager: Option<String>,
    /// Human-readable worktree label.
    pub label: Option<String>,
    /// Manager session identity.
    pub session: Option<String>,
}

/// Validated persisted registration for one worktree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorktreeIntegration {
    /// Persisted schema version.
    pub schema_version: u32,
    /// Marked store identity.
    pub store_id: Uuid,
    /// Repository-qualified worktree key.
    pub worktree_key: WorktreeKey,
    /// Stable repository identity.
    pub repository_id: RepositoryId,
    /// Stable canonical worktree identity.
    pub worktree_id: WorktreeId,
    /// Canonical absolute worktree path.
    pub canonical_path: PathBuf,
    /// Monotonic metadata revision.
    pub revision: u64,
    /// Durable integration lifecycle state.
    pub state: WorktreeIntegrationState,
    /// Optional manager name.
    pub manager: Option<String>,
    /// Optional display label.
    pub label: Option<String>,
    /// Optional manager session identity.
    pub session: Option<String>,
    /// Current Git commit observed by the latest validating transition.
    pub head: Option<String>,
    /// Creation time as Unix milliseconds.
    pub created_at: u64,
    /// Most recent mutation time as Unix milliseconds.
    pub updated_at: u64,
}

/// Result of one manager lifecycle operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HookReport {
    /// Requested operation.
    pub event: HookEvent,
    /// Stable result category.
    pub result: HookResult,
    /// State before the request, when a record matched.
    pub previous: Option<WorktreeIntegrationState>,
    /// State after the request, when a record matched.
    pub resulting: Option<WorktreeIntegrationState>,
    /// Validated record after the request, when available.
    pub integration: Option<WorktreeIntegration>,
    /// Human-readable attention or idempotency explanation.
    pub message: String,
    /// Nonfatal receipt-publication or retention result.
    pub history: crate::HistoryWrite,
}

impl HookReport {
    /// Whether the manager may continue its requested removal workflow.
    pub const fn attention_required(&self) -> bool {
        matches!(self.result, HookResult::ActiveBuild | HookResult::Attention)
    }
}

/// Untrusted integration entry excluded from coordination joins.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorktreeFinding {
    /// Rejected metadata path.
    pub path: PathBuf,
    /// Validation failure.
    pub reason: String,
}

/// Compact worktree-integration health summary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorktreeSummary {
    /// Number of validated registrations.
    pub registration_count: u64,
    /// Number of fail-closed draining registrations.
    pub draining_count: u64,
    /// Number of invalid or foreign entries.
    pub finding_count: u64,
    /// Recovery guidance for draining registrations.
    pub recovery: Vec<String>,
}
