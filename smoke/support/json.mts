import type { SmokeContext } from "smoque";

type JsonObject = Record<string, unknown>;

export interface ArenaSummary {
  buildDir: string;
  id: string;
  lastOutcome?: BuildOutcomeSummary;
  liveness: string;
  pinned: boolean;
  size: number;
  worktreeExists: boolean;
}

export interface BuildOutcomeSummary {
  code?: number;
  kind: string;
}

export interface InventorySummary {
  arenas: ArenaSummary[];
  reserved: number;
  storeRoot: string;
}

export interface CollectionSummary {
  budgetMet: boolean;
  dryRun: boolean;
  plannedArenaIds: string[];
  plannedReasons: string[];
  retiredArenaIds: string[];
}

export function parseInventory(t: SmokeContext, text: string): InventorySummary {
  const root = parseObject(t, text, "inventory");
  const arenas = requireArray(t, root.arenas, "inventory arenas").map((entry) => {
    const record = requireObject(t, entry, "arena record wrapper").record;
    const arena = requireObject(t, record, "arena record");
    const lastOutcome = parseOutcome(t, arena.last_outcome);
    return {
      buildDir: requireString(t, arena.build_dir, "arena build_dir"),
      id: requireString(t, arena.id, "arena id"),
      ...(lastOutcome === undefined ? {} : { lastOutcome }),
      liveness: requireString(t, arena.liveness, "arena liveness"),
      pinned: requireBoolean(t, arena.pinned, "arena pinned"),
      size: requireNumber(t, arena.size, "arena size"),
      worktreeExists: requireBoolean(t, arena.worktree_exists, "arena worktree_exists"),
    };
  });
  return {
    arenas,
    reserved: requireNumber(t, root.reserved, "inventory reserved"),
    storeRoot: requireString(t, root.store_root, "inventory store_root"),
  };
}

export function parseCollection(t: SmokeContext, text: string): CollectionSummary {
  const report = parseObject(t, text, "collection report");
  const plan = requireObject(t, report.plan, "collection plan");
  const plannedArenaIds = requireArray(t, plan.evictions, "planned evictions")
    .map((entry) => requireObject(t, entry, "planned eviction"))
    .map((entry) => requireString(t, entry.arena_id, "planned arena_id"));
  const plannedReasons = requireArray(t, plan.evictions, "planned evictions")
    .map((entry) => requireObject(t, entry, "planned eviction"))
    .map((entry) => requireString(t, entry.reason, "planned reason"));
  const retiredArenaIds = requireArray(t, report.retirements, "retirements")
    .map((entry) => requireObject(t, entry, "retirement"))
    .map((entry) => requireString(t, entry.arena_id, "retired arena_id"));
  return {
    budgetMet: requireBoolean(t, report.budget_met, "collection budget_met"),
    dryRun: requireBoolean(t, report.dry_run, "collection dry_run"),
    plannedArenaIds,
    plannedReasons,
    retiredArenaIds,
  };
}

function parseOutcome(t: SmokeContext, value: unknown): BuildOutcomeSummary | undefined {
  if (value === null || value === undefined) {
    return undefined;
  }
  const outcome = requireObject(t, value, "arena last_outcome");
  const code = outcome.code;
  return {
    ...(code === undefined ? {} : { code: requireNumber(t, code, "outcome code") }),
    kind: requireString(t, outcome.kind, "outcome kind"),
  };
}

function parseObject(t: SmokeContext, text: string, label: string): JsonObject {
  try {
    return requireObject(t, JSON.parse(text), label);
  } catch (error) {
    return t.fail(`${label} was not valid JSON: ${String(error)}`);
  }
}

export function tryObject(text: string): JsonObject | undefined {
  try {
    const value: unknown = JSON.parse(text);
    return isObject(value) ? value : undefined;
  } catch {
    return undefined;
  }
}

export function requireObject(t: SmokeContext, value: unknown, label: string): JsonObject {
  return isObject(value) ? value : t.fail(`${label} must be a JSON object`);
}

function requireArray(t: SmokeContext, value: unknown, label: string): unknown[] {
  return Array.isArray(value) ? value : t.fail(`${label} must be an array`);
}

export function requireString(t: SmokeContext, value: unknown, label: string): string {
  return typeof value === "string" ? value : t.fail(`${label} must be a string`);
}

function requireBoolean(t: SmokeContext, value: unknown, label: string): boolean {
  return typeof value === "boolean" ? value : t.fail(`${label} must be a boolean`);
}

export function requireNumber(t: SmokeContext, value: unknown, label: string): number {
  return typeof value === "number" ? value : t.fail(`${label} must be a number`);
}

function isObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
