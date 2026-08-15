#[cfg(unix)]
use std::fs;

#[cfg(unix)]
use tempfile::tempdir;

#[cfg(unix)]
use super::{measure_tree, remove_tree};

#[cfg(unix)]
#[test]
fn recursive_deletion_removes_a_symlink_without_following_it()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{MetadataExt, symlink};

    let temporary = tempdir()?;
    let owned = temporary.path().join("owned");
    let outside = temporary.path().join("outside");
    fs::create_dir(&owned)?;
    fs::create_dir(&outside)?;
    fs::write(outside.join("keep.txt"), b"keep")?;
    symlink(&outside, owned.join("link"))?;

    let measured = measure_tree(&owned)?;
    let expected_blocks =
        fs::symlink_metadata(&owned)?.blocks() + fs::symlink_metadata(owned.join("link"))?.blocks();
    remove_tree(&owned)?;

    assert_eq!(measured.as_u64(), expected_blocks.saturating_mul(512));
    assert!(!owned.exists());
    assert!(outside.join("keep.txt").is_file());
    Ok(())
}
