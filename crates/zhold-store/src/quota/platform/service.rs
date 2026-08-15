use std::path::Path;

use zhold_core::{ByteSize, QuotaHealth, QuotaProvider};

use super::super::{QuotaAction, QuotaObservation, QuotaPlan};

pub(crate) fn inspect(root: &Path, requested: QuotaProvider) -> QuotaObservation {
    #[cfg(target_os = "linux")]
    {
        return super::linux::inspect(root, requested);
    }
    #[cfg(target_os = "macos")]
    {
        return super::macos::inspect(root, requested);
    }
    #[cfg(target_os = "windows")]
    {
        return super::windows::inspect(root, requested);
    }
    #[allow(unreachable_code)]
    QuotaObservation::unavailable(
        requested,
        root.to_path_buf(),
        QuotaHealth::Unsupported,
        "no store-scoped quota provider is implemented for this platform",
    )
}

pub(crate) fn plan(root: &Path, hard_limit: ByteSize, provider: QuotaProvider) -> QuotaPlan {
    let observation = inspect(root, provider);
    let provider_name = observation.provider.to_string();
    QuotaPlan {
        hard_limit,
        requirements: vec![
            "the quota scope must exactly equal the canonical zhold store root".to_owned(),
            "the quota must be hard-enforcing and externally provisioned".to_owned(),
            "provider accounting must be enabled and consistent".to_owned(),
        ],
        actions: vec![
            QuotaAction {
                order: 1,
                description: format!(
                    "provision a {provider_name} hard quota of {hard_limit} for {} using platform administration tools",
                    root.display()
                ),
                privilege_required: true,
                program: provisioning_program(observation.provider).map(ToOwned::to_owned),
                arguments: provisioning_arguments(observation.provider, root, hard_limit),
            },
            QuotaAction {
                order: 2,
                description: format!(
                    "verify enforcement, then run `zhold quota adopt {}`",
                    hard_limit.as_u64()
                ),
                privilege_required: false,
                program: Some("zhold".to_owned()),
                arguments: vec![
                    "quota".to_owned(),
                    "adopt".to_owned(),
                    hard_limit.as_u64().to_string(),
                    "--provider".to_owned(),
                    observation.provider.to_string(),
                ],
            },
        ],
        observation,
    }
}

const fn provisioning_program(provider: QuotaProvider) -> Option<&'static str> {
    match provider {
        QuotaProvider::BtrfsQgroup => Some("btrfs"),
        _ => None,
    }
}

fn provisioning_arguments(
    provider: QuotaProvider,
    root: &Path,
    hard_limit: ByteSize,
) -> Vec<String> {
    match provider {
        QuotaProvider::BtrfsQgroup => vec![
            "qgroup".to_owned(),
            "limit".to_owned(),
            hard_limit.as_u64().to_string(),
            root.display().to_string(),
        ],
        _ => Vec::new(),
    }
}
