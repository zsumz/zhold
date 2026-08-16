use std::fs;

use tempfile::tempdir;
use uuid::Uuid;
use zhold_core::{BuildOutcome, ByteSize, CommandDescriptor};

use crate::{
    Store, StoreError,
    io::{create_json, read_json},
    manifest::{ArenaManifest, InitializationRecord},
    test_support::{context, create_idle_arena},
    time::unix_seconds,
};

#[test]
fn journal_without_a_staging_directory_is_reconciled() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(root.path())?;
    let context = context(project.path())?;
    let initialization_id = Uuid::new_v4();
    let record =
        InitializationRecord::create(&store, context.arena_id().clone(), initialization_id);
    let record_path = store.layout.initialization_record(initialization_id);
    create_json(&record_path, &record)?;
    drop(store);

    let reopened = Store::open_read_write(root.path())?;

    assert!(!record_path.exists());
    assert!(!reopened.layout.arena(context.arena_id()).exists());
    Ok(())
}

#[test]
fn journal_authorizes_cleanup_of_a_partial_staging_tree() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(root.path())?;
    let context = context(project.path())?;
    let initialization_id = Uuid::new_v4();
    let record =
        InitializationRecord::create(&store, context.arena_id().clone(), initialization_id);
    let record_path = store.layout.initialization_record(initialization_id);
    create_json(&record_path, &record)?;
    fs::create_dir_all(record.staging_path().join("build"))?;
    drop(store);

    Store::open_read_write(root.path())?;

    assert!(!record_path.exists());
    assert!(!record.staging_path().exists());
    Ok(())
}

#[test]
fn durable_reserved_staging_is_recovered_and_promoted() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(root.path())?;
    let context = context(project.path())?;
    let initialization_id = Uuid::new_v4();
    let record =
        InitializationRecord::create(&store, context.arena_id().clone(), initialization_id);
    let record_path = store.layout.initialization_record(initialization_id);
    create_json(&record_path, &record)?;
    fs::create_dir_all(record.staging_path().join("build"))?;
    let now = unix_seconds()?;
    let mut manifest = ArenaManifest::create(
        store.marker.store_id,
        &context,
        Some(initialization_id),
        now,
    );
    manifest.begin(
        &context,
        CommandDescriptor::default(),
        ByteSize::from_bytes(4_096),
        now,
    );
    manifest.observe_size(ByteSize::ZERO);
    create_json(&record.staging_path().join("arena.json"), &manifest)?;
    drop(store);

    let reopened = Store::open_read_write(root.path())?;
    let recovered: ArenaManifest = read_json(&reopened.layout.manifest(context.arena_id()))?;

    assert_eq!(recovered.last_outcome, Some(BuildOutcome::NotStarted));
    assert!(!record_path.exists());
    assert!(!record.staging_path().exists());
    Ok(())
}

#[test]
fn backup_only_staged_manifest_is_recovered_and_promoted() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(root.path())?;
    let context = context(project.path())?;
    let initialization_id = Uuid::new_v4();
    let record =
        InitializationRecord::create(&store, context.arena_id().clone(), initialization_id);
    let record_path = store.layout.initialization_record(initialization_id);
    create_json(&record_path, &record)?;
    fs::create_dir_all(record.staging_path().join("build"))?;
    let now = unix_seconds()?;
    let mut manifest = ArenaManifest::create(
        store.marker.store_id,
        &context,
        Some(initialization_id),
        now,
    );
    manifest.begin(
        &context,
        CommandDescriptor::default(),
        ByteSize::from_bytes(4_096),
        now,
    );
    let manifest_path = record.staging_path().join("arena.json");
    create_json(&manifest_path, &manifest)?;
    crate::io::json_recovery_test::rotate_to_backup(&manifest_path)?;
    drop(store);

    let reopened = Store::open_read_write(root.path())?;
    let recovered: ArenaManifest = read_json(&reopened.layout.manifest(context.arena_id()))?;

    assert_eq!(recovered.last_outcome, Some(BuildOutcome::NotStarted));
    assert!(!record_path.exists());
    assert!(!record.staging_path().exists());
    Ok(())
}

#[test]
fn completed_arena_with_a_leftover_journal_is_reconciled() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 64)?;
    let manifest: ArenaManifest = read_json(&store.layout.manifest(context.arena_id()))?;
    let initialization_id = manifest
        .initialization_id
        .ok_or("new arena did not retain its initialization identity")?;
    let record =
        InitializationRecord::create(&store, context.arena_id().clone(), initialization_id);
    let record_path = store.layout.initialization_record(initialization_id);
    create_json(&record_path, &record)?;
    drop(store);

    Store::open_read_write(root.path())?;

    assert!(!record_path.exists());
    assert!(record.final_path().is_dir());
    Ok(())
}

#[test]
fn staging_without_a_journal_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(root.path())?;
    let context = context(project.path())?;
    let staging = store
        .layout
        .arena_staging(context.arena_id(), Uuid::new_v4());
    fs::create_dir_all(staging.join("build"))?;
    drop(store);

    let reopened = Store::open_read_write(root.path());

    assert!(matches!(reopened, Err(StoreError::InvalidOwnership { .. })));
    assert!(staging.is_dir());
    Ok(())
}
