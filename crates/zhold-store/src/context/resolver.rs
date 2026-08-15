use std::path::Path;

use zhold_core::{ArenaId, RepositoryId, ToolchainId, WorkspaceId, WorktreeId, WorktreeKey};

use crate::{BuildContext, CargoInvocation, StoreError, WorktreeContext, context};

/// Resolves Cargo, Git, worktree, workspace, and toolchain identity.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContextResolver;

impl ContextResolver {
    /// Resolves and validates an existing Git worktree without invoking Cargo.
    pub fn resolve_worktree(path: &Path) -> Result<WorktreeContext, StoreError> {
        let git = context::git::resolve(path)?;
        let common = utf8(&git.common_dir, "Git common directory")?;
        let worktree = utf8(&git.worktree_root, "Git worktree root")?;
        let repository_id = RepositoryId::derive(common);
        let worktree_id = WorktreeId::derive(worktree);
        let key = WorktreeKey::derive(&repository_id, &worktree_id);
        Ok(WorktreeContext {
            repository_id,
            worktree_id,
            key,
            canonical_path: git.worktree_root,
            head: git.head,
        })
    }

    /// Resolves the complete managed build context for an invocation.
    pub fn resolve(invocation: &CargoInvocation) -> Result<BuildContext, StoreError> {
        let cargo = context::cargo::resolve(invocation)?;
        let git = context::git::resolve(&cargo.workspace_root)?;
        if !cargo.workspace_root.starts_with(&git.worktree_root) {
            return Err(StoreError::InvalidOwnership {
                path: cargo.workspace_root,
                reason: "Cargo workspace root is outside the Git worktree".to_owned(),
            });
        }

        let common = utf8(&git.common_dir, "Git common directory")?;
        let worktree = utf8(&git.worktree_root, "Git worktree root")?;
        let workspace = utf8(&cargo.workspace_root, "Cargo workspace root")?;
        let repository_id = RepositoryId::derive(common);
        let worktree_id = WorktreeId::derive(worktree);
        let workspace_id = WorkspaceId::derive(workspace);
        let toolchain_id = ToolchainId::derive(&cargo.toolchain_description);
        let arena_id = ArenaId::derive(&repository_id, &worktree_id, &workspace_id, &toolchain_id);

        Ok(BuildContext {
            arena_id,
            repository_id,
            worktree_id,
            workspace_id,
            toolchain_id,
            git_common_dir: git.common_dir,
            worktree_root: git.worktree_root,
            workspace_root: cargo.workspace_root,
            branch: git.branch,
            head: git.head,
            cargo_version: cargo.cargo_version,
            toolchain_description: cargo.toolchain_description,
        })
    }
}

fn utf8<'a>(path: &'a std::path::Path, kind: &'static str) -> Result<&'a str, StoreError> {
    path.to_str().ok_or_else(|| StoreError::NonUnicode {
        kind,
        path: path.to_path_buf(),
    })
}
