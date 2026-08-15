use std::{
    collections::BTreeSet,
    env,
    path::{Path, PathBuf},
};

pub(super) fn files(directory: &Path) -> Vec<PathBuf> {
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

    let mut seen = BTreeSet::new();
    paths.retain(|path| {
        let identity = path.canonicalize().unwrap_or_else(|_| path.clone());
        seen.insert(identity)
    });
    paths
}

pub(super) fn value_base(path: &Path) -> PathBuf {
    path.parent()
        .and_then(Path::parent)
        .or_else(|| path.parent())
        .unwrap_or(path)
        .to_path_buf()
}

fn cargo_home() -> Option<PathBuf> {
    env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .or_else(|| env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".cargo")))
}

fn config_at(directory: &Path) -> Option<PathBuf> {
    let extensionless = directory.join("config");
    if extensionless.is_file() {
        Some(extensionless)
    } else {
        let toml = directory.join("config.toml");
        toml.is_file().then_some(toml)
    }
}
