use std::{
    env, fs, io,
    io::Write,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, ExitStatus},
    thread,
    time::{Duration, Instant},
};

use nix::{
    errno::Errno,
    sys::{
        signal::{SigSet, SigmaskHow, Signal, killpg},
        wait::{WaitPidFlag, WaitStatus, waitpid},
    },
    unistd::{Pid, getpgrp, tcgetpgrp, tcsetpgrp},
};

use super::{
    BACKGROUND_ACTION, BUILD_RELEASE, DRIVER_MODE, DRIVER_RELEASE, FOREGROUND_ACTION,
    FOREGROUND_LATER, FRONT_PID, PROJECT, RESUME_PROBE, RESUMED, ResumeMode, STORE, wait_for_file,
};

const STANDARD_INPUT: i32 = 0;
const TIMEOUT: Duration = Duration::from_secs(60);

pub(super) fn run_if_requested() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os(DRIVER_MODE).is_none() {
        return Ok(());
    }
    let driver_group = getpgrp();
    let mut front = spawn_front()?;
    let front_group = process_id(front.id())?;
    let result = run_front(&mut front, front_group, driver_group);
    if result.is_err() {
        let _killed = killpg(front_group, Signal::SIGKILL);
        let _restored = set_foreground(driver_group);
        let _waited = front.wait();
    }
    result.map_err(Into::into)
}

fn spawn_front() -> Result<Child, io::Error> {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "kill -STOP $$\nexec \"$@\"", "zhold-front"])
        .arg(env!("CARGO_BIN_EXE_zhold"))
        .arg("--store")
        .arg(required_path(STORE)?)
        .args([
            "--budget", "100GiB", "cargo", "run", "--quiet", "--", "suspend",
        ])
        .current_dir(required_path(PROJECT)?)
        .env(BUILD_RELEASE, required_path(BUILD_RELEASE)?)
        .env(RESUME_PROBE, required_path(RESUME_PROBE)?)
        .env(RESUMED, required_path(RESUMED)?)
        .env("CARGO_TERM_COLOR", "never")
        .process_group(0)
        .spawn()
}

fn run_front(front: &mut Child, front_group: Pid, driver_group: Pid) -> Result<(), io::Error> {
    let initial = wait_for_stop(front_group, TIMEOUT)?;
    if initial != Signal::SIGSTOP {
        return Err(io::Error::other(format!(
            "front process stopped unexpectedly during setup: {initial:?}"
        )));
    }
    publish_front_pid(front_group)?;
    set_foreground(front_group)?;
    signal_group(front_group, Signal::SIGCONT)?;

    let stopped = wait_for_stop(front_group, TIMEOUT)?;
    require_foreground(front_group)?;
    set_foreground(driver_group)?;
    println!("JOB_STOPPED:{stopped:?}");
    io::stdout().flush()?;

    let mode = wait_for_resume_action(TIMEOUT)?;
    if mode == ResumeMode::Foreground {
        set_foreground(front_group)?;
    }
    signal_group(front_group, Signal::SIGCONT)?;

    let (status, promoted) = if mode == ResumeMode::Background {
        wait_for_exit_or_promotion(front, front_group, TIMEOUT)?
    } else {
        (wait_for_exit(front, TIMEOUT)?, false)
    };
    let expected = if mode == ResumeMode::Foreground || promoted {
        front_group
    } else {
        driver_group
    };
    require_foreground(expected)?;
    set_foreground(driver_group)?;
    println!("JOB_EXIT:{}", status.code().unwrap_or(1));
    io::stdout().flush()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "zhold front process failed: {status}"
        )));
    }
    wait_for_file(&required_path(DRIVER_RELEASE)?, TIMEOUT)
}

fn wait_for_exit_or_promotion(
    child: &mut Child,
    front_group: Pid,
    timeout: Duration,
) -> Result<(ExitStatus, bool), io::Error> {
    let promotion = required_path(FOREGROUND_LATER)?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if promotion.is_file() {
            set_foreground(front_group)?;
            signal_group(front_group, Signal::SIGCONT)?;
            return wait_for_exit(child, timeout).map(|status| (status, true));
        }
        if let Some(status) = child.try_wait()? {
            return Ok((status, false));
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "front process did not exit or return to the foreground",
    ))
}

fn publish_front_pid(front_group: Pid) -> Result<(), io::Error> {
    let destination = required_path(FRONT_PID)?;
    let staging = destination.with_extension("new");
    fs::write(&staging, front_group.as_raw().to_string())?;
    fs::rename(staging, destination)
}

fn wait_for_resume_action(timeout: Duration) -> Result<ResumeMode, io::Error> {
    let foreground = required_path(FOREGROUND_ACTION)?;
    let background = required_path(BACKGROUND_ACTION)?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if foreground.is_file() {
            return Ok(ResumeMode::Foreground);
        }
        if background.is_file() {
            return Ok(ResumeMode::Background);
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "resume action did not appear",
    ))
}

fn wait_for_stop(process: Pid, timeout: Duration) -> Result<Signal, io::Error> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match waitpid(process, Some(WaitPidFlag::WNOHANG | WaitPidFlag::WUNTRACED))
            .map_err(os_error)?
        {
            WaitStatus::Stopped(stopped, signal) if stopped == process => return Ok(signal),
            WaitStatus::StillAlive | WaitStatus::Continued(_) => {
                thread::sleep(Duration::from_millis(20));
            }
            status => {
                return Err(io::Error::other(format!(
                    "front process exited before stopping: {status:?}"
                )));
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "front process did not stop",
    ))
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> Result<ExitStatus, io::Error> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "front process did not exit",
    ))
}

fn require_foreground(expected: Pid) -> Result<(), io::Error> {
    let actual = tcgetpgrp(STANDARD_INPUT).map_err(os_error)?;
    if actual == expected {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "terminal foreground is {actual}, expected {expected}"
        )))
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

fn signal_group(group: Pid, signal: Signal) -> Result<(), io::Error> {
    killpg(group, signal).map_err(os_error)
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
