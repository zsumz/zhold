use std::{fs, io, path::Path, process::Command};

use tempfile::tempdir;

use super::{CargoInvocation, ContextResolver};

#[test]
fn physical_git_worktrees_receive_distinct_arenas_in_one_repository()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let main = temporary.path().join("main");
    let agent = temporary.path().join("agent");
    fs::create_dir(&main)?;
    create_project(&main)?;
    git(&main, &["init"])?;
    git(&main, &["config", "user.email", "zhold@example.invalid"])?;
    git(&main, &["config", "user.name", "zhold tests"])?;
    git(&main, &["add", "."])?;
    git(&main, &["commit", "-m", "initial"])?;
    git_worktree(&main, &agent)?;

    let main_context = ContextResolver::resolve(&invocation(&main)?)?;
    let agent_context = ContextResolver::resolve(&invocation(&agent)?)?;

    assert_eq!(main_context.repository_id, agent_context.repository_id);
    assert_ne!(main_context.worktree_id, agent_context.worktree_id);
    assert_ne!(main_context.workspace_id, agent_context.workspace_id);
    assert_eq!(main_context.toolchain_id, agent_context.toolchain_id);
    assert_ne!(main_context.arena_id, agent_context.arena_id);
    assert_eq!(main_context.worktree_root, main.canonicalize()?);
    assert_eq!(agent_context.worktree_root, agent.canonicalize()?);
    Ok(())
}

fn create_project(root: &Path) -> Result<(), io::Error> {
    fs::create_dir(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"worktree-fixture\"\nversion = \"0.0.1-rc.1\"\nedition = \"2024\"\n",
    )?;
    fs::write(root.join("src/lib.rs"), "pub fn answer() -> u8 { 42 }\n")
}

fn invocation(root: &Path) -> Result<CargoInvocation, crate::StoreError> {
    CargoInvocation::new(
        "cargo".to_owned(),
        vec!["check".to_owned()],
        root.to_path_buf(),
    )
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

fn git_worktree(root: &Path, destination: &Path) -> Result<(), io::Error> {
    let output = Command::new("git")
        .args(["worktree", "add", "-b", "agent"])
        .arg(destination)
        .current_dir(root)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "git worktree add failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}
