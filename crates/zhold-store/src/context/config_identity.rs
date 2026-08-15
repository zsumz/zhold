use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};

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

pub(super) fn effective_rustc(invocation: &CargoInvocation) -> Result<String, StoreError> {
    if let Some(program) = env::var_os("RUSTC").or_else(|| env::var_os("CARGO_BUILD_RUSTC")) {
        return unicode_program(program, "Cargo compiler environment value");
    }
    let directory = invocation.effective_working_directory()?;
    let mut program = None;
    for path in discovered_files(&directory) {
        if let Some(value) = compiler_from_file(&path, false)? {
            program = Some(value);
        }
    }
    for value in invocation.configuration_overrides()? {
        let configured = if value.contains('=') {
            compiler_from_toml(&value, "command-line Cargo configuration")?
        } else {
            compiler_from_file(&directory.join(value), true)?
        };
        if configured.is_some() {
            program = configured;
        }
    }
    normalize_program(program.as_deref().unwrap_or("rustc"), &directory)
}

fn discovered_files(directory: &Path) -> Vec<PathBuf> {
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

fn compiler_from_file(path: &Path, required: bool) -> Result<Option<String>, StoreError> {
    let text = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(StoreError::io("read Cargo configuration", path, error)),
    };
    compiler_from_toml(&text, &format!("Cargo configuration `{}`", path.display()))
}

fn compiler_from_toml(text: &str, source: &str) -> Result<Option<String>, StoreError> {
    let value: toml::Value = toml::from_str(text).map_err(|error| {
        StoreError::InvalidCargoInvocation(format!("invalid {source}: {error}"))
    })?;
    value
        .get("build")
        .and_then(|build| build.get("rustc"))
        .map(|rustc| {
            rustc.as_str().map(str::to_owned).ok_or_else(|| {
                StoreError::InvalidCargoInvocation(format!(
                    "build.rustc in {source} must be a string"
                ))
            })
        })
        .transpose()
}

fn normalize_program(program: &str, directory: &Path) -> Result<String, StoreError> {
    let path = Path::new(program);
    if !path.is_absolute() && path.components().count() == 1 {
        return Ok(program.to_owned());
    }
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        directory.join(path)
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

fn unicode_program(value: std::ffi::OsString, kind: &'static str) -> Result<String, StoreError> {
    value.into_string().map_err(|value| StoreError::NonUnicode {
        kind,
        path: PathBuf::from(value),
    })
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
