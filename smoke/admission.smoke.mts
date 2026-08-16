import { expect, smoke } from "smoque";

import { createRepositoryFixture } from "./support/fixture.mts";
import { managedCargo, readInventory, runZhold, setupZhold } from "./support/zhold.mts";

smoke.suite("build admission", async (t) => {
  const work = await t.tempDir("zhold-admission");
  const fixture = await t.step("create a Cargo project", async () =>
    createRepositoryFixture(t, work)
  );

  const refusedArena = await t.step("refuse an unsafe reservation", async () => {
    const result = await runZhold(
      t,
      fixture.store,
      fixture.repository,
      [
        "--budget",
        "1B",
        "--build-reserve",
        "1GiB",
        "cargo",
        "check",
        "--offline",
      ],
      { check: false },
    );
    expect.value(result.exitCode).toBe(1);
    expect.value(result.stderr).toContain("cannot admit the build");
    expect.value(result.stderr).toContain("reserved");
    await expect.file(work.path("repository", "target")).notToExist();

    const inventory = await readInventory(t, fixture.store, fixture.repository);
    expect.value(inventory.reserved).toBe(0);
    expect.value(inventory.arenas.length).toBe(1);
    expect.value(inventory.arenas[0]?.lastOutcome).toEqual({ kind: "not_started" });
    return inventory.arenas[0]?.id ?? t.fail("refused arena was not inventoried");
  });

  await t.step("admit the same arena with a safe budget", async () => {
    await setupZhold(t, fixture.store, fixture.repository);
    const started = await managedCargo(
      t,
      fixture.store,
      fixture.repository,
      ["check", "--offline"],
    );
    expect.value(started.arenaId).toBe(refusedArena);
    const inventory = await readInventory(t, fixture.store, fixture.repository);
    expect.value(inventory.arenas[0]?.lastOutcome).toEqual({ kind: "succeeded" });
  });
});
