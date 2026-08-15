use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub(crate) const STORE_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct StoreMarker {
    pub(crate) schema_version: u32,
    pub(crate) store_id: Uuid,
    #[serde(default)]
    fingerprint_key: [u8; 32],
}

impl StoreMarker {
    pub(crate) fn create() -> Self {
        Self {
            schema_version: STORE_SCHEMA_VERSION,
            store_id: Uuid::new_v4(),
            fingerprint_key: random_fingerprint_key(),
        }
    }

    pub(crate) const fn fingerprint_key(&self) -> &[u8; 32] {
        &self.fingerprint_key
    }

    pub(crate) fn upgrade_fingerprint_key(&mut self) {
        self.schema_version = STORE_SCHEMA_VERSION;
        self.fingerprint_key = random_fingerprint_key();
    }
}

fn random_fingerprint_key() -> [u8; 32] {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut key = [0_u8; 32];
    key[..16].copy_from_slice(first.as_bytes());
    key[16..].copy_from_slice(second.as_bytes());
    key
}
