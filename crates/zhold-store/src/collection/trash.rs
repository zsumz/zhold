use std::{fs, path::Path, str::FromStr};

use uuid::Uuid;
use zhold_core::{ArenaId, ByteSize};

use super::{TrashEntry, TrashOutcome, TrashReport};
use crate::{
    Store, StoreError,
    inventory::ensure_real_contained_directory,
    io::{measure_tree, read_json, remove_tree},
    lock::ExclusiveFileLock,
    manifest::ArenaManifest,
};

pub(crate) fn retry_trash(store: &Store, dry_run: bool) -> Result<TrashReport, StoreError> {
    let _collection_lock = ExclusiveFileLock::acquire(&store.layout.collection_lock())?;
    let root =
        store.layout.root().canonicalize().map_err(|error| {
            StoreError::io("canonicalize store root", store.layout.root(), error)
        })?;
    let trash = store.layout.trash();
    ensure_real_contained_directory(&trash, &root)?;
    let paths = entry_paths(&trash)?;
    let before = measure_paths(&paths);
    let mut reclaimed = ByteSize::ZERO;
    let mut entries = Vec::new();
    for path in paths {
        let attempt = inspect_entry(&path, &trash, &root, store.marker.store_id).and_then(
            |(arena_id, size)| {
                let Some(_arena_lock) =
                    ExclusiveFileLock::try_acquire(&store.layout.arena_lock(&arena_id))?
                else {
                    return Err(StoreError::ArenaActive(arena_id.to_string()));
                };
                if dry_run {
                    Ok((size, TrashOutcome::WouldDelete))
                } else {
                    remove_tree(&path)?;
                    reclaimed = reclaimed.saturating_add(size);
                    Ok((size, TrashOutcome::Deleted))
                }
            },
        );
        let (size, outcome) = match attempt {
            Ok(value) => value,
            Err(error) => (
                ByteSize::ZERO,
                TrashOutcome::Skipped {
                    error: error.to_string(),
                },
            ),
        };
        entries.push(TrashEntry {
            path,
            size,
            outcome,
        });
    }
    let remaining = if dry_run {
        let eligible = entries
            .iter()
            .filter(|entry| matches!(entry.outcome, TrashOutcome::WouldDelete))
            .fold(ByteSize::ZERO, |sum, entry| sum.saturating_add(entry.size));
        before.saturating_sub(eligible)
    } else {
        measure_paths(&entry_paths(&trash)?)
    };
    Ok(TrashReport {
        dry_run,
        before,
        reclaimed,
        remaining,
        entries,
        history: crate::HistoryWrite::default(),
    })
}

fn inspect_entry(
    path: &Path,
    trash: &Path,
    root: &Path,
    store_id: Uuid,
) -> Result<(ArenaId, ByteSize), StoreError> {
    let Some((arena_id, retirement_id)) = retired_identity(path) else {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "retirement entry name does not prove zhold ownership".to_owned(),
        });
    };
    if path.parent() != Some(trash) {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "retirement entry is not an immediate child of owned trash".to_owned(),
        });
    }
    ensure_real_contained_directory(path, root)?;
    let manifest_path = path.join("arena.json");
    let manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.validate(store_id, &arena_id, manifest_path)?;
    if manifest.retirement_id != Some(retirement_id) {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "retirement nonce does not match the owned manifest".to_owned(),
        });
    }
    ensure_real_contained_directory(&path.join("build"), root)?;
    Ok((arena_id, measure_tree(path)?))
}

fn retired_identity(path: &Path) -> Option<(ArenaId, Uuid)> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split_once('-'))
        .and_then(|(arena, nonce)| {
            ArenaId::from_str(arena)
                .ok()
                .zip(Uuid::parse_str(nonce).ok())
        })
}

fn entry_paths(path: &Path) -> Result<Vec<std::path::PathBuf>, StoreError> {
    let mut paths = fs::read_dir(path)
        .map_err(|error| StoreError::io("read retirement directory", path, error))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| StoreError::io("read retirement entry", path, error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    Ok(paths)
}

fn measure_paths(paths: &[std::path::PathBuf]) -> ByteSize {
    paths.iter().fold(ByteSize::ZERO, |sum, path| {
        measure_tree(path).map_or(sum, |size| sum.saturating_add(size))
    })
}
