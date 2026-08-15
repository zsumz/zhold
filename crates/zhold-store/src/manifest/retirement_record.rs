use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::{ArenaId, ByteSize};

use crate::{Store, StoreError};

const RETIREMENT_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct RetirementRecord {
    schema_version: u32,
    store_id: Uuid,
    arena_id: ArenaId,
    retirement_id: Uuid,
    original_path: PathBuf,
    trash_path: PathBuf,
    retired_revision: u64,
    #[serde(default)]
    retired_size: ByteSize,
}

impl RetirementRecord {
    pub(crate) fn create(
        store: &Store,
        arena_id: ArenaId,
        retirement_id: Uuid,
        retired_revision: u64,
        retired_size: ByteSize,
    ) -> Self {
        Self {
            schema_version: RETIREMENT_SCHEMA_VERSION,
            store_id: store.marker.store_id,
            original_path: store.layout.arena(&arena_id),
            trash_path: store.layout.trash_destination(&arena_id, retirement_id),
            arena_id,
            retirement_id,
            retired_revision,
            retired_size,
        }
    }

    pub(crate) fn validate(
        &self,
        store: &Store,
        arena_id: &ArenaId,
        retirement_id: Uuid,
    ) -> Result<(), StoreError> {
        let valid = (1..=RETIREMENT_SCHEMA_VERSION).contains(&self.schema_version)
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

    pub(crate) fn validate_journal(
        &self,
        store: &Store,
        record_path: &std::path::Path,
    ) -> Result<(), StoreError> {
        self.validate(store, &self.arena_id, self.retirement_id)?;
        if record_path == store.layout.retirement_record(self.retirement_id) {
            Ok(())
        } else {
            Err(StoreError::InvalidOwnership {
                path: record_path.to_path_buf(),
                reason: "retirement journal path does not match its nonce".to_owned(),
            })
        }
    }

    pub(crate) const fn retired_size(&self) -> ByteSize {
        self.retired_size
    }

    pub(crate) fn arena_id(&self) -> &ArenaId {
        &self.arena_id
    }

    pub(crate) const fn retirement_id(&self) -> Uuid {
        self.retirement_id
    }

    pub(crate) const fn retired_revision(&self) -> u64 {
        self.retired_revision
    }

    pub(crate) fn original_path(&self) -> &std::path::Path {
        &self.original_path
    }

    pub(crate) fn trash_path(&self) -> &std::path::Path {
        &self.trash_path
    }
}
