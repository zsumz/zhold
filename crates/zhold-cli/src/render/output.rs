use std::io;

use crate::CliError;

#[allow(
    clippy::needless_pass_by_value,
    reason = "io::Result::map_err passes its error by value"
)]
pub(super) fn output_error(error: io::Error) -> CliError {
    CliError::Output(error.to_string())
}
