use std::{collections::BTreeSet, env, fs, path::PathBuf};

use crate::{CargoInvocation, StoreError};

const COMPILER_ENVIRONMENT: &[&str] = &[
    "CARGO_BUILD_RUSTC",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTFLAGS",
    "CARGO_BUILD_TARGET",
    "CARGO_ENCODED_RUSTFLAGS",
    "RUSTC",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTC_WRAPPER",
    "RUSTFLAGS",
];

pub(super) fn fingerprint(invocation: &CargoInvocation) -> Result<String, StoreError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"zhold-cargo-configuration-v1\0");
    let directory = invocation.effective_working_directory()?;
    let mut seen = BTreeSet::new();
    for path in discovered_files(&directory) {
        hash_file(&mut hasher, &path, &mut seen, false)?;
    }
    for value in invocation.configuration_overrides()? {
        if value.contains('=') {
            hash_part(&mut hasher, b"inline", value.as_bytes());
        } else {
            let path = directory.join(value);
            hash_file(&mut hasher, &path, &mut seen, true)?;
        }
    }
    for name in COMPILER_ENVIRONMENT {
        if let Some(value) = env::var_os(name) {
            let value = value
                .into_string()
                .map_err(|value| StoreError::NonUnicode {
                    kind: "Cargo compiler environment value",
                    path: PathBuf::from(value),
                })?;
            hash_part(&mut hasher, name.as_bytes(), value.as_bytes());
        }
    }
    Ok(hasher.finalize().to_hex()[..32].to_owned())
}

fn discovered_files(directory: &std::path::Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = cargo_home()
        && let Some(config) = config_at(&home)
    {
        paths.push(config);
    }
    let mut ancestors = directory.ancestors().collect::<Vec<_>>();
    ancestors.reverse();
    paths.extend(
        ancestors
            .into_iter()
            .filter_map(|ancestor| config_at(&ancestor.join(".cargo"))),
    );
    paths
}

fn cargo_home() -> Option<PathBuf> {
    env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .or_else(|| env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".cargo")))
}

fn config_at(directory: &std::path::Path) -> Option<PathBuf> {
    let extensionless = directory.join("config");
    if extensionless.is_file() {
        Some(extensionless)
    } else {
        let toml = directory.join("config.toml");
        toml.is_file().then_some(toml)
    }
}

fn hash_file(
    hasher: &mut blake3::Hasher,
    path: &std::path::Path,
    seen: &mut BTreeSet<PathBuf>,
    required: bool,
) -> Result<(), StoreError> {
    let canonical = match path.canonicalize() {
        Ok(value) => value,
        Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(StoreError::io("resolve Cargo configuration", path, error)),
    };
    if !seen.insert(canonical.clone()) {
        return Ok(());
    }
    let bytes = fs::read(&canonical)
        .map_err(|error| StoreError::io("read Cargo configuration", &canonical, error))?;
    hash_part(hasher, canonical.as_os_str().as_encoded_bytes(), &bytes);
    Ok(())
}

fn hash_part(hasher: &mut blake3::Hasher, label: &[u8], value: &[u8]) {
    hasher.update(&u64::try_from(label.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(label);
    hasher.update(&u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(value);
}
