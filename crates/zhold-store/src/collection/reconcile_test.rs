use std::fs;

use tempfile::tempdir;
use uuid::Uuid;

use super::TrashOutcome;
use crate::{
    BuildContext, Store,
    io::{create_json, read_json, write_json},
    manifest::{ArenaManifest, RetirementRecord},
    test_support::create_idle_arena,
};

#[test]
fn journal_only_state_is_completed_without_requiring_trash()
-> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let state = prepare(&store, &context)?;
    fs::rename(&state.original, &state.trash)?;
    fs::remove_dir_all(&state.trash)?;

    let report = store.retry_trash(false)?;

    assert!(!state.record.exists());
    assert!(matches!(report.entries[0].outcome, TrashOutcome::Repaired));
    Ok(())
}

#[test]
fn pre_rename_state_resumes_the_recorded_retirement() -> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let state = prepare(&store, &context)?;

    let report = store.retry_trash(false)?;

    assert!(!state.original.exists());
    assert!(!state.trash.exists());
    assert!(!state.record.exists());
    assert!(matches!(report.entries[0].outcome, TrashOutcome::Deleted));
    Ok(())
}

#[test]
fn orphaned_active_intent_is_rolled_back_without_deleting_the_arena()
-> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let state = prepare(&store, &context)?;
    fs::remove_file(&state.record)?;
    assert_eq!(store.inventory_cached()?.uncertain_owned, 1);

    let report = store.retry_trash(false)?;
    let manifest: ArenaManifest = read_json(&store.layout.manifest(context.arena_id()))?;

    assert!(state.original.is_dir());
    assert_eq!(manifest.retirement_id, None);
    assert_eq!(store.inventory_cached()?.uncertain_owned, 0);
    assert!(
        report
            .entries
            .iter()
            .any(|entry| matches!(entry.outcome, TrashOutcome::Repaired))
    );
    Ok(())
}

#[test]
fn duplicate_original_and_trash_for_one_transaction_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let state = prepare(&store, &context)?;
    fs::create_dir(&state.trash)?;
    fs::write(state.trash.join("keep.bin"), b"keep")?;

    let report = store.retry_trash(false)?;

    assert!(state.original.is_dir());
    assert!(state.trash.join("keep.bin").is_file());
    assert!(state.record.is_file());
    assert!(matches!(
        report.entries[0].outcome,
        TrashOutcome::Skipped { .. }
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn cached_inventory_rejects_a_substituted_trash_symlink() -> Result<(), Box<dyn std::error::Error>>
{
    use std::os::unix::fs::symlink;

    let store_root = tempdir()?;
    let project = tempdir()?;
    let outside_root = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let state = prepare(&store, &context)?;
    let outside = outside_root.path().join("retired");
    fs::rename(&state.original, &outside)?;
    symlink(&outside, &state.trash)?;

    let inventory = store.inventory_cached()?;
    let report = store.retry_trash(false)?;

    assert_eq!(inventory.trash_quality, zhold_core::SizeQuality::Unknown);
    assert!(
        inventory
            .findings
            .iter()
            .any(|finding| finding.path == state.record)
    );
    assert!(outside.join("build/artifact.rlib").is_file());
    assert!(matches!(
        report.entries[0].outcome,
        TrashOutcome::Skipped { .. }
    ));
    Ok(())
}

struct RetirementFixture {
    original: std::path::PathBuf,
    trash: std::path::PathBuf,
    record: std::path::PathBuf,
}

fn prepare(
    store: &Store,
    context: &BuildContext,
) -> Result<RetirementFixture, Box<dyn std::error::Error>> {
    let original = store.layout.arena(context.arena_id());
    let retirement_id = Uuid::new_v4();
    let trash = store
        .layout
        .trash_destination(context.arena_id(), retirement_id);
    let manifest_path = store.layout.manifest(context.arena_id());
    let mut manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.prepare_retirement(retirement_id);
    write_json(&manifest_path, &manifest)?;
    let record = store.layout.retirement_record(retirement_id);
    let journal = RetirementRecord::create(
        store,
        context.arena_id().clone(),
        retirement_id,
        manifest.revision,
        crate::io::measure_tree(&original)?,
    );
    assert!(create_json(&record, &journal)?);
    Ok(RetirementFixture {
        original,
        trash,
        record,
    })
}
