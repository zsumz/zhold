use std::{fs, path::Path};

use zhold_core::{ArenaRecord, ArenaState, ByteSize, SizeQuality};

use super::uncertainty::{indexed_arena_id, recover_active_reservation};
use super::{Inventory, InventoryEntry, InventoryFinding};
use crate::{
    StoreError,
    io::{measure_tree, read_json},
    layout::StoreLayout,
    lock::{ExclusiveFileLock, LockState},
    manifest::ArenaManifest,
    time::unix_seconds,
};

pub(crate) fn read_inventory(store: &crate::Store) -> Result<Inventory, StoreError> {
    let layout = &store.layout;
    let store_id = store.marker.store_id;
    let observed_at = unix_seconds()?;
    let root = layout
        .root()
        .canonicalize()
        .map_err(|error| StoreError::io("canonicalize store root", layout.root(), error))?;
    let mut entries = Vec::new();
    let mut findings = Vec::new();
    let mut uncertain_owned = 0_u64;
    let mut recovered_reservations = ByteSize::ZERO;

    for path in arena_paths(layout, &mut findings)? {
        match read_entry(store, &root, &path, observed_at) {
            Ok(observation) => {
                if let Some(finding) = observation.finding {
                    uncertain_owned = uncertain_owned.saturating_add(1);
                    findings.push(finding);
                }
                entries.push(observation.entry);
            }
            Err(error) => {
                if indexed_arena_id(layout, &path).is_some() {
                    uncertain_owned = uncertain_owned.saturating_add(1);
                    recovered_reservations = recovered_reservations
                        .saturating_add(recover_active_reservation(store, &path));
                }
                findings.push(InventoryFinding {
                    path,
                    reason: error.to_string(),
                });
            }
        }
    }
    entries.sort_by(|left, right| left.record.id.cmp(&right.record.id));

    let total = entries.iter().fold(ByteSize::ZERO, |sum, entry| {
        sum.saturating_add(entry.record.size)
    });
    let protected = entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.record.state(),
                ArenaState::Active | ArenaState::Pinned
            ) || entry.record.size_quality != SizeQuality::Fresh
        })
        .fold(ByteSize::ZERO, |sum, entry| {
            sum.saturating_add(entry.record.size)
        });
    let reserved = entries
        .iter()
        .filter(|entry| entry.record.active)
        .fold(recovered_reservations, |sum, entry| {
            sum.saturating_add(entry.reservation)
        });
    let trash_path = layout.trash();
    ensure_real_contained_directory(&trash_path, &root)?;
    let trash = measure_directory_contents(&trash_path)?;
    let physical = measure_tree(layout.root())?;
    let available =
        ByteSize::from_bytes(fs2::available_space(layout.root()).map_err(|error| {
            StoreError::io("measure available store space", layout.root(), error)
        })?);
    let history = crate::history::summary(store)?;
    let worktrees = store.worktree_summary()?;
    let (quota, quota_finding) = match store.quota_status(zhold_core::QuotaProvider::Auto) {
        Ok(status) => (Some(status), None),
        Err(error @ (StoreError::InvalidOwnership { .. } | StoreError::Json { .. })) => {
            (None, Some(error.to_string()))
        }
        Err(error) => return Err(error),
    };

    Ok(Inventory {
        store_id,
        store_root: root,
        observed_at,
        total,
        protected,
        reserved,
        uncertain_owned,
        trash,
        physical,
        available,
        history,
        worktrees,
        quota,
        quota_finding,
        arenas: entries,
        findings,
    })
}

fn arena_paths(
    layout: &StoreLayout,
    findings: &mut Vec<InventoryFinding>,
) -> Result<Vec<std::path::PathBuf>, StoreError> {
    let mut paths = Vec::new();
    let prefixes = fs::read_dir(layout.arenas())
        .map_err(|error| StoreError::io("read arena index", layout.arenas(), error))?;
    for prefix in prefixes {
        let prefix =
            prefix.map_err(|error| StoreError::io("read arena prefix", layout.arenas(), error))?;
        let prefix_path = prefix.path();
        if !is_valid_prefix(&prefix_path) {
            findings.push(InventoryFinding {
                path: prefix_path,
                reason: "arena prefix is not two lowercase hexadecimal characters".to_owned(),
            });
            continue;
        }
        let metadata = fs::symlink_metadata(&prefix_path)
            .map_err(|error| StoreError::io("inspect arena prefix", &prefix_path, error))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            findings.push(InventoryFinding {
                path: prefix_path,
                reason: "arena prefix is not a real directory".to_owned(),
            });
            continue;
        }
        let arenas = fs::read_dir(&prefix_path)
            .map_err(|error| StoreError::io("read arena prefix", &prefix_path, error))?;
        for arena in arenas {
            let arena =
                arena.map_err(|error| StoreError::io("read arena entry", &prefix_path, error))?;
            paths.push(arena.path());
        }
    }
    Ok(paths)
}

fn is_valid_prefix(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.len() == 2
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

struct EntryObservation {
    entry: InventoryEntry,
    finding: Option<InventoryFinding>,
}

fn read_entry(
    store: &crate::Store,
    canonical_root: &Path,
    arena_path: &Path,
    observed_at: u64,
) -> Result<EntryObservation, StoreError> {
    let layout = &store.layout;
    ensure_real_contained_directory(arena_path, canonical_root)?;
    let arena_id =
        indexed_arena_id(layout, arena_path).ok_or_else(|| StoreError::InvalidOwnership {
            path: arena_path.to_path_buf(),
            reason: "arena path does not contain its indexed identity".to_owned(),
        })?;

    let manifest_path = layout.manifest(&arena_id);
    let manifest: ArenaManifest = read_json(&manifest_path)?;
    manifest.validate(store.marker.store_id, &arena_id, manifest_path)?;
    let build_dir = layout.build_dir(&arena_id);
    ensure_real_contained_directory(&build_dir, canonical_root)?;
    let active = matches!(
        ExclusiveFileLock::probe(&layout.arena_lock(&arena_id))?,
        LockState::Held
    );
    let pinned = manifest.is_pinned_at(observed_at);
    let (size, size_quality, finding) = match measure_tree(arena_path) {
        Ok(size) => (size, SizeQuality::Fresh, None),
        Err(error) if manifest.last_known_size > ByteSize::ZERO => (
            manifest.last_known_size,
            SizeQuality::Stale,
            Some(InventoryFinding {
                path: arena_path.to_path_buf(),
                reason: format!("current measurement failed; using last known size: {error}"),
            }),
        ),
        Err(error) => (
            ByteSize::ZERO,
            SizeQuality::Unknown,
            Some(InventoryFinding {
                path: arena_path.to_path_buf(),
                reason: format!("current measurement failed with no known size: {error}"),
            }),
        ),
    };

    let integration =
        crate::worktree::read_for_ids(store, &manifest.repository_id, &manifest.worktree_id)?;
    Ok(EntryObservation {
        finding,
        entry: InventoryEntry {
            revision: manifest.revision,
            record: ArenaRecord {
                id: arena_id,
                repository_id: manifest.repository_id,
                worktree_id: manifest.worktree_id,
                workspace_id: manifest.workspace_id,
                toolchain_id: manifest.toolchain_id,
                worktree_root: manifest.worktree_root.clone(),
                build_dir,
                size,
                size_quality,
                created_at: manifest.created_at,
                last_used_at: manifest.last_used_at,
                active,
                pinned,
                worktree_exists: manifest.worktree_root.is_dir(),
                last_outcome: manifest.last_outcome,
            },
            workspace_root: manifest.workspace_root,
            branch: manifest.branch,
            head: manifest.head,
            cargo_version: manifest.cargo_version,
            command: manifest.command,
            reservation: if active {
                manifest.reservation
            } else {
                ByteSize::ZERO
            },
            last_peak: manifest.last_peak,
            pin_expires_at: manifest.pin_expires_at,
            worktree_state: integration.as_ref().map(|record| record.state),
            manager: integration
                .as_ref()
                .and_then(|record| record.manager.clone()),
            label: integration.as_ref().and_then(|record| record.label.clone()),
            session: integration.and_then(|record| record.session),
        },
    })
}

fn measure_directory_contents(path: &Path) -> Result<ByteSize, StoreError> {
    let mut entries = fs::read_dir(path)
        .map_err(|error| StoreError::io("read managed directory", path, error))?;
    entries.try_fold(ByteSize::ZERO, |total, entry| {
        let entry = entry.map_err(|error| StoreError::io("read managed entry", path, error))?;
        Ok(total.saturating_add(measure_tree(&entry.path())?))
    })
}

pub(crate) fn ensure_real_contained_directory(
    path: &Path,
    canonical_root: &Path,
) -> Result<(), StoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| StoreError::io("inspect managed directory", path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "managed path is not a real directory".to_owned(),
        });
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| StoreError::io("canonicalize managed directory", path, error))?;
    if !canonical.starts_with(canonical_root) {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "managed path escapes the marked store root".to_owned(),
        });
    }
    Ok(())
}
