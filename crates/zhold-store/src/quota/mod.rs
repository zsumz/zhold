//! Optional exact-scope filesystem quota inspection and adoption.

mod admission;
mod expectation;
mod model;
mod platform;
mod provider;
mod service;

#[cfg(test)]
mod quota_test;

pub(crate) use expectation::{read_expectation, valid_identity};
pub use model::{
    QuotaAction, QuotaAdoption, QuotaExpectation, QuotaObservation, QuotaPlan, QuotaStatus,
};
pub(crate) use platform::{inspect, plan};
pub(crate) use provider::{QuotaProbe, SystemQuotaProbe};
