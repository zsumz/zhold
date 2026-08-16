use serde::{Deserialize, Serialize};
use zhold_core::{BuildOutcome, ByteSize};

use super::{ArenaManifest, arena_manifest::ARENA_SCHEMA_VERSION};
use crate::StoreError;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ArenaLifecycleStage {
    Reserved,
    Spawning,
    Spawned,
    Finalized,
}

impl ArenaManifest {
    pub(crate) fn mark_spawning(&mut self) -> Result<(), StoreError> {
        self.transition(ArenaLifecycleStage::Reserved, ArenaLifecycleStage::Spawning)
    }

    pub(crate) fn mark_spawned(&mut self) -> Result<(), StoreError> {
        self.transition(ArenaLifecycleStage::Spawning, ArenaLifecycleStage::Spawned)
    }

    pub(crate) fn finish(
        &mut self,
        outcome: BuildOutcome,
        high_water_observation: ByteSize,
        final_bytes: Option<ByteSize>,
        now: u64,
    ) -> Result<(), StoreError> {
        let stage = self.effective_lifecycle_stage();
        let allowed = matches!(
            (stage, outcome),
            (
                ArenaLifecycleStage::Reserved | ArenaLifecycleStage::Spawning,
                BuildOutcome::NotStarted
            ) | (
                ArenaLifecycleStage::Spawning | ArenaLifecycleStage::Spawned,
                BuildOutcome::Terminated
            ) | (
                ArenaLifecycleStage::Spawned,
                BuildOutcome::Succeeded | BuildOutcome::Failed(_)
            )
        );
        if !allowed {
            return Err(StoreError::InvalidLifecycleTransition {
                arena: self.arena_id.to_string(),
                transition: format!("{stage:?} -> {outcome:?}"),
            });
        }
        self.schema_version = ARENA_SCHEMA_VERSION;
        self.revision = self.revision.saturating_add(1);
        self.last_used_at = now;
        self.last_finished_at = Some(now);
        self.last_outcome = Some(outcome);
        self.lifecycle_stage = Some(ArenaLifecycleStage::Finalized);
        self.reservation = ByteSize::ZERO;
        self.last_observed_size = high_water_observation;
        if let Some(final_bytes) = final_bytes {
            self.last_known_size = Some(final_bytes);
        }
        Ok(())
    }

    pub(crate) fn is_unfinished(&self) -> bool {
        !matches!(
            self.effective_lifecycle_stage(),
            ArenaLifecycleStage::Finalized
        ) || (self.last_started_at.is_some() && self.last_finished_at.is_none())
    }

    pub(crate) fn recovery_outcome(&self) -> BuildOutcome {
        match self.effective_lifecycle_stage() {
            ArenaLifecycleStage::Reserved => BuildOutcome::NotStarted,
            ArenaLifecycleStage::Spawning
            | ArenaLifecycleStage::Spawned
            | ArenaLifecycleStage::Finalized => BuildOutcome::Terminated,
        }
    }

    fn transition(
        &mut self,
        expected: ArenaLifecycleStage,
        next: ArenaLifecycleStage,
    ) -> Result<(), StoreError> {
        let current = self.effective_lifecycle_stage();
        if current != expected || !self.is_unfinished() {
            return Err(StoreError::InvalidLifecycleTransition {
                arena: self.arena_id.to_string(),
                transition: format!("{current:?} -> {next:?}"),
            });
        }
        self.schema_version = ARENA_SCHEMA_VERSION;
        self.revision = self.revision.saturating_add(1);
        self.lifecycle_stage = Some(next);
        Ok(())
    }

    fn effective_lifecycle_stage(&self) -> ArenaLifecycleStage {
        self.lifecycle_stage.unwrap_or_else(|| {
            if self.last_started_at.is_some() && self.last_finished_at.is_none() {
                ArenaLifecycleStage::Spawned
            } else {
                ArenaLifecycleStage::Finalized
            }
        })
    }
}
