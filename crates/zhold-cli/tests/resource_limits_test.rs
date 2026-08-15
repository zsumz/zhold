//! Admission and runtime resource-control tests.

use std::{
    fs, io,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use tempfile::tempdir;
use zhold_core::{ArenaState, BuildOutcome, ByteSize};
use zhold_store::Store;

#[test]
fn minimum_free_space_blocks_cargo_before_spawn() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    create_project(&project)?;

    let output = zhold(&project, &store)
        .args(["--min-free", "18446744073709551615", "cargo", "check"])
        .output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr)?.contains("configured minimum"));
    let inventory = Store::open(&store)?.inventory()?;
    assert_eq!(
        inventory.arenas[0].record.last_outcome,
        Some(BuildOutcome::Terminated)
    );
    assert!(!project.join("target").exists());
    Ok(())
}

#[test]
fn build_reservation_participates_in_admission() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    create_project(&project)?;

    let output = zhold(&project, &store)
        .args([
            "--budget",
            "1B",
            "--build-reserve",
            "1GiB",
            "cargo",
            "check",
        ])
        .output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr)?.contains("reserved"));
    let inventory = Store::open(&store)?.inventory()?;
    assert_eq!(inventory.reserved, ByteSize::ZERO);
    assert_eq!(
        inventory.arenas[0].record.last_outcome,
        Some(BuildOutcome::Terminated)
    );
    Ok(())
}

#[test]
fn concurrent_builds_cannot_spend_the_same_soft_budget() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let first_project = temporary.path().join("first-project");
    let second_project = temporary.path().join("second-project");
    let store = temporary.path().join("store");
    let release = temporary.path().join("release-build");
    create_project(&first_project)?;
    create_project(&second_project)?;
    fs::write(first_project.join("build.rs"), waiting_build_script())?;

    let limits = [
        "--budget",
        "100MiB",
        "--build-reserve",
        "60MiB",
        "cargo",
        "check",
    ];
    let mut first = zhold(&first_project, &store)
        .args(limits)
        .env("ZHOLD_TEST_RELEASE", &release)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    wait_for_active_reservation(&store, ByteSize::from_bytes(60 * 1024 * 1024))?;

    let second = zhold(&second_project, &store).args(limits).output()?;
    fs::write(&release, b"continue")?;
    let first_status = first.wait()?;

    assert!(first_status.success());
    assert_eq!(second.status.code(), Some(1));
    assert!(String::from_utf8(second.stderr)?.contains("reserved"));
    assert_eq!(Store::open(&store)?.inventory()?.reserved, ByteSize::ZERO);
    Ok(())
}

#[test]
fn arena_size_threshold_warns_and_records_peak_without_killing_cargo()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    create_project(&project)?;

    let output = zhold(&project, &store)
        .args([
            "--format",
            "json",
            "--max-arena-size",
            "1B",
            "cargo",
            "check",
        ])
        .output()?;

    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr)?;
    let events = stderr
        .lines()
        .filter(|line| line.starts_with("{\"event\":\""))
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()?;
    assert!(
        events
            .iter()
            .any(|event| event["event"] == "arena_size_limit_exceeded")
    );
    let finished = events
        .iter()
        .find(|event| event["event"] == "cargo_finished")
        .ok_or_else(|| io::Error::other("missing cargo_finished event"))?;
    assert_eq!(finished["size_limit_exceeded"], true);
    assert!(
        finished["peak_size"]
            .as_u64()
            .is_some_and(|value| value > 1)
    );
    assert!(Store::open(&store)?.inventory()?.arenas[0].last_peak > ByteSize::from_bytes(1));
    Ok(())
}

fn zhold(project: &Path, store: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zhold"));
    command
        .arg("--store")
        .arg(store)
        .current_dir(project)
        .env_remove("ZHOLD_BUDGET")
        .env_remove("ZHOLD_MIN_FREE")
        .env_remove("ZHOLD_BUILD_RESERVE")
        .env_remove("ZHOLD_MAX_ARENA_SIZE");
    command
}

fn create_project(root: &Path) -> Result<(), io::Error> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"limits-fixture\"\nversion = \"0.0.1-rc.1\"\nedition = \"2024\"\n",
    )?;
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 5 }\n")?;
    git(root, &["init"])?;
    git(root, &["config", "user.email", "zhold@example.invalid"])?;
    git(root, &["config", "user.name", "zhold tests"])?;
    git(root, &["add", "."])?;
    git(root, &["commit", "-m", "fixture"])
}

fn waiting_build_script() -> &'static str {
    "use std::{path::Path, thread, time::Duration};\n\
     fn main() {\n\
         let release = std::env::var_os(\"ZHOLD_TEST_RELEASE\").unwrap();\n\
         while !Path::new(&release).is_file() {\n\
             thread::sleep(Duration::from_millis(10));\n\
         }\n\
     }\n"
}

fn wait_for_active_reservation(
    store: &Path,
    expected: ByteSize,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Ok(inventory) = Store::open(store).and_then(|store| store.inventory())
            && inventory.reserved == expected
            && inventory
                .arenas
                .iter()
                .any(|arena| arena.record.state() == ArenaState::Active)
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "managed build did not publish its active reservation",
    )
    .into())
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
