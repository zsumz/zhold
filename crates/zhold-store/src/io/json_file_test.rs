use std::fs;

use serde::{Deserialize, Serialize};
use tempfile::tempdir;

use crate::StoreError;

use super::json_file::{backup_eligible, write_json_with_fault};
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
fn publication_faults_preserve_the_authoritative_commit_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        ("primary_rotated", false),
        ("primary_published", true),
        ("published_directory_sync", true),
        ("published_directory_synced", true),
        ("backup_removal", true),
        ("backup_removed", true),
        ("final_directory_sync", true),
    ];
    for (fault, committed) in cases {
        let temporary = tempdir()?;
        let path = temporary.path().join("state.json");
        let stable = document(7, "stable");
        let updated = document(8, "updated");
        create_json(&path, &stable)?;

        let publication = write_json_with_fault(&path, &updated, fault);
        if committed {
            let publication = publication?;
            assert!(publication.cleanup_warning().is_some(), "fault {fault}");
            assert_eq!(read_json::<Document>(&path)?, updated, "fault {fault}");
        } else {
            assert!(publication.is_err(), "fault {fault}");
            assert_eq!(read_json::<Document>(&path)?, stable, "fault {fault}");
        }
    }
    Ok(())
}

#[test]
fn backup_recovery_is_limited_to_absent_or_corrupt_primaries()
-> Result<(), Box<dyn std::error::Error>> {
    let Err(corrupt) = serde_json::from_slice::<Document>(b"{") else {
        return Err(std::io::Error::other("invalid JSON fixture decoded").into());
    };
    assert!(backup_eligible(&StoreError::io(
        "read metadata",
        "missing.json",
        std::io::Error::from(std::io::ErrorKind::NotFound),
    )));
    assert!(backup_eligible(&StoreError::json("corrupt.json", corrupt)));
    assert!(!backup_eligible(&StoreError::io(
        "read metadata",
        "denied.json",
        std::io::Error::from(std::io::ErrorKind::PermissionDenied),
    )));
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

#[cfg(unix)]
#[test]
fn tightens_existing_metadata_permissions() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let temporary = tempdir()?;
    let path = temporary.path().join("state.json");
    let stable = document(3, "stable");
    create_json(&path, &stable)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;

    assert_eq!(read_json::<Document>(&path)?, stable);
    assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
    Ok(())
}

fn document(revision: u64, value: &str) -> Document {
    Document {
        revision,
        value: value.to_owned(),
    }
}
