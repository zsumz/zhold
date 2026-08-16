//! Platform-owned Cargo process-tree supervision.

#[cfg(unix)]
mod platform_unix;
#[cfg(windows)]
mod platform_windows;

#[cfg(unix)]
pub(super) use platform_unix::PlatformSupervisor as CargoSupervisor;
#[cfg(windows)]
pub(super) use platform_windows::PlatformSupervisor as CargoSupervisor;

#[derive(Debug)]
pub(super) struct SpawnError {
    source: std::io::Error,
    child_created: bool,
}

impl SpawnError {
    pub(super) const fn before_child(source: std::io::Error) -> Self {
        Self {
            source,
            child_created: false,
        }
    }

    pub(super) const fn after_child(source: std::io::Error) -> Self {
        Self {
            source,
            child_created: true,
        }
    }

    pub(super) const fn child_created(&self) -> bool {
        self.child_created
    }

    pub(super) fn into_source(self) -> std::io::Error {
        self.source
    }
}
