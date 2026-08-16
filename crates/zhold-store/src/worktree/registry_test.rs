use std::fs;

use tempfile::tempdir;
use zhold_core::{WorktreeIntegrationState, WorktreeKey};

use super::{HookMetadata, registry};
use crate::{Store, WorktreeContext, io::sync_metadata_directory, test_support};

#[test]
fn backup_only_records_preserve_every_worktree_lifecycle_state()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let root = temporary.path().join("worktree");
    fs::create_dir(&root)?;
    let context = test_support::context(&root)?;
    let worktree = worktree_context(&context);
    store.hook_ready(&worktree, HookMetadata::default())?;

    rotate_to_backup(&store, &worktree.key)?;
    assert_state(&store, &worktree.key, WorktreeIntegrationState::Ready)?;

    store.hook_prepare_remove(&root, None)?;
    rotate_to_backup(&store, &worktree.key)?;
    assert_state(&store, &worktree.key, WorktreeIntegrationState::Draining)?;
    let invocation = test_support::invocation(&root)?;
    assert!(matches!(
        store.lease(&context, &invocation),
        Err(crate::StoreError::WorktreeAdmissionBlocked { .. })
    ));

    fs::remove_dir_all(&root)?;
    store.hook_removed(&root, None)?;
    rotate_to_backup(&store, &worktree.key)?;
    assert_state(&store, &worktree.key, WorktreeIntegrationState::Removed)?;
    Ok(())
}

#[test]
fn registry_ignores_only_valid_staging_and_reports_malformed_backups()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let directory = store.layout.worktree_integrations();
    fs::write(
        directory.join("candidate.json.00000000-0000-4000-8000-000000000001.new"),
        b"unpublished",
    )?;

    let clean = store.worktree_summary()?;
    assert_eq!(clean.registration_count, 0);
    assert_eq!(clean.finding_count, 0);

    fs::write(directory.join(".json.bak"), b"{}")?;
    let malformed = store.worktree_summary()?;
    assert_eq!(malformed.registration_count, 0);
    assert_eq!(malformed.finding_count, 1);
    Ok(())
}

fn rotate_to_backup(store: &Store, key: &WorktreeKey) -> Result<(), crate::StoreError> {
    let primary = store.layout.worktree_integration(key);
    fs::rename(&primary, primary.with_extension("json.bak"))
        .map_err(|error| crate::StoreError::io("rotate test worktree record", &primary, error))?;
    sync_metadata_directory(&primary)
}

fn assert_state(
    store: &Store,
    key: &WorktreeKey,
    expected: WorktreeIntegrationState,
) -> Result<(), crate::StoreError> {
    let record =
        registry::read(store, key)?.ok_or_else(|| crate::StoreError::InvalidOwnership {
            path: store.layout.worktree_integration(key),
            reason: "backup-only worktree record was not recovered".to_owned(),
        })?;
    assert_eq!(record.state, expected);
    Ok(())
}

fn worktree_context(context: &crate::BuildContext) -> WorktreeContext {
    let key = WorktreeKey::derive(&context.repository_id, &context.worktree_id);
    WorktreeContext {
        repository_id: context.repository_id.clone(),
        worktree_id: context.worktree_id.clone(),
        key,
        canonical_path: context.worktree_root.clone(),
        head: context.head.clone(),
    }
}
