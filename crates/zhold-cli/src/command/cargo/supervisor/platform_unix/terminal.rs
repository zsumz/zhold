use std::io::{self, IsTerminal};

use nix::{
    errno::Errno,
    sys::signal::{SigSet, SigmaskHow, Signal, killpg},
    unistd::{Pid, getpgrp, tcgetpgrp, tcsetpgrp},
};

const STANDARD_INPUT: i32 = 0;

#[derive(Debug)]
pub(super) struct ForegroundTerminal {
    handoff: Option<Handoff>,
}

#[derive(Clone, Copy, Debug)]
struct Handoff {
    original_group: Pid,
    handed_off_group: Pid,
}

impl ForegroundTerminal {
    pub(super) fn hand_off(cargo_group: Pid) -> io::Result<Self> {
        let Some(original_group) = foreground_group()? else {
            return Ok(Self::detached());
        };
        set_foreground(cargo_group)?;
        let mut terminal = Self {
            handoff: Some(Handoff {
                original_group,
                handed_off_group: cargo_group,
            }),
        };
        if let Err(error) = killpg(cargo_group, Signal::SIGCONT)
            && error != Errno::ESRCH
        {
            let _restored = terminal.restore();
            return Err(os_error(error));
        }
        Ok(terminal)
    }

    pub(super) fn restore(&mut self) -> io::Result<()> {
        let Some(handoff) = self.handoff else {
            return Ok(());
        };
        let Some(current_group) = terminal_group()? else {
            self.handoff = None;
            return Ok(());
        };
        if current_group == handoff.handed_off_group {
            set_foreground(handoff.original_group)?;
        }
        self.handoff = None;
        Ok(())
    }

    const fn detached() -> Self {
        Self { handoff: None }
    }
}

impl Drop for ForegroundTerminal {
    fn drop(&mut self) {
        let _restored = self.restore();
    }
}

fn foreground_group() -> io::Result<Option<Pid>> {
    let foreground = terminal_group()?;
    Ok(foreground.filter(|group| *group == getpgrp()))
}

fn terminal_group() -> io::Result<Option<Pid>> {
    if !io::stdin().is_terminal() {
        return Ok(None);
    }
    let foreground = match tcgetpgrp(STANDARD_INPUT) {
        Ok(group) => group,
        Err(Errno::ENOTTY) => return Ok(None),
        Err(error) => return Err(os_error(error)),
    };
    Ok(Some(foreground))
}

fn set_foreground(group: Pid) -> io::Result<()> {
    let mut blocked = SigSet::empty();
    blocked.add(Signal::SIGTTOU);
    let previous = blocked
        .thread_swap_mask(SigmaskHow::SIG_BLOCK)
        .map_err(os_error)?;
    let changed = tcsetpgrp(STANDARD_INPUT, group).map_err(os_error);
    let restored = previous.thread_set_mask().map_err(os_error);
    match (changed, restored) {
        (Err(change), Err(mask)) => Err(io::Error::new(
            change.kind(),
            format!("{change}; failed to restore the terminal signal mask: {mask}"),
        )),
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn os_error(error: Errno) -> io::Error {
    io::Error::from_raw_os_error(error as i32)
}
