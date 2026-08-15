//! Opt-in control-plane command tests.

#![cfg(feature = "experimental")]

use std::{fs, io, path::Path, process::Command};

use tempfile::tempdir;

#[test]
fn history_policy_query_and_prune_are_structured() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("store");

    let policy = zhold(
        &store,
        temporary.path(),
        &[
            "--format",
            "json",
            "history",
            "policy",
            "--max-receipts",
            "5",
            "--max-bytes",
            "1MiB",
        ],
    )?;
    assert!(policy.status.success());
    let policy_json: serde_json::Value = serde_json::from_slice(&policy.stdout)?;
    assert_eq!(policy_json["policy"]["max_receipts"], 5);

    let history = zhold(&store, temporary.path(), &["--format", "json", "history"])?;
    assert!(history.status.success());
    let history_json: serde_json::Value = serde_json::from_slice(&history.stdout)?;
    assert_eq!(history_json["receipts"], serde_json::json!([]));

    let prune = zhold(
        &store,
        temporary.path(),
        &["--format", "json", "history", "prune", "--dry-run"],
    )?;
    assert!(prune.status.success());
    let prune_json: serde_json::Value = serde_json::from_slice(&prune.stdout)?;
    assert_eq!(prune_json["removed_count"], 0);
    Ok(())
}

#[test]
fn hook_protocol_is_end_to_end_and_queryable() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("store");
    let worktree = temporary.path().join("worktree");
    fs::create_dir(&worktree)?;
    let initialized = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&worktree)
        .status()?;
    assert!(initialized.success());

    let path = path_text(&worktree)?;
    let ready = zhold(
        &store,
        temporary.path(),
        &[
            "hook",
            "ready",
            "--path",
            path,
            "--manager",
            "worktrunk",
            "--label",
            "feature/history",
        ],
    )?;
    assert!(ready.status.success());
    assert!(
        zhold(
            &store,
            temporary.path(),
            &["hook", "prepare-remove", "--path", path],
        )?
        .status
        .success()
    );
    assert_eq!(
        zhold(
            &store,
            temporary.path(),
            &["hook", "removed", "--path", path],
        )?
        .status
        .code(),
        Some(2)
    );
    assert!(
        zhold(
            &store,
            temporary.path(),
            &["hook", "cancel-remove", "--path", path],
        )?
        .status
        .success()
    );

    let history = zhold(
        &store,
        temporary.path(),
        &["--format", "json", "history", "--kind", "hook"],
    )?;
    let document: serde_json::Value = serde_json::from_slice(&history.stdout)?;
    assert_eq!(document["receipts"].as_array().map(Vec::len), Some(4));
    Ok(())
}

#[test]
fn quota_status_and_plan_are_non_privileged_and_non_mutating()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("store");

    let status = zhold(
        &store,
        temporary.path(),
        &["--format", "json", "quota", "status"],
    )?;
    assert!(status.status.success());
    let status_json: serde_json::Value = serde_json::from_slice(&status.stdout)?;
    assert!(status_json["observation"]["health"].is_string());

    let plan = zhold(
        &store,
        temporary.path(),
        &["--format", "json", "quota", "plan", "20GiB"],
    )?;
    assert!(plan.status.success());
    let plan_json: serde_json::Value = serde_json::from_slice(&plan.stdout)?;
    assert_eq!(plan_json["hard_limit"], 20_u64 * 1_024 * 1_024 * 1_024);
    assert!(!store.join("quota.json").exists());
    Ok(())
}

fn zhold(
    store: &Path,
    working_directory: &Path,
    arguments: &[&str],
) -> Result<std::process::Output, io::Error> {
    Command::new(env!("CARGO_BIN_EXE_zhold"))
        .arg("--store")
        .arg(store)
        .args(arguments)
        .current_dir(working_directory)
        .output()
}

fn path_text(path: &Path) -> Result<&str, io::Error> {
    path.to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "path is not Unicode"))
}
