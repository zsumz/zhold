use std::{
    io,
    process::{Command, ExitStatus},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError, sync_channel},
    },
    thread::{self, JoinHandle},
};

use command_group::{CommandGroup, GroupChild};
use nix::{
    errno::Errno,
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use signal_hook::{
    consts::signal::{SIGHUP, SIGINT, SIGQUIT, SIGTERM},
    iterator::{Handle, Signals},
};

use super::SpawnError;

#[derive(Debug)]
pub(in crate::command::cargo) struct PlatformSupervisor {
    child: GroupChild,
    group: Pid,
    leader_status: Option<ExitStatus>,
    forwarder: SignalForwarder,
    complete: bool,
}

impl PlatformSupervisor {
    pub(in crate::command::cargo) fn spawn(
        command: &mut Command,
        spawned: impl FnOnce() -> io::Result<()>,
    ) -> Result<Self, SpawnError> {
        let signals =
            Signals::new([SIGINT, SIGTERM, SIGHUP, SIGQUIT]).map_err(SpawnError::before_child)?;
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
        Ok(Self {
            child,
            group,
            leader_status: None,
            forwarder,
            complete: false,
        })
    }

    pub(in crate::command::cargo) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.forwarder.check()?;
        if self.leader_status.is_none() {
            self.leader_status = self.child.try_wait()?;
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
        self.leader_status = self.child.wait().ok().or(self.leader_status);
        while group_alive(self.group)? {
            thread::sleep(std::time::Duration::from_millis(10));
        }
        self.complete = true;
        let _forwarder = self.forwarder.stop();
        Ok(())
    }

    pub(in crate::command::cargo) fn was_interrupted(&self) -> bool {
        self.forwarder.was_interrupted()
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

#[derive(Debug)]
struct SignalForwarder {
    handle: Handle,
    worker: Option<JoinHandle<()>>,
    errors: Receiver<io::Error>,
    interrupted: Arc<AtomicBool>,
}

impl SignalForwarder {
    fn spawn(mut signals: Signals, group: Pid) -> io::Result<Self> {
        let handle = signals.handle();
        let (errors, receiver) = sync_channel(1);
        let interrupted = Arc::new(AtomicBool::new(false));
        let worker_interrupted = Arc::clone(&interrupted);
        let worker = thread::Builder::new()
            .name("zhold-signal-forwarder".to_owned())
            .spawn(move || {
                forward_signals(&mut signals, group, &errors, &worker_interrupted);
            })?;
        Ok(Self {
            handle,
            worker: Some(worker),
            errors: receiver,
            interrupted,
        })
    }

    fn check(&self) -> io::Result<()> {
        match self.errors.try_recv() {
            Ok(error) => Err(error),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => Ok(()),
        }
    }

    fn stop(&mut self) -> io::Result<()> {
        self.handle.close();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("signal forwarder thread failed"))?;
        }
        self.check()
    }

    fn was_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }
}

impl Drop for SignalForwarder {
    fn drop(&mut self) {
        self.handle.close();
    }
}

fn forward_signals(
    signals: &mut Signals,
    group: Pid,
    errors: &SyncSender<io::Error>,
    interrupted: &AtomicBool,
) {
    let mut received = 0_u32;
    for raw_signal in signals.forever() {
        interrupted.store(true, Ordering::SeqCst);
        received = received.saturating_add(1);
        let signal = if received > 1 {
            Signal::SIGKILL
        } else {
            forwarded_signal(raw_signal)
        };
        if let Err(error) = killpg(group, signal)
            && error != Errno::ESRCH
        {
            let _result = errors.try_send(os_error(error));
            return;
        }
    }
}

fn forwarded_signal(raw_signal: i32) -> Signal {
    match raw_signal {
        SIGINT => Signal::SIGINT,
        SIGHUP => Signal::SIGHUP,
        SIGQUIT => Signal::SIGQUIT,
        _ => Signal::SIGTERM,
    }
}

fn group_alive(group: Pid) -> io::Result<bool> {
    match killpg(group, None) {
        Ok(()) | Err(Errno::EPERM) => Ok(true),
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
