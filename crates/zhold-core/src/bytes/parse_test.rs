use std::str::FromStr;

use super::{ByteSize, ParseByteSizeError};

#[test]
fn parses_decimal_and_binary_units() -> Result<(), ParseByteSizeError> {
    assert_eq!(ByteSize::from_str("200gb")?.as_u64(), 200_000_000_000);
    assert_eq!(ByteSize::from_str("2 GiB")?.as_u64(), 2_147_483_648);
    assert_eq!(ByteSize::from_str("1_024")?.as_u64(), 1_024);
    Ok(())
}

#[test]
fn rejects_fractional_and_unknown_units() {
    assert_eq!(
        ByteSize::from_str("1.5gb"),
        Err(ParseByteSizeError::UnknownUnit(".5gb".to_owned()))
    );
    assert_eq!(
        ByteSize::from_str("12blocks"),
        Err(ParseByteSizeError::UnknownUnit("blocks".to_owned()))
    );
}
