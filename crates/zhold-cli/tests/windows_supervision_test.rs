//! Windows console-control and Job Object cancellation qualification.

#![cfg(windows)]

use std::{
    fs, io,
    os::windows::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use tempfile::tempdir;
use zhold_core::{ArenaState, BuildOutcome, ByteSize};
use zhold_store::Store;

const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

#[test]
fn console_break_terminates_the_complete_cargo_job() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    let ready = temporary.path().join("ready");
    create_project(&project)?;

    let mut front = zhold_command(&project, &store)
        .env("ZHOLD_TEST_READY", &ready)
        .creation_flags(CREATE_NEW_PROCESS_GROUP)
        .spawn()?;
    wait_for_file(&ready, Duration::from_secs(20))?;
    send_console_break(front.id())?;
    let _front_status = front.wait()?;
    wait_for_state(&store, ArenaState::Idle, Duration::from_secs(20))?;

    let inventory = Store::open(&store)?.inventory()?;
    let arena = inventory
        .arenas
        .first()
        .ok_or_else(|| io::Error::other("managed arena is missing"))?;
    assert_eq!(arena.record.last_outcome, Some(BuildOutcome::Terminated));
    assert_eq!(inventory.reserved, ByteSize::ZERO);
    Ok(())
}

fn create_project(root: &Path) -> Result<(), io::Error> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"windows-supervision-fixture\"\nversion = \"0.0.1\"\nedition = \"2024\"\n",
    )?;
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 7 }\n")?;
    fs::write(
        root.join("build.rs"),
        "use std::{fs, thread, time::Duration};\n\
         fn main() {\n\
             let Some(ready) = std::env::var_os(\"ZHOLD_TEST_READY\") else { return; };\n\
             if fs::write(ready, b\"ready\").is_err() { return; }\n\
             loop { thread::sleep(Duration::from_secs(1)); }\n\
         }\n",
    )?;
    git(root, &["init"])?;
    git(root, &["config", "user.email", "zhold@example.invalid"])?;
    git(root, &["config", "user.name", "zhold tests"])?;
    git(root, &["add", "."])?;
    git(root, &["commit", "-m", "fixture"])
}

fn zhold_command(project: &Path, store: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zhold"));
    command
        .arg("--store")
        .arg(store)
        .args(["--budget", "100GiB", "cargo", "check"])
        .current_dir(project)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn send_console_break(process_group: u32) -> Result<(), io::Error> {
    let script = format!(
        "$native = Add-Type -MemberDefinition '[DllImport(\"kernel32.dll\", SetLastError=true)] public static extern bool GenerateConsoleCtrlEvent(uint signal, uint group);' -Name Native -Namespace Zhold -PassThru; if (-not $native::GenerateConsoleCtrlEvent(1, {process_group})) {{ exit 1 }}"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "failed to send console break: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
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
