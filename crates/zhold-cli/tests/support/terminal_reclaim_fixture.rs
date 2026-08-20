use std::{
    env, fs, io,
    io::Write,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use nix::{
    errno::Errno,
    sys::{
        signal::{SigSet, SigmaskHow, Signal, kill},
        wait::{WaitPidFlag, WaitStatus, waitpid},
    },
    unistd::{Pid, getpgrp, tcsetpgrp},
};
use portable_pty::CommandBuilder;

use super::terminal_fixture::{PtySession, TerminalFixture};

const DRIVER_MODE: &str = "ZHOLD_TEST_TERMINAL_RECLAIM_DRIVER";
const DRIVER_RELEASE: &str = "ZHOLD_TEST_DRIVER_RELEASE";
const FRONT_PID: &str = "ZHOLD_TEST_FRONT_PID";
const RECLAIMED: &str = "ZHOLD_TEST_DRIVER_RECLAIMED";
const RELEASE_BUILD: &str = "ZHOLD_TEST_RELEASE";
const STORE: &str = "ZHOLD_TEST_STORE";
const PROJECT: &str = "ZHOLD_TEST_PROJECT";
const STANDARD_INPUT: i32 = 0;

pub(crate) struct TerminalReclaimFixture {
    base: TerminalFixture,
    build_release: PathBuf,
    driver_release: PathBuf,
    front_pid: PathBuf,
    reclaimed: PathBuf,
}

impl TerminalReclaimFixture {
    pub(crate) fn create() -> Result<Self, Box<dyn std::error::Error>> {
        let base = TerminalFixture::create()?;
        let root = base
            .store()
            .parent()
            .ok_or_else(|| io::Error::other("terminal fixture has no root"))?;
        Ok(Self {
            build_release: root.join("release-build"),
            driver_release: root.join("release-driver"),
            front_pid: root.join("front-pid"),
            reclaimed: root.join("driver-reclaimed"),
            base,
        })
    }

    pub(crate) fn spawn(&self) -> Result<PtySession, Box<dyn std::error::Error>> {
        let mut command = CommandBuilder::new(env::current_exe()?);
        command.arg("--exact");
        command.arg("terminal_reclaim_driver");
        command.arg("--nocapture");
        command.cwd(self.base.project());
        command.env(DRIVER_MODE, "1");
        command.env(DRIVER_RELEASE, &self.driver_release);
        command.env(FRONT_PID, &self.front_pid);
        command.env(RECLAIMED, &self.reclaimed);
        command.env(RELEASE_BUILD, &self.build_release);
        command.env(STORE, self.base.store());
        command.env(PROJECT, self.base.project());
        command.env("CARGO_TERM_COLOR", "never");
        PtySession::spawn(command)
    }

    pub(crate) fn front_pid(&self, timeout: Duration) -> Result<Pid, io::Error> {
        wait_for_file(&self.front_pid, timeout)?;
        let raw = fs::read_to_string(&self.front_pid)?
            .trim()
            .parse::<i32>()
            .map_err(io::Error::other)?;
        Ok(Pid::from_raw(raw))
    }

    pub(crate) fn release_build(&self) -> Result<(), io::Error> {
        fs::write(&self.build_release, b"continue")
    }

    pub(crate) fn release_driver(&self) -> Result<(), io::Error> {
        fs::write(&self.driver_release, b"continue")
    }

    pub(crate) fn reclaimed(&self, timeout: Duration) -> Result<(), io::Error> {
        wait_for_file(&self.reclaimed, timeout)
    }

    pub(crate) fn store(&self) -> &Path {
        self.base.store()
    }
}

pub(crate) fn run_driver_if_requested() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os(DRIVER_MODE).is_none() {
        return Ok(());
    }
    let project = required_path(PROJECT)?;
    let store = required_path(STORE)?;
    let build_release = required_path(RELEASE_BUILD)?;
    let driver_release = required_path(DRIVER_RELEASE)?;
    let front_pid_file = required_path(FRONT_PID)?;
    let reclaimed = required_path(RECLAIMED)?;

    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "kill -STOP $$\nexec \"$@\"", "zhold-front"])
        .arg(env!("CARGO_BIN_EXE_zhold"))
        .arg("--store")
        .arg(store)
        .args([
            "--budget", "100GiB", "cargo", "run", "--quiet", "--", "release",
        ])
        .current_dir(project)
        .env(RELEASE_BUILD, build_release)
        .env("CARGO_TERM_COLOR", "never")
        .process_group(0);
    let mut front = command.spawn()?;
    let front_pid = process_id(front.id())?;
    wait_for_stop(front_pid)?;
    fs::write(front_pid_file, front_pid.as_raw().to_string())?;
    set_foreground(front_pid)?;
    kill(front_pid, Signal::SIGCONT).map_err(os_error)?;

    let status = front.wait()?;
    if status.success() {
        return Err(io::Error::other("front process exited without termination").into());
    }
    let driver_group = getpgrp();
    set_foreground(driver_group)?;
    fs::write(reclaimed, b"reclaimed")?;
    println!("DRIVER_RECLAIMED:{driver_group}");
    io::stdout().flush()?;
    wait_for_file(&driver_release, Duration::from_secs(90))?;
    Ok(())
}

fn wait_for_stop(process: Pid) -> Result<(), io::Error> {
    match waitpid(process, Some(WaitPidFlag::WUNTRACED)).map_err(os_error)? {
        WaitStatus::Stopped(stopped, Signal::SIGSTOP) if stopped == process => Ok(()),
        status => Err(io::Error::other(format!(
            "front process did not stop before terminal handoff: {status:?}"
        ))),
    }
}

fn set_foreground(group: Pid) -> Result<(), io::Error> {
    let mut blocked = SigSet::empty();
    blocked.add(Signal::SIGTTOU);
    let previous = blocked
        .thread_swap_mask(SigmaskHow::SIG_BLOCK)
        .map_err(os_error)?;
    let changed = tcsetpgrp(STANDARD_INPUT, group).map_err(os_error);
    let restored = previous.thread_set_mask().map_err(os_error);
    changed.and(restored)
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
        format!("fixture marker did not appear: {}", path.display()),
    ))
}

fn required_path(name: &str) -> Result<PathBuf, io::Error> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other(format!("{name} is missing")))
}

fn process_id(id: u32) -> Result<Pid, io::Error> {
    i32::try_from(id)
        .map(Pid::from_raw)
        .map_err(|_| io::Error::other("process identifier exceeds i32"))
}

fn os_error(error: Errno) -> io::Error {
    io::Error::from_raw_os_error(error as i32)
}
