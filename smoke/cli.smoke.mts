import { expect, smoke } from "smoque";

import { readInventory, runZhold, setupZhold } from "./support/zhold.mts";

smoke.suite("installed CLI", async (t) => {
  const work = await t.tempDir("zhold-cli");
  const store = work.path("store");

  await t.step("reports the release version", async () => {
    const result = await runZhold(t, store, t.repoRoot(), ["--version"]);
    expect.value(result.stdout.trim()).toBe("zhold 0.0.2");
  });

  await t.step("describes the bounded storage command", async () => {
    const result = await runZhold(t, store, t.repoRoot(), ["--help"]);
    expect.value(result.stdout).toContain("Bounded Cargo build storage");
    expect.value(result.stdout).toContain("setup    Persist simple defaults");
  });

  await t.step("requires setup before managed Cargo", async () => {
    const result = await runZhold(
      t,
      store,
      t.repoRoot(),
      ["cargo", "metadata", "--no-deps"],
      { check: false },
    );
    expect.value(result.exitCode).toBe(1);
    expect.value(result.stderr).toContain("run `zhold setup 200GiB` first");
  });

  await t.step("persists a budget and exposes an empty inventory", async () => {
    await setupZhold(t, store, t.repoRoot());
    const inventory = await readInventory(t, store, t.repoRoot());
    expect.value(inventory.arenas).toEqual([]);
    expect.value(inventory.storeRoot.endsWith("store")).toBe(true);
  });
});
