use std::io::{self, Write};

use zhold_store::DoctorReport;

use super::output::output_error;
use crate::{CliError, app::OutputFormat, render::json};

pub(crate) fn doctor(report: &DoctorReport, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        return json::write(report);
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(
        output,
        "{} — {} arenas, {} findings, {} pending trash entries ({})",
        if report.healthy {
            "healthy"
        } else {
            "attention required"
        },
        report.arena_count,
        report.finding_count,
        report.trash_count,
        report.trash_bytes
    )
    .map_err(output_error)?;
    writeln!(
        output,
        "worktrees {} ({} draining) — quota {}",
        report.worktree_count,
        report.draining_worktree_count,
        if report.quota_healthy {
            "healthy"
        } else {
            "attention required"
        }
    )
    .map_err(output_error)?;
    writeln!(
        output,
        "physical {} — available {} — history {} receipts ({})",
        report.physical_bytes, report.available_bytes, report.history_count, report.history_bytes
    )
    .map_err(output_error)?;
    for finding in &report.findings {
        writeln!(output, "  {finding}").map_err(output_error)?;
    }
    Ok(())
}
