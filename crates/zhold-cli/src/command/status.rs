use zhold_store::Store;

use crate::{
    CliError,
    app::{ExitStatus, OutputFormat},
    render,
};

pub(super) fn execute(store: &Store, format: OutputFormat) -> Result<ExitStatus, CliError> {
    let inventory = store.inventory()?;
    render::inventory(&inventory, format)?;
    Ok(ExitStatus::SUCCESS)
}
