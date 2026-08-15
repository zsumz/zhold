//! Manager-neutral worktree lifecycle coordination.

mod admission;
mod hooks;
mod model;
mod registry;
mod transitions;

#[cfg(test)]
mod worktree_test;

pub(crate) use admission::{WorktreeAdmission, acquire_admission};
pub use model::{HookMetadata, HookReport, WorktreeFinding, WorktreeIntegration, WorktreeSummary};
pub(crate) use registry::read_for_ids;
