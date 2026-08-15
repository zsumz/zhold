//! Structured Cargo lifecycle output tests.

use std::{fs, io, path::Path, process::Command};

use tempfile::tempdir;

#[test]
fn cargo_json_events_are_filterable_from_shared_stderr() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    create_project(&project, false)?;

    let output = Command::new(env!("CARGO_BIN_EXE_zhold"))
        .arg("--store")
        .arg(&store)
        .args(["--format", "json", "cargo", "check"])
        .current_dir(&project)
        .output()?;

    assert!(
        output.status.success(),
        "managed Cargo failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr)?;
    let events = stderr
        .lines()
        .filter(|line| line.starts_with("{\"event\":\""))
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["event"], "cargo_started");
    assert_eq!(events[1]["event"], "cargo_finished");
    assert_eq!(events[1]["exit_code"], 0);
    Ok(())
}

#[test]
fn finalization_failure_after_cargo_success_is_a_management_error()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    create_project(&project, true)?;

    let output = Command::new(env!("CARGO_BIN_EXE_zhold"))
        .arg("--store")
        .arg(&store)
        .args(["--format", "json", "cargo", "check"])
        .current_dir(&project)
        .output()?;

    assert_eq!(output.status.code(), Some(74));
    let stderr = String::from_utf8(output.stderr)?;
    let events = stderr
        .lines()
        .filter(|line| line.starts_with("{\"event\":\""))
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(events.len(), 2);
    assert_eq!(events[1]["event"], "cargo_finalization_failed");
    assert_eq!(events[1]["exit_code"], 0);
    Ok(())
}

fn create_project(root: &Path, corrupt_manifest: bool) -> Result<(), io::Error> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"json-fixture\"\nversion = \"0.0.1-rc.1\"\nedition = \"2024\"\n",
    )?;
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 5 }\n")?;
    if corrupt_manifest {
        fs::write(root.join("build.rs"), corrupting_build_script())?;
    }
    git(root, &["init"])?;
    git(root, &["config", "user.email", "zhold@example.invalid"])?;
    git(root, &["config", "user.name", "zhold tests"])?;
    git(root, &["add", "."])?;
    git(root, &["commit", "-m", "fixture"])
}

fn corrupting_build_script() -> &'static str {
    "use std::{env, fs, path::Path};\n\
     fn main() {\n\
         let Some(build) = env::var_os(\"CARGO_BUILD_BUILD_DIR\") else {\n\
             std::process::exit(24);\n\
         };\n\
         let Some(arena) = Path::new(&build).parent() else {\n\
             std::process::exit(25);\n\
         };\n\
         if fs::write(arena.join(\"arena.json\"), b\"corrupt\").is_err() {\n\
             std::process::exit(26);\n\
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
