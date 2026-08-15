# Platform support

The portable core is expected to work on current Linux, macOS, and Windows.
Every platform must pass locked tests and warnings-as-errors Clippy in CI.

| Capability | Linux | macOS | Windows |
| --- | --- | --- | --- |
| arena leases | OS file locks | OS file locks | OS file locks |
| process tree | process group | process group | Job Object |
| cancellation | forwarded Unix signals | forwarded Unix signals | console controls and job termination |
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

Cargo 1.91 or newer is required for the stable separate build-directory feature.
The pinned minimum supported Rust toolchain is 1.91.1.
