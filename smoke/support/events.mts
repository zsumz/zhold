import type { SmokeContext } from "smoque";

import {
  requireNumber,
  requireObject,
  requireString,
  tryObject,
} from "./json.mts";

export interface CargoStart {
  arenaId: string;
  buildDir: string;
}

export interface CargoFinish {
  arenaId: string;
  exitCode: number;
  outcome: string;
}

export function parseCargoStart(t: SmokeContext, text: string): CargoStart {
  const event = findEvent(t, text, "cargo_started");
  return {
    arenaId: requireString(t, event.arena_id, "cargo_started arena_id"),
    buildDir: requireString(t, event.build_dir, "cargo_started build_dir"),
  };
}

export function parseCargoFinish(t: SmokeContext, text: string): CargoFinish {
  const event = findEvent(t, text, "cargo_finished");
  const outcome = requireObject(t, event.outcome, "cargo_finished outcome");
  return {
    arenaId: requireString(t, event.arena_id, "cargo_finished arena_id"),
    exitCode: requireNumber(t, event.exit_code, "cargo_finished exit_code"),
    outcome: requireString(t, outcome.kind, "cargo_finished outcome kind"),
  };
}

function findEvent(t: SmokeContext, text: string, name: string): Record<string, unknown> {
  for (const line of text.split("\n")) {
    const event = tryObject(line);
    if (event?.event === name) {
      return event;
    }
  }
  return t.fail(`managed Cargo output did not contain ${name}`);
}
