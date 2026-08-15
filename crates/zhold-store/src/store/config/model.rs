use serde::{Deserialize, Serialize};
use zhold_core::ByteSize;

/// Durable defaults for governed Cargo builds in one marked store.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StoreConfig {
    /// Steady-state active arena budget.
    pub arena_budget: Option<ByteSize>,
    /// Emergency free-space floor checked before Cargo starts.
    pub min_filesystem_free: Option<ByteSize>,
    /// Minimum per-build growth reservation before historical adjustment.
    pub minimum_build_reservation: Option<ByteSize>,
}
