use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::ArenaId;

use crate::{Store, StoreError};

const RETIREMENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct RetirementRecord {
    schema_version: u32,
    store_id: Uuid,
    arena_id: ArenaId,
    retirement_id: Uuid,
    original_path: PathBuf,
    trash_path: PathBuf,
    retired_revision: u64,
}

impl RetirementRecord {
    pub(crate) fn create(
        store: &Store,
        arena_id: ArenaId,
        retirement_id: Uuid,
        retired_revision: u64,
    ) -> Self {
        Self {
            schema_version: RETIREMENT_SCHEMA_VERSION,
            store_id: store.marker.store_id,
            original_path: store.layout.arena(&arena_id),
            trash_path: store.layout.trash_destination(&arena_id, retirement_id),
            arena_id,
            retirement_id,
            retired_revision,
        }
    }

    pub(crate) fn validate(
        &self,
        store: &Store,
        arena_id: &ArenaId,
        retirement_id: Uuid,
    ) -> Result<(), StoreError> {
        let valid = self.schema_version == RETIREMENT_SCHEMA_VERSION
            && self.store_id == store.marker.store_id
            && &self.arena_id == arena_id
            && self.retirement_id == retirement_id
            && self.original_path == store.layout.arena(arena_id)
            && self.trash_path == store.layout.trash_destination(arena_id, retirement_id)
            && self.retired_revision > 0;
        if valid {
            Ok(())
        } else {
            Err(StoreError::InvalidOwnership {
                path: store.layout.retirement_record(retirement_id),
                reason: "retirement journal does not match the marked store and trash path"
                    .to_owned(),
            })
        }
    }
}
