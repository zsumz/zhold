use std::{fs, path::Path};

use crate::StoreError;

pub(crate) fn rotate_to_backup(path: &Path) -> Result<(), StoreError> {
    let backup = path.with_extension("json.bak");
    fs::rename(path, &backup)
        .map_err(|error| StoreError::io("rotate test JSON document", path, error))?;
    super::sync_metadata_directory(path)
}
