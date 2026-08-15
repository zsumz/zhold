use zhold_core::HistoryPolicy;

use crate::{
    HistoryPruneReport, HistoryPruneRequest, HistoryQuery, HistoryReport, HistorySummary, Store,
    StoreError, history,
};

impl Store {
    /// Reads bounded operation history newest-first.
    pub fn history(&self, query: &HistoryQuery) -> Result<HistoryReport, StoreError> {
        history::read_history(self, query)
    }

    /// Returns the effective persisted receipt-retention policy.
    pub fn history_policy(&self) -> Result<HistoryPolicy, StoreError> {
        history::history_policy(self)
    }

    /// Atomically updates receipt retention and applies the new bounds.
    pub fn set_history_policy(
        &self,
        policy: HistoryPolicy,
    ) -> Result<HistoryPruneReport, StoreError> {
        history::set_policy(self, policy)?;
        history::prune(
            self,
            HistoryPruneRequest {
                keep: Some(policy.max_receipts),
                max_bytes: Some(policy.max_bytes),
                older_than: None,
                dry_run: false,
            },
        )
    }

    /// Removes only validated receipts selected by deterministic bounds.
    pub fn prune_history(
        &self,
        request: HistoryPruneRequest,
    ) -> Result<HistoryPruneReport, StoreError> {
        history::prune(self, request)
    }

    /// Returns the compact history health summary used by status and doctor.
    pub fn history_summary(&self) -> Result<HistorySummary, StoreError> {
        history::summary(self)
    }
}
