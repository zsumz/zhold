import type {
  CommandOptions,
  CommandResult,
  PathRef,
  SmokeContext,
} from "smoque";

import { parseCargoStart, type CargoStart } from "./events.mts";
import { parseInventory, type InventorySummary } from "./json.mts";

type WorkingDirectory = string | PathRef;

export async function runZhold(
  t: SmokeContext,
  store: string,
  cwd: WorkingDirectory,
  arguments_: string[],
  options: CommandOptions = {},
): Promise<CommandResult> {
  return t.cmd(zholdBinary(t), ["--store", store, ...arguments_], {
    cwd,
    timeout: "5m",
    ...options,
  });
}

export function zholdBinary(t: SmokeContext): string {
  const binary = process.env.ZHOLD_SMOKE_BIN;
  return binary === undefined || binary.length === 0
    ? t.fail("ZHOLD_SMOKE_BIN is required")
    : binary;
}

export async function setupZhold(
  t: SmokeContext,
  store: string,
  cwd: WorkingDirectory,
  budget = "2GiB",
): Promise<void> {
  await runZhold(t, store, cwd, ["setup", budget]);
}

export async function managedCargo(
  t: SmokeContext,
  store: string,
  cwd: WorkingDirectory,
  cargoArguments: string[],
  options: CommandOptions = {},
): Promise<CargoStart> {
  const result = await runZhold(
    t,
    store,
    cwd,
    ["--format", "json", "cargo", ...cargoArguments],
    options,
  );
  return parseCargoStart(t, result.stderr);
}

export async function readInventory(
  t: SmokeContext,
  store: string,
  cwd: WorkingDirectory,
): Promise<InventorySummary> {
  const result = await runZhold(
    t,
    store,
    cwd,
    ["--format", "json", "status", "--deep"],
  );
  return parseInventory(t, result.stdout);
}
