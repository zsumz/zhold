use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError, sync_channel},
    },
    thread::{self, JoinHandle},
};

use nix::{
    errno::Errno,
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use signal_hook::{
    consts::signal::{SIGCONT, SIGHUP, SIGINT, SIGQUIT, SIGTERM},
    iterator::{Handle, Signals},
};

#[derive(Debug)]
pub(super) struct SignalForwarder {
    handle: Handle,
    worker: Option<JoinHandle<()>>,
    errors: Receiver<io::Error>,
    interrupted: Arc<AtomicBool>,
    continue_requested: Arc<AtomicBool>,
}

impl SignalForwarder {
    pub(super) fn spawn(mut signals: Signals, group: Pid) -> io::Result<Self> {
        let handle = signals.handle();
        let (errors, receiver) = sync_channel(1);
        let interrupted = Arc::new(AtomicBool::new(false));
        let continue_requested = Arc::new(AtomicBool::new(false));
        let worker_interrupted = Arc::clone(&interrupted);
        let worker_continue = Arc::clone(&continue_requested);
        let worker = thread::Builder::new()
            .name("zhold-signal-forwarder".to_owned())
            .spawn(move || {
                forward_signals(
                    &mut signals,
                    group,
                    &errors,
                    &worker_interrupted,
                    &worker_continue,
                );
            })?;
        Ok(Self {
            handle,
            worker: Some(worker),
            errors: receiver,
            interrupted,
            continue_requested,
        })
    }

    pub(super) fn check(&self) -> io::Result<()> {
        match self.errors.try_recv() {
            Ok(error) => Err(error),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => Ok(()),
        }
    }

    pub(super) fn stop(&mut self) -> io::Result<()> {
        self.handle.close();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("signal forwarder thread failed"))?;
        }
        self.check()
    }

    pub(super) fn was_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }

    pub(super) fn observe_leader_signal(&self, signal: Option<i32>) {
        if signal.is_some_and(is_forwarded_signal) {
            self.interrupted.store(true, Ordering::SeqCst);
        }
    }

    pub(super) fn take_continue_request(&self) -> bool {
        self.continue_requested.swap(false, Ordering::SeqCst)
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
    continue_requested: &AtomicBool,
) {
    for raw_signal in signals.forever() {
        if raw_signal == SIGCONT {
            continue_requested.store(true, Ordering::SeqCst);
            continue;
        }
        let signal = if interrupted.swap(true, Ordering::SeqCst) {
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

fn is_forwarded_signal(raw_signal: i32) -> bool {
    matches!(raw_signal, SIGINT | SIGTERM | SIGHUP | SIGQUIT)
}

fn forwarded_signal(raw_signal: i32) -> Signal {
    match raw_signal {
        SIGINT => Signal::SIGINT,
        SIGHUP => Signal::SIGHUP,
        SIGQUIT => Signal::SIGQUIT,
        _ => Signal::SIGTERM,
    }
}

fn os_error(error: Errno) -> io::Error {
    io::Error::from_raw_os_error(error as i32)
}
