//! Bounded immutable operation receipts.

mod draft;
mod index;
mod model;
mod prune;
mod reader;
mod receipt;
mod writer;

#[cfg(test)]
mod history_test;

pub(crate) use model::HistoryDraft;
pub use model::{
    BuildFinalization, FinalizationWarning, FinalizationWarningEvent, HistoryFinding,
    HistoryPolicyDocument, HistoryPruneReport, HistoryPruneRequest, HistoryQuery, HistoryReport,
    HistorySummary, HistoryWarning, HistoryWarningEvent, HistoryWrite,
};
pub(crate) use prune::prune;
pub(crate) use reader::{history_policy, read_history, summary};
pub use receipt::{
    BuildReceipt, CollectionReceipt, CollectionReceiptSource, HistoryPayload, HistoryReceipt,
    HookReceipt, QuotaReceipt, QuotaReceiptAction,
};
pub(crate) use writer::{persist, set_policy};
