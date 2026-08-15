use std::{path::Path, process::Command};

use crate::StoreError;

pub(super) fn required_output(
    description: &'static str,
    program: &str,
    arguments: &[String],
    working_directory: &Path,
    environment: Option<(&str, &str)>,
) -> Result<String, StoreError> {
    let output = command(program, arguments, working_directory, environment)
        .output()
        .map_err(|error| StoreError::io("spawn command", working_directory, error))?;
    if !output.status.success() {
        return Err(command_failed(description, &output));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|error| StoreError::CommandFailed {
            command: description.to_owned(),
            status: " with non-UTF-8 output".to_owned(),
            stderr: error.to_string(),
        })
}

pub(super) fn optional_output(
    description: &'static str,
    program: &str,
    arguments: &[String],
    working_directory: &Path,
) -> Result<Option<String>, StoreError> {
    let output = command(program, arguments, working_directory, None)
        .output()
        .map_err(|error| StoreError::io("spawn command", working_directory, error))?;
    if !output.status.success() {
        return Ok(None);
    }
    String::from_utf8(output.stdout)
        .map(|value| Some(value.trim().to_owned()))
        .map_err(|error| StoreError::CommandFailed {
            command: description.to_owned(),
            status: " with non-UTF-8 output".to_owned(),
            stderr: error.to_string(),
        })
}

fn command(
    program: &str,
    arguments: &[String],
    working_directory: &Path,
    environment: Option<(&str, &str)>,
) -> Command {
    let mut command = Command::new(program);
    command.args(arguments).current_dir(working_directory);
    if let Some((name, value)) = environment {
        command.env(name, value);
    }
    command
}

fn command_failed(description: &'static str, output: &std::process::Output) -> StoreError {
    let status = output.status.code().map_or_else(
        || " without an exit code".to_owned(),
        |code| format!(" with status {code}"),
    );
    StoreError::CommandFailed {
        command: description.to_owned(),
        status,
        stderr: "subprocess stderr omitted to protect invocation values".to_owned(),
    }
}
