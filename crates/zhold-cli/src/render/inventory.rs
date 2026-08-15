use std::io::{self, Write};

use zhold_core::ArenaState;
use zhold_store::Inventory;

use super::output::output_error;
use crate::{CliError, app::OutputFormat, render::json};

pub(crate) fn inventory(inventory: &Inventory, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        return json::write(inventory);
    }

    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "store      {}", inventory.store_root.display()).map_err(output_error)?;
    writeln!(output, "managed    {}", inventory.total).map_err(output_error)?;
    writeln!(output, "protected  {}", inventory.protected).map_err(output_error)?;
    writeln!(output, "reserved   {}", inventory.reserved).map_err(output_error)?;
    writeln!(output, "trash      {}", inventory.trash).map_err(output_error)?;
    writeln!(output, "physical   {}", inventory.physical).map_err(output_error)?;
    writeln!(output, "available  {}", inventory.available).map_err(output_error)?;
    writeln!(
        output,
        "history    {} receipts ({}, {} findings)",
        inventory.history.receipt_count,
        inventory.history.receipt_bytes,
        inventory.history.finding_count
    )
    .map_err(output_error)?;
    writeln!(
        output,
        "worktrees  {} registered ({} draining, {} findings)",
        inventory.worktrees.registration_count,
        inventory.worktrees.draining_count,
        inventory.worktrees.finding_count
    )
    .map_err(output_error)?;
    if let Some(quota) = &inventory.quota {
        writeln!(
            output,
            "quota      {} {:?}{}",
            quota.observation.provider,
            quota.observation.health,
            quota
                .observation
                .limit
                .map_or_else(String::new, |value| format!(" at {value}"))
        )
        .map_err(output_error)?;
    }
    if let Some(finding) = &inventory.quota_finding {
        writeln!(output, "quota      finding: {finding}").map_err(output_error)?;
    }
    writeln!(
        output,
        "reclaimable {}",
        inventory.total.saturating_sub(inventory.protected)
    )
    .map_err(output_error)?;
    writeln!(output, "arenas     {}", inventory.arenas.len()).map_err(output_error)?;
    if inventory.arenas.is_empty() {
        writeln!(output, "\nno managed arenas yet").map_err(output_error)?;
    } else {
        writeln!(
            output,
            "\nSTATE      SIZE       ARENA       BRANCH / WORKSPACE"
        )
        .map_err(output_error)?;
        for entry in &inventory.arenas {
            let state = state_name(entry.record.state());
            let branch = entry.branch.as_deref().unwrap_or("detached");
            let label = entry
                .label
                .as_ref()
                .map(|value| format!(" [{value}]"))
                .unwrap_or_default();
            let arena = entry
                .record
                .id
                .as_str()
                .get(..10)
                .unwrap_or(entry.record.id.as_str());
            writeln!(
                output,
                "{state:<10} {:<10} {arena:<11} {}  {}{}",
                entry.record.size,
                branch,
                entry.workspace_root.display(),
                label
            )
            .map_err(output_error)?;
        }
    }
    for finding in &inventory.findings {
        writeln!(
            output,
            "finding    {}: {}",
            finding.path.display(),
            finding.reason
        )
        .map_err(output_error)?;
    }
    Ok(())
}

fn state_name(state: ArenaState) -> &'static str {
    match state {
        ArenaState::Active => "active",
        ArenaState::Pinned => "pinned",
        ArenaState::Orphaned => "orphaned",
        ArenaState::Idle => "idle",
    }
}
