use std::{fs, io, path::Path, thread, time::Duration};

use tempfile::tempdir;
use zhold_core::{
    ArenaId, ArenaState, BuildOutcome, ByteSize, CollectionPolicy, RepositoryId, ToolchainId,
    WorkspaceId, WorktreeId,
};

use crate::{
    BuildContext, CargoInvocation, StoreError,
    io::read_json,
    manifest::{ArenaManifest, StoreMarker},
};

use super::Store;

#[test]
fn initializes_an_empty_marked_store() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let _store = Store::open(temporary.path())?;

    assert!(temporary.path().join("store.json").is_file());
    assert!(temporary.path().join("arenas").is_dir());
    Ok(())
}

#[test]
fn refuses_to_claim_a_non_empty_unmarked_directory() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let foreign = temporary.path().join("foreign.txt");
    fs::write(&foreign, b"not zhold")?;

    assert!(Store::open(temporary.path()).is_err());
    Ok(())
}

#[test]
fn concurrent_marker_publication_is_waited_out() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let marker = StoreMarker::create();
    let staging = temporary
        .path()
        .join(format!("store.json.{}.new", uuid::Uuid::new_v4()));
    let published = temporary.path().join("store.json");
    fs::write(&staging, serde_json::to_vec_pretty(&marker)?)?;
    let publisher = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        fs::hard_link(&staging, &published)?;
        fs::remove_file(staging)
    });

    let store = Store::open(temporary.path())?;
    publisher
        .join()
        .map_err(|_| io::Error::other("marker publisher thread failed"))??;

    assert_eq!(store.info().store_id, marker.store_id);
    Ok(())
}

#[test]
fn lease_is_authoritative_for_inventory_and_collection() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = context(project.path())?;
    let invocation = invocation(project.path())?;
    let lease = store.lease(&context, &invocation)?;
    fs::write(lease.build_dir().join("artifact.rlib"), vec![0_u8; 4_096])?;

    let active = store.inventory()?;
    assert_eq!(active.arenas.len(), 1);
    assert!(active.arenas[0].record.active);

    let blocked = store.collect(
        CollectionPolicy::new(zhold_core::ByteSize::from_bytes(1)),
        false,
    )?;
    assert!(blocked.retirements.is_empty());
    assert!(!blocked.budget_met);

    lease.finish(BuildOutcome::Succeeded)?;
    let retired = store.collect(
        CollectionPolicy::new(zhold_core::ByteSize::from_bytes(1)),
        false,
    )?;
    assert_eq!(retired.retirements.len(), 1);
    assert!(store.inventory()?.arenas.is_empty());
    Ok(())
}

#[test]
fn dropped_lease_records_a_terminated_run() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = context(project.path())?;
    let invocation = invocation(project.path())?;

    {
        let _lease = store.lease(&context, &invocation)?;
    }

    let inventory = store.inventory()?;
    assert_eq!(
        inventory.arenas[0].record.last_outcome,
        Some(BuildOutcome::Terminated)
    );
    Ok(())
}

#[test]
fn live_reservations_are_admission_accounting_and_clear_on_finish()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = context(project.path())?;
    let invocation = invocation(project.path())?;
    let reservation = ByteSize::from_bytes(4_096);
    let (lease, report) = store.lease_reserved_and_collect(
        &context,
        &invocation,
        reservation,
        CollectionPolicy::new(ByteSize::ZERO),
    )?;
    let active = store.inventory()?;

    assert_eq!(active.reserved, reservation);
    assert_eq!(report.reserved, reservation);
    assert!(!report.budget_met);

    let peak = ByteSize::from_bytes(8_192);
    lease.finish_with_peak(BuildOutcome::Succeeded, peak)?;
    let finished = store.inventory()?;
    assert_eq!(finished.reserved, ByteSize::ZERO);
    assert_eq!(finished.arenas[0].last_peak, peak);
    Ok(())
}

#[test]
fn expired_pins_stop_protecting_an_arena() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = context(project.path())?;
    let invocation = invocation(project.path())?;
    let lease = store.lease(&context, &invocation)?;
    lease.finish(BuildOutcome::Succeeded)?;
    let expires_at = store.pin_for(context.arena_id(), Some(3_600))?;
    assert!(expires_at.is_some());
    assert_eq!(
        store.inventory()?.arenas[0].record.state(),
        ArenaState::Pinned
    );

    let manifest_path = store.layout.manifest(context.arena_id());
    let mut manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.pin_expires_at = Some(0);
    crate::io::write_json(&manifest_path, &manifest)?;

    assert_eq!(
        store.inventory()?.arenas[0].record.state(),
        ArenaState::Idle
    );
    Ok(())
}

#[test]
fn refuses_to_adopt_an_existing_unmarked_arena() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = context(project.path())?;
    let invocation = invocation(project.path())?;
    let arena = store.layout.arena(context.arena_id());
    let parent = arena.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "arena has no prefix directory")
    })?;
    fs::create_dir_all(parent)?;
    fs::create_dir(&arena)?;
    fs::write(arena.join("foreign.bin"), b"not owned")?;

    assert!(store.lease(&context, &invocation).is_err());
    assert!(arena.join("foreign.bin").is_file());
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_a_symlink_store_root() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let temporary = tempdir()?;
    let real = temporary.path().join("real");
    let link = temporary.path().join("link");
    fs::create_dir(&real)?;
    symlink(&real, &link)?;

    assert!(matches!(
        Store::open(&link),
        Err(StoreError::InvalidOwnership { .. })
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejected_build_substitution_does_not_advance_the_manifest()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let temporary = tempdir()?;
    let project = tempdir()?;
    let outside = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = context(project.path())?;
    let invocation = invocation(project.path())?;
    let lease = store.lease(&context, &invocation)?;
    lease.finish(BuildOutcome::Succeeded)?;
    let manifest_path = store.layout.manifest(context.arena_id());
    let before: ArenaManifest = read_json(&manifest_path)?;
    let build = store.layout.build_dir(context.arena_id());
    let moved = outside.path().join("build");
    fs::rename(&build, &moved)?;
    symlink(&moved, &build)?;

    assert!(matches!(
        store.lease(&context, &invocation),
        Err(StoreError::InvalidOwnership { .. })
    ));
    let after: ArenaManifest = read_json(&manifest_path)?;
    assert_eq!(after.revision, before.revision);
    Ok(())
}

fn invocation(root: &Path) -> Result<CargoInvocation, StoreError> {
    CargoInvocation::new(
        "cargo".to_owned(),
        vec!["test".to_owned()],
        root.to_path_buf(),
    )
}

fn context(root: &Path) -> Result<BuildContext, Box<dyn std::error::Error>> {
    let worktree_root = root.canonicalize()?;
    let git_common_dir = worktree_root.join(".git");
    fs::create_dir(&git_common_dir)?;
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
