use std::path::{Path, PathBuf};

use tempfile::tempdir;
use zhold_core::{ByteSize, QuotaHealth, QuotaProvider};

use super::{QuotaExpectation, QuotaObservation, QuotaProbe};
use crate::{Store, io::create_json, time::unix_milliseconds};

#[derive(Clone, Debug)]
struct FakeProbe {
    observation: QuotaObservation,
}

impl QuotaProbe for FakeProbe {
    fn inspect(&self, _root: &Path, _requested: QuotaProvider) -> QuotaObservation {
        self.observation.clone()
    }
}

#[test]
fn fake_provider_distinguishes_optional_capability_states() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    for health in [
        QuotaHealth::Unsupported,
        QuotaHealth::AvailableUnconfigured,
        QuotaHealth::Inconsistent,
        QuotaHealth::PermissionRequired,
        QuotaHealth::ProviderUnavailable,
    ] {
        let probe = FakeProbe {
            observation: unavailable(&store, health),
        };
        let status = super::service::quota_status_with_probe(&store, QuotaProvider::Auto, &probe)?;
        assert!(status.expectation.is_none());
        assert!(status.healthy);
        assert_eq!(status.observation.health, health);
    }
    Ok(())
}

#[test]
fn adopted_expectation_matches_exact_identity_and_limit() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    write_expectation(&store, ByteSize::from_bytes(1_000))?;
    let probe = FakeProbe {
        observation: configured(
            &store,
            ByteSize::from_bytes(200),
            ByteSize::from_bytes(1_000),
        ),
    };

    let status = super::service::quota_status_with_probe(&store, QuotaProvider::Auto, &probe)?;

    assert!(status.healthy);
    assert_eq!(status.remaining, Some(ByteSize::from_bytes(800)));
    assert_eq!(status.observation.health, QuotaHealth::Configured);
    Ok(())
}

#[test]
fn adopted_limit_or_filesystem_drift_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    write_expectation(&store, ByteSize::from_bytes(1_000))?;
    let mut observation = configured(
        &store,
        ByteSize::from_bytes(200),
        ByteSize::from_bytes(2_000),
    );
    observation.filesystem_id = Some("changed-filesystem".to_owned());
    let probe = FakeProbe { observation };

    let status = super::service::quota_status_with_probe(&store, QuotaProvider::Auto, &probe)?;

    assert!(!status.healthy);
    assert_eq!(status.observation.health, QuotaHealth::Drifted);
    Ok(())
}

#[test]
fn persisted_provider_identities_are_bounded_and_control_free() {
    assert!(super::valid_identity("filesystem-1"));
    assert!(!super::valid_identity(""));
    assert!(!super::valid_identity("filesystem\nforged"));
    assert!(!super::valid_identity(&"x".repeat(4_097)));
}

#[test]
fn plan_and_failed_adoption_never_create_expectation() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let plan = store.quota_plan(ByteSize::from_bytes(10_000), QuotaProvider::Auto);
    assert_eq!(plan.hard_limit, ByteSize::from_bytes(10_000));
    assert!(!store.info().root.join("quota.json").exists());

    let adoption = store.quota_adopt(
        ByteSize::from_bytes(10_000),
        QuotaProvider::Auto,
        Some(ByteSize::from_bytes(20_000)),
    )?;
    assert!(adoption.attention_required);
    assert!(!store.info().root.join("quota.json").exists());
    Ok(())
}

#[test]
fn exact_external_enforcement_can_be_adopted_and_gates_reservations()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempdir()?;
    let store = Store::open(temporary.path().join("store"))?;
    let probe = FakeProbe {
        observation: configured(
            &store,
            ByteSize::from_bytes(200),
            ByteSize::from_bytes(1_000),
        ),
    };

    let adoption = super::service::adopt_locked(
        &store,
        ByteSize::from_bytes(1_000),
        QuotaProvider::Auto,
        Some(ByteSize::from_bytes(900)),
        &probe,
    )?;
    assert!(adoption.changed);
    assert!(!adoption.attention_required);
    assert!(store.info().root.join("quota.json").is_file());
    let repeated = super::service::adopt_locked(
        &store,
        ByteSize::from_bytes(1_000),
        QuotaProvider::Auto,
        Some(ByteSize::from_bytes(900)),
        &probe,
    )?;
    assert!(!repeated.changed);
    assert!(!repeated.attention_required);

    let status = super::service::quota_status_with_probe(&store, QuotaProvider::Auto, &probe)?;
    assert!(super::admission::validate(&status, ByteSize::from_bytes(800)).is_ok());
    assert!(super::admission::validate(&status, ByteSize::from_bytes(801)).is_err());
    let at_limit = super::service::status(
        status.expectation,
        configured(
            &store,
            ByteSize::from_bytes(1_000),
            ByteSize::from_bytes(1_000),
        ),
    );
    assert!(super::admission::validate(&at_limit, ByteSize::ZERO).is_err());
    Ok(())
}

fn configured(store: &Store, usage: ByteSize, limit: ByteSize) -> QuotaObservation {
    QuotaObservation {
        provider: QuotaProvider::ApfsVolume,
        health: QuotaHealth::Configured,
        scope: store.info().root,
        filesystem_id: Some("filesystem-1".to_owned()),
        quota_id: Some("quota-1".to_owned()),
        exact_scope: true,
        hard_enforcement: true,
        usage: Some(usage),
        limit: Some(limit),
        detail: "fake configured quota".to_owned(),
    }
}

fn unavailable(store: &Store, health: QuotaHealth) -> QuotaObservation {
    QuotaObservation::unavailable(
        QuotaProvider::ApfsVolume,
        store.info().root,
        health,
        "fake unavailable quota",
    )
}

fn write_expectation(store: &Store, hard_limit: ByteSize) -> Result<(), crate::StoreError> {
    let expectation = QuotaExpectation {
        schema_version: 1,
        store_id: store.info().store_id,
        provider: QuotaProvider::ApfsVolume,
        filesystem_id: "filesystem-1".to_owned(),
        quota_id: "quota-1".to_owned(),
        scope: store.info().root,
        hard_limit,
        adopted_at: unix_milliseconds()?,
    };
    let path: PathBuf = store.layout.quota();
    if create_json(&path, &expectation)? {
        Ok(())
    } else {
        Err(crate::StoreError::InvalidOwnership {
            path,
            reason: "test quota expectation already exists".to_owned(),
        })
    }
}
