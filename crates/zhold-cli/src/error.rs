use std::{io, path::PathBuf};

use thiserror::Error;

/// Failure to parse, execute, or render a zhold command.
#[derive(Debug, Error)]
pub enum CliError {
    /// Command-line arguments were invalid.
    #[error(transparent)]
    Arguments(#[from] clap::Error),
    /// Store or context operation failed.
    #[error(transparent)]
    Store(#[from] zhold_store::StoreError),
    /// Collection was requested without a configured budget.
    #[error("gc requires a budget, for example `zhold gc 200GiB` or ZHOLD_BUDGET=200GiB")]
    MissingBudget,
    /// Managed Cargo requires an explicit or persisted governance budget.
    #[error("managed Cargo requires a budget; run `zhold setup 200GiB` first")]
    MissingCargoBudget,
    /// The caller already selected a Cargo intermediate directory.
    #[error("CARGO_BUILD_BUILD_DIR is already set; refusing to replace the caller's build store")]
    ConflictingBuildDirectory,
    /// Preflight collection could not reach the requested budget.
    #[error(
        "store cannot admit the build after safe collection: {after} active + {reserved} reserved > {budget}"
    )]
    BudgetUnmet {
        /// Store size after confirmed collection.
        after: zhold_core::ByteSize,
        /// Additional headroom declared by live leases.
        reserved: zhold_core::ByteSize,
        /// Requested hard budget.
        budget: zhold_core::ByteSize,
    },
    /// The store volume lacks the configured emergency free-space floor.
    #[error("store volume has only {available} available; configured minimum is {minimum}")]
    InsufficientFreeSpace {
        /// Bytes available immediately before Cargo start.
        available: zhold_core::ByteSize,
        /// Configured minimum available bytes.
        minimum: zhold_core::ByteSize,
    },
    /// Arena ID prefixes must be long enough to be useful.
    #[error("arena prefix `{selector}` is too short; use at least {minimum} characters")]
    ArenaSelectorTooShort {
        /// Rejected selector.
        selector: String,
        /// Minimum accepted length.
        minimum: usize,
    },
    /// Arena selectors are lowercase hexadecimal prefixes.
    #[error("invalid arena ID prefix `{0}`; copy the prefix shown by `zhold status`")]
    InvalidArenaSelector(String),
    /// No managed arena matched the selector.
    #[error("no managed arena matches `{0}`")]
    ArenaSelectorNotFound(String),
    /// More than one managed arena matched the selector.
    #[error("arena prefix `{selector}` matches {count} arenas; provide more characters")]
    ArenaSelectorAmbiguous {
        /// Ambiguous selector.
        selector: String,
        /// Number of matching arenas.
        count: usize,
    },
    /// The wrapped Cargo process could not be started.
    #[error("failed to start Cargo in `{directory}`: {source}")]
    Spawn {
        /// Working directory.
        directory: PathBuf,
        /// Operating-system process error.
        #[source]
        source: Box<io::Error>,
    },
    /// The wrapped Cargo process could not be observed through completion.
    #[error("failed while waiting for Cargo in `{directory}`: {source}")]
    Wait {
        /// Working directory used by Cargo.
        directory: PathBuf,
        /// Operating-system process error.
        #[source]
        source: Box<io::Error>,
    },
    /// Current executable could not be resolved for the lease sentinel.
    #[error("failed to resolve the zhold executable: {0}")]
    CurrentExecutable(#[source] io::Error),
    /// The lease sentinel could not be started.
    #[error("failed to start the Cargo lease sentinel: {0}")]
    Sentinel(#[source] Box<io::Error>),
    /// Current working directory could not be resolved.
    #[error("failed to resolve current working directory: {0}")]
    CurrentDirectory(#[source] io::Error),
    /// The system clock predates the Unix epoch.
    #[error("system clock predates the Unix epoch")]
    InvalidClock,
    /// Filesystem hard quotas require a positive byte ceiling.
    #[error("quota hard limit must be greater than zero")]
    InvalidQuotaLimit,
    /// Structured output could not be encoded or written.
    #[error("failed to write command output: {0}")]
    Output(String),
}
