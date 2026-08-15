use std::fs;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::ByteSize;

use crate::{Store, StoreError};

/// Read-only store health summary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DoctorReport {
    /// Stable store identity.
    pub store_id: Uuid,
    /// Number of valid managed arenas.
    pub arena_count: usize,
    /// Number of entries that failed closed inventory validation.
    pub finding_count: usize,
    /// Number of retirement directories awaiting deletion or inspection.
    pub trash_count: usize,
    /// Measured bytes awaiting deletion in owned trash.
    pub trash_bytes: ByteSize,
    /// Complete measured marked-store footprint.
    pub physical_bytes: ByteSize,
    /// Bytes available on the store filesystem.
    pub available_bytes: ByteSize,
    /// Number of valid bounded-history receipts.
    pub history_count: u64,
    /// Measured bytes in valid history receipts.
    pub history_bytes: ByteSize,
    /// Number of invalid or foreign history entries.
    pub history_finding_count: u64,
    /// Number of validated worktree integration records.
    pub worktree_count: u64,
    /// Number of fail-closed draining worktrees.
    pub draining_worktree_count: u64,
    /// Whether an adopted quota expectation is currently healthy.
    pub quota_healthy: bool,
    /// Whether no ownership or retirement finding requires attention.
    pub healthy: bool,
    /// Human-readable findings.
    pub findings: Vec<String>,
}

impl DoctorReport {
    pub(crate) fn inspect(store: &Store) -> Result<Self, StoreError> {
        let inventory = store.inventory()?;
        let mut findings = inventory
            .findings
            .iter()
            .map(|finding| format!("{}: {}", finding.path.display(), finding.reason))
            .collect::<Vec<_>>();
        let trash_count = count_entries(&store.layout.trash())?;
        if trash_count > 0 {
            findings.push(format!(
                "{trash_count} retired arena directories remain under {}",
                store.layout.trash().display()
            ));
        }
        if inventory.history.finding_count > 0 {
            findings.push(format!(
                "{} history entries or policy documents require attention",
                inventory.history.finding_count
            ));
        }
        if inventory.history.oversized_newest {
            findings.push("the newest history receipt alone exceeds the byte bound".to_owned());
        }
        if inventory.worktrees.finding_count > 0 {
            findings.push(format!(
                "{} worktree integration entries require attention",
                inventory.worktrees.finding_count
            ));
        }
        findings.extend(inventory.worktrees.recovery.clone());
        if let Some(finding) = &inventory.quota_finding {
            findings.push(format!("quota expectation requires attention: {finding}"));
        }
        let quota_healthy = inventory
            .quota
            .as_ref()
            .is_none_or(|status| status.expectation.is_none() || status.healthy);
        if !quota_healthy {
            findings.push("adopted quota enforcement is drifted or unverifiable".to_owned());
        }
        let finding_count = findings.len();
        let physical_bytes = inventory.physical.ok_or_else(|| {
            StoreError::InvalidConfiguration(
                "deep doctor inventory omitted the physical footprint".to_owned(),
            )
        })?;
        Ok(Self {
            store_id: store.marker.store_id,
            arena_count: inventory.arenas.len(),
            finding_count,
            trash_count,
            trash_bytes: inventory.trash,
            physical_bytes,
            available_bytes: inventory.available,
            history_count: inventory.history.receipt_count,
            history_bytes: inventory.history.receipt_bytes,
            history_finding_count: inventory.history.finding_count,
            worktree_count: inventory.worktrees.registration_count,
            draining_worktree_count: inventory.worktrees.draining_count,
            quota_healthy,
            healthy: findings.is_empty(),
            findings,
        })
    }
}

fn count_entries(path: &std::path::Path) -> Result<usize, StoreError> {
    let entries = fs::read_dir(path)
        .map_err(|error| StoreError::io("read retirement directory", path, error))?;
    entries
        .map(|entry| {
            entry
                .map(|_| 1_usize)
                .map_err(|error| StoreError::io("read retirement entry", path, error))
        })
        .try_fold(0_usize, |count, value| value.map(|one| count + one))
}
