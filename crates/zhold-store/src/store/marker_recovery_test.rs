use std::fs;

use tempfile::tempdir;

use super::Store;
use crate::{
    io::{json_recovery_test::rotate_to_backup, read_json},
    manifest::StoreMarker,
};

#[test]
fn backup_only_store_marker_opens_read_only_and_read_write()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path())?;
    let store_id = store.info().store_id;
    rotate_to_backup(&store.layout.marker())?;
    drop(store);

    assert_eq!(
        Store::open_read_only(temporary.path())?.info().store_id,
        store_id
    );
    assert_eq!(
        Store::open_read_write(temporary.path())?.info().store_id,
        store_id
    );
    Ok(())
}

#[test]
fn backup_only_version_one_marker_is_upgraded_without_changing_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store_id = uuid::Uuid::new_v4();
    let marker = temporary.path().join("store.json");
    fs::write(
        &marker,
        format!("{{\"schema_version\":1,\"store_id\":\"{store_id}\"}}"),
    )?;
    rotate_to_backup(&marker)?;

    let store = Store::open_read_write(temporary.path())?;
    let upgraded: StoreMarker = read_json(&store.layout.marker())?;

    assert_eq!(store.info().store_id, store_id);
    assert_eq!(
        upgraded.schema_version,
        crate::manifest::STORE_SCHEMA_VERSION
    );
    assert_ne!(upgraded.fingerprint_key(), &[0; 32]);
    assert!(!marker.with_extension("json.bak").exists());
    Ok(())
}
