//! Fault-injection tests for Cargo sentinel ownership and recovery.

use std::{
    fs, io,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use tempfile::tempdir;
use zhold_core::ArenaState;
use zhold_store::Store;

#[cfg(unix)]
use nix::{
    errno::Errno,
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};

#[test]
fn lease_sentinel_survives_front_process_termination() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store_root = temporary.path().join("store");
    let release = temporary.path().join("release-build");
    create_project(&project)?;

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

#[cfg(unix)]
#[test]
fn sentinel_death_keeps_a_live_cargo_arena_suspect_and_uncollectible()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store_root = temporary.path().join("store");
    let release = temporary.path().join("release-build");
    create_project(&project)?;

    let mut front = zhold_command(&project, &store_root, &["cargo", "check"])
        .env("ZHOLD_TEST_RELEASE", &release)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    wait_for_state(&store_root, ArenaState::Active, Duration::from_secs(20))?;
    let sentinel = wait_for_child_process(front.id(), Duration::from_secs(20))?;
    let cargo = wait_for_child_process(sentinel, Duration::from_secs(20))?;

    kill(process_id(sentinel)?, Signal::SIGKILL)?;
    let _front_status = front.wait()?;
    wait_for_state(&store_root, ArenaState::Suspect, Duration::from_secs(20))?;
    let cargo_survived = process_is_alive(cargo)?;
    let gc = zhold(&project, &store_root, &["gc", "1B"])?;
    let inventory = Store::open(&store_root)?.inventory()?;

    fs::write(&release, b"continue")?;
    if wait_for_process_exit(cargo, Duration::from_secs(20)).is_err() {
        let _cleanup = killpg(process_id(cargo)?, Signal::SIGKILL);
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "orphaned Cargo process group did not terminate",
        )
        .into());
    }
    let selector = inventory.arenas[0].record.id.to_string();
    let recovered = zhold(
        &project,
        &store_root,
        &["recover", &selector, "--terminated"],
    )?;

    assert!(
        cargo_survived,
        "Cargo exited when only its sentinel was killed"
    );
    assert_eq!(gc.status.code(), Some(2));
    assert_eq!(inventory.arenas.len(), 1);
    assert_eq!(inventory.arenas[0].record.state(), ArenaState::Suspect);
    assert!(inventory.reserved > zhold_core::ByteSize::ZERO);
    assert!(recovered.status.success());
    assert_eq!(only_state(&store_root)?, ArenaState::Idle);
    Ok(())
}

fn create_project(root: &Path) -> Result<(), io::Error> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"zhold-fixture\"\nversion = \"0.1.0-alpha.1\"\nedition = \"2024\"\n",
    )?;
    fs::write(root.join("src/main.rs"), "fn main() {}\n")?;
    fs::write(root.join("build.rs"), waiting_build_script())?;
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
        .args(["--budget", "100GiB"])
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
        format!(
            "arena did not reach {expected:?}; last observation: {:?}",
            only_state(store)
        ),
    ))
}

#[cfg(unix)]
fn wait_for_child_process(parent: u32, timeout: Duration) -> Result<u32, io::Error> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(child) = child_processes()?
            .into_iter()
            .find_map(|(pid, ppid)| (ppid == parent).then_some(pid))
        {
            return Ok(child);
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("process {parent} did not spawn a child"),
    ))
}

#[cfg(unix)]
fn child_processes() -> Result<Vec<(u32, u32)>, io::Error> {
    let output = Command::new("ps").args(["-axo", "pid=,ppid="]).output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            "ps failed while inspecting the process tree",
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(io::Error::other)?
        .lines()
        .map(|line| {
            let mut fields = line.split_whitespace();
            let pid = parse_process_field(&mut fields, "process ID")?;
            let parent = parse_process_field(&mut fields, "parent process ID")?;
            Ok((pid, parent))
        })
        .collect()
}

#[cfg(unix)]
fn parse_process_field<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
    label: &str,
) -> Result<u32, io::Error> {
    fields
        .next()
        .ok_or_else(|| io::Error::other(format!("ps omitted a {label}")))?
        .parse()
        .map_err(io::Error::other)
}

#[cfg(unix)]
fn wait_for_process_exit(pid: u32, timeout: Duration) -> Result<(), io::Error> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !process_is_alive(pid)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("process {pid} remained alive"),
    ))
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> Result<bool, io::Error> {
    match kill(process_id(pid)?, None) {
        Ok(()) | Err(Errno::EPERM) => Ok(true),
        Err(Errno::ESRCH) => Ok(false),
        Err(error) => Err(io::Error::from_raw_os_error(error as i32)),
    }
}

#[cfg(unix)]
fn process_id(pid: u32) -> Result<Pid, io::Error> {
    i32::try_from(pid)
        .map(Pid::from_raw)
        .map_err(|_| io::Error::other("process identifier exceeds i32"))
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
