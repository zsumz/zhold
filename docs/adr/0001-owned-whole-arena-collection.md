# ADR 0001: collect only owned whole arenas

Status: accepted

## Context

Cargo intermediates contain internal relationships that are not a stable public
deletion API. Selective cleanup would require zhold to infer reachability and
compatibility inside Cargo's private layout while concurrent builds mutate it.

## Decision

zhold assigns a complete intermediate build directory to a deterministic
compatibility identity. It collects only that complete directory after proving
store ownership, identity, lease availability, unchanged revision, pin state,
and worktree state. Retirement is an atomic rename into owned trash followed by
a best-effort recursive deletion.

## Consequences

- Deletion is conservative and explainable.
- Active builds need one external lease per arena.
- Reuse granularity is an arena, not an individual artifact.
- Collection may reclaim less than a Cargo-internal optimizer could.
- Compatibility identity changes deliberately create a new arena.
- Pending trash is distinct from live arena usage and physical reclamation.
