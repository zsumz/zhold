use std::{collections::BTreeSet, fs, path::Path};

use zhold_core::ByteSize;

use crate::{
    Store, StoreError,
    inventory::ensure_real_contained_directory,
    io::{
        is_json_publication_artifact, measure_tree, read_json, read_optional_json, remove_json,
        remove_tree, write_json,
    },
    lock::ExclusiveFileLock,
    manifest::{ArenaManifest, InitializationRecord},
    time::unix_seconds,
};

pub(crate) fn reconcile_initializations(store: &Store) -> Result<(), StoreError> {
    store.ensure_writable("reconcile arena initialization")?;
    let _collection = ExclusiveFileLock::acquire(&store.layout.collection_lock())?;
    reconcile_initializations_locked(store)
}

pub(crate) fn reconcile_initializations_locked(store: &Store) -> Result<(), StoreError> {
    store.ensure_writable("reconcile arena initialization")?;
    let root =
        store.layout.root().canonicalize().map_err(|error| {
            StoreError::io("canonicalize store root", store.layout.root(), error)
        })?;
    let mut expected_staging = BTreeSet::new();
    for record_path in journal_paths(store)? {
        let record: InitializationRecord = read_json(&record_path)?;
        record.validate(store, &record_path)?;
        expected_staging.insert(record.staging_path().to_path_buf());
        reconcile_record(store, &root, &record_path, &record)?;
    }
    reject_orphaned_staging(store, &expected_staging)
}

fn reconcile_record(
    store: &Store,
    root: &Path,
    record_path: &Path,
    record: &InitializationRecord,
) -> Result<(), StoreError> {
    let staging = directory_state(record.staging_path())?;
    let final_state = directory_state(record.final_path())?;
    match (staging, final_state) {
        (PathState::Missing, PathState::Missing) => remove_json(record_path),
        (PathState::Directory, PathState::Missing) => {
            reconcile_staged(store, root, record_path, record)
        }
        (PathState::Missing, PathState::Directory) => {
            validate_final(store, root, record)?;
            remove_json(record_path)
        }
        (PathState::Directory, PathState::Directory) => {
            validate_final(store, root, record)?;
            ensure_real_contained_directory(record.staging_path(), root)?;
            remove_tree(record.staging_path())?;
            remove_json(record_path)
        }
        _ => Err(StoreError::InvalidOwnership {
            path: record_path.to_path_buf(),
            reason: "arena initialization path is not a real directory".to_owned(),
        }),
    }
}

fn reconcile_staged(
    store: &Store,
    root: &Path,
    record_path: &Path,
    record: &InitializationRecord,
) -> Result<(), StoreError> {
    ensure_real_contained_directory(record.staging_path(), root)?;
    let manifest_path = record.staging_path().join("arena.json");
    let Some(mut manifest): Option<ArenaManifest> = read_optional_json(&manifest_path)? else {
        remove_tree(record.staging_path())?;
        return remove_json(record_path);
    };
    manifest.validate(
        store.marker.store_id,
        record.arena_id(),
        manifest_path.clone(),
    )?;
    manifest.validate_initialization(record.initialization_id(), manifest_path.clone())?;
    if manifest.is_unfinished() {
        let measured = measure_tree(record.staging_path()).ok();
        let last_known = manifest.last_known_size.unwrap_or(ByteSize::ZERO);
        let high_water = measured
            .unwrap_or(last_known)
            .max(last_known)
            .max(manifest.last_observed_size);
        let outcome = manifest.recovery_outcome();
        manifest.finish(outcome, high_water, measured, unix_seconds()?)?;
        write_json(&manifest_path, &manifest)?;
    }
    fs::rename(record.staging_path(), record.final_path()).map_err(|error| {
        StoreError::io(
            "promote staged arena initialization",
            record.staging_path(),
            error,
        )
    })?;
    crate::io::sync_metadata_directory(record.final_path())?;
    remove_json(record_path)
}

fn validate_final(
    store: &Store,
    root: &Path,
    record: &InitializationRecord,
) -> Result<(), StoreError> {
    ensure_real_contained_directory(record.final_path(), root)?;
    let path = record.final_path().join("arena.json");
    let manifest: ArenaManifest = read_json(&path)?;
    manifest.validate(store.marker.store_id, record.arena_id(), path.clone())?;
    manifest.validate_initialization(record.initialization_id(), path)
}

fn reject_orphaned_staging(
    store: &Store,
    expected: &BTreeSet<std::path::PathBuf>,
) -> Result<(), StoreError> {
    for prefix in entry_paths(&store.layout.arenas())? {
        if !matches!(directory_state(&prefix)?, PathState::Directory) {
            continue;
        }
        for entry in entry_paths(&prefix)? {
            let staging = entry
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".init-"));
            if staging && !expected.contains(&entry) {
                return Err(StoreError::InvalidOwnership {
                    path: entry,
                    reason: "staged arena initialization has no external journal".to_owned(),
                });
            }
        }
    }
    Ok(())
}

fn journal_paths(store: &Store) -> Result<Vec<std::path::PathBuf>, StoreError> {
    entry_paths(&store.layout.initialization_index())?
        .into_iter()
        .filter_map(|path| {
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                Some(Ok(path))
            } else if is_json_publication_artifact(&path) {
                None
            } else {
                Some(Err(StoreError::InvalidOwnership {
                    path,
                    reason: "unexpected arena initialization journal entry".to_owned(),
                }))
            }
        })
        .collect()
}

fn entry_paths(path: &Path) -> Result<Vec<std::path::PathBuf>, StoreError> {
    let mut paths = fs::read_dir(path)
        .map_err(|error| StoreError::io("read initialization directory", path, error))?
        .map(|entry| {
            entry
                .map(|value| value.path())
                .map_err(|error| StoreError::io("read initialization entry", path, error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    Ok(paths)
}

#[derive(Clone, Copy)]
enum PathState {
    Missing,
    Directory,
    Other,
}

fn directory_state(path: &Path) -> Result<PathState, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Ok(PathState::Other)
        }
        Ok(_) => Ok(PathState::Directory),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(PathState::Missing),
        Err(error) => Err(StoreError::io(
            "inspect arena initialization path",
            path,
            error,
        )),
    }
}
