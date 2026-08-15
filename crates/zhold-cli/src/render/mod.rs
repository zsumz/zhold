//! Human and structured presentation.

mod cargo;
mod collection;
mod doctor;
mod explain;
mod history;
mod hook;
mod inventory;
mod json;
mod output;
mod quota;
mod scan;

pub(crate) use cargo::{
    cargo_finalization_failed, cargo_finish_with_history, cargo_management_failed,
    cargo_size_limit_exceeded, cargo_start,
};
pub(crate) use collection::{collection, pin, post_build, preflight, trash};
pub(crate) use doctor::doctor;
pub(crate) use explain::explain;
pub(crate) use history::{
    finalization as history_finalization, policy as history_policy, pruned as history_pruned,
    report as history,
};
pub(crate) use hook::hook;
pub(crate) use inventory::inventory;
pub(crate) use quota::{
    adoption as quota_adoption, plan as quota_plan, post_build as quota_post_build,
    status as quota_status,
};
pub(crate) use scan::scan;
