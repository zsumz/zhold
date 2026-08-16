use std::{collections::BTreeSet, fs};

use zhold_core::ByteSize;

use super::{
    HistoryPruneReport, HistoryPruneRequest,
    index::publish_totals,
    reader::{ValidatedReceipt, history_policy, read_receipt, read_receipts, receipt_order},
};
use crate::{Store, StoreError, lock::ExclusiveFileLock};

pub(crate) fn prune(
    store: &Store,
    request: HistoryPruneRequest,
) -> Result<HistoryPruneReport, StoreError> {
    if request.dry_run {
        return prune_locked(store, request);
    }
    store.ensure_writable("prune history")?;
    let _lock = ExclusiveFileLock::acquire(&store.layout.history_lock())?;
    prune_locked(store, request)
}

pub(crate) fn prune_locked(
    store: &Store,
    request: HistoryPruneRequest,
) -> Result<HistoryPruneReport, StoreError> {
    let policy = history_policy(store)?;
    let (mut valid, findings) = read_receipts(store)?;
    valid.sort_by(receipt_order);
    let before_count = count(&valid);
    let before_bytes = bytes(&valid);
    let keep = request.keep.unwrap_or(policy.max_receipts);
    let max_bytes = request.max_bytes.unwrap_or(policy.max_bytes);
    let removal = removal_plan(&valid, keep, max_bytes, request.older_than);
    let mut removed_bytes = ByteSize::ZERO;
    if request.dry_run {
        for index in &removal {
            removed_bytes = removed_bytes.saturating_add(valid[*index].bytes);
        }
    } else {
        for index in &removal {
            let item = &valid[*index];
            prove_and_remove(store, item)?;
            removed_bytes = removed_bytes.saturating_add(item.bytes);
        }
        sync_receipt_directory(store)?;
        publish_totals(
            store,
            before_count.saturating_sub(u64::try_from(removal.len()).unwrap_or(u64::MAX)),
            before_bytes.saturating_sub(removed_bytes),
        )?;
    }
    let removed_count = u64::try_from(removal.len()).unwrap_or(u64::MAX);
    Ok(HistoryPruneReport {
        dry_run: request.dry_run,
        before_count,
        before_bytes,
        removed_count,
        removed_bytes,
        after_count: before_count.saturating_sub(removed_count),
        after_bytes: before_bytes.saturating_sub(removed_bytes),
        findings,
    })
}

fn removal_plan(
    valid: &[ValidatedReceipt],
    keep: u64,
    max_bytes: ByteSize,
    older_than: Option<u64>,
) -> BTreeSet<usize> {
    let mut removal = BTreeSet::new();
    let excess = count(valid).saturating_sub(keep);
    for index in 0..usize::try_from(excess)
        .unwrap_or(usize::MAX)
        .min(valid.len())
    {
        removal.insert(index);
    }
    if let Some(cutoff) = older_than {
        for (index, item) in valid.iter().enumerate() {
            if item.receipt.recorded_at < cutoff {
                removal.insert(index);
            }
        }
    }
    let mut remaining_bytes = valid
        .iter()
        .enumerate()
        .filter(|(index, _)| !removal.contains(index))
        .fold(ByteSize::ZERO, |total, (_, item)| {
            total.saturating_add(item.bytes)
        });
    let mut remaining_count = valid.len().saturating_sub(removal.len());
    for (index, item) in valid.iter().enumerate() {
        if remaining_bytes <= max_bytes || remaining_count <= 1 {
            break;
        }
        if removal.insert(index) {
            remaining_bytes = remaining_bytes.saturating_sub(item.bytes);
            remaining_count = remaining_count.saturating_sub(1);
        }
    }
    removal
}

fn prove_and_remove(store: &Store, expected: &ValidatedReceipt) -> Result<(), StoreError> {
    let current = read_receipt(store, &expected.path)?;
    if current.receipt != expected.receipt || current.bytes != expected.bytes {
        return Err(StoreError::InvalidOwnership {
            path: expected.path.clone(),
            reason: "history receipt changed after retention planning".to_owned(),
        });
    }
    fs::remove_file(&expected.path)
        .map_err(|error| StoreError::io("remove validated history receipt", &expected.path, error))
}

fn count(valid: &[ValidatedReceipt]) -> u64 {
    u64::try_from(valid.len()).unwrap_or(u64::MAX)
}

fn bytes(valid: &[ValidatedReceipt]) -> ByteSize {
    valid.iter().fold(ByteSize::ZERO, |total, item| {
        total.saturating_add(item.bytes)
    })
}

#[cfg(unix)]
fn sync_receipt_directory(store: &Store) -> Result<(), StoreError> {
    let path = store.layout.history_receipts();
    fs::File::open(&path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| StoreError::io("sync history receipt directory", path, error))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn sync_receipt_directory(_store: &Store) -> Result<(), StoreError> {
    Ok(())
}
