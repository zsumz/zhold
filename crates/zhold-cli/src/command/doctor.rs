use zhold_store::Store;

use crate::{
    CliError,
    app::{ExitStatus, OutputFormat},
    render,
};

pub(super) fn execute(store: &Store, format: OutputFormat) -> Result<ExitStatus, CliError> {
    let report = store.doctor()?;
    let healthy = report.healthy;
    render::doctor(&report, format)?;
    if healthy {
        Ok(ExitStatus::SUCCESS)
    } else {
        Ok(ExitStatus::child(2))
    }
}
