//! CLI parsing and top-level dispatch.

mod args;
mod duration;
mod exit_status;
mod runner;

#[cfg(test)]
mod args_test;
#[cfg(test)]
mod duration_test;

pub use exit_status::ExitStatus;
pub use runner::{run, run_from};

pub(crate) use args::{Cli, Command, OutputFormat};
#[cfg(feature = "experimental")]
pub(crate) use args::{HistoryCommand, HookCommand, QuotaCommand};
pub(crate) use duration::PinDuration;
