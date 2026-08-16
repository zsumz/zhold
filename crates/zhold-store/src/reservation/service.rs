use zhold_core::{ByteSize, CargoCommandClass};

use super::ReservationProfile;
use crate::{
    CargoInvocation, Store, StoreError,
    io::{read_optional_json, upsert_json},
    lock::ExclusiveFileLock,
};

impl Store {
    /// Returns conservative growth headroom for this command class.
    pub fn recommended_reservation(
        &self,
        invocation: &CargoInvocation,
        configured_minimum: ByteSize,
    ) -> Result<ByteSize, StoreError> {
        if self.read_only {
            return read_profile(self)
                .map(|profile| profile.recommend(invocation.command_class(), configured_minimum));
        }
        let _lock = ExclusiveFileLock::acquire(&self.layout.reservation_lock())?;
        let profile = read_profile(self)?;
        Ok(profile.recommend(invocation.command_class(), configured_minimum))
    }

    pub(crate) fn record_reservation_growth(
        &self,
        command_class: CargoCommandClass,
        growth: ByteSize,
    ) -> Result<(), StoreError> {
        self.ensure_writable("record reservation growth")?;
        let _lock = ExclusiveFileLock::acquire(&self.layout.reservation_lock())?;
        let mut profile = read_profile(self)?;
        profile.record(command_class, growth);
        persist_profile(self, &profile)
    }
}

fn read_profile(store: &Store) -> Result<ReservationProfile, StoreError> {
    let path = store.layout.reservation_profile();
    match read_optional_json::<ReservationProfile>(&path)? {
        Some(profile) => {
            profile.validate(store.marker.store_id)?;
            Ok(profile)
        }
        None => Ok(ReservationProfile::empty(store.marker.store_id)),
    }
}

fn persist_profile(store: &Store, profile: &ReservationProfile) -> Result<(), StoreError> {
    upsert_json(&store.layout.reservation_profile(), profile)
}
