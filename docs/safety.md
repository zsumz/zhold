# Safety and threat model

zhold deletes data, so ownership proof is part of every destructive operation.

## Guaranteed boundary

zhold is designed to protect against crashes, stale processes, ordinary
concurrent builds, terminal cancellation, malformed metadata, accidental path
replacement, symlinks, and static directory substitution.

It does not claim protection against a malicious process running as the same
operating-system user and actively racing path traversal. Recursive deletion is
path-based. Handle-relative `openat`/`unlinkat` traversal on Unix and equivalent
reparse-point-aware handle traversal on Windows are future hardening work.

Run untrusted build scripts under a separate OS identity or sandbox if that
adversary is in scope.

## Ownership proof

A path is eligible for retirement only when all of these remain true:

- the store root has a valid marker and stable store UUID;
- the arena resides at the prefix and ID derived from its compatibility context;
- the arena, build directory, trash directory, and metadata are real contained
  paths rather than symlinks;
- the external arena lease can be acquired exclusively;
- the manifest belongs to the store and arena;
- the manifest revision equals the planned revision;
- the arena is not pinned;
- an orphaned worktree has not reappeared.

After proof, zhold writes a retirement nonce and atomically renames the complete
arena into owned trash before deletion.

## Fail-closed accounting

Active reservations count even when current tree measurement fails. A stale
measurement counts at least the last durable size. Unknown plausibly owned bytes
block admission and collection rather than becoming zero.

Cached completed sizes are trusted for serialized admission because every
managed transition durably records its observation. Manual GC remeasures before
destruction and retirement records the actual measured size.

## Privacy

Manifests and receipts store a bounded command class and fingerprint, never raw
Cargo argv. On Unix, zhold verifies effective-UID ownership and enforces `0700`
directories and `0600` metadata/lock files. Windows relies on the user's normal
profile ACL boundary; platform ACL verification remains release-qualification
work.

## Quotas

Default budgeting is not a hard limit while Cargo runs. The experimental quota
interface only adopts an already-provisioned exact-scope quota; it never runs
privileged provisioning commands. Provider identity, scope, limit, usage, and
drift are checked before an adopted quota participates in admission.

## Recovery

- A dead process releases OS locks automatically.
- An unfinished build whose sentinel lease vanished becomes suspect; its bytes
  and reservation remain protected until `zhold recover <arena> --terminated`
  records the operator's process-tree confirmation.
- A dropped reserved lease records `NotStarted`; a dropped spawning or spawned lease remains
  unfinished unless process-tree cleanup was explicitly confirmed.
- Torn JSON publication can recover from the last synchronized backup.
- A spawned arena is finalized only after the supervised process tree is observed exited or
  cleanup is confirmed; otherwise its unfinished lifecycle becomes suspect after lock release.
- Backup-based JSON recovery preserves the last validated generation until its replacement is
  durably synchronized.
- Retired directories that could not be removed retain an external journal in
  `trash-index` so partially deleting their contents cannot erase retry proof.
- A dirty history index is rebuilt from validated immutable receipts.
- Invalid or foreign entries are reported and never deleted as recovery work.
