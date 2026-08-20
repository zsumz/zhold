//! Unix controlling-terminal handoff and restoration qualification.

#![cfg(unix)]

use std::{thread, time::Duration};

use nix::sys::signal::{Signal, kill};
use zhold_core::{ArenaState, BuildOutcome, ByteSize};
use zhold_store::Store;

#[path = "support/terminal_fixture.rs"]
mod terminal_fixture;
#[path = "support/terminal_reclaim_fixture.rs"]
mod terminal_reclaim_fixture;

use terminal_fixture::{TerminalFixture, process_group_is_alive};
use terminal_reclaim_fixture::TerminalReclaimFixture;

const TIMEOUT: Duration = Duration::from_secs(90);

#[test]
fn interactive_program_reads_stdin_and_terminal_is_restored()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = TerminalFixture::create()?;
    let mut session = fixture.spawn("echo")?;

    session.wait_for("READY", TIMEOUT)?;
    session.write_line("hello-from-pty")?;
    session.wait_for("ECHO:hello-from-pty", TIMEOUT)?;
    session.wait_for("ZHOLD_STATUS:0", TIMEOUT)?;
    session.write_line("after-success")?;
    session.wait_for("RESTORED:after-success", TIMEOUT)?;

    let status = session.wait_for_exit(TIMEOUT)?;
    assert!(status.success(), "PTY driver failed: {:?}", session.text());
    Ok(())
}

#[test]
fn terminal_is_restored_after_cargo_failure() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = TerminalFixture::create()?;
    let mut session = fixture.spawn("fail")?;

    session.wait_for("READY", TIMEOUT)?;
    session.wait_for("ZHOLD_STATUS:23", TIMEOUT)?;
    session.write_line("after-failure")?;
    session.wait_for("RESTORED:after-failure", TIMEOUT)?;

    let status = session.wait_for_exit(TIMEOUT)?;
    assert!(status.success(), "PTY driver failed: {:?}", session.text());
    Ok(())
}

#[test]
fn ctrl_c_reaches_cargo_and_terminal_is_restored() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = TerminalFixture::create()?;
    let mut session = fixture.spawn("wait")?;

    session.wait_for("READY", TIMEOUT)?;
    session.write(&[3])?;
    session.wait_for("ZHOLD_STATUS:1", TIMEOUT)?;
    session.write_line("after-interrupt")?;
    session.wait_for("RESTORED:after-interrupt", TIMEOUT)?;

    let status = session.wait_for_exit(TIMEOUT)?;
    assert!(status.success(), "PTY driver failed: {:?}", session.text());
    Ok(())
}

#[test]
fn repeated_ctrl_c_forces_an_ignoring_descendant_and_restores_the_terminal()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = TerminalFixture::create()?;
    let mut session = fixture.spawn("ignore")?;

    session.wait_for("IGNORING_READY", TIMEOUT)?;
    let cargo_group = session
        .foreground_group()
        .ok_or("PTY has no foreground process group")?;
    let original_group = session.session_group();
    assert_ne!(cargo_group, original_group);
    session.track_group(cargo_group);
    session.wait_for_foreground(cargo_group, TIMEOUT)?;

    session.write(&[3])?;
    session.wait_for_foreground(original_group, Duration::from_secs(10))?;
    thread::sleep(Duration::from_millis(100));
    assert!(process_group_is_alive(cargo_group)?);
    assert!(session.is_running()?);
    let active = Store::open(fixture.store())?.inventory()?;
    assert_eq!(active.arenas[0].record.state(), ArenaState::Active);
    assert!(active.reserved > ByteSize::ZERO);
    assert_eq!(active.arenas[0].record.last_outcome, None);

    session.write(&[3])?;
    session.wait_for("ZHOLD_STATUS:1", Duration::from_secs(20))?;
    assert!(!process_group_is_alive(cargo_group)?);
    assert_eq!(session.foreground_group(), Some(original_group));
    session.write_line("after-repeated-interrupt")?;
    session.wait_for("RESTORED:after-repeated-interrupt", TIMEOUT)?;

    let status = session.wait_for_exit(TIMEOUT)?;
    assert!(status.success(), "PTY driver failed: {:?}", session.text());
    let finished = Store::open(fixture.store())?.inventory()?;
    assert_eq!(finished.arenas[0].record.state(), ArenaState::Idle);
    assert_eq!(finished.reserved, ByteSize::ZERO);
    assert_eq!(
        finished.arenas[0].record.last_outcome,
        Some(BuildOutcome::Terminated)
    );
    Ok(())
}

#[test]
fn terminal_restore_does_not_overwrite_a_new_foreground_owner()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = TerminalReclaimFixture::create()?;
    let mut session = fixture.spawn()?;

    session.wait_for("READY", TIMEOUT)?;
    let cargo_group = session
        .foreground_group()
        .ok_or("PTY has no foreground process group")?;
    session.track_group(cargo_group);
    let front = fixture.front_pid(TIMEOUT)?;
    let driver_group = session.session_group();
    assert_ne!(front, driver_group);
    assert_ne!(cargo_group, driver_group);
    assert_ne!(cargo_group, front);
    kill(front, Signal::SIGKILL)?;

    fixture.reclaimed(Duration::from_secs(10))?;
    session.wait_for_foreground(driver_group, Duration::from_secs(10))?;
    assert!(process_group_is_alive(front)?);
    assert!(process_group_is_alive(cargo_group)?);
    let active = Store::open(fixture.store())?.inventory()?;
    assert_eq!(active.arenas[0].record.state(), ArenaState::Active);
    assert!(active.reserved > ByteSize::ZERO);
    fixture.release_build()?;
    session.wait_for("zhold  succeeded (exit 0)", TIMEOUT)?;

    let inventory = Store::open(fixture.store())?.inventory()?;
    assert_eq!(inventory.arenas[0].record.state(), ArenaState::Idle);
    assert_eq!(inventory.reserved, ByteSize::ZERO);
    assert_eq!(
        inventory.arenas[0].record.last_outcome,
        Some(BuildOutcome::Succeeded)
    );
    assert_eq!(session.foreground_group(), Some(driver_group));

    fixture.release_driver()?;
    let status = session.wait_for_exit(TIMEOUT)?;
    assert!(status.success(), "PTY driver failed: {:?}", session.text());
    Ok(())
}

#[test]
fn terminal_reclaim_driver() -> Result<(), Box<dyn std::error::Error>> {
    terminal_reclaim_fixture::run_driver_if_requested()
}
