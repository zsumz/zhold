//! Arena observations and collection decisions.

mod arena;
mod collection;
mod history;
#[cfg(test)]
mod history_test;
mod quota;
#[cfg(test)]
mod quota_test;
mod worktree;

pub use arena::{ArenaRecord, ArenaState, BuildOutcome};
pub use collection::{CollectionPlan, CollectionPolicy, Eviction, EvictionReason};
pub use history::{HistoryKind, HistoryPolicy, ParseHistoryKindError};
pub use quota::{ParseQuotaProviderError, QuotaHealth, QuotaProvider};
pub use worktree::{HookEvent, HookResult, WorktreeIntegrationState};
