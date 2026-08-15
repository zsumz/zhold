use std::str::FromStr;

use super::quota::QuotaProvider;

#[test]
fn quota_providers_have_a_closed_stable_cli_vocabulary() {
    for provider in [
        QuotaProvider::Auto,
        QuotaProvider::XfsProject,
        QuotaProvider::Ext4Project,
        QuotaProvider::BtrfsQgroup,
        QuotaProvider::ApfsVolume,
        QuotaProvider::Fsrm,
    ] {
        let text = provider.to_string();
        assert_eq!(QuotaProvider::from_str(&text), Ok(provider));
    }
    assert!(QuotaProvider::from_str("user-quota").is_err());
}
