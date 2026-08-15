use tempfile::tempdir;
use zhold_core::ByteSize;

use super::StoreConfig;
use crate::Store;

#[test]
fn store_configuration_is_durable_and_store_scoped() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let root = temporary.path().join("store");
    let store = Store::open(&root)?;
    let config = StoreConfig {
        arena_budget: Some(ByteSize::from_bytes(200)),
        min_filesystem_free: Some(ByteSize::from_bytes(25)),
        minimum_build_reservation: Some(ByteSize::from_bytes(10)),
    };

    store.set_config(config)?;

    assert_eq!(Store::open(&root)?.config()?, config);
    Ok(())
}

#[test]
fn zero_arena_budget_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;

    let result = store.set_config(StoreConfig {
        arena_budget: Some(ByteSize::ZERO),
        ..StoreConfig::default()
    });

    assert!(result.is_err());
    assert_eq!(store.config()?, StoreConfig::default());
    Ok(())
}
