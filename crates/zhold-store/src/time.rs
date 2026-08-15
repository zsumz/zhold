use std::time::{SystemTime, UNIX_EPOCH};

use crate::StoreError;

pub(crate) fn unix_seconds() -> Result<u64, StoreError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| StoreError::InvalidClock)
}

pub(crate) fn unix_milliseconds() -> Result<u64, StoreError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| {
            let milliseconds = duration.as_millis();
            u64::try_from(milliseconds).unwrap_or(u64::MAX)
        })
        .map_err(|_| StoreError::InvalidClock)
}
