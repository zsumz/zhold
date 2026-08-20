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
    wrapper_group: Pid,
    cargo_group: Pid,
    active: bool,
}

impl ForegroundTerminal {
    pub(super) fn hand_off(cargo_group: Pid) -> io::Result<Self> {
        let Some(current_group) = terminal_group()? else {
            return Ok(Self::detached());
        };
        let wrapper_group = getpgrp();
        let mut terminal = Self {
            handoff: Some(Handoff {
                wrapper_group,
                cargo_group,
                active: false,
            }),
        };
        if current_group != wrapper_group {
            return Ok(terminal);
        }
        set_foreground(cargo_group)?;
        terminal.handoff = Some(Handoff {
            wrapper_group,
            cargo_group,
            active: true,
        });
        if let Err(error) = killpg(cargo_group, Signal::SIGCONT)
            && error != Errno::ESRCH
        {
            let _restored = terminal.restore();
            return Err(os_error(error));
        }
        Ok(terminal)
    }

    pub(super) fn wrapper_is_foreground(&self) -> io::Result<bool> {
        let Some(handoff) = self.handoff else {
            return Ok(false);
        };
        Ok(terminal_group()? == Some(handoff.wrapper_group))
    }

    pub(super) fn reclaim_for_stop(&mut self) -> io::Result<()> {
        let Some(mut handoff) = self.handoff else {
            return Ok(());
        };
        if handoff.active && terminal_group()? == Some(handoff.cargo_group) {
            set_foreground(handoff.wrapper_group)?;
        }
        handoff.active = false;
        self.handoff = Some(handoff);
        Ok(())
    }

    pub(super) fn resume_for_continue(&mut self) -> io::Result<()> {
        let Some(mut handoff) = self.handoff else {
            return Ok(());
        };
        match terminal_group()? {
            Some(current) if current == handoff.wrapper_group => {
                set_foreground(handoff.cargo_group)?;
                handoff.active = true;
            }
            Some(current) if current == handoff.cargo_group && handoff.active => {}
            _ => handoff.active = false,
        }
        self.handoff = Some(handoff);
        Ok(())
    }

    pub(super) fn restore(&mut self) -> io::Result<()> {
        let Some(handoff) = self.handoff else {
            return Ok(());
        };
        if handoff.active && terminal_group()? == Some(handoff.cargo_group) {
            set_foreground(handoff.wrapper_group)?;
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
