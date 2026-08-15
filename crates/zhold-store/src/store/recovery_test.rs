use tempfile::tempdir;
use zhold_core::{BuildOutcome, ByteSize, CommandDescriptor, HistoryKind};

use crate::{
    HistoryPayload, HistoryQuery, RecoveryReason, Store,
    io::{read_json, write_json},
    manifest::ArenaManifest,
    test_support::create_idle_arena,
};

#[test]
fn explicit_suspect_recovery_publishes_an_audit_receipt() -> Result<(), Box<dyn std::error::Error>>
{
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let manifest_path = store.layout.manifest(context.arena_id());
    let mut manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.begin(
        &context,
        CommandDescriptor::default(),
        ByteSize::from_bytes(8_192),
        crate::time::unix_seconds()?,
    );
    write_json(&manifest_path, &manifest)?;

    let history = store.recover_suspect(context.arena_id())?;
    let report = store.history(&HistoryQuery {
        kind: Some(HistoryKind::Recovery),
        ..HistoryQuery::default()
    })?;

    assert!(history.receipt_id.is_some());
    let HistoryPayload::Recovery(receipt) = &report.receipts[0].payload else {
        return Err("expected recovery receipt".into());
    };
    assert_eq!(receipt.arena_id, *context.arena_id());
    assert_eq!(receipt.previous_state, zhold_core::ArenaState::Suspect);
    assert_eq!(receipt.outcome, BuildOutcome::Terminated);
    assert_eq!(receipt.reason, RecoveryReason::ProcessTreeConfirmedStopped);
    assert_eq!(
        receipt.store_schema_version,
        crate::manifest::STORE_SCHEMA_VERSION
    );
    Ok(())
}
