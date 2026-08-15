//! Command orchestration.

mod cargo;
mod collect;
mod dispatch;
mod doctor;
mod explain;
mod history;
mod hook;
mod pin;
mod quota;
mod scan;
mod selector;
mod status;

#[cfg(test)]
mod selector_test;

pub(crate) use cargo::{CargoLimits, CargoReport};
pub(crate) use dispatch::execute;
pub(crate) use explain::ArenaExplanation;
