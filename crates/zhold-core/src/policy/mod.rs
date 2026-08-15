//! Deterministic retention planning.

mod planner;

#[cfg(test)]
mod planner_test;

pub use planner::{PolicyError, plan_collection, plan_collection_with_reservation};
