use std::path::Path;

use crate::{
    Store, StoreError,
    layout::StoreLayout,
    store::initialization::{
        ensure_layout, inspect_store_root, open_marker_read_only, open_marker_read_write,
        prepare_store_root, verify_filesystem_capabilities,
    },
};

impl Store {
    /// Opens an existing marked store or initializes an empty directory.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::open_read_write(root)
    }

    /// Opens an existing store without creating, probing, upgrading, or repairing it.
    pub fn open_read_only(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = inspect_store_root(root.as_ref())?;
        let layout = StoreLayout::new(root);
        let marker = open_marker_read_only(&layout)?;
        Ok(Self {
            layout,
            marker,
            read_only: true,
        })
    }

    /// Opens an existing store for mutation or initializes an empty directory.
    pub fn open_read_write(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let requested = root.as_ref();
        prepare_store_root(requested)?;
        let root = requested
            .canonicalize()
            .map_err(|error| StoreError::io("canonicalize store root", requested, error))?;
        let layout = StoreLayout::new(root);
        let marker = open_marker_read_write(&layout)?;
        verify_filesystem_capabilities(layout.root())?;
        ensure_layout(&layout)?;
        Ok(Self {
            layout,
            marker,
            read_only: false,
        })
    }
}
