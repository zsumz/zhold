import { expect, smoke } from "smoque";

import { createWorktreeFixture } from "./support/fixture.mts";
import { parseCollection } from "./support/json.mts";
import { managedCargo, readInventory, runZhold, setupZhold } from "./support/zhold.mts";

smoke.suite("bounded collection", async (t) => {
  const work = await t.tempDir("zhold-collection");
  const fixture = await t.step("create physical Git worktrees", async () =>
    createWorktreeFixture(t, work)
  );
  await t.step("configure the storage budget", async () => {
    await setupZhold(t, fixture.store, fixture.repository);
  });
  const { primary, linked } = await t.step("build both worktrees", async () => ({
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

  await t.step("pin the primary arena", async () => {
    await runZhold(t, fixture.store, fixture.repository, ["pin", primary.arenaId]);
  });

  await t.step("plan collection without mutation", async () => {
    const result = await runZhold(
      t,
      fixture.store,
      fixture.repository,
      ["--format", "json", "gc", "1B", "--low-watermark", "100", "--dry-run"],
      { check: false },
    );
    const report = parseCollection(t, result.stdout);
    expect.value(result.exitCode).toBe(2);
    expect.value(report.dryRun).toBe(true);
    expect.value(report.budgetMet).toBe(false);
    expect.value(report.plannedArenaIds).toEqual([linked.arenaId]);
    const inventory = await readInventory(t, fixture.store, fixture.repository);
    expect.value(inventory.arenas.length).toBe(2);
  });

  await t.step("retire only the unpinned arena", async () => {
    const result = await runZhold(
      t,
      fixture.store,
      fixture.repository,
      ["--format", "json", "gc", "1B", "--low-watermark", "100"],
      { check: false },
    );
    const report = parseCollection(t, result.stdout);
    expect.value(result.exitCode).toBe(2);
    expect.value(report.retiredArenaIds).toEqual([linked.arenaId]);
    const inventory = await readInventory(t, fixture.store, fixture.repository);
    expect.value(inventory.arenas.map((arena) => arena.id)).toEqual([primary.arenaId]);
    expect.value(inventory.arenas[0]?.pinned).toBe(true);
  });

  await t.step("collect the arena after unpinning", async () => {
    await runZhold(t, fixture.store, fixture.repository, ["unpin", primary.arenaId]);
    await runZhold(
      t,
      fixture.store,
      fixture.repository,
      ["gc", "1B", "--low-watermark", "100"],
    );
    const inventory = await readInventory(t, fixture.store, fixture.repository);
    expect.value(inventory.arenas).toEqual([]);
  });
});
