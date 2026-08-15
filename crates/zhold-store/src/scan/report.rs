use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zhold_core::ByteSize;

use crate::{Inventory, InventoryFinding};

/// Combined managed and read-only foreign storage scan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScanReport {
    /// Managed store inventory.
    pub managed: Inventory,
    /// Cargo target directories outside the marked store.
    pub foreign_targets: Vec<ForeignTarget>,
    /// Paths that could not be inspected.
    pub findings: Vec<InventoryFinding>,
}

/// Read-only Cargo target directory not owned by zhold.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ForeignTarget {
    /// Canonical target directory path.
    pub path: PathBuf,
    /// Best available measured size.
    pub size: ByteSize,
}
