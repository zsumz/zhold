use zhold_core::{ByteSize, CollectionPolicy};
use zhold_store::Store;

use crate::{
    CliError,
    app::{ExitStatus, OutputFormat},
    render,
};

#[derive(Clone, Copy, Debug)]
pub(super) struct GcOptions {
    pub(super) budget: Option<ByteSize>,
    pub(super) low_watermark: u8,
    pub(super) dry_run: bool,
    pub(super) trash_only: bool,
}

pub(super) fn execute(
    store: &Store,
    options: GcOptions,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    if options.trash_only {
        let report = store.retry_trash(options.dry_run)?;
        let complete = report.remaining == ByteSize::ZERO
            && report
                .entries
                .iter()
                .all(|entry| !matches!(entry.outcome, zhold_store::TrashOutcome::Skipped { .. }));
        render::trash(&report, format)?;
        return if complete {
            Ok(ExitStatus::SUCCESS)
        } else {
            Ok(ExitStatus::child(2))
        };
    }
    let budget = options.budget.ok_or(CliError::MissingBudget)?;
    let report = store.collect(
        CollectionPolicy {
            budget,
            low_watermark_percent: options.low_watermark,
        },
        options.dry_run,
    )?;
    let budget_met = report.budget_met;
    render::collection(&report, format)?;
    if budget_met {
        Ok(ExitStatus::SUCCESS)
    } else {
        Ok(ExitStatus::child(2))
    }
}
