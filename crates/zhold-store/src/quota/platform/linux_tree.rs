use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command, Stdio},
};

use super::linux_parser::ext4_attribute;

pub(super) fn xfs_tree_matches(root: &Path, mount: &Path, project_id: u32) -> bool {
    let Some(path) = root.to_str().filter(|value| !value.contains('\n')) else {
        return false;
    };
    let Ok(mut child) = command("xfs_quota")
        .args(["-x", "-D", "/dev/stdin", "-P", "/dev/null", "-c"])
        .arg(format!("project -c {project_id}"))
        .arg(mount)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let written = child
        .stdin
        .take()
        .is_some_and(|mut input| writeln!(input, "{project_id}:{path}").is_ok());
    let mut byte = [0_u8; 1];
    let empty = child
        .stdout
        .take()
        .is_some_and(|mut output| output.read(&mut byte).is_ok_and(|count| count == 0));
    if !empty {
        let _ = child.kill();
    }
    let status = child.wait();
    written && empty && status.is_ok_and(|value| value.success())
}

pub(super) fn ext4_tree_matches(root: &Path, project_id: u32) -> bool {
    let Ok(mut child) = command("lsattr")
        .args(["-aRp"])
        .arg(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let Some(output) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return false;
    };
    let mut consistent = true;
    for line in BufReader::new(output).lines() {
        match line {
            Ok(value) if tree_line_matches(&value, project_id) => {}
            Ok(_) | Err(_) => consistent = false,
        }
    }
    let status = child.wait();
    consistent && status.is_ok_and(|value| value.success())
}

fn tree_line_matches(line: &str, project_id: u32) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    if let Some((_, value)) = ext4_attribute(line) {
        return value == project_id;
    }
    trimmed.ends_with(':')
}

fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    command.env("LC_ALL", "C").env("LANG", "C");
    command
}
