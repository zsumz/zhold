use std::io;

use tempfile::tempdir;
use zhold_core::BuildOutcome;

use crate::{
    Store,
    io::read_json,
    manifest::ArenaManifest,
    test_support::{context, invocation},
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

fn lifecycle_stage(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    let manifest: ArenaManifest = read_json(path)?;
    serde_json::to_value(manifest)?["lifecycle_stage"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other("manifest has no durable lifecycle stage").into())
}
