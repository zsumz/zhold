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
    assert_eq!(String::from_utf8(version.stdout)?, "zhold 0.1.0-alpha.1\n");
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
fn managed_cargo_without_any_budget_requires_setup() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("store");

    let output = command(&store, temporary.path(), &["cargo", "check"])
        .env_remove("ZHOLD_BUDGET")
        .output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr)?.contains("zhold setup 200GiB"));
    assert!(Store::open(&store)?.inventory()?.arenas.is_empty());
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
#[cfg(not(feature = "experimental"))]
fn default_help_excludes_experimental_control_plane() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = temporary.path().join("store");

    let output = zhold(&store, temporary.path(), &["--help"])?;
    let help = String::from_utf8(output.stdout)?;

    assert!(output.status.success());
    assert!(!help.contains("  history"));
    assert!(!help.contains("  hook"));
    assert!(!help.contains("  quota"));
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
