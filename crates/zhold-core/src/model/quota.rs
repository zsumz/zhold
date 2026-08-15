use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Store-scoped filesystem quota provider.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaProvider {
    /// Select the best provider for the containing filesystem.
    Auto,
    /// XFS project quota.
    XfsProject,
    /// ext4 project quota.
    Ext4Project,
    /// Btrfs subvolume qgroup.
    BtrfsQgroup,
    /// Dedicated APFS volume quota.
    ApfsVolume,
    /// Windows Server FSRM directory quota.
    Fsrm,
}

impl Display for QuotaProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Auto => "auto",
            Self::XfsProject => "xfs-project",
            Self::Ext4Project => "ext4-project",
            Self::BtrfsQgroup => "btrfs-qgroup",
            Self::ApfsVolume => "apfs-volume",
            Self::Fsrm => "fsrm",
        })
    }
}

impl FromStr for QuotaProvider {
    type Err = ParseQuotaProviderError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "xfs-project" => Ok(Self::XfsProject),
            "ext4-project" => Ok(Self::Ext4Project),
            "btrfs-qgroup" => Ok(Self::BtrfsQgroup),
            "apfs-volume" => Ok(Self::ApfsVolume),
            "fsrm" => Ok(Self::Fsrm),
            _ => Err(ParseQuotaProviderError(value.to_owned())),
        }
    }
}

/// Error returned for an unknown quota provider.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("unknown quota provider `{0}`")]
pub struct ParseQuotaProviderError(String);

/// Verified health of an optional quota capability.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaHealth {
    /// No store-scoped provider exists on this platform or filesystem.
    Unsupported,
    /// A provider is available but no exact hard quota is configured.
    AvailableUnconfigured,
    /// The expected exact-scope hard quota is enforced.
    Configured,
    /// Observed enforcement differs from the adopted expectation.
    Drifted,
    /// Provider accounting is not currently trustworthy.
    Inconsistent,
    /// Inspection requires administrator permission.
    PermissionRequired,
    /// Required platform tooling could not be invoked or parsed.
    ProviderUnavailable,
}
