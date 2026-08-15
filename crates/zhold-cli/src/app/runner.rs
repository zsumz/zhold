use std::ffi::OsString;

use clap::{Parser, error::ErrorKind};

use super::{Cli, ExitStatus};
use crate::{CliError, command};

/// Parses process arguments and executes one zhold command.
pub fn run() -> Result<ExitStatus, CliError> {
    run_from(std::env::args_os())
}

/// Parses an explicit argument iterator and executes one zhold command.
pub fn run_from<I, T>(arguments: I) -> Result<ExitStatus, CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            error
                .print()
                .map_err(|source| CliError::Output(source.to_string()))?;
            return Ok(ExitStatus::SUCCESS);
        }
        Err(error) => return Err(CliError::Arguments(error)),
    };
    command::execute(cli)
}
