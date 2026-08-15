//! Command orchestration.

mod cargo;
mod collect;
mod dispatch;
mod doctor;
mod explain;
#[cfg(feature = "experimental")]
mod history;
#[cfg(feature = "experimental")]
mod hook;
mod pin;
#[cfg(feature = "experimental")]
mod quota;
mod recover;
mod scan;
mod selector;
mod setup;
mod status;

#[cfg(test)]
mod selector_test;

pub(crate) use cargo::{CargoLimits, CargoReport};
pub(crate) use dispatch::execute;
pub(crate) use explain::ArenaExplanation;
