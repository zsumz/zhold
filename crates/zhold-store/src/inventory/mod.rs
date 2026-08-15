//! Read-only managed-store inventory.

mod model;
mod reader;
mod uncertainty;

#[cfg(test)]
mod reader_test;

pub use model::{Inventory, InventoryEntry, InventoryFinding};
pub(crate) use reader::{ensure_real_contained_directory, read_inventory};
