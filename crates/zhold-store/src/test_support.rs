use std::{fs, io, path::Path};

use zhold_core::{ArenaId, BuildOutcome, RepositoryId, ToolchainId, WorkspaceId, WorktreeId};

use crate::{BuildContext, CargoInvocation, Store, StoreError};

pub(crate) fn invocation(root: &Path) -> Result<CargoInvocation, StoreError> {
    CargoInvocation::new(
        "cargo".to_owned(),
        vec!["test".to_owned()],
        root.to_path_buf(),
    )
}

pub(crate) fn context(root: &Path) -> Result<BuildContext, Box<dyn std::error::Error>> {
    let worktree_root = root.canonicalize()?;
    let git_common_dir = worktree_root.join(".git");
    fs::create_dir_all(&git_common_dir)?;
    let description = worktree_root.to_str().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "temporary path is not Unicode")
    })?;
    let repository_id = RepositoryId::derive(&format!("{description}/.git"));
    let worktree_id = WorktreeId::derive(description);
    let workspace_id = WorkspaceId::derive(description);
    let toolchain_description = "cargo 1.91.0\nrustc 1.91.0".to_owned();
    let toolchain_id = ToolchainId::derive(&toolchain_description);
    let arena_id = ArenaId::derive(&repository_id, &worktree_id, &workspace_id, &toolchain_id);

    Ok(BuildContext {
        arena_id,
        repository_id,
        worktree_id,
        workspace_id,
        toolchain_id,
        git_common_dir,
        worktree_root: worktree_root.clone(),
        workspace_root: worktree_root,
        branch: Some("main".to_owned()),
        head: Some("0123456789abcdef".to_owned()),
        cargo_version: "1.91.0".to_owned(),
        toolchain_description,
    })
}

pub(crate) fn create_idle_arena(
    store: &Store,
    root: &Path,
    bytes: usize,
) -> Result<(BuildContext, CargoInvocation), Box<dyn std::error::Error>> {
    let context = context(root)?;
    let invocation = invocation(root)?;
    let lease = store.lease(&context, &invocation)?;
    fs::write(lease.build_dir().join("artifact.rlib"), vec![0_u8; bytes])?;
    lease.finish(BuildOutcome::Succeeded)?;
    Ok((context, invocation))
}
