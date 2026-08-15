use std::{path::Path, process::Command};

use serde_json::Value;
use zhold_core::{ByteSize, QuotaHealth, QuotaProvider};

use super::super::QuotaObservation;

const INSPECT_SCRIPT: &str = "$ErrorActionPreference='Stop'; $s=(Resolve-Path -LiteralPath $env:ZHOLD_QUOTA_SCOPE).ProviderPath; $q=Get-FsrmQuota -Path $s; $v=Get-Volume -FilePath $s; $qp=[IO.Path]::GetFullPath($q.Path).TrimEnd('\\'); $sp=[IO.Path]::GetFullPath($s).TrimEnd('\\'); [pscustomobject]@{Path=$q.Path;ExactPath=[StringComparer]::OrdinalIgnoreCase.Equals($qp,$sp);Size=$q.Size;Usage=$q.Usage;SoftLimit=$q.SoftLimit;Enabled=(-not $q.Disabled);QuotaId=$q.Path;FilesystemId=$v.UniqueId}|ConvertTo-Json -Compress";

pub(super) fn inspect(root: &Path, requested: QuotaProvider) -> QuotaObservation {
    let provider = match requested {
        QuotaProvider::Auto | QuotaProvider::Fsrm => QuotaProvider::Fsrm,
        other => {
            return QuotaObservation::unavailable(
                other,
                root.to_path_buf(),
                QuotaHealth::Unsupported,
                "requested quota provider is not available on Windows",
            );
        }
    };
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", INSPECT_SCRIPT])
        .env("ZHOLD_QUOTA_SCOPE", root)
        .output();
    let Ok(output) = output else {
        return QuotaObservation::unavailable(
            provider,
            root.to_path_buf(),
            QuotaHealth::Unsupported,
            "Windows Server FSRM PowerShell capability is unavailable",
        );
    };
    if !output.status.success() {
        return QuotaObservation::unavailable(
            provider,
            root.to_path_buf(),
            QuotaHealth::AvailableUnconfigured,
            "no inspectable exact-path FSRM quota is configured",
        );
    }
    let Ok(value) = serde_json::from_slice::<Value>(&output.stdout) else {
        return QuotaObservation::unavailable(
            provider,
            root.to_path_buf(),
            QuotaHealth::ProviderUnavailable,
            "FSRM returned metadata that could not be parsed",
        );
    };
    from_value(root, provider, &value)
}

pub(super) fn from_value(root: &Path, provider: QuotaProvider, value: &Value) -> QuotaObservation {
    let path = value
        .get("Path")
        .and_then(Value::as_str)
        .map_or_else(|| root.to_path_buf(), std::path::PathBuf::from);
    let limit = value
        .get("Size")
        .and_then(Value::as_u64)
        .map(ByteSize::from_bytes);
    let usage = value
        .get("Usage")
        .and_then(Value::as_u64)
        .map(ByteSize::from_bytes);
    let soft = value
        .get("SoftLimit")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let enabled = value
        .get("Enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let exact = value
        .get("ExactPath")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let scope = if exact { root.to_path_buf() } else { path };
    let configured = exact && enabled && !soft && limit.is_some_and(|value| value.as_u64() > 0);
    QuotaObservation {
        provider,
        health: if configured {
            QuotaHealth::Configured
        } else {
            QuotaHealth::AvailableUnconfigured
        },
        scope,
        filesystem_id: value
            .get("FilesystemId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        quota_id: value
            .get("QuotaId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        exact_scope: exact,
        hard_enforcement: configured,
        usage,
        limit,
        detail: if configured {
            "exact-path Windows Server FSRM hard quota is configured".to_owned()
        } else {
            "FSRM quota is absent, disabled, soft, inherited, or broader than the store".to_owned()
        },
    }
}
