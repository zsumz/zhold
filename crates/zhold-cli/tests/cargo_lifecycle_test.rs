//! End-to-end managed Cargo lifecycle tests.

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

use tempfile::tempdir;
use zhold_core::{ArenaState, BuildOutcome};
use zhold_store::Store;

#[test]
fn cargo_build_separates_intermediates_and_final_output() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store_root = temporary.path().join("store");
    create_project(&project, BuildScript::None)?;

    let output = zhold(&project, &store_root, &["cargo", "build"])?;
    assert!(
        output.status.success(),
        "zhold cargo build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let inventory = Store::open(&store_root)?.inventory()?;
    assert_eq!(inventory.arenas.len(), 1);
    let arena = &inventory.arenas[0].record;
    assert_eq!(arena.state(), ArenaState::Idle);
    assert!(arena.build_dir.is_dir());
    assert!(directory_has_entries(&arena.build_dir)?);
    assert!(final_binary(&project).is_file());
    Ok(())
}

#[test]
fn bare_zhold_shows_status_and_printed_prefix_can_be_pinned()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store_root = temporary.path().join("store");
    create_project(&project, BuildScript::None)?;
    let build = zhold(&project, &store_root, &["cargo", "check"])?;
    assert!(build.status.success());

    let status = zhold(&project, &store_root, &[])?;
    assert!(status.status.success());
    assert!(String::from_utf8(status.stdout)?.contains("reclaimable"));

    let store = Store::open(&store_root)?;
    let id = &store.inventory()?.arenas[0].record.id;
    let prefix = id
        .as_str()
        .get(..10)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "arena identity is too short"))?;
    let pin = zhold(&project, &store_root, &["pin", prefix])?;
    assert!(pin.status.success());
    assert_eq!(
        Store::open(&store_root)?.inventory()?.arenas[0]
            .record
            .state(),
        ArenaState::Pinned
    );
    let expiring = zhold(&project, &store_root, &["pin", prefix, "--for", "1h"])?;
    assert!(expiring.status.success());
    assert!(
        Store::open(&store_root)?.inventory()?.arenas[0]
            .pin_expires_at
            .is_some()
    );
    let explain = zhold(&project, &store_root, &["explain", prefix])?;
    assert!(explain.status.success());
    assert!(String::from_utf8(explain.stdout)?.contains("protected by a user pin"));
    let unpin = zhold(&project, &store_root, &["unpin", prefix])?;
    assert!(unpin.status.success());
    assert_eq!(
        Store::open(&store_root)?.inventory()?.arenas[0]
            .record
            .state(),
        ArenaState::Idle
    );
    Ok(())
}

#[test]
fn child_failure_is_preserved_in_exit_status_and_manifest() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store_root = temporary.path().join("store");
    create_project(&project, BuildScript::Fail)?;

    let output = zhold(&project, &store_root, &["cargo", "check"])?;

    assert_eq!(output.status.code(), Some(101));
    let inventory = Store::open(&store_root)?.inventory()?;
    assert_eq!(
        inventory.arenas[0].record.last_outcome,
        Some(BuildOutcome::Failed(101))
    );
    Ok(())
}

#[test]
fn short_gc_syntax_produces_a_non_mutating_plan() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store_root = temporary.path().join("store");
    create_project(&project, BuildScript::None)?;
    assert!(
        zhold(&project, &store_root, &["cargo", "check"])?
            .status
            .success()
    );

    let output = zhold(&project, &store_root, &["gc", "1B", "--dry-run"])?;

    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)?.contains("would retire"));
    assert_eq!(Store::open(&store_root)?.inventory()?.arenas.len(), 1);
    Ok(())
}

#[test]
fn cargo_clean_leaves_an_inventoryable_and_retirable_arena()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store_root = temporary.path().join("store");
    create_project(&project, BuildScript::None)?;
    assert!(
        zhold(&project, &store_root, &["cargo", "build"])?
            .status
            .success()
    );

    let clean = zhold(&project, &store_root, &["cargo", "clean"])?;
    let inventory = Store::open(&store_root)?.inventory()?;
    let collection = zhold(&project, &store_root, &["gc", "1B"])?;

    assert!(
        clean.status.success(),
        "zhold cargo clean failed: {}",
        String::from_utf8_lossy(&clean.stderr)
    );
    assert_eq!(inventory.uncertain_owned, 0);
    assert_eq!(inventory.arenas.len(), 1);
    assert_eq!(inventory.arenas[0].record.state(), ArenaState::Idle);
    assert!(collection.status.success());
    assert!(Store::open(&store_root)?.inventory()?.arenas.is_empty());
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum BuildScript {
    None,
    Fail,
}

fn create_project(root: &Path, build_script: BuildScript) -> Result<(), io::Error> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"zhold-fixture\"\nversion = \"0.1.0-alpha.1\"\nedition = \"2024\"\n",
    )?;
    fs::write(
        root.join("src/main.rs"),
        "fn main() { println!(\"ok\"); }\n",
    )?;
    match build_script {
        BuildScript::None => {}
        BuildScript::Fail => {
            fs::write(
                root.join("build.rs"),
                "fn main() { std::process::exit(23); }\n",
            )?;
        }
    }
    git(root, &["init"])?;
    git(root, &["config", "user.email", "zhold@example.invalid"])?;
    git(root, &["config", "user.name", "zhold tests"])?;
    git(root, &["add", "."])?;
    git(root, &["commit", "-m", "fixture"])
}

fn zhold(
    project: &Path,
    store: &Path,
    arguments: &[&str],
) -> Result<std::process::Output, io::Error> {
    zhold_command(project, store, arguments).output()
}

fn zhold_command(project: &Path, store: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zhold"));
    command
        .arg("--store")
        .arg(store)
        .args(["--budget", "100GiB"])
        .args(arguments)
        .current_dir(project);
    command
}

fn git(root: &Path, arguments: &[&str]) -> Result<(), io::Error> {
    let output = Command::new("git")
        .args(["-c", "commit.gpgsign=false"])
        .args(arguments)
        .current_dir(root)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn directory_has_entries(path: &Path) -> Result<bool, io::Error> {
    fs::read_dir(path)?
        .next()
        .transpose()
        .map(|entry| entry.is_some())
}

fn final_binary(project: &Path) -> PathBuf {
    let name = if cfg!(windows) {
        "zhold-fixture.exe"
    } else {
        "zhold-fixture"
    };
    project.join("target/debug").join(name)
}
