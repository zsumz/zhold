use zhold_core::QuotaProvider;

use super::QuotaExpectation;
use crate::{Store, StoreError, io::read_optional_json};

const MAX_PROVIDER_IDENTITY_BYTES: usize = 4_096;

pub(crate) fn valid_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PROVIDER_IDENTITY_BYTES
        && !value.chars().any(char::is_control)
}

pub(crate) fn read_expectation(store: &Store) -> Result<Option<QuotaExpectation>, StoreError> {
    let path = store.layout.quota();
    match read_optional_json::<QuotaExpectation>(&path)? {
        Some(expectation) => validate(store, expectation, path).map(Some),
        None => Ok(None),
    }
}

fn validate(
    store: &Store,
    expectation: QuotaExpectation,
    path: std::path::PathBuf,
) -> Result<QuotaExpectation, StoreError> {
    let scope_valid = expectation.scope == store.layout.root()
        && expectation.scope.is_absolute()
        && expectation.scope.to_str().is_some();
    let provider_valid = expectation.provider != QuotaProvider::Auto;
    if expectation.schema_version == 1
        && expectation.store_id == store.marker.store_id
        && scope_valid
        && provider_valid
        && expectation.hard_limit.as_u64() > 0
        && valid_identity(&expectation.filesystem_id)
        && valid_identity(&expectation.quota_id)
    {
        Ok(expectation)
    } else {
        Err(StoreError::InvalidOwnership {
            path,
            reason: "quota expectation ownership, identity, scope, or limit is invalid".to_owned(),
        })
    }
}
