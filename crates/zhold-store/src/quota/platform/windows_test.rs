use std::path::Path;

use serde_json::json;
use zhold_core::{ByteSize, QuotaHealth, QuotaProvider};

#[test]
fn fsrm_requires_an_enabled_exact_path_hard_quota() {
    let value = json!({
        "Path": "C:\\ZHOLD",
        "ExactPath": true,
        "Size": 10_000_u64,
        "Usage": 2_000_u64,
        "SoftLimit": false,
        "Enabled": true,
        "QuotaId": "C:\\ZHOLD",
        "FilesystemId": "volume-1"
    });
    let observation =
        super::windows::from_value(Path::new("C:\\zhold"), QuotaProvider::Fsrm, &value);

    assert_eq!(observation.health, QuotaHealth::Configured);
    assert_eq!(observation.scope, Path::new("C:\\zhold"));
    assert_eq!(observation.usage, Some(ByteSize::from_bytes(2_000)));
    assert_eq!(observation.limit, Some(ByteSize::from_bytes(10_000)));
}

#[test]
fn disabled_or_broader_fsrm_quota_is_not_adoptable() {
    let value = json!({
        "Path": "C:\\",
        "ExactPath": false,
        "Size": 10_000_u64,
        "Usage": 2_000_u64,
        "SoftLimit": false,
        "Enabled": false
    });
    let observation =
        super::windows::from_value(Path::new("C:\\zhold"), QuotaProvider::Fsrm, &value);

    assert_eq!(observation.health, QuotaHealth::AvailableUnconfigured);
    assert!(!observation.exact_scope);
    assert!(!observation.hard_enforcement);
}
