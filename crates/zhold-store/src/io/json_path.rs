use std::path::{Path, PathBuf};

pub(crate) fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

pub(super) fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension(format!("json.{}.new", uuid::Uuid::new_v4()))
}

pub(crate) fn is_staging_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(stem) = name.strip_suffix(".new") else {
        return false;
    };
    let Some((primary, nonce)) = stem.rsplit_once(".json.") else {
        return false;
    };
    !primary.is_empty() && uuid::Uuid::parse_str(nonce).is_ok()
}

pub(crate) fn backup_primary_path(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let primary = name.strip_suffix(".json.bak")?;
    if primary.is_empty() {
        return None;
    }
    Some(path.with_file_name(format!("{primary}.json")))
}

pub(crate) fn is_publication_artifact(path: &Path) -> bool {
    is_staging_path(path) || backup_primary_path(path).is_some()
}
