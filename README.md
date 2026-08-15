<p align="center">
  <img src="./zhold-logo.svg" alt="zhold" width="720">
</p>

<p align="center"><strong>Bounded Cargo build storage for parallel Git worktrees.</strong></p>

zhold gives every Git worktree an isolated Cargo intermediate directory, keeps
active builds leased, and reclaims only complete storage it can prove it owns.
It requires no account, daemon, source upload, network request, API key, or LLM.

## Install

From this checkout:

```sh
cargo install --path crates/zhold-cli --locked
```

This RC is a source-only preview. Its crates are not published to crates.io.

## Use

Set a shared storage budget and run Cargo through zhold:

```sh
export ZHOLD_BUDGET=100GiB
zhold cargo test
```

Each physical Git worktree receives a distinct arena while compatible runs in
that worktree reuse the same intermediates. Cargo artifacts requested with
`--target-dir` remain separate from zhold's managed intermediate storage.

Inspect or manage the store:

```sh
zhold
zhold cargo test --workspace
zhold gc --dry-run
zhold gc 100GiB
zhold pin 0123456789 --for 7d
zhold explain 0123456789
zhold history --kind build
zhold doctor
```

Coordinate a worktree manager with the optional lifecycle protocol:

```sh
zhold hook ready --path . --manager worktrunk
zhold hook prepare-remove --path . --manager worktrunk
zhold hook removed --path . --manager worktrunk
```

If removal fails, restore the ready state with `zhold hook cancel-remove`.

## Quotas

zhold can inspect and adopt an existing dedicated APFS quota, Linux project or
Btrfs quota, or Windows FSRM quota as an additional refusal boundary:

```sh
zhold quota status
zhold quota plan
zhold quota adopt
```

Quota commands never provision quotas, elevate privileges, or weaken the normal
zhold budget. Adoption succeeds only when the provider identity, scope, hard
limit, and current usage can be verified exactly.

## Safety

- Collection is deterministic and protects active, pinned, and revalidated arenas.
- Deletion requires exact ownership, identity, and retirement proofs.
- Symlinks and substituted directories fail closed instead of being followed.
- Worktree removal gates block new builds before a manager removes the path.
- Operational receipts are private, bounded, and never authoritative for deletion.
- Quota drift blocks admission without changing external filesystem policy.

## Qualification

Run the complete offline repository gate with:

```sh
./scripts/check
```

It checks architecture guardrails, formatting, Clippy, tests, rustdoc, and a
clean diff.

## Status

Version 0.0.1-rc.1 is the initial release candidate. The macOS provider has a
live discovery smoke test; Linux and Windows providers have strict fixture
coverage and cross-target warnings-as-errors builds. Privileged enforcement
qualification belongs on dedicated target-host CI.

## License

Apache-2.0. See [LICENSE](LICENSE).
