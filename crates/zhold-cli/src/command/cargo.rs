use std::{
    env,
    path::PathBuf,
    process::{Child, Command as ProcessCommand, ExitStatus as ProcessExitStatus},
    thread,
    time::Duration,
};

use serde::Serialize;
use zhold_core::{ArenaId, BuildOutcome, ByteSize, CollectionPolicy};
use zhold_store::{CargoInvocation, ContextResolver, Store};

use crate::{
    CliError,
    app::{ExitStatus, OutputFormat},
    render,
};

const SENTINEL_ENV: &str = "ZHOLD_INTERNAL_CARGO_SENTINEL";
const DEFAULT_BUILD_RESERVATION: ByteSize = ByteSize::from_bytes(1024 * 1024 * 1024);

#[derive(Debug, Serialize)]
pub(crate) struct CargoReport {
    pub(crate) arena_id: ArenaId,
    pub(crate) build_dir: PathBuf,
    pub(crate) outcome: BuildOutcome,
    pub(crate) exit_code: i32,
    pub(crate) reservation: ByteSize,
    pub(crate) peak_size: ByteSize,
    pub(crate) size_limit_exceeded: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CargoLimits {
    pub(crate) budget: Option<ByteSize>,
    pub(crate) min_free: Option<ByteSize>,
    pub(crate) build_reserve: Option<ByteSize>,
    pub(crate) max_arena_size: Option<ByteSize>,
}

pub(super) fn execute(
    store: &Store,
    arguments: Vec<String>,
    limits: CargoLimits,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    if env::var_os(SENTINEL_ENV).is_some() {
        execute_managed(store, arguments, limits, format)
    } else {
        launch_sentinel(store, arguments, limits, format)
    }
}

fn launch_sentinel(
    store: &Store,
    arguments: Vec<String>,
    limits: CargoLimits,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    let executable = env::current_exe().map_err(CliError::CurrentExecutable)?;
    let mut command = ProcessCommand::new(executable);
    command
        .arg("--store")
        .arg(store.info().root)
        .arg("--format")
        .arg(format_name(format));
    if let Some(budget) = limits.budget {
        command.arg("--budget").arg(budget.as_u64().to_string());
    }
    append_limit(&mut command, "--min-free", limits.min_free);
    append_limit(&mut command, "--build-reserve", limits.build_reserve);
    append_limit(&mut command, "--max-arena-size", limits.max_arena_size);
    let status = command
        .arg("cargo")
        .args(arguments)
        .env(SENTINEL_ENV, "1")
        .status()
        .map_err(|source| CliError::Sentinel(Box::new(source)))?;
    Ok(ExitStatus::child(status.code().unwrap_or(1)))
}

fn execute_managed(
    store: &Store,
    arguments: Vec<String>,
    limits: CargoLimits,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    if env::var_os("CARGO_BUILD_BUILD_DIR").is_some() {
        return Err(CliError::ConflictingBuildDirectory);
    }
    let working_directory = env::current_dir().map_err(CliError::CurrentDirectory)?;
    let invocation =
        CargoInvocation::new("cargo".to_owned(), arguments, working_directory.clone())?;
    let context = ContextResolver::resolve(&invocation)?;
    let minimum_reservation = limits.build_reserve.unwrap_or(DEFAULT_BUILD_RESERVATION);
    let reservation = store.recommended_reservation(&invocation, minimum_reservation)?;
    let lease = if let Some(budget) = limits.budget {
        let (lease, collection) = store.lease_reserved_and_collect(
            &context,
            &invocation,
            reservation,
            CollectionPolicy::new(budget),
        )?;
        render::preflight(&collection, format)?;
        if !collection.budget_met {
            let after = collection.after;
            let reserved = collection.reserved;
            finish_before_error(lease, format)?;
            return Err(CliError::BudgetUnmet {
                after,
                reserved,
                budget,
            });
        }
        lease
    } else {
        store.lease_reserved(&context, &invocation, reservation)?
    };
    if let Some(minimum) = limits.min_free {
        let available = store.available_space()?;
        if available < minimum {
            finish_before_error(lease, format)?;
            return Err(CliError::InsufficientFreeSpace { available, minimum });
        }
    }
    let managed_arguments = invocation.managed_arguments(lease.build_dir())?;
    render::cargo_start(lease.arena_id(), lease.build_dir(), reservation, format)?;
    let child = ProcessCommand::new(invocation.program())
        .args(managed_arguments)
        .current_dir(invocation.working_directory())
        .env("CARGO_BUILD_BUILD_DIR", lease.build_dir())
        .env_remove(SENTINEL_ENV)
        .spawn();
    let mut child = match child {
        Ok(value) => value,
        Err(source) => {
            finish_before_error(lease, format)?;
            return Err(CliError::Spawn {
                directory: working_directory,
                source: Box::new(source),
            });
        }
    };
    let observed = wait_for_cargo(&mut child, &lease, limits.max_arena_size, format);
    let (status, peak_size, size_limit_exceeded) = match observed {
        Ok(value) => value,
        Err(source) => {
            finish_before_error(lease, format)?;
            return Err(CliError::Wait {
                directory: working_directory,
                source: Box::new(source),
            });
        }
    };
    let exit_code = status.code().unwrap_or(1);
    let outcome = if status.success() {
        BuildOutcome::Succeeded
    } else if let Some(code) = status.code() {
        BuildOutcome::Failed(code)
    } else {
        BuildOutcome::Terminated
    };
    let report = CargoReport {
        arena_id: lease.arena_id().clone(),
        build_dir: lease.build_dir().to_path_buf(),
        outcome,
        exit_code,
        reservation,
        peak_size,
        size_limit_exceeded,
    };
    finalize_cargo(store, lease, &report, limits.max_arena_size, format)
}

fn finalize_cargo(
    store: &Store,
    lease: zhold_store::ArenaLease,
    report: &CargoReport,
    warning_threshold: Option<ByteSize>,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    if let Ok(Some(quota)) = store.observe_adopted_quota() {
        let _ignored = render::quota_post_build(&quota, format);
    }
    let finalization = lease.finish_observed(
        report.outcome,
        report.peak_size,
        warning_threshold,
        report.size_limit_exceeded,
    );
    match finalization {
        Ok(finalization) => {
            render::cargo_finish_with_history(report, &finalization, format)?;
            Ok(ExitStatus::child(report.exit_code))
        }
        Err(error) => {
            render::cargo_finalization_failed(report, &error.to_string(), format)?;
            if report.exit_code == 0 {
                Ok(ExitStatus::MANAGEMENT_FAILURE)
            } else {
                Ok(ExitStatus::child(report.exit_code))
            }
        }
    }
}

fn wait_for_cargo(
    child: &mut Child,
    lease: &zhold_store::ArenaLease,
    limit: Option<ByteSize>,
    format: OutputFormat,
) -> Result<(ProcessExitStatus, ByteSize, bool), std::io::Error> {
    let mut peak = measured_or_zero(lease);
    let mut exceeded = false;
    let Some(limit) = limit else {
        let status = child.wait()?;
        peak = std::cmp::max(peak, measured_or_zero(lease));
        return Ok((status, peak, false));
    };
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let observed = measured_or_zero(lease);
                peak = std::cmp::max(peak, observed);
                exceeded = report_limit_once(lease, observed, limit, exceeded, format);
                return Ok((status, peak, exceeded));
            }
            Ok(None) => {}
            Err(_) => {
                let status = child.wait()?;
                let observed = measured_or_zero(lease);
                peak = std::cmp::max(peak, observed);
                exceeded = report_limit_once(lease, observed, limit, exceeded, format);
                return Ok((status, peak, exceeded));
            }
        }
        let observed = measured_or_zero(lease);
        peak = std::cmp::max(peak, observed);
        exceeded = report_limit_once(lease, observed, limit, exceeded, format);
        thread::sleep(Duration::from_millis(500));
    }
}

fn report_limit_once(
    lease: &zhold_store::ArenaLease,
    observed: ByteSize,
    limit: ByteSize,
    already_exceeded: bool,
    format: OutputFormat,
) -> bool {
    if already_exceeded || observed <= limit {
        return already_exceeded;
    }
    let _ignored = render::cargo_size_limit_exceeded(lease.arena_id(), observed, limit, format);
    true
}

fn measured_or_zero(lease: &zhold_store::ArenaLease) -> ByteSize {
    lease.measure().unwrap_or(ByteSize::ZERO)
}

fn append_limit(command: &mut ProcessCommand, name: &str, value: Option<ByteSize>) {
    if let Some(value) = value {
        command.arg(name).arg(value.as_u64().to_string());
    }
}

fn finish_before_error(
    lease: zhold_store::ArenaLease,
    format: OutputFormat,
) -> Result<(), CliError> {
    let finalization = lease.finish(BuildOutcome::Terminated)?;
    let _ignored = render::history_finalization(&finalization, format);
    Ok(())
}

const fn format_name(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Human => "human",
        OutputFormat::Json => "json",
    }
}
