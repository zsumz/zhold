use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::{
    ArenaId, BuildOutcome, ByteSize, CommandDescriptor, RepositoryId, ToolchainId, WorkspaceId,
    WorktreeId,
};

use super::ArenaLifecycleStage;
use crate::{BuildContext, StoreError};

pub(crate) const ARENA_SCHEMA_VERSION: u32 = 7;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ArenaManifest {
    pub(crate) schema_version: u32,
    pub(crate) store_id: Uuid,
    pub(crate) arena_id: ArenaId,
    pub(crate) revision: u64,
    pub(crate) repository_id: RepositoryId,
    pub(crate) worktree_id: WorktreeId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) toolchain_id: ToolchainId,
    pub(crate) git_common_dir: PathBuf,
    pub(crate) worktree_root: PathBuf,
    pub(crate) workspace_root: PathBuf,
    pub(crate) branch: Option<String>,
    pub(crate) head: Option<String>,
    pub(crate) cargo_version: String,
    pub(crate) toolchain_description: String,
    pub(crate) created_at: u64,
    pub(crate) last_used_at: u64,
    pub(crate) last_started_at: Option<u64>,
    pub(crate) last_finished_at: Option<u64>,
    #[serde(default)]
    pub(crate) lifecycle_stage: Option<ArenaLifecycleStage>,
    #[serde(default)]
    pub(crate) command: CommandDescriptor,
    pub(crate) last_outcome: Option<BuildOutcome>,
    pub(crate) pinned: bool,
    #[serde(default)]
    pub(crate) pin_expires_at: Option<u64>,
    #[serde(default)]
    pub(crate) reservation: ByteSize,
    #[serde(default, alias = "last_peak")]
    pub(crate) last_observed_size: ByteSize,
    #[serde(default)]
    pub(crate) last_known_size: Option<ByteSize>,
    #[serde(default)]
    pub(crate) retirement_id: Option<Uuid>,
}

impl ArenaManifest {
    pub(crate) fn create(store_id: Uuid, context: &BuildContext, now: u64) -> Self {
        Self {
            schema_version: ARENA_SCHEMA_VERSION,
            store_id,
            arena_id: context.arena_id.clone(),
            revision: 0,
            repository_id: context.repository_id.clone(),
            worktree_id: context.worktree_id.clone(),
            workspace_id: context.workspace_id.clone(),
            toolchain_id: context.toolchain_id.clone(),
            git_common_dir: context.git_common_dir.clone(),
            worktree_root: context.worktree_root.clone(),
            workspace_root: context.workspace_root.clone(),
            branch: context.branch.clone(),
            head: context.head.clone(),
            cargo_version: context.cargo_version.clone(),
            toolchain_description: context.toolchain_description.clone(),
            created_at: now,
            last_used_at: now,
            last_started_at: None,
            last_finished_at: None,
            lifecycle_stage: Some(ArenaLifecycleStage::Finalized),
            command: CommandDescriptor::default(),
            last_outcome: None,
            pinned: false,
            pin_expires_at: None,
            reservation: ByteSize::ZERO,
            last_observed_size: ByteSize::ZERO,
            last_known_size: None,
            retirement_id: None,
        }
    }

    pub(crate) fn validate(
        &self,
        store_id: Uuid,
        expected_id: &ArenaId,
        path: PathBuf,
    ) -> Result<(), StoreError> {
        if !(1..=ARENA_SCHEMA_VERSION).contains(&self.schema_version) {
            return Err(StoreError::InvalidOwnership {
                path,
                reason: format!("unsupported arena schema {}", self.schema_version),
            });
        }
        if self.store_id != store_id {
            return Err(StoreError::InvalidOwnership {
                path,
                reason: "arena belongs to another store".to_owned(),
            });
        }
        if &self.arena_id != expected_id {
            return Err(StoreError::InvalidOwnership {
                path,
                reason: "arena identity does not match its path".to_owned(),
            });
        }
        let common = context_path(&self.git_common_dir, "Git common directory")?;
        let worktree = context_path(&self.worktree_root, "Git worktree root")?;
        let workspace = context_path(&self.workspace_root, "Cargo workspace root")?;
        let fields_match = self.repository_id == RepositoryId::derive(common)
            && self.worktree_id == WorktreeId::derive(worktree)
            && self.workspace_id == WorkspaceId::derive(workspace)
            && self.toolchain_id == ToolchainId::derive(&self.toolchain_description)
            && self.workspace_root.starts_with(&self.worktree_root);
        if !fields_match {
            return Err(StoreError::InvalidOwnership {
                path,
                reason: "arena compatibility context does not match its identities".to_owned(),
            });
        }
        let derived = ArenaId::derive(
            &self.repository_id,
            &self.worktree_id,
            &self.workspace_id,
            &self.toolchain_id,
        );
        if derived != self.arena_id {
            return Err(StoreError::InvalidOwnership {
                path,
                reason: "arena identity does not match its compatibility context".to_owned(),
            });
        }
        Ok(())
    }

    pub(crate) fn validate_context(
        &self,
        context: &BuildContext,
        path: PathBuf,
    ) -> Result<(), StoreError> {
        let matches = self.repository_id == context.repository_id
            && self.worktree_id == context.worktree_id
            && self.workspace_id == context.workspace_id
            && self.toolchain_id == context.toolchain_id
            && self.git_common_dir == context.git_common_dir
            && self.worktree_root == context.worktree_root
            && self.workspace_root == context.workspace_root;
        if matches {
            Ok(())
        } else {
            Err(StoreError::InvalidOwnership {
                path,
                reason: "persisted arena context does not match the resolved build".to_owned(),
            })
        }
    }

    pub(crate) fn begin(
        &mut self,
        context: &BuildContext,
        command: CommandDescriptor,
        reservation: ByteSize,
        now: u64,
    ) {
        self.schema_version = ARENA_SCHEMA_VERSION;
        self.revision = self.revision.saturating_add(1);
        self.branch.clone_from(&context.branch);
        self.head.clone_from(&context.head);
        self.last_used_at = now;
        self.last_started_at = Some(now);
        self.last_finished_at = None;
        self.lifecycle_stage = Some(ArenaLifecycleStage::Reserved);
        self.command = command;
        self.last_outcome = None;
        self.reservation = reservation;
        self.retirement_id = None;
    }

    pub(crate) fn observe_size(&mut self, size: ByteSize) {
        self.schema_version = ARENA_SCHEMA_VERSION;
        self.last_known_size = Some(size);
    }

    pub(crate) fn set_pin(&mut self, pinned: bool, expires_at: Option<u64>) {
        let expires_at = if pinned { expires_at } else { None };
        if self.pinned != pinned || self.pin_expires_at != expires_at {
            self.schema_version = ARENA_SCHEMA_VERSION;
            self.revision = self.revision.saturating_add(1);
            self.pinned = pinned;
            self.pin_expires_at = expires_at;
        }
    }

    pub(crate) fn is_pinned_at(&self, now: u64) -> bool {
        self.pinned
            && self
                .pin_expires_at
                .is_none_or(|expires_at| expires_at > now)
    }

    pub(crate) fn prepare_retirement(&mut self, retirement_id: Uuid) {
        self.schema_version = ARENA_SCHEMA_VERSION;
        self.revision = self.revision.saturating_add(1);
        self.retirement_id = Some(retirement_id);
    }

    pub(crate) fn clear_retirement(&mut self, retirement_id: Uuid) -> bool {
        if self.retirement_id == Some(retirement_id) {
            self.schema_version = ARENA_SCHEMA_VERSION;
            self.revision = self.revision.saturating_add(1);
            self.retirement_id = None;
            true
        } else {
            false
        }
    }
}

fn context_path<'a>(path: &'a std::path::Path, kind: &'static str) -> Result<&'a str, StoreError> {
    path.to_str().ok_or_else(|| StoreError::NonUnicode {
        kind,
        path: path.to_path_buf(),
    })
}
