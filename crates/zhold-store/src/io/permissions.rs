use std::{fs, path::Path};

use crate::StoreError;

#[cfg(unix)]
pub(crate) fn secure_directory(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let metadata = fs::symlink_metadata(path)
        .map_err(|error| StoreError::io("inspect private directory", path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "private store path is not a real directory".to_owned(),
        });
    }
    verify_owner(path, metadata.uid())?;
    if metadata.permissions().mode() & 0o777 != 0o700 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| StoreError::io("restrict private directory", path, error))?;
    }
    Ok(())
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn secure_directory(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn secure_file(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| StoreError::io("open private metadata", path, error))?;
    secure_open_file(&file, path)
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn secure_file(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn secure_open_file(file: &fs::File, path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let metadata = file
        .metadata()
        .map_err(|error| StoreError::io("inspect private file", path, error))?;
    if !metadata.is_file() {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "private metadata path is not a real file".to_owned(),
        });
    }
    verify_owner(path, metadata.uid())?;
    if metadata.permissions().mode() & 0o777 != 0o600 {
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| StoreError::io("restrict private file", path, error))?;
    }
    Ok(())
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn secure_open_file(_file: &fs::File, _path: &Path) -> Result<(), StoreError> {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn configure_private_file(options: &mut fs::OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;

    options.mode(0o600);
}

#[cfg(not(unix))]
pub(crate) fn configure_private_file(_options: &mut fs::OpenOptions) {}

#[cfg(unix)]
fn verify_owner(path: &Path, owner: u32) -> Result<(), StoreError> {
    let effective = nix::unistd::Uid::effective().as_raw();
    if owner == effective {
        Ok(())
    } else {
        Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: format!("store path belongs to user {owner}, not effective user {effective}"),
        })
    }
}
