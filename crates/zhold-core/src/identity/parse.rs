use thiserror::Error;

/// Failure to parse a stable zhold identity.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ParseIdentityError {
    /// Identities are exactly 32 hexadecimal characters.
    #[error("identity must contain exactly 32 lowercase hexadecimal characters")]
    InvalidFormat,
}

pub(super) fn validate_identity(value: &str) -> Result<(), ParseIdentityError> {
    if value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(ParseIdentityError::InvalidFormat)
    }
}
