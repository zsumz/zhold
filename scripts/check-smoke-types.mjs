import { spawnSync } from "node:child_process";
import { cp, mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const temporary = await mkdtemp(join(tmpdir(), "zhold-smoke-types-"));

try {
  await cp(join(repoRoot, "smoke"), join(temporary, "smoke"), { recursive: true });
  runNpm([
    "install",
    "--ignore-scripts",
    "--no-audit",
    "--no-fund",
    "--no-save",
    "--package-lock=false",
    "--prefix",
    temporary,
    "@types/node@22.18.0",
    "smoque@0.1.2",
    "typescript@5.9.3",
    "undici-types@6.21.0",
  ]);

  const sources = await findModules(join(temporary, "smoke"));
  run(process.execPath, [
    join(temporary, "node_modules", "typescript", "bin", "tsc"),
    "--allowImportingTsExtensions",
    "--erasableSyntaxOnly",
    "--exactOptionalPropertyTypes",
    "--forceConsistentCasingInFileNames",
    "--module",
    "NodeNext",
    "--moduleResolution",
    "NodeNext",
    "--noEmit",
    "--noUncheckedIndexedAccess",
    "--strict",
    "--target",
    "ES2022",
    "--types",
    "node",
    "--verbatimModuleSyntax",
    ...sources,
  ]);
  console.log(`smoke types passed: ${String(sources.length)} modules`);
} finally {
  await rm(temporary, { force: true, recursive: true });
}

async function findModules(root) {
  const modules = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) {
      modules.push(...await findModules(path));
    } else if (entry.isFile() && entry.name.endsWith(".mts")) {
      modules.push(path);
    }
  }
  return modules.sort();
}

function run(command, arguments_) {
  const result = spawnSync(command, arguments_, {
    cwd: temporary,
    stdio: "inherit",
  });
  if (result.error !== undefined) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`${command} failed with status ${String(result.status)}`);
  }
}

function runNpm(arguments_) {
  if (process.platform === "win32") {
    run(process.env.ComSpec ?? "cmd.exe", [
      "/d",
      "/s",
      "/c",
      "npm.cmd",
      ...arguments_,
    ]);
    return;
  }
  run("npm", arguments_);
}
