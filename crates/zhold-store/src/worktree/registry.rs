use std::{fs, path::Path, str::FromStr};

use zhold_core::{WorktreeId, WorktreeIntegrationState, WorktreeKey};

use super::{WorktreeFinding, WorktreeIntegration};
use crate::{
    Store, StoreError, WorktreeContext,
    io::{is_json_publication_artifact, read_json},
};

pub(crate) fn read_for_context(
    store: &Store,
    context: &crate::BuildContext,
) -> Result<Option<WorktreeIntegration>, StoreError> {
    let key = WorktreeKey::derive(&context.repository_id, &context.worktree_id);
    read(store, &key)
}

pub(crate) fn read_for_ids(
    store: &Store,
    repository: &zhold_core::RepositoryId,
    worktree: &WorktreeId,
) -> Result<Option<WorktreeIntegration>, StoreError> {
    let key = WorktreeKey::derive(repository, worktree);
    read(store, &key)
}

pub(crate) fn read(
    store: &Store,
    key: &WorktreeKey,
) -> Result<Option<WorktreeIntegration>, StoreError> {
    let path = store.layout.worktree_integration(key);
    match fs::symlink_metadata(&path) {
        Ok(_) => read_path(store, &path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(StoreError::io("inspect worktree integration", path, error)),
    }
}

pub(crate) fn scan(
    store: &Store,
) -> Result<(Vec<WorktreeIntegration>, Vec<WorktreeFinding>), StoreError> {
    let directory = store.layout.worktree_integrations();
    let entries = fs::read_dir(&directory)
        .map_err(|error| StoreError::io("read worktree integrations", &directory, error))?;
    let mut records = Vec::new();
    let mut findings = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            StoreError::io("read worktree integration entry", &directory, error)
        })?;
        let path = entry.path();
        if is_json_publication_artifact(&path) {
            continue;
        }
        match read_path(store, &path) {
            Ok(record) => records.push(record),
            Err(error) => findings.push(WorktreeFinding {
                path,
                reason: error.to_string(),
            }),
        }
    }
    records.sort_by(|left, right| left.worktree_key.cmp(&right.worktree_key));
    Ok((records, findings))
}

pub(crate) fn find_path(
    store: &Store,
    path: &Path,
) -> Result<Option<WorktreeIntegration>, StoreError> {
    let requested = registered_path(path)?;
    let (records, _) = scan(store)?;
    let matching = records
        .into_iter()
        .filter(|record| record.canonical_path == requested)
        .collect::<Vec<_>>();
    let mut current = matching
        .iter()
        .filter(|record| record.state != WorktreeIntegrationState::Removed);
    let selected = current.next().cloned();
    if current.next().is_some() {
        return Err(StoreError::InvalidOwnership {
            path: requested,
            reason: "multiple live worktree records claim the same path".to_owned(),
        });
    }
    Ok(selected.or_else(|| matching.into_iter().max_by_key(|record| record.updated_at)))
}

pub(crate) fn reject_alias(store: &Store, context: &WorktreeContext) -> Result<(), StoreError> {
    let (records, _) = scan(store)?;
    if let Some(record) = records.into_iter().find(|record| {
        record.canonical_path == context.canonical_path
            && record.worktree_key != context.key
            && record.state != WorktreeIntegrationState::Removed
    }) {
        return Err(StoreError::InvalidOwnership {
            path: context.canonical_path.clone(),
            reason: format!(
                "path is already claimed by worktree integration {}",
                record.worktree_key
            ),
        });
    }
    Ok(())
}

pub(crate) fn validate_context(context: &WorktreeContext) -> Result<(), StoreError> {
    let canonical = context.canonical_path.canonicalize().map_err(|error| {
        StoreError::io(
            "canonicalize registered worktree context",
            &context.canonical_path,
            error,
        )
    })?;
    let text = canonical
        .to_str()
        .filter(|_| canonical.is_absolute())
        .ok_or_else(|| StoreError::InvalidOwnership {
            path: context.canonical_path.clone(),
            reason: "worktree context path is not absolute Unicode".to_owned(),
        })?;
    let key = WorktreeKey::derive(&context.repository_id, &context.worktree_id);
    if canonical == context.canonical_path
        && WorktreeId::derive(text) == context.worktree_id
        && key == context.key
    {
        Ok(())
    } else {
        Err(StoreError::InvalidOwnership {
            path: context.canonical_path.clone(),
            reason: "worktree context path or derived identity is inconsistent".to_owned(),
        })
    }
}

fn read_path(store: &Store, path: &Path) -> Result<WorktreeIntegration, StoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| StoreError::io("inspect worktree integration", path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "worktree integration is not a real file".to_owned(),
        });
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "worktree integration filename is not Unicode".to_owned(),
        })?;
    let identity = name
        .strip_suffix(".json")
        .ok_or_else(|| StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "worktree integration filename is malformed".to_owned(),
        })?;
    let key = WorktreeKey::from_str(identity).map_err(|error| StoreError::InvalidOwnership {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })?;
    let record: WorktreeIntegration = read_json(path)?;
    validate(store, path, &key, &record)?;
    Ok(record)
}

fn validate(
    store: &Store,
    path: &Path,
    key: &WorktreeKey,
    record: &WorktreeIntegration,
) -> Result<(), StoreError> {
    let canonical = record
        .canonical_path
        .to_str()
        .filter(|_| record.canonical_path.is_absolute())
        .ok_or_else(|| StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "registered worktree path is not absolute Unicode".to_owned(),
        })?;
    let derived_worktree = WorktreeId::derive(canonical);
    let derived_key = WorktreeKey::derive(&record.repository_id, &record.worktree_id);
    let valid = record.schema_version == 1
        && record.store_id == store.marker.store_id
        && &record.worktree_key == key
        && record.worktree_id == derived_worktree
        && record.worktree_key == derived_key;
    if valid {
        Ok(())
    } else {
        Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "worktree integration identity or ownership is invalid".to_owned(),
        })
    }
}

pub(crate) fn registered_path(path: &Path) -> Result<std::path::PathBuf, StoreError> {
    if path.exists() {
        return path
            .canonicalize()
            .map_err(|error| StoreError::io("canonicalize worktree hook path", path, error));
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| StoreError::io("resolve hook working directory", path, error))?
            .join(path)
    };
    let normalized = normalize_absolute(&absolute)?;
    canonicalize_existing_prefix(&normalized)
}

fn canonicalize_existing_prefix(path: &Path) -> Result<std::path::PathBuf, StoreError> {
    let mut cursor = path;
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(cursor) {
            Ok(_) => {
                let mut resolved = cursor.canonicalize().map_err(|error| {
                    StoreError::io("canonicalize worktree hook path prefix", cursor, error)
                })?;
                for component in missing.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = cursor
                    .file_name()
                    .ok_or_else(|| StoreError::InvalidOwnership {
                        path: path.to_path_buf(),
                        reason: "worktree hook path has no existing filesystem prefix".to_owned(),
                    })?;
                missing.push(name.to_os_string());
                cursor = cursor
                    .parent()
                    .ok_or_else(|| StoreError::InvalidOwnership {
                        path: path.to_path_buf(),
                        reason: "worktree hook path has no parent".to_owned(),
                    })?;
            }
            Err(error) => {
                return Err(StoreError::io(
                    "inspect worktree hook path prefix",
                    cursor,
                    error,
                ));
            }
        }
    }
}

fn normalize_absolute(path: &Path) -> Result<std::path::PathBuf, StoreError> {
    use std::path::Component;

    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(StoreError::InvalidOwnership {
                        path: path.to_path_buf(),
                        reason: "worktree hook path escapes its filesystem root".to_owned(),
                    });
                }
            }
        }
    }
    Ok(normalized)
}
