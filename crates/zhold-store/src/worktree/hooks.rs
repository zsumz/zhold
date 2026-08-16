use std::path::Path;

use zhold_core::{HookEvent, WorktreeIntegrationState};

use super::{HookMetadata, HookReport, WorktreeSummary, registry, transitions};
use crate::{
    Store, StoreError, WorktreeContext,
    history::{HistoryDraft, persist},
    lock::ExclusiveFileLock,
};

impl Store {
    /// Registers or reactivates an existing validated Git worktree.
    pub fn hook_ready(
        &self,
        context: &WorktreeContext,
        metadata: HookMetadata,
    ) -> Result<HookReport, StoreError> {
        self.ensure_writable("register a worktree")?;
        transitions::validate_metadata(&metadata)?;
        registry::validate_context(context)?;
        let mut report = {
            let _registry = ExclusiveFileLock::acquire(&self.layout.worktree_registry_lock())?;
            let _gate = ExclusiveFileLock::acquire(&self.layout.worktree_lock(&context.key))?;
            registry::reject_alias(self, context)?;
            transitions::ready(self, context, metadata)?
        };
        persist_hook(self, &mut report);
        Ok(report)
    }

    /// Establishes a durable draining guard if no build holds the worktree gate.
    pub fn hook_prepare_remove(
        &self,
        path: &Path,
        manager: Option<String>,
    ) -> Result<HookReport, StoreError> {
        self.ensure_writable("prepare worktree removal")?;
        transitions::validate_value("manager", manager.as_deref())?;
        let mut report = {
            let _registry = ExclusiveFileLock::acquire(&self.layout.worktree_registry_lock())?;
            let Some(record) = registry::find_path(self, path)? else {
                return Ok(transitions::unmatched(HookEvent::PrepareRemove, path));
            };
            match ExclusiveFileLock::try_acquire(&self.layout.worktree_lock(&record.worktree_key))?
            {
                Some(_gate) => transitions::transition(
                    self,
                    record,
                    HookEvent::PrepareRemove,
                    manager,
                    WorktreeIntegrationState::Draining,
                )?,
                None => transitions::active(record, HookEvent::PrepareRemove),
            }
        };
        persist_hook(self, &mut report);
        Ok(report)
    }

    /// Confirms a draining worktree path is absent and records removal.
    pub fn hook_removed(
        &self,
        path: &Path,
        manager: Option<String>,
    ) -> Result<HookReport, StoreError> {
        self.ensure_writable("record worktree removal")?;
        transitions::validate_value("manager", manager.as_deref())?;
        let mut report = {
            let _registry = ExclusiveFileLock::acquire(&self.layout.worktree_registry_lock())?;
            let Some(record) = registry::find_path(self, path)? else {
                return Ok(transitions::unmatched(HookEvent::Removed, path));
            };
            match ExclusiveFileLock::try_acquire(&self.layout.worktree_lock(&record.worktree_key))?
            {
                Some(_gate) => transitions::removed(self, record, manager)?,
                None => transitions::active(record, HookEvent::Removed),
            }
        };
        persist_hook(self, &mut report);
        Ok(report)
    }

    /// Cancels a failed removal after revalidating the original Git identity.
    pub fn hook_cancel_remove(
        &self,
        context: &WorktreeContext,
        manager: Option<String>,
    ) -> Result<HookReport, StoreError> {
        self.ensure_writable("cancel worktree removal")?;
        transitions::validate_value("manager", manager.as_deref())?;
        registry::validate_context(context)?;
        let mut report = {
            let _registry = ExclusiveFileLock::acquire(&self.layout.worktree_registry_lock())?;
            let _gate = ExclusiveFileLock::acquire(&self.layout.worktree_lock(&context.key))?;
            let Some(record) = registry::read(self, &context.key)? else {
                return Ok(transitions::unmatched(
                    HookEvent::CancelRemove,
                    context.canonical_path(),
                ));
            };
            transitions::validate_cancel(context, &record)?;
            transitions::transition(
                self,
                record,
                HookEvent::CancelRemove,
                manager,
                WorktreeIntegrationState::Ready,
            )?
        };
        persist_hook(self, &mut report);
        Ok(report)
    }

    /// Summarizes validated worktree registrations and recovery actions.
    pub fn worktree_summary(&self) -> Result<WorktreeSummary, StoreError> {
        let (records, findings) = registry::scan(self)?;
        let draining = records
            .iter()
            .filter(|record| record.state == WorktreeIntegrationState::Draining)
            .collect::<Vec<_>>();
        Ok(WorktreeSummary {
            registration_count: count(records.len()),
            draining_count: count(draining.len()),
            finding_count: count(findings.len()),
            recovery: draining
                .iter()
                .map(|record| {
                    format!(
                        "worktree {} is draining; recover with `zhold hook cancel-remove --path {}`",
                        record.worktree_key,
                        record.canonical_path.display()
                    )
                })
                .collect(),
        })
    }
}

fn persist_hook(store: &Store, report: &mut HookReport) {
    if let Some(draft) = HistoryDraft::hook(report) {
        report.history = persist(store, draft);
    }
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
