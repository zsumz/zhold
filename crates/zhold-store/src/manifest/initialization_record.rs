use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::ArenaId;

use crate::{Store, StoreError};

const INITIALIZATION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct InitializationRecord {
    schema_version: u32,
    store_id: Uuid,
    arena_id: ArenaId,
    initialization_id: Uuid,
    staging_path: PathBuf,
    final_path: PathBuf,
}

impl InitializationRecord {
    pub(crate) fn create(store: &Store, arena_id: ArenaId, initialization_id: Uuid) -> Self {
        Self {
            schema_version: INITIALIZATION_SCHEMA_VERSION,
            store_id: store.marker.store_id,
            staging_path: store.layout.arena_staging(&arena_id, initialization_id),
            final_path: store.layout.arena(&arena_id),
            arena_id,
            initialization_id,
        }
    }

    pub(crate) fn validate(&self, store: &Store, path: &Path) -> Result<(), StoreError> {
        let valid = self.schema_version == INITIALIZATION_SCHEMA_VERSION
            && self.store_id == store.marker.store_id
            && self.staging_path
                == store
                    .layout
                    .arena_staging(&self.arena_id, self.initialization_id)
            && self.final_path == store.layout.arena(&self.arena_id)
            && path == store.layout.initialization_record(self.initialization_id);
        if valid {
            Ok(())
        } else {
            Err(StoreError::InvalidOwnership {
                path: path.to_path_buf(),
                reason:
                    "arena initialization journal does not match its store, identity, and nonce"
                        .to_owned(),
            })
        }
    }

    pub(crate) fn arena_id(&self) -> &ArenaId {
        &self.arena_id
    }

    pub(crate) const fn initialization_id(&self) -> Uuid {
        self.initialization_id
    }

    pub(crate) fn staging_path(&self) -> &Path {
        &self.staging_path
    }

    pub(crate) fn final_path(&self) -> &Path {
        &self.final_path
    }
}
