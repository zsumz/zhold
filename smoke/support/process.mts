import type { PathRef, SmokeContext } from "smoque";

export async function createProcessGroupLauncher(
  t: SmokeContext,
  work: PathRef,
): Promise<string> {
  const source = work.path("process-group-launcher.rs");
  const launcher = work.path("process-group-launcher");
  await t.fs.writeText(
    source,
    `use std::{env, fs, os::unix::process::CommandExt, process::Command};

fn main() {
    let mut arguments = env::args_os().skip(1);
    let pid_path = arguments.next().expect("pid path");
    let program = arguments.next().expect("program");
    let mut child = Command::new(program)
        .args(arguments)
        .process_group(0)
        .spawn()
        .expect("spawn process group");
    fs::write(pid_path, child.id().to_string()).expect("write pid");
    let status = child.wait().expect("wait for process group");
    std::process::exit(status.code().unwrap_or(1));
}
`,
  );
  await t.cmd("rustc", [source, "-o", launcher]);
  return launcher;
}
