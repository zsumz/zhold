use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub(crate) const STORE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct StoreMarker {
    pub(crate) schema_version: u32,
    pub(crate) store_id: Uuid,
}

impl StoreMarker {
    pub(crate) fn create() -> Self {
        Self {
            schema_version: STORE_SCHEMA_VERSION,
            store_id: Uuid::new_v4(),
        }
    }
}
