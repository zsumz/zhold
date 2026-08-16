use std::io::{self, IsTerminal};

use nix::{
    errno::Errno,
    sys::signal::{SigSet, SigmaskHow, Signal, killpg},
    unistd::{Pid, getpgrp, tcgetpgrp, tcsetpgrp},
};

const STANDARD_INPUT: i32 = 0;

#[derive(Debug)]
pub(super) struct ForegroundTerminal {
    original_group: Option<Pid>,
}

impl ForegroundTerminal {
    pub(super) fn hand_off(cargo_group: Pid) -> io::Result<Self> {
        let Some(original_group) = foreground_group()? else {
            return Ok(Self::detached());
        };
        set_foreground(cargo_group)?;
        if let Err(error) = killpg(cargo_group, Signal::SIGCONT)
            && error != Errno::ESRCH
        {
            let _restored = set_foreground(original_group);
            return Err(os_error(error));
        }
        Ok(Self {
            original_group: Some(original_group),
        })
    }

    pub(super) fn restore(&mut self) -> io::Result<()> {
        let Some(original_group) = self.original_group else {
            return Ok(());
        };
        set_foreground(original_group)?;
        self.original_group = None;
        Ok(())
    }

    const fn detached() -> Self {
        Self {
            original_group: None,
        }
    }
}

impl Drop for ForegroundTerminal {
    fn drop(&mut self) {
        let _restored = self.restore();
    }
}

fn foreground_group() -> io::Result<Option<Pid>> {
    if !io::stdin().is_terminal() {
        return Ok(None);
    }
    let foreground = match tcgetpgrp(STANDARD_INPUT) {
        Ok(group) => group,
        Err(Errno::ENOTTY) => return Ok(None),
        Err(error) => return Err(os_error(error)),
    };
    Ok((foreground == getpgrp()).then_some(foreground))
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
