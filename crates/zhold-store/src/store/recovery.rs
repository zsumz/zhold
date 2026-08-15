use zhold_core::{ArenaId, BuildOutcome};

use crate::{
    Store, StoreError,
    inventory::ensure_real_contained_directory,
    io::{measure_tree, read_json, write_json},
    lock::ExclusiveFileLock,
    manifest::ArenaManifest,
    time::unix_seconds,
};

impl Store {
    /// Marks a suspect build terminated after the caller confirms its process tree is gone.
    pub fn recover_suspect(&self, id: &ArenaId) -> Result<(), StoreError> {
        let _collection = ExclusiveFileLock::acquire(&self.layout.collection_lock())?;
        let Some(_arena_lock) = ExclusiveFileLock::try_acquire(&self.layout.arena_lock(id))? else {
            return Err(StoreError::ArenaActive(id.to_string()));
        };
        let _metadata = ExclusiveFileLock::acquire(&self.layout.metadata_lock(id))?;
        let arena = self.layout.arena(id);
        if !arena.exists() {
            return Err(StoreError::ArenaNotFound(id.to_string()));
        }
        let root = self.layout.root().canonicalize().map_err(|error| {
            StoreError::io("canonicalize store root", self.layout.root(), error)
        })?;
        ensure_real_contained_directory(&arena, &root)?;
        let manifest_path = self.layout.manifest(id);
        let mut manifest: ArenaManifest = read_json(&manifest_path)?;
        manifest.validate(self.marker.store_id, id, manifest_path.clone())?;
        if !manifest.is_unfinished() {
            return Err(StoreError::ArenaNotSuspect(id.to_string()));
        }
        let measured = measure_tree(&arena).ok();
        let high_water = measured
            .unwrap_or(manifest.last_known_size)
            .max(manifest.last_observed_size)
            .max(manifest.last_known_size);
        manifest.finish(
            BuildOutcome::Terminated,
            high_water,
            measured,
            unix_seconds()?,
        );
        write_json(&manifest_path, &manifest)
    }
}
