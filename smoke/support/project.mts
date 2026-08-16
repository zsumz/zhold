import { join } from "node:path";

import type { SmokeContext } from "smoque";

export async function writeCargoProject(
  t: SmokeContext,
  root: string,
  name = "zhold-smoke-fixture",
): Promise<void> {
  await t.fs.writeText(
    join(root, "Cargo.toml"),
    `[package]
name = "${name}"
version = "0.0.0"
edition = "2024"

[workspace]
`,
  );
  await t.fs.writeText(
    join(root, "src", "main.rs"),
    `fn main() {
    println!("zhold smoke fixture");
}
`,
  );
}

export async function addFailingBuildScript(
  t: SmokeContext,
  repository: string,
): Promise<void> {
  await t.fs.writeText(
    join(repository, "build.rs"),
    `fn main() {
    eprintln!("intentional zhold smoke failure");
    std::process::exit(23);
}
`,
  );
}

export async function addWaitingBuildScript(
  t: SmokeContext,
  repository: string,
): Promise<void> {
  await t.fs.writeText(
    join(repository, "build.rs"),
    `use std::{env, fs, path::Path, thread, time::Duration};

fn main() {
    let ready = env::var("ZHOLD_SMOKE_READY").expect("ready path");
    let release = env::var("ZHOLD_SMOKE_RELEASE").expect("release path");
    fs::write(ready, "ready\n").expect("write ready marker");
    while !Path::new(&release).is_file() {
        thread::sleep(Duration::from_millis(10));
    }
}
`,
  );
}

export async function addInterruptBuildScript(
  t: SmokeContext,
  repository: string,
): Promise<void> {
  await t.fs.writeText(
    join(repository, "build.rs"),
    `use std::{env, process::Command};

fn main() {
    let pid = env::var("ZHOLD_SMOKE_CHILD_PID").expect("child pid path");
    let status = Command::new("sh")
        .args([
            "-c",
            "trap 'exit 130' INT TERM; echo $$ > \\\"$1\\\"; while :; do sleep 1; done",
            "zhold-child",
            &pid,
        ])
        .status()
        .expect("start child");
    std::process::exit(status.code().unwrap_or(1));
}
`,
  );
}
