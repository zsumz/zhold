use std::time::{SystemTime, UNIX_EPOCH};

use zhold_core::{HistoryKind, HistoryPolicy, WorktreeId};
use zhold_store::{HistoryPruneRequest, HistoryQuery, Store};

use crate::{
    CliError,
    app::{ExitStatus, HistoryCommand, OutputFormat},
    render,
};

#[derive(Debug)]
pub(super) struct HistoryOptions {
    pub(super) kind: Option<HistoryKind>,
    pub(super) arena: Option<String>,
    pub(super) worktree: Option<WorktreeId>,
    pub(super) since_seconds: Option<u64>,
    pub(super) limit: usize,
    pub(super) action: Option<HistoryCommand>,
}

pub(super) fn execute(
    store: &Store,
    options: HistoryOptions,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    match options.action {
        Some(HistoryCommand::Prune {
            keep,
            max_bytes,
            older_than,
            dry_run,
        }) => {
            let report = store.prune_history(HistoryPruneRequest {
                keep,
                max_bytes,
                older_than: cutoff(older_than.map(crate::app::PinDuration::as_seconds))?,
                dry_run,
            })?;
            let attention = !report.findings.is_empty();
            render::history_pruned(&report, format)?;
            Ok(if attention {
                ExitStatus::child(2)
            } else {
                ExitStatus::SUCCESS
            })
        }
        Some(HistoryCommand::Policy {
            enabled,
            max_receipts,
            max_bytes,
        }) => {
            let current = store.history_policy()?;
            let requested = HistoryPolicy {
                enabled: enabled.unwrap_or(current.enabled),
                max_receipts: max_receipts.unwrap_or(current.max_receipts),
                max_bytes: max_bytes.unwrap_or(current.max_bytes),
            };
            if requested == current {
                render::history_policy(&current, None, format)?;
            } else {
                let report = store.set_history_policy(requested)?;
                render::history_policy(&requested, Some(&report), format)?;
            }
            Ok(ExitStatus::SUCCESS)
        }
        None => {
            let report = store.history(&HistoryQuery {
                kind: options.kind,
                arena_prefix: options.arena,
                worktree_id: options.worktree,
                since: cutoff(options.since_seconds)?,
                limit: options.limit,
            })?;
            render::history(&report, format)?;
            Ok(ExitStatus::SUCCESS)
        }
    }
}

fn cutoff(seconds: Option<u64>) -> Result<Option<u64>, CliError> {
    let Some(seconds) = seconds else {
        return Ok(None);
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CliError::InvalidClock)?
        .as_millis();
    let milliseconds = u64::try_from(now).unwrap_or(u64::MAX);
    Ok(Some(
        milliseconds.saturating_sub(seconds.saturating_mul(1_000)),
    ))
}
