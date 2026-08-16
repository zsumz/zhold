import { expect, smoke } from "smoque";

import { parseCargoFinish, parseCargoStart } from "./support/events.mts";
import {
  addFailingBuildScript,
  createRepositoryFixture,
} from "./support/fixture.mts";
import { parseCollection } from "./support/json.mts";
import { readInventory, runZhold, setupZhold } from "./support/zhold.mts";

smoke.suite("failed Cargo lifecycle", async (t) => {
  const work = await t.tempDir("zhold-failure");
  const fixture = await t.step("create a failing Cargo project", async () => {
    const created = await createRepositoryFixture(t, work);
    await addFailingBuildScript(t, created.repository);
    return created;
  });
  await t.step("configure the storage budget", async () => {
    await setupZhold(t, fixture.store, fixture.repository);
  });

  const started = await t.step("preserve the Cargo failure", async () => {
    const result = await runZhold(
      t,
      fixture.store,
      fixture.repository,
      ["--format", "json", "cargo", "check", "--offline"],
      { check: false },
    );
    const start = parseCargoStart(t, result.stderr);
    const finish = parseCargoFinish(t, result.stderr);
    expect.value(result.exitCode).toBe(101);
    expect.value(finish.arenaId).toBe(start.arenaId);
    expect.value(finish.exitCode).toBe(101);
    expect.value(finish.outcome).toBe("failed");
    return start;
  });

  await t.step("record the failed arena", async () => {
    const inventory = await readInventory(t, fixture.store, fixture.repository);
    expect.value(inventory.arenas.length).toBe(1);
    expect.value(inventory.arenas[0]?.id).toBe(started.arenaId);
    expect.value(inventory.arenas[0]?.lastOutcome).toEqual({
      code: 101,
      kind: "failed",
    });
    expect.value(inventory.arenas[0]?.liveness).toBe("inactive");
  });

  await t.step("prefer the failed arena for collection", async () => {
    const result = await runZhold(
      t,
      fixture.store,
      fixture.repository,
      ["--format", "json", "gc", "1B", "--low-watermark", "100", "--dry-run"],
    );
    const report = parseCollection(t, result.stdout);
    expect.value(report.plannedArenaIds).toEqual([started.arenaId]);
    expect.value(report.plannedReasons).toEqual(["failed_build"]);
  });
});
