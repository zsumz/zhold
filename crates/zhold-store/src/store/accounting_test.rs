use std::fs;

use tempfile::tempdir;
use zhold_core::{BuildOutcome, ByteSize, CollectionPolicy};

use crate::{CargoInvocation, Store, StoreError, test_support};

#[test]
fn lease_is_authoritative_for_inventory_and_collection() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = test_support::context(project.path())?;
    let invocation = test_support::invocation(project.path())?;
    let lease = store.lease(&context, &invocation)?;
    fs::write(lease.build_dir().join("artifact.rlib"), vec![0_u8; 4_096])?;

    let active = store.inventory()?;
    assert_eq!(active.arenas.len(), 1);
    assert_eq!(
        active.arenas[0].record.liveness,
        zhold_core::ArenaLiveness::Active
    );

    let blocked = store.collect(CollectionPolicy::new(ByteSize::from_bytes(1)), false)?;
    assert!(blocked.retirements.is_empty());
    assert!(!blocked.budget_met);

    lease.finish(BuildOutcome::Succeeded)?;
    let retired = store.collect(CollectionPolicy::new(ByteSize::from_bytes(1)), false)?;
    assert_eq!(retired.retirements.len(), 1);
    assert!(store.inventory()?.arenas.is_empty());
    Ok(())
}

#[test]
fn live_reservations_are_admission_accounting_and_clear_on_finish()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = test_support::context(project.path())?;
    let invocation = test_support::invocation(project.path())?;
    let reservation = ByteSize::from_bytes(4_096);
    let (lease, report) = store.lease_reserved_and_collect(
        &context,
        &invocation,
        reservation,
        CollectionPolicy::new(ByteSize::ZERO),
    )?;
    let active = store.inventory()?;

    assert_eq!(active.reserved, reservation);
    assert_eq!(report.reserved, reservation);
    assert!(!report.budget_met);

    let peak = ByteSize::from_bytes(8_192);
    lease.finish_with_peak(BuildOutcome::Succeeded, peak)?;
    let finished = store.inventory()?;
    assert_eq!(finished.reserved, ByteSize::ZERO);
    assert_eq!(finished.arenas[0].last_peak, peak);
    Ok(())
}

#[test]
fn raw_cargo_arguments_are_never_persisted_or_exposed() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = test_support::context(project.path())?;
    let secret = "registries.private.token='extremely-secret-token'";
    let invocation = CargoInvocation::new(
        "cargo".to_owned(),
        vec!["--config".to_owned(), secret.to_owned(), "test".to_owned()],
        project.path().to_path_buf(),
    )?;

    store
        .lease(&context, &invocation)?
        .finish(BuildOutcome::Succeeded)?;

    let manifest = fs::read_to_string(store.layout.manifest(context.arena_id()))?;
    let inventory = serde_json::to_string(&store.inventory()?)?;
    assert!(!manifest.contains(secret));
    assert!(!inventory.contains(secret));
    assert!(manifest.contains("arguments_fingerprint"));
    assert!(inventory.contains("arguments_fingerprint"));
    Ok(())
}

#[test]
fn unreadable_owned_metadata_blocks_collection() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(temporary.path())?;
    let (context, _invocation) = test_support::create_idle_arena(&store, project.path(), 4_096)?;
    fs::write(store.layout.manifest(context.arena_id()), b"corrupt")?;

    let inventory = store.inventory()?;
    assert_eq!(inventory.uncertain_owned, 1);
    assert_eq!(inventory.arenas.len(), 0);
    assert!(matches!(
        store.collect(CollectionPolicy::new(ByteSize::from_bytes(1)), false),
        Err(StoreError::InventoryUncertain { count: 1 })
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn active_reservation_survives_build_directory_measurement_failure()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let temporary = tempdir()?;
    let project = tempdir()?;
    let outside = tempdir()?;
    let store = Store::open(temporary.path())?;
    let context = test_support::context(project.path())?;
    let invocation = test_support::invocation(project.path())?;
    let reservation = ByteSize::from_bytes(8_192);
    let lease = store.lease_reserved(&context, &invocation, reservation)?;
    let build = lease.build_dir().to_path_buf();
    let moved = outside.path().join("build");
    fs::rename(&build, &moved)?;
    symlink(&moved, &build)?;

    let inventory = store.inventory()?;

    assert_eq!(inventory.uncertain_owned, 1);
    assert_eq!(inventory.reserved, reservation);
    drop(lease);
    Ok(())
}
