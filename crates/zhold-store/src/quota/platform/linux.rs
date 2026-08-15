use std::{path::Path, process::Command};

use serde_json::Value;
use zhold_core::{ByteSize, QuotaHealth, QuotaProvider};

use super::super::QuotaObservation;

pub(super) fn inspect(root: &Path, requested: QuotaProvider) -> QuotaObservation {
    let mount = match mount_info(root) {
        Ok(value) => value,
        Err(detail) => {
            return QuotaObservation::unavailable(
                requested,
                root.to_path_buf(),
                QuotaHealth::ProviderUnavailable,
                detail,
            );
        }
    };
    let detected = match mount.filesystem.as_str() {
        "xfs" => QuotaProvider::XfsProject,
        "ext4" => QuotaProvider::Ext4Project,
        "btrfs" => QuotaProvider::BtrfsQgroup,
        _ => {
            return QuotaObservation::unavailable(
                requested,
                root.to_path_buf(),
                QuotaHealth::Unsupported,
                format!(
                    "{} has no supported store-scoped provider",
                    mount.filesystem
                ),
            );
        }
    };
    let provider = if requested == QuotaProvider::Auto {
        detected
    } else {
        requested
    };
    if provider != detected {
        return QuotaObservation::unavailable(
            provider,
            root.to_path_buf(),
            QuotaHealth::Unsupported,
            format!("requested provider does not match {}", mount.filesystem),
        );
    }
    match provider {
        QuotaProvider::BtrfsQgroup => btrfs(root, &mount),
        QuotaProvider::XfsProject => super::linux_project::xfs(root, &mount.target, mount.identity),
        QuotaProvider::Ext4Project => {
            super::linux_project::ext4(root, &mount.target, mount.identity)
        }
        _ => QuotaObservation::unavailable(
            provider,
            root.to_path_buf(),
            QuotaHealth::Unsupported,
            "provider is not implemented on Linux",
        ),
    }
}

#[derive(Debug)]
struct MountInfo {
    filesystem: String,
    identity: Option<String>,
    target: std::path::PathBuf,
}

fn mount_info(root: &Path) -> Result<MountInfo, String> {
    let output = Command::new("findmnt")
        .args(["--json", "--target"])
        .arg(root)
        .args(["--output", "FSTYPE,SOURCE,TARGET,UUID"])
        .output()
        .map_err(|error| format!("could not start findmnt: {error}"))?;
    if !output.status.success() {
        return Err("findmnt could not inspect the store filesystem".to_owned());
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("findmnt returned invalid JSON: {error}"))?;
    let item = value
        .get("filesystems")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .ok_or_else(|| "findmnt returned no containing filesystem".to_owned())?;
    let filesystem = item
        .get("fstype")
        .and_then(Value::as_str)
        .ok_or_else(|| "findmnt omitted the filesystem type".to_owned())?
        .to_owned();
    let identity = item
        .get("uuid")
        .or_else(|| item.get("source"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let target = item
        .get("target")
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from)
        .filter(|value| value.is_absolute())
        .ok_or_else(|| "findmnt omitted an absolute mount target".to_owned())?;
    Ok(MountInfo {
        filesystem,
        identity,
        target,
    })
}

fn btrfs(root: &Path, mount: &MountInfo) -> QuotaObservation {
    let subvolume = Command::new("btrfs")
        .args(["subvolume", "show"])
        .arg(root)
        .output();
    let Ok(subvolume) = subvolume else {
        return unavailable_btrfs(root, mount, "btrfs administration tool is unavailable");
    };
    if !subvolume.status.success() {
        return unavailable_btrfs(root, mount, "store root is not a dedicated Btrfs subvolume");
    }
    let text = String::from_utf8_lossy(&subvolume.stdout);
    let Some(identifier) = line_value(&text, "Subvolume ID:") else {
        return unavailable_btrfs(root, mount, "Btrfs subvolume identity was not parseable");
    };
    let qgroup_id = format!("0/{identifier}");
    let qgroups = Command::new("btrfs")
        .args(["qgroup", "show", "--raw", "-reF"])
        .arg(root)
        .output();
    let Ok(qgroups) = qgroups else {
        return unavailable_btrfs(root, mount, "btrfs qgroup inspection is unavailable");
    };
    if !qgroups.status.success() {
        return unavailable_btrfs(
            root,
            mount,
            "Btrfs qgroup accounting is disabled or inconsistent",
        );
    }
    let table = String::from_utf8_lossy(&qgroups.stdout);
    let Some((usage, limit)) = qgroup_values(&table, &qgroup_id) else {
        return unavailable_btrfs(
            root,
            mount,
            "store subvolume has no verifiable qgroup limit",
        );
    };
    QuotaObservation {
        provider: QuotaProvider::BtrfsQgroup,
        health: QuotaHealth::Configured,
        scope: root.to_path_buf(),
        filesystem_id: mount.identity.clone(),
        quota_id: Some(qgroup_id),
        exact_scope: true,
        hard_enforcement: true,
        usage: Some(ByteSize::from_bytes(usage)),
        limit: Some(ByteSize::from_bytes(limit)),
        detail: "dedicated Btrfs subvolume qgroup limit is configured".to_owned(),
    }
}

fn unavailable_btrfs(root: &Path, mount: &MountInfo, detail: &str) -> QuotaObservation {
    let mut observation = QuotaObservation::unavailable(
        QuotaProvider::BtrfsQgroup,
        root.to_path_buf(),
        QuotaHealth::AvailableUnconfigured,
        detail,
    );
    observation.filesystem_id.clone_from(&mount.identity);
    observation
}

fn line_value<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(prefix).map(str::trim))
}

fn qgroup_values(text: &str, identity: &str) -> Option<(u64, u64)> {
    text.lines().find_map(|line| {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.first().copied() != Some(identity) || columns.len() < 5 {
            return None;
        }
        let usage = columns.get(1)?.parse::<u64>().ok()?;
        let limit = columns.get(3)?.parse::<u64>().ok()?;
        (limit > 0).then_some((usage, limit))
    })
}
