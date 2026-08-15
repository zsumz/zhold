use std::{collections::BTreeSet, fs, path::Path};

use zhold_core::{ByteSize, SizeQuality};

use super::{ArenaMeasurement, Inventory, InventoryDepth, InventoryFinding, read_arena_snapshot};
use crate::{
    StoreError,
    io::{measure_tree, read_json},
    manifest::RetirementRecord,
};

pub(crate) fn read_inventory(
    store: &crate::Store,
    measurement: ArenaMeasurement,
) -> Result<Inventory, StoreError> {
    let mut snapshot = read_arena_snapshot(store, measurement)?;
    let layout = &store.layout;
    let trash_path = layout.trash();
    ensure_real_contained_directory(&trash_path, &snapshot.root)?;
    let (cached_trash, trash_findings) = cached_trash(store)?;
    let trash_uncertain = !trash_findings.is_empty();
    snapshot.findings.extend(trash_findings);
    let (depth, trash, trash_quality, physical) = match measurement {
        ArenaMeasurement::Cached => (
            InventoryDepth::Cached,
            cached_trash,
            if trash_uncertain {
                SizeQuality::Unknown
            } else {
                SizeQuality::Cached
            },
            None,
        ),
        ArenaMeasurement::Deep => (
            InventoryDepth::Deep,
            measure_directory_contents(&trash_path)?,
            SizeQuality::Fresh,
            Some(measure_tree(layout.root())?),
        ),
    };
    let available =
        ByteSize::from_bytes(fs2::available_space(layout.root()).map_err(|error| {
            StoreError::io("measure available store space", layout.root(), error)
        })?);
    let history = crate::history::summary(store)?;
    let worktrees = store.worktree_summary()?;
    let (quota, quota_finding) = match store.quota_status(zhold_core::QuotaProvider::Auto) {
        Ok(status) => (Some(status), None),
        Err(error @ (StoreError::InvalidOwnership { .. } | StoreError::Json { .. })) => {
            (None, Some(error.to_string()))
        }
        Err(error) => return Err(error),
    };

    Ok(Inventory {
        depth,
        store_id: store.marker.store_id,
        store_root: snapshot.root,
        observed_at: snapshot.observed_at,
        total: snapshot.total,
        protected: snapshot.protected,
        reserved: snapshot.reserved,
        uncertain_owned: snapshot.uncertain_owned,
        trash,
        trash_quality,
        physical,
        available,
        history,
        worktrees,
        quota,
        quota_finding,
        arenas: snapshot.arenas,
        findings: snapshot.findings,
    })
}

fn cached_trash(store: &crate::Store) -> Result<(ByteSize, Vec<InventoryFinding>), StoreError> {
    let mut total = ByteSize::ZERO;
    let mut owned_paths = BTreeSet::new();
    let mut findings = Vec::new();
    let index = store.layout.trash_index();
    for entry in fs::read_dir(&index)
        .map_err(|error| StoreError::io("read retirement journal index", &index, error))?
    {
        let path = entry
            .map_err(|error| StoreError::io("read retirement journal", &index, error))?
            .path();
        let result = read_json::<RetirementRecord>(&path).and_then(|record| {
            record.validate_journal(store, &path)?;
            if record.trash_path().is_dir() {
                total = total.saturating_add(record.retired_size());
                owned_paths.insert(record.trash_path().to_path_buf());
            } else {
                return Err(StoreError::InvalidOwnership {
                    path: path.clone(),
                    reason: "retirement journal has no matching trash directory".to_owned(),
                });
            }
            Ok(())
        });
        if let Err(error) = result {
            findings.push(InventoryFinding {
                path,
                reason: error.to_string(),
            });
        }
    }
    for entry in fs::read_dir(store.layout.trash())
        .map_err(|error| StoreError::io("read retirement directory", store.layout.trash(), error))?
    {
        let path = entry
            .map_err(|error| {
                StoreError::io(
                    "read retirement directory entry",
                    store.layout.trash(),
                    error,
                )
            })?
            .path();
        if !owned_paths.contains(&path) {
            findings.push(InventoryFinding {
                path,
                reason: "trash entry has no valid external retirement journal".to_owned(),
            });
        }
    }
    Ok((total, findings))
}

fn measure_directory_contents(path: &Path) -> Result<ByteSize, StoreError> {
    let mut entries = fs::read_dir(path)
        .map_err(|error| StoreError::io("read managed directory", path, error))?;
    entries.try_fold(ByteSize::ZERO, |total, entry| {
        let entry = entry.map_err(|error| StoreError::io("read managed entry", path, error))?;
        Ok(total.saturating_add(measure_tree(&entry.path())?))
    })
}

pub(crate) fn ensure_real_contained_directory(
    path: &Path,
    canonical_root: &Path,
) -> Result<(), StoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| StoreError::io("inspect managed directory", path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "managed path is not a real directory".to_owned(),
        });
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| StoreError::io("canonicalize managed directory", path, error))?;
    if !canonical.starts_with(canonical_root) {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "managed path escapes the marked store root".to_owned(),
        });
    }
    Ok(())
}
