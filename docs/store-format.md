# Store format

The store is private implementation state. External tools may inspect it, but
must not write it. Compatibility is governed by schema fields, ownership UUIDs,
and validation code rather than by path names alone.

```text
<root>/
  store.json
  config.json
  reservation-profile.json
  quota.json                         experimental, optional
  arenas/<2-hex>/<arena-id>/
    arena.json
    build/
  trash/<arena-id>-<retirement-id>/
  history/
    policy.json
    index.json
    receipts/<millis>-<uuid>.json
  integrations/worktrees/<key>.json experimental
  locks/
    collection.lock
    config.lock
    history.lock
    quota.lock
    reservation.lock
    worktrees.lock
    arenas/<arena-id>.lock
    metadata/<arena-id>.lock
    worktrees/<key>.lock
```

`store.json` establishes ownership with schema version 1 and a random store
UUID. zhold refuses to claim a non-empty unmarked root.

Arena manifests currently write schema version 4 and read versions 1 through 4
with conservative defaults. Their arena ID must rederive from repository,
physical worktree, workspace, and toolchain identities. Revisions are monotonic
and are used for post-plan revalidation.

Configuration, reservation profiles, quota expectations, worktree records,
history policies, history indexes, and receipts each use independent versioned
envelopes bound to the store UUID. A history index is an optimization only; a
dirty or absent index is rebuilt from receipts. Receipts and history never grant
deletion authority.

JSON writes use a same-directory unique staging file, file synchronization,
atomic publication or rename, directory synchronization where available, and a
recoverable primary backup during replacement. Staging files are ignored by
readers and never treated as owned arenas or receipts.

## Migration policy

- Unknown future schemas fail closed.
- Older accepted arena schemas receive only explicit safe defaults.
- Identity or store-UUID disagreement is an ownership failure, not a migration.
- A format migration must be crash-safe, idempotent, and independently tested.
- No migration may infer ownership for an unmarked directory.
- Release qualification must include restart tests at each durable transition.
