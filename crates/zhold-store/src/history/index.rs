use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::ByteSize;

use super::reader::read_receipts;
use crate::{
    Store, StoreError,
    io::{read_optional_json, upsert_json},
};

#[cfg(test)]
use crate::io::read_json;

const INDEX_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryIndex {
    schema_version: u32,
    store_id: Uuid,
    clean: bool,
    receipt_count: u64,
    receipt_bytes: ByteSize,
}

impl HistoryIndex {
    pub(crate) fn projected(&self, additional_bytes: ByteSize) -> Self {
        Self {
            schema_version: INDEX_SCHEMA_VERSION,
            store_id: self.store_id,
            clean: true,
            receipt_count: self.receipt_count.saturating_add(1),
            receipt_bytes: self.receipt_bytes.saturating_add(additional_bytes),
        }
    }

    pub(crate) fn fits(&self, count: u64, bytes: ByteSize) -> bool {
        self.receipt_count <= count && self.receipt_bytes <= bytes
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.clean = false;
    }

    #[cfg(test)]
    pub(crate) const fn is_clean(&self) -> bool {
        self.clean
    }

    #[cfg(test)]
    pub(crate) const fn receipt_count(&self) -> u64 {
        self.receipt_count
    }
}

pub(crate) fn begin_publication(store: &Store) -> Result<HistoryIndex, StoreError> {
    let mut index = read_or_rebuild(store)?;
    index.mark_dirty();
    publish(store, &index)?;
    Ok(index)
}

pub(crate) fn publish_projection(
    store: &Store,
    index: &HistoryIndex,
    additional_bytes: ByteSize,
) -> Result<HistoryIndex, StoreError> {
    let projected = index.projected(additional_bytes);
    publish(store, &projected)?;
    Ok(projected)
}

pub(crate) fn publish_totals(
    store: &Store,
    receipt_count: u64,
    receipt_bytes: ByteSize,
) -> Result<(), StoreError> {
    publish(
        store,
        &HistoryIndex {
            schema_version: INDEX_SCHEMA_VERSION,
            store_id: store.marker.store_id,
            clean: true,
            receipt_count,
            receipt_bytes,
        },
    )
}

#[cfg(test)]
pub(crate) fn read(store: &Store) -> Result<HistoryIndex, StoreError> {
    read_json(&store.layout.history_index())
}

fn read_or_rebuild(store: &Store) -> Result<HistoryIndex, StoreError> {
    let path = store.layout.history_index();
    match read_optional_json::<HistoryIndex>(&path)? {
        Some(index) => {
            if index.schema_version != INDEX_SCHEMA_VERSION
                || index.store_id != store.marker.store_id
            {
                return Err(StoreError::InvalidOwnership {
                    path,
                    reason: "history index does not match its store or schema".to_owned(),
                });
            }
            if index.clean {
                Ok(index)
            } else {
                rebuild(store)
            }
        }
        None => rebuild(store),
    }
}

fn rebuild(store: &Store) -> Result<HistoryIndex, StoreError> {
    let (receipts, _findings) = read_receipts(store)?;
    let receipt_count = u64::try_from(receipts.len()).unwrap_or(u64::MAX);
    let receipt_bytes = receipts.iter().fold(ByteSize::ZERO, |total, receipt| {
        total.saturating_add(receipt.bytes)
    });
    let index = HistoryIndex {
        schema_version: INDEX_SCHEMA_VERSION,
        store_id: store.marker.store_id,
        clean: true,
        receipt_count,
        receipt_bytes,
    };
    publish(store, &index)?;
    Ok(index)
}

fn publish(store: &Store, index: &HistoryIndex) -> Result<(), StoreError> {
    upsert_json(&store.layout.history_index(), index)
}
