use std::path::{Path, PathBuf};

use zhold_core::CommandDescriptor;

use crate::StoreError;

/// One Cargo command to run from a working directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CargoInvocation {
    program: String,
    arguments: Vec<String>,
    working_directory: PathBuf,
}

impl CargoInvocation {
    /// Validates and creates a Cargo invocation.
    pub fn new(
        program: String,
        arguments: Vec<String>,
        working_directory: PathBuf,
    ) -> Result<Self, StoreError> {
        let file_name = Path::new(&program)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !file_name.eq_ignore_ascii_case("cargo") && !file_name.eq_ignore_ascii_case("cargo.exe")
        {
            return Err(StoreError::NotCargo(program));
        }
        Ok(Self {
            program,
            arguments,
            working_directory,
        })
    }

    /// Cargo executable as supplied by the caller.
    pub fn program(&self) -> &str {
        &self.program
    }

    /// Arguments after the Cargo executable.
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    /// Directory from which Cargo should run.
    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    /// Optional leading rustup toolchain selector such as `+nightly`.
    pub fn toolchain_override(&self) -> Option<&str> {
        self.arguments
            .first()
            .and_then(|argument| argument.strip_prefix('+'))
            .filter(|value| !value.is_empty())
    }

    pub(crate) fn discovery_arguments(&self) -> Vec<String> {
        self.arguments
            .first()
            .filter(|argument| argument.starts_with('+'))
            .cloned()
            .into_iter()
            .collect()
    }

    pub(crate) fn descriptor(&self) -> CommandDescriptor {
        CommandDescriptor::from_arguments(&self.arguments)
    }
}
