use std::{fs, path::Path};

use zhold_core::{HookEvent, HookResult, WorktreeIntegrationState};

use super::{HookMetadata, HookReport, WorktreeIntegration, registry};
use crate::{
    Store, StoreError, WorktreeContext,
    io::{create_json, write_json},
    time::unix_milliseconds,
};

const MAX_HOOK_VALUE_BYTES: usize = 256;

pub(super) fn ready(
    store: &Store,
    context: &WorktreeContext,
    metadata: HookMetadata,
) -> Result<HookReport, StoreError> {
    let existing = registry::read(store, &context.key)?;
    if let Some(record) = existing {
        if record.state == WorktreeIntegrationState::Draining {
            return Ok(attention(
                HookEvent::Ready,
                record,
                "worktree is draining; use cancel-remove after a failed removal",
            ));
        }
        let previous = record.state;
        let mut updated = record.clone();
        updated.state = WorktreeIntegrationState::Ready;
        updated.head.clone_from(&context.head);
        merge_metadata(&mut updated, &metadata);
        return commit_update(store, HookEvent::Ready, previous, record, updated);
    }
    let now = unix_milliseconds()?;
    let record = WorktreeIntegration {
        schema_version: 1,
        store_id: store.marker.store_id,
        worktree_key: context.key.clone(),
        repository_id: context.repository_id.clone(),
        worktree_id: context.worktree_id.clone(),
        canonical_path: context.canonical_path.clone(),
        revision: 1,
        state: WorktreeIntegrationState::Ready,
        manager: metadata.manager,
        label: metadata.label,
        session: metadata.session,
        head: context.head.clone(),
        created_at: now,
        updated_at: now,
    };
    let path = store.layout.worktree_integration(&context.key);
    if !create_json(&path, &record)? {
        return Err(StoreError::InvalidOwnership {
            path,
            reason: "worktree registration appeared during creation".to_owned(),
        });
    }
    Ok(report(
        HookEvent::Ready,
        HookResult::Changed,
        None,
        record,
        "registered",
    ))
}

pub(super) fn transition(
    store: &Store,
    record: WorktreeIntegration,
    event: HookEvent,
    manager: Option<String>,
    target: WorktreeIntegrationState,
) -> Result<HookReport, StoreError> {
    if !allowed_transition(event, record.state) {
        return Ok(attention(
            event,
            record,
            "lifecycle transition is not valid from this state",
        ));
    }
    let previous = record.state;
    let mut updated = record.clone();
    updated.state = target;
    if manager.is_some() {
        updated.manager = manager;
    }
    commit_update(store, event, previous, record, updated)
}

pub(super) fn removed(
    store: &Store,
    record: WorktreeIntegration,
    manager: Option<String>,
) -> Result<HookReport, StoreError> {
    if record.state == WorktreeIntegrationState::Removed {
        return Ok(report(
            HookEvent::Removed,
            HookResult::Unchanged,
            Some(record.state),
            record,
            "worktree is already recorded as removed",
        ));
    }
    if record.state != WorktreeIntegrationState::Draining {
        return Ok(attention(
            HookEvent::Removed,
            record,
            "worktree is not draining",
        ));
    }
    match fs::symlink_metadata(&record.canonical_path) {
        Ok(_) => {
            return Ok(attention(
                HookEvent::Removed,
                record,
                "registered path still exists",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(StoreError::io(
                "verify removed worktree path",
                &record.canonical_path,
                error,
            ));
        }
    }
    transition(
        store,
        record,
        HookEvent::Removed,
        manager,
        WorktreeIntegrationState::Removed,
    )
}

pub(super) fn validate_cancel(
    context: &WorktreeContext,
    record: &WorktreeIntegration,
) -> Result<(), StoreError> {
    if record.canonical_path == context.canonical_path
        && record.repository_id == context.repository_id
        && record.worktree_id == context.worktree_id
    {
        Ok(())
    } else {
        Err(StoreError::InvalidOwnership {
            path: context.canonical_path.clone(),
            reason: "cancel-remove Git identity differs from the registered worktree".to_owned(),
        })
    }
}

pub(super) fn validate_metadata(metadata: &HookMetadata) -> Result<(), StoreError> {
    validate_value("manager", metadata.manager.as_deref())?;
    validate_value("label", metadata.label.as_deref())?;
    validate_value("session", metadata.session.as_deref())
}

pub(super) fn validate_value(field: &'static str, value: Option<&str>) -> Result<(), StoreError> {
    if value
        .is_some_and(|text| text.len() > MAX_HOOK_VALUE_BYTES || text.chars().any(char::is_control))
    {
        Err(StoreError::InvalidHookValue {
            field,
            maximum: MAX_HOOK_VALUE_BYTES,
        })
    } else {
        Ok(())
    }
}

pub(super) fn unmatched(event: HookEvent, path: &Path) -> HookReport {
    HookReport {
        event,
        result: HookResult::Attention,
        previous: None,
        resulting: None,
        integration: None,
        message: format!(
            "no validated worktree registration matches {}",
            path.display()
        ),
        history: crate::HistoryWrite::default(),
    }
}

pub(super) fn active(record: WorktreeIntegration, event: HookEvent) -> HookReport {
    report(
        event,
        HookResult::ActiveBuild,
        Some(record.state),
        record,
        "a managed build holds the worktree gate",
    )
}

fn commit_update(
    store: &Store,
    event: HookEvent,
    previous: WorktreeIntegrationState,
    original: WorktreeIntegration,
    mut updated: WorktreeIntegration,
) -> Result<HookReport, StoreError> {
    if original == updated {
        return Ok(report(
            event,
            HookResult::Unchanged,
            Some(previous),
            original,
            "already current",
        ));
    }
    updated.revision = original.revision.saturating_add(1);
    updated.updated_at = unix_milliseconds()?;
    write_json(
        &store.layout.worktree_integration(&updated.worktree_key),
        &updated,
    )?;
    Ok(report(
        event,
        HookResult::Changed,
        Some(previous),
        updated,
        "state committed",
    ))
}

fn report(
    event: HookEvent,
    result: HookResult,
    previous: Option<WorktreeIntegrationState>,
    integration: WorktreeIntegration,
    message: &str,
) -> HookReport {
    HookReport {
        event,
        result,
        previous,
        resulting: Some(integration.state),
        integration: Some(integration),
        message: message.to_owned(),
        history: crate::HistoryWrite::default(),
    }
}

fn attention(event: HookEvent, record: WorktreeIntegration, message: &str) -> HookReport {
    report(
        event,
        HookResult::Attention,
        Some(record.state),
        record,
        message,
    )
}

fn allowed_transition(event: HookEvent, state: WorktreeIntegrationState) -> bool {
    match event {
        HookEvent::PrepareRemove | HookEvent::CancelRemove => {
            state != WorktreeIntegrationState::Removed
        }
        HookEvent::Ready | HookEvent::Removed => true,
    }
}

fn merge_metadata(record: &mut WorktreeIntegration, metadata: &HookMetadata) {
    if metadata.manager.is_some() {
        record.manager.clone_from(&metadata.manager);
    }
    if metadata.label.is_some() {
        record.label.clone_from(&metadata.label);
    }
    if metadata.session.is_some() {
        record.session.clone_from(&metadata.session);
    }
}
