use std::fs;

use tempfile::tempdir;
use zhold_core::{BuildOutcome, ByteSize};

use crate::{CargoInvocation, Store, io::read_json, manifest::ArenaManifest, test_support};

#[test]
fn completed_growth_increases_the_next_command_reservation()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let worktree = temporary.path().join("worktree");
    fs::create_dir(&worktree)?;
    let context = test_support::context(&worktree)?;
    let invocation = CargoInvocation::new("cargo".to_owned(), vec!["check".to_owned()], worktree)?;
    assert_eq!(
        store.recommended_reservation(&invocation, ByteSize::from_bytes(1))?,
        ByteSize::from_bytes(1)
    );

    let lease = store.lease(&context, &invocation)?;
    fs::write(lease.build_dir().join("growth"), vec![1_u8; 512])?;
    let peak = lease.measure()?;
    let _finalization = lease.finish_observed(BuildOutcome::Succeeded, peak)?;

    assert!(
        store.recommended_reservation(&invocation, ByteSize::ZERO)? >= ByteSize::from_bytes(512)
    );
    Ok(())
}

#[test]
fn profile_uses_p95_and_previous_growth() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let invocation = test_support::invocation(temporary.path())?;
    for growth in 1..=20 {
        store.record_reservation_growth(
            invocation.descriptor().command_class,
            ByteSize::from_bytes(growth),
        )?;
    }
    store.record_reservation_growth(
        invocation.descriptor().command_class,
        ByteSize::from_bytes(1),
    )?;
    assert_eq!(
        store.recommended_reservation(&invocation, ByteSize::ZERO)?,
        ByteSize::from_bytes(19)
    );
    store.record_reservation_growth(
        invocation.descriptor().command_class,
        ByteSize::from_bytes(40),
    )?;
    assert_eq!(
        store.recommended_reservation(&invocation, ByteSize::from_bytes(30))?,
        ByteSize::from_bytes(40)
    );
    Ok(())
}

#[test]
fn advisory_learning_failure_preserves_the_committed_outcome()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let context = test_support::context(project.path())?;
    let invocation = test_support::invocation(project.path())?;
    let lease = store.lease(&context, &invocation)?;
    fs::create_dir(store.layout.reservation_profile())?;

    let finalization = lease.finish(BuildOutcome::Succeeded)?;
    let manifest: ArenaManifest = read_json(&store.layout.manifest(context.arena_id()))?;

    assert_eq!(manifest.last_outcome, Some(BuildOutcome::Succeeded));
    assert!(manifest.last_finished_at.is_some());
    assert_eq!(finalization.warnings.len(), 1);
    assert_eq!(
        finalization.warnings[0].event,
        super::super::FinalizationWarningEvent::ReservationLearningFailed
    );
    Ok(())
}
