use std::io::{self, Write};

use zhold_store::QuotaStatus;
#[cfg(feature = "experimental")]
use zhold_store::{QuotaAdoption, QuotaPlan};

use super::output::output_error;
use crate::{CliError, app::OutputFormat, render::json};

#[cfg(feature = "experimental")]
pub(crate) fn status(status: &QuotaStatus, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        return json::write(status);
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "provider   {}", status.observation.provider).map_err(output_error)?;
    writeln!(output, "health     {:?}", status.observation.health).map_err(output_error)?;
    writeln!(output, "scope      {}", status.observation.scope.display()).map_err(output_error)?;
    writeln!(
        output,
        "usage      {}",
        status
            .observation
            .usage
            .map_or_else(|| "unknown".to_owned(), |value| value.to_string())
    )
    .map_err(output_error)?;
    writeln!(
        output,
        "hard limit {}",
        status
            .observation
            .limit
            .map_or_else(|| "unconfigured".to_owned(), |value| value.to_string())
    )
    .map_err(output_error)?;
    writeln!(output, "adopted    {}", status.expectation.is_some()).map_err(output_error)?;
    writeln!(output, "detail     {}", status.observation.detail).map_err(output_error)
}

#[cfg(feature = "experimental")]
pub(crate) fn plan(plan: &QuotaPlan, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        return json::write(plan);
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "provider   {}", plan.observation.provider).map_err(output_error)?;
    writeln!(output, "hard limit {}", plan.hard_limit).map_err(output_error)?;
    for requirement in &plan.requirements {
        writeln!(output, "requires   {requirement}").map_err(output_error)?;
    }
    for action in &plan.actions {
        writeln!(
            output,
            "step {}     {}{}",
            action.order,
            action.description,
            if action.privilege_required {
                " [administrator]"
            } else {
                ""
            }
        )
        .map_err(output_error)?;
        if let Some(program) = &action.program {
            writeln!(output, "command    {} {:?}", program, action.arguments)
                .map_err(output_error)?;
        }
    }
    Ok(())
}

#[cfg(feature = "experimental")]
pub(crate) fn adoption(result: &QuotaAdoption, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        json::write(result)?;
    } else {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        writeln!(output, "{}", result.message).map_err(output_error)?;
        writeln!(output, "health {:?}", result.status.observation.health).map_err(output_error)?;
    }
    super::history::warnings(&result.history.warnings, format)
}

pub(crate) fn post_build(status: &QuotaStatus, format: OutputFormat) -> Result<(), CliError> {
    let at_limit = status.remaining == Some(zhold_core::ByteSize::ZERO);
    let drifted = status.expectation.is_some() && !status.healthy;
    if !at_limit && !drifted {
        return Ok(());
    }
    let event = if drifted {
        "quota_drifted"
    } else {
        "quota_at_limit"
    };
    if matches!(format, OutputFormat::Json) {
        #[derive(serde::Serialize)]
        struct Warning<'a> {
            event: &'static str,
            provider: zhold_core::QuotaProvider,
            detail: &'a str,
        }
        return json::write_stderr(&Warning {
            event,
            provider: status.observation.provider,
            detail: &status.observation.detail,
        });
    }
    let stderr = io::stderr();
    let mut output = stderr.lock();
    writeln!(output, "zhold  {event}: {}", status.observation.detail).map_err(output_error)
}
