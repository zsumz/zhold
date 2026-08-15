<p align="center">
  <img src="./zhold-logo.svg" alt="zhold" width="720">
</p>

<p align="center"><strong>Bounded Cargo build storage for parallel Git worktrees.</strong></p>

zhold gives each physical Git worktree its own reusable Cargo intermediate
directory, protects running builds with leases, and retires only complete
storage it can prove it owns. It has no daemon, account, network service, API
key, source upload, or LLM dependency.

This repository is `0.1.0-alpha.1`. The format and command surface can still
change before the first release candidate.

## Start

Install from this checkout with Rust 1.91.1 or newer:

```sh
cargo install --path crates/zhold-cli --locked
```

Set one durable budget, then use Cargo normally through zhold:

```sh
zhold setup 200GiB
zhold cargo test
zhold
zhold gc --dry-run
zhold gc
```

`zhold` by itself shows the store, arena, reservation, pending-trash, history,
and filesystem state. A one-off collection budget remains available as
`zhold gc 100GiB --dry-run`.

Useful core commands:

```sh
zhold cargo test --workspace
zhold pin 0123456789 --for 7d
zhold unpin 0123456789
zhold explain 0123456789
zhold scan ../projects
zhold doctor
```

## What zhold bounds

The default is a conservative steady-state arena budget, not an
operating-system hard quota.

Before Cargo starts, zhold serializes admission, counts every live build
reservation, collects cold arenas, and checks an optional free-space floor.
Reservations learn from the command class's historical p95 and previous
observed growth.
After the complete Cargo process tree exits, zhold finalizes the arena, releases
its lease, and collects again.

A running build can exceed its estimate. Configure the emergency floor when the
store shares a filesystem with important data:

```sh
zhold setup 200GiB --min-free 25GiB --build-reserve 2GiB
```

Only a successfully adopted OS quota is a hard physical boundary. Quota
inspection and adoption are experimental and never provision or elevate:

```sh
cargo install --path crates/zhold-cli --locked --features experimental
zhold quota status
zhold quota plan 220GiB
zhold quota adopt 220GiB
```

The same feature exposes advanced history administration and worktree-manager
hooks. They remain outside the default alpha command surface.

## Cargo and worktrees

Arena identity includes repository, physical worktree, Cargo workspace,
toolchain, configured compiler, and relevant Cargo configuration. Multiple Cargo
workspaces in one Git worktree remain distinct. `--manifest-path`, nightly `-C`,
and `--config` participate in effective invocation discovery.

zhold appends its managed `build.build-dir` at Cargo's final command-line
configuration precedence. Cargo's final artifacts remain in the workspace
target directory; zhold owns only the separate intermediate directory supported
by Cargo 1.91.

## Safety boundary

- Active and pinned arenas are never collection candidates.
- Admission fails closed on unknown owned bytes or reservations.
- Collection rereads identity, revision, pin, lease, and worktree state.
- Retirement atomically renames a whole arena into owned trash before deletion.
- Raw Cargo arguments are never persisted.
- Unix store state is owner-only (`0700` directories, `0600` files).
- Exit zero means Cargo and zhold lifecycle finalization both succeeded.

zhold protects against crashes, ordinary concurrency, cancellation, malformed
state, symlinks, and accidental replacement. It does not claim resistance to a
malicious same-user process actively racing path-based deletion. See
[Safety and threat model](docs/safety.md) for the exact boundary.

## Reference

- [Design and guarantees](docs/design.md)
- [Safety and threat model](docs/safety.md)
- [Store format and migration](docs/store-format.md)
- [Locking and concurrency](docs/locking.md)
- [Platform support](docs/platform-support.md)
- [Release qualification](docs/release-qualification.md)

Run the complete local gate with:

```sh
./scripts/check
```

It enforces architecture capabilities, formatting, locked warnings-as-errors
Clippy, all tests, the default command surface, rustdoc, and a clean worktree.

## License

Apache-2.0. See [LICENSE](LICENSE).
