use std::path::PathBuf;

use super::{CargoInvocation, cargo::parse_version};

#[test]
fn cargo_invocation_accepts_a_path_to_cargo() {
    let invocation = CargoInvocation::new(
        "/opt/rust/bin/cargo".to_owned(),
        vec!["+nightly".to_owned(), "test".to_owned()],
        PathBuf::from("/tmp/project"),
    );

    assert!(invocation.is_ok());
}

#[test]
fn cargo_invocation_accepts_windows_executable_case_variants() {
    let invocation = CargoInvocation::new(
        "/opt/rust/bin/CARGO.EXE".to_owned(),
        vec!["check".to_owned()],
        PathBuf::from("/tmp/project"),
    );

    assert!(invocation.is_ok());
}

#[test]
fn cargo_invocation_rejects_other_commands() {
    let invocation = CargoInvocation::new(
        "rustc".to_owned(),
        Vec::new(),
        PathBuf::from("/tmp/project"),
    );

    assert!(invocation.is_err());
}

#[test]
fn cargo_release_is_parsed_from_standard_output() {
    assert_eq!(
        parse_version("cargo 1.91.0 (abc 2025-10-30)"),
        Some("1.91.0".to_owned())
    );
}
