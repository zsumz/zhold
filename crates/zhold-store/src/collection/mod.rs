//! Deterministic whole-arena collection.

mod collector;
mod reconcile;
mod report;
mod trash;

#[cfg(test)]
mod collector_test;
#[cfg(test)]
mod reconcile_test;
#[cfg(test)]
mod trash_test;

pub(crate) use collector::{collect, collect_locked};
pub use report::{
    CollectionReport, CollectionSkip, Retirement, RetirementDisposition, TrashEntry, TrashOutcome,
    TrashReport,
};
pub(crate) use trash::retry_trash;
