use std::path::{Path, PathBuf};

use zhold_core::{RepositoryId, WorktreeId, WorktreeKey};

/// Validated Git identity for one existing worktree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeContext {
    pub(crate) repository_id: RepositoryId,
    pub(crate) worktree_id: WorktreeId,
    pub(crate) key: WorktreeKey,
    pub(crate) canonical_path: PathBuf,
    pub(crate) head: Option<String>,
}

impl WorktreeContext {
    /// Stable repository identity.
    pub fn repository_id(&self) -> &RepositoryId {
        &self.repository_id
    }

    /// Stable worktree identity.
    pub fn worktree_id(&self) -> &WorktreeId {
        &self.worktree_id
    }

    /// Stable repository-qualified worktree coordination key.
    pub fn key(&self) -> &WorktreeKey {
        &self.key
    }

    /// Canonical worktree root.
    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    /// Current Git commit when available.
    pub fn head(&self) -> Option<&str> {
        self.head.as_deref()
    }
}
