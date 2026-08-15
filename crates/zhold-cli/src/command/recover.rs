use zhold_store::Store;

use crate::{
    CliError,
    app::{ExitStatus, OutputFormat},
    render,
};

pub(super) fn execute(
    store: &Store,
    selector: &str,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    let arena = super::selector::resolve(store, selector)?;
    let history = store.recover_suspect(&arena)?;
    render::recovery(&arena, format)?;
    render::history_warnings(&history.warnings, format)?;
    Ok(ExitStatus::SUCCESS)
}
