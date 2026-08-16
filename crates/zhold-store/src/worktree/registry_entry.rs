use std::{collections::BTreeSet, fs, path::PathBuf};

use crate::{
    StoreError,
    io::{is_json_staging_path, json_backup_primary_path},
};

pub(super) fn logical_paths(directory: &std::path::Path) -> Result<Vec<PathBuf>, StoreError> {
    let entries = fs::read_dir(directory)
        .map_err(|error| StoreError::io("read worktree integrations", directory, error))?;
    let mut logical = BTreeSet::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| StoreError::io("read worktree integration entry", directory, error))?;
        let path = entry.path();
        if is_json_staging_path(&path) {
            continue;
        }
        logical.insert(json_backup_primary_path(&path).unwrap_or(path));
    }
    Ok(logical.into_iter().collect())
}
