//! Private crash-aware metadata and no-follow filesystem operations.

mod json_create;
mod json_file;
mod json_publish;
mod permissions;
mod tree;

#[cfg(test)]
mod json_file_test;
#[cfg(test)]
mod tree_test;

pub(crate) use json_create::{JsonCreation, create_json, create_json_commit_aware};
pub(crate) use json_file::{
    is_json_publication_artifact, read_json, remove_json, write_json, write_json_commit_aware,
};
pub(crate) use json_publish::{JsonPublication, sync_metadata_directory};
pub(crate) use permissions::{
    configure_private_file, secure_directory, secure_file, secure_open_file, verify_file,
    verify_open_file,
};
pub(crate) use tree::{measure_tree, remove_tree};
