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

#[test]
fn managed_build_directory_has_final_configuration_precedence()
-> Result<(), Box<dyn std::error::Error>> {
    let invocation = CargoInvocation::new(
        "cargo".to_owned(),
        vec![
            "run".to_owned(),
            "--config".to_owned(),
            "build.build-dir='caller'".to_owned(),
            "--".to_owned(),
            "--config".to_owned(),
            "application-value".to_owned(),
        ],
        PathBuf::from("/tmp/project"),
    )?;

    let arguments = invocation.managed_arguments(&PathBuf::from("/owned/build"))?;

    assert_eq!(
        arguments,
        vec![
            "run",
            "--config",
            "build.build-dir='caller'",
            "--config",
            "build.build-dir=\"/owned/build\"",
            "--",
            "--config",
            "application-value",
        ]
    );
    Ok(())
}

#[test]
fn repeated_change_directory_options_compose_in_order() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let nested = temporary.path().join("one/two");
    std::fs::create_dir_all(&nested)?;
    let invocation = CargoInvocation::new(
        "cargo".to_owned(),
        vec![
            "-C".to_owned(),
            "one".to_owned(),
            "-Ctwo".to_owned(),
            "check".to_owned(),
        ],
        temporary.path().to_path_buf(),
    )?;

    assert_eq!(
        invocation.effective_working_directory()?,
        nested.canonicalize()?
    );
    Ok(())
}
