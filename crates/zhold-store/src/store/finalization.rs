use zhold_core::{ArenaId, BuildOutcome, ByteSize, CargoCommandClass};

use crate::{
    BuildReceipt, FinalizationWarning, FinalizationWarningEvent, Store, StoreError,
    history::HistoryDraft,
    io::{read_json, write_json_commit_aware},
    lock::ExclusiveFileLock,
    manifest::ArenaManifest,
    time::{unix_milliseconds, unix_seconds},
};

pub(crate) struct PrimaryFinalization {
    pub(crate) history: HistoryDraft,
    pub(crate) command_class: CargoCommandClass,
    pub(crate) observed_growth: ByteSize,
    pub(crate) warnings: Vec<FinalizationWarning>,
}

impl Store {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish_primary(
        &self,
        id: &ArenaId,
        outcome: BuildOutcome,
        high_water_observation: ByteSize,
        initial_bytes: ByteSize,
        final_bytes: Option<ByteSize>,
        started_at: u64,
        integration: Option<&crate::WorktreeIntegration>,
    ) -> Result<PrimaryFinalization, StoreError> {
        let _metadata_lock = ExclusiveFileLock::acquire(&self.layout.metadata_lock(id))?;
        let manifest_path = self.layout.manifest(id);
        let mut manifest: ArenaManifest = read_json(&manifest_path)?;
        manifest.validate(self.marker.store_id, id, manifest_path.clone())?;
        let finished_seconds = unix_seconds()?;
        let finished_at = unix_milliseconds()?;
        let reservation = manifest.reservation;
        let command_class = manifest.command.command_class;
        let final_bytes = final_bytes
            .or(manifest.last_known_size)
            .unwrap_or(ByteSize::ZERO);
        let high_water_observation = high_water_observation.max(initial_bytes).max(final_bytes);
        manifest.finish(
            outcome,
            high_water_observation,
            Some(final_bytes),
            finished_seconds,
        );
        let publication = write_json_commit_aware(&manifest_path, &manifest)?;
        let warnings = publication
            .cleanup_warning()
            .map_or_else(Vec::new, |error| {
                vec![FinalizationWarning {
                    event: FinalizationWarningEvent::MetadataCleanupFailed,
                    message: error.to_string(),
                }]
            });
        let observed_growth = high_water_observation.saturating_sub(initial_bytes);
        let history = HistoryDraft::build(
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
                    BuildOutcome::NotStarted | BuildOutcome::Terminated => None,
                },
                initial_bytes,
                final_bytes,
                high_water_observation,
                reservation,
                manager: None,
                label: None,
                session: None,
            },
            integration,
        );
        Ok(PrimaryFinalization {
            history,
            command_class,
            observed_growth,
            warnings,
        })
    }
}
