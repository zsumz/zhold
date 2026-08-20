<p align="center">
  <img src="https://raw.githubusercontent.com/zsumz/zhold/main/zhold-logo.svg" alt="zhold" width="720">
</p>

<p align="center"><strong>Bounded Cargo build storage for parallel Git worktrees.</strong></p>

zhold manages Cargo intermediate files for repositories that use multiple Git
worktrees. Each worktree keeps isolated, reusable build storage. zhold removes
inactive storage to stay near one configured budget.

zhold is a public alpha. Rust 1.91.1 or newer is required.

## Problem

Sharing Cargo build intermediates across worktrees causes build-directory lock
contention. Keeping them separate avoids that contention, but disk use grows
with every worktree.

zhold keeps the build directories separate and manages their lifecycle under
one budget.

Related Cargo issues:
[#16804](https://github.com/rust-lang/cargo/issues/16804) and
[#5026](https://github.com/rust-lang/cargo/issues/5026).

## Install

```sh
cargo install zhold --locked
```

## Start

```sh
zhold setup 200GiB
zhold cargo test
zhold
zhold gc --dry-run
```

Use `zhold cargo ...` in place of `cargo ...` for managed builds. Running
`zhold` shows the current store status.

Pass a one-time budget to collection when needed:

```sh
zhold gc 100GiB --dry-run
```

## Model

- Cargo intermediates go into a zhold-owned build directory.
- Builds with the same worktree, workspace, toolchain, compiler, and Cargo
  configuration reuse the same directory.
- zhold reserves space and may remove inactive directories before and after a
  build.
- Active, pinned, suspect, and uncertain directories are not removed.
- Final artifacts remain in the workspace `target/` directory and are outside
  the zhold budget.

## Budget

The budget is a steady-state limit, not a hard quota. A running build can exceed
its reservation. A minimum free-space floor can stop a new managed build before
it starts:

```sh
zhold setup 200GiB --min-free 25GiB --build-reserve 2GiB
```

## Safety

zhold removes only complete build directories that it created and can validate.
It checks ownership, metadata, leases, pins, and worktree state before removal.
Unknown state stops collection.

See the [safety and threat model](https://github.com/zsumz/zhold/blob/main/docs/safety.md)
for the exact guarantees.

## Development

```sh
cargo install --path crates/zhold-cli --locked
./scripts/check
./scripts/smoke
./scripts/package-smoke
```

See [design](https://github.com/zsumz/zhold/blob/main/docs/design.md),
[locking](https://github.com/zsumz/zhold/blob/main/docs/locking.md), and
[platform support](https://github.com/zsumz/zhold/blob/main/docs/platform-support.md)
for implementation details.

## License

Apache-2.0. See [LICENSE](https://github.com/zsumz/zhold/blob/main/LICENSE).
