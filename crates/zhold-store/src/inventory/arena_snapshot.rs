use std::{fs, path::Path};

use zhold_core::{ArenaLiveness, ArenaRecord, ArenaState, ByteSize, SizeQuality};

use super::uncertainty::{indexed_arena_id, recover_active_reservation};
use super::{InventoryEntry, InventoryFinding};
use crate::{
    Store, StoreError,
    inventory::ensure_real_contained_directory,
    io::{measure_tree, read_json},
    layout::StoreLayout,
    lock::{ExclusiveFileLock, LockState},
    manifest::ArenaManifest,
    time::unix_seconds,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArenaMeasurement {
    Cached,
    Deep,
}

pub(crate) struct ArenaSnapshot {
    pub(crate) root: std::path::PathBuf,
    pub(crate) observed_at: u64,
    pub(crate) total: ByteSize,
    pub(crate) protected: ByteSize,
    pub(crate) reserved: ByteSize,
    pub(crate) uncertain_owned: u64,
    pub(crate) arenas: Vec<InventoryEntry>,
    pub(crate) findings: Vec<InventoryFinding>,
}

pub(crate) fn read_arena_snapshot(
    store: &Store,
    measurement: ArenaMeasurement,
) -> Result<ArenaSnapshot, StoreError> {
    let layout = &store.layout;
    let observed_at = unix_seconds()?;
    let root = layout
        .root()
        .canonicalize()
        .map_err(|error| StoreError::io("canonicalize store root", layout.root(), error))?;
    let mut arenas = Vec::new();
    let mut findings = Vec::new();
    let mut uncertain_owned = 0_u64;
    let mut recovered_reservations = ByteSize::ZERO;

    for path in arena_paths(layout, &mut findings, &mut uncertain_owned)? {
        match read_entry(store, &root, &path, observed_at, measurement) {
            Ok(observation) => {
                if let Some(finding) = observation.finding {
                    uncertain_owned = uncertain_owned.saturating_add(1);
                    findings.push(finding);
                }
                arenas.push(observation.entry);
            }
            Err(error) => {
                uncertain_owned = uncertain_owned.saturating_add(1);
                if indexed_arena_id(layout, &path).is_some() {
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
    arenas.sort_by(|left, right| left.record.id.cmp(&right.record.id));

    let total = arenas.iter().fold(ByteSize::ZERO, |sum, entry| {
        sum.saturating_add(entry.record.size)
    });
    let protected = arenas
        .iter()
        .filter(|entry| {
            matches!(
                entry.record.state(),
                ArenaState::Active | ArenaState::Suspect | ArenaState::Pinned
            ) || matches!(
                entry.record.size_quality,
                SizeQuality::Stale | SizeQuality::Unknown
            )
        })
        .fold(ByteSize::ZERO, |sum, entry| {
            sum.saturating_add(entry.record.size)
        });
    let reserved = arenas
        .iter()
        .filter(|entry| !matches!(entry.record.liveness, ArenaLiveness::Inactive))
        .fold(recovered_reservations, |sum, entry| {
            sum.saturating_add(entry.reservation)
        });

    Ok(ArenaSnapshot {
        root,
        observed_at,
        total,
        protected,
        reserved,
        uncertain_owned,
        arenas,
        findings,
    })
}

fn arena_paths(
    layout: &StoreLayout,
    findings: &mut Vec<InventoryFinding>,
    uncertain_owned: &mut u64,
) -> Result<Vec<std::path::PathBuf>, StoreError> {
    let mut paths = Vec::new();
    let prefixes = fs::read_dir(layout.arenas())
        .map_err(|error| StoreError::io("read arena index", layout.arenas(), error))?;
    for prefix in prefixes {
        let prefix =
            prefix.map_err(|error| StoreError::io("read arena prefix", layout.arenas(), error))?;
        let prefix_path = prefix.path();
        if !is_valid_prefix(&prefix_path) {
            *uncertain_owned = uncertain_owned.saturating_add(1);
            findings.push(InventoryFinding {
                path: prefix_path,
                reason: "arena prefix is not two lowercase hexadecimal characters".to_owned(),
            });
            continue;
        }
        let metadata = fs::symlink_metadata(&prefix_path)
            .map_err(|error| StoreError::io("inspect arena prefix", &prefix_path, error))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            *uncertain_owned = uncertain_owned.saturating_add(1);
            findings.push(InventoryFinding {
                path: prefix_path,
                reason: "arena prefix is not a real directory".to_owned(),
            });
            continue;
        }
        let entries = fs::read_dir(&prefix_path)
            .map_err(|error| StoreError::io("read arena prefix", &prefix_path, error))?;
        for arena in entries {
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
    store: &Store,
    canonical_root: &Path,
    arena_path: &Path,
    observed_at: u64,
    measurement: ArenaMeasurement,
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
    let unfinished = manifest.is_unfinished();
    let (size, size_quality, finding) = observed_size(arena_path, &manifest, measurement);
    let pinned = manifest.is_pinned_at(observed_at);
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
                liveness: if active {
                    ArenaLiveness::Active
                } else if unfinished {
                    ArenaLiveness::Suspect
                } else {
                    ArenaLiveness::Inactive
                },
                pinned,
                worktree_exists: manifest.worktree_root.is_dir(),
                last_outcome: manifest.last_outcome,
            },
            workspace_root: manifest.workspace_root,
            branch: manifest.branch,
            head: manifest.head,
            cargo_version: manifest.cargo_version,
            command: manifest.command,
            reservation: if active || unfinished {
                manifest.reservation
            } else {
                ByteSize::ZERO
            },
            last_observed_size: manifest.last_observed_size,
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

fn observed_size(
    arena_path: &Path,
    manifest: &ArenaManifest,
    measurement: ArenaMeasurement,
) -> (ByteSize, SizeQuality, Option<InventoryFinding>) {
    if measurement == ArenaMeasurement::Cached {
        return if manifest.last_known_size > ByteSize::ZERO {
            (manifest.last_known_size, SizeQuality::Cached, None)
        } else {
            (
                ByteSize::ZERO,
                SizeQuality::Unknown,
                Some(InventoryFinding {
                    path: arena_path.to_path_buf(),
                    reason: "arena has no durable size observation".to_owned(),
                }),
            )
        };
    }
    match measure_tree(arena_path) {
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
    }
}
