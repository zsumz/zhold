use std::{fs, path::PathBuf};

use super::{CargoInvocation, ContextResolver, cargo::parse_version};

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

#[test]
fn project_config_resolves_relative_rustc_from_the_config_origin()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let cargo = temporary.path().join(".cargo");
    let compiler = temporary.path().join("tools/rustc");
    fs::create_dir_all(&cargo)?;
    fs::create_dir_all(compiler.parent().ok_or("compiler has no parent")?)?;
    fs::write(&compiler, b"fixture")?;
    fs::write(
        cargo.join("config.toml"),
        "[build]\nrustc = 'tools/rustc'\n",
    )?;
    let invocation = CargoInvocation::new(
        "cargo".to_owned(),
        vec!["check".to_owned()],
        temporary.path().to_path_buf(),
    )?;

    let configuration = super::config_identity::resolve(&invocation, &[3; 32])?;

    assert_eq!(
        configuration.rustc_program,
        compiler.canonicalize()?.to_str().ok_or("path")?
    );
    Ok(())
}

#[test]
fn explicit_config_file_uses_the_same_source_relative_path_rule()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let cargo = temporary.path().join(".cargo");
    let compiler = temporary.path().join("tools/rustc");
    fs::create_dir_all(&cargo)?;
    fs::create_dir_all(compiler.parent().ok_or("compiler has no parent")?)?;
    fs::write(&compiler, b"fixture")?;
    let config = cargo.join("explicit.toml");
    fs::write(&config, "[build]\nrustc = 'tools/rustc'\n")?;
    let invocation = CargoInvocation::new(
        "cargo".to_owned(),
        vec![
            "--config".to_owned(),
            config.display().to_string(),
            "check".to_owned(),
        ],
        temporary.path().to_path_buf(),
    )?;

    let configuration = super::config_identity::resolve(&invocation, &[3; 32])?;

    assert_eq!(
        configuration.rustc_program,
        compiler.canonicalize()?.to_str().ok_or("path")?
    );
    Ok(())
}

#[test]
fn recursive_includes_support_optional_files_and_detect_cycles()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let cargo = temporary.path().join(".cargo");
    let nested = cargo.join("nested");
    let compiler = cargo.join("tools/rustc");
    fs::create_dir_all(&nested)?;
    fs::create_dir_all(compiler.parent().ok_or("compiler has no parent")?)?;
    fs::write(&compiler, b"fixture")?;
    fs::write(
        cargo.join("top.toml"),
        "include = ['nested/one.toml', { path = 'missing.toml', optional = true }]\n",
    )?;
    fs::write(nested.join("one.toml"), "include = ['two.toml']\n")?;
    fs::write(nested.join("two.toml"), "[build]\nrustc = 'tools/rustc'\n")?;
    let invocation = explicit_config_invocation(temporary.path(), &cargo.join("top.toml"))?;

    let configuration = super::config_identity::resolve(&invocation, &[4; 32])?;
    assert_eq!(
        configuration.rustc_program,
        compiler.canonicalize()?.to_str().ok_or("path")?
    );

    fs::write(nested.join("two.toml"), "include = ['one.toml']\n")?;
    let error = super::config_identity::resolve(&invocation, &[4; 32])
        .err()
        .ok_or("include cycle unexpectedly resolved")?;
    assert!(error.to_string().contains("include cycle"));
    Ok(())
}

#[test]
fn failed_context_subprocesses_do_not_render_arbitrary_arguments()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let secret = "registries.private.token='extremely-secret-token'";
    let invocation = CargoInvocation::new(
        "cargo".to_owned(),
        vec![
            "--config".to_owned(),
            secret.to_owned(),
            "check".to_owned(),
            "--manifest-path".to_owned(),
            "missing/Cargo.toml".to_owned(),
        ],
        temporary.path().to_path_buf(),
    )?;

    let Err(error) = ContextResolver::resolve(&invocation, &[7; 32]) else {
        return Err("missing Cargo manifest unexpectedly resolved".into());
    };

    assert!(!error.to_string().contains(secret));
    assert!(error.to_string().contains("Cargo metadata query"));
    Ok(())
}

fn explicit_config_invocation(
    root: &std::path::Path,
    config: &std::path::Path,
) -> Result<CargoInvocation, crate::StoreError> {
    CargoInvocation::new(
        "cargo".to_owned(),
        vec![
            "--config".to_owned(),
            config.display().to_string(),
            "check".to_owned(),
        ],
        root.to_path_buf(),
    )
}
