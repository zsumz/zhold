use std::{
    env,
    path::{Path, PathBuf},
};

use crate::{CargoInvocation, StoreError, context::process};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CargoContext {
    pub(super) workspace_root: PathBuf,
    pub(super) cargo_version: String,
    pub(super) toolchain_description: String,
}

pub(super) fn resolve(invocation: &CargoInvocation) -> Result<CargoContext, StoreError> {
    let prefix = invocation.discovery_arguments();
    let version = cargo_version(invocation, &prefix)?;
    ensure_supported(&version)?;
    let workspace_root = workspace_root(invocation, &prefix)?;
    let cargo_verbose = cargo_verbose(invocation, &prefix)?;
    let (rustc_program, rustc_verbose) = rustc_verbose(invocation)?;
    Ok(CargoContext {
        workspace_root,
        cargo_version: version,
        toolchain_description: format!(
            "{cargo_verbose}\n--- rustc: {rustc_program} ---\n{rustc_verbose}"
        ),
    })
}

fn cargo_version(invocation: &CargoInvocation, prefix: &[String]) -> Result<String, StoreError> {
    let mut arguments = prefix.to_vec();
    arguments.push("--version".to_owned());
    let output = process::required_output(
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
        invocation.program(),
        &arguments,
        invocation.working_directory(),
        None,
    )
}

fn rustc_verbose(invocation: &CargoInvocation) -> Result<(String, String), StoreError> {
    let arguments = vec!["-vV".to_owned()];
    let environment = invocation
        .toolchain_override()
        .map(|toolchain| ("RUSTUP_TOOLCHAIN", toolchain));
    let program = match env::var_os("RUSTC").or_else(|| env::var_os("CARGO_BUILD_RUSTC")) {
        Some(value) => value
            .into_string()
            .map_err(|value| StoreError::NonUnicode {
                kind: "Rust compiler path",
                path: PathBuf::from(value),
            })?,
        None => "rustc".to_owned(),
    };
    let verbose = process::required_output(
        &program,
        &arguments,
        invocation.working_directory(),
        environment,
    )?;
    Ok((program, verbose))
}

fn workspace_root(invocation: &CargoInvocation, prefix: &[String]) -> Result<PathBuf, StoreError> {
    let mut arguments = prefix.to_vec();
    arguments.extend([
        "locate-project".to_owned(),
        "--workspace".to_owned(),
        "--message-format".to_owned(),
        "plain".to_owned(),
    ]);
    let manifest = process::required_output(
        invocation.program(),
        &arguments,
        invocation.working_directory(),
        None,
    )?;
    let manifest = PathBuf::from(manifest);
    let manifest = super::git::canonical_path(&manifest, "Cargo workspace manifest")?;
    manifest
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| StoreError::InvalidOwnership {
            path: manifest,
            reason: "workspace manifest has no parent directory".to_owned(),
        })
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
