use std::io::{self, Write};

use serde::Serialize;
use zhold_core::ArenaId;

use super::output::output_error;
use crate::{CliError, app::OutputFormat, render::json};

pub(crate) fn recovery(arena: &ArenaId, format: OutputFormat) -> Result<(), CliError> {
    if matches!(format, OutputFormat::Json) {
        #[derive(Serialize)]
        struct Recovery<'a> {
            event: &'static str,
            arena_id: &'a ArenaId,
            outcome: &'static str,
        }
        return json::write(&Recovery {
            event: "arena_recovered",
            arena_id: arena,
            outcome: "terminated",
        });
    }
    writeln!(
        io::stdout().lock(),
        "recovered  {} as terminated",
        arena.as_str().get(..10).unwrap_or(arena.as_str())
    )
    .map_err(output_error)
}
