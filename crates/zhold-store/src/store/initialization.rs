use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use uuid::Uuid;

use crate::{
    StoreError,
    io::{create_json, read_json},
    layout::StoreLayout,
    manifest::StoreMarker,
};

pub(super) fn open_marker(layout: &StoreLayout) -> Result<StoreMarker, StoreError> {
    let marker_path = layout.marker();
    match fs::symlink_metadata(&marker_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(StoreError::InvalidOwnership {
                    path: marker_path,
                    reason: "store marker is not a real file".to_owned(),
                });
            }
            let marker: StoreMarker = read_json(&marker_path)?;
            validate_marker(marker, marker_path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => initialize_marker(layout),
        Err(error) => Err(StoreError::io("inspect store marker", marker_path, error)),
    }
}

fn validate_marker(marker: StoreMarker, path: PathBuf) -> Result<StoreMarker, StoreError> {
    if marker.schema_version == 1 {
        Ok(marker)
    } else {
        Err(StoreError::InvalidOwnership {
            path,
            reason: format!("unsupported store schema {}", marker.schema_version),
        })
    }
}

fn initialize_marker(layout: &StoreLayout) -> Result<StoreMarker, StoreError> {
    let marker_path = layout.marker();
    if marker_path.exists() {
        let winner: StoreMarker = read_json(&marker_path)?;
        return validate_marker(winner, marker_path);
    }

    let mut entries = fs::read_dir(layout.root())
        .map_err(|error| StoreError::io("inspect store root", layout.root(), error))?;
    let occupied = entries
        .next()
        .transpose()
        .map_err(|error| StoreError::io("inspect store root entry", layout.root(), error))?
        .is_some();
    if occupied {
        if layout.marker().exists() {
            let winner: StoreMarker = read_json(&layout.marker())?;
            return validate_marker(winner, layout.marker());
        }
        if contains_only_marker_staging(layout.root())? {
            return wait_for_marker(layout);
        }
        return Err(StoreError::UnmarkedStore(layout.root().to_path_buf()));
    }
    let marker = StoreMarker::create();
    if create_json(&layout.marker(), &marker)? {
        Ok(marker)
    } else {
        let winner: StoreMarker = read_json(&layout.marker())?;
        validate_marker(winner, layout.marker())
    }
}

fn contains_only_marker_staging(root: &Path) -> Result<bool, StoreError> {
    let entries = fs::read_dir(root)
        .map_err(|error| StoreError::io("inspect store initialization", root, error))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| StoreError::io("inspect store initialization entry", root, error))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| StoreError::io("inspect store initialization entry", &path, error))?;
        if !metadata.is_file() || !is_marker_staging(&path) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn is_marker_staging(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("store.json."))
        .and_then(|name| name.strip_suffix(".new"))
        .is_some_and(|value| Uuid::parse_str(value).is_ok())
}

fn wait_for_marker(layout: &StoreLayout) -> Result<StoreMarker, StoreError> {
    let marker_path = layout.marker();
    for _attempt in 0..100 {
        match fs::symlink_metadata(&marker_path) {
            Ok(_) => {
                let winner: StoreMarker = read_json(&marker_path)?;
                return validate_marker(winner, marker_path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(StoreError::io("inspect store marker", marker_path, error)),
        }
    }
    Err(StoreError::UnmarkedStore(layout.root().to_path_buf()))
}

pub(super) fn ensure_layout(layout: &StoreLayout) -> Result<(), StoreError> {
    for directory in [
        layout.arenas(),
        layout.locks(),
        layout.locks().join("arenas"),
        layout.locks().join("metadata"),
        layout.trash(),
        layout.history(),
        layout.history_receipts(),
        layout.integrations(),
        layout.worktree_integrations(),
        layout.worktree_locks(),
    ] {
        ensure_managed_directory(layout.root(), &directory)?;
    }
    Ok(())
}

pub(super) fn prepare_arena_root(layout: &StoreLayout, arena: &Path) -> Result<bool, StoreError> {
    let prefix = arena.parent().ok_or_else(|| StoreError::InvalidOwnership {
        path: arena.to_path_buf(),
        reason: "arena path has no prefix directory".to_owned(),
    })?;
    ensure_managed_directory(layout.root(), prefix)?;
    match fs::symlink_metadata(arena) {
        Ok(_) => {
            ensure_managed_directory(layout.root(), arena)?;
            Ok(false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(arena)
                .map_err(|source| StoreError::io("create managed arena", arena, source))?;
            ensure_managed_directory(layout.root(), arena)?;
            Ok(true)
        }
        Err(error) => Err(StoreError::io("inspect managed arena", arena, error)),
    }
}

pub(super) fn ensure_managed_directory(root: &Path, path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(StoreError::InvalidOwnership {
                    path: path.to_path_buf(),
                    reason: "managed store path is not a real directory".to_owned(),
                });
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => match fs::create_dir(path) {
            Ok(()) => {}
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(StoreError::io(
                    "create managed store directory",
                    path,
                    source,
                ));
            }
        },
        Err(error) => {
            return Err(StoreError::io(
                "inspect managed store directory",
                path,
                error,
            ));
        }
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| StoreError::io("canonicalize managed store directory", path, error))?;
    if canonical.starts_with(root) {
        Ok(())
    } else {
        Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "managed store directory escapes the store root".to_owned(),
        })
    }
}

pub(super) fn prepare_store_root(path: &Path) -> Result<(), StoreError> {
    fs::create_dir_all(path).map_err(|error| StoreError::io("create store root", path, error))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| StoreError::io("inspect store root", path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StoreError::InvalidOwnership {
            path: path.to_path_buf(),
            reason: "store root is not a real directory".to_owned(),
        });
    }
    Ok(())
}
