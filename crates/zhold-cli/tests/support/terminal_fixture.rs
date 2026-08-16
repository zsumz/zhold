use std::{
    fs, io,
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use portable_pty::{Child, CommandBuilder, ExitStatus, PtySize, native_pty_system};
use tempfile::TempDir;

const WAIT_STEP: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub(crate) struct TerminalFixture {
    _temporary: TempDir,
    project: PathBuf,
    store: PathBuf,
    driver: PathBuf,
}

impl TerminalFixture {
    pub(crate) fn create() -> Result<Self, Box<dyn std::error::Error>> {
        let temporary = tempfile::tempdir()?;
        let project = temporary.path().join("project");
        let store = temporary.path().join("store");
        let driver = temporary.path().join("terminal-driver.sh");
        fs::create_dir_all(project.join("src"))?;
        fs::write(project.join("Cargo.toml"), manifest())?;
        fs::write(project.join("src/main.rs"), program())?;
        fs::write(&driver, driver_script())?;
        let mut permissions = fs::metadata(&driver)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&driver, permissions)?;
        initialize_git(&project)?;
        Ok(Self {
            _temporary: temporary,
            project,
            store,
            driver,
        })
    }

    pub(crate) fn spawn(&self, mode: &str) -> Result<PtySession, Box<dyn std::error::Error>> {
        let mut command = CommandBuilder::new(&self.driver);
        command.arg(env!("CARGO_BIN_EXE_zhold"));
        command.arg(&self.store);
        command.arg(mode);
        command.cwd(&self.project);
        command.env("CARGO_TERM_COLOR", "never");
        PtySession::spawn(command)
    }
}

pub(crate) struct PtySession {
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    output: Receiver<Vec<u8>>,
    captured: Vec<u8>,
    complete: bool,
}

impl PtySession {
    fn spawn(command: CommandBuilder) -> Result<Self, Box<dyn std::error::Error>> {
        let pair = native_pty_system().openpty(PtySize::default())?;
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let (sender, output) = mpsc::channel();
        thread::spawn(move || copy_output(&mut reader, &sender));
        Ok(Self {
            child,
            writer,
            output,
            captured: Vec::new(),
            complete: false,
        })
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> Result<(), io::Error> {
        self.writer.write_all(bytes)?;
        self.writer.flush()
    }

    pub(crate) fn write_line(&mut self, line: &str) -> Result<(), io::Error> {
        self.write(format!("{line}\n").as_bytes())
    }

    pub(crate) fn wait_for(&mut self, expected: &str, timeout: Duration) -> Result<(), io::Error> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.text().contains(expected) {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.output.recv_timeout(remaining.min(WAIT_STEP)) {
                Ok(bytes) => self.captured.extend(bytes),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("PTY output did not contain {expected:?}: {:?}", self.text()),
        ))
    }

    pub(crate) fn wait_for_exit(&mut self, timeout: Duration) -> Result<ExitStatus, io::Error> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            self.drain_output();
            if let Some(status) = self.child.try_wait()? {
                self.complete = true;
                self.drain_output();
                return Ok(status);
            }
            thread::sleep(WAIT_STEP);
        }
        let _killed = self.child.kill();
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("PTY child did not exit: {:?}", self.text()),
        ))
    }

    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.captured).into_owned()
    }

    fn drain_output(&mut self) {
        self.captured.extend(self.output.try_iter().flatten());
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        if !self.complete {
            let _killed = self.child.kill();
            let _waited = self.child.wait();
        }
    }
}

fn copy_output(reader: &mut impl Read, sender: &mpsc::Sender<Vec<u8>>) {
    let mut buffer = [0_u8; 1024];
    while let Ok(size) = reader.read(&mut buffer) {
        if size == 0 || sender.send(buffer[..size].to_vec()).is_err() {
            return;
        }
    }
}

fn initialize_git(root: &Path) -> Result<(), io::Error> {
    for arguments in [
        &["init", "-q"][..],
        &["config", "user.email", "zhold@example.invalid"],
        &["config", "user.name", "zhold tests"],
        &["add", "."],
        &["commit", "-q", "-m", "fixture"],
    ] {
        let status = Command::new("git")
            .args(["-c", "commit.gpgsign=false"])
            .args(arguments)
            .current_dir(root)
            .status()?;
        if !status.success() {
            return Err(io::Error::other(format!(
                "git {} failed with {status}",
                arguments.join(" ")
            )));
        }
    }
    Ok(())
}

fn manifest() -> &'static str {
    "[package]\nname = \"terminal-fixture\"\nversion = \"0.0.1\"\nedition = \"2024\"\n"
}

fn program() -> &'static str {
    "use std::{env, io, io::BufRead, thread, time::Duration};\n\
     fn main() -> io::Result<()> {\n\
         println!(\"READY\");\n\
         match env::args().nth(1).as_deref() {\n\
             Some(\"echo\") => {\n\
                 let mut line = String::new();\n\
                 io::stdin().lock().read_line(&mut line)?;\n\
                 println!(\"ECHO:{}\", line.trim_end());\n\
             }\n\
             Some(\"fail\") => std::process::exit(23),\n\
             _ => loop { thread::sleep(Duration::from_secs(1)); },\n\
         }\n\
         Ok(())\n\
     }\n"
}

fn driver_script() -> &'static str {
    "#!/bin/sh\n\
     \"$1\" --store \"$2\" --budget 100GiB cargo run --quiet -- \"$3\"\n\
     status=$?\n\
     printf '\\nZHOLD_STATUS:%s\\n' \"$status\"\n\
     IFS= read -r restored\n\
     printf 'RESTORED:%s\\n' \"$restored\"\n\
     exit 0\n"
}
