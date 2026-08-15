use std::str::FromStr;

use super::PinDuration;

#[test]
fn parses_pin_duration_units() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PinDuration::from_str("30m")?.as_seconds(), 1_800);
    assert_eq!(PinDuration::from_str("7d")?.as_seconds(), 604_800);
    assert_eq!(PinDuration::from_str("2w")?.as_seconds(), 1_209_600);
    Ok(())
}

#[test]
fn rejects_zero_missing_and_overflowing_durations() {
    assert!(PinDuration::from_str("0s").is_err());
    assert!(PinDuration::from_str("7").is_err());
    assert!(PinDuration::from_str("18446744073709551615w").is_err());
}
