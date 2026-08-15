# Locking and concurrency

Locks are ordinary operating-system file locks stored outside arena trees. They
are process-authoritative and release when the owning process dies.

## Lock roles

| Lock | Role |
| --- | --- |
| worktree gate | Shared for builds; exclusive for manager removal transitions |
| collection | Serializes admission snapshots, collection, and trash retirement |
| arena | Exclusive lease for one compatibility identity |
| arena metadata | Serializes manifest transitions and post-plan rereads |
| worktree registry | Serializes registration and lifecycle records |
| history | Serializes receipt publication, index changes, and pruning |
| quota | Serializes adopted quota expectation changes |
| reservation | Serializes historical growth profile changes |
| config | Serializes persistent user configuration changes |

## Admission sequence

```text
shared worktree gate
-> collection lock
-> arena lock (retry without holding collection if contended)
-> arena metadata lock
-> record size and reservation
-> cached aggregate collection and quota check
-> release collection lock
-> retain worktree gate and arena lock for the process tree
```

The retry rule prevents an admission waiting on an active arena from blocking
unrelated collection. Because the collection lock covers inventory refresh,
aggregate reservation comparison, accepted reservation persistence, and
preflight collection, concurrent agents cannot spend the same capacity.

## Collection sequence

```text
collection lock
-> immutable snapshot and deterministic plan
-> candidate arena lock (nonblocking)
-> candidate metadata lock
-> reread and revalidate
-> retirement rename
-> deletion attempt
```

A candidate that acquires a lease or changes revision after planning is skipped.
No collection path waits for an active build while holding its candidate lock.

## Worktree-manager sequence

Manager transitions take the registry lock before an exclusive worktree gate.
Build admission takes only the shared gate and reads its specific registration.
`prepare-remove` uses nonblocking exclusive acquisition so it reports an active
build rather than deadlocking with one.

## Independent locks

History, quota, reservation, and config locks are not held while recursively
deleting arenas. Non-authoritative receipt publication happens after primary
lifecycle state commits, so history failure cannot roll back a completed build
or manufacture deletion authority.
