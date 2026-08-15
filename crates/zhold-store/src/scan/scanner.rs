use std::{fs, path::PathBuf};

use super::{ForeignTarget, ScanReport};
use crate::{InventoryFinding, Store, StoreError, io::measure_tree};

const MAX_SCAN_DEPTH: usize = 12;

pub(crate) fn scan(store: &Store, roots: &[PathBuf]) -> Result<ScanReport, StoreError> {
    let managed = store.inventory()?;
    let mut foreign_targets = Vec::new();
    let mut findings = Vec::new();
    let canonical_store =
        store.layout.root().canonicalize().map_err(|error| {
            StoreError::io("canonicalize store root", store.layout.root(), error)
        })?;

    for root in roots {
        let canonical = match root.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                findings.push(InventoryFinding {
                    path: root.clone(),
                    reason: error.to_string(),
                });
                continue;
            }
        };
        visit(
            &canonical,
            0,
            &canonical_store,
            &mut foreign_targets,
            &mut findings,
        );
    }
    foreign_targets.sort_by(|left, right| left.path.cmp(&right.path));
    foreign_targets.dedup_by(|left, right| left.path == right.path);

    Ok(ScanReport {
        managed,
        foreign_targets,
        findings,
    })
}

fn visit(
    path: &std::path::Path,
    depth: usize,
    store_root: &std::path::Path,
    targets: &mut Vec<ForeignTarget>,
    findings: &mut Vec<InventoryFinding>,
) {
    if depth > MAX_SCAN_DEPTH || path.starts_with(store_root) {
        return;
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) => {
            findings.push(InventoryFinding {
                path: path.to_path_buf(),
                reason: error.to_string(),
            });
            return;
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return;
    }
    if is_cargo_target(path) {
        match measure_tree(path) {
            Ok(size) => targets.push(ForeignTarget {
                path: path.to_path_buf(),
                size,
            }),
            Err(error) => findings.push(InventoryFinding {
                path: path.to_path_buf(),
                reason: error.to_string(),
            }),
        }
        return;
    }
    if should_skip(path) {
        return;
    }

    let entries = match fs::read_dir(path) {
        Ok(value) => value,
        Err(error) => {
            findings.push(InventoryFinding {
                path: path.to_path_buf(),
                reason: error.to_string(),
            });
            return;
        }
    };
    for entry in entries {
        match entry {
            Ok(value) => visit(&value.path(), depth + 1, store_root, targets, findings),
            Err(error) => findings.push(InventoryFinding {
                path: path.to_path_buf(),
                reason: error.to_string(),
            }),
        }
    }
}

fn is_cargo_target(path: &std::path::Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("target")
        && path.join(".rustc_info.json").is_file()
}

fn should_skip(path: &std::path::Path) -> bool {
    path.file_name().is_some_and(|name| {
        matches!(
            name.to_str(),
            Some(".git" | "node_modules" | ".direnv" | ".venv")
        )
    })
}
