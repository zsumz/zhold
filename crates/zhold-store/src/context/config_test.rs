use std::fs;

use super::CargoInvocation;

#[test]
fn fingerprints_are_store_keyed_and_include_sensitive() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let cargo = temporary.path().join(".cargo");
    fs::create_dir_all(&cargo)?;
    fs::write(cargo.join("top.toml"), "include = ['shared.toml']\n")?;
    fs::write(cargo.join("shared.toml"), "[build]\ntarget = 'first'\n")?;
    let invocation = explicit_invocation(temporary.path(), &cargo.join("top.toml"))?;

    let first = super::config_identity::resolve(&invocation, &[5; 32])?;
    let other_store = super::config_identity::resolve(&invocation, &[6; 32])?;
    fs::write(cargo.join("shared.toml"), "[build]\ntarget = 'second'\n")?;
    let changed = super::config_identity::resolve(&invocation, &[5; 32])?;

    assert_ne!(first.fingerprint, other_store.fingerprint);
    assert_ne!(first.fingerprint, changed.fingerprint);
    Ok(())
}

#[test]
fn inline_configuration_include_is_an_explicit_compatibility_error()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let invocation = CargoInvocation::new(
        "cargo".to_owned(),
        vec![
            "--config".to_owned(),
            "include=['compiler.toml']".to_owned(),
            "check".to_owned(),
        ],
        temporary.path().to_path_buf(),
    )?;
    let Some(error) = super::config_identity::resolve(&invocation, &[4; 32]).err() else {
        return Err("inline include unexpectedly resolved".into());
    };

    assert!(
        error
            .to_string()
            .contains("Cargo configuration include is supported only from configuration files")
    );
    Ok(())
}

fn explicit_invocation(
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
