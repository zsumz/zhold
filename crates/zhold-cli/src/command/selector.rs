use zhold_core::ArenaId;
use zhold_store::{Inventory, Store};

use crate::CliError;

const MINIMUM_PREFIX: usize = 6;

pub(super) fn resolve(store: &Store, selector: &str) -> Result<ArenaId, CliError> {
    let inventory = store.inventory()?;
    resolve_inventory(&inventory, selector)
}

pub(super) fn resolve_inventory(
    inventory: &Inventory,
    selector: &str,
) -> Result<ArenaId, CliError> {
    validate(selector)?;
    select(
        selector,
        inventory.arenas.iter().map(|entry| &entry.record.id),
    )
}

fn validate(selector: &str) -> Result<(), CliError> {
    if selector.len() < MINIMUM_PREFIX {
        return Err(CliError::ArenaSelectorTooShort {
            selector: selector.to_owned(),
            minimum: MINIMUM_PREFIX,
        });
    }
    if selector.len() > 32
        || !selector
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CliError::InvalidArenaSelector(selector.to_owned()));
    }
    Ok(())
}

fn select<'a>(selector: &str, ids: impl Iterator<Item = &'a ArenaId>) -> Result<ArenaId, CliError> {
    let matches = ids
        .filter(|id| id.as_str().starts_with(selector))
        .cloned()
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Err(CliError::ArenaSelectorNotFound(selector.to_owned())),
        [selected] => Ok(selected.clone()),
        _ => Err(CliError::ArenaSelectorAmbiguous {
            selector: selector.to_owned(),
            count: matches.len(),
        }),
    }
}

#[cfg(test)]
pub(super) fn select_for_test(selector: &str, ids: &[ArenaId]) -> Result<ArenaId, CliError> {
    validate(selector)?;
    select(selector, ids.iter())
}
