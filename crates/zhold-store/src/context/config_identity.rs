use std::path::{Path, PathBuf};

use crate::{CargoInvocation, StoreError};

#[derive(Debug)]
pub(super) struct CargoConfiguration {
    pub(super) fingerprint: String,
    pub(super) rustc_program: String,
}

pub(super) fn resolve(
    invocation: &CargoInvocation,
    fingerprint_key: &[u8; 32],
) -> Result<CargoConfiguration, StoreError> {
    let loaded = super::config_loader::load(invocation, fingerprint_key)?;
    let program = loaded
        .rustc
        .unwrap_or_else(|| super::config_loader::SourcedProgram {
            value: "rustc".to_owned(),
            base: invocation.working_directory().to_path_buf(),
        });
    Ok(CargoConfiguration {
        fingerprint: loaded.fingerprint,
        rustc_program: normalize_program(&program.value, &program.base)?,
    })
}

fn normalize_program(program: &str, base: &Path) -> Result<String, StoreError> {
    let path = Path::new(program);
    if !path.is_absolute() && path.components().count() == 1 {
        return Ok(program.to_owned());
    }
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let canonical = candidate
        .canonicalize()
        .map_err(|error| StoreError::io("resolve configured Rust compiler", candidate, error))?;
    canonical
        .into_os_string()
        .into_string()
        .map_err(|value| StoreError::NonUnicode {
            kind: "configured Rust compiler path",
            path: PathBuf::from(value),
        })
}
