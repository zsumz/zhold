use std::{path::Path, str::FromStr};

use zhold_core::{ArenaId, ByteSize};

use crate::{Store, io::read_json, layout::StoreLayout, lock::LockState, manifest::ArenaManifest};

pub(super) fn indexed_arena_id(layout: &StoreLayout, arena_path: &Path) -> Option<ArenaId> {
    let name = arena_path.file_name()?.to_str()?;
    let arena_id = ArenaId::from_str(name).ok()?;
    (layout.arena(&arena_id).as_path() == arena_path).then_some(arena_id)
}

pub(super) fn recover_active_reservation(store: &Store, arena_path: &Path) -> ByteSize {
    let Some(arena_id) = indexed_arena_id(&store.layout, arena_path) else {
        return ByteSize::ZERO;
    };
    let manifest_path = store.layout.manifest(&arena_id);
    let Ok(manifest) = read_json::<ArenaManifest>(&manifest_path) else {
        return ByteSize::ZERO;
    };
    if manifest
        .validate(store.marker.store_id, &arena_id, manifest_path)
        .is_err()
    {
        return ByteSize::ZERO;
    }
    if matches!(
        store.probe_lock(&store.layout.arena_lock(&arena_id)),
        Ok(LockState::Held)
    ) {
        manifest.reservation
    } else {
        ByteSize::ZERO
    }
}
