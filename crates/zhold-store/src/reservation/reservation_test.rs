use std::fs;

use tempfile::tempdir;
use zhold_core::{ArenaState, BuildOutcome, ByteSize, CommandDescriptor};

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

    let mut lease = store.lease(&context, &invocation)?;
    test_support::mark_spawned(&mut lease)?;
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
        store
            .record_reservation_growth(invocation.command_class(), ByteSize::from_bytes(growth))?;
    }
    store.record_reservation_growth(invocation.command_class(), ByteSize::from_bytes(1))?;
    assert_eq!(
        store.recommended_reservation(&invocation, ByteSize::ZERO)?,
        ByteSize::from_bytes(19)
    );
    store.record_reservation_growth(invocation.command_class(), ByteSize::from_bytes(40))?;
    assert_eq!(
        store.recommended_reservation(&invocation, ByteSize::from_bytes(30))?,
        ByteSize::from_bytes(40)
    );
    Ok(())
}

#[test]
fn backup_only_profile_preserves_learned_reservations() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let invocation = test_support::invocation(temporary.path())?;
    for growth in 1..=20 {
        store
            .record_reservation_growth(invocation.command_class(), ByteSize::from_bytes(growth))?;
    }
    crate::io::json_recovery_test::rotate_to_backup(&store.layout.reservation_profile())?;

    let learned = store.recommended_reservation(&invocation, ByteSize::from_bytes(1))?;
    assert_eq!(learned, ByteSize::from_bytes(20));
    store.record_reservation_growth(invocation.command_class(), ByteSize::from_bytes(40))?;
    assert_eq!(
        store.recommended_reservation(&invocation, ByteSize::from_bytes(1))?,
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
    let mut lease = store.lease(&context, &invocation)?;
    test_support::mark_spawned(&mut lease)?;
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

#[test]
fn a_command_rejected_before_spawn_does_not_train_reservations()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let context = test_support::context(project.path())?;
    let invocation = test_support::invocation(project.path())?;

    store.lease(&context, &invocation)?.finish_not_started()?;

    let manifest: ArenaManifest = read_json(&store.layout.manifest(context.arena_id()))?;
    assert_eq!(manifest.last_outcome, Some(BuildOutcome::NotStarted));
    assert!(!store.layout.reservation_profile().exists());
    Ok(())
}

#[test]
fn a_spawned_command_cannot_be_finalized_as_not_started() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let context = test_support::context(project.path())?;
    let invocation = test_support::invocation(project.path())?;
    let mut lease = store.lease(&context, &invocation)?;
    test_support::mark_spawned(&mut lease)?;

    assert!(matches!(
        lease.finish_not_started(),
        Err(crate::StoreError::InvalidLifecycleTransition { .. })
    ));

    let manifest: ArenaManifest = read_json(&store.layout.manifest(context.arena_id()))?;
    assert_eq!(manifest.last_outcome, None);
    assert_eq!(
        store.inventory()?.arenas[0].record.state(),
        ArenaState::Suspect
    );
    assert!(!store.layout.reservation_profile().exists());
    Ok(())
}

#[test]
fn absent_final_measurement_preserves_the_durable_size() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let (context, _) = test_support::create_idle_arena(&store, project.path(), 4_096)?;
    let mut manifest: ArenaManifest = read_json(&store.layout.manifest(context.arena_id()))?;
    let durable = manifest.last_known_size;

    manifest.begin(
        &context,
        CommandDescriptor::default(),
        ByteSize::ZERO,
        manifest.last_used_at.saturating_add(1),
    );
    manifest.mark_spawning()?;
    manifest.mark_spawned()?;
    manifest.finish(
        BuildOutcome::Terminated,
        durable.unwrap_or_default(),
        None,
        manifest.last_used_at.saturating_add(1),
    )?;

    assert_eq!(manifest.last_known_size, durable);
    Ok(())
}
