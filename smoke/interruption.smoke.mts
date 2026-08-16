import { expect, smoke } from "smoque";

import { parseCargoFinish } from "./support/events.mts";
import {
  addInterruptBuildScript,
  createRepositoryFixture,
} from "./support/fixture.mts";
import { createProcessGroupLauncher } from "./support/process.mts";
import { readInventory, zholdBinary } from "./support/zhold.mts";

smoke.suite("installed interruption", async (t) => {
  if (process.platform === "win32") {
    t.skip("Unix signal forwarding is qualified separately from Windows controls");
  }
  const work = await t.tempDir("zhold-interruption");
  const childPid = work.path("child.pid");
  const zholdPid = work.path("zhold.pid");
  const fixture = await t.step("create a Cargo process tree", async () => {
    const created = await createRepositoryFixture(t, work);
    await addInterruptBuildScript(t, created.repository);
    return created;
  });
  const launcher = await t.step("compile a terminal process-group launcher", async () =>
    createProcessGroupLauncher(t, work)
  );

  const front = await t.step("start the managed process tree", async () =>
    t.process.start(
      launcher,
      [
        zholdPid,
        zholdBinary(t),
        "--store",
        fixture.store,
        "--format",
        "json",
        "--budget",
        "2GiB",
        "cargo",
        "check",
        "--offline",
      ],
      {
        cwd: fixture.repository,
        env: { ZHOLD_SMOKE_CHILD_PID: childPid },
        name: "interrupt-build",
        ready: t.fs.ready(childPid),
        timeout: "20s",
      },
    )
  );

  await t.step("forward SIGINT through Cargo descendants", async () => {
    const pid = (await t.fs.readText(zholdPid)).trim();
    await t.cmd("kill", ["-INT", `-${pid}`]);
    await t.poll(
      "interrupted Cargo finalization",
      () => front.stderr().includes('"event":"cargo_finished"'),
      { timeout: "20s" },
    );
    const finish = parseCargoFinish(t, front.stderr());
    expect.value(finish.outcome).toBe("terminated");
    await front.stop();
  });

  await t.step("clear the lease and descendant", async () => {
    const child = (await t.fs.readText(childPid)).trim();
    const alive = await t.cmd("kill", ["-0", child], { check: false });
    expect.value(alive.exitCode === 0).toBe(false);
    const inventory = await readInventory(t, fixture.store, fixture.repository);
    expect.value(inventory.reserved).toBe(0);
    expect.value(inventory.arenas[0]?.liveness).toBe("inactive");
    expect.value(inventory.arenas[0]?.lastOutcome).toEqual({ kind: "terminated" });
  });
});
