# Platform support

The portable core is expected to work on current Linux, macOS, and Windows.
Every platform must pass locked tests and warnings-as-errors Clippy in CI.

| Capability | Linux | macOS | Windows |
| --- | --- | --- | --- |
| arena leases | OS file locks | OS file locks | OS file locks |
| process tree | process group | process group | Job Object |
| cancellation | forwarded Unix signals | forwarded Unix signals | console controls and job termination |
| interactive stdin | foreground process-group handoff | foreground process-group handoff | inherited console |
| atomic retirement | same-filesystem rename | same-filesystem rename | same-volume rename |
| private state | UID plus `0700`/`0600` | UID plus `0700`/`0600` | profile ACL boundary |
| optional hard quota | project/Btrfs discovery | dedicated APFS volume | FSRM discovery |

Quota support is experimental. zhold reports provisioning requirements but does
not execute them. Adoption requires an exact, already-enforced, store-scoped
limit and fails closed on identity or limit drift.

The project does not claim that path-based deletion resists a malicious same-user
race on any platform. Windows reparse-point and ACL qualification, filesystem
disconnect behavior, and target-host quota probes require dedicated host tests;
cross-compilation alone is not sufficient evidence.

On Unix, zhold transfers a controlling terminal to Cargo only when zhold already
owns the foreground process group. It restores the previous foreground group
after success, failure, or interruption. Redirected input and background
invocations do not change terminal ownership. When Cargo stops, zhold returns
the terminal to its shell-visible job and stops with it. A foreground resume
hands the terminal back to Cargo; a background resume leaves it with the shell.

Cargo 1.91 or newer is required for the stable separate build-directory feature.
The pinned minimum supported Rust toolchain is 1.91.1.

The store filesystem must support atomic same-filesystem rename, advisory file
locks, and same-directory hard links. Store opening probes hard-link publication
and fails with an explicit capability error before managed metadata is written.
