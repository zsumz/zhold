use std::{
    io,
    os::unix::process::ExitStatusExt,
    process::{Command, ExitStatus},
    thread,
};

use command_group::{CommandGroup, GroupChild};
use nix::{
    errno::Errno,
    sys::signal::{Signal, killpg},
    unistd::{Pid, getpgrp},
};
use signal_hook::{
    consts::signal::{SIGCONT, SIGHUP, SIGINT, SIGQUIT, SIGTERM},
    iterator::Signals,
};

use super::SpawnError;

mod leader;
mod signals;
mod terminal;

use leader::{Leader, LeaderEvent};
use signals::SignalForwarder;
use terminal::ForegroundTerminal;

#[derive(Debug)]
pub(in crate::command::cargo) struct PlatformSupervisor {
    _child: GroupChild,
    group: Pid,
    leader: Leader,
    leader_status: Option<ExitStatus>,
    quiesce_stop_pending: bool,
    forwarder: SignalForwarder,
    terminal: ForegroundTerminal,
    complete: bool,
}

impl PlatformSupervisor {
    pub(in crate::command::cargo) fn spawn(
        command: &mut Command,
        spawned: impl FnOnce() -> io::Result<()>,
    ) -> Result<Self, SpawnError> {
        let signals = Signals::new([SIGINT, SIGTERM, SIGHUP, SIGQUIT, SIGCONT])
            .map_err(SpawnError::before_child)?;
        let mut child = command.group_spawn().map_err(SpawnError::before_child)?;
        if let Err(error) = spawned() {
            return cleanup_failed_spawn(&mut child, error);
        }
        let group = match process_group(child.id()) {
            Ok(group) => group,
            Err(error) => return cleanup_failed_spawn(&mut child, error),
        };
        let forwarder = match SignalForwarder::spawn(signals, group) {
            Ok(forwarder) => forwarder,
            Err(error) => return cleanup_failed_spawn(&mut child, error),
        };
        let mut forwarder = forwarder;
        let terminal = match ForegroundTerminal::hand_off(group) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _forwarder = forwarder.stop();
                return cleanup_failed_spawn(&mut child, error);
            }
        };
        Ok(Self {
            _child: child,
            group,
            leader: Leader::new(group),
            leader_status: None,
            quiesce_stop_pending: false,
            forwarder,
            terminal,
            complete: false,
        })
    }

    pub(in crate::command::cargo) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.forwarder
            .check()
            .map_err(|error| contextual("check forwarded signals", &error))?;
        let continue_requested = self.forwarder.take_continue_request();
        if self.leader_status.is_none() {
            match self
                .leader
                .poll()
                .map_err(|error| contextual("observe Cargo leader", &error))?
            {
                LeaderEvent::Running => {
                    let should_reconcile = continue_requested || self.quiesce_stop_pending;
                    self.quiesce_stop_pending = false;
                    if should_reconcile && self.wrapper_is_foreground()? {
                        self.resume_cargo()?;
                    }
                }
                LeaderEvent::Continued => {
                    if (continue_requested || self.quiesce_stop_pending)
                        && self.wrapper_is_foreground()?
                    {
                        self.resume_cargo()?;
                    }
                }
                LeaderEvent::Stopped(Signal::SIGSTOP) if self.quiesce_stop_pending => {
                    self.resume_cargo()?;
                }
                LeaderEvent::Stopped(_) if self.wrapper_is_foreground()? => {
                    self.resume_cargo()?;
                }
                LeaderEvent::Stopped(signal) => self.suspend_cargo(signal)?,
                LeaderEvent::Exited(status) => self.record_leader_exit(status),
            }
        }
        if self.leader_status.is_some() {
            self.terminal.restore()?;
        }
        if self.leader_status.is_some() && !group_alive(self.group)? {
            self.forwarder.stop()?;
            self.complete = true;
            return Ok(self.leader_status);
        }
        Ok(None)
    }

    pub(in crate::command::cargo) fn terminate_and_wait(&mut self) -> io::Result<()> {
        if let Err(error) = killpg(self.group, Signal::SIGKILL)
            && error != Errno::ESRCH
        {
            return Err(os_error(error));
        }
        if self.leader_status.is_none() {
            self.leader_status = Some(self.leader.wait_for_exit()?);
        }
        while group_alive(self.group)? {
            thread::sleep(std::time::Duration::from_millis(10));
        }
        self.terminal.restore()?;
        self.forwarder.stop()?;
        self.complete = true;
        Ok(())
    }

    pub(in crate::command::cargo) fn was_interrupted(&self) -> bool {
        self.forwarder.was_interrupted()
    }

    fn suspend_cargo(&mut self, stop_signal: Signal) -> io::Result<()> {
        if !signal_group(self.group, Signal::SIGSTOP)
            .map_err(|error| contextual("stop Cargo process group", &error))?
        {
            self.quiesce_stop_pending = false;
            return self.terminal.restore();
        }
        self.quiesce_stop_pending = true;
        self.terminal
            .reclaim_for_stop()
            .map_err(|error| contextual("reclaim terminal for suspended job", &error))?;
        if !signal_group(getpgrp(), stop_signal)
            .map_err(|error| contextual("stop zhold process group", &error))?
        {
            return Err(io::Error::other(
                "zhold process group disappeared while suspending",
            ));
        }
        self.resume_cargo()
    }

    fn resume_cargo(&mut self) -> io::Result<()> {
        self.terminal
            .resume_for_continue()
            .map_err(|error| contextual("reconcile terminal after continue", &error))?;
        match signal_group(self.group, Signal::SIGCONT) {
            Ok(true) => {}
            Ok(false) => self.terminal.restore()?,
            Err(error) if error.raw_os_error() == Some(Errno::EPERM as i32) => {
                if self.confirm_leader_exit()? {
                    self.terminal.restore()?;
                } else {
                    return Err(contextual("continue Cargo process group", &error));
                }
            }
            Err(error) => return Err(contextual("continue Cargo process group", &error)),
        }
        Ok(())
    }

    fn wrapper_is_foreground(&self) -> io::Result<bool> {
        self.terminal
            .wrapper_is_foreground()
            .map_err(|error| contextual("inspect terminal ownership", &error))
    }

    fn record_leader_exit(&mut self, status: ExitStatus) {
        self.quiesce_stop_pending = false;
        self.forwarder.observe_leader_signal(status.signal());
        self.leader_status = Some(status);
    }

    fn confirm_leader_exit(&mut self) -> io::Result<bool> {
        loop {
            match self
                .leader
                .poll()
                .map_err(|error| contextual("observe Cargo leader after continue", &error))?
            {
                LeaderEvent::Exited(status) => {
                    self.record_leader_exit(status);
                    return Ok(true);
                }
                LeaderEvent::Stopped(_) | LeaderEvent::Continued => {}
                LeaderEvent::Running => return Ok(false),
            }
        }
    }
}

impl Drop for PlatformSupervisor {
    fn drop(&mut self) {
        if !self.complete {
            let _cleanup = self.terminate_and_wait();
        }
    }
}

fn cleanup_failed_spawn(
    child: &mut GroupChild,
    setup_error: io::Error,
) -> Result<PlatformSupervisor, SpawnError> {
    Err(SpawnError::after_child(
        setup_error,
        terminate_group_child(child),
    ))
}

fn terminate_group_child(child: &mut GroupChild) -> io::Result<()> {
    if let Err(kill_error) = child.kill()
        && kill_error.kind() != io::ErrorKind::InvalidInput
    {
        return match child.try_wait() {
            Ok(Some(_status)) => Ok(()),
            Ok(None) => Err(kill_error),
            Err(wait_error) => Err(io::Error::new(
                kill_error.kind(),
                format!("{kill_error}; failed to observe Cargo process group: {wait_error}"),
            )),
        };
    }
    child.wait().map(|_status| ())
}

fn group_alive(group: Pid) -> io::Result<bool> {
    match killpg(group, None) {
        Ok(()) | Err(Errno::EPERM) => Ok(true),
        Err(Errno::ESRCH) => Ok(false),
        Err(error) => Err(os_error(error)),
    }
}

fn signal_group(group: Pid, signal: Signal) -> io::Result<bool> {
    match killpg(group, signal) {
        Ok(()) => Ok(true),
        Err(Errno::ESRCH) => Ok(false),
        Err(error) => Err(os_error(error)),
    }
}

fn process_group(id: u32) -> io::Result<Pid> {
    i32::try_from(id)
        .map(Pid::from_raw)
        .map_err(|_| io::Error::other("Cargo process group identifier exceeds i32"))
}

fn os_error(error: Errno) -> io::Error {
    io::Error::from_raw_os_error(error as i32)
}

fn contextual(operation: &str, error: &io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{operation}: {error}"))
}
