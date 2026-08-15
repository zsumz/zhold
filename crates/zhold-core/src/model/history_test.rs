use std::str::FromStr;

use super::history::HistoryKind;

#[test]
fn history_kinds_have_a_closed_stable_cli_vocabulary() {
    for (text, expected) in [
        ("build", HistoryKind::Build),
        ("collection", HistoryKind::Collection),
        ("hook", HistoryKind::Hook),
        ("quota", HistoryKind::Quota),
    ] {
        assert_eq!(HistoryKind::from_str(text), Ok(expected));
        assert_eq!(expected.to_string(), text);
    }
    assert!(HistoryKind::from_str("command").is_err());
}
