use zhold_core::{ByteSize, QuotaProvider};
use zhold_store::Store;

use crate::{
    CliError,
    app::{ExitStatus, OutputFormat, QuotaCommand},
    render,
};

pub(super) fn execute(
    store: &Store,
    action: &QuotaCommand,
    budget: Option<ByteSize>,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    match action {
        QuotaCommand::Status => {
            let status = store.quota_status(QuotaProvider::Auto)?;
            render::quota_status(&status, format)?;
            Ok(ExitStatus::SUCCESS)
        }
        QuotaCommand::Plan {
            hard_limit,
            provider,
        } => {
            require_positive(*hard_limit)?;
            let plan = store.quota_plan(*hard_limit, *provider);
            render::quota_plan(&plan, format)?;
            Ok(ExitStatus::SUCCESS)
        }
        QuotaCommand::Adopt {
            hard_limit,
            provider,
        } => {
            require_positive(*hard_limit)?;
            let result = store.quota_adopt(*hard_limit, *provider, budget)?;
            let attention = result.attention_required;
            render::quota_adoption(&result, format)?;
            Ok(if attention {
                ExitStatus::child(2)
            } else {
                ExitStatus::SUCCESS
            })
        }
        QuotaCommand::Unadopt => {
            let result = store.quota_unadopt()?;
            render::quota_adoption(&result, format)?;
            Ok(ExitStatus::SUCCESS)
        }
    }
}

fn require_positive(hard_limit: ByteSize) -> Result<(), CliError> {
    if hard_limit == ByteSize::ZERO {
        Err(CliError::InvalidQuotaLimit)
    } else {
        Ok(())
    }
}
