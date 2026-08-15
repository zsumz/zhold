use zhold_core::{ByteSize, QuotaHealth, QuotaProvider};

use super::{
    QuotaAdoption, QuotaExpectation, QuotaPlan, QuotaProbe, QuotaStatus, SystemQuotaProbe, plan,
    read_expectation,
};
use crate::{
    QuotaReceiptAction, Store, StoreError,
    history::{HistoryDraft, persist},
    io::{create_json, remove_json},
    lock::ExclusiveFileLock,
    time::unix_milliseconds,
};

impl Store {
    pub(crate) fn has_adopted_quota(&self) -> Result<bool, StoreError> {
        read_expectation(self).map(|expectation| expectation.is_some())
    }

    /// Inspects current capability and verifies any adopted expectation.
    pub fn quota_status(&self, requested: QuotaProvider) -> Result<QuotaStatus, StoreError> {
        quota_status_with_probe(self, requested, &SystemQuotaProbe)
    }

    /// Produces external administrator requirements without filesystem mutation.
    pub fn quota_plan(&self, hard_limit: ByteSize, provider: QuotaProvider) -> QuotaPlan {
        plan(self.layout.root(), hard_limit, provider)
    }

    /// Adopts an already-provisioned exact-scope hard quota expectation.
    pub fn quota_adopt(
        &self,
        hard_limit: ByteSize,
        provider: QuotaProvider,
        budget: Option<ByteSize>,
    ) -> Result<QuotaAdoption, StoreError> {
        let result = {
            let _lock = ExclusiveFileLock::acquire(&self.layout.quota_lock())?;
            adopt_locked(self, hard_limit, provider, budget, &SystemQuotaProbe)?
        };
        finish_adoption(self, result, QuotaReceiptAction::Adopted)
    }

    /// Removes only zhold's expectation and never changes operating-system enforcement.
    pub fn quota_unadopt(&self) -> Result<QuotaAdoption, StoreError> {
        let previous = {
            let _lock = ExclusiveFileLock::acquire(&self.layout.quota_lock())?;
            let previous = read_expectation(self)?;
            if previous.is_some() {
                remove_expectation(self, previous.as_ref())?;
            }
            previous
        };
        let requested = previous
            .as_ref()
            .map_or(QuotaProvider::Auto, |value| value.provider);
        let status = self.quota_status(requested)?;
        let mut result = QuotaAdoption {
            changed: previous.is_some(),
            attention_required: false,
            status,
            message: if previous.is_some() {
                "quota expectation removed; operating-system quota was unchanged".to_owned()
            } else {
                "no quota expectation was adopted".to_owned()
            },
            history: crate::HistoryWrite::default(),
        };
        if previous.is_some()
            && let Some(draft) = HistoryDraft::quota(&result.status, QuotaReceiptAction::Unadopted)
        {
            result.history = persist(self, draft);
        }
        Ok(result)
    }

    /// Verifies adopted enforcement and aggregate live growth before process spawn.
    pub fn verify_quota_admission(
        &self,
        aggregate_reservation: ByteSize,
    ) -> Result<Option<QuotaStatus>, StoreError> {
        let Some(expectation) = read_expectation(self)? else {
            return Ok(None);
        };
        let status = self.quota_status(expectation.provider)?;
        super::admission::validate(&status, aggregate_reservation)?;
        Ok(Some(status))
    }

    /// Observes an adopted quota after a child exits without changing child semantics.
    pub fn observe_adopted_quota(&self) -> Result<Option<QuotaStatus>, StoreError> {
        let Some(expectation) = read_expectation(self)? else {
            return Ok(None);
        };
        self.quota_status(expectation.provider).map(Some)
    }
}

pub(crate) fn quota_status_with_probe(
    store: &Store,
    requested: QuotaProvider,
    probe: &dyn QuotaProbe,
) -> Result<QuotaStatus, StoreError> {
    let expectation = read_expectation(store)?;
    let provider = expectation
        .as_ref()
        .map_or(requested, |value| value.provider);
    let observation = probe.inspect(store.layout.root(), provider);
    Ok(status(expectation, observation))
}

pub(super) fn adopt_locked(
    store: &Store,
    hard_limit: ByteSize,
    provider: QuotaProvider,
    budget: Option<ByteSize>,
    probe: &dyn QuotaProbe,
) -> Result<QuotaAdoption, StoreError> {
    if let Some(existing) = read_expectation(store)? {
        let current = quota_status_with_probe(store, existing.provider, probe)?;
        let same = existing.provider == provider || provider == QuotaProvider::Auto;
        if same && existing.hard_limit == hard_limit && current.healthy {
            return Ok(adoption(
                false,
                false,
                current,
                "quota expectation is already current",
            ));
        }
        return Ok(adoption(
            false,
            true,
            current,
            "a different quota expectation is already adopted; unadopt it explicitly first",
        ));
    }
    let observation = probe.inspect(store.layout.root(), provider);
    let current = status(None, observation.clone());
    let valid_limit = hard_limit.as_u64() > 0
        && observation.limit == Some(hard_limit)
        && observation.usage.is_some_and(|usage| usage < hard_limit);
    let budget_valid = budget.is_none_or(|value| hard_limit > value);
    let identities = observation
        .filesystem_id
        .clone()
        .zip(observation.quota_id.clone())
        .filter(|(filesystem, quota)| {
            super::valid_identity(filesystem) && super::valid_identity(quota)
        });
    let provider_matches = observation.provider != QuotaProvider::Auto
        && (provider == QuotaProvider::Auto || observation.provider == provider);
    let enforceable = observation.health == QuotaHealth::Configured
        && provider_matches
        && observation.exact_scope
        && observation.scope == store.layout.root()
        && observation.hard_enforcement
        && identities.is_some();
    if !enforceable || !valid_limit || !budget_valid {
        let message = if !budget_valid {
            "hard quota must exceed the configured active-arena budget"
        } else if observation.usage.is_some_and(|usage| usage >= hard_limit) {
            "observed usage is already at or above the requested hard limit"
        } else if observation.limit != Some(hard_limit) {
            "observed hard limit does not exactly match the requested limit"
        } else {
            "provider did not prove a configured exact-scope hard quota"
        };
        return Ok(adoption(false, true, current, message));
    }
    let Some((filesystem_id, quota_id)) = identities else {
        return Ok(adoption(
            false,
            true,
            current,
            "provider did not return stable filesystem and quota identities",
        ));
    };
    let expectation = QuotaExpectation {
        schema_version: 1,
        store_id: store.marker.store_id,
        provider: observation.provider,
        filesystem_id,
        quota_id,
        scope: store.layout.root().to_path_buf(),
        hard_limit,
        adopted_at: unix_milliseconds()?,
    };
    let path = store.layout.quota();
    if !create_json(&path, &expectation)? {
        return Err(StoreError::InvalidOwnership {
            path,
            reason: "quota expectation appeared during adoption".to_owned(),
        });
    }
    Ok(adoption(
        true,
        false,
        status(Some(expectation), observation),
        "externally provisioned hard quota adopted",
    ))
}

fn finish_adoption(
    store: &Store,
    mut result: QuotaAdoption,
    action: QuotaReceiptAction,
) -> Result<QuotaAdoption, StoreError> {
    if result.changed {
        result.status = store.quota_status(result.status.observation.provider)?;
        if !result.status.healthy {
            result.attention_required = true;
            "quota changed during adoption verification; expectation is fail-closed"
                .clone_into(&mut result.message);
        }
        if let Some(draft) = HistoryDraft::quota(&result.status, action) {
            result.history = persist(store, draft);
        }
    }
    Ok(result)
}

pub(super) fn status(
    expectation: Option<QuotaExpectation>,
    mut observation: super::QuotaObservation,
) -> QuotaStatus {
    let healthy = expectation.as_ref().is_none_or(|expected| {
        observation.health == QuotaHealth::Configured
            && observation.exact_scope
            && observation.hard_enforcement
            && observation.scope == expected.scope
            && observation.filesystem_id.as_ref() == Some(&expected.filesystem_id)
            && observation.quota_id.as_ref() == Some(&expected.quota_id)
            && observation.limit == Some(expected.hard_limit)
    });
    if expectation.is_some() && !healthy {
        observation.health = QuotaHealth::Drifted;
    }
    let remaining = observation
        .limit
        .zip(observation.usage)
        .map(|(limit, usage)| limit.saturating_sub(usage));
    QuotaStatus {
        expectation,
        observation,
        healthy,
        remaining,
    }
}

fn adoption(
    changed: bool,
    attention_required: bool,
    status: QuotaStatus,
    message: &str,
) -> QuotaAdoption {
    QuotaAdoption {
        changed,
        attention_required,
        status,
        message: message.to_owned(),
        history: crate::HistoryWrite::default(),
    }
}

fn remove_expectation(
    store: &Store,
    expected: Option<&QuotaExpectation>,
) -> Result<(), StoreError> {
    let current = read_expectation(store)?;
    if current.as_ref() != expected {
        return Err(StoreError::InvalidOwnership {
            path: store.layout.quota(),
            reason: "quota expectation changed before unadoption".to_owned(),
        });
    }
    remove_json(&store.layout.quota())
}
