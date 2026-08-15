use zhold_store::Store;

use crate::{
    CliError,
    app::{ExitStatus, OutputFormat},
    render,
};

pub(super) fn execute(
    store: &Store,
    selector: &str,
    pinned: bool,
    duration: Option<u64>,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    let arena = super::selector::resolve(store, selector)?;
    let expires_at = if pinned {
        store.pin_for(&arena, duration)?
    } else {
        store.set_pinned(&arena, false)?;
        None
    };
    render::pin(&arena, pinned, expires_at, format)?;
    Ok(ExitStatus::SUCCESS)
}
