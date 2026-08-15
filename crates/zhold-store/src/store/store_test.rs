use std::{fs, io, thread, time::Duration};

use tempfile::tempdir;
use zhold_core::{ArenaState, BuildOutcome};

#[cfg(unix)]
use crate::StoreError;
use crate::{
    io::read_json,
    manifest::{ArenaManifest, StoreMarker},
    test_support::{context, invocation},
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
fn enforces_owner_only_store_permissions() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let temporary = tempdir()?;
    fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o755))?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = context(project.path())?;
    let invocation = invocation(project.path())?;
    let lease = store.lease(&context, &invocation)?;
    lease.finish(BuildOutcome::Succeeded)?;

    let expected_owner = nix::unistd::Uid::effective().as_raw();
    for directory in [
        temporary.path().to_path_buf(),
        store.layout.arenas(),
        store.layout.arena(context.arena_id()),
        store.layout.build_dir(context.arena_id()),
    ] {
        let metadata = fs::metadata(directory)?;
        assert_eq!(metadata.uid(), expected_owner);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
    }
    for file in [
        store.layout.marker(),
        store.layout.manifest(context.arena_id()),
        store.layout.arena_lock(context.arena_id()),
    ] {
        let metadata = fs::metadata(file)?;
        assert_eq!(metadata.uid(), expected_owner);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }
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
