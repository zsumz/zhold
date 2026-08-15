//! Read-only managed-store inventory.

mod arena_snapshot;
mod model;
mod reader;
mod uncertainty;

#[cfg(test)]
mod reader_test;

pub(crate) use arena_snapshot::{ArenaMeasurement, read_arena_snapshot};
pub use model::{Inventory, InventoryEntry, InventoryFinding};
pub(crate) use reader::{ensure_real_contained_directory, read_inventory};
