use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::{
    ArenaId, BuildOutcome, ByteSize, HistoryKind, HookEvent, HookResult, QuotaHealth,
    QuotaProvider, RepositoryId, ToolchainId, WorkspaceId, WorktreeId, WorktreeIntegrationState,
};

/// Immutable versioned evidence for one committed operation outcome.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryReceipt {
    /// Persisted schema version.
    pub schema_version: u32,
    /// Unique receipt identity.
    pub receipt_id: Uuid,
    /// Marked store identity.
    pub store_id: Uuid,
    /// Publication time as Unix milliseconds.
    pub recorded_at: u64,
    /// Stable operation category.
    pub kind: HistoryKind,
    /// Category-specific committed summary.
    pub payload: HistoryPayload,
}

/// Typed committed operation summary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "details")]
pub enum HistoryPayload {
    /// Completed managed build.
    Build(BuildReceipt),
    /// Confirmed collection attempt.
    Collection(CollectionReceipt),
    /// Worktree lifecycle event.
    Hook(HookReceipt),
    /// Quota expectation event.
    Quota(QuotaReceipt),
}

/// Source of one committed collection receipt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionReceiptSource {
    /// Explicit `zhold gc` collection.
    Manual,
    /// Budget preflight collection before a managed build.
    Preflight,
    /// Explicit retry of already-retired owned trash.
    TrashRetry,
}

/// Privacy-bounded completed-build evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildReceipt {
    /// Stable arena identity.
    pub arena_id: ArenaId,
    /// Stable repository identity.
    pub repository_id: RepositoryId,
    /// Stable worktree identity.
    pub worktree_id: WorktreeId,
    /// Stable workspace identity.
    pub workspace_id: WorkspaceId,
    /// Stable toolchain identity.
    pub toolchain_id: ToolchainId,
    /// Sentinel start time as Unix milliseconds.
    pub started_at: u64,
    /// Finalization time as Unix milliseconds.
    pub finished_at: u64,
    /// Saturating elapsed wall time.
    pub elapsed_milliseconds: u64,
    /// Authoritative child outcome.
    pub outcome: BuildOutcome,
    /// Portable child exit code, when available.
    pub exit_code: Option<i32>,
    /// Arena bytes observed before child spawn.
    pub initial_bytes: ByteSize,
    /// Arena bytes observed after child exit.
    pub final_bytes: ByteSize,
    /// Best sampled arena-size lower bound.
    pub observed_peak: ByteSize,
    /// Declared additional growth reservation.
    pub reservation: ByteSize,
    /// Optional warning-only arena threshold.
    pub warning_threshold: Option<ByteSize>,
    /// Whether the warning threshold was sampled as exceeded.
    pub warning_threshold_exceeded: bool,
    /// Registered worktree manager at admission.
    pub manager: Option<String>,
    /// Registered worktree label at admission.
    pub label: Option<String>,
    /// Registered manager session at admission.
    pub session: Option<String>,
}

/// Confirmed collection evidence without raw paths or commands.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionReceipt {
    /// Collection source such as manual or preflight.
    pub source: CollectionReceiptSource,
    /// Requested active-arena budget.
    pub budget: ByteSize,
    /// Live build reservation total after collection.
    pub reserved: ByteSize,
    /// Active bytes before collection.
    pub before: ByteSize,
    /// Policy low-watermark target.
    pub target: ByteSize,
    /// Immutable plan projection.
    pub projected: ByteSize,
    /// Confirmed active bytes after mutation.
    pub after: ByteSize,
    /// Bytes protected in the immutable plan.
    pub protected: ByteSize,
    /// Bytes confirmed deleted.
    pub reclaimed: ByteSize,
    /// Whether confirmed active bytes plus reservations fit the budget.
    pub budget_met: bool,
    /// Arena identities retired from the active index.
    pub retirements: Vec<ArenaId>,
    /// Arena identities skipped after revalidation.
    pub skipped: Vec<ArenaId>,
}

/// Worktree-manager lifecycle evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HookReceipt {
    /// Requested lifecycle event.
    pub event: HookEvent,
    /// Stable repository identity.
    pub repository_id: RepositoryId,
    /// Stable worktree identity.
    pub worktree_id: WorktreeId,
    /// Bounded manager name.
    pub manager: Option<String>,
    /// Bounded user-facing label.
    pub label: Option<String>,
    /// Bounded manager session.
    pub session: Option<String>,
    /// State before the request.
    pub previous: Option<WorktreeIntegrationState>,
    /// State after the request.
    pub resulting: WorktreeIntegrationState,
    /// Stable result category.
    pub result: HookResult,
}

/// Quota expectation evidence without raw provider output.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuotaReceipt {
    /// Observed store-scoped provider.
    pub provider: QuotaProvider,
    /// Canonical provider scope.
    pub scope: PathBuf,
    /// Stable containing filesystem identity.
    pub filesystem_id: String,
    /// Adopted expected limit, when any.
    pub expected_limit: Option<ByteSize>,
    /// Fresh provider-reported hard limit.
    pub observed_limit: Option<ByteSize>,
    /// Fresh provider-reported usage.
    pub observed_usage: Option<ByteSize>,
    /// Expectation action such as adopted or unadopted.
    pub action: QuotaReceiptAction,
    /// Fresh provider result category.
    pub result: QuotaHealth,
}

/// Quota expectation transition recorded in history.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaReceiptAction {
    /// An externally provisioned quota expectation was adopted.
    Adopted,
    /// Only zhold's expectation metadata was removed.
    Unadopted,
}
