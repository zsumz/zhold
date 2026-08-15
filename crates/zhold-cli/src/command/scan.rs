use std::path::PathBuf;

use zhold_store::Store;

use crate::{
    CliError,
    app::{ExitStatus, OutputFormat},
    render,
};

pub(super) fn execute(
    store: &Store,
    paths: Vec<PathBuf>,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    let report = store.scan(&scan_roots(paths)?)?;
    render::scan(&report, format)?;
    Ok(ExitStatus::SUCCESS)
}

fn scan_roots(mut paths: Vec<PathBuf>) -> Result<Vec<PathBuf>, CliError> {
    if paths.is_empty() {
        paths.push(std::env::current_dir().map_err(CliError::CurrentDirectory)?);
    }
    Ok(paths)
}
