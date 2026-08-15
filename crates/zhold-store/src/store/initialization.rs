use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};

use uuid::Uuid;

use crate::{
    StoreError,
    io::{configure_private_file, create_json, read_json, secure_directory},
    layout::StoreLayout,
    lock::ExclusiveFileLock,
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
            let marker = validate_marker(marker, marker_path)?;
            verify_filesystem_capabilities(layout.root())?;
            Ok(marker)
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
    if !contains_only_initialization_files(layout)? {
        return Err(StoreError::UnmarkedStore(layout.root().to_path_buf()));
    }
    let _initialization = ExclusiveFileLock::acquire(&layout.initialization_lock())?;
    if marker_path.exists() {
        let winner: StoreMarker = read_json(&marker_path)?;
        let winner = validate_marker(winner, marker_path)?;
        verify_filesystem_capabilities(layout.root())?;
        return Ok(winner);
    }
    cleanup_abandoned_staging(layout.root())?;
    if !contains_only_initialization_files(layout)? {
        return Err(StoreError::UnmarkedStore(layout.root().to_path_buf()));
    }
    verify_filesystem_capabilities(layout.root())?;
    let marker = StoreMarker::create();
    if create_json(&layout.marker(), &marker)? {
        Ok(marker)
    } else {
        let winner: StoreMarker = read_json(&layout.marker())?;
        validate_marker(winner, layout.marker())
    }
}

fn contains_only_initialization_files(layout: &StoreLayout) -> Result<bool, StoreError> {
    let entries = fs::read_dir(layout.root())
        .map_err(|error| StoreError::io("inspect store initialization", layout.root(), error))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            StoreError::io("inspect store initialization entry", layout.root(), error)
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| StoreError::io("inspect store initialization entry", &path, error))?;
        let recognized = path == layout.marker()
            || path == layout.initialization_lock()
            || is_marker_staging(&path)
            || is_capability_probe(&path);
        if !metadata.is_file() || !recognized {
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

fn is_capability_probe(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("store.probe."))
        .and_then(|name| name.rsplit_once('.'))
        .is_some_and(|(nonce, kind)| {
            matches!(kind, "source" | "link") && Uuid::parse_str(nonce).is_ok()
        })
}

fn cleanup_abandoned_staging(root: &Path) -> Result<(), StoreError> {
    let entries = fs::read_dir(root)
        .map_err(|error| StoreError::io("inspect abandoned store initialization", root, error))?;
    for entry in entries {
        let path = entry
            .map_err(|error| StoreError::io("inspect abandoned initialization entry", root, error))?
            .path();
        if is_marker_staging(&path) || is_capability_probe(&path) {
            fs::remove_file(&path).map_err(|error| {
                StoreError::io("remove abandoned store marker staging", &path, error)
            })?;
        }
    }
    Ok(())
}

fn verify_filesystem_capabilities(root: &Path) -> Result<(), StoreError> {
    let nonce = Uuid::new_v4();
    let source = root.join(format!("store.probe.{nonce}.source"));
    let link = root.join(format!("store.probe.{nonce}.link"));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    configure_private_file(&mut options);
    let file = options
        .open(&source)
        .map_err(|error| capability_error(root, error))?;
    drop(file);
    if let Err(error) = fs::hard_link(&source, &link) {
        let _cleanup = fs::remove_file(&source);
        return Err(capability_error(root, error));
    }
    fs::remove_file(&link)
        .map_err(|error| StoreError::io("remove filesystem capability link", &link, error))?;
    fs::remove_file(&source)
        .map_err(|error| StoreError::io("remove filesystem capability source", &source, error))
}

fn capability_error(root: &Path, source: std::io::Error) -> StoreError {
    StoreError::FilesystemCapability {
        path: root.to_path_buf(),
        capability: "same-directory hard-link publication",
        source: Box::new(source),
    }
}

pub(super) fn ensure_layout(layout: &StoreLayout) -> Result<(), StoreError> {
    for directory in [
        layout.arenas(),
        layout.locks(),
        layout.locks().join("arenas"),
        layout.locks().join("metadata"),
        layout.trash(),
        layout.trash_index(),
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
    secure_directory(path)?;
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
    secure_directory(path)
}
