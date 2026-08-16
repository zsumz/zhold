use std::{fs, fs::OpenOptions, path::Path};

use serde::Serialize;

use crate::{
    StoreError,
    io::{
        configure_private_file,
        json_file::{
            encoded, remove_staging_file, temporary_path, validate_existing_file,
            validate_metadata_parent, write_and_sync,
        },
        json_publish::{JsonPublication, PublicationPoint, sync_metadata_directory},
        secure_open_file,
    },
};

#[derive(Debug)]
pub(crate) enum JsonCreation {
    Existing,
    Published(JsonPublication),
}

pub(crate) fn create_json<T: Serialize>(path: &Path, value: &T) -> Result<bool, StoreError> {
    match create_json_commit_aware(path, value)? {
        JsonCreation::Existing => Ok(false),
        JsonCreation::Published(JsonPublication::Durable { .. }) => Ok(true),
        JsonCreation::Published(JsonPublication::VisibleButDurabilityUnconfirmed { error }) => {
            Err(error)
        }
    }
}

pub(crate) fn create_json_commit_aware<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<JsonCreation, StoreError> {
    create_json_commit_aware_with(path, value, |_| Ok(()))
}

fn create_json_commit_aware_with<T: Serialize>(
    path: &Path,
    value: &T,
    mut checkpoint: impl FnMut(PublicationPoint) -> Result<(), StoreError>,
) -> Result<JsonCreation, StoreError> {
    validate_metadata_parent(path)?;
    validate_existing_file(path)?;
    let temporary = temporary_path(path);
    let bytes = encoded(path, value)?;
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    configure_private_file(&mut options);
    let mut file = options
        .open(&temporary)
        .map_err(|error| StoreError::io("create metadata staging file", &temporary, error))?;
    secure_open_file(&file, &temporary)?;
    write_and_sync(&mut file, &temporary, &bytes)?;
    drop(file);
    match fs::hard_link(&temporary, path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            remove_staging_file(&temporary)?;
            validate_existing_file(path)?;
            return Ok(JsonCreation::Existing);
        }
        Err(error) => {
            let _ignored = fs::remove_file(&temporary);
            return Err(StoreError::io("publish new metadata file", path, error));
        }
    }
    if let Some(publication) = synchronize_publication(path, &mut checkpoint) {
        return Ok(JsonCreation::Published(publication));
    }
    if let Err(error) = checkpoint(PublicationPoint::BeforeStagingRemoval) {
        return Ok(JsonCreation::Published(JsonPublication::cleanup_warning(
            error,
        )));
    }
    if let Err(error) = remove_staging_file(&temporary) {
        return Ok(JsonCreation::Published(JsonPublication::cleanup_warning(
            error,
        )));
    }
    for point in [
        PublicationPoint::StagingRemoved,
        PublicationPoint::BeforeFinalDirectorySync,
    ] {
        if let Err(error) = checkpoint(point) {
            return Ok(JsonCreation::Published(JsonPublication::cleanup_warning(
                error,
            )));
        }
    }
    if let Err(error) = sync_metadata_directory(path) {
        return Ok(JsonCreation::Published(JsonPublication::cleanup_warning(
            error,
        )));
    }
    Ok(JsonCreation::Published(JsonPublication::clean()))
}

fn synchronize_publication(
    path: &Path,
    checkpoint: &mut impl FnMut(PublicationPoint) -> Result<(), StoreError>,
) -> Option<JsonPublication> {
    for point in [
        PublicationPoint::PrimaryPublished,
        PublicationPoint::BeforePublishedDirectorySync,
    ] {
        if let Err(error) = checkpoint(point) {
            return Some(JsonPublication::durability_unconfirmed(path, &error));
        }
    }
    if let Err(error) = sync_metadata_directory(path) {
        return Some(JsonPublication::durability_unconfirmed(path, &error));
    }
    if let Err(error) = checkpoint(PublicationPoint::PublishedDirectorySynced) {
        return Some(JsonPublication::cleanup_warning(error));
    }
    None
}

#[cfg(test)]
pub(super) fn create_json_with_fault<T: Serialize>(
    path: &Path,
    value: &T,
    fault: &'static str,
) -> Result<JsonCreation, StoreError> {
    create_json_commit_aware_with(path, value, |point| {
        let name = match point {
            PublicationPoint::PrimaryPublished => "primary_published",
            PublicationPoint::BeforePublishedDirectorySync => "published_directory_sync",
            PublicationPoint::PublishedDirectorySynced => "published_directory_synced",
            PublicationPoint::BeforeStagingRemoval => "staging_removal",
            PublicationPoint::StagingRemoved => "staging_removed",
            PublicationPoint::BeforeFinalDirectorySync => "final_directory_sync",
            PublicationPoint::PrimaryRotated
            | PublicationPoint::PrimaryWithBackupStabilized
            | PublicationPoint::BeforePreviousBackupRemoval
            | PublicationPoint::PreviousBackupRemoved
            | PublicationPoint::PreviousBackupRemovalSynced
            | PublicationPoint::BeforeBackupRemoval
            | PublicationPoint::BackupRemoved => return Ok(()),
        };
        if name == fault {
            Err(StoreError::io(
                "inject metadata creation fault",
                path,
                std::io::Error::other(name),
            ))
        } else {
            Ok(())
        }
    })
}
