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

    pub(crate) fn toolchain_arguments(&self) -> Vec<String> {
        self.arguments
            .first()
            .filter(|argument| argument.starts_with('+'))
            .cloned()
            .into_iter()
            .collect()
    }

    pub(crate) fn metadata_arguments(&self) -> Result<Vec<String>, StoreError> {
        let boundary = self
            .arguments
            .iter()
            .position(|argument| argument == "--")
            .unwrap_or(self.arguments.len());
        let mut prefix = self.toolchain_arguments();
        let mut manifest = None;
        let mut modes = Vec::new();
        let mut index = usize::from(!prefix.is_empty());
        while index < boundary {
            let argument = &self.arguments[index];
            match argument.as_str() {
                "-C" | "-Z" | "--config" => {
                    let value = required_value(&self.arguments, index, argument)?;
                    prefix.extend([argument.clone(), value.clone()]);
                    index = index.saturating_add(2);
                }
                "--manifest-path" => {
                    let value = required_value(&self.arguments, index, argument)?;
                    manifest = Some(value.clone());
                    index = index.saturating_add(2);
                }
                "--locked" | "--offline" | "--frozen" => {
                    modes.push(argument.clone());
                    index = index.saturating_add(1);
                }
                _ if joined_prelude(argument) => {
                    prefix.push(argument.clone());
                    index = index.saturating_add(1);
                }
                _ if argument.starts_with("--manifest-path=") => {
                    manifest = argument.split_once('=').map(|(_, value)| value.to_owned());
                    index = index.saturating_add(1);
                }
                _ => index = index.saturating_add(1),
            }
        }
        prefix.extend([
            "metadata".to_owned(),
            "--no-deps".to_owned(),
            "--format-version".to_owned(),
            "1".to_owned(),
        ]);
        if let Some(manifest) = manifest {
            prefix.extend(["--manifest-path".to_owned(), manifest]);
        }
        prefix.extend(modes);
        Ok(prefix)
    }

    pub(crate) fn effective_working_directory(&self) -> Result<PathBuf, StoreError> {
        let mut directory = self.working_directory.clone();
        let mut index = usize::from(self.toolchain_override().is_some());
        while let Some(argument) = self.arguments.get(index) {
            match argument.as_str() {
                "-C" => {
                    let value = required_value(&self.arguments, index, argument)?;
                    directory = resolve_directory(&self.working_directory, value);
                    index = index.saturating_add(2);
                }
                "-Z" | "--color" | "--config" => index = index.saturating_add(2),
                _ if argument.starts_with("-C") && argument.len() > 2 => {
                    directory = resolve_directory(&self.working_directory, &argument[2..]);
                    index = index.saturating_add(1);
                }
                _ if argument.starts_with('-') => index = index.saturating_add(1),
                _ => break,
            }
        }
        directory
            .canonicalize()
            .map_err(|error| StoreError::io("resolve Cargo working directory", directory, error))
    }

    pub(crate) fn configuration_overrides(&self) -> Result<Vec<String>, StoreError> {
        let boundary = self
            .arguments
            .iter()
            .position(|argument| argument == "--")
            .unwrap_or(self.arguments.len());
        let mut values = Vec::new();
        let mut index = 0;
        while index < boundary {
            let argument = &self.arguments[index];
            if argument == "--config" {
                values.push(required_value(&self.arguments, index, argument)?.clone());
                index = index.saturating_add(2);
            } else {
                if let Some(("--config", value)) = argument.split_once('=') {
                    values.push(value.to_owned());
                }
                index = index.saturating_add(1);
            }
        }
        Ok(values)
    }

    /// Returns Cargo arguments with zhold's owned build directory at final precedence.
    pub fn managed_arguments(&self, build_dir: &Path) -> Result<Vec<String>, StoreError> {
        let build_dir = build_dir.to_str().ok_or_else(|| StoreError::NonUnicode {
            kind: "managed Cargo build directory",
            path: build_dir.to_path_buf(),
        })?;
        let quoted = serde_json::to_string(build_dir)
            .map_err(|error| StoreError::InvalidCargoInvocation(error.to_string()))?;
        let mut arguments = self.arguments.clone();
        let insertion = arguments
            .iter()
            .position(|argument| argument == "--")
            .unwrap_or(arguments.len());
        arguments.splice(
            insertion..insertion,
            ["--config".to_owned(), format!("build.build-dir={quoted}")],
        );
        Ok(arguments)
    }

    pub(crate) fn descriptor(&self) -> CommandDescriptor {
        CommandDescriptor::from_arguments(&self.arguments)
    }
}

fn required_value<'a>(
    arguments: &'a [String],
    index: usize,
    option: &str,
) -> Result<&'a String, StoreError> {
    arguments
        .get(index.saturating_add(1))
        .filter(|value| value.as_str() != "--")
        .ok_or_else(|| StoreError::InvalidCargoInvocation(format!("{option} requires a value")))
}

fn joined_prelude(argument: &str) -> bool {
    (argument.starts_with("-C") && argument.len() > 2)
        || (argument.starts_with("-Z") && argument.len() > 2)
        || argument.starts_with("--config=")
}

fn resolve_directory(base: &Path, value: &str) -> PathBuf {
    let value = Path::new(value);
    if value.is_absolute() {
        value.to_path_buf()
    } else {
        base.join(value)
    }
}
