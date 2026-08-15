//! Owned build arenas and safe filesystem lifecycle for `zhold`.
//!
//! The crate exposes capabilities through a flat facade. Internal paths, lock files, manifests,
//! and mutation ordering are not public compatibility boundaries.

mod collection;
mod context;
mod error;
mod history;
mod inventory;
mod io;
mod layout;
mod lock;
mod manifest;
mod quota;
mod reservation;
mod scan;
mod store;
mod time;
mod worktree;

#[cfg(test)]
mod test_support;

pub use collection::{
    CollectionReport, CollectionSkip, Retirement, RetirementDisposition, TrashEntry, TrashOutcome,
    TrashReport,
};
pub use context::{BuildContext, CargoInvocation, ContextResolver, WorktreeContext};
pub use error::StoreError;
pub use history::{
    BuildFinalization, BuildReceipt, CollectionReceipt, CollectionReceiptSource,
    FinalizationWarning, FinalizationWarningEvent, HistoryFinding, HistoryPayload,
    HistoryPolicyDocument, HistoryPruneReport, HistoryPruneRequest, HistoryQuery, HistoryReceipt,
    HistoryReport, HistorySummary, HistoryWarning, HistoryWarningEvent, HistoryWrite, HookReceipt,
    QuotaReceipt, QuotaReceiptAction,
};
pub use inventory::{Inventory, InventoryDepth, InventoryEntry, InventoryFinding};
pub use quota::{
    QuotaAction, QuotaAdoption, QuotaExpectation, QuotaObservation, QuotaPlan, QuotaStatus,
};
pub use scan::{ForeignTarget, ScanReport};
pub use store::{ArenaLease, DoctorReport, Store, StoreConfig, StoreInfo};
pub use worktree::{
    HookMetadata, HookReport, WorktreeFinding, WorktreeIntegration, WorktreeSummary,
};
