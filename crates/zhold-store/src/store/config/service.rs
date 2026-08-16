use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::ByteSize;

use super::StoreConfig;
use crate::{
    Store, StoreError,
    io::{read_optional_json, upsert_json},
    lock::ExclusiveFileLock,
};

const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ConfigDocument {
    schema_version: u32,
    store_id: Uuid,
    config: StoreConfig,
}

impl Store {
    /// Reads durable store defaults without applying environment overrides.
    pub fn config(&self) -> Result<StoreConfig, StoreError> {
        if self.read_only {
            return read_config(self);
        }
        let _lock = ExclusiveFileLock::acquire(&self.layout.config_lock())?;
        read_config(self)
    }

    /// Atomically replaces durable store defaults.
    pub fn set_config(&self, config: StoreConfig) -> Result<(), StoreError> {
        self.ensure_writable("replace store configuration")?;
        validate_config(config)?;
        let _lock = ExclusiveFileLock::acquire(&self.layout.config_lock())?;
        persist_config(self, config)
    }

    /// Patches only explicitly supplied durable defaults and preserves all others.
    pub fn patch_config(&self, patch: StoreConfig) -> Result<StoreConfig, StoreError> {
        self.ensure_writable("patch store configuration")?;
        let _lock = ExclusiveFileLock::acquire(&self.layout.config_lock())?;
        let current = read_config(self)?;
        let merged = StoreConfig {
            arena_budget: patch.arena_budget.or(current.arena_budget),
            min_filesystem_free: patch.min_filesystem_free.or(current.min_filesystem_free),
            minimum_build_reservation: patch
                .minimum_build_reservation
                .or(current.minimum_build_reservation),
        };
        validate_config(merged)?;
        persist_config(self, merged)?;
        Ok(merged)
    }
}

fn persist_config(store: &Store, config: StoreConfig) -> Result<(), StoreError> {
    let document = ConfigDocument {
        schema_version: SCHEMA_VERSION,
        store_id: store.marker.store_id,
        config,
    };
    let path = store.layout.config();
    upsert_json(&path, &document)
}

fn read_config(store: &Store) -> Result<StoreConfig, StoreError> {
    let path = store.layout.config();
    match read_optional_json::<ConfigDocument>(&path)? {
        Some(document) => {
            if document.schema_version != SCHEMA_VERSION
                || document.store_id != store.marker.store_id
            {
                return Err(StoreError::InvalidConfiguration(
                    "document schema or store identity does not match".to_owned(),
                ));
            }
            validate_config(document.config)?;
            Ok(document.config)
        }
        None => Ok(StoreConfig::default()),
    }
}

fn validate_config(config: StoreConfig) -> Result<(), StoreError> {
    if config.arena_budget == Some(ByteSize::ZERO) {
        return Err(StoreError::InvalidConfiguration(
            "arena budget must be greater than zero".to_owned(),
        ));
    }
    Ok(())
}
