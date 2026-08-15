use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::{
    StoreError,
    io::{configure_private_file, secure_file, secure_open_file},
};

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, StoreError> {
    match read_one(path) {
        Ok(value) => Ok(value),
        Err(StoreError::Json { .. } | StoreError::Io { .. }) => {
            let backup = backup_path(path);
            if is_real_file(&backup)? {
                read_one(&backup)
            } else {
                read_one(path)
            }
        }
        Err(error) => Err(error),
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

fn replace_with_backup(path: &Path, temporary: &Path) -> Result<(), StoreError> {
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
    sync_metadata_directory(path)?;
    if is_real_file(&backup)? {
        fs::remove_file(&backup)
            .map_err(|error| StoreError::io("remove metadata backup", &backup, error))?;
        sync_metadata_directory(path)?;
    }
    Ok(())
}

#[cfg(unix)]
fn sync_metadata_directory(path: &Path) -> Result<(), StoreError> {
    let parent = metadata_parent(path)?;
    let directory = fs::File::open(parent)
        .map_err(|error| StoreError::io("open metadata directory for sync", parent, error))?;
    directory
        .sync_all()
        .map_err(|error| StoreError::io("sync metadata directory", parent, error))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn sync_metadata_directory(_path: &Path) -> Result<(), StoreError> {
    Ok(())
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

fn metadata_parent(path: &Path) -> Result<&Path, StoreError> {
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

fn validate_existing_file(path: &Path) -> Result<(), StoreError> {
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

fn is_real_file(path: &Path) -> Result<bool, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(StoreError::InvalidOwnership {
                path: path.to_path_buf(),
                reason: "metadata path is not a real file".to_owned(),
            })
        }
        Ok(_) => {
            secure_file(path)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(StoreError::io("inspect metadata path", path, error)),
    }
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension(format!("json.{}.new", Uuid::new_v4()))
}
