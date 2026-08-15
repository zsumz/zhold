//! Cargo and Git context discovery.

mod build_context;
mod cargo;
mod config_discovery;
mod config_identity;
mod config_loader;
#[cfg(test)]
mod config_test;
mod git;
mod invocation;
mod process;
mod resolver;
mod worktree_context;

#[cfg(test)]
mod context_test;
#[cfg(test)]
mod real_worktree_test;

pub use build_context::BuildContext;
pub use invocation::CargoInvocation;
pub use resolver::ContextResolver;
pub use worktree_context::WorktreeContext;
