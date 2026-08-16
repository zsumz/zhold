use std::{fs, path::Path};

use crate::{
    StoreError,
    io::{
        json_file::{backup_path, is_real_file, validate_existing_file},
        secure_open_file,
    },
};

#[cfg(unix)]
use crate::io::json_file::metadata_parent;

#[derive(Debug)]
pub(crate) enum JsonPublication {
    Durable { cleanup_warning: Option<StoreError> },
    VisibleButDurabilityUnconfirmed { error: StoreError },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum JsonSource {
    PrimaryOnly,
    PrimaryWithBackup,
    Backup,
    Absent,
}

impl JsonPublication {
    pub(super) const fn clean() -> Self {
        Self::Durable {
            cleanup_warning: None,
        }
    }

    pub(super) fn cleanup_warning(error: StoreError) -> Self {
        Self::Durable {
            cleanup_warning: Some(error),
        }
    }

    pub(super) fn durability_unconfirmed(path: &Path, error: &StoreError) -> Self {
        Self::VisibleButDurabilityUnconfirmed {
            error: StoreError::durability_unconfirmed(path, error),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublicationPoint {
    PrimaryWithBackupStabilized,
    BeforePreviousBackupRemoval,
    PreviousBackupRemoved,
    PreviousBackupRemovalSynced,
    PrimaryRotated,
    PrimaryPublished,
    BeforePublishedDirectorySync,
    PublishedDirectorySynced,
    BeforeBackupRemoval,
    BackupRemoved,
    BeforeStagingRemoval,
    StagingRemoved,
    BeforeFinalDirectorySync,
}

pub(super) fn replace_with_backup(
    path: &Path,
    temporary: &Path,
    source: JsonSource,
) -> Result<JsonPublication, StoreError> {
    replace_with_backup_with(path, temporary, source, |_| Ok(()))
}

pub(super) fn replace_with_backup_with(
    path: &Path,
    temporary: &Path,
    source: JsonSource,
    mut checkpoint: impl FnMut(PublicationPoint) -> Result<(), StoreError>,
) -> Result<JsonPublication, StoreError> {
    let backup = backup_path(path);
    validate_existing_file(&backup)?;
    let primary_exists = is_real_file(path)?;
    let backup_exists = is_real_file(&backup)?;
    validate_source(path, source, primary_exists, backup_exists)?;
    match source {
        JsonSource::PrimaryWithBackup => {
            stabilize_primary(path)?;
            checkpoint(PublicationPoint::PrimaryWithBackupStabilized)?;
            checkpoint(PublicationPoint::BeforePreviousBackupRemoval)?;
            fs::remove_file(&backup).map_err(|error| {
                StoreError::io("remove previous metadata backup", &backup, error)
            })?;
            checkpoint(PublicationPoint::PreviousBackupRemoved)?;
            sync_metadata_directory(path)?;
            checkpoint(PublicationPoint::PreviousBackupRemovalSynced)?;
            fs::rename(path, &backup)
                .map_err(|error| StoreError::io("rotate metadata backup", path, error))?;
            checkpoint(PublicationPoint::PrimaryRotated)?;
            sync_metadata_directory(path)?;
        }
        JsonSource::PrimaryOnly => {
            fs::rename(path, &backup)
                .map_err(|error| StoreError::io("rotate metadata backup", path, error))?;
            checkpoint(PublicationPoint::PrimaryRotated)?;
            sync_metadata_directory(path)?;
        }
        JsonSource::Backup if primary_exists => {
            fs::remove_file(path).map_err(|error| {
                StoreError::io("discard rejected metadata primary", path, error)
            })?;
            checkpoint(PublicationPoint::PrimaryRotated)?;
            sync_metadata_directory(path)?;
        }
        JsonSource::Backup | JsonSource::Absent => {}
    }
    if let Err(error) = fs::rename(temporary, path) {
        if matches!(
            source,
            JsonSource::PrimaryOnly | JsonSource::PrimaryWithBackup
        ) && is_real_file(&backup)?
        {
            fs::rename(&backup, path)
                .map_err(|recovery| StoreError::io("restore metadata backup", path, recovery))?;
            sync_metadata_directory(path)?;
        }
        return Err(StoreError::io("publish metadata", path, error));
    }
    if let Err(error) = checkpoint(PublicationPoint::PrimaryPublished) {
        return Ok(JsonPublication::durability_unconfirmed(path, &error));
    }
    if let Err(error) = checkpoint(PublicationPoint::BeforePublishedDirectorySync) {
        return Ok(JsonPublication::durability_unconfirmed(path, &error));
    }
    if let Err(error) = sync_metadata_directory(path) {
        return Ok(JsonPublication::durability_unconfirmed(path, &error));
    }
    if let Err(error) = checkpoint(PublicationPoint::PublishedDirectorySynced) {
        return Ok(JsonPublication::cleanup_warning(error));
    }
    if is_real_file(&backup)? {
        if let Err(error) = checkpoint(PublicationPoint::BeforeBackupRemoval) {
            return Ok(JsonPublication::cleanup_warning(error));
        }
        if let Err(error) = fs::remove_file(&backup) {
            return Ok(JsonPublication::cleanup_warning(StoreError::io(
                "remove metadata backup",
                &backup,
                error,
            )));
        }
        if let Err(error) = checkpoint(PublicationPoint::BackupRemoved) {
            return Ok(JsonPublication::cleanup_warning(error));
        }
        if let Err(error) = checkpoint(PublicationPoint::BeforeFinalDirectorySync) {
            return Ok(JsonPublication::cleanup_warning(error));
        }
        if let Err(error) = sync_metadata_directory(path) {
            return Ok(JsonPublication::cleanup_warning(error));
        }
    }
    Ok(JsonPublication::clean())
}

fn validate_source(
    path: &Path,
    source: JsonSource,
    primary_exists: bool,
    backup_exists: bool,
) -> Result<(), StoreError> {
    let consistent = match source {
        JsonSource::PrimaryOnly => primary_exists && !backup_exists,
        JsonSource::PrimaryWithBackup => primary_exists && backup_exists,
        JsonSource::Backup => backup_exists,
        JsonSource::Absent => !primary_exists && !backup_exists,
    };
    if consistent {
        Ok(())
    } else {
        Err(StoreError::io(
            "validate metadata publication source",
            path,
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "metadata source changed during publication",
            ),
        ))
    }
}

fn stabilize_primary(path: &Path) -> Result<(), StoreError> {
    let file = fs::File::open(path)
        .map_err(|error| StoreError::io("open metadata primary for sync", path, error))?;
    secure_open_file(&file, path)?;
    file.sync_all()
        .map_err(|error| StoreError::io("sync metadata primary", path, error))?;
    sync_metadata_directory(path)
}

#[cfg(unix)]
pub(crate) fn sync_metadata_directory(path: &Path) -> Result<(), StoreError> {
    let parent = metadata_parent(path)?;
    let directory = fs::File::open(parent)
        .map_err(|error| StoreError::io("open metadata directory for sync", parent, error))?;
    directory
        .sync_all()
        .map_err(|error| StoreError::io("sync metadata directory", parent, error))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn sync_metadata_directory(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}
