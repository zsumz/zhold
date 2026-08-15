//! Deterministic storage-governance vocabulary for `zhold`.
//!
//! The crate is deliberately free of filesystem and process I/O. Callers provide
//! observed arena records and receive deterministic retention decisions.

mod bytes;
mod identity;
mod model;
mod policy;

pub use bytes::{ByteSize, ParseByteSizeError};
pub use identity::{
    ArenaId, ParseIdentityError, RepositoryId, ToolchainId, WorkspaceId, WorktreeId, WorktreeKey,
};
pub use model::{
    ArenaRecord, ArenaState, BuildOutcome, CargoCommandClass, CollectionPlan, CollectionPolicy,
    CommandDescriptor, Eviction, EvictionReason, HistoryKind, HistoryPolicy, HookEvent, HookResult,
    ParseHistoryKindError, ParseQuotaProviderError, QuotaHealth, QuotaProvider, SizeQuality,
    WorktreeIntegrationState,
};
pub use policy::{PolicyError, plan_collection, plan_collection_with_reservation};
