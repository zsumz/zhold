use std::path::{Path, PathBuf};

use zhold_core::{ArenaId, BuildOutcome, ByteSize};

use crate::{
    BuildFinalization, Store, StoreError,
    history::{HistoryDraft, persist},
    io::measure_tree,
    lock::ExclusiveFileLock,
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
        self.finish_observed(outcome, ByteSize::ZERO, None, false)
    }

    /// Records the child outcome and observed peak arena size, then releases the lease.
    pub fn finish_with_peak(
        self,
        outcome: BuildOutcome,
        peak: ByteSize,
    ) -> Result<BuildFinalization, StoreError> {
        self.finish_observed(outcome, peak, None, false)
    }

    /// Records the complete bounded peak observation used by build history.
    pub fn finish_observed(
        mut self,
        outcome: BuildOutcome,
        peak: ByteSize,
        warning_threshold: Option<ByteSize>,
        warning_threshold_exceeded: bool,
    ) -> Result<BuildFinalization, StoreError> {
        self.finished = true;
        let final_bytes = self.measure().unwrap_or(peak);
        let integration = self
            .worktree
            .as_ref()
            .and_then(|admission| admission.integration.as_ref());
        let build = self.store.finish_primary(
            &self.arena_id,
            outcome,
            peak,
            self.initial_bytes,
            final_bytes,
            self.started_at,
            warning_threshold,
            warning_threshold_exceeded,
            integration,
        )?;
        self.release_locks();
        self.pending_history.push(build);
        let history = self
            .pending_history
            .drain(..)
            .map(|draft| persist(&self.store, draft))
            .collect();
        Ok(BuildFinalization { history })
    }

    fn release_locks(&mut self) {
        drop(self.arena_lock.take());
        drop(self.worktree.take());
    }
}

impl Drop for ArenaLease {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        let final_bytes = self.measure().unwrap_or(ByteSize::ZERO);
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
            None,
            false,
            integration,
        );
        self.release_locks();
        if let Ok(build) = build {
            self.pending_history.push(build);
            for draft in self.pending_history.drain(..) {
                let _ignored = persist(&self.store, draft);
            }
        }
    }
}
