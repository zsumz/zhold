use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

use tempfile::tempdir;

use super::{ExclusiveFileLock, LockState};

const HELPER_MODE: &str = "ZHOLD_LOCK_HELPER";
const LOCK_PATH: &str = "ZHOLD_LOCK_PATH";
const READY_PATH: &str = "ZHOLD_READY_PATH";
const RELEASE_PATH: &str = "ZHOLD_RELEASE_PATH";

#[test]
fn another_process_makes_the_lock_authoritatively_held() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::create()?;
    let mut child = fixture.spawn()?;
    fixture.wait_until_ready(&mut child)?;

    assert_eq!(ExclusiveFileLock::probe(&fixture.lock)?, LockState::Held);

    fixture.release()?;
    assert!(child.wait()?.success());
    assert_eq!(
        ExclusiveFileLock::probe(&fixture.lock)?,
        LockState::Available
    );
    Ok(())
}

#[test]
fn process_death_releases_the_operating_system_lock() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::create()?;
    let mut child = fixture.spawn()?;
    fixture.wait_until_ready(&mut child)?;
    assert_eq!(ExclusiveFileLock::probe(&fixture.lock)?, LockState::Held);

    child.kill()?;
    let _status = child.wait()?;

    assert_eq!(
        ExclusiveFileLock::probe(&fixture.lock)?,
        LockState::Available
    );
    Ok(())
}

#[test]
fn lock_process_helper() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os(HELPER_MODE).is_none() {
        return Ok(());
    }
    let lock = required_path(LOCK_PATH)?;
    let ready = required_path(READY_PATH)?;
    let release = required_path(RELEASE_PATH)?;
    let _held = ExclusiveFileLock::acquire(&lock)?;
    fs::write(&ready, b"ready")?;
    wait_for_path(&release, Duration::from_secs(10))
}

#[derive(Debug)]
struct Fixture {
    _temporary: tempfile::TempDir,
    lock: PathBuf,
    ready: PathBuf,
    release: PathBuf,
}

impl Fixture {
    fn create() -> Result<Self, io::Error> {
        let temporary = tempdir()?;
        Ok(Self {
            lock: temporary.path().join("lease.lock"),
            ready: temporary.path().join("ready"),
            release: temporary.path().join("release"),
            _temporary: temporary,
        })
    }

    fn spawn(&self) -> Result<Child, io::Error> {
        Command::new(env::current_exe()?)
            .args([
                "--exact",
                "lock::file_lock_test::lock_process_helper",
                "--nocapture",
            ])
            .env(HELPER_MODE, "1")
            .env(LOCK_PATH, &self.lock)
            .env(READY_PATH, &self.ready)
            .env(RELEASE_PATH, &self.release)
            .spawn()
    }

    fn wait_until_ready(&self, child: &mut Child) -> Result<(), io::Error> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if self.ready.is_file() {
                return Ok(());
            }
            if let Some(status) = child.try_wait()? {
                return Err(io::Error::other(format!(
                    "lock helper exited before readiness: {status}"
                )));
            }
            thread::sleep(Duration::from_millis(10));
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "lock helper did not become ready",
        ))
    }

    fn release(&self) -> Result<(), io::Error> {
        fs::write(&self.release, b"release")
    }
}

fn required_path(name: &str) -> Result<PathBuf, io::Error> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("missing {name}")))
}

fn wait_for_path(path: &Path, timeout: Duration) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.is_file() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("timed out waiting for {}", path.display()),
    )
    .into())
}
