//! Private external lock capability.

mod file_lock;

#[cfg(test)]
mod file_lock_test;

pub(crate) use file_lock::{ExclusiveFileLock, LockState, SharedFileLock};
