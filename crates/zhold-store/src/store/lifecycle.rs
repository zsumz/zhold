use zhold_core::ArenaId;

use crate::{
    Store, StoreError,
    io::{read_json, write_json},
    lock::ExclusiveFileLock,
    manifest::ArenaManifest,
};

#[derive(Clone, Copy)]
pub(super) enum LifecycleTransition {
    Spawning,
    Spawned,
}

impl Store {
    pub(super) fn transition_lifecycle(
        &self,
        id: &ArenaId,
        transition: LifecycleTransition,
    ) -> Result<(), StoreError> {
        self.ensure_writable("transition arena lifecycle")?;
        let _metadata = ExclusiveFileLock::acquire(&self.layout.metadata_lock(id))?;
        let path = self.layout.manifest(id);
        let mut manifest: ArenaManifest = read_json(&path)?;
        manifest.validate(self.marker.store_id, id, path.clone())?;
        match transition {
            LifecycleTransition::Spawning => manifest.mark_spawning()?,
            LifecycleTransition::Spawned => manifest.mark_spawned()?,
        }
        write_json(&path, &manifest)
    }
}
