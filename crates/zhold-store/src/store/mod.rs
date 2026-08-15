//! Marked store capability and leased arena lifecycle.

mod arena_lease;
mod doctor;
mod finalization;
mod history;
mod initialization;
mod service;

#[cfg(test)]
mod service_test;
#[cfg(test)]
mod store_test;

pub use arena_lease::ArenaLease;
pub use doctor::DoctorReport;
pub use service::{Store, StoreInfo};
