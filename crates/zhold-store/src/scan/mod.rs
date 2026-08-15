//! Read-only foreign Cargo target discovery.

mod report;
mod scanner;

pub use report::{ForeignTarget, ScanReport};
pub(crate) use scanner::scan;
