//! Human and structured presentation.

mod cargo;
mod collection;
mod config;
mod doctor;
mod explain;
mod history;
#[cfg(feature = "experimental")]
mod hook;
mod inventory;
mod json;
mod output;
mod quota;
mod recovery;
mod scan;

pub(crate) use cargo::{
    cargo_finalization_failed, cargo_finish_with_history, cargo_management_failed, cargo_start,
};
pub(crate) use collection::{collection, pin, post_build, preflight, trash};
pub(crate) use config::setup;
pub(crate) use doctor::doctor;
pub(crate) use explain::explain;
pub(crate) use history::{finalization as history_finalization, warnings as history_warnings};
#[cfg(feature = "experimental")]
pub(crate) use history::{policy as history_policy, pruned as history_pruned, report as history};
#[cfg(feature = "experimental")]
pub(crate) use hook::hook;
pub(crate) use inventory::inventory;
pub(crate) use quota::post_build as quota_post_build;
#[cfg(feature = "experimental")]
pub(crate) use quota::{adoption as quota_adoption, plan as quota_plan, status as quota_status};
pub(crate) use recovery::recovery;
pub(crate) use scan::scan;
