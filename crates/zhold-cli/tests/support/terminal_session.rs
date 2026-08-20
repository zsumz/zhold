use std::{
    io,
    io::{Read, Write},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use nix::{
    errno::Errno,
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use portable_pty::{Child, CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system};

const WAIT_STEP: Duration = Duration::from_millis(20);

pub(crate) struct PtySession {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    output: Receiver<Vec<u8>>,
    captured: Vec<u8>,
    session_group: Pid,
    managed_group: Option<Pid>,
    complete: bool,
}

impl PtySession {
    pub(crate) fn spawn(command: CommandBuilder) -> Result<Self, Box<dyn std::error::Error>> {
        let pair = native_pty_system().openpty(PtySize::default())?;
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let child = pair.slave.spawn_command(command)?;
        let child_id = child
            .process_id()
            .ok_or_else(|| io::Error::other("PTY child has no process identifier"))?;
        let session_group = process_id(child_id)?;
        drop(pair.slave);
        let (sender, output) = mpsc::channel();
        thread::spawn(move || copy_output(&mut reader, &sender));
        Ok(Self {
            child,
            master: pair.master,
            writer,
            output,
            captured: Vec::new(),
            session_group,
            managed_group: None,
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
        self.terminate();
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("PTY child did not exit: {:?}", self.text()),
        ))
    }

    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.captured).into_owned()
    }

    pub(crate) const fn session_group(&self) -> Pid {
        self.session_group
    }

    pub(crate) fn foreground_group(&self) -> Option<Pid> {
        self.master.process_group_leader().map(Pid::from_raw)
    }

    pub(crate) fn wait_for_foreground(
        &self,
        expected: Pid,
        timeout: Duration,
    ) -> Result<(), io::Error> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.foreground_group() == Some(expected) {
                return Ok(());
            }
            thread::sleep(WAIT_STEP);
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("PTY foreground did not become {expected}"),
        ))
    }

    pub(crate) fn track_group(&mut self, group: Pid) {
        self.managed_group = Some(group);
    }

    pub(crate) fn is_running(&mut self) -> Result<bool, io::Error> {
        Ok(self.child.try_wait()?.is_none())
    }

    fn drain_output(&mut self) {
        self.captured.extend(self.output.try_iter().flatten());
    }

    fn terminate(&mut self) {
        let foreground = self.foreground_group();
        for group in [self.managed_group, foreground, Some(self.session_group)]
            .into_iter()
            .flatten()
        {
            let _killed = killpg(group, Signal::SIGKILL);
        }
        let _killed = self.child.kill();
        let _waited = self.child.wait();
        self.complete = true;
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        if !self.complete {
            self.terminate();
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

fn process_id(id: u32) -> Result<Pid, io::Error> {
    i32::try_from(id)
        .map(Pid::from_raw)
        .map_err(|_| io::Error::other("PTY child process identifier exceeds i32"))
}

pub(crate) fn process_group_is_alive(group: Pid) -> Result<bool, io::Error> {
    match killpg(group, None) {
        Ok(()) | Err(Errno::EPERM) => Ok(true),
        Err(Errno::ESRCH) => Ok(false),
        Err(error) => Err(io::Error::from_raw_os_error(error as i32)),
    }
}
