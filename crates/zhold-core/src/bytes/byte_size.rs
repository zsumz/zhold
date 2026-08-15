use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

/// A non-negative count of bytes.
#[must_use]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ByteSize(u64);

impl ByteSize {
    /// Zero bytes.
    pub const ZERO: Self = Self(0);

    /// Creates a size from an exact byte count.
    pub const fn from_bytes(bytes: u64) -> Self {
        Self(bytes)
    }

    /// Returns the exact byte count.
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Adds two sizes, saturating at the largest representable value.
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    /// Subtracts two sizes, saturating at zero.
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    /// Multiplies the size by a percentage using integer arithmetic.
    pub fn percent(self, percent: u8) -> Self {
        let scaled = u128::from(self.0) * u128::from(percent) / 100;
        match u64::try_from(scaled) {
            Ok(bytes) => Self(bytes),
            Err(_) => Self(u64::MAX),
        }
    }
}

impl Display for ByteSize {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        const KIB: u64 = 1_024;
        const MIB: u64 = KIB * 1_024;
        const GIB: u64 = MIB * 1_024;
        const TIB: u64 = GIB * 1_024;

        let (unit, suffix) = match self.0 {
            bytes if bytes >= TIB => (TIB, "TiB"),
            bytes if bytes >= GIB => (GIB, "GiB"),
            bytes if bytes >= MIB => (MIB, "MiB"),
            bytes if bytes >= KIB => (KIB, "KiB"),
            bytes => return write!(formatter, "{bytes} B"),
        };
        let whole = self.0 / unit;
        let tenths = self.0 % unit * 10 / unit;
        write!(formatter, "{whole}.{tenths} {suffix}")
    }
}
