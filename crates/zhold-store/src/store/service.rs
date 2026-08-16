//! Store opening and lifecycle operations.

use std::{env, path::PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::{ArenaId, ByteSize, CollectionPolicy};

use crate::{
    ArenaLease, BuildContext, CargoInvocation, CollectionReport, DoctorReport, Inventory,
    ScanReport, StoreError,
    collection::collect_locked,
    history::{CollectionReceiptSource, HistoryDraft},
    inventory::{ArenaMeasurement, read_arena_snapshot, read_inventory},
    io::{read_json, write_json},
    layout::StoreLayout,
    lock::ExclusiveFileLock,
    manifest::{ArenaManifest, StoreMarker},
    scan::scan,
    time::unix_seconds,
    worktree::acquire_admission,
};

/// Opened marked zhold store.
#[derive(Clone, Debug)]
pub struct Store {
    pub(crate) layout: StoreLayout,
    pub(crate) marker: StoreMarker,
    pub(crate) read_only: bool,
}

/// Stable metadata for an opened store.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StoreInfo {
    /// Stable store identity.
    pub store_id: Uuid,
    /// Canonical store root.
    pub root: PathBuf,
}

impl Store {
    pub(crate) fn ensure_writable(&self, operation: &'static str) -> Result<(), StoreError> {
        if self.read_only {
            Err(StoreError::ReadOnly { operation })
        } else {
            Ok(())
        }
    }

    pub(crate) fn probe_lock(
        &self,
        path: &std::path::Path,
    ) -> Result<crate::lock::LockState, StoreError> {
        if self.read_only {
            ExclusiveFileLock::probe_read_only(path)
        } else {
            ExclusiveFileLock::probe(path)
        }
    }

    /// Derives the platform cache location used when no explicit store is configured.
    pub fn default_root() -> Result<PathBuf, StoreError> {
        if let Some(value) = env::var_os("ZHOLD_HOME") {
            return Ok(PathBuf::from(value));
        }
        if cfg!(target_os = "macos") {
            return env::var_os("HOME")
                .map(|home| PathBuf::from(home).join("Library/Caches/zhold"))
                .ok_or(StoreError::MissingCacheRoot);
        }
        if cfg!(target_os = "windows") {
            return env::var_os("LOCALAPPDATA")
                .map(|root| PathBuf::from(root).join("zhold"))
                .ok_or(StoreError::MissingCacheRoot);
        }
        if let Some(value) = env::var_os("XDG_CACHE_HOME") {
            return Ok(PathBuf::from(value).join("zhold"));
        }
        env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".cache/zhold"))
            .ok_or(StoreError::MissingCacheRoot)
    }

    /// Returns stable store metadata.
    pub fn info(&self) -> StoreInfo {
        StoreInfo {
            store_id: self.marker.store_id,
            root: self.layout.root().to_path_buf(),
        }
    }

    /// Returns bytes currently available to the store filesystem user.
    pub fn available_space(&self) -> Result<ByteSize, StoreError> {
        fs2::available_space(self.layout.root())
            .map(ByteSize::from_bytes)
            .map_err(|error| {
                StoreError::io("measure available store space", self.layout.root(), error)
            })
    }

    /// Resolves a Cargo invocation using this store's private compatibility key.
    pub fn resolve_context(
        &self,
        invocation: &CargoInvocation,
    ) -> Result<BuildContext, StoreError> {
        crate::ContextResolver::resolve(invocation, self.marker.fingerprint_key())
    }

    /// Acquires the arena lease and records the beginning of a managed Cargo command.
    pub fn lease(
        &self,
        context: &BuildContext,
        invocation: &CargoInvocation,
    ) -> Result<ArenaLease, StoreError> {
        self.lease_reserved(context, invocation, ByteSize::ZERO)
    }

    /// Acquires an arena lease with declared additional build-growth headroom.
    pub fn lease_reserved(
        &self,
        context: &BuildContext,
        invocation: &CargoInvocation,
        reservation: ByteSize,
    ) -> Result<ArenaLease, StoreError> {
        self.ensure_writable("acquire an arena lease")?;
        let id = context.arena_id();
        let worktree = acquire_admission(self, context)?;
        let (_collection_lock, arena_lock) = self.admission_locks(id)?;
        let lease =
            self.lease_reserved_locked(context, invocation, reservation, (arena_lock, worktree))?;
        if self.has_adopted_quota()? {
            let aggregate_reservation =
                read_arena_snapshot(self, ArenaMeasurement::Cached)?.reserved;
            self.verify_quota_admission(aggregate_reservation)?;
        }
        Ok(lease)
    }

    /// Acquires a reserved lease and performs collection as one serialized admission.
    pub fn lease_reserved_and_collect(
        &self,
        context: &BuildContext,
        invocation: &CargoInvocation,
        reservation: ByteSize,
        policy: CollectionPolicy,
    ) -> Result<(ArenaLease, CollectionReport), StoreError> {
        self.ensure_writable("acquire an arena lease and collect")?;
        let id = context.arena_id();
        let worktree = acquire_admission(self, context)?;
        let (_collection_lock, arena_lock) = self.admission_locks(id)?;
        let mut lease =
            self.lease_reserved_locked(context, invocation, reservation, (arena_lock, worktree))?;
        let report = collect_locked(self, policy, false, ArenaMeasurement::Cached)?;
        self.verify_quota_admission(report.reserved)?;
        lease.queue_history(HistoryDraft::collection(
            &report,
            CollectionReceiptSource::Preflight,
        ));
        Ok((lease, report))
    }

    fn admission_locks(
        &self,
        id: &ArenaId,
    ) -> Result<(ExclusiveFileLock, ExclusiveFileLock), StoreError> {
        loop {
            let collection = ExclusiveFileLock::acquire(&self.layout.collection_lock())?;
            if let Some(arena) = ExclusiveFileLock::try_acquire(&self.layout.arena_lock(id))? {
                return Ok((collection, arena));
            }
            drop(collection);
            let arena = ExclusiveFileLock::acquire(&self.layout.arena_lock(id))?;
            drop(arena);
        }
    }

    /// Reads all valid managed arenas without mutating them.
    pub fn inventory(&self) -> Result<Inventory, StoreError> {
        read_inventory(self, ArenaMeasurement::Deep)
    }

    /// Reads a metadata-only inventory without traversing arena or trash trees.
    pub fn inventory_cached(&self) -> Result<Inventory, StoreError> {
        read_inventory(self, ArenaMeasurement::Cached)
    }

    /// Scans managed arenas and read-only foreign Cargo target directories.
    pub fn scan(&self, roots: &[PathBuf]) -> Result<ScanReport, StoreError> {
        scan(self, roots)
    }

    /// Pins or unpins a managed arena.
    pub fn set_pinned(&self, id: &ArenaId, pinned: bool) -> Result<(), StoreError> {
        self.update_pin(id, pinned, None)
    }

    /// Pins a managed arena forever or for a finite number of seconds.
    pub fn pin_for(&self, id: &ArenaId, seconds: Option<u64>) -> Result<Option<u64>, StoreError> {
        let expires_at = match seconds {
            Some(seconds) => Some(
                unix_seconds()?
                    .checked_add(seconds)
                    .ok_or(StoreError::PinExpirationOverflow)?,
            ),
            None => None,
        };
        self.update_pin(id, true, expires_at)?;
        Ok(expires_at)
    }

    fn update_pin(
        &self,
        id: &ArenaId,
        pinned: bool,
        expires_at: Option<u64>,
    ) -> Result<(), StoreError> {
        self.ensure_writable("update an arena pin")?;
        let arena = self.layout.arena(id);
        if !arena.exists() {
            return Err(StoreError::ArenaNotFound(id.to_string()));
        }
        let canonical_root = self.layout.root().canonicalize().map_err(|error| {
            StoreError::io("canonicalize store root", self.layout.root(), error)
        })?;
        crate::inventory::ensure_real_contained_directory(&arena, &canonical_root)?;
        let manifest_path = self.layout.manifest(id);
        let _metadata_lock = ExclusiveFileLock::acquire(&self.layout.metadata_lock(id))?;
        let mut manifest: ArenaManifest = read_json(&manifest_path)?;
        manifest.validate(self.marker.store_id, id, manifest_path.clone())?;
        manifest.set_pin(pinned, expires_at);
        write_json(&manifest_path, &manifest)
    }

    /// Validates the marker, inventory, ownership findings, and retirement backlog.
    pub fn doctor(&self) -> Result<DoctorReport, StoreError> {
        DoctorReport::inspect(self)
    }
}
