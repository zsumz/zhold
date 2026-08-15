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

#[test]
fn metadata_discovery_preserves_context_affecting_options() -> Result<(), Box<dyn std::error::Error>>
{
    let invocation = CargoInvocation::new(
        "cargo".to_owned(),
        vec![
            "+nightly".to_owned(),
            "-Zunstable-options".to_owned(),
            "-C".to_owned(),
            "services/api".to_owned(),
            "test".to_owned(),
            "--config".to_owned(),
            "net.offline=true".to_owned(),
            "--manifest-path=member/Cargo.toml".to_owned(),
            "--locked".to_owned(),
        ],
        PathBuf::from("/tmp/project"),
    )?;

    assert_eq!(
        invocation.metadata_arguments()?,
        vec![
            "+nightly".to_owned(),
            "-Zunstable-options".to_owned(),
            "-C".to_owned(),
            "services/api".to_owned(),
            "--config".to_owned(),
            "net.offline=true".to_owned(),
            "metadata".to_owned(),
            "--no-deps".to_owned(),
            "--format-version".to_owned(),
            "1".to_owned(),
            "--manifest-path".to_owned(),
            "member/Cargo.toml".to_owned(),
            "--locked".to_owned(),
        ]
    );
    Ok(())
}

#[test]
fn metadata_discovery_rejects_missing_option_values() -> Result<(), Box<dyn std::error::Error>> {
    let invocation = CargoInvocation::new(
        "cargo".to_owned(),
        vec!["test".to_owned(), "--manifest-path".to_owned()],
        PathBuf::from("/tmp/project"),
    )?;

    assert!(invocation.metadata_arguments().is_err());
    Ok(())
}
