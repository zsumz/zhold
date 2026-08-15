use super::ByteSize;

#[test]
fn percentage_is_exact_before_narrowing() {
    let actual = ByteSize::from_bytes(u64::MAX).percent(80);
    let expected = u64::try_from(u128::from(u64::MAX) * 80 / 100);

    assert_eq!(expected, Ok(actual.as_u64()));
}

#[test]
fn percentage_saturates_only_when_the_result_exceeds_the_domain() {
    let actual = ByteSize::from_bytes(u64::MAX).percent(200);

    assert_eq!(actual, ByteSize::from_bytes(u64::MAX));
}
