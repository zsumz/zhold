use zhold_core::ByteSize;
use zhold_store::{Store, StoreConfig};

use crate::{
    CliError,
    app::{ExitStatus, OutputFormat},
    render,
};

pub(super) fn execute(
    store: &Store,
    budget: ByteSize,
    min_free: Option<ByteSize>,
    build_reserve: Option<ByteSize>,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    let config = StoreConfig {
        arena_budget: Some(budget),
        min_filesystem_free: min_free,
        minimum_build_reservation: build_reserve,
    };
    store.set_config(config)?;
    render::setup(&config, format)?;
    Ok(ExitStatus::SUCCESS)
}
