use serde::{Deserialize, Serialize};

/// Durable lifecycle state for one registered worktree.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeIntegrationState {
    /// Builds may be admitted.
    Ready,
    /// Removal is prepared and new builds are denied.
    Draining,
    /// The registered path was verified absent.
    Removed,
}

/// Worktree-manager lifecycle operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// Register or reactivate a validated worktree.
    Ready,
    /// Establish the fail-closed removal guard.
    PrepareRemove,
    /// Confirm that the registered path is absent.
    Removed,
    /// Recover a still-valid worktree after manager failure.
    CancelRemove,
}

/// Stable result category for a lifecycle operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookResult {
    /// The requested state is now committed.
    Changed,
    /// The requested state was already committed.
    Unchanged,
    /// A live build currently holds the worktree gate.
    ActiveBuild,
    /// The requested transition requires user attention.
    Attention,
}
