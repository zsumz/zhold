use std::{fs, path::Path, str::FromStr};

use uuid::Uuid;
use zhold_core::{ByteSize, HistoryKind, HistoryPolicy};

use super::{
    HistoryFinding, HistoryPayload, HistoryPolicyDocument, HistoryQuery, HistoryReceipt,
    HistoryReport, HistorySummary,
};
use crate::{
    Store, StoreError,
    io::{is_json_publication_artifact, read_json},
};

#[derive(Clone, Debug)]
pub(crate) struct ValidatedReceipt {
    pub(crate) receipt: HistoryReceipt,
    pub(crate) path: std::path::PathBuf,
    pub(crate) bytes: ByteSize,
}

pub(crate) fn history_policy(store: &Store) -> Result<HistoryPolicy, StoreError> {
    let path = store.layout.history_policy();
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(StoreError::InvalidOwnership {
                path,
                reason: "history policy is not a real file".to_owned(),
            })
        }
        Ok(_) => {
            let document: HistoryPolicyDocument = read_json(&path)?;
            validate_policy(store, &document, &path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HistoryPolicy::default()),
        Err(error) => Err(StoreError::io("inspect history policy", path, error)),
    }
}

pub(crate) fn validate_policy(
    store: &Store,
    document: &HistoryPolicyDocument,
    path: &Path,
) -> Result<HistoryPolicy, StoreError> {
    if document.schema_version != 1 {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: format!(
                "unsupported history policy schema {}",
                document.schema_version
            ),
        });
    }
    if document.store_id != store.marker.store_id {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "history policy belongs to another store".to_owned(),
        });
    }
    Ok(document.policy)
}

pub(crate) fn read_history(
    store: &Store,
    query: &HistoryQuery,
) -> Result<HistoryReport, StoreError> {
    let (policy, mut findings) = effective_policy(store)?;
    let (mut valid, receipt_findings) = read_receipts(store)?;
    findings.extend(receipt_findings);
    valid.sort_by(|left, right| receipt_order(right, left));
    let mut matching = valid
        .into_iter()
        .filter(|item| matches_query(&item.receipt, query));
    let receipts = matching
        .by_ref()
        .take(query.limit)
        .map(|item| item.receipt)
        .collect();
    let more = matching.next().is_some();
    Ok(HistoryReport {
        filters: query.clone(),
        policy,
        receipts,
        findings,
        more,
    })
}

pub(crate) fn summary(store: &Store) -> Result<HistorySummary, StoreError> {
    let (policy, mut findings) = effective_policy(store)?;
    let (mut valid, receipt_findings) = read_receipts(store)?;
    findings.extend(receipt_findings);
    valid.sort_by(receipt_order);
    let receipt_bytes = valid.iter().fold(ByteSize::ZERO, |total, item| {
        total.saturating_add(item.bytes)
    });
    let oversized_newest = valid
        .last()
        .is_some_and(|item| item.bytes > policy.max_bytes);
    Ok(HistorySummary {
        policy,
        receipt_count: u64::try_from(valid.len()).unwrap_or(u64::MAX),
        receipt_bytes,
        finding_count: u64::try_from(findings.len()).unwrap_or(u64::MAX),
        oversized_newest,
    })
}

fn effective_policy(store: &Store) -> Result<(HistoryPolicy, Vec<HistoryFinding>), StoreError> {
    match history_policy(store) {
        Ok(policy) => Ok((policy, Vec::new())),
        Err(error @ (StoreError::InvalidOwnership { .. } | StoreError::Json { .. })) => Ok((
            HistoryPolicy::default(),
            vec![HistoryFinding {
                path: store.layout.history_policy(),
                reason: error.to_string(),
            }],
        )),
        Err(error) => Err(error),
    }
}

pub(crate) fn read_receipts(
    store: &Store,
) -> Result<(Vec<ValidatedReceipt>, Vec<HistoryFinding>), StoreError> {
    let directory = store.layout.history_receipts();
    let entries = fs::read_dir(&directory)
        .map_err(|error| StoreError::io("read history receipts", &directory, error))?;
    let mut valid = Vec::new();
    let mut findings = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| StoreError::io("read history receipt entry", &directory, error))?;
        let path = entry.path();
        if is_json_publication_artifact(&path) {
            continue;
        }
        match read_receipt(store, &path) {
            Ok(item) => valid.push(item),
            Err(error) => findings.push(HistoryFinding {
                path,
                reason: error.to_string(),
            }),
        }
    }
    Ok((valid, findings))
}

pub(crate) fn read_receipt(store: &Store, path: &Path) -> Result<ValidatedReceipt, StoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| StoreError::io("inspect history receipt", path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "history receipt is not a real file".to_owned(),
        });
    }
    let (recorded_at, receipt_id) = receipt_name(path)?;
    let receipt: HistoryReceipt = read_json(path)?;
    validate_receipt(store, path, recorded_at, receipt_id, &receipt)?;
    Ok(ValidatedReceipt {
        receipt,
        path: path.to_path_buf(),
        bytes: ByteSize::from_bytes(metadata.len()),
    })
}

fn receipt_name(path: &Path) -> Result<(u64, Uuid), StoreError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "history receipt name is not Unicode".to_owned(),
        })?;
    let stem = name
        .strip_suffix(".json")
        .ok_or_else(|| StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "history receipt name does not end in .json".to_owned(),
        })?;
    let (timestamp, identifier) =
        stem.split_once('-')
            .ok_or_else(|| StoreError::InvalidOwnership {
                path: path.to_path_buf(),
                reason: "history receipt name is malformed".to_owned(),
            })?;
    let recorded_at = timestamp
        .parse::<u64>()
        .map_err(|_| StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "history receipt timestamp is malformed".to_owned(),
        })?;
    let receipt_id = Uuid::from_str(identifier).map_err(|_| StoreError::InvalidOwnership {
        path: path.to_path_buf(),
        reason: "history receipt identifier is malformed".to_owned(),
    })?;
    Ok((recorded_at, receipt_id))
}

fn validate_receipt(
    store: &Store,
    path: &Path,
    recorded_at: u64,
    receipt_id: Uuid,
    receipt: &HistoryReceipt,
) -> Result<(), StoreError> {
    let payload_kind = match receipt.payload {
        HistoryPayload::Build(_) => HistoryKind::Build,
        HistoryPayload::Collection(_) => HistoryKind::Collection,
        HistoryPayload::Hook(_) => HistoryKind::Hook,
        HistoryPayload::Quota(_) => HistoryKind::Quota,
        HistoryPayload::Recovery(_) => HistoryKind::Recovery,
    };
    let valid = receipt.schema_version == 1
        && receipt.store_id == store.marker.store_id
        && receipt.recorded_at == recorded_at
        && receipt.receipt_id == receipt_id
        && receipt.kind == payload_kind;
    if valid {
        Ok(())
    } else {
        Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "history receipt envelope does not match its store, name, or payload"
                .to_owned(),
        })
    }
}

pub(crate) fn receipt_order(
    left: &ValidatedReceipt,
    right: &ValidatedReceipt,
) -> std::cmp::Ordering {
    left.receipt
        .recorded_at
        .cmp(&right.receipt.recorded_at)
        .then_with(|| left.receipt.receipt_id.cmp(&right.receipt.receipt_id))
}

fn matches_query(receipt: &HistoryReceipt, query: &HistoryQuery) -> bool {
    query.kind.is_none_or(|kind| kind == receipt.kind)
        && query.since.is_none_or(|since| receipt.recorded_at >= since)
        && query
            .arena_prefix
            .as_ref()
            .is_none_or(|prefix| match &receipt.payload {
                HistoryPayload::Build(build) => build.arena_id.as_str().starts_with(prefix),
                HistoryPayload::Recovery(recovery) => {
                    recovery.arena_id.as_str().starts_with(prefix)
                }
                _ => false,
            })
        && query
            .worktree_id
            .as_ref()
            .is_none_or(|expected| match &receipt.payload {
                HistoryPayload::Build(build) => &build.worktree_id == expected,
                HistoryPayload::Hook(hook) => &hook.worktree_id == expected,
                _ => false,
            })
}
