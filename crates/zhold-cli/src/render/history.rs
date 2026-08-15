use std::io::{self, Write};

use serde::Serialize;
#[cfg(feature = "experimental")]
use zhold_core::HistoryPolicy;
use zhold_store::{
    BuildFinalization, FinalizationWarningEvent, HistoryWarning, HistoryWarningEvent,
};
#[cfg(feature = "experimental")]
use zhold_store::{HistoryPruneReport, HistoryReport};

use super::output::output_error;
use crate::{CliError, app::OutputFormat, render::json};

pub(crate) fn warnings(warnings: &[HistoryWarning], format: OutputFormat) -> Result<(), CliError> {
    for warning in warnings {
        if matches!(format, OutputFormat::Json) {
            #[derive(Serialize)]
            struct Event<'a> {
                event: &'static str,
                message: &'a str,
            }
            json::write_stderr(&Event {
                event: event_name(warning.event),
                message: &warning.message,
            })?;
        } else {
            let stderr = io::stderr();
            let mut output = stderr.lock();
            writeln!(
                output,
                "zhold  {}: {}",
                event_name(warning.event),
                warning.message
            )
            .map_err(output_error)?;
        }
    }
    Ok(())
}

pub(crate) fn finalization(
    finalization: &BuildFinalization,
    format: OutputFormat,
) -> Result<(), CliError> {
    for warning in &finalization.warnings {
        let event = match warning.event {
            FinalizationWarningEvent::ReservationLearningFailed => "reservation_learning_failed",
        };
        if matches!(format, OutputFormat::Json) {
            #[derive(Serialize)]
            struct Event<'a> {
                event: &'static str,
                message: &'a str,
            }
            json::write_stderr(&Event {
                event,
                message: &warning.message,
            })?;
        } else {
            let stderr = io::stderr();
            let mut output = stderr.lock();
            writeln!(output, "zhold  {event}: {}", warning.message).map_err(output_error)?;
        }
    }
    for write in &finalization.history {
        warnings(&write.warnings, format)?;
    }
    Ok(())
}

#[cfg(feature = "experimental")]
pub(crate) fn report(report: &HistoryReport, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        return json::write(report);
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(
        output,
        "history {} — showing {} newest-first{}",
        if report.policy.enabled {
            "enabled"
        } else {
            "disabled"
        },
        report.receipts.len(),
        if report.more { " (more available)" } else { "" }
    )
    .map_err(output_error)?;
    for receipt in &report.receipts {
        writeln!(
            output,
            "{}  {:<10} {}",
            receipt.recorded_at, receipt.kind, receipt.receipt_id
        )
        .map_err(output_error)?;
    }
    for finding in &report.findings {
        writeln!(
            output,
            "finding  {}: {}",
            finding.path.display(),
            finding.reason
        )
        .map_err(output_error)?;
    }
    Ok(())
}

#[cfg(feature = "experimental")]
pub(crate) fn pruned(report: &HistoryPruneReport, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        return json::write(report);
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(
        output,
        "{} {} receipts ({}) — {} remain ({})",
        if report.dry_run {
            "would remove"
        } else {
            "removed"
        },
        report.removed_count,
        report.removed_bytes,
        report.after_count,
        report.after_bytes
    )
    .map_err(output_error)?;
    for finding in &report.findings {
        writeln!(
            output,
            "finding  {}: {}",
            finding.path.display(),
            finding.reason
        )
        .map_err(output_error)?;
    }
    Ok(())
}

#[cfg(feature = "experimental")]
pub(crate) fn policy(
    policy: &HistoryPolicy,
    retention: Option<&HistoryPruneReport>,
    format: OutputFormat,
) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        #[derive(Serialize)]
        struct PolicyReport<'a> {
            policy: &'a HistoryPolicy,
            retention: Option<&'a HistoryPruneReport>,
        }
        return json::write(&PolicyReport { policy, retention });
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "enabled      {}", policy.enabled).map_err(output_error)?;
    writeln!(output, "max receipts {}", policy.max_receipts).map_err(output_error)?;
    writeln!(output, "max bytes    {}", policy.max_bytes).map_err(output_error)?;
    if let Some(retention) = retention {
        writeln!(output, "pruned       {} receipts", retention.removed_count)
            .map_err(output_error)?;
    }
    Ok(())
}

const fn event_name(event: HistoryWarningEvent) -> &'static str {
    match event {
        HistoryWarningEvent::PersistFailed => "history_persist_failed",
        HistoryWarningEvent::RetentionFailed => "history_retention_failed",
    }
}
