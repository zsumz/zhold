use std::path::{Path, PathBuf};

use zhold_core::{ArenaId, BuildOutcome, ByteSize};

use crate::{
    BuildFinalization, FinalizationWarning, FinalizationWarningEvent, Store, StoreError,
    history::{HistoryDraft, persist},
    io::measure_tree,
    lock::ExclusiveFileLock,
    store::initialization::ensure_managed_directory,
    worktree::WorktreeAdmission,
};

/// Exclusive live-build lease for one managed arena and shared worktree gate.
#[derive(Debug)]
pub struct ArenaLease {
    store: Store,
    arena_id: ArenaId,
    arena_root: PathBuf,
    build_dir: PathBuf,
    initial_bytes: ByteSize,
    started_at: u64,
    stage: LeaseStage,
    arena_lock: Option<ExclusiveFileLock>,
    worktree: Option<WorktreeAdmission>,
    pending_history: Vec<HistoryDraft>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LeaseStage {
    Reserved,
    Spawned,
    Finalized,
}

impl ArenaLease {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        store: Store,
        arena_id: ArenaId,
        arena_root: PathBuf,
        build_dir: PathBuf,
        arena_lock: ExclusiveFileLock,
        worktree: WorktreeAdmission,
        initial_bytes: ByteSize,
        started_at: u64,
    ) -> Self {
        Self {
            store,
            arena_id,
            arena_root,
            build_dir,
            initial_bytes,
            started_at,
            stage: LeaseStage::Reserved,
            arena_lock: Some(arena_lock),
            worktree: Some(worktree),
            pending_history: Vec::new(),
        }
    }

    pub(crate) fn queue_history(&mut self, draft: HistoryDraft) {
        self.pending_history.push(draft);
    }

    /// Stable arena identity.
    pub fn arena_id(&self) -> &ArenaId {
        &self.arena_id
    }

    /// Root containing zhold metadata and intermediate build data.
    pub fn arena_root(&self) -> &Path {
        &self.arena_root
    }

    /// Directory to provide through `CARGO_BUILD_BUILD_DIR`.
    pub fn build_dir(&self) -> &Path {
        &self.build_dir
    }

    /// Measures currently allocated bytes beneath this leased arena.
    pub fn measure(&self) -> Result<ByteSize, StoreError> {
        measure_tree(&self.arena_root)
    }

    /// Records that Cargo was successfully spawned for this reservation.
    pub fn mark_spawned(&mut self) {
        if matches!(self.stage, LeaseStage::Reserved) {
            self.stage = LeaseStage::Spawned;
        }
    }

    /// Records the child process outcome and releases the lease.
    pub fn finish(self, outcome: BuildOutcome) -> Result<BuildFinalization, StoreError> {
        self.finish_observed(outcome, ByteSize::ZERO)
    }

    /// Records the child outcome and a bounded high-water size observation.
    pub fn finish_with_observation(
        self,
        outcome: BuildOutcome,
        high_water_observation: ByteSize,
    ) -> Result<BuildFinalization, StoreError> {
        self.finish_observed(outcome, high_water_observation)
    }

    /// Records an admitted command that failed before Cargo was spawned.
    pub fn finish_not_started(self) -> Result<BuildFinalization, StoreError> {
        self.finish_aborted()
    }

    /// Finalizes an abnormal path according to whether Cargo was ever spawned.
    pub fn finish_aborted(self) -> Result<BuildFinalization, StoreError> {
        let (outcome, observation) = match self.stage {
            LeaseStage::Reserved => (BuildOutcome::NotStarted, ByteSize::ZERO),
            LeaseStage::Spawned => (
                BuildOutcome::Terminated,
                self.measure().unwrap_or(self.initial_bytes),
            ),
            LeaseStage::Finalized => return Ok(BuildFinalization::default()),
        };
        self.finish_observed(outcome, observation)
    }

    /// Records the bounded high-water observation used by build history.
    pub(crate) fn finish_observed(
        mut self,
        mut outcome: BuildOutcome,
        high_water_observation: ByteSize,
    ) -> Result<BuildFinalization, StoreError> {
        if matches!(self.stage, LeaseStage::Spawned) && matches!(outcome, BuildOutcome::NotStarted)
        {
            outcome = BuildOutcome::Terminated;
        }
        if !matches!(outcome, BuildOutcome::NotStarted) {
            self.mark_spawned();
        }
        ensure_managed_directory(self.store.layout.root(), &self.build_dir)?;
        let final_bytes = self.measure().ok();
        let integration = self
            .worktree
            .as_ref()
            .and_then(|admission| admission.integration.as_ref());
        let primary = self.store.finish_primary(
            &self.arena_id,
            outcome,
            high_water_observation,
            self.initial_bytes,
            final_bytes,
            self.started_at,
            integration,
        )?;
        self.stage = LeaseStage::Finalized;
        self.release_locks();
        if let Some(error) = primary.durability_error {
            return Err(error);
        }
        let mut warnings = primary.warnings;
        if !matches!(outcome, BuildOutcome::NotStarted) {
            warnings.extend(self.learn_reservation(primary.command_class, primary.observed_growth));
        }
        self.pending_history.push(primary.history);
        let history = self
            .pending_history
            .drain(..)
            .map(|draft| persist(&self.store, draft))
            .collect();
        Ok(BuildFinalization { warnings, history })
    }

    fn release_locks(&mut self) {
        drop(self.arena_lock.take());
        drop(self.worktree.take());
    }

    fn learn_reservation(
        &self,
        command_class: zhold_core::CargoCommandClass,
        observed_growth: ByteSize,
    ) -> Vec<FinalizationWarning> {
        self.store
            .record_reservation_growth(command_class, observed_growth)
            .err()
            .map_or_else(Vec::new, |error| {
                vec![FinalizationWarning {
                    event: FinalizationWarningEvent::ReservationLearningFailed,
                    message: error.to_string(),
                }]
            })
    }
}

impl Drop for ArenaLease {
    fn drop(&mut self) {
        if matches!(self.stage, LeaseStage::Finalized) {
            return;
        }
        let outcome = match self.stage {
            LeaseStage::Reserved => BuildOutcome::NotStarted,
            LeaseStage::Spawned => BuildOutcome::Terminated,
            LeaseStage::Finalized => return,
        };
        self.stage = LeaseStage::Finalized;
        if ensure_managed_directory(self.store.layout.root(), &self.build_dir).is_err() {
            self.release_locks();
            return;
        }
        let final_bytes = self.measure().ok();
        let integration = self
            .worktree
            .as_ref()
            .and_then(|admission| admission.integration.as_ref());
        let build = self.store.finish_primary(
            &self.arena_id,
            outcome,
            ByteSize::ZERO,
            self.initial_bytes,
            final_bytes,
            self.started_at,
            integration,
        );
        self.release_locks();
        if let Ok(primary) = build {
            if primary.durability_error.is_some() {
                return;
            }
            if !matches!(outcome, BuildOutcome::NotStarted) {
                let _warnings =
                    self.learn_reservation(primary.command_class, primary.observed_growth);
            }
            self.pending_history.push(primary.history);
            for draft in self.pending_history.drain(..) {
                let _ignored = persist(&self.store, draft);
            }
        }
    }
}
