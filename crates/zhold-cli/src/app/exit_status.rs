use std::process::ExitCode;

/// Portable process status returned by the CLI facade.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExitStatus(i32);

impl ExitStatus {
    /// Successful zhold command.
    pub const SUCCESS: Self = Self(0);

    /// Creates a status from a child process exit code.
    pub const fn child(code: i32) -> Self {
        Self(code)
    }

    /// Returns the numeric process status.
    pub const fn code(self) -> i32 {
        self.0
    }

    /// Converts to the standard library's portable exit representation.
    pub fn into_exit_code(self) -> ExitCode {
        match u8::try_from(self.0) {
            Ok(code) => ExitCode::from(code),
            Err(_) => ExitCode::FAILURE,
        }
    }
}
