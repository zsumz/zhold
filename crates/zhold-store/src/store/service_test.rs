use std::{io, sync::mpsc, thread, time::Duration};

use tempfile::tempdir;
use zhold_core::BuildOutcome;

use crate::{Store, lock::ExclusiveFileLock, test_support};

#[test]
fn read_only_open_does_not_create_or_repair_store_state() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("store");
    let store = Store::open_read_write(&root)?;
    let info = store.info();
    drop(store);

    let opened = Store::open_read_only(&root)?;
    assert_eq!(opened.info(), info);
    drop(opened);

    std::fs::remove_dir(root.join("integrations/worktrees"))?;
    let opened = Store::open_read_only(&root)?;
    assert!(!root.join("integrations/worktrees").exists());
    drop(opened);

    let missing = temporary.path().join("missing");
    assert!(Store::open_read_only(&missing).is_err());
    assert!(!missing.exists());
    Ok(())
}

#[test]
fn read_only_inventory_never_creates_a_missing_lock_file() -> Result<(), Box<dyn std::error::Error>>
{
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open_read_write(store_root.path())?;
    let (context, _) = test_support::create_idle_arena(&store, project.path(), 4_096)?;
    let lock = store.layout.arena_lock(context.arena_id());
    std::fs::remove_file(&lock)?;
    drop(store);

    let store = Store::open_read_only(store_root.path())?;
    assert_eq!(store.inventory_cached()?.arenas.len(), 1);
    assert!(!lock.exists());
    Ok(())
}

#[test]
fn contended_admission_does_not_hold_the_collection_lock() -> Result<(), Box<dyn std::error::Error>>
{
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let context = test_support::context(project.path())?;
    let invocation = test_support::invocation(project.path())?;
    let mut first = store.lease(&context, &invocation)?;
    test_support::mark_spawned(&mut first)?;
    let contender_store = store.clone();
    let contender_context = context.clone();
    let contender_invocation = invocation.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let contender = thread::spawn(move || {
        let _ignored = started_tx.send(());
        let lease = contender_store.lease(&contender_context, &contender_invocation)?;
        test_support::finish_succeeded(lease)
    });
    started_rx.recv_timeout(Duration::from_secs(1))?;
    thread::sleep(Duration::from_millis(50));

    let collection_available =
        ExclusiveFileLock::try_acquire(&store.layout.collection_lock())?.is_some();

    first.finish(BuildOutcome::Succeeded)?;
    contender
        .join()
        .map_err(|_| io::Error::other("contended admission thread panicked"))??;
    assert!(collection_available);
    Ok(())
}
