//! Marked store capability and leased arena lifecycle.

mod admission;
mod arena_lease;
mod collection;
mod config;
mod doctor;
mod finalization;
mod history;
mod initialization;
mod lifecycle;
mod opening;
mod recovery;
mod service;

#[cfg(test)]
mod accounting_test;
#[cfg(test)]
mod lifecycle_test;
#[cfg(test)]
mod recovery_test;
#[cfg(test)]
mod service_test;
#[cfg(test)]
mod store_test;

pub use arena_lease::ArenaLease;
pub use config::StoreConfig;
pub use doctor::DoctorReport;
pub use service::{Store, StoreInfo};
