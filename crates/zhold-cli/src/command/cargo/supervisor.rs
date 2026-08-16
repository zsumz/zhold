//! Platform-owned Cargo process-tree supervision.

#[cfg(unix)]
mod platform_unix;
#[cfg(windows)]
mod platform_windows;

#[cfg(unix)]
pub(super) use platform_unix::PlatformSupervisor as CargoSupervisor;
#[cfg(windows)]
pub(super) use platform_windows::PlatformSupervisor as CargoSupervisor;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::command) enum ChildDisposition {
    NeverCreated,
    CleanupConfirmed,
    CleanupUnconfirmed,
}

#[derive(Debug)]
pub(in crate::command) struct SpawnError {
    source: std::io::Error,
    disposition: ChildDisposition,
}

impl SpawnError {
    pub(in crate::command) const fn before_child(source: std::io::Error) -> Self {
        Self {
            source,
            disposition: ChildDisposition::NeverCreated,
        }
    }

    pub(in crate::command) fn after_child(
        source: std::io::Error,
        cleanup: Result<(), std::io::Error>,
    ) -> Self {
        match cleanup {
            Ok(()) => Self {
                source,
                disposition: ChildDisposition::CleanupConfirmed,
            },
            Err(cleanup) => Self {
                source: cleanup_unconfirmed_error(&source, &cleanup),
                disposition: ChildDisposition::CleanupUnconfirmed,
            },
        }
    }

    pub(in crate::command) const fn disposition(&self) -> ChildDisposition {
        self.disposition
    }

    pub(super) fn into_source(self) -> std::io::Error {
        self.source
    }
}

#[derive(Debug)]
pub(in crate::command) struct WaitError {
    source: std::io::Error,
    disposition: ChildDisposition,
}

impl WaitError {
    pub(in crate::command) fn after_child(
        source: std::io::Error,
        cleanup: Result<(), std::io::Error>,
    ) -> Self {
        let failure = SpawnError::after_child(source, cleanup);
        Self {
            source: failure.source,
            disposition: failure.disposition,
        }
    }

    pub(in crate::command) const fn disposition(&self) -> ChildDisposition {
        self.disposition
    }

    pub(in crate::command) fn into_source(self) -> std::io::Error {
        self.source
    }
}

fn cleanup_unconfirmed_error(source: &std::io::Error, cleanup: &std::io::Error) -> std::io::Error {
    std::io::Error::new(
        source.kind(),
        format!("{source}; Cargo process-tree cleanup could not be confirmed: {cleanup}"),
    )
}
