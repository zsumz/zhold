//! Platform-owned Cargo process-tree supervision.

#[cfg(unix)]
mod platform_unix;
#[cfg(windows)]
mod platform_windows;

#[cfg(unix)]
pub(super) use platform_unix::PlatformSupervisor as CargoSupervisor;
#[cfg(windows)]
pub(super) use platform_windows::PlatformSupervisor as CargoSupervisor;
