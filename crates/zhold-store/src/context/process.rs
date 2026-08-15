use std::{path::Path, process::Command};

use crate::StoreError;

pub(super) fn required_output(
    program: &str,
    arguments: &[String],
    working_directory: &Path,
    environment: Option<(&str, &str)>,
) -> Result<String, StoreError> {
    let output = command(program, arguments, working_directory, environment)
        .output()
        .map_err(|error| StoreError::io("spawn command", working_directory, error))?;
    if !output.status.success() {
        return Err(command_failed(program, arguments, &output));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|error| StoreError::CommandFailed {
            command: render(program, arguments),
            status: " with non-UTF-8 output".to_owned(),
            stderr: error.to_string(),
        })
}

pub(super) fn optional_output(
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
            command: render(program, arguments),
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

fn command_failed(
    program: &str,
    arguments: &[String],
    output: &std::process::Output,
) -> StoreError {
    let status = output.status.code().map_or_else(
        || " without an exit code".to_owned(),
        |code| format!(" with status {code}"),
    );
    StoreError::CommandFailed {
        command: render(program, arguments),
        status,
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    }
}

fn render(program: &str, arguments: &[String]) -> String {
    std::iter::once(program)
        .chain(arguments.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}
