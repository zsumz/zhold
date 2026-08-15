use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

use super::{digest::digest, parse::validate_identity};

use crate::ParseIdentityError;

macro_rules! identity_type {
    ($name:ident, $namespace:literal, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Derives the identity from a canonical local description.
            pub fn derive(description: &str) -> Self {
                Self(digest($namespace, &[description]))
            }

            /// Returns the lowercase hexadecimal identity.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = ParseIdentityError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                validate_identity(value)?;
                Ok(Self(value.to_owned()))
            }
        }
    };
}

identity_type!(
    RepositoryId,
    "zhold.repository.v1",
    "Stable identity for one local Git common directory."
);
identity_type!(
    WorktreeId,
    "zhold.worktree.v1",
    "Stable identity for one canonical Git worktree root."
);
identity_type!(
    WorkspaceId,
    "zhold.workspace.v1",
    "Stable identity for one canonical Cargo workspace root."
);
identity_type!(
    ToolchainId,
    "zhold.toolchain.v1",
    "Stable identity for a complete Rust toolchain description."
);

/// Stable coordination identity for one repository worktree.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct WorktreeKey(String);

impl WorktreeKey {
    /// Derives the coordination identity from repository and worktree identities.
    pub fn derive(repository: &RepositoryId, worktree: &WorktreeId) -> Self {
        Self(digest(
            "zhold.worktree-key.v1",
            &[repository.as_str(), worktree.as_str()],
        ))
    }

    /// Returns the lowercase hexadecimal identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for WorktreeKey {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for WorktreeKey {
    type Err = ParseIdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        validate_identity(value)?;
        Ok(Self(value.to_owned()))
    }
}

/// Stable identity for one managed build arena.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ArenaId(String);

impl ArenaId {
    /// Derives an arena identity from every managed Cargo compatibility boundary.
    pub fn derive(
        repository: &RepositoryId,
        worktree: &WorktreeId,
        workspace: &WorkspaceId,
        toolchain: &ToolchainId,
    ) -> Self {
        Self(digest(
            "zhold.arena.v1",
            &[
                repository.as_str(),
                worktree.as_str(),
                workspace.as_str(),
                toolchain.as_str(),
            ],
        ))
    }

    /// Returns the lowercase hexadecimal identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ArenaId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for ArenaId {
    type Err = ParseIdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        validate_identity(value)?;
        Ok(Self(value.to_owned()))
    }
}
