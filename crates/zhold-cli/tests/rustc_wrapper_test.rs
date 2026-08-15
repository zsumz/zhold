//! Existing Rust compiler-wrapper compatibility tests.

use std::{fs, io, path::Path, process::Command};

use tempfile::tempdir;
use zhold_store::Store;

#[test]
fn managed_cargo_preserves_the_existing_rustc_wrapper() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    let source = temporary.path().join("wrapper.rs");
    let wrapper = temporary.path().join(wrapper_name());
    let log = temporary.path().join("wrapper-used");
    create_project(&project)?;
    fs::write(&source, wrapper_source())?;

    let compiled = Command::new("rustc")
        .arg(&source)
        .arg("-o")
        .arg(&wrapper)
        .status()?;
    assert!(compiled.success());

    let output = Command::new(env!("CARGO_BIN_EXE_zhold"))
        .arg("--store")
        .arg(&store)
        .args(["--budget", "100GiB"])
        .args(["cargo", "check"])
        .env("RUSTC_WRAPPER", &wrapper)
        .env("ZHOLD_TEST_WRAPPER_LOG", &log)
        .current_dir(&project)
        .output()?;

    assert!(
        output.status.success(),
        "managed Cargo failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(log.is_file());
    Ok(())
}

#[test]
fn caller_selected_rustc_receives_a_distinct_arena() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    let source = temporary.path().join("rustc-proxy.rs");
    let proxy = temporary.path().join(wrapper_name());
    create_project(&project)?;

    let baseline = zhold(&project, &store)
        .env_remove("RUSTC")
        .env_remove("CARGO_BUILD_RUSTC")
        .output()?;
    assert!(baseline.status.success());
    fs::write(&source, compiler_proxy_source())?;
    let compiled = Command::new("rustc")
        .arg(&source)
        .arg("-o")
        .arg(&proxy)
        .status()?;
    assert!(compiled.success());

    let selected = zhold(&project, &store)
        .env("RUSTC", &proxy)
        .env_remove("CARGO_BUILD_RUSTC")
        .env("ZHOLD_TEST_REAL_RUSTC", "rustc")
        .output()?;

    assert!(
        selected.status.success(),
        "managed Cargo failed: {}",
        String::from_utf8_lossy(&selected.stderr)
    );
    assert_eq!(Store::open(&store)?.inventory()?.arenas.len(), 2);

    let cargo_proxy = temporary
        .path()
        .join(format!("cargo-build-{}", wrapper_name()));
    fs::copy(&proxy, &cargo_proxy)?;
    let cargo_selected = zhold(&project, &store)
        .env_remove("RUSTC")
        .env("CARGO_BUILD_RUSTC", &cargo_proxy)
        .env("ZHOLD_TEST_REAL_RUSTC", "rustc")
        .output()?;
    assert!(cargo_selected.status.success());
    assert_eq!(Store::open(&store)?.inventory()?.arenas.len(), 3);
    Ok(())
}

fn zhold(project: &Path, store: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zhold"));
    command
        .arg("--store")
        .arg(store)
        .args(["--budget", "100GiB"])
        .args(["cargo", "check"])
        .current_dir(project);
    command
}

fn create_project(root: &Path) -> Result<(), io::Error> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"wrapper-fixture\"\nversion = \"0.1.0-alpha.1\"\nedition = \"2024\"\n",
    )?;
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 5 }\n")?;
    git(root, &["init"])?;
    git(root, &["config", "user.email", "zhold@example.invalid"])?;
    git(root, &["config", "user.name", "zhold tests"])?;
    git(root, &["add", "."])?;
    git(root, &["commit", "-m", "fixture"])
}

fn wrapper_name() -> &'static str {
    if cfg!(windows) {
        "rustc-wrapper.exe"
    } else {
        "rustc-wrapper"
    }
}

fn wrapper_source() -> &'static str {
    "use std::{env, fs, process::Command};\n\
     fn main() {\n\
         let mut arguments = env::args_os().skip(1);\n\
         let Some(rustc) = arguments.next() else {\n\
             std::process::exit(90);\n\
         };\n\
         let Some(log) = env::var_os(\"ZHOLD_TEST_WRAPPER_LOG\") else {\n\
             std::process::exit(91);\n\
         };\n\
         if fs::write(log, b\"used\").is_err() {\n\
             std::process::exit(92);\n\
         }\n\
         match Command::new(rustc).args(arguments).status() {\n\
             Ok(status) => std::process::exit(status.code().unwrap_or(1)),\n\
             Err(_) => std::process::exit(93),\n\
         }\n\
     }\n"
}

fn compiler_proxy_source() -> &'static str {
    "use std::{env, process::Command};\n\
     fn main() {\n\
         let Some(rustc) = env::var_os(\"ZHOLD_TEST_REAL_RUSTC\") else {\n\
             std::process::exit(94);\n\
         };\n\
         match Command::new(rustc).args(env::args_os().skip(1)).status() {\n\
             Ok(status) => std::process::exit(status.code().unwrap_or(1)),\n\
             Err(_) => std::process::exit(95),\n\
         }\n\
     }\n"
}

fn git(root: &Path, arguments: &[&str]) -> Result<(), io::Error> {
    let output = Command::new("git")
        .args(["-c", "commit.gpgsign=false"])
        .args(arguments)
        .current_dir(root)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}
