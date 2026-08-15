//! End-to-end managed Cargo lifecycle tests.

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
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
fn lease_sentinel_survives_front_process_termination() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store_root = temporary.path().join("store");
    let release = temporary.path().join("release-build");
    create_project(&project, BuildScript::Wait)?;

    let mut front = zhold_command(&project, &store_root, &["cargo", "check"])
        .env("ZHOLD_TEST_RELEASE", &release)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    wait_for_state(&store_root, ArenaState::Active, Duration::from_secs(20))?;
    front.kill()?;
    let _status = front.wait()?;

    let state_after_kill = only_state(&store_root)?;
    let gc = zhold(&project, &store_root, &["gc", "1B"])?;
    let retained_after_gc = Store::open(&store_root)?.inventory()?.arenas.len();
    fs::write(&release, b"continue")?;
    wait_for_state(&store_root, ArenaState::Idle, Duration::from_secs(20))?;

    assert_eq!(state_after_kill, ArenaState::Active);
    assert_eq!(gc.status.code(), Some(2));
    assert_eq!(retained_after_gc, 1);
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum BuildScript {
    None,
    Fail,
    Wait,
}

fn create_project(root: &Path, build_script: BuildScript) -> Result<(), io::Error> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"zhold-fixture\"\nversion = \"0.0.1-rc.1\"\nedition = \"2024\"\n",
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
        BuildScript::Wait => fs::write(root.join("build.rs"), waiting_build_script())?,
    }
    git(root, &["init"])?;
    git(root, &["config", "user.email", "zhold@example.invalid"])?;
    git(root, &["config", "user.name", "zhold tests"])?;
    git(root, &["add", "."])?;
    git(root, &["commit", "-m", "fixture"])
}

fn waiting_build_script() -> &'static str {
    "use std::{path::Path, thread, time::Duration};\n\
     fn main() {\n\
         let Some(release) = std::env::var_os(\"ZHOLD_TEST_RELEASE\") else {\n\
             std::process::exit(24);\n\
         };\n\
         while !Path::new(&release).is_file() {\n\
             thread::sleep(Duration::from_millis(10));\n\
         }\n\
     }\n"
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
        .args(arguments)
        .current_dir(project);
    command
}

fn wait_for_state(store: &Path, expected: ArenaState, timeout: Duration) -> Result<(), io::Error> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if matches!(only_state(store), Ok(state) if state == expected) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("arena did not reach {expected:?}"),
    ))
}

fn only_state(store: &Path) -> Result<ArenaState, io::Error> {
    let inventory = Store::open(store)
        .and_then(|store| store.inventory())
        .map_err(io::Error::other)?;
    inventory
        .arenas
        .first()
        .map(|entry| entry.record.state())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "managed arena is missing"))
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
