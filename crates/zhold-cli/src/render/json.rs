use std::io::{self, Write};

use serde::Serialize;

use crate::CliError;

pub(super) fn write<T: Serialize>(value: &T) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    write_value(&mut output, value)
}

pub(super) fn write_stderr<T: Serialize>(value: &T) -> Result<(), CliError> {
    let stderr = io::stderr();
    let mut output = stderr.lock();
    write_value(&mut output, value)
}

fn write_value<T: Serialize>(output: &mut impl Write, value: &T) -> Result<(), CliError> {
    serde_json::to_writer(&mut *output, value)
        .map_err(|error| CliError::Output(error.to_string()))?;
    writeln!(output).map_err(|error| CliError::Output(error.to_string()))
}
