use zhold_core::CollectionPolicy;
use zhold_store::{ArenaLease, Store};

use super::{CargoLimits, CargoReport};
use crate::{
    CliError,
    app::{ExitStatus, OutputFormat},
    render,
};

pub(super) fn execute(
    store: &Store,
    lease: ArenaLease,
    report: &CargoReport,
    limits: CargoLimits,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    if let Ok(Some(quota)) = store.observe_adopted_quota() {
        let _ignored = render::quota_post_build(&quota, format);
    }
    let finalization = lease.finish_with_observation(report.outcome, report.high_water_observation);
    match finalization {
        Ok(finalization) => {
            render::cargo_finish_with_history(report, &finalization, format)?;
            collect_after_build(store, report, limits.budget, format)
        }
        Err(error) => {
            render::cargo_finalization_failed(report, &error.to_string(), format)?;
            Ok(management_exit(report))
        }
    }
}

fn collect_after_build(
    store: &Store,
    report: &CargoReport,
    budget: zhold_core::ByteSize,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    match store.collect_post_build(CollectionPolicy::new(budget)) {
        Ok(collection) => {
            render::post_build(&collection, format)?;
            if collection.budget_met {
                Ok(ExitStatus::child(report.exit_code))
            } else {
                let error = format!(
                    "safe collection left {} active + {} reserved above the {} budget",
                    collection.after, collection.reserved, budget
                );
                management_failure(report, &error, format)
            }
        }
        Err(error) => management_failure(report, &error.to_string(), format),
    }
}

fn management_failure(
    report: &CargoReport,
    error: &str,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    render::cargo_management_failed(report, "post_build_collection", error, format)?;
    Ok(management_exit(report))
}

fn management_exit(report: &CargoReport) -> ExitStatus {
    if report.exit_code == 0 {
        ExitStatus::MANAGEMENT_FAILURE
    } else {
        ExitStatus::child(report.exit_code)
    }
}
