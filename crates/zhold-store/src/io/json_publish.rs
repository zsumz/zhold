use std::{fs, path::Path};

use crate::{
    StoreError,
    io::json_file::{backup_path, is_real_file, validate_existing_file},
};

#[cfg(unix)]
use crate::io::json_file::metadata_parent;

#[derive(Debug)]
pub(crate) struct JsonPublication {
    cleanup_warning: Option<StoreError>,
}

impl JsonPublication {
    pub(crate) fn cleanup_warning(self) -> Option<StoreError> {
        self.cleanup_warning
    }

    const fn clean() -> Self {
        Self {
            cleanup_warning: None,
        }
    }

    fn warning(error: StoreError) -> Self {
        Self {
            cleanup_warning: Some(error),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublicationPoint {
    PrimaryRotated,
    PrimaryPublished,
    BeforePublishedDirectorySync,
    PublishedDirectorySynced,
    BeforeBackupRemoval,
    BackupRemoved,
    BeforeFinalDirectorySync,
}

pub(super) fn replace_with_backup(
    path: &Path,
    temporary: &Path,
) -> Result<JsonPublication, StoreError> {
    replace_with_backup_with(path, temporary, |_| Ok(()))
}

pub(super) fn replace_with_backup_with(
    path: &Path,
    temporary: &Path,
    mut checkpoint: impl FnMut(PublicationPoint) -> Result<(), StoreError>,
) -> Result<JsonPublication, StoreError> {
    let backup = backup_path(path);
    validate_existing_file(&backup)?;
    let primary_exists = is_real_file(path)?;
    let backup_exists = is_real_file(&backup)?;
    if primary_exists {
        if backup_exists {
            fs::remove_file(&backup)
                .map_err(|error| StoreError::io("remove stale metadata backup", &backup, error))?;
        }
        fs::rename(path, &backup)
            .map_err(|error| StoreError::io("rotate metadata backup", path, error))?;
        checkpoint(PublicationPoint::PrimaryRotated)?;
        sync_metadata_directory(path)?;
    }
    if let Err(error) = fs::rename(temporary, path) {
        if is_real_file(&backup)? {
            fs::rename(&backup, path)
                .map_err(|recovery| StoreError::io("restore metadata backup", path, recovery))?;
            sync_metadata_directory(path)?;
        }
        return Err(StoreError::io("publish metadata", path, error));
    }
    if let Err(error) = checkpoint(PublicationPoint::PrimaryPublished) {
        return Ok(JsonPublication::warning(error));
    }
    if let Err(error) = checkpoint(PublicationPoint::BeforePublishedDirectorySync) {
        return Ok(JsonPublication::warning(error));
    }
    if let Err(error) = sync_metadata_directory(path) {
        return Ok(JsonPublication::warning(error));
    }
    if let Err(error) = checkpoint(PublicationPoint::PublishedDirectorySynced) {
        return Ok(JsonPublication::warning(error));
    }
    if is_real_file(&backup)? {
        if let Err(error) = checkpoint(PublicationPoint::BeforeBackupRemoval) {
            return Ok(JsonPublication::warning(error));
        }
        if let Err(error) = fs::remove_file(&backup) {
            return Ok(JsonPublication::warning(StoreError::io(
                "remove metadata backup",
                &backup,
                error,
            )));
        }
        if let Err(error) = checkpoint(PublicationPoint::BackupRemoved) {
            return Ok(JsonPublication::warning(error));
        }
        if let Err(error) = checkpoint(PublicationPoint::BeforeFinalDirectorySync) {
            return Ok(JsonPublication::warning(error));
        }
        if let Err(error) = sync_metadata_directory(path) {
            return Ok(JsonPublication::warning(error));
        }
    }
    Ok(JsonPublication::clean())
}

#[cfg(unix)]
pub(super) fn sync_metadata_directory(path: &Path) -> Result<(), StoreError> {
    let parent = metadata_parent(path)?;
    let directory = fs::File::open(parent)
        .map_err(|error| StoreError::io("open metadata directory for sync", parent, error))?;
    directory
        .sync_all()
        .map_err(|error| StoreError::io("sync metadata directory", parent, error))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
pub(super) fn sync_metadata_directory(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}
