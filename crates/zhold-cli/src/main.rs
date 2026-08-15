//! `zhold` command-line executable.

use std::process::ExitCode;

fn main() -> ExitCode {
    match zhold::run() {
        Ok(status) => status.into_exit_code(),
        Err(error) => {
            eprintln!("zhold: {error}");
            ExitCode::FAILURE
        }
    }
}
