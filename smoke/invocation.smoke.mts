import { join } from "node:path";

import { expect, smoke } from "smoque";

import { finalArtifact } from "./support/fixture.mts";
import { initializeGitRepository } from "./support/git.mts";
import { writeCargoProject } from "./support/project.mts";
import { managedCargo, setupZhold } from "./support/zhold.mts";

smoke.suite("Cargo invocation compatibility", async (t) => {
  const work = await t.tempDir("zhold-invocation");
  const repository = work.path("repository");
  const store = work.path("store");
  const alpha = join(repository, "alpha");
  const beta = join(repository, "beta");
  const config = join(repository, "cargo-config.toml");

  await t.step("create two workspaces in one repository", async () => {
    await writeCargoProject(t, alpha, "zhold-smoke-alpha");
    await writeCargoProject(t, beta, "zhold-smoke-beta");
    await t.fs.writeText(config, "[build]\nincremental = false\n");
    await initializeGitRepository(t, repository);
    await setupZhold(t, store, repository);
  });

  const manifest = await t.step("build through manifest-path", async () => {
    const started = await managedCargo(
      t,
      store,
      repository,
      ["build", "--manifest-path", join(alpha, "Cargo.toml"), "--offline"],
    );
    await expect.file(finalArtifact(alpha, "zhold-smoke-alpha")).toExist();
    return started;
  });

  const changedDirectory = await t.step("build through Cargo -C", async () => {
    const started = await managedCargo(
      t,
      store,
      repository,
      ["-Z", "unstable-options", "-C", "beta", "build", "--offline"],
      { env: { RUSTC_BOOTSTRAP: "1" } },
    );
    await expect.file(finalArtifact(beta, "zhold-smoke-beta")).toExist();
    return started;
  });

  const inline = await t.step("accept inline Cargo configuration", async () =>
    managedCargo(
      t,
      store,
      repository,
      [
        "--config",
        "build.incremental=false",
        "build",
        "--manifest-path",
        join(alpha, "Cargo.toml"),
        "--offline",
      ],
    )
  );

  const fromFile = await t.step("accept file Cargo configuration", async () =>
    managedCargo(
      t,
      store,
      repository,
      [
        "-Z",
        "unstable-options",
        "--config",
        config,
        "-C",
        "beta",
        "build",
        "--offline",
      ],
      { env: { RUSTC_BOOTSTRAP: "1" } },
    )
  );

  await t.step("separate incompatible invocation contexts", async () => {
    const ids = [
      manifest.arenaId,
      changedDirectory.arenaId,
      inline.arenaId,
      fromFile.arenaId,
    ];
    expect.value(new Set(ids).size).toBe(4);
    const repeated = await managedCargo(
      t,
      store,
      repository,
      [
        "--config",
        "build.incremental=false",
        "build",
        "--manifest-path",
        join(alpha, "Cargo.toml"),
        "--offline",
      ],
    );
    expect.value(repeated.arenaId).toBe(inline.arenaId);
  });
});
