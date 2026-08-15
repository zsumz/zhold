use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use zhold_core::ByteSize;
#[cfg(feature = "experimental")]
use zhold_core::{HistoryKind, QuotaProvider, WorktreeId};

use super::PinDuration;

/// Bounded Cargo build storage for parallel worktrees.
#[derive(Debug, Parser)]
#[command(name = "zhold", version, about, long_about = None)]
pub(crate) struct Cli {
    /// Marked storage root. Defaults to `ZHOLD_HOME` or the platform cache directory.
    #[arg(long, global = true, env = "ZHOLD_HOME")]
    pub(crate) store: Option<PathBuf>,
    /// Default storage budget for managed Cargo runs and garbage collection.
    #[arg(long, global = true, env = "ZHOLD_BUDGET")]
    pub(crate) budget: Option<ByteSize>,
    /// Minimum free bytes required on the store volume before Cargo starts.
    #[arg(long, global = true, env = "ZHOLD_MIN_FREE")]
    pub(crate) min_free: Option<ByteSize>,
    /// Minimum growth headroom; history may raise it conservatively.
    #[arg(long, global = true, env = "ZHOLD_BUILD_RESERVE")]
    pub(crate) build_reserve: Option<ByteSize>,
    /// Presentation format for zhold-owned output.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Human)]
    pub(crate) format: OutputFormat,
    /// Operation to perform. With no command, zhold shows status.
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Persist simple defaults for governed Cargo builds.
    Setup {
        /// Steady-state active arena budget.
        budget: ByteSize,
        /// Emergency free-space floor checked before Cargo starts.
        #[arg(long)]
        min_free: Option<ByteSize>,
        /// Minimum growth reservation before historical adjustment.
        #[arg(long)]
        build_reserve: Option<ByteSize>,
    },
    /// Run Cargo in a leased, worktree-specific build arena.
    Cargo {
        /// Cargo command and arguments. The `--` separator is optional.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<String>,
    },
    /// Inventory managed arenas and read-only foreign Cargo targets.
    Scan {
        /// Roots to inspect. Defaults to the current directory.
        paths: Vec<PathBuf>,
    },
    /// Show managed arena state.
    Status {
        /// Recursively reconcile arenas, trash, and physical store bytes.
        #[arg(long)]
        deep: bool,
    },
    /// Retire cold whole arenas until the store reaches its low watermark.
    Gc {
        /// Maximum desired managed store size. Falls back to `--budget` or `ZHOLD_BUDGET`.
        #[arg(value_name = "SIZE")]
        size: Option<ByteSize>,
        /// Collection target as a percentage of budget once collection triggers.
        #[arg(long, default_value_t = 80, value_parser = clap::value_parser!(u8).range(1..=100))]
        low_watermark: u8,
        /// Print the exact policy plan without mutating the store.
        #[arg(long)]
        dry_run: bool,
        /// Retry only already-retired owned trash; no active arena is selected.
        #[arg(long)]
        trash_only: bool,
    },
    /// Protect a managed arena from collection using its displayed ID prefix.
    Pin {
        /// Unique arena ID prefix from `zhold status`.
        arena: String,
        /// Automatically release the pin after a duration such as 12h or 7d.
        #[arg(long = "for", value_name = "DURATION")]
        duration: Option<PinDuration>,
    },
    /// Remove explicit collection protection from a managed arena.
    Unpin {
        /// Unique arena ID prefix from `zhold status`.
        arena: String,
    },
    /// Recover a suspect arena after confirming its orphaned process tree is gone.
    Recover {
        /// Unique suspect arena ID prefix from `zhold status`.
        arena: String,
        /// Confirm that the orphaned Cargo process tree has terminated.
        #[arg(long, required = true)]
        terminated: bool,
    },
    /// Validate store ownership, metadata, and retirement health.
    Doctor,
    /// Explain one arena's identity, protection, and collection state.
    Explain {
        /// Unique arena ID prefix from `zhold status`.
        arena: String,
    },
    /// Query or configure bounded persistent operation history.
    #[cfg(feature = "experimental")]
    History {
        /// Restrict receipts to one category.
        #[arg(long)]
        kind: Option<HistoryKind>,
        /// Restrict build receipts to an arena ID prefix.
        #[arg(long)]
        arena: Option<String>,
        /// Restrict build and hook receipts to one worktree identity.
        #[arg(long)]
        worktree: Option<WorktreeId>,
        /// Restrict receipts to the recent duration.
        #[arg(long)]
        since: Option<PinDuration>,
        /// Maximum matching receipts to return.
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// History maintenance operation.
        #[command(subcommand)]
        action: Option<HistoryCommand>,
    },
    /// Integrate an external worktree manager with build/removal coordination.
    #[cfg(feature = "experimental")]
    Hook {
        /// Worktree lifecycle event.
        #[command(subcommand)]
        action: HookCommand,
    },
    /// Inspect or adopt an externally provisioned store-scoped hard quota.
    #[cfg(feature = "experimental")]
    Quota {
        /// Quota operation.
        #[command(subcommand)]
        action: QuotaCommand,
    },
}

#[derive(Debug, Subcommand)]
#[cfg(feature = "experimental")]
pub(crate) enum HistoryCommand {
    /// Remove only validated receipts selected by deterministic bounds.
    Prune {
        /// Keep at most this many newest validated receipts.
        #[arg(long)]
        keep: Option<u64>,
        /// Keep at most this many bytes of validated receipts.
        #[arg(long)]
        max_bytes: Option<ByteSize>,
        /// Remove receipts older than this duration.
        #[arg(long)]
        older_than: Option<PinDuration>,
        /// Print the exact removal plan without mutation.
        #[arg(long)]
        dry_run: bool,
    },
    /// Show or update the persisted default retention policy.
    Policy {
        /// Enable or disable publication of new receipts.
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: Option<bool>,
        /// Maximum number of validated receipts retained.
        #[arg(long)]
        max_receipts: Option<u64>,
        /// Maximum bytes across validated receipt files.
        #[arg(long)]
        max_bytes: Option<ByteSize>,
    },
}

#[derive(Debug, Subcommand)]
#[cfg(feature = "experimental")]
pub(crate) enum HookCommand {
    /// Register or reactivate a validated Git worktree.
    Ready {
        /// Existing worktree path.
        #[arg(long)]
        path: PathBuf,
        /// Worktree manager name.
        #[arg(long)]
        manager: Option<String>,
        /// User-facing worktree label.
        #[arg(long)]
        label: Option<String>,
        /// Manager session identity.
        #[arg(long)]
        session: Option<String>,
    },
    /// Establish a draining guard before worktree removal.
    PrepareRemove {
        /// Registered worktree path.
        #[arg(long)]
        path: PathBuf,
        /// Worktree manager name.
        #[arg(long)]
        manager: Option<String>,
    },
    /// Confirm that a draining registered path is absent.
    Removed {
        /// Registered worktree path, which must now be absent.
        #[arg(long)]
        path: PathBuf,
        /// Worktree manager name.
        #[arg(long)]
        manager: Option<String>,
    },
    /// Recover a validated worktree after manager removal failed.
    CancelRemove {
        /// Existing registered worktree path.
        #[arg(long)]
        path: PathBuf,
        /// Worktree manager name.
        #[arg(long)]
        manager: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
#[cfg(feature = "experimental")]
pub(crate) enum QuotaCommand {
    /// Inspect provider capability and adopted expectation health.
    Status,
    /// Print external administrator requirements without executing them.
    Plan {
        /// Requested store hard limit.
        hard_limit: ByteSize,
        /// Explicit provider; defaults to platform/filesystem detection.
        #[arg(long, default_value_t = QuotaProvider::Auto)]
        provider: QuotaProvider,
    },
    /// Adopt an already-provisioned exact-scope hard quota.
    Adopt {
        /// Expected store hard limit, which must match provider observation exactly.
        hard_limit: ByteSize,
        /// Explicit provider; defaults to platform/filesystem detection.
        #[arg(long, default_value_t = QuotaProvider::Auto)]
        provider: QuotaProvider,
    },
    /// Remove zhold's expectation without changing operating-system enforcement.
    Unadopt,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub(crate) enum OutputFormat {
    #[default]
    Human,
    Json,
}
