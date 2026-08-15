use std::path::{Path, PathBuf};

use uuid::Uuid;
use zhold_core::{ArenaId, WorktreeKey};

#[derive(Clone, Debug)]
pub(crate) struct StoreLayout {
    root: PathBuf,
}

impl StoreLayout {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn marker(&self) -> PathBuf {
        self.root.join("store.json")
    }

    pub(crate) fn config(&self) -> PathBuf {
        self.root.join("config.json")
    }

    pub(crate) fn config_lock(&self) -> PathBuf {
        self.locks().join("config.lock")
    }

    pub(crate) fn arenas(&self) -> PathBuf {
        self.root.join("arenas")
    }

    pub(crate) fn locks(&self) -> PathBuf {
        self.root.join("locks")
    }

    pub(crate) fn trash(&self) -> PathBuf {
        self.root.join("trash")
    }

    pub(crate) fn trash_index(&self) -> PathBuf {
        self.root.join("trash-index")
    }

    pub(crate) fn history(&self) -> PathBuf {
        self.root.join("history")
    }

    pub(crate) fn history_policy(&self) -> PathBuf {
        self.history().join("policy.json")
    }

    pub(crate) fn history_index(&self) -> PathBuf {
        self.history().join("index.json")
    }

    pub(crate) fn history_receipts(&self) -> PathBuf {
        self.history().join("receipts")
    }

    pub(crate) fn history_lock(&self) -> PathBuf {
        self.locks().join("history.lock")
    }

    pub(crate) fn history_receipt(&self, recorded_at: u64, receipt_id: Uuid) -> PathBuf {
        self.history_receipts()
            .join(format!("{recorded_at}-{receipt_id}.json"))
    }

    pub(crate) fn integrations(&self) -> PathBuf {
        self.root.join("integrations")
    }

    pub(crate) fn worktree_integrations(&self) -> PathBuf {
        self.integrations().join("worktrees")
    }

    pub(crate) fn worktree_integration(&self, key: &WorktreeKey) -> PathBuf {
        self.worktree_integrations().join(format!("{key}.json"))
    }

    pub(crate) fn worktree_locks(&self) -> PathBuf {
        self.locks().join("worktrees")
    }

    pub(crate) fn worktree_registry_lock(&self) -> PathBuf {
        self.locks().join("worktrees.lock")
    }

    pub(crate) fn worktree_lock(&self, key: &WorktreeKey) -> PathBuf {
        self.worktree_locks().join(format!("{key}.lock"))
    }

    pub(crate) fn quota(&self) -> PathBuf {
        self.root.join("quota.json")
    }

    pub(crate) fn quota_lock(&self) -> PathBuf {
        self.locks().join("quota.lock")
    }

    pub(crate) fn reservation_profile(&self) -> PathBuf {
        self.root.join("reservation-profile.json")
    }

    pub(crate) fn reservation_lock(&self) -> PathBuf {
        self.locks().join("reservation.lock")
    }

    pub(crate) fn arena(&self, id: &ArenaId) -> PathBuf {
        self.arenas().join(prefix(id)).join(id.as_str())
    }

    pub(crate) fn build_dir(&self, id: &ArenaId) -> PathBuf {
        self.arena(id).join("build")
    }

    pub(crate) fn manifest(&self, id: &ArenaId) -> PathBuf {
        self.arena(id).join("arena.json")
    }

    pub(crate) fn arena_lock(&self, id: &ArenaId) -> PathBuf {
        self.locks().join("arenas").join(format!("{id}.lock"))
    }

    pub(crate) fn metadata_lock(&self, id: &ArenaId) -> PathBuf {
        self.locks().join("metadata").join(format!("{id}.lock"))
    }

    pub(crate) fn collection_lock(&self) -> PathBuf {
        self.locks().join("collection.lock")
    }

    pub(crate) fn trash_destination(&self, id: &ArenaId, retirement_id: Uuid) -> PathBuf {
        self.trash().join(format!("{id}-{retirement_id}"))
    }

    pub(crate) fn retirement_record(&self, retirement_id: Uuid) -> PathBuf {
        self.trash_index().join(format!("{retirement_id}.json"))
    }
}

fn prefix(id: &ArenaId) -> &str {
    id.as_str().get(..2).unwrap_or("00")
}
