//! Stable local identities for repositories, worktrees, workspaces, and toolchains.

mod digest;
mod id;
mod parse;

#[cfg(test)]
mod id_test;

pub use id::{ArenaId, RepositoryId, ToolchainId, WorkspaceId, WorktreeId, WorktreeKey};

pub(crate) use digest::digest as stable_digest;
pub use parse::ParseIdentityError;
