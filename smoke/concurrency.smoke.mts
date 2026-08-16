import { expect, smoke } from "smoque";

import { createBlockingRustcWrapper } from "./support/blocking-rustc.mts";
import { createRepositoryFixture } from "./support/fixture.mts";
import { parseCargoFinish } from "./support/events.mts";
import { readInventory, runZhold, zholdBinary } from "./support/zhold.mts";

smoke.suite("concurrent admission", async (t) => {
  const work = await t.tempDir("zhold-concurrency");
  const firstProject = await t.step("create the reserved build", async () =>
    createRepositoryFixture(t, work, "first-repository")
  );
  const secondProject = await t.step("create the competing build", async () =>
    createRepositoryFixture(t, work, "second-repository")
  );
  const blocker = await t.step("compile the reservation blocker", async () =>
    createBlockingRustcWrapper(t, work)
  );
  const ready = work.path("first-build-ready");
  const release = work.path("release-first-build");
  const limits = [
    "--budget",
    "100MiB",
    "--build-reserve",
    "60MiB",
    "cargo",
    "check",
    "--offline",
  ];

  const first = await t.step("hold one active reservation", async () =>
    t.process.start(
      zholdBinary(t),
      ["--store", firstProject.store, "--format", "json", ...limits],
      {
        cwd: firstProject.repository,
        env: {
          RUSTC_WRAPPER: blocker,
          ZHOLD_SMOKE_READY: ready,
          ZHOLD_SMOKE_RELEASE: release,
        },
        name: "reserved-build",
        ready: t.fs.ready(ready),
        timeout: "20s",
      },
    )
  );

  await t.step("refuse the competing reservation", async () => {
    const result = await runZhold(
      t,
      secondProject.store,
      secondProject.repository,
      limits,
      { check: false },
    );
    expect.value(result.exitCode).toBe(1);
    expect.value(result.stderr).toContain("reserved");
    const inventory = await readInventory(
      t,
      firstProject.store,
      firstProject.repository,
    );
    expect.value(inventory.reserved).toBe(60 * 1024 * 1024);
    expect.value(inventory.arenas.filter((arena) => arena.liveness === "active").length)
      .toBe(1);
  });

  await t.step("release the reservation cleanly", async () => {
    await t.fs.writeText(release, "continue\n");
    await t.poll(
      "reserved build completion",
      () => first.stderr().includes('"event":"post_build_collection"'),
      { timeout: "20s" },
    );
    const finish = parseCargoFinish(t, first.stderr());
    expect.value(finish.exitCode).toBe(0);
    expect.value(finish.outcome).toBe("succeeded");

    const inventory = await readInventory(
      t,
      firstProject.store,
      firstProject.repository,
    );
    expect.value(inventory.reserved).toBe(0);
    expect.value(inventory.arenas.map((arena) => arena.lastOutcome?.kind).sort()).toEqual([
      "not_started",
      "succeeded",
    ]);
    await first.stop();
  });
});
