import { realpath } from "node:fs/promises";

import { expect, smoke } from "smoque";

import { normalizePathText } from "./support/path.mts";
import { readInventory, runZhold, setupZhold } from "./support/zhold.mts";

smoke.suite("foreign Cargo target safety", async (t) => {
  const work = await t.tempDir("zhold-foreign");
  const store = work.path("store");
  const projects = work.path("projects");
  const target = work.path("projects", "sample", "target");
  const metadata = work.path("projects", "sample", "target", ".rustc_info.json");
  const artifact = work.path("projects", "sample", "target", "artifact.rlib");

  await t.step("create an unmanaged Cargo target", async () => {
    await t.fs.writeText(metadata, "{\"foreign\":true}\n");
    await t.fs.writeText(artifact, "foreign artifact bytes\n");
    await setupZhold(t, store, work);
  });

  await t.step("report the foreign target", async () => {
    const result = await runZhold(t, store, work, ["scan", projects]);
    expect.value(result.stdout).toContain("foreign Cargo targets: 1");
    expect.value(normalizePathText(result.stdout))
      .toContain(normalizePathText(await realpath(target)));
  });

  await t.step("leave every foreign byte unchanged", async () => {
    expect.value(await t.fs.readText(metadata)).toBe("{\"foreign\":true}\n");
    expect.value(await t.fs.readText(artifact)).toBe("foreign artifact bytes\n");
    const inventory = await readInventory(t, store, work);
    expect.value(inventory.arenas).toEqual([]);
  });
});
