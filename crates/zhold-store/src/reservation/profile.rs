use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::{ByteSize, CargoCommandClass};

use crate::StoreError;

pub(super) const SAMPLE_LIMIT: usize = 128;
const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReservationProfile {
    schema_version: u32,
    store_id: Uuid,
    classes: Vec<ClassGrowth>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ClassGrowth {
    command_class: CargoCommandClass,
    samples: Vec<ByteSize>,
}

impl ReservationProfile {
    pub(super) fn empty(store_id: Uuid) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            store_id,
            classes: Vec::new(),
        }
    }

    pub(super) fn validate(&self, store_id: Uuid) -> Result<(), StoreError> {
        let mut classes = HashSet::new();
        let valid = self.schema_version == SCHEMA_VERSION
            && self.store_id == store_id
            && self.classes.len() <= 7
            && self.classes.iter().all(|profile| {
                profile.samples.len() <= SAMPLE_LIMIT && classes.insert(profile.command_class)
            });
        if valid {
            Ok(())
        } else {
            Err(StoreError::InvalidReservationProfile)
        }
    }

    pub(super) fn recommend(
        &self,
        command_class: CargoCommandClass,
        minimum: ByteSize,
    ) -> ByteSize {
        let Some(samples) = self.samples(command_class) else {
            return minimum;
        };
        let historical_p95 = percentile_95(samples);
        let previous = samples.last().copied().unwrap_or(ByteSize::ZERO);
        std::cmp::max(minimum, std::cmp::max(historical_p95, previous))
    }

    pub(super) fn record(&mut self, command_class: CargoCommandClass, growth: ByteSize) {
        if let Some(profile) = self
            .classes
            .iter_mut()
            .find(|profile| profile.command_class == command_class)
        {
            push_sample(&mut profile.samples, growth);
            return;
        }
        let mut samples = Vec::new();
        push_sample(&mut samples, growth);
        self.classes.push(ClassGrowth {
            command_class,
            samples,
        });
    }

    fn samples(&self, command_class: CargoCommandClass) -> Option<&[ByteSize]> {
        self.classes
            .iter()
            .find(|profile| profile.command_class == command_class)
            .map(|profile| profile.samples.as_slice())
    }
}

fn push_sample(samples: &mut Vec<ByteSize>, growth: ByteSize) {
    if samples.len() == SAMPLE_LIMIT {
        let _oldest = samples.remove(0);
    }
    samples.push(growth);
}

fn percentile_95(samples: &[ByteSize]) -> ByteSize {
    if samples.is_empty() {
        return ByteSize::ZERO;
    }
    let mut ordered = samples.to_vec();
    ordered.sort_unstable();
    let rank = ordered.len().saturating_mul(95).div_ceil(100);
    ordered[rank.saturating_sub(1)]
}
