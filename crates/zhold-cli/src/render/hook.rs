use std::io::{self, Write};

use zhold_store::HookReport;

use super::output::output_error;
use crate::{CliError, app::OutputFormat, render::json};

pub(crate) fn hook(report: &HookReport, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        json::write(report)?;
    } else {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        writeln!(output, "{:?}: {}", report.result, report.message).map_err(output_error)?;
        if let Some(integration) = &report.integration {
            writeln!(
                output,
                "worktree {} — {:?} — {}",
                integration.worktree_key,
                integration.state,
                integration.canonical_path.display()
            )
            .map_err(output_error)?;
        }
    }
    super::history::warnings(&report.history.warnings, format)
}
