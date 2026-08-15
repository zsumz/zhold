//! Serializable inventory data.

use std::path::PathBuf;

use crate::{HistorySummary, QuotaStatus, WorktreeSummary};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::{ArenaRecord, ByteSize, CommandDescriptor, WorktreeIntegrationState};

/// Snapshot of all valid managed arenas in one store.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Inventory {
    /// Stable store identity.
    pub store_id: Uuid,
    /// Canonical store root.
    pub store_root: PathBuf,
    /// Time of observation as Unix seconds.
    pub observed_at: u64,
    /// Measured bytes in valid arenas.
    pub total: ByteSize,
    /// Measured bytes currently protected by leases or pins.
    pub protected: ByteSize,
    /// Additional growth headroom declared by live build leases.
    pub reserved: ByteSize,
    /// Plausibly owned arenas whose current accounting could not be proven.
    pub uncertain_owned: u64,
    /// Measured bytes in retired arenas awaiting deletion.
    pub trash: ByteSize,
    /// Measured bytes beneath the complete marked store root.
    pub physical: ByteSize,
    /// Bytes currently available to the store filesystem user.
    pub available: ByteSize,
    /// Bounded persistent operation-history health.
    pub history: HistorySummary,
    /// Validated manager-integration summary.
    pub worktrees: WorktreeSummary,
    /// Fresh optional quota capability and adoption health.
    pub quota: Option<QuotaStatus>,
    /// Invalid optional quota metadata excluded from admission decisions.
    pub quota_finding: Option<String>,
    /// Valid managed arenas ordered by stable identity.
    pub arenas: Vec<InventoryEntry>,
    /// Entries that were visible but failed closed validation.
    pub findings: Vec<InventoryFinding>,
}

/// One valid managed arena with policy and presentation metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InventoryEntry {
    pub(crate) revision: u64,
    /// Deterministic policy input.
    pub record: ArenaRecord,
    /// Cargo workspace root.
    pub workspace_root: PathBuf,
    /// Git branch recorded during the latest managed run.
    pub branch: Option<String>,
    /// Git commit recorded during the latest managed run.
    pub head: Option<String>,
    /// Selected Cargo release.
    pub cargo_version: String,
    /// Sanitized descriptor of the most recently wrapped command.
    pub command: CommandDescriptor,
    /// Additional growth headroom declared by this lease when active.
    pub reservation: ByteSize,
    /// Largest observed arena size during the most recently completed run.
    pub last_peak: ByteSize,
    /// Unix timestamp at which an explicit pin expires, when finite.
    pub pin_expires_at: Option<u64>,
    /// Validated worktree lifecycle state when registered.
    pub worktree_state: Option<WorktreeIntegrationState>,
    /// Registered manager name when available.
    pub manager: Option<String>,
    /// Registered user-facing label when available.
    pub label: Option<String>,
    /// Registered manager session when available.
    pub session: Option<String>,
}

/// An untrusted or unreadable entry excluded from mutation planning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InventoryFinding {
    /// Path that could not be trusted.
    pub path: PathBuf,
    /// Human-readable failure reason.
    pub reason: String,
}
