use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

use portable_pty::CommandBuilder;
use tempfile::TempDir;

#[path = "terminal_session.rs"]
mod terminal_session;

pub(crate) use terminal_session::{PtySession, process_group_is_alive};

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

    pub(crate) fn store(&self) -> &Path {
        &self.store
    }

    pub(crate) fn project(&self) -> &Path {
        &self.project
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
    "use std::{env, fs, io, io::BufRead, path::Path, process::Command, thread, time::Duration};\n\
     fn main() -> io::Result<()> {\n\
         println!(\"READY\");\n\
         match env::args().nth(1).as_deref() {\n\
             Some(\"echo\") => {\n\
                 let mut line = String::new();\n\
                 io::stdin().lock().read_line(&mut line)?;\n\
                 println!(\"ECHO:{}\", line.trim_end());\n\
             }\n\
             Some(\"fail\") => std::process::exit(23),\n\
             Some(\"release\") => {\n\
                 let release = env::var_os(\"ZHOLD_TEST_RELEASE\")\n\
                     .ok_or_else(|| io::Error::other(\"release marker is missing\"))?;\n\
                 while !Path::new(&release).is_file() {\n\
                     thread::sleep(Duration::from_millis(10));\n\
                 }\n\
             }\n\
             Some(\"suspend\") => {\n\
                 let probe = env::var_os(\"ZHOLD_TEST_SUSPEND_PROBE\")\n\
                     .ok_or_else(|| io::Error::other(\"resume probe is missing\"))?;\n\
                 while !Path::new(&probe).is_file() {\n\
                     thread::sleep(Duration::from_millis(10));\n\
                 }\n\
                 let resumed = env::var_os(\"ZHOLD_TEST_SUSPEND_RESUMED\")\n\
                     .ok_or_else(|| io::Error::other(\"resumed marker is missing\"))?;\n\
                 fs::write(resumed, b\"resumed\")?;\n\
                 let release = env::var_os(\"ZHOLD_TEST_RELEASE\")\n\
                     .ok_or_else(|| io::Error::other(\"release marker is missing\"))?;\n\
                 while !Path::new(&release).is_file() {\n\
                     thread::sleep(Duration::from_millis(10));\n\
                 }\n\
             }\n\
             Some(\"ignore\") => {\n\
                 let script = \"trap '' INT TERM HUP QUIT; printf 'IGNORING_READY\\n'; while :; do sleep 1; done\";\n\
                 let status = Command::new(\"sh\").args([\"-c\", script]).status()?;\n\
                 std::process::exit(status.code().unwrap_or(1));\n\
             }\n\
             _ => loop { thread::sleep(Duration::from_secs(1)); },\n\
         }\n\
         Ok(())\n\
     }\n"
}

fn driver_script() -> &'static str {
    "#!/bin/sh\n\
     trap '' INT TERM HUP QUIT\n\
     \"$1\" --store \"$2\" --budget 100GiB cargo run --quiet -- \"$3\"\n\
     status=$?\n\
     printf '\\nZHOLD_STATUS:%s\\n' \"$status\"\n\
     IFS= read -r restored\n\
     printf 'RESTORED:%s\\n' \"$restored\"\n\
     exit 0\n"
}
