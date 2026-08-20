# Release qualification

No release is promoted on source inspection alone.

## Required automated gates

```sh
./scripts/check
./scripts/smoke
./scripts/package-smoke
```

The canonical gate requires a locked dependency graph, formatting, capability
guardrails, warnings-as-errors Clippy, all-feature tests, the default command
surface test, and warning-free rustdoc under Rust 1.91.1.

The smoke gate installs the CLI, then uses physical Git worktrees to verify
arena isolation and reuse, final artifact placement, dry-run collection, pins,
failed builds, orphan priority, reservation admission, concurrent builds, and
real collection. Strict TypeScript checking uses an ephemeral npm prefix, so the
repository needs no package metadata. Portable suites run through pinned Smoque
on Linux, macOS, and Windows; Unix signal forwarding runs on Linux and macOS.
The package gate builds all three `.crate` archives, extracts them, and installs
the packaged CLI with its packaged library dependencies in a clean Cargo home.

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
- Unix PTY stdin, Ctrl-C delivery, Ctrl-Z foreground/background resume, and
  terminal restoration after every outcome;
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

The first public build was `0.0.1`. Promotion to RC requires the cross-platform
gates and the real multi-worktree soak, not merely passing unit tests.

## Release order

Finalize every publishable manifest and the lockfile, then commit the release
source. From that clean commit, run:

```sh
./scripts/release-check 0.0.2
```

Wait for every required CI job to pass. Create an annotated signed `v0.0.2` tag
on that exact commit. Before publishing any crate, verify the commit and tag:

```sh
python3 scripts/check-release.py tag 0.0.2
```

Publish `zhold-core`, `zhold-store`, and `zhold` in that order. Once the registry
has all three crates, verify the public consumer path in a clean Cargo home:

```sh
./scripts/registry-smoke 0.0.2
```

Create the GitHub Release only after the registry smoke passes. Never move a
published release tag.
