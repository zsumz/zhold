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
    finished: bool,
    arena_lock: Option<ExclusiveFileLock>,
    worktree: Option<WorktreeAdmission>,
    pending_history: Vec<HistoryDraft>,
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
            finished: false,
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
        self.finish_observed(BuildOutcome::NotStarted, ByteSize::ZERO)
    }

    /// Records the bounded high-water observation used by build history.
    pub(crate) fn finish_observed(
        mut self,
        outcome: BuildOutcome,
        high_water_observation: ByteSize,
    ) -> Result<BuildFinalization, StoreError> {
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
        self.finished = true;
        self.release_locks();
        let warnings = if matches!(outcome, BuildOutcome::NotStarted) {
            Vec::new()
        } else {
            self.learn_reservation(primary.command_class, primary.observed_growth)
        };
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
        if self.finished {
            return;
        }
        self.finished = true;
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
            BuildOutcome::Terminated,
            ByteSize::ZERO,
            self.initial_bytes,
            final_bytes,
            self.started_at,
            integration,
        );
        self.release_locks();
        if let Ok(primary) = build {
            let _warnings = self.learn_reservation(primary.command_class, primary.observed_growth);
            self.pending_history.push(primary.history);
            for draft in self.pending_history.drain(..) {
                let _ignored = persist(&self.store, draft);
            }
        }
    }
}
