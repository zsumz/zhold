use std::fs;

use uuid::Uuid;
use zhold_core::ByteSize;

use crate::{
    ArenaLease, BuildContext, CargoInvocation, Store, StoreError,
    collection::reconcile_initializations_locked,
    io::{
        JsonCreation, JsonPublication, create_json, create_json_commit_aware, measure_tree,
        read_json, remove_json, sync_metadata_directory, write_json_commit_aware,
    },
    lock::ExclusiveFileLock,
    manifest::{ArenaManifest, InitializationRecord},
    store::initialization::ensure_managed_directory,
    time::{unix_milliseconds, unix_seconds},
    worktree::WorktreeAdmission,
};

impl Store {
    pub(super) fn lease_reserved_locked(
        &self,
        context: &BuildContext,
        invocation: &CargoInvocation,
        reservation: ByteSize,
        admission: (ExclusiveFileLock, WorktreeAdmission),
    ) -> Result<ArenaLease, StoreError> {
        reconcile_initializations_locked(self)?;
        let arena = self.layout.arena(context.arena_id());
        match fs::symlink_metadata(&arena) {
            Ok(_) => self.lease_existing(context, invocation, reservation, admission),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.lease_new(context, invocation, reservation, admission)
            }
            Err(error) => Err(StoreError::io("inspect managed arena", arena, error)),
        }
    }

    fn lease_existing(
        &self,
        context: &BuildContext,
        invocation: &CargoInvocation,
        reservation: ByteSize,
        admission: (ExclusiveFileLock, WorktreeAdmission),
    ) -> Result<ArenaLease, StoreError> {
        let id = context.arena_id();
        let arena = self.layout.arena(id);
        let build_dir = self.layout.build_dir(id);
        ensure_managed_directory(self.layout.root(), &arena)?;
        ensure_managed_directory(self.layout.root(), &build_dir)?;
        let initial_bytes = measure_tree(&arena)?;
        let started_at = unix_milliseconds()?;
        let now = unix_seconds()?;
        let metadata = ExclusiveFileLock::acquire(&self.layout.metadata_lock(id))?;
        let manifest_path = self.layout.manifest(id);
        let mut manifest: ArenaManifest = read_json(&manifest_path)?;
        manifest.validate(self.marker.store_id, id, manifest_path.clone())?;
        manifest.validate_context(context, manifest_path.clone())?;
        if manifest.is_unfinished() {
            return Err(StoreError::ArenaSuspect(id.to_string()));
        }
        manifest.begin(
            context,
            invocation.descriptor(self.marker.fingerprint_key()),
            reservation,
            now,
        );
        manifest.observe_size(initial_bytes);
        let publication = write_json_commit_aware(&manifest_path, &manifest)?;
        let lease = ArenaLease::new(
            self.clone(),
            id.clone(),
            arena,
            build_dir,
            admission.0,
            admission.1,
            initial_bytes,
            started_at,
        );
        drop(metadata);
        match publication {
            JsonPublication::VisibleButDurabilityUnconfirmed { error } => Err(error),
            JsonPublication::Durable { .. } => Ok(lease),
        }
    }

    fn lease_new(
        &self,
        context: &BuildContext,
        invocation: &CargoInvocation,
        reservation: ByteSize,
        admission: (ExclusiveFileLock, WorktreeAdmission),
    ) -> Result<ArenaLease, StoreError> {
        let id = context.arena_id();
        let final_arena = self.layout.arena(id);
        let prefix = final_arena
            .parent()
            .ok_or_else(|| StoreError::InvalidOwnership {
                path: final_arena.clone(),
                reason: "arena path has no prefix directory".to_owned(),
            })?;
        ensure_managed_directory(self.layout.root(), prefix)?;
        let initialization_id = Uuid::new_v4();
        let record = InitializationRecord::create(self, id.clone(), initialization_id);
        let record_path = self.layout.initialization_record(initialization_id);
        if !create_json(&record_path, &record)? {
            return Err(StoreError::InvalidOwnership {
                path: record_path,
                reason: "arena initialization journal nonce already exists".to_owned(),
            });
        }
        let staging = record.staging_path().to_path_buf();
        fs::create_dir(&staging)
            .map_err(|error| StoreError::io("create staged arena", &staging, error))?;
        ensure_managed_directory(self.layout.root(), &staging)?;
        let build_dir = staging.join("build");
        ensure_managed_directory(self.layout.root(), &build_dir)?;
        let initial_bytes = measure_tree(&staging)?;
        let started_at = unix_milliseconds()?;
        let now = unix_seconds()?;
        let metadata = ExclusiveFileLock::acquire(&self.layout.metadata_lock(id))?;
        let mut manifest =
            ArenaManifest::create(self.marker.store_id, context, Some(initialization_id), now);
        manifest.begin(
            context,
            invocation.descriptor(self.marker.fingerprint_key()),
            reservation,
            now,
        );
        manifest.observe_size(initial_bytes);
        let manifest_path = staging.join("arena.json");
        let publication = create_json_commit_aware(&manifest_path, &manifest)?;
        let mut lease = ArenaLease::new(
            self.clone(),
            id.clone(),
            staging.clone(),
            build_dir,
            admission.0,
            admission.1,
            initial_bytes,
            started_at,
        );
        drop(metadata);
        match publication {
            JsonCreation::Existing => {
                return Err(StoreError::InvalidOwnership {
                    path: manifest_path,
                    reason: "staged arena manifest unexpectedly existed".to_owned(),
                });
            }
            JsonCreation::Published(JsonPublication::VisibleButDurabilityUnconfirmed { error }) => {
                return Err(error);
            }
            JsonCreation::Published(JsonPublication::Durable { .. }) => {}
        }
        fs::rename(&staging, &final_arena)
            .map_err(|error| StoreError::io("promote initialized arena", &staging, error))?;
        lease.promote_paths(final_arena.clone(), final_arena.join("build"));
        sync_metadata_directory(&final_arena)?;
        remove_json(&record_path)?;
        Ok(lease)
    }
}
