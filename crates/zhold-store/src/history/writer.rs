use uuid::Uuid;

use super::{
    HistoryDraft, HistoryPolicyDocument, HistoryPruneRequest, HistoryReceipt, HistoryWarning,
    HistoryWarningEvent, HistoryWrite,
    index::{begin_publication, publish_projection},
    prune::prune_locked,
    reader::history_policy,
};
use crate::{
    Store, StoreError,
    io::{create_json, upsert_json},
    lock::ExclusiveFileLock,
    time::unix_milliseconds,
};
use zhold_core::HistoryPolicy;

pub(crate) fn persist(store: &Store, draft: HistoryDraft) -> HistoryWrite {
    match persist_inner(store, draft) {
        Ok(result) => result,
        Err(error) => HistoryWrite {
            receipt_id: None,
            warnings: vec![HistoryWarning {
                event: HistoryWarningEvent::PersistFailed,
                message: error.to_string(),
            }],
        },
    }
}

fn persist_inner(store: &Store, draft: HistoryDraft) -> Result<HistoryWrite, StoreError> {
    let _lock = ExclusiveFileLock::acquire(&store.layout.history_lock())?;
    let policy = history_policy(store)?;
    if !policy.enabled {
        return Ok(HistoryWrite::default());
    }
    let index = begin_publication(store)?;
    let recorded_at = unix_milliseconds()?;
    let receipt_id = Uuid::new_v4();
    let receipt = HistoryReceipt {
        schema_version: 1,
        receipt_id,
        store_id: store.marker.store_id,
        recorded_at,
        kind: draft.kind,
        payload: draft.payload,
    };
    let path = store.layout.history_receipt(recorded_at, receipt_id);
    if !create_json(&path, &receipt)? {
        return Err(StoreError::InvalidOwnership {
            path,
            reason: "generated history receipt path already exists".to_owned(),
        });
    }
    let receipt_bytes = zhold_core::ByteSize::from_bytes(
        std::fs::metadata(&path)
            .map_err(|error| StoreError::io("inspect published history receipt", &path, error))?
            .len(),
    );
    let projected = publish_projection(store, &index, receipt_bytes)?;
    let retention = if projected.fits(policy.max_receipts, policy.max_bytes) {
        Ok(())
    } else {
        prune_locked(
            store,
            HistoryPruneRequest {
                keep: Some(policy.max_receipts),
                max_bytes: Some(policy.max_bytes),
                older_than: None,
                dry_run: false,
            },
        )
        .map(|_report| ())
    };
    let retained = retention.is_ok() && path.is_file();
    let warnings = retention.err().map_or_else(Vec::new, |error| {
        vec![HistoryWarning {
            event: HistoryWarningEvent::RetentionFailed,
            message: error.to_string(),
        }]
    });
    Ok(HistoryWrite {
        receipt_id: retained.then_some(receipt_id),
        warnings,
    })
}

pub(crate) fn set_policy(store: &Store, policy: HistoryPolicy) -> Result<(), StoreError> {
    let _lock = ExclusiveFileLock::acquire(&store.layout.history_lock())?;
    let path = store.layout.history_policy();
    let document = HistoryPolicyDocument {
        schema_version: 1,
        store_id: store.marker.store_id,
        policy,
    };
    upsert_json(&path, &document)
}
