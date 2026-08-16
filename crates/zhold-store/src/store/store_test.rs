use std::{fs, io};

use tempfile::tempdir;
use zhold_core::{ArenaState, BuildOutcome};

#[cfg(unix)]
use crate::StoreError;
use crate::{
    io::read_json,
    manifest::{ArenaManifest, StoreMarker},
    test_support::{context, finish_succeeded, invocation, mark_spawned},
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
fn concurrent_store_initializers_serialize_marker_publication()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let root = temporary.path().to_path_buf();
    let contender = root.clone();
    let opener = std::thread::spawn(move || Store::open(contender));
    let store = Store::open(root)?;
    let concurrent = opener
        .join()
        .map_err(|_| io::Error::other("concurrent initializer thread failed"))??;

    assert_eq!(store.info().store_id, concurrent.info().store_id);
    Ok(())
}

#[test]
fn abandoned_store_marker_staging_is_recovered() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let staging = temporary
        .path()
        .join(format!("store.json.{}.new", uuid::Uuid::new_v4()));
    fs::write(&staging, serde_json::to_vec_pretty(&StoreMarker::create())?)?;

    let store = Store::open(temporary.path())?;

    assert!(store.info().root.join("store.json").is_file());
    assert!(!staging.exists());
    assert!(store.info().root.join("store.initialize.lock").is_file());
    Ok(())
}

#[test]
fn version_one_store_markers_gain_a_private_fingerprint_key()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store_id = uuid::Uuid::new_v4();
    fs::write(
        temporary.path().join("store.json"),
        format!("{{\"schema_version\":1,\"store_id\":\"{store_id}\"}}"),
    )?;

    let store = Store::open(temporary.path())?;
    let marker: StoreMarker = read_json(&store.layout.marker())?;

    assert_eq!(store.info().store_id, store_id);
    assert_eq!(marker.schema_version, crate::manifest::STORE_SCHEMA_VERSION);
    assert_ne!(marker.fingerprint_key(), &[0; 32]);
    Ok(())
}

#[test]
fn dropped_reserved_lease_records_a_not_started_run() -> Result<(), Box<dyn std::error::Error>> {
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
        Some(BuildOutcome::NotStarted)
    );
    assert!(!store.layout.reservation_profile().exists());
    Ok(())
}

#[test]
fn dropped_spawned_lease_records_a_terminated_run() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = context(project.path())?;
    let invocation = invocation(project.path())?;

    let mut lease = store.lease(&context, &invocation)?;
    mark_spawned(&mut lease)?;
    fs::write(lease.build_dir().join("partial-output"), vec![0_u8; 64])?;
    drop(lease);

    let inventory = store.inventory()?;
    assert_eq!(
        inventory.arenas[0].record.last_outcome,
        Some(BuildOutcome::Terminated)
    );
    assert!(store.layout.reservation_profile().is_file());
    Ok(())
}

#[test]
fn refuses_to_reuse_an_unfinished_arena_without_a_live_lease()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = context(project.path())?;
    let invocation = invocation(project.path())?;
    finish_succeeded(store.lease(&context, &invocation)?)?;
    let manifest_path = store.layout.manifest(context.arena_id());
    let mut manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.last_started_at = Some(manifest.last_used_at.saturating_add(1));
    manifest.last_finished_at = None;
    manifest.reservation = zhold_core::ByteSize::from_bytes(4096);
    crate::io::write_json(&manifest_path, &manifest)?;

    let inventory = store.inventory()?;
    assert_eq!(inventory.arenas[0].record.state(), ArenaState::Suspect);
    assert_eq!(inventory.reserved, manifest.reservation);
    assert!(matches!(
        store.lease(&context, &invocation),
        Err(crate::StoreError::ArenaSuspect(_))
    ));
    Ok(())
}

#[test]
fn expired_pins_stop_protecting_an_arena() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = context(project.path())?;
    let invocation = invocation(project.path())?;
    finish_succeeded(store.lease(&context, &invocation)?)?;
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
    finish_succeeded(store.lease(&context, &invocation)?)?;

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
    finish_succeeded(store.lease(&context, &invocation)?)?;
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
