use zhold_store::Store;

use crate::{
    CliError,
    app::{ExitStatus, OutputFormat},
    render,
};

pub(super) fn execute(
    store: &Store,
    deep: bool,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    let inventory = if deep {
        store.inventory()?
    } else {
        store.inventory_cached()?
    };
    render::inventory(&inventory, format)?;
    Ok(ExitStatus::SUCCESS)
}
