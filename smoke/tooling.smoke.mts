import { join } from "node:path";

import { expect, smoke } from "smoque";

import {
  compilerEnvironment,
  configureCompiler,
  createCompilerTools,
} from "./support/compiler.mts";
import { createRepositoryFixture } from "./support/fixture.mts";
import { managedCargo, runZhold, setupZhold } from "./support/zhold.mts";

smoke.suite("Cargo compiler configuration", async (t) => {
  const work = await t.tempDir("zhold-tooling");
  const fixture = await t.step("create a Cargo project", async () =>
    createRepositoryFixture(t, work)
  );
  await setupZhold(t, fixture.store, fixture.repository);

  const baseline = await t.step("build with the default compiler", async () =>
    managedCargo(t, fixture.store, fixture.repository, ["check", "--offline"])
  );

  const tools = await t.step("compile compiler proxies", async () => {
    const compiled = await createCompilerTools(t, work);
    await configureCompiler(t, fixture.repository, compiled.proxy);
    return compiled;
  });
  const environment = compilerEnvironment(tools);

  await t.step("preserve configured rustc and RUSTC_WRAPPER", async () => {
    const selected = await managedCargo(
      t,
      fixture.store,
      fixture.repository,
      ["check", "--offline"],
      { env: environment },
    );
    expect.value(selected.arenaId === baseline.arenaId).toBe(false);
    await expect.file(tools.compilerLog).toContain("used");
    await expect.file(tools.wrapperLog).toContain("used");
  });

  await t.step("override configured build directories", async () => {
    const projectOverride = work.path("project-override");
    const fileOverride = work.path("file-override");
    const inlineOverride = work.path("inline-override");
    const extra = work.path("build-dir-config.toml");
    await configureCompiler(t, fixture.repository, tools.proxy, projectOverride);
    await t.fs.writeText(
      extra,
      `[build]\nbuild-dir = ${JSON.stringify(fileOverride)}\n`,
    );
    await managedCargo(
      t,
      fixture.store,
      fixture.repository,
      [
        "check",
        "--offline",
        "--config",
        extra,
        "--config",
        `build.build-dir=${JSON.stringify(inlineOverride)}`,
      ],
      { env: environment },
    );
    await expect.file(projectOverride).notToExist();
    await expect.file(fileOverride).notToExist();
    await expect.file(inlineOverride).notToExist();
  });

  await t.step("reject an inherited build directory", async () => {
    const inherited = join(work.path("caller-build"));
    const result = await runZhold(
      t,
      fixture.store,
      fixture.repository,
      ["cargo", "check", "--offline"],
      {
        check: false,
        env: { ...environment, CARGO_BUILD_BUILD_DIR: inherited },
      },
    );
    expect.value(result.exitCode).toBe(1);
    expect.value(result.stderr).toContain("refusing to replace");
    await expect.file(inherited).notToExist();
  });
});
