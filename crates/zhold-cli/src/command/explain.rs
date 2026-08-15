use serde::Serialize;
use zhold_core::{ArenaState, BuildOutcome};
use zhold_store::{InventoryEntry, Store};

use crate::{
    CliError,
    app::{ExitStatus, OutputFormat},
    render,
};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ArenaExplanation {
    pub(crate) entry: InventoryEntry,
    pub(crate) state: ArenaState,
    pub(crate) reclaimable: bool,
    pub(crate) explanation: String,
}

pub(super) fn execute(
    store: &Store,
    selector: &str,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    let inventory = store.inventory()?;
    let arena = super::selector::resolve_inventory(&inventory, selector)?;
    let entry = inventory
        .arenas
        .into_iter()
        .find(|entry| entry.record.id == arena)
        .ok_or_else(|| CliError::ArenaSelectorNotFound(selector.to_owned()))?;
    let state = entry.record.state();
    let reclaimable = matches!(state, ArenaState::Orphaned | ArenaState::Idle);
    let explanation = explanation(&entry, state, inventory.observed_at);
    render::explain(
        &ArenaExplanation {
            entry,
            state,
            reclaimable,
            explanation,
        },
        format,
    )?;
    Ok(ExitStatus::SUCCESS)
}

fn explanation(entry: &InventoryEntry, state: ArenaState, observed_at: u64) -> String {
    match state {
        ArenaState::Active => "protected by a live operating-system lease".to_owned(),
        ArenaState::Pinned => entry.pin_expires_at.map_or_else(
            || "protected by a permanent user pin".to_owned(),
            |expires| format!("protected by a user pin until Unix {expires}"),
        ),
        ArenaState::Orphaned => "reclaimable first because its worktree is absent".to_owned(),
        ArenaState::Idle
            if entry
                .pin_expires_at
                .is_some_and(|value| value <= observed_at) =>
        {
            "reclaimable because its time-limited pin expired".to_owned()
        }
        ArenaState::Idle if matches!(entry.record.last_outcome, Some(BuildOutcome::Failed(_))) => {
            "reclaimable before ordinary idle arenas because its last build failed".to_owned()
        }
        ArenaState::Idle => "reclaimable by least-recently-used order".to_owned(),
    }
}
