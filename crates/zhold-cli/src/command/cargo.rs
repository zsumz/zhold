use std::{
    env,
    path::PathBuf,
    process::{Command as ProcessCommand, ExitStatus as ProcessExitStatus},
    thread,
    time::Duration,
};

use serde::Serialize;
use zhold_core::{ArenaId, BuildOutcome, ByteSize, CollectionPolicy};
use zhold_store::{CargoInvocation, Store};

use crate::{
    CliError,
    app::{ExitStatus, OutputFormat},
    render,
};

mod finalize;
mod supervisor;

const SENTINEL_ENV: &str = "ZHOLD_INTERNAL_CARGO_SENTINEL";
const DEFAULT_BUILD_RESERVATION: ByteSize = ByteSize::from_bytes(1024 * 1024 * 1024);

#[derive(Debug, Serialize)]
pub(crate) struct CargoReport {
    pub(crate) arena_id: ArenaId,
    pub(crate) build_dir: PathBuf,
    pub(crate) outcome: BuildOutcome,
    pub(crate) exit_code: i32,
    pub(crate) reservation: ByteSize,
    pub(crate) high_water_observation: ByteSize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CargoLimits {
    pub(crate) budget: ByteSize,
    pub(crate) min_free: Option<ByteSize>,
    pub(crate) build_reserve: Option<ByteSize>,
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
    command
        .arg("--budget")
        .arg(limits.budget.as_u64().to_string());
    append_limit(&mut command, "--min-free", limits.min_free);
    append_limit(&mut command, "--build-reserve", limits.build_reserve);
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
    let context = store.resolve_context(&invocation)?;
    let minimum_reservation = limits.build_reserve.unwrap_or(DEFAULT_BUILD_RESERVATION);
    let reservation = store.recommended_reservation(&invocation, minimum_reservation)?;
    let budget = limits.budget;
    let (mut lease, collection) = store.lease_reserved_and_collect(
        &context,
        &invocation,
        reservation,
        CollectionPolicy::new(budget),
    )?;
    render::preflight(&collection, format)?;
    if !collection.budget_met {
        let after = collection.after;
        let reserved = collection.reserved;
        finish_after_error(lease, format)?;
        return Err(CliError::BudgetUnmet {
            after,
            reserved,
            budget,
        });
    }
    if let Some(minimum) = limits.min_free {
        let available = store.available_space()?;
        if available < minimum {
            finish_after_error(lease, format)?;
            return Err(CliError::InsufficientFreeSpace { available, minimum });
        }
    }
    let managed_arguments = invocation.managed_arguments(lease.build_dir())?;
    render::cargo_start(lease.arena_id(), lease.build_dir(), reservation, format)?;
    let mut command = ProcessCommand::new(invocation.program());
    command
        .args(managed_arguments)
        .current_dir(invocation.working_directory())
        .env("CARGO_BUILD_BUILD_DIR", lease.build_dir())
        .env_remove(SENTINEL_ENV);
    let child = supervisor::CargoSupervisor::spawn(&mut command, || lease.mark_spawned());
    let mut child = match child {
        Ok(value) => value,
        Err(source) => {
            finish_after_error(lease, format)?;
            return Err(CliError::Spawn {
                directory: working_directory,
                source: Box::new(source),
            });
        }
    };
    let initial_size = measured_or_zero(&lease);
    let status = match wait_for_cargo(&mut child) {
        Ok(value) => value,
        Err(source) => {
            finish_after_error(lease, format)?;
            return Err(CliError::Wait {
                directory: working_directory,
                source: Box::new(source),
            });
        }
    };
    let was_interrupted = child.was_interrupted();
    let high_water_observation = std::cmp::max(initial_size, measured_or_zero(&lease));
    let exit_code = status.code().unwrap_or(1);
    let outcome = if was_interrupted {
        BuildOutcome::Terminated
    } else if status.success() {
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
        high_water_observation,
    };
    finalize::execute(store, lease, &report, limits, format)
}

fn wait_for_cargo(
    child: &mut supervisor::CargoSupervisor,
) -> Result<ProcessExitStatus, std::io::Error> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => {
                let _termination = child.terminate_and_wait();
                return Err(error);
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn measured_or_zero(lease: &zhold_store::ArenaLease) -> ByteSize {
    lease.measure().unwrap_or(ByteSize::ZERO)
}

fn append_limit(command: &mut ProcessCommand, name: &str, value: Option<ByteSize>) {
    if let Some(value) = value {
        command.arg(name).arg(value.as_u64().to_string());
    }
}

fn finish_after_error(
    lease: zhold_store::ArenaLease,
    format: OutputFormat,
) -> Result<(), CliError> {
    let finalization = lease.finish_aborted()?;
    let _ignored = render::history_finalization(&finalization, format);
    Ok(())
}

const fn format_name(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Human => "human",
        OutputFormat::Json => "json",
    }
}
