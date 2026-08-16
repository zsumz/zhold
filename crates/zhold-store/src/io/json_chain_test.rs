use serde::{Deserialize, Serialize};
use tempfile::tempdir;

use crate::StoreError;

use super::{
    create_json,
    json_file::{read_json, write_json_with_fault},
    json_publish::JsonPublication,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Document {
    revision: u64,
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
