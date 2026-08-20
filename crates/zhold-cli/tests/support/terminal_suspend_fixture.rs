use std::{
    env, fs, io,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use nix::unistd::Pid;
use portable_pty::CommandBuilder;

use super::terminal_fixture::{PtySession, TerminalFixture};

#[path = "terminal_suspend_driver.rs"]
mod driver;

const BACKGROUND_ACTION: &str = "ZHOLD_TEST_SUSPEND_BACKGROUND";
const BUILD_RELEASE: &str = "ZHOLD_TEST_RELEASE";
const DRIVER_MODE: &str = "ZHOLD_TEST_SUSPEND_DRIVER";
const DRIVER_RELEASE: &str = "ZHOLD_TEST_SUSPEND_DRIVER_RELEASE";
const FRONT_PID: &str = "ZHOLD_TEST_SUSPEND_FRONT_PID";
const FOREGROUND_ACTION: &str = "ZHOLD_TEST_SUSPEND_FOREGROUND";
const FOREGROUND_LATER: &str = "ZHOLD_TEST_SUSPEND_FOREGROUND_LATER";
const PROJECT: &str = "ZHOLD_TEST_PROJECT";
const RESUME_PROBE: &str = "ZHOLD_TEST_SUSPEND_PROBE";
const RESUMED: &str = "ZHOLD_TEST_SUSPEND_RESUMED";
const STORE: &str = "ZHOLD_TEST_STORE";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResumeMode {
    Foreground,
    Background,
}

pub(crate) struct TerminalSuspendFixture {
    base: TerminalFixture,
    background_action: PathBuf,
    build_release: PathBuf,
    driver_release: PathBuf,
    front_pid: PathBuf,
    foreground_action: PathBuf,
    foreground_later: PathBuf,
    resume_probe: PathBuf,
    resumed: PathBuf,
}

impl TerminalSuspendFixture {
    pub(crate) fn create() -> Result<Self, Box<dyn std::error::Error>> {
        let base = TerminalFixture::create()?;
        let root = base
            .store()
            .parent()
            .ok_or_else(|| io::Error::other("terminal fixture has no root"))?;
        Ok(Self {
            background_action: root.join("resume-background"),
            build_release: root.join("release-build"),
            driver_release: root.join("release-driver"),
            front_pid: root.join("suspend-front-pid"),
            foreground_action: root.join("resume-foreground"),
            foreground_later: root.join("foreground-later"),
            resume_probe: root.join("resume-probe"),
            resumed: root.join("resumed"),
            base,
        })
    }

    pub(crate) fn spawn(&self) -> Result<PtySession, Box<dyn std::error::Error>> {
        let mut command = CommandBuilder::new(env::current_exe()?);
        command.arg("--exact");
        command.arg("terminal_suspend_driver");
        command.arg("--nocapture");
        command.cwd(self.base.project());
        for (name, value) in self.environment() {
            command.env(name, value);
        }
        command.env(DRIVER_MODE, "1");
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

    pub(crate) fn prepare_resume(&self) -> Result<(), io::Error> {
        fs::write(&self.resume_probe, b"probe")
    }

    pub(crate) fn resume(&self, mode: ResumeMode) -> Result<(), io::Error> {
        let action = match mode {
            ResumeMode::Foreground => &self.foreground_action,
            ResumeMode::Background => &self.background_action,
        };
        fs::write(action, b"resume")
    }

    pub(crate) fn wait_for_resumed(&self, timeout: Duration) -> Result<(), io::Error> {
        wait_for_file(&self.resumed, timeout)
    }

    pub(crate) fn foreground_after_background(&self) -> Result<(), io::Error> {
        fs::write(&self.foreground_later, b"foreground")
    }

    pub(crate) fn has_resumed(&self) -> bool {
        self.resumed.is_file()
    }

    pub(crate) fn release_build(&self) -> Result<(), io::Error> {
        fs::write(&self.build_release, b"continue")
    }

    pub(crate) fn release_driver(&self) -> Result<(), io::Error> {
        fs::write(&self.driver_release, b"continue")
    }

    pub(crate) fn store(&self) -> &Path {
        self.base.store()
    }

    fn environment(&self) -> [(&'static str, &Path); 10] {
        [
            (BACKGROUND_ACTION, &self.background_action),
            (BUILD_RELEASE, &self.build_release),
            (DRIVER_RELEASE, &self.driver_release),
            (FRONT_PID, &self.front_pid),
            (FOREGROUND_ACTION, &self.foreground_action),
            (FOREGROUND_LATER, &self.foreground_later),
            (PROJECT, self.base.project()),
            (RESUME_PROBE, &self.resume_probe),
            (RESUMED, &self.resumed),
            (STORE, self.base.store()),
        ]
    }
}

pub(crate) fn run_driver_if_requested() -> Result<(), Box<dyn std::error::Error>> {
    driver::run_if_requested()
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
