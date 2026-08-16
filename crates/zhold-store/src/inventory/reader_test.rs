use std::fs;

use crate::{
    Store,
    inventory::{ArenaMeasurement, read_arena_snapshot},
    io::{read_json, write_json},
    manifest::ArenaManifest,
    test_support::create_idle_arena,
};
use tempfile::tempdir;

#[cfg(unix)]
#[test]
fn cached_inventory_does_not_descend_into_build_trees() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let blocked = store.layout.build_dir(context.arena_id()).join("blocked");
    fs::create_dir(&blocked)?;
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000))?;

    let inventory = store.inventory_cached();

    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700))?;
    let inventory = inventory?;
    assert_eq!(inventory.depth, super::InventoryDepth::Cached);
    assert!(inventory.physical.is_none());
    Ok(())
}

#[test]
fn reports_unexpected_arena_prefixes() -> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let store = Store::open(store_root.path())?;
    let unexpected = store.layout.arenas().join("zz");
    fs::create_dir(&unexpected)?;

    let inventory = store.inventory()?;

    assert!(inventory.arenas.is_empty());
    assert_eq!(inventory.findings.len(), 1);
    assert_eq!(inventory.findings[0].path, unexpected);
    assert!(
        inventory.findings[0]
            .reason
            .contains("lowercase hexadecimal")
    );
    Ok(())
}

#[test]
fn rejects_a_manifest_whose_context_does_not_derive_its_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let manifest_path = store.layout.manifest(context.arena_id());
    let mut manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.worktree_root = project.path().join("substituted-worktree");
    write_json(&manifest_path, &manifest)?;

    let inventory = store.inventory()?;

    assert!(inventory.arenas.is_empty());
    assert_eq!(inventory.findings.len(), 1);
    assert!(
        inventory.findings[0]
            .reason
            .contains("compatibility context")
    );
    Ok(())
}

#[test]
fn reads_schema_one_manifests_with_safe_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let manifest_path = store.layout.manifest(context.arena_id());
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| std::io::Error::other("manifest was not an object"))?;
    object.insert("schema_version".to_owned(), serde_json::Value::from(1));
    object.remove("pin_expires_at");
    object.remove("reservation");
    object.remove("last_observed_size");
    object.remove("retirement_id");
    fs::write(&manifest_path, serde_json::to_vec(&value)?)?;

    let inventory = store.inventory()?;

    assert_eq!(inventory.arenas.len(), 1);
    assert_eq!(inventory.arenas[0].reservation, zhold_core::ByteSize::ZERO);
    assert_eq!(
        inventory.arenas[0].last_observed_size,
        zhold_core::ByteSize::ZERO
    );
    assert_eq!(inventory.arenas[0].pin_expires_at, None);
    Ok(())
}

#[test]
fn a_stale_reservation_without_a_live_lease_is_inactive() -> Result<(), Box<dyn std::error::Error>>
{
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let manifest_path = store.layout.manifest(context.arena_id());
    let mut manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.reservation = zhold_core::ByteSize::from_bytes(8_192);
    write_json(&manifest_path, &manifest)?;

    let inventory = store.inventory()?;

    assert_eq!(inventory.reserved, zhold_core::ByteSize::ZERO);
    assert_eq!(inventory.arenas[0].reservation, zhold_core::ByteSize::ZERO);
    Ok(())
}

#[test]
fn distinguishes_cached_admission_sizes_from_deep_measurements()
-> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    create_idle_arena(&store, project.path(), 4_096)?;

    let cached = read_arena_snapshot(&store, ArenaMeasurement::Cached)?;
    let deep = read_arena_snapshot(&store, ArenaMeasurement::Deep)?;

    assert_eq!(cached.arenas.len(), 1);
    assert_eq!(
        cached.arenas[0].record.size_quality,
        zhold_core::SizeQuality::Cached
    );
    assert_eq!(
        deep.arenas[0].record.size_quality,
        zhold_core::SizeQuality::Fresh
    );
    assert_eq!(cached.uncertain_owned, 0);
    assert!(cached.total > zhold_core::ByteSize::ZERO);
    Ok(())
}

#[test]
fn a_known_zero_size_is_not_owned_uncertainty() -> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let project = tempdir()?;
    let store = Store::open(store_root.path())?;
    let (context, _) = create_idle_arena(&store, project.path(), 4_096)?;
    let manifest_path = store.layout.manifest(context.arena_id());
    let mut manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.observe_size(zhold_core::ByteSize::ZERO);
    write_json(&manifest_path, &manifest)?;

    let cached = read_arena_snapshot(&store, ArenaMeasurement::Cached)?;

    assert_eq!(cached.uncertain_owned, 0);
    assert_eq!(cached.total, zhold_core::ByteSize::ZERO);
    assert_eq!(
        cached.arenas[0].record.size_quality,
        zhold_core::SizeQuality::Cached
    );
    Ok(())
}

#[test]
fn cached_inventory_ignores_abandoned_journal_staging() -> Result<(), Box<dyn std::error::Error>> {
    let store_root = tempdir()?;
    let store = Store::open(store_root.path())?;
    let staging = store.layout.trash_index().join(format!(
        "{}.json.{}.new",
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4()
    ));
    fs::write(&staging, b"incomplete")?;

    let inventory = store.inventory_cached()?;

    assert!(inventory.findings.iter().all(|item| item.path != staging));
    assert!(staging.is_file());
    Ok(())
}
