use std::str::FromStr;

use zhold_core::ArenaId;

use crate::CliError;

use super::selector::select_for_test;

#[test]
fn resolves_the_prefix_printed_by_status() -> Result<(), Box<dyn std::error::Error>> {
    let first = ArenaId::from_str("0123456789abcdef0123456789abcdef")?;
    let second = ArenaId::from_str("fedcba9876543210fedcba9876543210")?;

    let selected = select_for_test("0123456789", &[first.clone(), second])?;

    assert_eq!(selected, first);
    Ok(())
}

#[test]
fn rejects_short_or_ambiguous_prefixes() -> Result<(), Box<dyn std::error::Error>> {
    let first = ArenaId::from_str("0123456789abcdef0123456789abcdef")?;
    let second = ArenaId::from_str("012345ffffffffff012345ffffffffff")?;

    assert!(matches!(
        select_for_test("01234", std::slice::from_ref(&first)),
        Err(CliError::ArenaSelectorTooShort { .. })
    ));
    assert!(matches!(
        select_for_test("012345", &[first, second]),
        Err(CliError::ArenaSelectorAmbiguous { count: 2, .. })
    ));
    Ok(())
}
