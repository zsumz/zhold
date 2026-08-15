//! Command-line facade for `zhold`.

mod app;
mod command;
mod error;
mod render;

pub use app::{ExitStatus, run, run_from};
pub use error::CliError;
