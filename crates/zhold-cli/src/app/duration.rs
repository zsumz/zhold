use std::{fmt::Display, str::FromStr};

use thiserror::Error;

/// Positive user-facing duration accepted by expiring pins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PinDuration(u64);

impl PinDuration {
    pub(crate) const fn as_seconds(self) -> u64 {
        self.0
    }
}

impl FromStr for PinDuration {
    type Err = ParsePinDurationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let boundary = value
            .find(|character: char| !character.is_ascii_digit() && character != '_')
            .unwrap_or(value.len());
        let (number, suffix) = value.split_at(boundary);
        let number = number.replace('_', "");
        let amount = number.parse::<u64>().map_err(|_| ParsePinDurationError)?;
        let multiplier = match suffix.to_ascii_lowercase().as_str() {
            "s" => 1,
            "m" => 60,
            "h" => 60 * 60,
            "d" => 24 * 60 * 60,
            "w" => 7 * 24 * 60 * 60,
            _ => return Err(ParsePinDurationError),
        };
        let seconds = amount
            .checked_mul(multiplier)
            .filter(|seconds| *seconds > 0)
            .ok_or(ParsePinDurationError)?;
        Ok(Self(seconds))
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("expected a positive duration such as 30m, 12h, 7d, or 2w")]
pub(crate) struct ParsePinDurationError;

impl Display for PinDuration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}s", self.0)
    }
}
