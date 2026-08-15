use std::str::FromStr;

use thiserror::Error;

use super::ByteSize;

/// Failure to parse a human-readable byte size.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ParseByteSizeError {
    /// The input contained no value.
    #[error("size cannot be empty")]
    Empty,
    /// The numeric portion was not an unsigned integer.
    #[error("size must start with a non-negative integer")]
    InvalidNumber,
    /// The unit was not one of the supported SI or IEC units.
    #[error("unsupported size unit `{0}`")]
    UnknownUnit(String),
    /// The value exceeds the largest supported byte count.
    #[error("size exceeds the supported byte range")]
    Overflow,
}

impl FromStr for ByteSize {
    type Err = ParseByteSizeError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let normalized = input.trim().to_ascii_lowercase().replace('_', "");
        if normalized.is_empty() {
            return Err(ParseByteSizeError::Empty);
        }

        let split_at = normalized
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(normalized.len());
        let (number, unit) = normalized.split_at(split_at);
        if number.is_empty() {
            return Err(ParseByteSizeError::InvalidNumber);
        }

        let value = number
            .parse::<u64>()
            .map_err(|_| ParseByteSizeError::InvalidNumber)?;
        let multiplier = unit_multiplier(unit)?;
        let bytes = value
            .checked_mul(multiplier)
            .ok_or(ParseByteSizeError::Overflow)?;
        Ok(Self::from_bytes(bytes))
    }
}

fn unit_multiplier(unit: &str) -> Result<u64, ParseByteSizeError> {
    let multiplier = match unit.trim() {
        "" | "b" => 1,
        "k" | "kb" => 1_000,
        "m" | "mb" => 1_000_000,
        "g" | "gb" => 1_000_000_000,
        "t" | "tb" => 1_000_000_000_000,
        "ki" | "kib" => 1_024,
        "mi" | "mib" => 1_048_576,
        "gi" | "gib" => 1_073_741_824,
        "ti" | "tib" => 1_099_511_627_776,
        unknown => return Err(ParseByteSizeError::UnknownUnit(unknown.to_owned())),
    };
    Ok(multiplier)
}
