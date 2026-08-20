use std::{io, os::unix::process::ExitStatusExt, process::ExitStatus};

use nix::{
    sys::{
        signal::Signal,
        wait::{WaitPidFlag, WaitStatus, waitpid},
    },
    unistd::Pid,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum LeaderEvent {
    Running,
    Continued,
    Stopped(Signal),
    Exited(ExitStatus),
}

#[derive(Debug)]
pub(super) struct Leader {
    process: Pid,
}

impl Leader {
    pub(super) const fn new(process: Pid) -> Self {
        Self { process }
    }

    pub(super) fn poll(&self) -> io::Result<LeaderEvent> {
        let flags = WaitPidFlag::WNOHANG | WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED;
        waitpid(self.process, Some(flags))
            .map(event_from_status)
            .map_err(os_error)
    }

    pub(super) fn wait_for_exit(&self) -> io::Result<ExitStatus> {
        loop {
            match waitpid(self.process, None).map_err(os_error)? {
                WaitStatus::Exited(_, code) => return Ok(exited(code)),
                WaitStatus::Signaled(_, signal, _) => return Ok(signaled(signal)),
                _ => {}
            }
        }
    }
}

fn event_from_status(status: WaitStatus) -> LeaderEvent {
    match status {
        WaitStatus::Exited(_, code) => LeaderEvent::Exited(exited(code)),
        WaitStatus::Signaled(_, signal, _) => LeaderEvent::Exited(signaled(signal)),
        WaitStatus::Stopped(_, signal) => LeaderEvent::Stopped(signal),
        WaitStatus::Continued(_) => LeaderEvent::Continued,
        WaitStatus::StillAlive => LeaderEvent::Running,
        #[cfg(any(target_os = "android", target_os = "linux"))]
        WaitStatus::PtraceEvent(_, _, _) | WaitStatus::PtraceSyscall(_) => LeaderEvent::Running,
    }
}

fn exited(code: i32) -> ExitStatus {
    ExitStatus::from_raw(code << 8)
}

fn signaled(signal: Signal) -> ExitStatus {
    ExitStatus::from_raw(signal as i32)
}

fn os_error(error: nix::errno::Errno) -> io::Error {
    io::Error::from_raw_os_error(error as i32)
}
