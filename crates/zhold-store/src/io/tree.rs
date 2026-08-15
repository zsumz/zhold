use std::{fs, path::Path};

use zhold_core::ByteSize;

use crate::StoreError;

pub(crate) fn measure_tree(root: &Path) -> Result<ByteSize, StoreError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| StoreError::io("inspect filesystem entry", root, error))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Ok(ByteSize::from_bytes(measured_len(&metadata)));
    }

    let mut total = ByteSize::from_bytes(measured_len(&metadata));
    let entries =
        fs::read_dir(root).map_err(|error| StoreError::io("read directory", root, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| StoreError::io("read directory entry", root, error))?;
        total = total.saturating_add(measure_tree(&entry.path())?);
    }
    Ok(total)
}

pub(crate) fn remove_tree(root: &Path) -> Result<(), StoreError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| StoreError::io("inspect retirement entry", root, error))?;
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        let entries = fs::read_dir(root)
            .map_err(|error| StoreError::io("read retirement directory", root, error))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| StoreError::io("read retirement directory entry", root, error))?;
            remove_tree(&entry.path())?;
        }
        fs::remove_dir(root)
            .map_err(|error| StoreError::io("remove retirement directory", root, error))
    } else {
        fs::remove_file(root).map_err(|error| StoreError::io("remove retirement file", root, error))
    }
}

#[cfg(unix)]
fn measured_len(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;

    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn measured_len(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}
