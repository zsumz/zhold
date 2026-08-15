use std::path::{Path, PathBuf};

use zhold_core::{ArenaId, RepositoryId, ToolchainId, WorkspaceId, WorktreeId};

/// Resolved compatibility and lifecycle identity for one managed Cargo build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildContext {
    pub(crate) arena_id: ArenaId,
    pub(crate) repository_id: RepositoryId,
    pub(crate) worktree_id: WorktreeId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) toolchain_id: ToolchainId,
    pub(crate) git_common_dir: PathBuf,
    pub(crate) worktree_root: PathBuf,
    pub(crate) workspace_root: PathBuf,
    pub(crate) branch: Option<String>,
    pub(crate) head: Option<String>,
    pub(crate) cargo_version: String,
    pub(crate) toolchain_description: String,
}

impl BuildContext {
    /// Stable arena identity.
    pub fn arena_id(&self) -> &ArenaId {
        &self.arena_id
    }

    /// Canonical Git worktree root.
    pub fn worktree_root(&self) -> &Path {
        &self.worktree_root
    }

    /// Canonical Cargo workspace root.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Current branch when the worktree is attached to one.
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    /// Current Git commit when available.
    pub fn head(&self) -> Option<&str> {
        self.head.as_deref()
    }

    /// Cargo release selected for this invocation.
    pub fn cargo_version(&self) -> &str {
        &self.cargo_version
    }

    /// Complete Cargo and rustc description used for toolchain identity.
    pub fn toolchain_description(&self) -> &str {
        &self.toolchain_description
    }
}
