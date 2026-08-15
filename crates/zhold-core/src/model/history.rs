use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ByteSize;

/// Durable operation category stored in history.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    /// A completed managed build.
    Build,
    /// A committed collection attempt.
    Collection,
    /// A worktree integration transition.
    Hook,
    /// A quota expectation transition or drift observation.
    Quota,
}

impl Display for HistoryKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Build => "build",
            Self::Collection => "collection",
            Self::Hook => "hook",
            Self::Quota => "quota",
        })
    }
}

impl FromStr for HistoryKind {
    type Err = ParseHistoryKindError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "build" => Ok(Self::Build),
            "collection" => Ok(Self::Collection),
            "hook" => Ok(Self::Hook),
            "quota" => Ok(Self::Quota),
            _ => Err(ParseHistoryKindError(value.to_owned())),
        }
    }
}

/// Error returned for an unknown history category.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("unknown history kind `{0}`; expected build, collection, hook, or quota")]
pub struct ParseHistoryKindError(String);

/// Persisted receipt-retention policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryPolicy {
    /// Whether new receipts are published.
    pub enabled: bool,
    /// Maximum number of validated receipt files.
    pub max_receipts: u64,
    /// Maximum total bytes across validated receipt files.
    pub max_bytes: ByteSize,
}

impl Default for HistoryPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            max_receipts: 10_000,
            max_bytes: ByteSize::from_bytes(64 * 1_024 * 1_024),
        }
    }
}
