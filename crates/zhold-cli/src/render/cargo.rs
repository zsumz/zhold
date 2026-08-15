use std::{
    io::{self, Write},
    path::Path,
};

use serde::Serialize;
use zhold_core::{ArenaId, BuildOutcome, ByteSize};
use zhold_store::BuildFinalization;

use super::output::output_error;
use crate::{CliError, app::OutputFormat, command::CargoReport, render::json};

pub(crate) fn cargo_start(
    arena: &ArenaId,
    build_dir: &Path,
    reservation: ByteSize,
    format: OutputFormat,
) -> Result<(), CliError> {
    #[derive(Serialize)]
    struct Start<'a> {
        event: &'static str,
        arena_id: &'a ArenaId,
        build_dir: &'a Path,
        reservation: ByteSize,
    }
    if matches!(format, OutputFormat::Json) {
        return json::write_stderr(&Start {
            event: "cargo_started",
            arena_id: arena,
            build_dir,
            reservation,
        });
    }
    let stderr = io::stderr();
    let mut output = stderr.lock();
    writeln!(
        output,
        "zhold  arena {}\n       build {}\n       reserved {} growth",
        short_id(arena),
        build_dir.display(),
        reservation
    )
    .map_err(output_error)
}

pub(crate) fn cargo_finish(report: &CargoReport, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        #[derive(Serialize)]
        struct Finish<'a> {
            event: &'static str,
            arena_id: &'a ArenaId,
            build_dir: &'a Path,
            outcome: BuildOutcome,
            exit_code: i32,
            reservation: ByteSize,
            peak_size: zhold_core::ByteSize,
            size_limit_exceeded: bool,
        }
        return json::write_stderr(&Finish {
            event: "cargo_finished",
            arena_id: &report.arena_id,
            build_dir: &report.build_dir,
            outcome: report.outcome,
            exit_code: report.exit_code,
            reservation: report.reservation,
            peak_size: report.peak_size,
            size_limit_exceeded: report.size_limit_exceeded,
        });
    }
    let stderr = io::stderr();
    let mut output = stderr.lock();
    writeln!(
        output,
        "zhold  {} (exit {})",
        outcome_name(report.outcome),
        report.exit_code
    )
    .map_err(output_error)
}

pub(crate) fn cargo_finish_with_history(
    report: &CargoReport,
    finalization: &BuildFinalization,
    format: OutputFormat,
) -> Result<(), CliError> {
    cargo_finish(report, format)?;
    super::history::finalization(finalization, format)
}

pub(crate) fn cargo_size_limit_exceeded(
    arena: &ArenaId,
    observed: zhold_core::ByteSize,
    limit: zhold_core::ByteSize,
    format: OutputFormat,
) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        #[derive(Serialize)]
        struct Limit<'a> {
            event: &'static str,
            arena_id: &'a ArenaId,
            observed: zhold_core::ByteSize,
            limit: zhold_core::ByteSize,
            action: &'static str,
        }
        return json::write_stderr(&Limit {
            event: "arena_size_limit_exceeded",
            arena_id: arena,
            observed,
            limit,
            action: "warning",
        });
    }
    let stderr = io::stderr();
    let mut output = stderr.lock();
    writeln!(
        output,
        "zhold  arena {} exceeds warning threshold: {} > {}",
        short_id(arena),
        observed,
        limit
    )
    .map_err(output_error)
}

pub(crate) fn cargo_finalization_failed(
    report: &CargoReport,
    error: &str,
    format: OutputFormat,
) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        #[derive(Serialize)]
        struct Failure<'a> {
            event: &'static str,
            arena_id: &'a ArenaId,
            build_dir: &'a Path,
            outcome: BuildOutcome,
            exit_code: i32,
            peak_size: zhold_core::ByteSize,
            size_limit_exceeded: bool,
            error: &'a str,
        }
        return json::write_stderr(&Failure {
            event: "cargo_finalization_failed",
            arena_id: &report.arena_id,
            build_dir: &report.build_dir,
            outcome: report.outcome,
            exit_code: report.exit_code,
            peak_size: report.peak_size,
            size_limit_exceeded: report.size_limit_exceeded,
            error,
        });
    }
    let stderr = io::stderr();
    let mut output = stderr.lock();
    writeln!(
        output,
        "zhold  Cargo exited {}, but arena finalization failed: {error}",
        report.exit_code
    )
    .map_err(output_error)
}

fn short_id(arena: &ArenaId) -> &str {
    arena.as_str().get(..10).unwrap_or(arena.as_str())
}

const fn outcome_name(outcome: BuildOutcome) -> &'static str {
    match outcome {
        BuildOutcome::Succeeded => "succeeded",
        BuildOutcome::Failed(_) => "failed",
        BuildOutcome::Terminated => "terminated",
    }
}
