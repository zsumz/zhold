use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use zhold_core::ByteSize;

use super::{TrashEntry, TrashOutcome};
use crate::{
    Store, StoreError,
    inventory::ensure_real_contained_directory,
    io::{is_json_staging_path, measure_tree, read_json, remove_json, write_json},
    lock::{ExclusiveFileLock, LockState},
    manifest::{ArenaManifest, RetirementRecord},
};

pub(super) struct Reconciliation {
    pub(super) before: ByteSize,
    pub(super) reclaimed: ByteSize,
    pub(super) remaining: ByteSize,
    pub(super) entries: Vec<TrashEntry>,
}

pub(super) fn reconcile_locked(store: &Store, dry_run: bool) -> Result<Reconciliation, StoreError> {
    if !dry_run {
        store.ensure_writable("reconcile interrupted retirements")?;
    }
    let root = canonical_root(store)?;
    ensure_real_contained_directory(&store.layout.trash(), &root)?;
    ensure_real_contained_directory(&store.layout.trash_index(), &root)?;
    let trash_before = entry_paths(&store.layout.trash())?;
    let before = measure_paths(&trash_before)?;
    let mut owned_trash = BTreeSet::new();
    let mut entries = Vec::new();
    let mut reclaimed = ByteSize::ZERO;

    for record_path in entry_paths(&store.layout.trash_index())? {
        if is_json_staging_path(&record_path) {
            continue;
        }
        let attempt = read_json::<RetirementRecord>(&record_path).and_then(|record| {
            record.validate_journal(store, &record_path)?;
            owned_trash.insert(record.trash_path().to_path_buf());
            reconcile_record(store, &root, &record_path, &record, dry_run)
        });
        match attempt {
            Ok((entry, bytes)) => {
                reclaimed = reclaimed.saturating_add(bytes);
                entries.push(entry);
            }
            Err(error) => entries.push(skipped(record_path, &error)),
        }
    }
    for path in trash_before {
        if !owned_trash.contains(&path) {
            entries.push(TrashEntry {
                path,
                size: ByteSize::ZERO,
                outcome: TrashOutcome::Skipped {
                    error: "trash entry has no valid external retirement journal".to_owned(),
                },
            });
        }
    }
    entries.extend(repair_orphaned_intents(store, &root, dry_run)?);
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let remaining = if dry_run {
        before
    } else {
        measure_paths(&entry_paths(&store.layout.trash())?)?
    };
    Ok(Reconciliation {
        before,
        reclaimed,
        remaining,
        entries,
    })
}

fn reconcile_record(
    store: &Store,
    root: &Path,
    record_path: &Path,
    record: &RetirementRecord,
    dry_run: bool,
) -> Result<(TrashEntry, ByteSize), StoreError> {
    let original = path_state(record.original_path())?;
    let trash = path_state(record.trash_path())?;
    match (original, trash) {
        (PathState::Missing, PathState::Missing) => {
            if !dry_run {
                remove_json(record_path)?;
            }
            Ok((repair_entry(record_path, dry_run), ByteSize::ZERO))
        }
        (PathState::Directory, PathState::Missing) => {
            super::trash::resume_retirement(store, root, record_path, record, dry_run)
        }
        (PathState::Missing, PathState::Directory) => {
            super::trash::delete_retired(store, root, record_path, record, dry_run)
        }
        (PathState::Directory, PathState::Directory) => {
            if original_matches_record(store, root, record)? {
                Err(StoreError::InvalidOwnership {
                    path: record_path.to_path_buf(),
                    reason: "retirement has both its original and trash directory".to_owned(),
                })
            } else {
                super::trash::delete_retired(store, root, record_path, record, dry_run)
            }
        }
        _ => Err(StoreError::InvalidOwnership {
            path: record_path.to_path_buf(),
            reason: "retirement path was replaced by a non-directory or symbolic link".to_owned(),
        }),
    }
}

fn original_matches_record(
    store: &Store,
    root: &Path,
    record: &RetirementRecord,
) -> Result<bool, StoreError> {
    ensure_real_contained_directory(record.original_path(), root)?;
    let manifest_path = store.layout.manifest(record.arena_id());
    let manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.validate(store.marker.store_id, record.arena_id(), manifest_path)?;
    Ok(manifest.retirement_id == Some(record.retirement_id())
        && manifest.revision == record.retired_revision())
}

fn repair_orphaned_intents(
    store: &Store,
    root: &Path,
    dry_run: bool,
) -> Result<Vec<TrashEntry>, StoreError> {
    let mut repaired = Vec::new();
    for manifest_path in manifest_paths(store)? {
        let manifest: ArenaManifest = match read_json(&manifest_path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(retirement_id) = manifest.retirement_id else {
            continue;
        };
        if store.layout.retirement_record(retirement_id).exists() {
            continue;
        }
        let arena_id = manifest.arena_id.clone();
        if dry_run {
            if store.probe_lock(&store.layout.arena_lock(&arena_id))? == LockState::Held {
                continue;
            }
            let mut current: ArenaManifest = read_json(&manifest_path)?;
            current.validate(store.marker.store_id, &arena_id, manifest_path.clone())?;
            ensure_real_contained_directory(&store.layout.arena(&arena_id), root)?;
            if current.clear_retirement(retirement_id) {
                repaired.push(repair_entry(&manifest_path, true));
            }
            continue;
        }
        store.ensure_writable("repair an interrupted retirement")?;
        let Some(_arena) = ExclusiveFileLock::try_acquire(&store.layout.arena_lock(&arena_id))?
        else {
            continue;
        };
        let _metadata = ExclusiveFileLock::acquire(&store.layout.metadata_lock(&arena_id))?;
        let mut current: ArenaManifest = read_json(&manifest_path)?;
        current.validate(store.marker.store_id, &arena_id, manifest_path.clone())?;
        ensure_real_contained_directory(&store.layout.arena(&arena_id), root)?;
        if !current.clear_retirement(retirement_id) {
            continue;
        }
        write_json(&manifest_path, &current)?;
        repaired.push(repair_entry(&manifest_path, false));
    }
    Ok(repaired)
}

#[derive(Clone, Copy)]
enum PathState {
    Missing,
    Directory,
    Other,
}

fn path_state(path: &Path) -> Result<PathState, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Ok(PathState::Other)
        }
        Ok(_) => Ok(PathState::Directory),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(PathState::Missing),
        Err(error) => Err(StoreError::io("inspect retirement path", path, error)),
    }
}

fn manifest_paths(store: &Store) -> Result<Vec<PathBuf>, StoreError> {
    let mut paths = Vec::new();
    for prefix in entry_paths(&store.layout.arenas())? {
        if !matches!(path_state(&prefix)?, PathState::Directory) {
            continue;
        }
        paths.extend(
            entry_paths(&prefix)?
                .into_iter()
                .map(|arena| arena.join("arena.json")),
        );
    }
    paths.sort();
    Ok(paths)
}

fn entry_paths(path: &Path) -> Result<Vec<PathBuf>, StoreError> {
    let mut paths = fs::read_dir(path)
        .map_err(|error| StoreError::io("read managed directory", path, error))?
        .map(|entry| {
            entry
                .map(|value| value.path())
                .map_err(|error| StoreError::io("read managed entry", path, error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    Ok(paths)
}

fn measure_paths(paths: &[PathBuf]) -> Result<ByteSize, StoreError> {
    paths.iter().try_fold(ByteSize::ZERO, |sum, path| {
        measure_tree(path).map(|size| sum.saturating_add(size))
    })
}

fn canonical_root(store: &Store) -> Result<PathBuf, StoreError> {
    store
        .layout
        .root()
        .canonicalize()
        .map_err(|error| StoreError::io("canonicalize store root", store.layout.root(), error))
}

fn repair_entry(path: &Path, dry_run: bool) -> TrashEntry {
    TrashEntry {
        path: path.to_path_buf(),
        size: ByteSize::ZERO,
        outcome: if dry_run {
            TrashOutcome::WouldRepair
        } else {
            TrashOutcome::Repaired
        },
    }
}

fn skipped(path: PathBuf, error: &StoreError) -> TrashEntry {
    TrashEntry {
        path,
        size: ByteSize::ZERO,
        outcome: TrashOutcome::Skipped {
            error: error.to_string(),
        },
    }
}
