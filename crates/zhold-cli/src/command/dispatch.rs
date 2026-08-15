use zhold_store::Store;

use crate::{
    CliError,
    app::{Cli, Command, ExitStatus},
};

pub(crate) fn execute(cli: Cli) -> Result<ExitStatus, CliError> {
    let command = cli.command.unwrap_or(Command::Status);
    let root = match cli.store {
        Some(path) => path,
        None => Store::default_root()?,
    };
    if cli.budget.is_none() && !root.exists() {
        match &command {
            Command::Cargo { .. } => return Err(CliError::MissingCargoBudget),
            Command::Gc {
                size: None,
                trash_only: false,
                ..
            } => return Err(CliError::MissingBudget),
            _ => {}
        }
    }
    let store = Store::open(root)?;
    if let Command::Setup {
        budget,
        min_free,
        build_reserve,
    } = &command
    {
        return super::setup::execute(&store, *budget, *min_free, *build_reserve, cli.format);
    }
    let config = store.config()?;
    let budget = cli.budget.or(config.arena_budget);
    let gc_budget = match &command {
        Command::Gc {
            size, trash_only, ..
        } if !trash_only => Some(size.or(budget).ok_or(CliError::MissingBudget)?),
        _ => None,
    };
    match command {
        Command::Setup { .. } => Ok(ExitStatus::SUCCESS),
        Command::Cargo { arguments } => super::cargo::execute(
            &store,
            arguments,
            super::CargoLimits {
                budget: budget.ok_or(CliError::MissingCargoBudget)?,
                min_free: cli.min_free.or(config.min_filesystem_free),
                build_reserve: cli.build_reserve.or(config.minimum_build_reservation),
            },
            cli.format,
        ),
        Command::Scan { paths } => super::scan::execute(&store, paths, cli.format),
        Command::Status => super::status::execute(&store, cli.format),
        Command::Gc {
            size: _,
            low_watermark,
            dry_run,
            trash_only,
        } => super::collect::execute(
            &store,
            super::collect::GcOptions {
                budget: gc_budget,
                low_watermark,
                dry_run,
                trash_only,
            },
            cli.format,
        ),
        Command::Pin { arena, duration } => super::pin::execute(
            &store,
            &arena,
            true,
            duration.map(crate::app::PinDuration::as_seconds),
            cli.format,
        ),
        Command::Unpin { arena } => super::pin::execute(&store, &arena, false, None, cli.format),
        Command::Recover { arena, .. } => super::recover::execute(&store, &arena, cli.format),
        Command::Doctor => super::doctor::execute(&store, cli.format),
        Command::Explain { arena } => super::explain::execute(&store, &arena, cli.format),
        #[cfg(feature = "experimental")]
        Command::History {
            kind,
            arena,
            worktree,
            since,
            limit,
            action,
        } => super::history::execute(
            &store,
            super::history::HistoryOptions {
                kind,
                arena,
                worktree,
                since_seconds: since.map(crate::app::PinDuration::as_seconds),
                limit,
                action,
            },
            cli.format,
        ),
        #[cfg(feature = "experimental")]
        Command::Hook { action } => super::hook::execute(&store, action, cli.format),
        #[cfg(feature = "experimental")]
        Command::Quota { action } => super::quota::execute(&store, &action, budget, cli.format),
    }
}
