use std::{
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SourcedProgram {
    pub(super) value: String,
    pub(super) base: PathBuf,
}

#[derive(Debug)]
pub(super) struct LoadedConfiguration {
    pub(super) rustc: Option<SourcedProgram>,
    pub(super) fingerprint: String,
}

pub(super) fn load(
    invocation: &CargoInvocation,
    fingerprint_key: &[u8; 32],
) -> Result<LoadedConfiguration, StoreError> {
    let directory = invocation.effective_working_directory()?;
    let mut loader = Loader::new(fingerprint_key, directory.clone());
    for path in super::config_discovery::files(&directory) {
        loader.merge_file(&path, false)?;
    }
    loader.merge_environment()?;
    for value in invocation.configuration_overrides()? {
        if value.contains('=') {
            loader.merge_inline(&value)?;
        } else {
            loader.merge_file(&directory.join(value), true)?;
        }
    }
    Ok(loader.finish())
}

struct Loader {
    hasher: blake3::Hasher,
    rustc: Option<SourcedProgram>,
    directory: PathBuf,
    stack: Vec<PathBuf>,
}

impl Loader {
    fn new(key: &[u8; 32], directory: PathBuf) -> Self {
        let mut hasher = blake3::Hasher::new_keyed(key);
        hasher.update(b"zhold-cargo-configuration-v2\0");
        Self {
            hasher,
            rustc: None,
            directory,
            stack: Vec::new(),
        }
    }

    fn merge_environment(&mut self) -> Result<(), StoreError> {
        for name in COMPILER_ENVIRONMENT {
            let Some(value) = env::var_os(name) else {
                continue;
            };
            let value = value
                .into_string()
                .map_err(|value| StoreError::NonUnicode {
                    kind: "Cargo compiler environment value",
                    path: PathBuf::from(value),
                })?;
            self.hash_part(name.as_bytes(), value.as_bytes());
            if matches!(*name, "CARGO_BUILD_RUSTC" | "RUSTC") {
                self.rustc = Some(SourcedProgram {
                    value,
                    base: self.directory.clone(),
                });
            }
        }
        Ok(())
    }

    fn merge_inline(&mut self, text: &str) -> Result<(), StoreError> {
        self.hash_part(b"inline", text.as_bytes());
        let value = parse_document(text, "command-line Cargo configuration")?;
        if value.get("include").is_some() {
            return Err(StoreError::InvalidCargoInvocation(
                "Cargo configuration include is supported only from configuration files".to_owned(),
            ));
        }
        self.merge_rustc(
            &value,
            self.directory.clone(),
            "command-line Cargo configuration",
        )
    }

    fn merge_file(&mut self, path: &Path, required: bool) -> Result<(), StoreError> {
        let canonical = match path.canonicalize() {
            Ok(value) => value,
            Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(());
            }
            Err(error) => {
                return Err(StoreError::io("resolve Cargo configuration", path, error));
            }
        };
        if self.stack.contains(&canonical) {
            let mut cycle = self.stack.clone();
            cycle.push(canonical);
            return Err(StoreError::InvalidCargoInvocation(format!(
                "Cargo configuration include cycle: {}",
                cycle
                    .iter()
                    .map(|entry| entry.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ")
            )));
        }
        let bytes = fs::read(&canonical)
            .map_err(|error| StoreError::io("read Cargo configuration", &canonical, error))?;
        let text = std::str::from_utf8(&bytes).map_err(|error| {
            StoreError::InvalidCargoInvocation(format!(
                "Cargo configuration `{}` is not UTF-8: {error}",
                canonical.display()
            ))
        })?;
        self.hash_part(canonical.as_os_str().as_encoded_bytes(), &bytes);
        let source = format!("Cargo configuration `{}`", canonical.display());
        let value = parse_document(text, &source)?;
        self.stack.push(canonical.clone());
        for include in includes(&value, &canonical, &source)? {
            self.merge_file(&include.path, !include.optional)?;
        }
        let _popped = self.stack.pop();
        self.merge_rustc(
            &value,
            super::config_discovery::value_base(&canonical),
            &source,
        )
    }

    fn merge_rustc(
        &mut self,
        value: &toml::Value,
        base: PathBuf,
        source: &str,
    ) -> Result<(), StoreError> {
        let Some(rustc) = value.get("build").and_then(|build| build.get("rustc")) else {
            return Ok(());
        };
        let rustc = rustc.as_str().ok_or_else(|| {
            StoreError::InvalidCargoInvocation(format!("build.rustc in {source} must be a string"))
        })?;
        self.rustc = Some(SourcedProgram {
            value: rustc.to_owned(),
            base,
        });
        Ok(())
    }

    fn hash_part(&mut self, label: &[u8], value: &[u8]) {
        hash_part(&mut self.hasher, label, value);
    }

    fn finish(self) -> LoadedConfiguration {
        LoadedConfiguration {
            rustc: self.rustc,
            fingerprint: self.hasher.finalize().to_hex()[..32].to_owned(),
        }
    }
}

struct Include {
    path: PathBuf,
    optional: bool,
}

fn includes(
    value: &toml::Value,
    source_path: &Path,
    source: &str,
) -> Result<Vec<Include>, StoreError> {
    let Some(include) = value.get("include") else {
        return Ok(Vec::new());
    };
    let values = include.as_array().ok_or_else(|| {
        StoreError::InvalidCargoInvocation(format!("include in {source} must be an array"))
    })?;
    let parent = source_path.parent().ok_or_else(|| {
        StoreError::InvalidCargoInvocation(format!("{source} has no parent directory"))
    })?;
    values
        .iter()
        .map(|value| parse_include(value, parent, source))
        .collect()
}

fn parse_include(value: &toml::Value, parent: &Path, source: &str) -> Result<Include, StoreError> {
    let (path, optional) = if let Some(path) = value.as_str() {
        (path, false)
    } else {
        let table = value.as_table().ok_or_else(|| {
            StoreError::InvalidCargoInvocation(format!(
                "include entries in {source} must be strings or tables"
            ))
        })?;
        let path = table
            .get("path")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| {
                StoreError::InvalidCargoInvocation(format!(
                    "include table in {source} requires a string path"
                ))
            })?;
        let optional = match table.get("optional") {
            Some(value) => value.as_bool().ok_or_else(|| {
                StoreError::InvalidCargoInvocation(format!(
                    "include optional value in {source} must be a boolean"
                ))
            })?,
            None => false,
        };
        (path, optional)
    };
    if !Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
    {
        return Err(StoreError::InvalidCargoInvocation(format!(
            "included Cargo configuration `{path}` in {source} must end with .toml"
        )));
    }
    Ok(Include {
        path: parent.join(path),
        optional,
    })
}

fn parse_document(text: &str, source: &str) -> Result<toml::Value, StoreError> {
    toml::from_str(text)
        .map_err(|error| StoreError::InvalidCargoInvocation(format!("invalid {source}: {error}")))
}

fn hash_part(hasher: &mut blake3::Hasher, label: &[u8], value: &[u8]) {
    hasher.update(&u64::try_from(label.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(label);
    hasher.update(&u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(value);
}
