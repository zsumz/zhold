//! Store opening and lifecycle operations.

use std::{
    env,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zhold_core::{ArenaId, ByteSize, CollectionPolicy};

use crate::{
    ArenaLease, BuildContext, CargoInvocation, CollectionReport, DoctorReport, Inventory,
    ScanReport, StoreError, TrashReport,
    collection::{collect, collect_locked, retry_trash},
    history::{CollectionReceiptSource, HistoryDraft, persist},
    inventory::read_inventory,
    io::{create_json, read_json, write_json},
    layout::StoreLayout,
    lock::ExclusiveFileLock,
    manifest::{ArenaManifest, StoreMarker},
    scan::scan,
    store::initialization::{
        ensure_layout, ensure_managed_directory, open_marker, prepare_arena_root,
        prepare_store_root,
    },
    time::{unix_milliseconds, unix_seconds},
    worktree::acquire_admission,
};

/// Opened marked zhold store.
#[derive(Clone, Debug)]
pub struct Store {
    pub(crate) layout: StoreLayout,
    pub(crate) marker: StoreMarker,
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
    /// Opens an existing marked store or initializes an empty directory.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let requested = root.as_ref();
        prepare_store_root(requested)?;
        let root = requested
            .canonicalize()
            .map_err(|error| StoreError::io("canonicalize store root", requested, error))?;
        let layout = StoreLayout::new(root);
        let marker = open_marker(&layout)?;
        ensure_layout(&layout)?;
        Ok(Self { layout, marker })
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
        let id = context.arena_id();
        let worktree = acquire_admission(self, context)?;
        let (_collection_lock, arena_lock) = self.admission_locks(id)?;
        let lease =
            self.lease_reserved_locked(context, invocation, reservation, (arena_lock, worktree))?;
        if self.has_adopted_quota()? {
            let aggregate_reservation = read_inventory(self)?.reserved;
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
        let id = context.arena_id();
        let worktree = acquire_admission(self, context)?;
        let (_collection_lock, arena_lock) = self.admission_locks(id)?;
        let mut lease =
            self.lease_reserved_locked(context, invocation, reservation, (arena_lock, worktree))?;
        let report = collect_locked(self, policy, false)?;
        self.verify_quota_admission(report.reserved)?;
        lease.queue_history(HistoryDraft::collection(
            &report,
            CollectionReceiptSource::Preflight,
        ));
        Ok((lease, report))
    }

    fn lease_reserved_locked(
        &self,
        context: &BuildContext,
        invocation: &CargoInvocation,
        reservation: ByteSize,
        admission: (ExclusiveFileLock, crate::worktree::WorktreeAdmission),
    ) -> Result<ArenaLease, StoreError> {
        let (arena_lock, worktree) = admission;
        let id = context.arena_id();
        let arena = self.layout.arena(id);
        let build_dir = self.layout.build_dir(id);
        let created = prepare_arena_root(&self.layout, &arena)?;
        ensure_managed_directory(self.layout.root(), &build_dir)?;
        let _metadata_lock = ExclusiveFileLock::acquire(&self.layout.metadata_lock(id))?;
        let manifest_path = self.layout.manifest(id);
        let command = invocation.descriptor();
        let now = unix_seconds()?;
        let mut manifest = if created {
            ArenaManifest::create(self.marker.store_id, context, now)
        } else {
            let manifest: ArenaManifest = read_json(&manifest_path)?;
            manifest.validate(self.marker.store_id, id, manifest_path.clone())?;
            manifest.validate_context(context, manifest_path.clone())?;
            manifest
        };
        manifest.begin(context, command, reservation, now);
        if created {
            if !create_json(&manifest_path, &manifest)? {
                return Err(StoreError::InvalidOwnership {
                    path: manifest_path,
                    reason: "new arena manifest appeared during initialization".to_owned(),
                });
            }
        } else {
            write_json(&manifest_path, &manifest)?;
        }
        let initial_bytes = crate::io::measure_tree(&arena)?;
        Ok(ArenaLease::new(
            self.clone(),
            context.arena_id().clone(),
            arena,
            build_dir,
            arena_lock,
            worktree,
            initial_bytes,
            unix_milliseconds()?,
        ))
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
        read_inventory(self)
    }

    /// Scans managed arenas and read-only foreign Cargo target directories.
    pub fn scan(&self, roots: &[PathBuf]) -> Result<ScanReport, StoreError> {
        scan(self, roots)
    }

    /// Plans or executes deterministic whole-arena collection.
    pub fn collect(
        &self,
        policy: CollectionPolicy,
        dry_run: bool,
    ) -> Result<CollectionReport, StoreError> {
        let mut report = collect(self, policy, dry_run)?;
        if !dry_run {
            report.history = persist(
                self,
                HistoryDraft::collection(&report, CollectionReceiptSource::Manual),
            );
        }
        Ok(report)
    }

    /// Retries deletion of already-retired, validated owned trash entries.
    pub fn retry_trash(&self, dry_run: bool) -> Result<TrashReport, StoreError> {
        let mut report = retry_trash(self, dry_run)?;
        if !dry_run {
            report.history = persist(self, HistoryDraft::trash(&report));
        }
        Ok(report)
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
