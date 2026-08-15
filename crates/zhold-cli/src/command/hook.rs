use zhold_store::{ContextResolver, HookMetadata, Store};

use crate::{
    CliError,
    app::{ExitStatus, HookCommand, OutputFormat},
    render,
};

pub(super) fn execute(
    store: &Store,
    action: HookCommand,
    format: OutputFormat,
) -> Result<ExitStatus, CliError> {
    let report = match action {
        HookCommand::Ready {
            path,
            manager,
            label,
            session,
        } => {
            let context = ContextResolver::resolve_worktree(&path)?;
            store.hook_ready(
                &context,
                HookMetadata {
                    manager,
                    label,
                    session,
                },
            )?
        }
        HookCommand::PrepareRemove { path, manager } => {
            store.hook_prepare_remove(&path, manager)?
        }
        HookCommand::Removed { path, manager } => store.hook_removed(&path, manager)?,
        HookCommand::CancelRemove { path, manager } => {
            let context = ContextResolver::resolve_worktree(&path)?;
            store.hook_cancel_remove(&context, manager)?
        }
    };
    let attention = report.attention_required();
    render::hook(&report, format)?;
    Ok(if attention {
        ExitStatus::child(2)
    } else {
        ExitStatus::SUCCESS
    })
}
