use std::fs;

use tempfile::tempdir;
use uuid::Uuid;

use crate::{
    BuildContext, CollectionReceiptSource, HistoryPayload, HistoryQuery, Store,
    io::{create_json, read_json, write_json},
    manifest::{ArenaManifest, RetirementRecord},
    test_support::{create_idle_arena, finish_succeeded},
};

use super::TrashOutcome;

#[test]
fn dry_run_and_retry_delete_only_owned_retirement_entries() -> Result<(), Box<dyn std::error::Error>>
{
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let retired = retire_for_test(&store, &context)?;
    let cached = store.inventory_cached()?;

    let preview = store.retry_trash(true)?;
    assert!(retired.is_dir());
    assert!(preview.before > zhold_core::ByteSize::ZERO);
    assert!(cached.trash > zhold_core::ByteSize::ZERO);
    assert_eq!(cached.trash_quality, zhold_core::SizeQuality::Cached);
    assert_eq!(preview.entries.len(), 1);
    assert!(matches!(
        preview.entries[0].outcome,
        TrashOutcome::WouldDelete
    ));
    assert!(preview.history.receipt_id.is_none());

    let deletion = store.retry_trash(false)?;
    assert!(!retired.exists());
    assert_eq!(deletion.remaining, zhold_core::ByteSize::ZERO);
    assert!(matches!(deletion.entries[0].outcome, TrashOutcome::Deleted));
    assert!(deletion.history.receipt_id.is_some());
    let history = store.history(&HistoryQuery {
        kind: Some(zhold_core::HistoryKind::Collection),
        ..HistoryQuery::default()
    })?;
    let HistoryPayload::Collection(receipt) = &history.receipts[0].payload else {
        return Err("expected trash collection receipt".into());
    };
    assert_eq!(receipt.source, CollectionReceiptSource::TrashRetry);
    Ok(())
}

#[test]
fn a_live_lease_for_the_same_identity_blocks_trash_retry() -> Result<(), Box<dyn std::error::Error>>
{
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, invocation) = create_idle_arena(&store, project.path(), 4_096)?;
    let retired = retire_for_test(&store, &context)?;
    let lease = store.lease(&context, &invocation)?;

    let blocked = store.retry_trash(false)?;

    assert!(retired.is_dir());
    assert!(matches!(
        blocked.entries[0].outcome,
        TrashOutcome::Skipped { .. }
    ));
    finish_succeeded(lease)?;
    let retried = store.retry_trash(false)?;
    assert!(matches!(retried.entries[0].outcome, TrashOutcome::Deleted));
    Ok(())
}

#[test]
fn foreign_retirement_entries_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let store = Store::open(store_root.path())?;
    let foreign = store.layout.trash().join("foreign");
    fs::create_dir(&foreign)?;
    fs::write(foreign.join("keep.txt"), b"keep")?;

    let report = store.retry_trash(false)?;

    assert!(foreign.join("keep.txt").is_file());
    assert_eq!(report.entries.len(), 1);
    assert!(matches!(
        report.entries[0].outcome,
        TrashOutcome::Skipped { .. }
    ));
    assert!(report.remaining > zhold_core::ByteSize::ZERO);
    Ok(())
}

#[test]
fn a_copied_manifest_without_the_retirement_nonce_fails_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let forged = store
        .layout
        .trash_destination(context.arena_id(), Uuid::new_v4());
    fs::create_dir_all(forged.join("build"))?;
    fs::copy(
        store.layout.manifest(context.arena_id()),
        forged.join("arena.json"),
    )?;
    fs::write(forged.join("build/keep.txt"), b"keep")?;

    let report = store.retry_trash(false)?;

    assert!(forged.join("build/keep.txt").is_file());
    assert!(matches!(
        report.entries[0].outcome,
        TrashOutcome::Skipped { .. }
    ));
    Ok(())
}

#[test]
fn a_partially_deleted_retirement_keeps_external_retry_authority()
-> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let retired = retire_for_test(&store, &context)?;
    fs::remove_file(retired.join("arena.json"))?;
    fs::remove_dir_all(retired.join("build"))?;
    fs::write(retired.join("remaining.bin"), b"remaining")?;

    let report = store.retry_trash(false)?;

    assert!(!retired.exists());
    assert!(matches!(report.entries[0].outcome, TrashOutcome::Deleted));
    assert!(store.layout.trash_index().read_dir()?.next().is_none());
    Ok(())
}

#[cfg(unix)]
#[test]
fn an_unmeasurable_trash_tree_fails_closed_instead_of_counting_zero()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let retired = retire_for_test(&store, &context)?;
    let blocked = retired.join("blocked");
    fs::create_dir(&blocked)?;
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000))?;

    let result = store.retry_trash(false);

    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700))?;
    assert!(result.is_err());
    assert!(retired.is_dir());
    Ok(())
}

fn retire_for_test(
    store: &Store,
    context: &BuildContext,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let arena = store.layout.arena(context.arena_id());
    let retirement_id = Uuid::new_v4();
    let retired = store
        .layout
        .trash_destination(context.arena_id(), retirement_id);
    let manifest_path = store.layout.manifest(context.arena_id());
    let mut manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.prepare_retirement(retirement_id);
    write_json(&manifest_path, &manifest)?;
    let record_path = store.layout.retirement_record(retirement_id);
    let record = RetirementRecord::create(
        store,
        context.arena_id().clone(),
        retirement_id,
        manifest.revision,
        crate::io::measure_tree(&arena)?,
    );
    assert!(create_json(&record_path, &record)?);
    fs::rename(arena, &retired)?;
    Ok(retired)
}
