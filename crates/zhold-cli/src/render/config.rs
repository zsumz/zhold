use std::io::{self, Write};

use zhold_store::StoreConfig;

use super::output::output_error;
use crate::{CliError, app::OutputFormat, render::json};

pub(crate) fn setup(config: &StoreConfig, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        return json::write(config);
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let budget = config
        .arena_budget
        .map_or_else(|| "not configured".to_owned(), |value| value.to_string());
    writeln!(output, "zhold configured\n  arena budget  {budget}").map_err(output_error)?;
    if let Some(minimum) = config.min_filesystem_free {
        writeln!(output, "  minimum free  {minimum}").map_err(output_error)?;
    }
    if let Some(reservation) = config.minimum_build_reservation {
        writeln!(output, "  reserve floor {reservation}").map_err(output_error)?;
    }
    Ok(())
}
