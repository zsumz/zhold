import { expect, smoke } from "smoque";

import { createWorktreeFixture, finalArtifact } from "./support/fixture.mts";
import { managedCargo, readInventory, setupZhold } from "./support/zhold.mts";

smoke.suite("worktree build arenas", async (t) => {
  const work = await t.tempDir("zhold-worktrees");
  const fixture = await t.step("create physical Git worktrees", async () =>
    createWorktreeFixture(t, work)
  );
  await t.step("configure the storage budget", async () => {
    await setupZhold(t, fixture.store, fixture.repository);
  });

  const primary = await t.step("build the primary worktree", async () => {
    const started = await managedCargo(
      t,
      fixture.store,
      fixture.repository,
      ["build", "--offline"],
    );
    await expect.file(finalArtifact(fixture.repository)).toExist();
    return started;
  });

  const linked = await t.step("build the linked worktree", async () => {
    const started = await managedCargo(
      t,
      fixture.store,
      fixture.worktree,
      ["build", "--offline"],
    );
    await expect.file(finalArtifact(fixture.worktree)).toExist();
    return started;
  });

  await t.step("keep worktree arenas isolated", () => {
    expect.value(primary.arenaId === linked.arenaId).toBe(false);
    expect.value(primary.buildDir === linked.buildDir).toBe(false);
  });

  await t.step("reuse the primary arena", async () => {
    const repeated = await managedCargo(
      t,
      fixture.store,
      fixture.repository,
      ["build", "--offline"],
    );
    expect.value(repeated.arenaId).toBe(primary.arenaId);
  });

  await t.step("report two inactive arenas", async () => {
    const inventory = await readInventory(t, fixture.store, fixture.repository);
    expect.value(inventory.arenas.map((arena) => arena.id).sort()).toEqual(
      [primary.arenaId, linked.arenaId].sort(),
    );
    expect.value(inventory.arenas.map((arena) => arena.liveness)).toEqual([
      "inactive",
      "inactive",
    ]);
  });
});
