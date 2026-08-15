//! Black-box proof that Cargo writes only into the arena zhold leased.

use std::{fs, io, path::Path, process::Command};

use tempfile::tempdir;
use zhold_store::Store;

#[test]
fn managed_directory_overrides_every_cargo_configuration_source()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    let project_override = temporary.path().join("project-override");
    let file_override = temporary.path().join("file-override");
    let inline_override = temporary.path().join("inline-override");
    create_project(&project)?;
    fs::create_dir_all(project.join(".cargo"))?;
    fs::write(
        project.join(".cargo/config.toml"),
        format!("[build]\nbuild-dir = {project_override:?}\n"),
    )?;
    let extra = temporary.path().join("extra.toml");
    fs::write(&extra, format!("[build]\nbuild-dir = {file_override:?}\n"))?;
    let inline = format!("build.build-dir={inline_override:?}");

    let output = Command::new(env!("CARGO_BIN_EXE_zhold"))
        .arg("--store")
        .arg(&store)
        .args(["--budget", "100GiB"])
        .args(["cargo", "check", "--config"])
        .arg(&extra)
        .args(["--config", &inline])
        .current_dir(&project)
        .output()?;

    assert!(
        output.status.success(),
        "managed Cargo failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!project_override.exists());
    assert!(!file_override.exists());
    assert!(!inline_override.exists());
    let inventory = Store::open(&store)?.inventory()?;
    assert_eq!(inventory.arenas.len(), 1);
    assert!(inventory.arenas[0].record.build_dir.is_dir());
    Ok(())
}

#[test]
fn inherited_build_directory_is_rejected_before_cargo_starts()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    let caller = temporary.path().join("caller-build");
    create_project(&project)?;

    let output = Command::new(env!("CARGO_BIN_EXE_zhold"))
        .arg("--store")
        .arg(&store)
        .args(["--budget", "100GiB"])
        .args(["cargo", "check"])
        .env("CARGO_BUILD_BUILD_DIR", &caller)
        .current_dir(&project)
        .output()?;

    assert!(!output.status.success());
    assert!(!caller.exists());
    assert!(String::from_utf8(output.stderr)?.contains("refusing to replace"));
    Ok(())
}

fn create_project(root: &Path) -> Result<(), io::Error> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='build-directory-fixture'\nversion='0.0.1'\nedition='2024'\n",
    )?;
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 7 }\n")?;
    let output = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(root)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other("git init failed for test fixture"))
    }
}
