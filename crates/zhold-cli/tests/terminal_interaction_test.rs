//! Unix controlling-terminal handoff and restoration qualification.

#![cfg(unix)]

use std::time::Duration;

#[path = "support/terminal_fixture.rs"]
mod terminal_fixture;

use terminal_fixture::TerminalFixture;

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
