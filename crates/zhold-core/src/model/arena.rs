use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{ArenaId, ByteSize, RepositoryId, ToolchainId, WorkspaceId, WorktreeId};

/// Result of the most recent managed command.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "code")]
pub enum BuildOutcome {
    /// The command exited successfully.
    Succeeded,
    /// The command exited with a non-zero status code.
    Failed(i32),
    /// The command terminated without a portable exit code.
    Terminated,
}

/// Current retention state observed for an arena.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArenaState {
    /// An exclusive build lease is currently held.
    Active,
    /// A build was started, but neither completion nor a live lease can be proven.
    Suspect,
    /// The user explicitly protected the arena.
    Pinned,
    /// The owning worktree no longer exists.
    Orphaned,
    /// The arena is inactive and eligible under normal retention policy.
    Idle,
}

/// Proven liveness of the most recent managed command.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArenaLiveness {
    /// No managed command is currently running or left unfinished.
    #[default]
    Inactive,
    /// A live operating-system lease proves that the command is protected.
    Active,
    /// The command is unfinished, but its lease is no longer live.
    Suspect,
}

/// Confidence in the byte count attached to an arena observation.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeQuality {
    /// The arena tree was measured successfully in this snapshot.
    #[default]
    Fresh,
    /// Durable size from the most recent completed managed transition.
    Cached,
    /// Current measurement failed, so the last durable successful size is used.
    Stale,
    /// No trustworthy byte count is available.
    Unknown,
}

/// Complete policy input for one managed arena.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArenaRecord {
    /// Stable arena identity.
    pub id: ArenaId,
    /// Stable repository identity.
    pub repository_id: RepositoryId,
    /// Stable worktree identity.
    pub worktree_id: WorktreeId,
    /// Stable workspace identity.
    pub workspace_id: WorkspaceId,
    /// Stable toolchain identity.
    pub toolchain_id: ToolchainId,
    /// Canonical worktree root recorded when the arena was created.
    pub worktree_root: PathBuf,
    /// Managed intermediate build directory.
    pub build_dir: PathBuf,
    /// Measured bytes beneath the arena.
    pub size: ByteSize,
    /// Confidence in `size` for admission and collection decisions.
    pub size_quality: SizeQuality,
    /// Creation time as Unix seconds.
    pub created_at: u64,
    /// Most recent managed use as Unix seconds.
    pub last_used_at: u64,
    /// Proven liveness of the most recent managed command.
    pub liveness: ArenaLiveness,
    /// Whether the user explicitly protected the arena.
    pub pinned: bool,
    /// Whether the recorded worktree root still exists.
    pub worktree_exists: bool,
    /// Result of the most recent managed command, when known.
    pub last_outcome: Option<BuildOutcome>,
}

impl ArenaRecord {
    /// Returns the state used by collection policy.
    pub const fn state(&self) -> ArenaState {
        if matches!(self.liveness, ArenaLiveness::Active) {
            ArenaState::Active
        } else if matches!(self.liveness, ArenaLiveness::Suspect) {
            ArenaState::Suspect
        } else if self.pinned {
            ArenaState::Pinned
        } else if !self.worktree_exists {
            ArenaState::Orphaned
        } else {
            ArenaState::Idle
        }
    }
}
