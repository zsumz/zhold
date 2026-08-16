//! Private persisted metadata schema.

mod arena_lifecycle;
mod arena_manifest;
mod initialization_record;
mod retirement_record;
mod store_marker;

pub(crate) use arena_lifecycle::ArenaLifecycleStage;
pub(crate) use arena_manifest::ArenaManifest;
pub(crate) use initialization_record::InitializationRecord;
pub(crate) use retirement_record::RetirementRecord;
pub(crate) use store_marker::{STORE_SCHEMA_VERSION, StoreMarker};
