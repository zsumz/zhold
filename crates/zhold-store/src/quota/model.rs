use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::{ByteSize, QuotaHealth, QuotaProvider};

/// Fresh provider observation for the canonical store root.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuotaObservation {
    /// Provider selected for this observation.
    pub provider: QuotaProvider,
    /// Honest enforcement or capability state.
    pub health: QuotaHealth,
    /// Canonical scope observed by the provider.
    pub scope: PathBuf,
    /// Stable containing filesystem or volume identity, when known.
    pub filesystem_id: Option<String>,
    /// Stable quota object identity, when known.
    pub quota_id: Option<String>,
    /// Whether the provider proves scope exactly equals the store root.
    pub exact_scope: bool,
    /// Whether writes are refused at the observed limit.
    pub hard_enforcement: bool,
    /// Provider-accounted bytes currently used.
    pub usage: Option<ByteSize>,
    /// Provider-enforced maximum bytes.
    pub limit: Option<ByteSize>,
    /// Bounded diagnostic explanation without raw provider output.
    pub detail: String,
}

impl QuotaObservation {
    pub(crate) fn unavailable(
        provider: QuotaProvider,
        scope: PathBuf,
        health: QuotaHealth,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            health,
            scope,
            filesystem_id: None,
            quota_id: None,
            exact_scope: false,
            hard_enforcement: false,
            usage: None,
            limit: None,
            detail: detail.into(),
        }
    }
}

/// Persisted expectation for one externally provisioned exact-scope hard quota.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuotaExpectation {
    /// Persisted schema version.
    pub schema_version: u32,
    /// Marked store identity.
    pub store_id: Uuid,
    /// Adopted provider.
    pub provider: QuotaProvider,
    /// Stable containing filesystem identity.
    pub filesystem_id: String,
    /// Stable provider quota-object identity.
    pub quota_id: String,
    /// Canonical exact store scope.
    pub scope: PathBuf,
    /// Expected hard limit.
    pub hard_limit: ByteSize,
    /// Adoption time as Unix milliseconds.
    pub adopted_at: u64,
}

/// One read-only administrator action proposed by `quota plan`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuotaAction {
    /// Ordered plan step.
    pub order: u32,
    /// Human-readable administrator action.
    pub description: String,
    /// Whether the step normally requires elevated privileges.
    pub privilege_required: bool,
    /// Direct executable when a safe fixed-vector command can be proposed.
    pub program: Option<String>,
    /// Ordered command arguments, never executed by zhold.
    pub arguments: Vec<String>,
}

/// Read-only provider plan for a requested hard ceiling.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuotaPlan {
    /// Requested hard limit.
    pub hard_limit: ByteSize,
    /// Current provider observation.
    pub observation: QuotaObservation,
    /// Preconditions that must be true before adoption.
    pub requirements: Vec<String>,
    /// Ordered external administrator actions; zhold executes none of them.
    pub actions: Vec<QuotaAction>,
}

/// Complete current adoption and provider health.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuotaStatus {
    /// Persisted expectation when quota enforcement was adopted.
    pub expectation: Option<QuotaExpectation>,
    /// Fresh provider observation.
    pub observation: QuotaObservation,
    /// Whether an adopted expectation exactly matches healthy enforcement.
    pub healthy: bool,
    /// Provider-reported capacity remaining beneath the hard limit.
    pub remaining: Option<ByteSize>,
}

/// Result of adopting or unadopting an expectation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuotaAdoption {
    /// Whether expectation metadata changed.
    pub changed: bool,
    /// Whether administrator attention is required before adoption can succeed.
    pub attention_required: bool,
    /// Current complete quota status.
    pub status: QuotaStatus,
    /// Human-readable result explanation.
    pub message: String,
    /// Nonfatal quota receipt result.
    pub history: crate::HistoryWrite,
}
