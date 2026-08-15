use std::io::{self, Write};

use serde::Serialize;
use zhold_core::{ArenaId, EvictionReason};
use zhold_store::{CollectionReport, RetirementDisposition, TrashOutcome, TrashReport};

use super::output::output_error;
use crate::{CliError, app::OutputFormat, render::json};

pub(crate) fn preflight(report: &CollectionReport, format: OutputFormat) -> Result<(), CliError> {
    if report.retirements.is_empty() && report.skipped.is_empty() {
        return Ok(());
    }
    if matches!(format, OutputFormat::Json) {
        #[derive(Serialize)]
        struct Preflight<'a> {
            event: &'static str,
            report: &'a CollectionReport,
        }
        return json::write_stderr(&Preflight {
            event: "preflight_collection",
            report,
        });
    }
    let stderr = io::stderr();
    let mut output = stderr.lock();
    writeln!(
        output,
        "zhold  retired {} arenas; reclaimed {}; {} active remain",
        report.retirements.len(),
        report.reclaimed,
        report.after
    )
    .map_err(output_error)?;
    for retirement in &report.retirements {
        if let RetirementDisposition::PendingDeletion { path, error } = &retirement.disposition {
            writeln!(
                output,
                "zhold  pending deletion at {}: {error}",
                path.display()
            )
            .map_err(output_error)?;
        }
    }
    for skipped in &report.skipped {
        writeln!(
            output,
            "zhold  kept {}: {}",
            short_id(&skipped.arena_id),
            skipped.reason
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn collection(report: &CollectionReport, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        json::write(report)?;
        return super::history::warnings(&report.history.warnings, format);
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "before      {}", report.plan.before).map_err(output_error)?;
    writeln!(output, "target      {}", report.plan.target).map_err(output_error)?;
    writeln!(output, "budget      {}", report.budget).map_err(output_error)?;
    writeln!(output, "reserved    {}", report.reserved).map_err(output_error)?;
    writeln!(
        output,
        "{}       {}",
        if report.dry_run {
            "projected"
        } else {
            "after    "
        },
        report.after
    )
    .map_err(output_error)?;
    writeln!(output, "protected   {}", report.plan.protected).map_err(output_error)?;
    writeln!(
        output,
        "{}   {}",
        if report.dry_run {
            "reclaimable"
        } else {
            "reclaimed  "
        },
        if report.dry_run {
            report.plan.reclaimable
        } else {
            report.reclaimed
        }
    )
    .map_err(output_error)?;
    writeln!(output, "budget met  {}", report.budget_met).map_err(output_error)?;
    if report.dry_run {
        writeln!(output, "\nwould retire:").map_err(output_error)?;
        for eviction in &report.plan.evictions {
            writeln!(
                output,
                "  {:<10} {}  {}",
                eviction.size,
                short_id(&eviction.arena_id),
                reason(eviction.reason)
            )
            .map_err(output_error)?;
        }
    } else {
        for retirement in &report.retirements {
            let disposition = match &retirement.disposition {
                RetirementDisposition::Deleted => "deleted".to_owned(),
                RetirementDisposition::PendingDeletion { path, error } => {
                    format!("pending at {}: {error}", path.display())
                }
            };
            writeln!(
                output,
                "retired     {}  {}  {disposition}",
                short_id(&retirement.arena_id),
                retirement.size
            )
            .map_err(output_error)?;
        }
        for skipped in &report.skipped {
            writeln!(
                output,
                "skipped     {}  {}",
                short_id(&skipped.arena_id),
                skipped.reason
            )
            .map_err(output_error)?;
        }
    }
    drop(output);
    super::history::warnings(&report.history.warnings, format)
}

pub(crate) fn trash(report: &TrashReport, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        json::write(report)?;
        return super::history::warnings(&report.history.warnings, format);
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "trash before {}", report.before).map_err(output_error)?;
    writeln!(
        output,
        "{} {}",
        if report.dry_run {
            "projected  "
        } else {
            "reclaimed  "
        },
        if report.dry_run {
            report.before.saturating_sub(report.remaining)
        } else {
            report.reclaimed
        }
    )
    .map_err(output_error)?;
    writeln!(output, "remaining    {}", report.remaining).map_err(output_error)?;
    for entry in &report.entries {
        let outcome = match &entry.outcome {
            TrashOutcome::WouldDelete => "would delete".to_owned(),
            TrashOutcome::Deleted => "deleted".to_owned(),
            TrashOutcome::Skipped { error } => format!("skipped: {error}"),
        };
        writeln!(
            output,
            "{outcome:<12} {}  {}",
            entry.size,
            entry.path.display()
        )
        .map_err(output_error)?;
    }
    drop(output);
    super::history::warnings(&report.history.warnings, format)
}

pub(crate) fn pin(
    arena: &ArenaId,
    pinned: bool,
    expires_at: Option<u64>,
    format: OutputFormat,
) -> Result<(), CliError> {
    #[derive(Serialize)]
    struct PinReport<'a> {
        arena_id: &'a ArenaId,
        pinned: bool,
        expires_at: Option<u64>,
    }
    if matches!(format, OutputFormat::Json) {
        return json::write(&PinReport {
            arena_id: arena,
            pinned,
            expires_at,
        });
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let expiration = expires_at.map_or_else(String::new, |value| format!(" until Unix {value}"));
    writeln!(
        output,
        "{} {}{expiration}",
        if pinned { "pinned" } else { "unpinned" },
        arena
    )
    .map_err(output_error)
}

fn short_id(arena: &ArenaId) -> &str {
    arena.as_str().get(..10).unwrap_or(arena.as_str())
}

fn reason(reason: EvictionReason) -> &'static str {
    match reason {
        EvictionReason::OrphanedWorktree => "orphaned worktree",
        EvictionReason::FailedBuild => "failed build",
        EvictionReason::LeastRecentlyUsed => "least recently used",
    }
}
