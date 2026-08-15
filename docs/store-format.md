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
  trash-index/<retirement-id>.json
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

`store.json` establishes ownership with schema version 2, a random store UUID,
and a private command-fingerprint key. The key scopes sanitized invocation
fingerprints to one store and is never exposed through status output. zhold
refuses to claim a non-empty unmarked root.

Arena manifests currently write schema version 5 and read versions 1 through 5
with conservative defaults. Their arena ID must rederive from repository,
physical worktree, workspace, and toolchain identities. Revisions are monotonic
and are used for post-plan revalidation.

Retirement journals live outside the subtree being deleted and retain the
measured retired size, arena identity, nonce, revision, and exact paths. Cached
status sums validated journals; recursive deletion removes each journal last.

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
- Store schema 1 is upgraded under the initialization lock by adding a private
  fingerprint key without changing the store UUID.
- Identity or store-UUID disagreement is an ownership failure, not a migration.
- A format migration must be crash-safe, idempotent, and independently tested.
- No migration may infer ownership for an unmarked directory.
- Release qualification must include restart tests at each durable transition.
