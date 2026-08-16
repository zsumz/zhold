use std::{fs, path::Path};

use serde::{Serialize, de::DeserializeOwned};

use crate::{
    StoreError,
    io::{
        json_create::create_json,
        json_file::{is_real_file, read_json, validate_existing_file, write_json},
        json_path::backup_path,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JsonDocumentState {
    Absent,
    Primary,
    BackupOnly,
    PrimaryWithBackup,
}

impl JsonDocumentState {
    pub(crate) const fn is_absent(self) -> bool {
        matches!(self, Self::Absent)
    }
}

pub(crate) fn json_document_state(path: &Path) -> Result<JsonDocumentState, StoreError> {
    Ok(classify(
        is_real_file(path)?,
        is_real_file(&backup_path(path))?,
    ))
}

pub(crate) fn secure_json_document(path: &Path) -> Result<JsonDocumentState, StoreError> {
    let backup = backup_path(path);
    validate_existing_file(path)?;
    validate_existing_file(&backup)?;
    Ok(classify(entry_exists(path)?, entry_exists(&backup)?))
}

pub(crate) fn read_optional_json<T: DeserializeOwned>(
    path: &Path,
) -> Result<Option<T>, StoreError> {
    if json_document_state(path)?.is_absent() {
        Ok(None)
    } else {
        read_json(path).map(Some)
    }
}

pub(crate) fn upsert_json<T: DeserializeOwned + Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), StoreError> {
    if secure_json_document(path)?.is_absent() && create_json(path, value)? {
        Ok(())
    } else {
        write_json(path, value)
    }
}

fn classify(primary: bool, backup: bool) -> JsonDocumentState {
    match (primary, backup) {
        (false, false) => JsonDocumentState::Absent,
        (true, false) => JsonDocumentState::Primary,
        (false, true) => JsonDocumentState::BackupOnly,
        (true, true) => JsonDocumentState::PrimaryWithBackup,
    }
}

fn entry_exists(path: &Path) -> Result<bool, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(StoreError::io("inspect JSON document", path, error)),
    }
}
