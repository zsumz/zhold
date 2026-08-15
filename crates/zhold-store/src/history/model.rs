use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::{ByteSize, HistoryKind, HistoryPolicy, WorktreeId};

use super::HistoryPayload;

#[derive(Clone, Debug)]
pub(crate) struct HistoryDraft {
    pub(crate) kind: HistoryKind,
    pub(crate) payload: HistoryPayload,
}

/// Persisted receipt-retention policy envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryPolicyDocument {
    /// Persisted schema version.
    pub schema_version: u32,
    /// Marked store identity.
    pub store_id: Uuid,
    /// Validated effective policy.
    pub policy: HistoryPolicy,
}

/// Newest-first receipt filters.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryQuery {
    /// Optional receipt category.
    pub kind: Option<HistoryKind>,
    /// Optional build arena ID prefix.
    pub arena_prefix: Option<String>,
    /// Optional build or hook worktree identity.
    pub worktree_id: Option<WorktreeId>,
    /// Optional inclusive Unix-millisecond lower bound.
    pub since: Option<u64>,
    /// Maximum number of matching receipts returned.
    pub limit: usize,
}

impl Default for HistoryQuery {
    fn default() -> Self {
        Self {
            kind: None,
            arena_prefix: None,
            worktree_id: None,
            since: None,
            limit: 50,
        }
    }
}

/// Untrusted history entry excluded from query and retention.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryFinding {
    /// Rejected path.
    pub path: PathBuf,
    /// Validation failure.
    pub reason: String,
}

/// Complete filtered persistent-history query result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryReport {
    /// Filters applied before the result limit.
    pub filters: HistoryQuery,
    /// Effective receipt policy.
    pub policy: HistoryPolicy,
    /// Matching validated receipts, newest-first.
    pub receipts: Vec<super::HistoryReceipt>,
    /// Invalid or foreign entries excluded from results.
    pub findings: Vec<HistoryFinding>,
    /// Whether additional validated matches remain after the limit.
    pub more: bool,
}

/// Compact persistent-history health for status and doctor.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistorySummary {
    /// Effective receipt policy.
    pub policy: HistoryPolicy,
    /// Number of validated receipt files.
    pub receipt_count: u64,
    /// Total encoded bytes across validated receipt files.
    pub receipt_bytes: ByteSize,
    /// Number of invalid entries or policy documents.
    pub finding_count: u64,
    /// Whether the newest receipt alone exceeds the byte bound.
    pub oversized_newest: bool,
}

/// Deterministic manual receipt-pruning constraints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryPruneRequest {
    /// Maximum newest validated receipts retained.
    pub keep: Option<u64>,
    /// Maximum validated receipt bytes retained.
    pub max_bytes: Option<ByteSize>,
    /// Remove receipts older than this Unix-millisecond cutoff.
    pub older_than: Option<u64>,
    /// Plan without removing files.
    pub dry_run: bool,
}

/// Result of deterministic validated receipt pruning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryPruneReport {
    /// Whether filesystem mutation was disabled.
    pub dry_run: bool,
    /// Validated receipt count before pruning.
    pub before_count: u64,
    /// Validated receipt bytes before pruning.
    pub before_bytes: ByteSize,
    /// Selected or removed receipt count.
    pub removed_count: u64,
    /// Selected or removed receipt bytes.
    pub removed_bytes: ByteSize,
    /// Projected or confirmed receipt count after pruning.
    pub after_count: u64,
    /// Projected or confirmed receipt bytes after pruning.
    pub after_bytes: ByteSize,
    /// Invalid entries deliberately left untouched.
    pub findings: Vec<HistoryFinding>,
}

/// Nonfatal receipt subsystem warning category.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryWarningEvent {
    /// Atomic receipt publication failed after the primary operation committed.
    PersistFailed,
    /// Publication succeeded but automatic retention failed.
    RetentionFailed,
}

/// Nonfatal receipt subsystem warning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryWarning {
    /// Stable lifecycle event category.
    pub event: HistoryWarningEvent,
    /// Bounded diagnostic explanation.
    pub message: String,
}

/// Result of attempting one non-authoritative receipt publication.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryWrite {
    /// Published receipt identity, when publication succeeded and retention retained it.
    pub receipt_id: Option<Uuid>,
    /// Nonfatal publication or retention warnings.
    pub warnings: Vec<HistoryWarning>,
}

/// Nonfatal history results produced while finalizing one managed build.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildFinalization {
    /// Preflight collection and build receipt publication results, in commit order.
    pub history: Vec<HistoryWrite>,
}
