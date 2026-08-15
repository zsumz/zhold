use std::fs;

use zhold_core::{ByteSize, CargoCommandClass};

use super::ReservationProfile;
use crate::{
    CargoInvocation, Store, StoreError,
    io::{create_json, read_json, write_json},
    lock::ExclusiveFileLock,
};

impl Store {
    /// Returns conservative growth headroom for this command class.
    pub fn recommended_reservation(
        &self,
        invocation: &CargoInvocation,
        configured_minimum: ByteSize,
    ) -> Result<ByteSize, StoreError> {
        let _lock = ExclusiveFileLock::acquire(&self.layout.reservation_lock())?;
        let profile = read_profile(self)?;
        Ok(profile.recommend(invocation.descriptor().command_class, configured_minimum))
    }

    pub(crate) fn record_reservation_growth(
        &self,
        command_class: CargoCommandClass,
        growth: ByteSize,
    ) -> Result<(), StoreError> {
        let _lock = ExclusiveFileLock::acquire(&self.layout.reservation_lock())?;
        let mut profile = read_profile(self)?;
        profile.record(command_class, growth);
        persist_profile(self, &profile)
    }
}

fn read_profile(store: &Store) -> Result<ReservationProfile, StoreError> {
    let path = store.layout.reservation_profile();
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            let profile: ReservationProfile = read_json(&path)?;
            profile.validate(store.marker.store_id)?;
            Ok(profile)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(ReservationProfile::empty(store.marker.store_id))
        }
        Err(error) => Err(StoreError::io("inspect reservation profile", path, error)),
    }
}

fn persist_profile(store: &Store, profile: &ReservationProfile) -> Result<(), StoreError> {
    let path = store.layout.reservation_profile();
    if path.exists() {
        write_json(&path, profile)
    } else if create_json(&path, profile)? {
        Ok(())
    } else {
        write_json(&path, profile)
    }
}
