//! Unix process-group cancellation and descendant-lifetime qualification.

#![cfg(unix)]

use std::{
    fs, io,
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use tempfile::tempdir;
use zhold_core::{ArenaState, BuildOutcome};
use zhold_store::Store;

#[test]
fn sigint_reaches_the_complete_cargo_group() -> Result<(), Box<dyn std::error::Error>> {
    assert_interrupted(Signal::SIGINT, Script::Wait, false)
}

#[test]
fn sigterm_reaches_the_complete_cargo_group() -> Result<(), Box<dyn std::error::Error>> {
    assert_interrupted(Signal::SIGTERM, Script::Wait, false)
}

#[test]
fn repeated_interrupt_forces_an_ignoring_descendant_to_exit()
-> Result<(), Box<dyn std::error::Error>> {
    assert_interrupted(Signal::SIGINT, Script::IgnoreSignals, true)
}

#[test]
fn lease_remains_active_until_a_cargo_descendant_exits() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    let spawned = temporary.path().join("descendant-spawned");
    let done = temporary.path().join("descendant-done");
    create_project(&project, Script::OutlivingDescendant)?;

    let mut front = zhold_command(&project, &store)
        .env("ZHOLD_TEST_READY", &spawned)
        .env("ZHOLD_TEST_DONE", &done)
        .spawn()?;
    wait_for_file(&spawned, Duration::from_secs(20))?;
    thread::sleep(Duration::from_millis(750));

    assert!(!done.exists());
    assert!(front.try_wait()?.is_none());
    assert_eq!(only_state(&store)?, ArenaState::Active);

    let status = front.wait()?;
    assert!(status.success());
    assert!(done.is_file());
    assert_eq!(only_state(&store)?, ArenaState::Idle);
    Ok(())
}

fn assert_interrupted(
    signal: Signal,
    script: Script,
    repeat: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    let ready = temporary.path().join("ready");
    create_project(&project, script)?;

    let mut command = zhold_command(&project, &store);
    command.env("ZHOLD_TEST_READY", &ready).process_group(0);
    let mut front = command.spawn()?;
    let group = process_group(&front)?;
    wait_for_file(&ready, Duration::from_secs(20))?;

    killpg(group, signal)?;
    if repeat {
        thread::sleep(Duration::from_millis(100));
        killpg(group, signal)?;
    }
    let _front_status = front.wait()?;
    wait_for_state(&store, ArenaState::Idle, Duration::from_secs(20))?;

    let inventory = Store::open(&store)?.inventory()?;
    let arena = inventory
        .arenas
        .first()
        .ok_or_else(|| io::Error::other("managed arena is missing"))?;
    assert_eq!(arena.record.last_outcome, Some(BuildOutcome::Terminated));
    assert_eq!(inventory.reserved, zhold_core::ByteSize::ZERO);
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum Script {
    Wait,
    IgnoreSignals,
    OutlivingDescendant,
}

fn create_project(root: &Path, script: Script) -> Result<(), io::Error> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"supervision-fixture\"\nversion = \"0.0.1\"\nedition = \"2024\"\n",
    )?;
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 7 }\n")?;
    let build_script = match script {
        Script::Wait => waiting_script(),
        Script::IgnoreSignals => ignoring_script(),
        Script::OutlivingDescendant => descendant_script(),
    };
    fs::write(root.join("build.rs"), build_script)?;
    git(root, &["init"])?;
    git(root, &["config", "user.email", "zhold@example.invalid"])?;
    git(root, &["config", "user.name", "zhold tests"])?;
    git(root, &["add", "."])?;
    git(root, &["commit", "-m", "fixture"])
}

fn waiting_script() -> &'static str {
    "use std::{fs, thread, time::Duration};\n\
     fn main() {\n\
         let Some(ready) = std::env::var_os(\"ZHOLD_TEST_READY\") else { return; };\n\
         if fs::write(ready, b\"ready\").is_err() { return; }\n\
         loop { thread::sleep(Duration::from_secs(1)); }\n\
     }\n"
}

fn ignoring_script() -> &'static str {
    "use std::{fs, process::Command, thread, time::Duration};\n\
     fn main() {\n\
         let Some(ready) = std::env::var_os(\"ZHOLD_TEST_READY\") else { return; };\n\
         let script = \"trap '' INT TERM HUP QUIT; printf ready > \\\"$1\\\"; while :; do sleep 1; done\";\n\
         let child = Command::new(\"sh\").args([\"-c\", script, \"zhold-child\"]).arg(ready).spawn();\n\
         if child.is_err() { return; }\n\
         loop { thread::sleep(Duration::from_secs(1)); }\n\
     }\n"
}

fn descendant_script() -> &'static str {
    "use std::{fs, process::Command};\n\
     fn main() {\n\
         let Some(ready) = std::env::var_os(\"ZHOLD_TEST_READY\") else { return; };\n\
         let Some(done) = std::env::var_os(\"ZHOLD_TEST_DONE\") else { return; };\n\
         let script = \"sleep 2; printf done > \\\"$1\\\"\";\n\
         let child = Command::new(\"sh\").args([\"-c\", script, \"zhold-child\"]).arg(done).spawn();\n\
         if child.is_ok() { let _result = fs::write(ready, b\"spawned\"); }\n\
     }\n"
}

fn zhold_command(project: &Path, store: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zhold"));
    command
        .arg("--store")
        .arg(store)
        .args(["--budget", "100GiB"])
        .args(["cargo", "check"])
        .current_dir(project)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn process_group(child: &Child) -> Result<Pid, io::Error> {
    i32::try_from(child.id())
        .map(Pid::from_raw)
        .map_err(|_| io::Error::other("front process identifier exceeds i32"))
}

fn wait_for_file(path: &Path, timeout: Duration) -> Result<(), io::Error> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.is_file() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("file did not appear: {}", path.display()),
    ))
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
    Store::open(store)
        .and_then(|store| store.inventory())
        .map_err(io::Error::other)?
        .arenas
        .first()
        .map(|arena| arena.record.state())
        .ok_or_else(|| io::Error::other("managed arena is missing"))
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
