use std::{
    fs::{self, File, OpenOptions},
    io,
    path::Path,
};

use fs2::FileExt;

use crate::StoreError;

#[derive(Debug)]
pub(crate) struct ExclusiveFileLock {
    _file: File,
}

#[derive(Debug)]
pub(crate) struct SharedFileLock {
    _file: File,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LockState {
    Available,
    Held,
}

impl ExclusiveFileLock {
    pub(crate) fn acquire(path: &Path) -> Result<Self, StoreError> {
        let file = open_lock_file(path)?;
        FileExt::lock_exclusive(&file)
            .map_err(|error| StoreError::io("acquire lock", path, error))?;
        Ok(Self { _file: file })
    }

    pub(crate) fn try_acquire(path: &Path) -> Result<Option<Self>, StoreError> {
        let file = open_lock_file(path)?;
        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(StoreError::io("probe lock", path, error)),
        }
    }

    pub(crate) fn probe(path: &Path) -> Result<LockState, StoreError> {
        Ok(match Self::try_acquire(path)? {
            Some(_lock) => LockState::Available,
            None => LockState::Held,
        })
    }
}

impl SharedFileLock {
    pub(crate) fn acquire(path: &Path) -> Result<Self, StoreError> {
        let file = open_lock_file(path)?;
        FileExt::lock_shared(&file)
            .map_err(|error| StoreError::io("acquire shared lock", path, error))?;
        Ok(Self { _file: file })
    }
}

fn open_lock_file(path: &Path) -> Result<File, StoreError> {
    validate_lock_parent(path)?;
    validate_lock_path(path)?;
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    configure_no_follow(&mut options);
    let file = options
        .open(path)
        .map_err(|error| StoreError::io("open lock", path, error))?;
    if !file
        .metadata()
        .map_err(|error| StoreError::io("inspect opened lock", path, error))?
        .is_file()
    {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "lock path is not a real file".to_owned(),
        });
    }
    Ok(file)
}

fn validate_lock_parent(path: &Path) -> Result<(), StoreError> {
    let parent = path.parent().ok_or_else(|| {
        StoreError::io(
            "resolve lock parent",
            path,
            io::Error::new(io::ErrorKind::InvalidInput, "lock path has no parent"),
        )
    })?;
    let metadata = fs::symlink_metadata(parent)
        .map_err(|error| StoreError::io("inspect lock directory", parent, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StoreError::InvalidOwnership {
            path: parent.to_path_buf(),
            reason: "lock parent is not a real directory".to_owned(),
        });
    }
    Ok(())
}

fn validate_lock_path(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(StoreError::InvalidOwnership {
                path: path.to_path_buf(),
                reason: "lock path is not a real file".to_owned(),
            })
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StoreError::io("inspect lock path", path, error)),
    }
}

#[cfg(unix)]
fn configure_no_follow(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;

    options.custom_flags(libc::O_NOFOLLOW);
}

#[cfg(not(unix))]
fn configure_no_follow(_options: &mut OpenOptions) {}
