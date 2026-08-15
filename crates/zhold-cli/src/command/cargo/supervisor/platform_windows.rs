use std::{
    io,
    process::{Command, ExitStatus},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use command_group::{CommandGroup, GroupChild};
use signal_hook::{
    SigId,
    consts::{SIGBREAK, SIGINT},
    flag,
};

#[derive(Debug)]
pub(in crate::command::cargo) struct PlatformSupervisor {
    child: GroupChild,
    signal_ids: Vec<SigId>,
    interrupted: Arc<AtomicBool>,
    was_interrupted: bool,
}

impl PlatformSupervisor {
    pub(in crate::command::cargo) fn spawn(
        command: &mut Command,
        spawned: impl FnOnce(),
    ) -> io::Result<Self> {
        let interrupted = Arc::new(AtomicBool::new(false));
        let signal_ids = register_handlers(&interrupted)?;
        let child = match command.group().kill_on_drop(true).spawn() {
            Ok(child) => child,
            Err(error) => {
                unregister_handlers(signal_ids);
                return Err(error);
            }
        };
        spawned();
        Ok(Self {
            child,
            signal_ids,
            interrupted,
            was_interrupted: false,
        })
    }

    pub(in crate::command::cargo) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.interrupted.swap(false, Ordering::SeqCst) {
            self.was_interrupted = true;
            return self.kill_and_wait().map(Some);
        }
        let status = self.child.try_wait()?;
        if status.is_some() {
            self.clear_handlers();
        }
        Ok(status)
    }

    pub(in crate::command::cargo) fn terminate_and_wait(&mut self) -> io::Result<()> {
        self.kill_and_wait().map(|_status| ())
    }

    pub(in crate::command::cargo) const fn was_interrupted(&self) -> bool {
        self.was_interrupted
    }

    fn kill_and_wait(&mut self) -> io::Result<ExitStatus> {
        if let Err(error) = self.child.kill()
            && error.kind() != io::ErrorKind::InvalidInput
        {
            return Err(error);
        }
        let status = self.child.wait();
        self.clear_handlers();
        status
    }

    fn clear_handlers(&mut self) {
        unregister_handlers(std::mem::take(&mut self.signal_ids));
    }
}

impl Drop for PlatformSupervisor {
    fn drop(&mut self) {
        self.clear_handlers();
    }
}

fn register_handlers(interrupted: &Arc<AtomicBool>) -> io::Result<Vec<SigId>> {
    let mut signal_ids = Vec::new();
    for signal in [SIGINT, SIGBREAK] {
        match flag::register(signal, Arc::clone(interrupted)) {
            Ok(id) => signal_ids.push(id),
            Err(error) => {
                unregister_handlers(signal_ids);
                return Err(error);
            }
        }
    }
    Ok(signal_ids)
}

fn unregister_handlers(signal_ids: Vec<SigId>) {
    for signal_id in signal_ids {
        let _removed = signal_hook::low_level::unregister(signal_id);
    }
}
