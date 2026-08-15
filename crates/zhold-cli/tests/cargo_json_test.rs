//! Structured Cargo lifecycle output tests.

use std::{fs, io, path::Path, process::Command};

use tempfile::tempdir;
use zhold_store::Store;

#[derive(Clone, Copy, Debug)]
enum FixtureMutation {
    None,
    CorruptManifest,
    GrowArena,
    AddUncertainArena,
}

#[test]
fn cargo_json_events_are_filterable_from_shared_stderr() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    create_project(&project, FixtureMutation::None)?;

    let output = Command::new(env!("CARGO_BIN_EXE_zhold"))
        .arg("--store")
        .arg(&store)
        .args(["--format", "json", "--budget", "100GiB", "cargo", "check"])
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

    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["event"], "cargo_started");
    assert_eq!(events[1]["event"], "cargo_finished");
    assert_eq!(events[1]["exit_code"], 0);
    assert_eq!(events[2]["event"], "post_build_collection");
    Ok(())
}

#[test]
fn finalization_failure_after_cargo_success_is_a_management_error()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    create_project(&project, FixtureMutation::CorruptManifest)?;

    let output = Command::new(env!("CARGO_BIN_EXE_zhold"))
        .arg("--store")
        .arg(&store)
        .args(["--format", "json", "--budget", "100GiB", "cargo", "check"])
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

#[test]
fn post_build_collection_restores_the_steady_state_budget() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    create_project(&project, FixtureMutation::GrowArena)?;

    let output = managed_with_small_budget(&project, &store).output()?;

    assert!(output.status.success());
    let growth_path = fs::read_to_string(store.join("test-growth-path"))?;
    assert!(!Path::new(&growth_path).exists());
    let events = json_events(output.stderr)?;
    let post_build = events
        .iter()
        .find(|event| event["event"] == "post_build_collection")
        .ok_or_else(|| io::Error::other("missing post-build collection event"))?;
    assert_eq!(post_build["report"]["budget_met"], true);
    assert!(
        post_build["report"]["retirements"]
            .as_array()
            .is_some_and(|retirements| !retirements.is_empty()),
        "{post_build:#}"
    );
    assert!(Store::open(&store)?.inventory()?.arenas.is_empty());
    Ok(())
}

#[test]
fn post_build_collection_failure_after_cargo_success_is_a_management_error()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let project = temporary.path().join("project");
    let store = temporary.path().join("store");
    create_project(&project, FixtureMutation::AddUncertainArena)?;

    let output = managed_with_small_budget(&project, &store).output()?;

    assert_eq!(output.status.code(), Some(74));
    let events = json_events(output.stderr)?;
    let failure = events
        .iter()
        .find(|event| event["event"] == "cargo_management_failed")
        .ok_or_else(|| io::Error::other("missing management failure event"))?;
    assert_eq!(failure["stage"], "post_build_collection");
    assert_eq!(failure["cargo_exit_code"], 0);
    Ok(())
}

fn managed_with_small_budget(project: &Path, store: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zhold"));
    command
        .arg("--store")
        .arg(store)
        .args([
            "--format",
            "json",
            "--budget",
            "64KiB",
            "--build-reserve",
            "0B",
            "cargo",
            "check",
        ])
        .current_dir(project)
        .env("ZHOLD_TEST_STORE", store);
    command
}

fn json_events(stderr: Vec<u8>) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    Ok(String::from_utf8(stderr)?
        .lines()
        .filter(|line| line.starts_with("{\"event\":\""))
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()?)
}

fn create_project(root: &Path, mutation: FixtureMutation) -> Result<(), io::Error> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"json-fixture\"\nversion = \"0.0.1-rc.1\"\nedition = \"2024\"\n",
    )?;
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 5 }\n")?;
    match mutation {
        FixtureMutation::None => {}
        FixtureMutation::CorruptManifest => {
            fs::write(root.join("build.rs"), corrupting_build_script())?;
        }
        FixtureMutation::GrowArena => {
            fs::write(root.join("build.rs"), growing_build_script())?;
        }
        FixtureMutation::AddUncertainArena => {
            fs::write(root.join("build.rs"), uncertain_arena_build_script())?;
        }
    }
    git(root, &["init"])?;
    git(root, &["config", "user.email", "zhold@example.invalid"])?;
    git(root, &["config", "user.name", "zhold tests"])?;
    git(root, &["add", "."])?;
    git(root, &["commit", "-m", "fixture"])
}

fn growing_build_script() -> &'static str {
    "use std::{env, fs};\n\
     fn main() {\n\
         let Some(store) = env::var_os(\"ZHOLD_TEST_STORE\") else { return; };\n\
         let Ok(mut prefixes) = fs::read_dir(std::path::Path::new(&store).join(\"arenas\")) else { return; };\n\
         let Some(Ok(prefix)) = prefixes.next() else { return; };\n\
         let Ok(mut arenas) = fs::read_dir(prefix.path()) else { return; };\n\
         let Some(Ok(arena)) = arenas.next() else { return; };\n\
         let arena = arena.path();\n\
         let mut state = 0x9e3779b97f4a7c15_u64;\n\
         let mut data = Vec::with_capacity(2 * 1024 * 1024);\n\
         for _ in 0..data.capacity() {\n\
             state ^= state << 13; state ^= state >> 7; state ^= state << 17;\n\
             data.push(state as u8);\n\
         }\n\
         let growth = arena.join(\"test-growth\");\n\
         if fs::write(&growth, data).is_err() { return; }\n\
         let marker = std::path::Path::new(&store).join(\"test-growth-path\");\n\
         let _result = fs::write(marker, growth.to_string_lossy().as_bytes());\n\
     }\n"
}

fn uncertain_arena_build_script() -> &'static str {
    "use std::{env, fs, path::Path};\n\
     fn main() {\n\
         let Some(store) = env::var_os(\"ZHOLD_TEST_STORE\") else { return; };\n\
         let arenas = Path::new(&store).join(\"arenas\");\n\
         let id = \"ffffffffffffffffffffffffffffffff\";\n\
         let _result = fs::create_dir_all(arenas.join(\"ff\").join(id));\n\
     }\n"
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
