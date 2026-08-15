use std::{fs, sync::Arc, thread};

use tempfile::tempdir;
use zhold_core::{
    BuildOutcome, ByteSize, CargoCommandClass, CollectionPolicy, HistoryKind, HistoryPolicy,
};

use super::{CollectionReceiptSource, HistoryPayload, HistoryPruneRequest, HistoryQuery};
use crate::{CargoInvocation, Store, io::read_json, manifest::ArenaManifest, test_support};

#[test]
fn completed_build_publishes_a_private_peak_receipt() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let worktree = temporary.path().join("worktree");
    fs::create_dir(&worktree)?;
    let context = test_support::context(&worktree)?;
    let invocation = CargoInvocation::new(
        "cargo".to_owned(),
        vec!["check".to_owned(), "--token=secret-value".to_owned()],
        worktree,
    )?;

    let lease = store.lease_reserved(&context, &invocation, ByteSize::from_bytes(400))?;
    fs::write(lease.build_dir().join("artifact"), vec![1_u8; 200])?;
    let finalization = lease.finish_observed(
        BuildOutcome::Succeeded,
        ByteSize::from_bytes(300),
        Some(ByteSize::from_bytes(250)),
        true,
    )?;

    assert_eq!(finalization.history.len(), 1);
    assert!(finalization.history[0].warnings.is_empty());
    let report = store.history(&HistoryQuery::default())?;
    assert_eq!(report.receipts.len(), 1);
    let HistoryPayload::Build(build) = &report.receipts[0].payload else {
        return Err("expected build receipt".into());
    };
    assert_eq!(build.observed_peak, ByteSize::from_bytes(300));
    assert_eq!(build.reservation, ByteSize::from_bytes(400));
    assert_eq!(build.command_class, CargoCommandClass::Check);
    assert!(build.warning_threshold_exceeded);
    let encoded = serde_json::to_string(&report)?;
    assert!(!encoded.contains("secret-value"));
    Ok(())
}

#[test]
fn collection_and_filters_are_newest_first() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let worktree = temporary.path().join("worktree");
    fs::create_dir(&worktree)?;
    test_support::create_idle_arena(&store, &worktree, 128)?;

    let report = store.collect(CollectionPolicy::new(ByteSize::from_bytes(1)), false)?;
    assert!(report.history.receipt_id.is_some());
    let history = store.history(&HistoryQuery {
        kind: Some(HistoryKind::Collection),
        limit: 1,
        ..HistoryQuery::default()
    })?;
    assert_eq!(history.receipts.len(), 1);
    assert!(matches!(
        history.receipts[0].payload,
        HistoryPayload::Collection(_)
    ));
    assert!(!history.more);
    Ok(())
}

#[test]
fn post_build_collection_records_its_lifecycle_source() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;

    let report = store.collect_post_build(CollectionPolicy::new(ByteSize::from_bytes(1)))?;
    assert!(report.history.receipt_id.is_some());
    let history = store.history(&HistoryQuery {
        kind: Some(HistoryKind::Collection),
        limit: 1,
        ..HistoryQuery::default()
    })?;
    let Some(receipt) = history.receipts.first() else {
        return Err("missing collection receipt".into());
    };
    let HistoryPayload::Collection(collection) = &receipt.payload else {
        return Err("expected collection receipt".into());
    };
    assert_eq!(collection.source, CollectionReceiptSource::PostBuild);
    Ok(())
}

#[test]
fn zero_retention_and_disabled_publication_are_deterministic()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    store.set_history_policy(HistoryPolicy {
        enabled: true,
        max_receipts: 0,
        max_bytes: ByteSize::ZERO,
    })?;
    let worktree = temporary.path().join("first");
    fs::create_dir(&worktree)?;
    test_support::create_idle_arena(&store, &worktree, 1)?;
    assert!(store.history(&HistoryQuery::default())?.receipts.is_empty());

    store.set_history_policy(HistoryPolicy {
        enabled: false,
        max_receipts: 10,
        max_bytes: ByteSize::from_bytes(1_000_000),
    })?;
    let second = temporary.path().join("second");
    fs::create_dir(&second)?;
    test_support::create_idle_arena(&store, &second, 1)?;
    assert!(store.history(&HistoryQuery::default())?.receipts.is_empty());
    Ok(())
}

#[test]
fn pruning_never_removes_invalid_or_foreign_entries() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let worktree = temporary.path().join("worktree");
    fs::create_dir(&worktree)?;
    test_support::create_idle_arena(&store, &worktree, 1)?;
    let foreign = store.info().root.join("history/receipts/foreign.txt");
    fs::write(&foreign, b"not owned")?;

    let dry = store.prune_history(HistoryPruneRequest {
        keep: Some(0),
        max_bytes: None,
        older_than: None,
        dry_run: true,
    })?;
    assert_eq!(dry.removed_count, 1);
    assert!(foreign.is_file());
    assert_eq!(store.history(&HistoryQuery::default())?.receipts.len(), 1);

    let actual = store.prune_history(HistoryPruneRequest {
        keep: Some(0),
        max_bytes: None,
        older_than: None,
        dry_run: false,
    })?;
    assert_eq!(actual.findings.len(), 1);
    assert!(foreign.is_file());
    assert!(store.history(&HistoryQuery::default())?.receipts.is_empty());
    Ok(())
}

#[test]
fn publication_failure_cannot_change_a_committed_build() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let worktree = temporary.path().join("worktree");
    fs::create_dir(&worktree)?;
    let context = test_support::context(&worktree)?;
    let invocation = test_support::invocation(&worktree)?;
    let lease = store.lease(&context, &invocation)?;
    let receipts = store.info().root.join("history/receipts");
    fs::remove_dir(&receipts)?;
    fs::write(&receipts, b"blocks receipt publication")?;

    let finalization = lease.finish(BuildOutcome::Succeeded)?;

    assert_eq!(finalization.history.len(), 1);
    assert_eq!(finalization.history[0].warnings.len(), 1);
    assert_eq!(
        finalization.history[0].warnings[0].event,
        super::HistoryWarningEvent::PersistFailed
    );
    let manifest: ArenaManifest = read_json(&store.layout.manifest(context.arena_id()))?;
    assert_eq!(manifest.last_outcome, Some(BuildOutcome::Succeeded));
    Ok(())
}

#[test]
fn concurrent_publication_retains_every_unique_receipt() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let worktree = temporary.path().join("worktree");
    fs::create_dir(&worktree)?;
    let policy = HistoryPolicy::default();
    store.set_history_policy(HistoryPolicy {
        enabled: false,
        ..policy
    })?;
    test_support::create_idle_arena(&store, &worktree, 64)?;
    store.set_history_policy(policy)?;
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let mut workers = Vec::new();
    for _ in 0..8 {
        let worker_store = store.clone();
        let worker_barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            worker_barrier.wait();
            worker_store
                .collect(
                    CollectionPolicy::new(ByteSize::from_bytes(1_000_000)),
                    false,
                )
                .is_ok_and(|report| report.history.receipt_id.is_some())
        }));
    }
    for worker in workers {
        assert!(worker.join().is_ok_and(|published| published));
    }
    let report = store.history(&HistoryQuery {
        kind: Some(HistoryKind::Collection),
        limit: 20,
        ..HistoryQuery::default()
    })?;
    assert_eq!(report.receipts.len(), 8);
    let identities = report
        .receipts
        .iter()
        .map(|receipt| receipt.receipt_id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(identities.len(), 8);
    Ok(())
}
