use zhold_core::{WorktreeIntegrationState, WorktreeKey};

use super::registry;
use crate::{BuildContext, Store, StoreError, lock::SharedFileLock};

#[derive(Debug)]
pub(crate) struct WorktreeAdmission {
    pub(crate) _gate: SharedFileLock,
    pub(crate) integration: Option<super::WorktreeIntegration>,
}

pub(crate) fn acquire_admission(
    store: &Store,
    context: &BuildContext,
) -> Result<WorktreeAdmission, StoreError> {
    let key = WorktreeKey::derive(&context.repository_id, &context.worktree_id);
    let gate = SharedFileLock::acquire(&store.layout.worktree_lock(&key))?;
    let integration = registry::read_for_context(store, context)?;
    if let Some(record) = &integration
        && record.state != WorktreeIntegrationState::Ready
    {
        return Err(StoreError::WorktreeAdmissionBlocked {
            path: record.canonical_path.clone(),
            state: format!("{:?}", record.state).to_ascii_lowercase(),
        });
    }
    Ok(WorktreeAdmission {
        _gate: gate,
        integration,
    })
}
