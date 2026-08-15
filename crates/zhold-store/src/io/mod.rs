//! Private crash-aware metadata and no-follow filesystem operations.

mod json_file;
mod permissions;
mod tree;

#[cfg(test)]
mod json_file_test;
#[cfg(test)]
mod tree_test;

pub(crate) use json_file::{create_json, read_json, remove_json, write_json};
pub(crate) use permissions::{
    configure_private_file, secure_directory, secure_file, secure_open_file,
};
pub(crate) use tree::{measure_tree, remove_tree};
