use super::{ArenaId, RepositoryId, ToolchainId, WorkspaceId, WorktreeId, WorktreeKey};

#[test]
fn arena_identity_changes_at_each_compatibility_boundary() {
    let repository = RepositoryId::derive("/repo/.git");
    let worktree = WorktreeId::derive("/repo");
    let workspace = WorkspaceId::derive("/repo");
    let toolchain = ToolchainId::derive("rustc 1.91.0");
    let baseline = ArenaId::derive(&repository, &worktree, &workspace, &toolchain);

    let alternate_repository = ArenaId::derive(
        &RepositoryId::derive("/other/.git"),
        &worktree,
        &workspace,
        &toolchain,
    );
    let alternate_worktree = ArenaId::derive(
        &repository,
        &WorktreeId::derive("/repo-worktree"),
        &workspace,
        &toolchain,
    );
    let alternate_workspace = ArenaId::derive(
        &repository,
        &worktree,
        &WorkspaceId::derive("/repo/member"),
        &toolchain,
    );
    let alternate_toolchain = ArenaId::derive(
        &repository,
        &worktree,
        &workspace,
        &ToolchainId::derive("rustc 1.92.0"),
    );

    assert_ne!(baseline, alternate_repository);
    assert_ne!(baseline, alternate_worktree);
    assert_ne!(baseline, alternate_workspace);
    assert_ne!(baseline, alternate_toolchain);
    assert_eq!(baseline.as_str().len(), 32);
}

#[test]
fn worktree_keys_bind_repository_and_physical_worktree() {
    let repository = RepositoryId::derive("/repo/.git");
    let worktree = WorktreeId::derive("/repo/feature");
    let baseline = WorktreeKey::derive(&repository, &worktree);

    assert_ne!(
        baseline,
        WorktreeKey::derive(&RepositoryId::derive("/other/.git"), &worktree)
    );
    assert_eq!(baseline.to_string().parse::<WorktreeKey>(), Ok(baseline));
}
