use std::fs;

use tempfile::tempdir;
use zhold_core::{BuildOutcome, HookResult, WorktreeIntegrationState, WorktreeKey};

use super::HookMetadata;
use crate::{Store, WorktreeContext, test_support};

#[test]
fn removal_gate_blocks_active_and_future_builds() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let root = temporary.path().join("worktree");
    fs::create_dir(&root)?;
    let context = test_support::context(&root)?;
    let worktree = worktree_context(&context);
    let ready = store.hook_ready(
        &worktree,
        HookMetadata {
            manager: Some("worktrunk".to_owned()),
            label: Some("feature/receipt-history".to_owned()),
            session: Some("session-1".to_owned()),
        },
    )?;
    assert_eq!(ready.result, HookResult::Changed);

    let invocation = test_support::invocation(&root)?;
    let lease = store.lease(&context, &invocation)?;
    let active = store.hook_prepare_remove(&root, Some("worktrunk".to_owned()))?;
    assert_eq!(active.result, HookResult::ActiveBuild);
    assert_eq!(active.resulting, Some(WorktreeIntegrationState::Ready));
    lease.finish(BuildOutcome::Succeeded)?;

    let draining = store.hook_prepare_remove(&root, Some("worktrunk".to_owned()))?;
    assert_eq!(draining.resulting, Some(WorktreeIntegrationState::Draining));
    let blocked = store.lease(&context, &invocation);
    assert!(matches!(
        blocked,
        Err(crate::StoreError::WorktreeAdmissionBlocked { .. })
    ));
    let summary = store.worktree_summary()?;
    assert_eq!(summary.draining_count, 1);
    assert_eq!(summary.recovery.len(), 1);

    let cancelled = store.hook_cancel_remove(&worktree, None)?;
    assert_eq!(cancelled.resulting, Some(WorktreeIntegrationState::Ready));
    store
        .lease(&context, &invocation)?
        .finish(BuildOutcome::Succeeded)?;
    Ok(())
}

#[test]
fn removed_requires_absence_and_preserves_history() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let root = temporary.path().join("worktree");
    fs::create_dir(&root)?;
    let context = test_support::context(&root)?;
    let worktree = worktree_context(&context);
    store.hook_ready(&worktree, HookMetadata::default())?;
    store.hook_prepare_remove(&root, None)?;

    let present = store.hook_removed(&root, None)?;
    assert_eq!(present.result, HookResult::Attention);
    assert_eq!(present.resulting, Some(WorktreeIntegrationState::Draining));
    fs::remove_dir_all(&root)?;
    let removed = store.hook_removed(&root, None)?;
    assert_eq!(removed.resulting, Some(WorktreeIntegrationState::Removed));

    let history = store.history(&crate::HistoryQuery {
        kind: Some(zhold_core::HistoryKind::Hook),
        ..crate::HistoryQuery::default()
    })?;
    assert_eq!(history.receipts.len(), 4);
    Ok(())
}

#[test]
fn metadata_is_byte_bounded_and_aliases_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let root = temporary.path().join("worktree");
    fs::create_dir(&root)?;
    let context = test_support::context(&root)?;
    let worktree = worktree_context(&context);
    let oversized = store.hook_ready(
        &worktree,
        HookMetadata {
            manager: None,
            label: Some("🦀".repeat(65)),
            session: None,
        },
    );
    assert!(matches!(
        oversized,
        Err(crate::StoreError::InvalidHookValue { field: "label", .. })
    ));
    assert!(matches!(
        store.hook_ready(
            &worktree,
            HookMetadata {
                manager: Some("forged\nmanager".to_owned()),
                label: None,
                session: None,
            }
        ),
        Err(crate::StoreError::InvalidHookValue {
            field: "manager",
            ..
        })
    ));

    store.hook_ready(&worktree, HookMetadata::default())?;
    let mut alias = worktree;
    alias.repository_id = zhold_core::RepositoryId::derive("another repository");
    alias.key = WorktreeKey::derive(&alias.repository_id, &alias.worktree_id);
    assert!(matches!(
        store.hook_ready(&alias, HookMetadata::default()),
        Err(crate::StoreError::InvalidOwnership { .. })
    ));
    Ok(())
}

#[test]
fn a_reused_path_selects_the_new_live_registration() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let root = temporary.path().join("worktree");
    fs::create_dir(&root)?;
    let context = test_support::context(&root)?;
    let old = worktree_context(&context);
    store.hook_ready(&old, HookMetadata::default())?;
    store.hook_prepare_remove(&root, None)?;
    fs::remove_dir_all(&root)?;
    store.hook_removed(&root, None)?;

    fs::create_dir(&root)?;
    let mut replacement = old;
    replacement.repository_id = zhold_core::RepositoryId::derive("replacement repository");
    replacement.key = WorktreeKey::derive(&replacement.repository_id, &replacement.worktree_id);
    store.hook_ready(&replacement, HookMetadata::default())?;

    let draining = store.hook_prepare_remove(&root, None)?;
    assert_eq!(draining.result, HookResult::Changed);
    assert_eq!(
        draining
            .integration
            .as_ref()
            .map(|value| &value.worktree_key),
        Some(&replacement.key)
    );
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
