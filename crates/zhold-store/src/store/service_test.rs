use std::{io, sync::mpsc, thread, time::Duration};

use tempfile::tempdir;
use zhold_core::BuildOutcome;

use crate::{Store, lock::ExclusiveFileLock, test_support};

#[test]
fn contended_admission_does_not_hold_the_collection_lock() -> Result<(), Box<dyn std::error::Error>>
{
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let context = test_support::context(project.path())?;
    let invocation = test_support::invocation(project.path())?;
    let first = store.lease(&context, &invocation)?;
    let contender_store = store.clone();
    let contender_context = context.clone();
    let contender_invocation = invocation.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let contender = thread::spawn(move || {
        let _ignored = started_tx.send(());
        let lease = contender_store.lease(&contender_context, &contender_invocation)?;
        lease.finish(BuildOutcome::Succeeded)
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
