use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::{
    StoreError,
    io::{
        configure_private_file,
        json_publish::{JsonPublication, replace_with_backup, sync_metadata_directory},
        secure_file, secure_open_file, verify_file,
    },
};

#[cfg(test)]
use crate::io::json_publish::{PublicationPoint, replace_with_backup_with};

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, StoreError> {
    match read_one(path) {
        Ok(value) => Ok(value),
        Err(error) if backup_eligible(&error) => read_backup(path, error),
        Err(error) => Err(error),
    }
}

pub(super) fn backup_eligible(error: &StoreError) -> bool {
    match error {
        StoreError::Json { .. } => true,
        StoreError::Io { source, .. } => source.kind() == std::io::ErrorKind::NotFound,
        _ => false,
    }
}

fn read_backup<T: DeserializeOwned>(path: &Path, primary: StoreError) -> Result<T, StoreError> {
    let backup = backup_path(path);
    if is_real_file(&backup)? {
        read_one(&backup)
    } else {
        Err(primary)
    }
}

pub(crate) fn create_json<T: Serialize>(path: &Path, value: &T) -> Result<bool, StoreError> {
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
            return Ok(false);
        }
        Err(error) => {
            let _ignored = fs::remove_file(&temporary);
            return Err(StoreError::io("publish new metadata file", path, error));
        }
    }
    sync_metadata_directory(path)?;
    remove_staging_file(&temporary)?;
    sync_metadata_directory(path)?;
    Ok(true)
}

pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), StoreError> {
    let _publication = write_json_commit_aware(path, value)?;
    Ok(())
}

pub(crate) fn write_json_commit_aware<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<JsonPublication, StoreError> {
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
    replace_with_backup(path, &temporary)
}

pub(crate) fn remove_json(path: &Path) -> Result<(), StoreError> {
    validate_metadata_parent(path)?;
    if !is_real_file(path)? {
        return Err(StoreError::io(
            "remove metadata",
            path,
            std::io::Error::new(std::io::ErrorKind::NotFound, "metadata file does not exist"),
        ));
    }
    fs::remove_file(path).map_err(|error| StoreError::io("remove metadata", path, error))?;
    sync_metadata_directory(path)
}

fn encoded<T: Serialize>(path: &Path, value: &T) -> Result<Vec<u8>, StoreError> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|error| StoreError::json(path, error))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_and_sync(file: &mut std::fs::File, path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    file.write_all(bytes)
        .map_err(|error| StoreError::io("write metadata file", path, error))?;
    file.sync_all()
        .map_err(|error| StoreError::io("sync metadata file", path, error))
}

fn remove_staging_file(path: &Path) -> Result<(), StoreError> {
    fs::remove_file(path)
        .map_err(|error| StoreError::io("remove metadata staging file", path, error))
}

fn read_one<T: DeserializeOwned>(path: &Path) -> Result<T, StoreError> {
    if !is_real_file(path)? {
        return Err(StoreError::io(
            "read metadata",
            path,
            std::io::Error::new(std::io::ErrorKind::NotFound, "metadata file does not exist"),
        ));
    }
    let bytes = fs::read(path).map_err(|error| StoreError::io("read metadata", path, error))?;
    serde_json::from_slice(&bytes).map_err(|error| StoreError::json(path, error))
}

#[cfg(test)]
pub(super) fn write_json_with_fault<T: Serialize>(
    path: &Path,
    value: &T,
    fault: &'static str,
) -> Result<JsonPublication, StoreError> {
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
    replace_with_backup_with(path, &temporary, |point| {
        let name = match point {
            PublicationPoint::PrimaryRotated => "primary_rotated",
            PublicationPoint::PrimaryPublished => "primary_published",
            PublicationPoint::BeforePublishedDirectorySync => "published_directory_sync",
            PublicationPoint::PublishedDirectorySynced => "published_directory_synced",
            PublicationPoint::BeforeBackupRemoval => "backup_removal",
            PublicationPoint::BackupRemoved => "backup_removed",
            PublicationPoint::BeforeFinalDirectorySync => "final_directory_sync",
        };
        if name == fault {
            Err(StoreError::io(
                "inject metadata publication fault",
                path,
                std::io::Error::other(name),
            ))
        } else {
            Ok(())
        }
    })
}

fn validate_metadata_parent(path: &Path) -> Result<(), StoreError> {
    let parent = metadata_parent(path)?;
    let metadata = fs::symlink_metadata(parent)
        .map_err(|error| StoreError::io("inspect metadata directory", parent, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StoreError::InvalidOwnership {
            path: parent.to_path_buf(),
            reason: "metadata parent is not a real directory".to_owned(),
        });
    }
    Ok(())
}

pub(super) fn metadata_parent(path: &Path) -> Result<&Path, StoreError> {
    path.parent().ok_or_else(|| {
        StoreError::io(
            "resolve metadata parent",
            path,
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "metadata path has no parent",
            ),
        )
    })
}

pub(super) fn validate_existing_file(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(StoreError::InvalidOwnership {
                path: path.to_path_buf(),
                reason: "metadata path is not a real file".to_owned(),
            })
        }
        Ok(_) => secure_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StoreError::io("inspect metadata path", path, error)),
    }
}

pub(super) fn is_real_file(path: &Path) -> Result<bool, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(StoreError::InvalidOwnership {
                path: path.to_path_buf(),
                reason: "metadata path is not a real file".to_owned(),
            })
        }
        Ok(_) => {
            verify_file(path)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(StoreError::io("inspect metadata path", path, error)),
    }
}

pub(super) fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension(format!("json.{}.new", Uuid::new_v4()))
}
