use serde::{Deserialize, Serialize};

/// Bounded classification of a managed Cargo command.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CargoCommandClass {
    /// `cargo build`.
    Build,
    /// `cargo check`.
    Check,
    /// `cargo test`.
    Test,
    /// `cargo run`.
    Run,
    /// `cargo bench`.
    Bench,
    /// `cargo doc`.
    Doc,
    /// Another Cargo command whose raw name and arguments are not retained.
    #[default]
    Other,
}

/// Non-secret description of one managed Cargo invocation.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandDescriptor {
    /// Closed command classification suitable for history and reservation policy.
    pub command_class: CargoCommandClass,
    /// Deterministic fingerprint of the complete argument vector.
    pub arguments_fingerprint: String,
}

impl CommandDescriptor {
    /// Classifies and fingerprints arguments without retaining their contents.
    pub fn from_arguments(arguments: &[String], fingerprint_key: &[u8; 32]) -> Self {
        Self {
            command_class: Self::classify(arguments),
            arguments_fingerprint: keyed_fingerprint(arguments, fingerprint_key),
        }
    }

    /// Returns the bounded command class without retaining or hashing argument values.
    pub fn classify(arguments: &[String]) -> CargoCommandClass {
        command_class(arguments)
    }
}

fn keyed_fingerprint(arguments: &[String], key: &[u8; 32]) -> String {
    let mut hasher = blake3::Hasher::new_keyed(key);
    hasher.update(b"zhold-cargo-arguments-v2\0");
    for argument in arguments {
        let bytes = argument.as_bytes();
        hasher.update(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
        hasher.update(bytes);
    }
    hasher.finalize().to_hex()[..32].to_owned()
}

fn command_class(arguments: &[String]) -> CargoCommandClass {
    let mut index = usize::from(
        arguments
            .first()
            .is_some_and(|value| value.starts_with('+')),
    );
    while let Some(argument) = arguments.get(index) {
        if argument == "--" {
            return CargoCommandClass::Other;
        }
        if option_takes_value(argument) {
            index = index.saturating_add(2);
            continue;
        }
        if argument.starts_with('-') {
            index = index.saturating_add(1);
            continue;
        }
        return match argument.as_str() {
            "build" => CargoCommandClass::Build,
            "check" => CargoCommandClass::Check,
            "test" => CargoCommandClass::Test,
            "run" => CargoCommandClass::Run,
            "bench" => CargoCommandClass::Bench,
            "doc" => CargoCommandClass::Doc,
            _ => CargoCommandClass::Other,
        };
    }
    CargoCommandClass::Other
}

fn option_takes_value(argument: &str) -> bool {
    matches!(argument, "--color" | "--config" | "-C" | "-Z")
}
