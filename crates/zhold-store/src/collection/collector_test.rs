#[cfg(unix)]
use std::fs;

use tempfile::tempdir;
use uuid::Uuid;
use zhold_core::{
    ArenaId, ByteSize, CollectionPolicy, Eviction, EvictionReason, RepositoryId, ToolchainId,
    WorkspaceId, WorktreeId,
};

use crate::{
    Store, StoreError,
    io::{read_json, write_json},
    manifest::ArenaManifest,
    test_support::{create_idle_arena, finish_succeeded},
};

use super::collector::{RetirementAttempt, accounted_bytes, budget_is_confirmed, retire};
use super::{Retirement, RetirementDisposition};

#[test]
fn final_uncertainty_prevents_a_confirmed_budget_result() {
    assert!(!budget_is_confirmed(
        ByteSize::ZERO,
        ByteSize::ZERO,
        1,
        ByteSize::from_bytes(1),
    ));
    assert!(budget_is_confirmed(
        ByteSize::from_bytes(1),
        ByteSize::ZERO,
        0,
        ByteSize::from_bytes(1),
    ));
}

#[test]
fn pending_trash_leaves_active_storage_but_not_physical_reclamation() {
    let retirements = vec![
        Retirement {
            arena_id: arena_id("deleted"),
            size: ByteSize::from_bytes(10),
            reason: EvictionReason::LeastRecentlyUsed,
            disposition: RetirementDisposition::Deleted,
        },
        Retirement {
            arena_id: arena_id("pending"),
            size: ByteSize::from_bytes(20),
            reason: EvictionReason::LeastRecentlyUsed,
            disposition: RetirementDisposition::PendingDeletion {
                path: std::path::PathBuf::from("trash/pending"),
                error: "busy".to_owned(),
            },
        },
    ];

    let (retired, reclaimed) = accounted_bytes(&retirements);

    assert_eq!(retired, ByteSize::from_bytes(30));
    assert_eq!(reclaimed, ByteSize::from_bytes(10));
}

#[test]
fn collection_policy_uses_an_eighty_percent_low_watermark() {
    let policy = CollectionPolicy::new(ByteSize::from_bytes(1_000));

    assert_eq!(policy.low_watermark_percent, 80);
}

#[test]
fn a_lease_acquired_after_planning_blocks_retirement() -> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, invocation) = create_idle_arena(&store, project.path(), 4_096)?;
    let (eviction, revision, canonical_root) = snapshot(&store)?;
    let lease = store.lease(&context, &invocation)?;

    let result = retire(&store, &canonical_root, &eviction, revision)?;

    assert!(matches!(result, RetirementAttempt::Skipped(reason) if reason.contains("live lease")));
    assert!(store.layout.arena(context.arena_id()).is_dir());
    finish_succeeded(lease)?;
    Ok(())
}

#[test]
fn metadata_changed_after_planning_blocks_retirement() -> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let (eviction, revision, canonical_root) = snapshot(&store)?;
    store.set_pinned(context.arena_id(), true)?;

    let result = retire(&store, &canonical_root, &eviction, revision)?;

    assert!(matches!(
        result,
        RetirementAttempt::Skipped(reason) if reason.contains("metadata changed")
    ));
    assert!(store.layout.arena(context.arena_id()).is_dir());
    Ok(())
}

#[test]
fn a_pin_is_rechecked_even_when_the_revision_is_unchanged() -> Result<(), Box<dyn std::error::Error>>
{
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let (eviction, revision, canonical_root) = snapshot(&store)?;
    let manifest_path = store.layout.manifest(context.arena_id());
    let mut manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.pinned = true;
    write_json(&manifest_path, &manifest)?;

    let result = retire(&store, &canonical_root, &eviction, revision)?;

    assert!(matches!(
        result,
        RetirementAttempt::Skipped(reason) if reason.contains("became pinned")
    ));
    assert!(store.layout.arena(context.arena_id()).is_dir());
    Ok(())
}

#[test]
fn manifest_substitution_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let (eviction, revision, canonical_root) = snapshot(&store)?;
    let manifest_path = store.layout.manifest(context.arena_id());
    let mut manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.store_id = Uuid::new_v4();
    write_json(&manifest_path, &manifest)?;

    let result = retire(&store, &canonical_root, &eviction, revision);

    assert!(matches!(result, Err(StoreError::InvalidOwnership { .. })));
    assert!(store.layout.arena(context.arena_id()).is_dir());
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlink_substitution_never_reaches_the_external_directory()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let store_root = tempdir()?;
    let project = tempdir()?;
    let outside_root = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let (eviction, revision, canonical_root) = snapshot(&store)?;
    let arena = store.layout.arena(context.arena_id());
    let outside = outside_root.path().join("stolen-arena");
    fs::rename(&arena, &outside)?;
    symlink(&outside, &arena)?;

    let result = retire(&store, &canonical_root, &eviction, revision);

    assert!(matches!(result, Err(StoreError::InvalidOwnership { .. })));
    assert!(outside.join("build/artifact.rlib").is_file());
    Ok(())
}

#[cfg(unix)]
#[test]
fn substituted_build_directory_blocks_retirement() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let store_root = tempdir()?;
    let project = tempdir()?;
    let outside_root = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let (eviction, revision, canonical_root) = snapshot(&store)?;
    let build = store.layout.build_dir(context.arena_id());
    let outside = outside_root.path().join("stolen-build");
    fs::rename(&build, &outside)?;
    symlink(&outside, &build)?;

    let result = retire(&store, &canonical_root, &eviction, revision);

    assert!(matches!(result, Err(StoreError::InvalidOwnership { .. })));
    assert!(store.layout.arena(context.arena_id()).is_dir());
    assert!(outside.join("artifact.rlib").is_file());
    Ok(())
}

#[cfg(unix)]
#[test]
fn substituted_trash_directory_blocks_retirement() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let store_root = tempdir()?;
    let project = tempdir()?;
    let outside_root = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let (eviction, revision, canonical_root) = snapshot(&store)?;
    let trash = store.layout.trash();
    fs::remove_dir(&trash)?;
    symlink(outside_root.path(), &trash)?;

    let result = retire(&store, &canonical_root, &eviction, revision);

    assert!(matches!(result, Err(StoreError::InvalidOwnership { .. })));
    assert!(store.layout.arena(context.arena_id()).is_dir());
    assert!(fs::read_dir(outside_root.path())?.next().is_none());
    Ok(())
}

#[test]
fn an_orphaned_worktree_that_reappears_is_kept() -> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let inventory = store.inventory()?;
    let entry = &inventory.arenas[0];
    let eviction = Eviction {
        arena_id: context.arena_id().clone(),
        size: entry.record.size,
        reason: EvictionReason::OrphanedWorktree,
    };

    let result = retire(&store, &inventory.store_root, &eviction, entry.revision)?;

    assert!(matches!(result, RetirementAttempt::Skipped(reason) if reason.contains("reappeared")));
    assert!(store.layout.arena(context.arena_id()).is_dir());
    Ok(())
}

fn snapshot(store: &Store) -> Result<(Eviction, u64, std::path::PathBuf), StoreError> {
    let inventory = store.inventory()?;
    let entry = inventory
        .arenas
        .first()
        .ok_or_else(|| StoreError::InvalidOwnership {
            path: inventory.store_root.clone(),
            reason: "test arena is missing".to_owned(),
        })?;
    Ok((
        Eviction {
            arena_id: entry.record.id.clone(),
            size: entry.record.size,
            reason: EvictionReason::LeastRecentlyUsed,
        },
        entry.revision,
        inventory.store_root,
    ))
}

fn arena_id(key: &str) -> ArenaId {
    ArenaId::derive(
        &RepositoryId::derive("repository"),
        &WorktreeId::derive(key),
        &WorkspaceId::derive("workspace"),
        &ToolchainId::derive("toolchain"),
    )
}
