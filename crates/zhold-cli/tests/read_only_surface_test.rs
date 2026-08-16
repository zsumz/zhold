//! Nonmutating command capability tests.

use std::{collections::BTreeMap, fs, io, path::Path, process::Command, time::UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use tempfile::tempdir;
use zhold_core::ByteSize;
use zhold_store::{Store, StoreConfig};

#[test]
fn read_only_commands_leave_every_store_entry_unchanged() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = tempdir()?;
    let store = temporary.path().join("store");
    initialize(&store)?;
    let expected = snapshot(&store)?;

    for arguments in read_only_commands() {
        let output = zhold(&store, temporary.path(), arguments)?;
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(snapshot(&store)?, expected, "{arguments:?} mutated store");
    }
    Ok(())
}

#[test]
fn read_only_commands_never_initialize_a_missing_store() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("missing");
    for arguments in [
        &["status"][..],
        &["doctor"][..],
        &["--budget", "1KiB", "gc", "--dry-run"][..],
    ] {
        let output = zhold(&store, temporary.path(), arguments)?;
        assert!(!output.status.success());
        assert!(!store.exists(), "{arguments:?} initialized the store");
    }
    Ok(())
}

#[test]
#[cfg(unix)]
fn read_only_commands_work_on_a_filesystem_read_only_store()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("store");
    initialize(&store)?;
    set_store_modes(&store, true)?;
    let outputs = read_only_commands()
        .into_iter()
        .map(|arguments| (arguments, zhold(&store, temporary.path(), arguments)))
        .collect::<Vec<_>>();
    set_store_modes(&store, false)?;

    for (arguments, output) in outputs {
        let output = output?;
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn initialize(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Store::open(root)?.set_config(StoreConfig {
        arena_budget: Some(ByteSize::from_bytes(1024)),
        ..StoreConfig::default()
    })?;
    Ok(())
}

fn read_only_commands() -> [&'static [&'static str]; 4] {
    [
        &["status"],
        &["status", "--deep"],
        &["doctor"],
        &["gc", "--dry-run"],
    ]
}

fn zhold(store: &Path, cwd: &Path, arguments: &[&str]) -> Result<std::process::Output, io::Error> {
    Command::new(env!("CARGO_BIN_EXE_zhold"))
        .arg("--store")
        .arg(store)
        .args(arguments)
        .current_dir(cwd)
        .output()
}

#[derive(Debug, Eq, PartialEq)]
struct SnapshotEntry {
    kind: u8,
    len: u64,
    modified_ns: u128,
    contents: Vec<u8>,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    mode: u32,
    #[cfg(unix)]
    links: u64,
}

fn snapshot(root: &Path) -> Result<BTreeMap<std::path::PathBuf, SnapshotEntry>, io::Error> {
    let mut result = BTreeMap::new();
    snapshot_into(root, root, &mut result)?;
    Ok(result)
}

fn snapshot_into(
    root: &Path,
    path: &Path,
    result: &mut BTreeMap<std::path::PathBuf, SnapshotEntry>,
) -> Result<(), io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    let kind = if metadata.is_dir() {
        1
    } else if metadata.is_file() {
        2
    } else {
        3
    };
    let contents = if metadata.is_file() {
        fs::read(path)?
    } else {
        Vec::new()
    };
    result.insert(
        path.strip_prefix(root).unwrap_or(path).to_path_buf(),
        SnapshotEntry {
            kind,
            len: metadata.len(),
            modified_ns: metadata
                .modified()?
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            contents,
            #[cfg(unix)]
            inode: metadata.ino(),
            #[cfg(unix)]
            mode: metadata.mode(),
            #[cfg(unix)]
            links: metadata.nlink(),
        },
    );
    if metadata.is_dir() {
        let mut children = fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        children.sort();
        for child in children {
            snapshot_into(root, &child, result)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_store_modes(root: &Path, read_only: bool) -> Result<(), io::Error> {
    let mut paths = snapshot(root)?.keys().cloned().collect::<Vec<_>>();
    paths.sort_by_key(|path| path.components().count());
    if read_only {
        paths.reverse();
    }
    for relative in paths {
        let path = root.join(relative);
        let metadata = fs::symlink_metadata(&path)?;
        let mode = match (metadata.is_dir(), read_only) {
            (true, true) => 0o500,
            (true, false) => 0o700,
            // zhold's private-file contract requires 0600 even when the directory namespace is
            // made non-writable to emulate a read-only store mount.
            (false, _) => 0o600,
        };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}
