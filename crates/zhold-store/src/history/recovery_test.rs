use std::fs;

use tempfile::tempdir;
use zhold_core::{ByteSize, CollectionPolicy, HistoryPolicy};

use crate::{Store, io::json_recovery_test::rotate_to_backup, test_support};

#[test]
fn backup_only_policy_preserves_disabled_state_and_partial_updates()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let policy = HistoryPolicy {
        enabled: false,
        max_receipts: 37,
        max_bytes: ByteSize::from_bytes(12_345),
    };
    store.set_history_policy(policy)?;
    rotate_to_backup(&store.layout.history_policy())?;

    let recovered = store.history_policy()?;
    assert_eq!(recovered, policy);
    let patched = HistoryPolicy {
        max_receipts: 41,
        ..recovered
    };
    store.set_history_policy(patched)?;

    assert_eq!(store.history_policy()?, patched);
    assert!(!patched.enabled);
    assert_eq!(patched.max_bytes, policy.max_bytes);
    Ok(())
}

#[test]
fn backup_only_history_index_remains_publishable() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let worktree = temporary.path().join("worktree");
    fs::create_dir(&worktree)?;
    test_support::create_idle_arena(&store, &worktree, 64)?;
    let before = super::index::read(&store)?.receipt_count();
    rotate_to_backup(&store.layout.history_index())?;

    let report = store.collect(
        CollectionPolicy::new(ByteSize::from_bytes(1_000_000)),
        false,
    )?;
    let after = super::index::read(&store)?;

    assert!(report.history.warnings.is_empty());
    assert!(after.is_clean());
    assert_eq!(after.receipt_count(), before.saturating_add(1));
    Ok(())
}
