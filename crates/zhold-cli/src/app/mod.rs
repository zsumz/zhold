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

pub(crate) use args::{Cli, Command, HistoryCommand, HookCommand, OutputFormat, QuotaCommand};
pub(crate) use duration::PinDuration;
