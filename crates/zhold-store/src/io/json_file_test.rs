use std::fs;

use serde::{Deserialize, Serialize};
use tempfile::tempdir;

#[cfg(unix)]
use crate::StoreError;

use super::{create_json, read_json, write_json};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Document {
    revision: u64,
    value: String,
}

#[test]
fn writes_and_reads_an_atomic_metadata_document() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let path = temporary.path().join("state.json");
    let initial = document(1, "initial");
    let updated = document(2, "updated");

    assert!(create_json(&path, &initial)?);
    assert!(!create_json(&path, &initial)?);
    write_json(&path, &updated)?;

    assert_eq!(read_json::<Document>(&path)?, updated);
    assert!(!path.with_extension("json.bak").exists());
    Ok(())
}

#[test]
fn recovers_from_a_torn_primary_using_the_last_valid_backup()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let path = temporary.path().join("state.json");
    let backup = path.with_extension("json.bak");
    let stable = document(7, "stable");
    create_json(&path, &stable)?;
    fs::copy(&path, &backup)?;
    fs::write(&path, b"{\"revision\":")?;

    assert_eq!(read_json::<Document>(&path)?, stable);
    Ok(())
}

#[test]
fn publishes_without_discarding_a_recoverable_backup() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let path = temporary.path().join("state.json");
    let backup = path.with_extension("json.bak");
    let stable = document(7, "stable");
    let updated = document(8, "updated");
    create_json(&path, &stable)?;
    fs::rename(&path, &backup)?;

    assert_eq!(read_json::<Document>(&path)?, stable);
    write_json(&path, &updated)?;

    assert_eq!(read_json::<Document>(&path)?, updated);
    assert!(!backup.exists());
    Ok(())
}

#[test]
fn ignores_unpublished_staging_files() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let path = temporary.path().join("state.json");
    let stable = document(3, "stable");
    create_json(&path, &stable)?;
    fs::write(
        temporary.path().join("state.json.abandoned.new"),
        b"garbage",
    )?;

    assert_eq!(read_json::<Document>(&path)?, stable);
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_a_symlink_metadata_file() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let temporary = tempdir()?;
    let outside = temporary.path().join("outside.json");
    let path = temporary.path().join("state.json");
    fs::write(&outside, b"{}")?;
    symlink(&outside, &path)?;

    assert!(matches!(
        read_json::<Document>(&path),
        Err(StoreError::InvalidOwnership { .. })
    ));
    assert!(matches!(
        write_json(&path, &document(1, "blocked")),
        Err(StoreError::InvalidOwnership { .. })
    ));
    Ok(())
}

fn document(revision: u64, value: &str) -> Document {
    Document {
        revision,
        value: value.to_owned(),
    }
}
