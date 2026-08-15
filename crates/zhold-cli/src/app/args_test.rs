use clap::Parser;
use zhold_core::ByteSize;

use super::{Cli, Command, HistoryCommand, HookCommand, QuotaCommand};

#[test]
fn parses_persistent_setup_defaults() -> Result<(), clap::Error> {
    let parsed = Cli::try_parse_from([
        "zhold",
        "setup",
        "200GiB",
        "--min-free",
        "25GiB",
        "--build-reserve",
        "2GiB",
    ])?;
    let Some(Command::Setup {
        budget,
        min_free,
        build_reserve,
    }) = parsed.command
    else {
        return Err(clap::Error::new(clap::error::ErrorKind::InvalidSubcommand));
    };
    assert_eq!(budget, ByteSize::from_bytes(200 * 1_024_u64.pow(3)));
    assert_eq!(min_free, Some(ByteSize::from_bytes(25 * 1_024_u64.pow(3))));
    assert_eq!(
        build_reserve,
        Some(ByteSize::from_bytes(2 * 1_024_u64.pow(3)))
    );
    Ok(())
}

#[test]
fn parses_the_short_gc_command() -> Result<(), clap::Error> {
    let parsed = Cli::try_parse_from(["zhold", "gc", "200gb", "--dry-run"])?;
    let Some(Command::Gc { size, dry_run, .. }) = parsed.command else {
        return Err(clap::Error::new(clap::error::ErrorKind::InvalidSubcommand));
    };

    assert_eq!(size, Some(ByteSize::from_bytes(200_000_000_000)));
    assert!(dry_run);
    Ok(())
}

#[test]
fn parses_cargo_without_a_separator() -> Result<(), clap::Error> {
    let parsed = Cli::try_parse_from(["zhold", "cargo", "test", "--workspace"])?;
    let Some(Command::Cargo { arguments }) = parsed.command else {
        return Err(clap::Error::new(clap::error::ErrorKind::InvalidSubcommand));
    };

    assert_eq!(arguments, vec!["test".to_owned(), "--workspace".to_owned()]);
    Ok(())
}

#[test]
fn accepts_the_separator_for_shell_familiarity() -> Result<(), clap::Error> {
    let parsed = Cli::try_parse_from(["zhold", "cargo", "--", "test", "--workspace"])?;
    let Some(Command::Cargo { arguments }) = parsed.command else {
        return Err(clap::Error::new(clap::error::ErrorKind::InvalidSubcommand));
    };

    assert_eq!(arguments, vec!["test".to_owned(), "--workspace".to_owned()]);
    Ok(())
}

#[test]
fn no_command_selects_status_at_dispatch_time() -> Result<(), clap::Error> {
    let parsed = Cli::try_parse_from(["zhold"])?;

    assert!(parsed.command.is_none());
    Ok(())
}

#[test]
fn configured_budget_remains_global_to_gc() -> Result<(), clap::Error> {
    let parsed = Cli::try_parse_from(["zhold", "--budget", "200GiB", "gc", "--dry-run"])?;
    let Some(Command::Gc { size, .. }) = parsed.command else {
        return Err(clap::Error::new(clap::error::ErrorKind::InvalidSubcommand));
    };

    assert_eq!(parsed.budget, Some(ByteSize::from_bytes(214_748_364_800)));
    assert_eq!(size, None);
    Ok(())
}

#[test]
fn global_budget_is_accepted_after_gc() -> Result<(), clap::Error> {
    let parsed = Cli::try_parse_from(["zhold", "gc", "--budget", "200GiB", "--dry-run"])?;
    let Some(Command::Gc { size, dry_run, .. }) = parsed.command else {
        return Err(clap::Error::new(clap::error::ErrorKind::InvalidSubcommand));
    };

    assert_eq!(parsed.budget, Some(ByteSize::from_bytes(214_748_364_800)));
    assert_eq!(size, None);
    assert!(dry_run);
    Ok(())
}

#[test]
fn parses_build_safety_limits() -> Result<(), clap::Error> {
    let parsed = Cli::try_parse_from([
        "zhold",
        "--min-free",
        "10GiB",
        "--build-reserve",
        "2GiB",
        "--max-arena-size",
        "20GiB",
        "cargo",
        "check",
    ])?;

    assert_eq!(
        parsed.min_free,
        Some(ByteSize::from_bytes(10 * 1_024_u64.pow(3)))
    );
    assert_eq!(
        parsed.build_reserve,
        Some(ByteSize::from_bytes(2 * 1_024_u64.pow(3)))
    );
    assert_eq!(
        parsed.max_arena_size,
        Some(ByteSize::from_bytes(20 * 1_024_u64.pow(3)))
    );
    Ok(())
}

#[test]
fn parses_expiring_pins_and_trash_only_gc() -> Result<(), clap::Error> {
    let pin = Cli::try_parse_from(["zhold", "pin", "abcdef", "--for", "7d"])?;
    let Some(Command::Pin { duration, .. }) = pin.command else {
        return Err(clap::Error::new(clap::error::ErrorKind::InvalidSubcommand));
    };
    assert_eq!(duration.map(super::PinDuration::as_seconds), Some(604_800));

    let gc = Cli::try_parse_from(["zhold", "gc", "--trash-only"])?;
    let Some(Command::Gc { trash_only, .. }) = gc.command else {
        return Err(clap::Error::new(clap::error::ErrorKind::InvalidSubcommand));
    };
    assert!(trash_only);
    Ok(())
}

#[test]
fn parses_history_filters_and_maintenance() -> Result<(), clap::Error> {
    let query = Cli::try_parse_from([
        "zhold", "history", "--kind", "build", "--since", "12h", "--limit", "25",
    ])?;
    let Some(Command::History { kind, limit, .. }) = query.command else {
        return Err(clap::Error::new(clap::error::ErrorKind::InvalidSubcommand));
    };
    assert_eq!(kind, Some(zhold_core::HistoryKind::Build));
    assert_eq!(limit, 25);

    let prune = Cli::try_parse_from([
        "zhold",
        "history",
        "prune",
        "--keep",
        "100",
        "--max-bytes",
        "8MiB",
        "--dry-run",
    ])?;
    let Some(Command::History {
        action: Some(HistoryCommand::Prune { dry_run, .. }),
        ..
    }) = prune.command
    else {
        return Err(clap::Error::new(clap::error::ErrorKind::InvalidSubcommand));
    };
    assert!(dry_run);
    Ok(())
}

#[test]
fn parses_hook_and_quota_protocols() -> Result<(), clap::Error> {
    let hook = Cli::try_parse_from([
        "zhold",
        "hook",
        "ready",
        "--path",
        "/tmp/worktree",
        "--manager",
        "worktrunk",
    ])?;
    assert!(matches!(
        hook.command,
        Some(Command::Hook {
            action: HookCommand::Ready { .. }
        })
    ));

    let quota = Cli::try_parse_from([
        "zhold",
        "quota",
        "adopt",
        "200GiB",
        "--provider",
        "apfs-volume",
    ])?;
    assert!(matches!(
        quota.command,
        Some(Command::Quota {
            action: QuotaCommand::Adopt {
                provider: zhold_core::QuotaProvider::ApfsVolume,
                ..
            }
        })
    ));
    Ok(())
}
