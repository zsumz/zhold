use std::io::{self, Write};

use zhold_store::ScanReport;

use super::output::output_error;
use crate::{
    CliError,
    app::OutputFormat,
    render::{inventory, json},
};

pub(crate) fn scan(report: &ScanReport, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        return json::write(report);
    }
    inventory(&report.managed, format)?;
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(
        output,
        "\nforeign Cargo targets: {}",
        report.foreign_targets.len()
    )
    .map_err(output_error)?;
    for target in &report.foreign_targets {
        writeln!(output, "  {:<10} {}", target.size, target.path.display())
            .map_err(output_error)?;
    }
    for finding in &report.findings {
        writeln!(
            output,
            "scan finding  {}: {}",
            finding.path.display(),
            finding.reason
        )
        .map_err(output_error)?;
    }
    Ok(())
}
