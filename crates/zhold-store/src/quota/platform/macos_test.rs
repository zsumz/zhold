use std::path::Path;

use serde_json::json;
use zhold_core::{ByteSize, QuotaHealth, QuotaProvider};

#[test]
fn exact_dedicated_apfs_quota_is_configured() {
    let info = json!({
        "FilesystemType": "apfs",
        "MountPoint": "/Volumes/zhold",
        "VolumeUUID": "volume-uuid",
        "DeviceIdentifier": "disk9s1",
        "CapacityQuota": 10_000_u64,
        "CapacityInUse": 2_000_u64
    });
    let observed = super::macos::observation(
        Path::new("/Volumes/zhold"),
        QuotaProvider::ApfsVolume,
        &info,
    );
    assert_eq!(observed.health, QuotaHealth::Configured);
    assert!(observed.exact_scope);
    assert!(observed.hard_enforcement);
    assert_eq!(observed.usage, Some(ByteSize::from_bytes(2_000)));
    assert_eq!(observed.limit, Some(ByteSize::from_bytes(10_000)));
}

#[test]
fn apfs_directory_or_missing_volume_usage_cannot_be_adopted() {
    let info = json!({
        "FilesystemType": "apfs",
        "MountPoint": "/Volumes/data",
        "VolumeUUID": "volume-uuid",
        "DeviceIdentifier": "disk9s1",
        "CapacityQuota": 10_000_u64,
        "CapacityFree": 2_000_u64
    });
    let observed = super::macos::observation(
        Path::new("/Volumes/data/zhold"),
        QuotaProvider::ApfsVolume,
        &info,
    );
    assert_eq!(observed.health, QuotaHealth::AvailableUnconfigured);
    assert!(!observed.exact_scope);
    assert_eq!(observed.usage, None);
}
