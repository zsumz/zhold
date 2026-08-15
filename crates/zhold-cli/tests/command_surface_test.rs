//! Public command-surface tests.

use std::{fs, io, path::Path, process::Command};

use tempfile::tempdir;
use zhold_core::ByteSize;
use zhold_store::Store;

#[test]
fn help_and_version_are_successful_standard_output() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("store");

    let help = zhold(&store, temporary.path(), &["--help"])?;
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout)?.starts_with("Bounded Cargo build storage"));
    assert!(help.stderr.is_empty());

    let version = zhold(&store, temporary.path(), &["--version"])?;
    assert!(version.status.success());
    assert_eq!(String::from_utf8(version.stdout)?, "zhold 0.0.1-rc.1\n");
    assert!(version.stderr.is_empty());
    Ok(())
}

#[test]
fn setup_persists_the_budget_used_by_later_commands() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("store");

    let setup = command(&store, temporary.path(), &["setup", "200GiB"])
        .env_remove("ZHOLD_BUDGET")
        .output()?;
    let gc = command(&store, temporary.path(), &["gc", "--dry-run"])
        .env_remove("ZHOLD_BUDGET")
        .output()?;

    assert!(setup.status.success());
    assert!(String::from_utf8(setup.stdout)?.contains("arena budget"));
    assert!(gc.status.success());
    assert_eq!(
        Store::open(&store)?.config()?.arena_budget,
        Some(ByteSize::from_bytes(200 * 1_024_u64.pow(3)))
    );
    Ok(())
}

#[test]
fn scan_reports_but_never_mutates_a_foreign_target() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("store");
    let root = temporary.path().join("projects");
    let target = root.join("sample/target");
    fs::create_dir_all(&target)?;
    fs::write(target.join(".rustc_info.json"), b"{}")?;
    fs::write(target.join("artifact.rlib"), vec![0_u8; 1_024])?;

    let output = zhold(&store, temporary.path(), &["scan", path_text(&root)?])?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("foreign Cargo targets: 1"));
    assert!(stdout.contains(&target.display().to_string()));
    assert!(target.join("artifact.rlib").is_file());
    Ok(())
}

#[test]
fn doctor_reports_a_new_store_as_healthy() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("store");

    let output = zhold(&store, temporary.path(), &["doctor"])?;

    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)?.starts_with("healthy"));
    Ok(())
}

#[test]
fn garbage_collection_without_any_budget_fails_helpfully() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = tempdir()?;
    let store = temporary.path().join("store");

    let output = command(&store, temporary.path(), &["gc"])
        .env_remove("ZHOLD_BUDGET")
        .output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr)?.contains("zhold gc 200GiB"));
    assert!(!store.exists());
    Ok(())
}

#[test]
fn status_json_is_one_parseable_document() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("store");

    let output = zhold(&store, temporary.path(), &["--format", "json", "status"])?;

    assert!(output.status.success());
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(document["arenas"], serde_json::json!([]));
    assert!(document["store_root"].is_string());
    assert!(document["physical"].is_number());
    assert!(document["available"].is_number());
    assert_eq!(document["reserved"], 0);
    Ok(())
}

#[test]
fn trash_only_gc_needs_no_budget() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("store");

    let output = command(&store, temporary.path(), &["gc", "--trash-only"])
        .env_remove("ZHOLD_BUDGET")
        .output()?;

    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)?.contains("remaining"));
    Ok(())
}

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
    let prepare = zhold(
        &store,
        temporary.path(),
        &["hook", "prepare-remove", "--path", path],
    )?;
    assert!(prepare.status.success());
    let present = zhold(
        &store,
        temporary.path(),
        &["hook", "removed", "--path", path],
    )?;
    assert_eq!(present.status.code(), Some(2));
    let cancel = zhold(
        &store,
        temporary.path(),
        &["hook", "cancel-remove", "--path", path],
    )?;
    assert!(cancel.status.success());

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
    command(store, working_directory, arguments).output()
}

fn command(store: &Path, working_directory: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zhold"));
    command
        .arg("--store")
        .arg(store)
        .args(arguments)
        .current_dir(working_directory);
    command
}

fn path_text(path: &Path) -> Result<&str, io::Error> {
    path.to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "path is not Unicode"))
}
