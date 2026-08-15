use std::path::{Path, PathBuf};

use crate::{StoreError, context::process};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct GitContext {
    pub(super) common_dir: PathBuf,
    pub(super) worktree_root: PathBuf,
    pub(super) branch: Option<String>,
    pub(super) head: Option<String>,
}

pub(super) fn resolve(working_directory: &Path) -> Result<GitContext, StoreError> {
    let worktree = required_git(working_directory, &["rev-parse", "--show-toplevel"])?;
    let worktree_root = canonical_path(Path::new(&worktree), "Git worktree root")?;
    let common = required_git(
        &worktree_root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let common_path = PathBuf::from(common);
    let common_path = if common_path.is_absolute() {
        common_path
    } else {
        worktree_root.join(common_path)
    };
    let common_dir = canonical_path(&common_path, "Git common directory")?;
    let branch = optional_git(
        &worktree_root,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
    )?;
    let head = optional_git(&worktree_root, &["rev-parse", "--verify", "HEAD"])?;

    Ok(GitContext {
        common_dir,
        worktree_root,
        branch,
        head,
    })
}

fn required_git(working_directory: &Path, arguments: &[&str]) -> Result<String, StoreError> {
    let arguments = arguments
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    process::required_output(
        "Git repository query",
        "git",
        &arguments,
        working_directory,
        None,
    )
}

fn optional_git(
    working_directory: &Path,
    arguments: &[&str],
) -> Result<Option<String>, StoreError> {
    let arguments = arguments
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    process::optional_output(
        "Git optional metadata query",
        "git",
        &arguments,
        working_directory,
    )
}

pub(super) fn canonical_path(path: &Path, kind: &'static str) -> Result<PathBuf, StoreError> {
    let canonical = path
        .canonicalize()
        .map_err(|error| StoreError::io("canonicalize path", path, error))?;
    if canonical.to_str().is_none() {
        return Err(StoreError::NonUnicode {
            kind,
            path: canonical,
        });
    }
    Ok(canonical)
}
