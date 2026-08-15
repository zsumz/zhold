//! Bounded command-class growth estimates for conservative admission.

mod profile;
mod service;

#[cfg(test)]
mod reservation_test;

pub(crate) use profile::ReservationProfile;
