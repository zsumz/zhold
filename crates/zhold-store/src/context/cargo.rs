use std::path::PathBuf;

use serde::Deserialize;

use crate::{CargoInvocation, StoreError, context::process};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CargoContext {
    pub(super) workspace_root: PathBuf,
    pub(super) cargo_version: String,
    pub(super) toolchain_description: String,
}

#[derive(Debug, Deserialize)]
struct MetadataOutput {
    workspace_root: PathBuf,
}

pub(super) fn resolve(
    invocation: &CargoInvocation,
    fingerprint_key: &[u8; 32],
) -> Result<CargoContext, StoreError> {
    let prefix = invocation.toolchain_arguments();
    let version = cargo_version(invocation, &prefix)?;
    ensure_supported(&version)?;
    let workspace_root = workspace_root(invocation)?;
    let cargo_verbose = cargo_verbose(invocation, &prefix)?;
    let configuration = super::config_identity::resolve(invocation, fingerprint_key)?;
    let (rustc_program, rustc_verbose) = rustc_verbose(invocation, &configuration.rustc_program)?;
    Ok(CargoContext {
        workspace_root,
        cargo_version: version,
        toolchain_description: format!(
            "{cargo_verbose}\n--- rustc: {rustc_program} ---\n{rustc_verbose}\n--- cargo config ---\n{}",
            configuration.fingerprint
        ),
    })
}

fn cargo_version(invocation: &CargoInvocation, prefix: &[String]) -> Result<String, StoreError> {
    let mut arguments = prefix.to_vec();
    arguments.push("--version".to_owned());
    let output = process::required_output(
        "Cargo version query",
        invocation.program(),
        &arguments,
        invocation.working_directory(),
        None,
    )?;
    parse_version(&output).ok_or(StoreError::UnsupportedCargo { found: output })
}

fn cargo_verbose(invocation: &CargoInvocation, prefix: &[String]) -> Result<String, StoreError> {
    let mut arguments = prefix.to_vec();
    arguments.push("-Vv".to_owned());
    process::required_output(
        "Cargo verbose version query",
        invocation.program(),
        &arguments,
        invocation.working_directory(),
        None,
    )
}

fn rustc_verbose(
    invocation: &CargoInvocation,
    program: &str,
) -> Result<(String, String), StoreError> {
    let arguments = vec!["-vV".to_owned()];
    let environment = invocation
        .toolchain_override()
        .map(|toolchain| ("RUSTUP_TOOLCHAIN", toolchain));
    let directory = invocation.effective_working_directory()?;
    let verbose = process::required_output(
        "Rust compiler identity query",
        program,
        &arguments,
        &directory,
        environment,
    )?;
    Ok((program.to_owned(), verbose))
}

fn workspace_root(invocation: &CargoInvocation) -> Result<PathBuf, StoreError> {
    let arguments = invocation.metadata_arguments()?;
    let manifest = process::required_output(
        "Cargo metadata query",
        invocation.program(),
        &arguments,
        invocation.working_directory(),
        None,
    )?;
    let metadata: MetadataOutput = serde_json::from_str(&manifest)
        .map_err(|error| StoreError::InvalidCargoMetadata(error.to_string()))?;
    super::git::canonical_path(&metadata.workspace_root, "Cargo workspace root")
}

pub(super) fn parse_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .nth(1)
        .map(|release| release.trim().to_owned())
}

fn ensure_supported(version: &str) -> Result<(), StoreError> {
    let mut components = version.split('.');
    let major = components
        .next()
        .and_then(|value| value.parse::<u64>().ok());
    let minor = components
        .next()
        .and_then(|value| value.parse::<u64>().ok());
    if matches!(
        (major, minor),
        (Some(major), Some(minor)) if major > 1 || (major == 1 && minor >= 91)
    ) {
        Ok(())
    } else {
        Err(StoreError::UnsupportedCargo {
            found: version.to_owned(),
        })
    }
}
