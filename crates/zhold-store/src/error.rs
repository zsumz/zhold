use std::{io, path::PathBuf};

use thiserror::Error;
use zhold_core::PolicyError;

/// Failure at the zhold filesystem or process boundary.
#[derive(Debug, Error)]
pub enum StoreError {
    /// A filesystem operation failed.
    #[error("failed to {operation} `{path}`: {source}")]
    Io {
        /// Human-readable operation name.
        operation: &'static str,
        /// Path involved in the failed operation.
        path: PathBuf,
        /// Underlying operating-system error.
        #[source]
        source: Box<io::Error>,
    },
    /// Persisted JSON could not be decoded or encoded.
    #[error("invalid zhold metadata at `{path}`: {source}")]
    Json {
        /// Metadata path.
        path: PathBuf,
        /// JSON codec failure.
        #[source]
        source: Box<serde_json::Error>,
    },
    /// A non-empty directory did not contain a valid zhold store marker.
    #[error("refusing to claim non-empty unmarked store root `{0}`")]
    UnmarkedStore(PathBuf),
    /// Persisted metadata belongs to another store or identity.
    #[error("ownership validation failed for `{path}`: {reason}")]
    InvalidOwnership {
        /// Rejected path.
        path: PathBuf,
        /// Validation explanation.
        reason: String,
    },
    /// A required external command failed.
    #[error("command `{command}` failed{status}: {stderr}")]
    CommandFailed {
        /// Rendered command.
        command: String,
        /// Rendered process status.
        status: String,
        /// Captured standard error.
        stderr: String,
    },
    /// A path or command argument was not valid Unicode.
    #[error("{kind} is not valid Unicode: `{path}`")]
    NonUnicode {
        /// Kind of value that was rejected.
        kind: &'static str,
        /// Lossy representation for diagnosis.
        path: PathBuf,
    },
    /// The wrapped command is not Cargo.
    #[error("zhold can manage only Cargo commands; received `{0}`")]
    NotCargo(String),
    /// Cargo invocation options required for safe context resolution were malformed.
    #[error("invalid Cargo invocation: {0}")]
    InvalidCargoInvocation(String),
    /// Cargo metadata returned output that could not establish a workspace.
    #[error("invalid Cargo metadata output: {0}")]
    InvalidCargoMetadata(String),
    /// The discovered Cargo version predates managed build directories.
    #[error("Cargo {found} is unsupported; zhold requires Cargo 1.91 or newer")]
    UnsupportedCargo {
        /// Parsed Cargo release.
        found: String,
    },
    /// No platform cache root could be derived from the process environment.
    #[error("cannot determine a default cache directory; pass --store or set ZHOLD_HOME")]
    MissingCacheRoot,
    /// The system clock predates the Unix epoch.
    #[error("system clock predates the Unix epoch")]
    InvalidClock,
    /// A requested pin duration cannot be represented as an absolute timestamp.
    #[error("pin duration exceeds the representable timestamp range")]
    PinExpirationOverflow,
    /// A managed arena was not found.
    #[error("managed arena `{0}` was not found")]
    ArenaNotFound(String),
    /// An arena could not be changed because a build currently holds its lease.
    #[error("managed arena `{0}` currently has an active build lease")]
    ArenaActive(String),
    /// A registered worktree lifecycle state denies new builds.
    #[error("worktree admission is blocked for `{path}`: {state}")]
    WorktreeAdmissionBlocked {
        /// Registered canonical worktree path.
        path: PathBuf,
        /// Durable lifecycle state.
        state: String,
    },
    /// A manager-provided metadata value violated its bounded text contract.
    #[error(
        "worktree hook {field} must be at most {maximum} UTF-8 bytes and contain no control characters"
    )]
    InvalidHookValue {
        /// Rejected metadata field.
        field: &'static str,
        /// Maximum accepted encoded length, alongside the control-free requirement.
        maximum: usize,
    },
    /// An adopted quota could not safely admit a managed build.
    #[error("quota admission is blocked: {0}")]
    QuotaAdmissionBlocked(String),
    /// A collection policy was invalid.
    #[error(transparent)]
    Policy(#[from] PolicyError),
}

impl StoreError {
    pub(crate) fn io(operation: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            source: Box::new(source),
        }
    }

    pub(crate) fn json(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        Self::Json {
            path: path.into(),
            source: Box::new(source),
        }
    }
}
