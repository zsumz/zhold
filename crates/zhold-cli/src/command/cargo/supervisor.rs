use std::{io, process::Command};

#[cfg(unix)]
mod platform {
    use std::{
        io,
        process::{Command, ExitStatus},
        sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel},
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

    #[derive(Debug)]
    pub(super) struct PlatformSupervisor {
        child: GroupChild,
        group: Pid,
        leader_status: Option<ExitStatus>,
        forwarder: SignalForwarder,
    }

    impl PlatformSupervisor {
        pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
            let signals = Signals::new([SIGINT, SIGTERM, SIGHUP, SIGQUIT])?;
            let mut child = command.group_spawn()?;
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
            })
        }

        pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            self.forwarder.check()?;
            if self.leader_status.is_none() {
                self.leader_status = self.child.try_wait()?;
            }
            if self.leader_status.is_some() && !group_alive(self.group)? {
                self.forwarder.stop()?;
                return Ok(self.leader_status);
            }
            Ok(None)
        }

        pub(super) fn terminate_and_wait(&mut self) -> io::Result<()> {
            if let Err(error) = killpg(self.group, Signal::SIGKILL)
                && error != Errno::ESRCH
            {
                return Err(os_error(error));
            }
            self.leader_status = self.child.wait().ok().or(self.leader_status);
            while group_alive(self.group)? {
                thread::sleep(std::time::Duration::from_millis(10));
            }
            self.forwarder.stop()
        }
    }

    fn cleanup_failed_spawn(
        child: &mut GroupChild,
        setup_error: io::Error,
    ) -> io::Result<PlatformSupervisor> {
        let _kill = child.kill();
        let _wait = child.wait();
        Err(setup_error)
    }

    #[derive(Debug)]
    struct SignalForwarder {
        handle: Handle,
        worker: Option<JoinHandle<()>>,
        errors: Receiver<io::Error>,
    }

    impl SignalForwarder {
        fn spawn(mut signals: Signals, group: Pid) -> io::Result<Self> {
            let handle = signals.handle();
            let (errors, receiver) = sync_channel(1);
            let worker = thread::Builder::new()
                .name("zhold-signal-forwarder".to_owned())
                .spawn(move || forward_signals(&mut signals, group, &errors))?;
            Ok(Self {
                handle,
                worker: Some(worker),
                errors: receiver,
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
    }

    impl Drop for SignalForwarder {
        fn drop(&mut self) {
            self.handle.close();
        }
    }

    fn forward_signals(signals: &mut Signals, group: Pid, errors: &SyncSender<io::Error>) {
        let mut received = 0_u32;
        for raw_signal in signals.forever() {
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
}

#[cfg(windows)]
mod platform {
    use std::{
        io,
        process::{Command, ExitStatus},
        sync::{
            Arc,
            atomic::AtomicBool,
            mpsc::{Receiver, TryRecvError, sync_channel},
        },
        thread::{self, JoinHandle},
    };

    use command_group::CommandGroup;
    use signal_hook::{
        SigId,
        consts::{SIGBREAK, SIGINT},
        flag,
    };

    #[derive(Debug)]
    pub(super) struct PlatformSupervisor {
        results: Receiver<io::Result<ExitStatus>>,
        worker: Option<JoinHandle<()>>,
        signal_ids: Vec<SigId>,
        _interrupted: Arc<AtomicBool>,
    }

    impl PlatformSupervisor {
        pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
            let interrupted = Arc::new(AtomicBool::new(false));
            let signal_ids = [SIGINT, SIGBREAK]
                .into_iter()
                .map(|signal| flag::register(signal, Arc::clone(&interrupted)))
                .collect::<Result<Vec<_>, _>>()?;
            let mut child = command.group().kill_on_drop(true).spawn()?;
            let (sender, results) = sync_channel(1);
            let worker = thread::Builder::new()
                .name("zhold-job-waiter".to_owned())
                .spawn(move || {
                    let _result = sender.send(child.wait());
                })?;
            Ok(Self {
                results,
                worker: Some(worker),
                signal_ids,
                _interrupted: interrupted,
            })
        }

        pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            match self.results.try_recv() {
                Ok(result) => {
                    self.finish_worker()?;
                    result.map(Some)
                }
                Err(TryRecvError::Empty) => Ok(None),
                Err(TryRecvError::Disconnected) => {
                    self.finish_worker()?;
                    Err(io::Error::other("Windows Job Object waiter disconnected"))
                }
            }
        }

        pub(super) fn terminate_and_wait(&mut self) -> io::Result<()> {
            while self.try_wait()?.is_none() {
                thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(())
        }

        fn finish_worker(&mut self) -> io::Result<()> {
            if let Some(worker) = self.worker.take() {
                worker
                    .join()
                    .map_err(|_| io::Error::other("Windows Job Object waiter failed"))?;
            }
            for signal_id in self.signal_ids.drain(..) {
                let _removed = signal_hook::low_level::unregister(signal_id);
            }
            Ok(())
        }
    }
}

/// Platform-owned Cargo process tree with delayed completion until descendants exit.
#[derive(Debug)]
pub(super) struct CargoSupervisor {
    platform: platform::PlatformSupervisor,
}

impl CargoSupervisor {
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        platform::PlatformSupervisor::spawn(command).map(|platform| Self { platform })
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        self.platform.try_wait()
    }

    pub(super) fn terminate_and_wait(&mut self) -> io::Result<()> {
        self.platform.terminate_and_wait()
    }
}
