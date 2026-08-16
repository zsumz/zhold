import { join } from "node:path";

import type { PathRef, SmokeContext } from "smoque";

export interface CompilerTools {
  compilerLog: string;
  proxy: string;
  wrapper: string;
  wrapperLog: string;
}

export async function createCompilerTools(
  t: SmokeContext,
  work: PathRef,
): Promise<CompilerTools> {
  const extension = process.platform === "win32" ? ".exe" : "";
  const proxySource = work.path("rustc-proxy.rs");
  const wrapperSource = work.path("rustc-wrapper.rs");
  const proxy = work.path(`rustc-proxy${extension}`);
  const wrapper = work.path(`rustc-wrapper${extension}`);

  await t.fs.writeText(proxySource, compilerProxySource());
  await t.fs.writeText(wrapperSource, wrapperProxySource());
  await t.cmd("rustc", [proxySource, "-o", proxy]);
  await t.cmd("rustc", [wrapperSource, "-o", wrapper]);

  return {
    compilerLog: work.path("compiler-used"),
    proxy,
    wrapper,
    wrapperLog: work.path("wrapper-used"),
  };
}

export async function configureCompiler(
  t: SmokeContext,
  repository: string,
  proxy: string,
  buildDirectory?: string,
): Promise<void> {
  const buildDir = buildDirectory === undefined
    ? ""
    : `build-dir = ${JSON.stringify(buildDirectory)}\n`;
  await t.fs.writeText(
    join(repository, ".cargo", "config.toml"),
    `[build]
rustc = ${JSON.stringify(proxy)}
${buildDir}`,
  );
}

export function compilerEnvironment(tools: CompilerTools): Record<string, string> {
  return {
    RUSTC_WRAPPER: tools.wrapper,
    ZHOLD_SMOKE_COMPILER_LOG: tools.compilerLog,
    ZHOLD_SMOKE_REAL_RUSTC: "rustc",
    ZHOLD_SMOKE_WRAPPER_LOG: tools.wrapperLog,
  };
}

function compilerProxySource(): string {
  return `use std::{env, fs, process::Command};

fn main() {
    let rustc = env::var_os("ZHOLD_SMOKE_REAL_RUSTC").expect("real rustc");
    let log = env::var_os("ZHOLD_SMOKE_COMPILER_LOG").expect("compiler log");
    fs::write(log, b"used").expect("write compiler log");
    let status = Command::new(rustc)
        .args(env::args_os().skip(1))
        .status()
        .expect("run rustc");
    std::process::exit(status.code().unwrap_or(1));
}
`;
}

function wrapperProxySource(): string {
  return `use std::{env, fs, process::Command};

fn main() {
    let mut arguments = env::args_os().skip(1);
    let rustc = arguments.next().expect("wrapped rustc");
    let log = env::var_os("ZHOLD_SMOKE_WRAPPER_LOG").expect("wrapper log");
    fs::write(log, b"used").expect("write wrapper log");
    let status = Command::new(rustc)
        .args(arguments)
        .status()
        .expect("run wrapped rustc");
    std::process::exit(status.code().unwrap_or(1));
}
`;
}
