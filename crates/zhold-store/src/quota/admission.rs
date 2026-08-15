use zhold_core::ByteSize;

use super::QuotaStatus;
use crate::StoreError;

pub(super) fn validate(
    status: &QuotaStatus,
    aggregate_reservation: ByteSize,
) -> Result<(), StoreError> {
    if !status.healthy {
        return Err(StoreError::QuotaAdmissionBlocked(format!(
            "adopted {} enforcement is not healthy: {}",
            status.observation.provider, status.observation.detail
        )));
    }
    let remaining = status.remaining.ok_or_else(|| {
        StoreError::QuotaAdmissionBlocked(
            "provider did not report usage and remaining capacity".to_owned(),
        )
    })?;
    if remaining == ByteSize::ZERO {
        return Err(StoreError::QuotaAdmissionBlocked(
            "the adopted hard quota is already at its limit".to_owned(),
        ));
    }
    if aggregate_reservation > remaining {
        return Err(StoreError::QuotaAdmissionBlocked(format!(
            "aggregate live reservation {aggregate_reservation} exceeds quota remaining {remaining}"
        )));
    }
    Ok(())
}
