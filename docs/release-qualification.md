# Release qualification

No release is promoted on source inspection alone.

## Required automated gates

```sh
./scripts/check
```

The canonical gate requires a locked dependency graph, formatting, capability
guardrails, warnings-as-errors Clippy, all-feature tests, the default command
surface test, and warning-free rustdoc under Rust 1.91.1.

Stable Linux, macOS, and Windows CI must independently run locked architecture,
Clippy, all-feature test, and default-surface gates. Platform-specific tests must
cover locks, rename and trash behavior, path handling, signal/process-tree
supervision, and provider compilation.

## Black-box qualification

- multiple Cargo workspaces in one Git repository;
- physical Git worktrees with distinct arena identities;
- `--manifest-path`, `-C`, inline and file `--config`;
- configured `build.rustc` and preserved `RUSTC_WRAPPER`;
- attempts to override `build.build-dir`;
- active filesystem mutation and uncertain accounting;
- two simultaneous admissions competing for one budget;
- SIGINT, SIGTERM, repeated interrupt, and descendants outliving Cargo;
- finalization and post-build collection corruption;
- pending-trash recovery and restart at durable transition points;
- low free space and adopted-quota drift;
- non-Unicode paths where the platform can produce them, or a documented error.

## Soak before RC

An RC additionally requires sustained use on the intended agent host and external
SSD with multiple repositories, multiple Git worktrees, 8–12 simultaneous
agents, successful/failed/cancelled Cargo commands, disconnect/reconnect,
repeated dry-run and real GC, and machine restart with active or staged state.

Review the resulting physical footprint, pending trash, reservation accuracy,
false admission refusals, cancellation latency, and every ownership finding.

The first public build is `0.0.1`. Promotion to RC requires the cross-platform
gates and the real multi-worktree soak, not merely passing unit tests.
