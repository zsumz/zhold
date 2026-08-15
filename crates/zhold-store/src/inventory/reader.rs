use std::{fs, path::Path};

use zhold_core::ByteSize;

use super::{ArenaMeasurement, Inventory, read_arena_snapshot};
use crate::{StoreError, io::measure_tree};

pub(crate) fn read_inventory(store: &crate::Store) -> Result<Inventory, StoreError> {
    let snapshot = read_arena_snapshot(store, ArenaMeasurement::Deep)?;
    let layout = &store.layout;
    let trash_path = layout.trash();
    ensure_real_contained_directory(&trash_path, &snapshot.root)?;
    let trash = measure_directory_contents(&trash_path)?;
    let physical = measure_tree(layout.root())?;
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
        store_id: store.marker.store_id,
        store_root: snapshot.root,
        observed_at: snapshot.observed_at,
        total: snapshot.total,
        protected: snapshot.protected,
        reserved: snapshot.reserved,
        uncertain_owned: snapshot.uncertain_owned,
        trash,
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
