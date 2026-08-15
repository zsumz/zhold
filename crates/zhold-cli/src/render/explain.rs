use std::io::{self, Write};

use super::output::output_error;
use crate::{CliError, app::OutputFormat, command::ArenaExplanation, render::json};

pub(crate) fn explain(report: &ArenaExplanation, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        return json::write(report);
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let record = &report.entry.record;
    writeln!(output, "arena       {}", record.id).map_err(output_error)?;
    writeln!(output, "state       {}", state_name(report.state)).map_err(output_error)?;
    writeln!(output, "size        {}", record.size).map_err(output_error)?;
    writeln!(output, "reservation {}", report.entry.reservation).map_err(output_error)?;
    writeln!(output, "high water  {}", report.entry.last_observed_size).map_err(output_error)?;
    writeln!(output, "repository  {}", record.repository_id).map_err(output_error)?;
    writeln!(output, "worktree    {}", record.worktree_id).map_err(output_error)?;
    writeln!(output, "workspace   {}", record.workspace_id).map_err(output_error)?;
    writeln!(output, "toolchain   {}", record.toolchain_id).map_err(output_error)?;
    writeln!(output, "root        {}", record.worktree_root.display()).map_err(output_error)?;
    writeln!(output, "build       {}", record.build_dir.display()).map_err(output_error)?;
    writeln!(output, "policy      {}", report.explanation).map_err(output_error)
}

const fn state_name(state: zhold_core::ArenaState) -> &'static str {
    match state {
        zhold_core::ArenaState::Active => "active",
        zhold_core::ArenaState::Suspect => "suspect",
        zhold_core::ArenaState::Pinned => "pinned",
        zhold_core::ArenaState::Orphaned => "orphaned",
        zhold_core::ArenaState::Idle => "idle",
    }
}
