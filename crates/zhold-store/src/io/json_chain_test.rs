use std::fs;

use serde::{Deserialize, Serialize};
use tempfile::tempdir;

use crate::StoreError;

use super::{
    create_json,
    json_document::{JsonDocumentState, json_document_state, read_optional_json, upsert_json},
    json_file::{read_json, write_json_with_fault},
    json_publish::JsonPublication,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Document {
    revision: u64,
}

#[test]
fn logical_document_state_and_upsert_include_backup_only_documents()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let path = temporary.path().join("state.json");
    let first = Document { revision: 1 };
    let second = Document { revision: 2 };

    assert_eq!(json_document_state(&path)?, JsonDocumentState::Absent);
    assert_eq!(read_optional_json::<Document>(&path)?, None);
    upsert_json(&path, &first)?;
    assert_eq!(json_document_state(&path)?, JsonDocumentState::Primary);

    super::json_recovery_test::rotate_to_backup(&path)?;
    assert_eq!(json_document_state(&path)?, JsonDocumentState::BackupOnly);
    assert_eq!(read_optional_json::<Document>(&path)?, Some(first));
    upsert_json(&path, &second)?;
    assert_eq!(read_optional_json::<Document>(&path)?, Some(second));
    assert_eq!(json_document_state(&path)?, JsonDocumentState::Primary);

    fs::copy(&path, path.with_extension("json.bak"))?;
    assert_eq!(
        json_document_state(&path)?,
        JsonDocumentState::PrimaryWithBackup
    );
    Ok(())
}

#[test]
fn optional_reads_fail_closed_for_invalid_backup_only_candidates()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let path = temporary.path().join("state.json");
    let backup = path.with_extension("json.bak");
    assert!(create_json(&path, &Document { revision: 1 })?);
    super::json_recovery_test::rotate_to_backup(&path)?;
    fs::write(&backup, b"{")?;
    assert!(matches!(
        read_optional_json::<Document>(&path),
        Err(StoreError::Json { .. })
    ));

    fs::remove_file(&backup)?;
    fs::create_dir(&backup)?;
    assert!(matches!(
        read_optional_json::<Document>(&path),
        Err(StoreError::InvalidOwnership { .. })
    ));
    Ok(())
}

#[test]
fn chained_write_preserves_a_valid_generation_at_every_fault()
-> Result<(), Box<dyn std::error::Error>> {
    let faults = [
        "primary_with_backup_stabilized",
        "previous_backup_removal",
        "previous_backup_removed",
        "previous_backup_removal_synced",
        "primary_rotated",
        "primary_published",
        "published_directory_sync",
        "published_directory_synced",
        "backup_removal",
        "backup_removed",
        "final_directory_sync",
    ];
    for fault in faults {
        let temporary = tempdir()?;
        let path = temporary.path().join("state.json");
        let first = Document { revision: 1 };
        let second = Document { revision: 2 };
        let third = Document { revision: 3 };
        assert!(create_json(&path, &first)?);

        let uncertain = write_json_with_fault(&path, &second, "published_directory_sync")?;
        assert!(matches!(
            uncertain,
            JsonPublication::VisibleButDurabilityUnconfirmed {
                error: StoreError::MetadataDurabilityUnconfirmed { .. }
            }
        ));
        assert_eq!(read_json::<Document>(&path)?, second);
        assert!(path.with_extension("json.bak").exists());

        let _publication = write_json_with_fault(&path, &third, fault);
        let recovered = read_json::<Document>(&path)?;
        assert!(
            [first.clone(), second.clone(), third.clone()].contains(&recovered),
            "fault {fault} recovered {recovered:?}"
        );
    }
    Ok(())
}
