use std::{fs, io};

use tempfile::tempdir;
use zhold_core::{ArenaState, BuildOutcome};

use crate::{
    Store,
    io::read_json,
    manifest::ArenaManifest,
    test_support::{context, invocation, mark_spawned},
};

#[test]
fn durable_lifecycle_stage_follows_process_creation() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = context(project.path())?;
    let invocation = invocation(project.path())?;
    let manifest_path = store.layout.manifest(context.arena_id());
    let mut lease = store.lease(&context, &invocation)?;

    assert_eq!(lifecycle_stage(&manifest_path)?, "reserved");
    lease.mark_spawning()?;
    assert_eq!(lifecycle_stage(&manifest_path)?, "spawning");
    lease.mark_spawned()?;
    assert_eq!(lifecycle_stage(&manifest_path)?, "spawned");
    lease.finish(BuildOutcome::Succeeded)?;
    assert_eq!(lifecycle_stage(&manifest_path)?, "finalized");
    Ok(())
}

#[test]
fn dropped_spawned_lease_remains_suspect_without_cleanup_proof()
-> Result<(), Box<dyn std::error::Error>> {
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
    assert_eq!(inventory.arenas[0].record.state(), ArenaState::Suspect);
    assert_eq!(inventory.arenas[0].record.last_outcome, None);
    assert!(!store.layout.reservation_profile().exists());
    assert!(matches!(
        store.lease(&context, &invocation),
        Err(crate::StoreError::ArenaSuspect(_))
    ));
    Ok(())
}

#[test]
fn explicit_unconfirmed_cleanup_handoff_leaves_a_suspect_arena()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = context(project.path())?;
    let invocation = invocation(project.path())?;

    let mut lease = store.lease(&context, &invocation)?;
    mark_spawned(&mut lease)?;
    lease.leave_suspect()?;

    let inventory = store.inventory()?;
    assert_eq!(inventory.arenas[0].record.state(), ArenaState::Suspect);
    assert_eq!(inventory.arenas[0].record.last_outcome, None);
    Ok(())
}

fn lifecycle_stage(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    let manifest: ArenaManifest = read_json(path)?;
    serde_json::to_value(manifest)?["lifecycle_stage"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other("manifest has no durable lifecycle stage").into())
}
