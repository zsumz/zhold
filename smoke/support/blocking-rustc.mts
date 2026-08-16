import type { PathRef, SmokeContext } from "smoque";

export async function createBlockingRustcWrapper(
  t: SmokeContext,
  work: PathRef,
): Promise<string> {
  const extension = process.platform === "win32" ? ".exe" : "";
  const source = work.path("blocking-rustc.rs");
  const wrapper = work.path(`blocking-rustc${extension}`);

  await t.fs.writeText(source, blockingRustcSource());
  await t.cmd("rustc", [source, "-o", wrapper]);
  return wrapper;
}

function blockingRustcSource(): string {
  return `use std::{env, fs, path::Path, process::Command, thread, time::Duration};

fn main() {
    let mut arguments = env::args_os().skip(1);
    let rustc = arguments.next().expect("wrapped rustc");
    let ready = env::var_os("ZHOLD_SMOKE_READY").expect("ready path");
    let release = env::var_os("ZHOLD_SMOKE_RELEASE").expect("release path");
    fs::write(ready, b"ready\n").expect("write ready marker");
    while !Path::new(&release).is_file() {
        thread::sleep(Duration::from_millis(10));
    }
    let status = Command::new(rustc)
        .args(arguments)
        .status()
        .expect("run wrapped rustc");
    std::process::exit(status.code().unwrap_or(1));
}
`;
}
