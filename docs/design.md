# Design

zhold is a lifecycle manager for Cargo intermediate build storage. Its unit of
ownership and collection is a complete arena, never an inferred Cargo-internal
subtree.

## Contract

The default contract is a steady-state arena budget:

1. Resolve the effective Cargo and Git context.
2. Derive one arena from repository, physical worktree, workspace, toolchain,
   compiler, and relevant Cargo configuration identity.
3. Serialize admission, count every live reservation, and collect to make room.
4. Hold an external lease for the complete Cargo process tree.
5. Finalize the arena durably, release the lease, and collect again.

The default backend cannot prevent a running process from exceeding its
reservation. It combines a persistent minimum, historical p95 observed growth,
the previous observed growth, and a minimum-free-space floor to make that risk
conservative and visible. Only a successfully adopted operating-system quota is
a hard physical byte boundary.

## Crates

```text
zhold-core   identities, byte arithmetic, domain models, pure policy
zhold-store  ownership, leases, metadata, accounting, retirement
zhold-cli    parsing, orchestration, supervision, presentation
```

Executable guardrails prevent core from gaining OS capabilities, CLI and
rendering code from gaining destructive store capabilities, and quota providers
from gaining arena-manifest access.

## Arena lifecycle

```text
absent -> staged -> reserved -> spawning -> spawned -> finalized -> retired -> deleted
                         |   |         |
                         |   +-> suspect (explicit recovery required)
                         +------> pinned
```

An active arena is protected by an operating-system file lock outside the arena
tree. The manifest revision changes at lifecycle transitions. Collection plans
from an immutable snapshot, then rereads and revalidates identity, revision,
pin, lease, and worktree state immediately before retirement.

If a build is unfinished after its sentinel lease disappears, the arena becomes
suspect. Its size and reservation remain protected, collection refuses to
retire it, and reuse requires an explicit recovery decision.

Retirement first writes an external authority journal, then renames an owned
arena into owned trash. Recursive removal is attempted only after that namespace
transition, and the journal is deleted last. Failed physical deletion therefore
retains retry proof even if arena contents were already removed.

## Accounting

Routine Cargo admission uses manifests, leases, cached completed sizes, and live
reservations. It does not recursively walk arena trees. A missing durable size,
invalid owned metadata, or an unrecoverable active reservation blocks admission.

Bare status uses cached sizes and retirement journals without descending into
build trees. `zhold status --deep`, doctor, and manual collection perform deep
measurement. The inventory distinguishes fresh, cached, stale, and unknown
size quality. Stale and unknown owned state is protected and makes the budget
unsatisfied.

The user-facing vocabulary is deliberately separate:

- `arena_budget`: steady-state live arena bytes plus active reservations;
- `min_filesystem_free`: free-space refusal floor before process spawn;
- pending trash: retired but not physically reclaimed bytes;
- physical footprint: every byte beneath the marked store root;
- adopted quota: optional hard physical enforcement outside zhold.

## Cargo integration

zhold resolves Cargo metadata using the effective leading toolchain and global
options, including `--manifest-path`, `-C`, and `--config`. Relevant config files
and compiler selection participate in compatibility identity. The managed
`build.build-dir` override is appended at final command-line precedence.
Recursive file-based `include` values are source-relative and identity-bearing;
inline `--config` values containing `include` are rejected as an explicit
compatibility limit.

Final artifacts continue to use Cargo's workspace target directory; zhold owns
only the separate intermediate build directory introduced in Cargo 1.91.

## Process contract

The sentinel, not the front process, owns the lease. On Unix it owns a child
process group and forwards termination signals. On Windows it owns a Job Object.
The lease is released only after the owned process tree exits and primary state
is durably finalized.

The manifest persists `Reserved`, `Spawning`, `Spawned`, and `Finalized`, so
restart recovery never has to infer whether process creation was attempted from
timestamps. New arenas are assembled and synchronized in a journal-authorized
staging directory before atomic promotion into the owned arena namespace.

Exit zero means Cargo succeeded, primary finalization succeeded, and mandatory
post-build collection completed without losing zhold's lifecycle guarantees.
Cargo failures preserve Cargo's code; zhold management failures use a distinct
nonzero code.

## Read-only contract

Status, deep status, doctor, scan, explain, and GC dry-run open only an existing
store. They do not initialize layout, probe filesystem publication capability,
create lock files, repair metadata, or publish receipts. Mutating Store methods
reject a read-only handle before acquiring writable locks or touching metadata.
