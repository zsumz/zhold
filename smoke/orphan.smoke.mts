import { expect, smoke } from "smoque";

import { createWorktreeFixture } from "./support/fixture.mts";
import { parseCollection } from "./support/json.mts";
import { managedCargo, readInventory, runZhold, setupZhold } from "./support/zhold.mts";

smoke.suite("orphaned worktree collection", async (t) => {
  const work = await t.tempDir("zhold-orphan");
  const fixture = await t.step("create physical Git worktrees", async () =>
    createWorktreeFixture(t, work)
  );
  await t.step("configure the storage budget", async () => {
    await setupZhold(t, fixture.store, fixture.repository);
  });
  const arenas = await t.step("build both worktrees", async () => ({
    primary: await managedCargo(
      t,
      fixture.store,
      fixture.repository,
      ["build", "--offline"],
    ),
    linked: await managedCargo(
      t,
      fixture.store,
      fixture.worktree,
      ["build", "--offline"],
    ),
  }));

  const budget = await t.step("recognize the removed worktree", async () => {
    await t.cmd(
      "git",
      ["worktree", "remove", "--force", fixture.worktree],
      { cwd: fixture.repository },
    );
    const inventory = await readInventory(t, fixture.store, fixture.repository);
    const orphan = inventory.arenas.find((arena) => arena.id === arenas.linked.arenaId);
    if (orphan === undefined) {
      return t.fail("linked worktree arena was not inventoried");
    }
    expect.value(orphan.worktreeExists).toBe(false);
    return inventory.arenas.reduce((total, arena) => total + arena.size, 0) - orphan.size;
  });

  await t.step("select the orphan before a live worktree", async () => {
    const result = await runZhold(
      t,
      fixture.store,
      fixture.repository,
      [
        "--format",
        "json",
        "gc",
        `${String(budget)}B`,
        "--low-watermark",
        "100",
        "--dry-run",
      ],
    );
    const report = parseCollection(t, result.stdout);
    expect.value(report.plannedArenaIds).toEqual([arenas.linked.arenaId]);
    expect.value(report.plannedReasons).toEqual(["orphaned_worktree"]);
  });

  await t.step("retire only the orphan", async () => {
    await runZhold(
      t,
      fixture.store,
      fixture.repository,
      ["gc", `${String(budget)}B`, "--low-watermark", "100"],
    );
    const inventory = await readInventory(t, fixture.store, fixture.repository);
    expect.value(inventory.arenas.map((arena) => arena.id)).toEqual([
      arenas.primary.arenaId,
    ]);
  });
});
