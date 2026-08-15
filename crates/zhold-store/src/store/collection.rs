use zhold_core::CollectionPolicy;

use crate::{
    CollectionReport, Store, StoreError, TrashReport,
    collection::{collect, retry_trash},
    history::{CollectionReceiptSource, HistoryDraft, persist},
};

impl Store {
    /// Plans or executes deterministic whole-arena collection.
    pub fn collect(
        &self,
        policy: CollectionPolicy,
        dry_run: bool,
    ) -> Result<CollectionReport, StoreError> {
        let mut report = collect(self, policy, dry_run)?;
        if !dry_run {
            report.history = persist(
                self,
                HistoryDraft::collection(&report, CollectionReceiptSource::Manual),
            );
        }
        Ok(report)
    }

    /// Restores the steady-state budget after a managed build releases its lease.
    pub fn collect_post_build(
        &self,
        policy: CollectionPolicy,
    ) -> Result<CollectionReport, StoreError> {
        let mut report = collect(self, policy, false)?;
        report.history = persist(
            self,
            HistoryDraft::collection(&report, CollectionReceiptSource::PostBuild),
        );
        Ok(report)
    }

    /// Retries deletion of already-retired, validated owned trash entries.
    pub fn retry_trash(&self, dry_run: bool) -> Result<TrashReport, StoreError> {
        let mut report = retry_trash(self, dry_run)?;
        if !dry_run {
            report.history = persist(self, HistoryDraft::trash(&report));
        }
        Ok(report)
    }
}
