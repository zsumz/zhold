use zhold_core::{ArenaId, BuildOutcome, ByteSize};

use crate::{
    BuildReceipt, Store, StoreError,
    history::HistoryDraft,
    io::{read_json, write_json},
    lock::ExclusiveFileLock,
    manifest::ArenaManifest,
    time::{unix_milliseconds, unix_seconds},
};

impl Store {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish_primary(
        &self,
        id: &ArenaId,
        outcome: BuildOutcome,
        peak: ByteSize,
        initial_bytes: ByteSize,
        final_bytes: ByteSize,
        started_at: u64,
        warning_threshold: Option<ByteSize>,
        warning_threshold_exceeded: bool,
        integration: Option<&crate::WorktreeIntegration>,
    ) -> Result<HistoryDraft, StoreError> {
        let _metadata_lock = ExclusiveFileLock::acquire(&self.layout.metadata_lock(id))?;
        let manifest_path = self.layout.manifest(id);
        let mut manifest: ArenaManifest = read_json(&manifest_path)?;
        manifest.validate(self.marker.store_id, id, manifest_path.clone())?;
        let finished_seconds = unix_seconds()?;
        let finished_at = unix_milliseconds()?;
        let reservation = manifest.reservation;
        let command_class = manifest.command.command_class;
        manifest.finish(outcome, peak, final_bytes, finished_seconds);
        write_json(&manifest_path, &manifest)?;
        let observed_growth = std::cmp::max(peak, final_bytes).saturating_sub(initial_bytes);
        self.record_reservation_growth(command_class, observed_growth)?;
        Ok(HistoryDraft::build(
            BuildReceipt {
                arena_id: manifest.arena_id,
                repository_id: manifest.repository_id,
                worktree_id: manifest.worktree_id,
                workspace_id: manifest.workspace_id,
                toolchain_id: manifest.toolchain_id,
                command_class,
                started_at,
                finished_at,
                elapsed_milliseconds: finished_at.saturating_sub(started_at),
                outcome,
                exit_code: match outcome {
                    BuildOutcome::Succeeded => Some(0),
                    BuildOutcome::Failed(code) => Some(code),
                    BuildOutcome::Terminated => None,
                },
                initial_bytes,
                final_bytes,
                observed_peak: peak,
                reservation,
                warning_threshold,
                warning_threshold_exceeded,
                manager: None,
                label: None,
                session: None,
            },
            integration,
        ))
    }
}
