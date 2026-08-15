use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde_json::Value;
use zhold_core::{ByteSize, QuotaHealth, QuotaProvider};

use super::super::QuotaObservation;

pub(super) fn inspect(root: &Path, requested: QuotaProvider) -> QuotaObservation {
    let provider = match requested {
        QuotaProvider::Auto | QuotaProvider::ApfsVolume => QuotaProvider::ApfsVolume,
        other => {
            return QuotaObservation::unavailable(
                other,
                root.to_path_buf(),
                QuotaHealth::Unsupported,
                "requested quota provider is not available on macOS",
            );
        }
    };
    match disk_info(root) {
        Ok(info) => observation(root, provider, &info),
        Err(detail) => QuotaObservation::unavailable(
            provider,
            root.to_path_buf(),
            QuotaHealth::ProviderUnavailable,
            detail,
        ),
    }
}

fn disk_info(root: &Path) -> Result<Value, String> {
    let device = containing_device(root)?;
    let output = Command::new("/usr/sbin/diskutil")
        .args(["info", "-plist"])
        .arg(device)
        .output()
        .map_err(|error| format!("could not start diskutil: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "diskutil inspection failed{}",
            status_suffix(output.status.code())
        ));
    }
    let mut info = convert_plist(&output.stdout)?;
    let filesystem = text(
        &info,
        &["FilesystemType", "FilesystemName", "FileSystemPersonality"],
    );
    if !filesystem.is_some_and(|value| value.to_ascii_lowercase().contains("apfs")) {
        return Ok(info);
    }
    let container = text(&info, &["APFSContainerReference"])
        .ok_or_else(|| "diskutil omitted the APFS container identity".to_owned())?;
    let volume_id = text(&info, &["DeviceIdentifier"])
        .ok_or_else(|| "diskutil omitted the APFS volume device identity".to_owned())?;
    let list = Command::new("/usr/sbin/diskutil")
        .args(["apfs", "list", "-plist", container])
        .output()
        .map_err(|error| format!("could not inspect the APFS container: {error}"))?;
    if !list.status.success() {
        return Err("diskutil could not inspect the APFS container".to_owned());
    }
    let topology = convert_plist(&list.stdout)?;
    let volume = apfs_volume(&topology, volume_id)
        .ok_or_else(|| "APFS topology omitted the mounted volume".to_owned())?;
    let Some(details) = info.as_object_mut() else {
        return Err("diskutil info property list was not a dictionary".to_owned());
    };
    let Some(volume) = volume.as_object() else {
        return Err("APFS volume metadata was not a dictionary".to_owned());
    };
    for key in ["CapacityQuota", "CapacityInUse", "APFSVolumeUUID"] {
        if let Some(value) = volume.get(key) {
            details.insert(key.to_owned(), value.clone());
        }
    }
    Ok(info)
}

fn containing_device(root: &Path) -> Result<String, String> {
    let output = Command::new("/bin/df")
        .args(["-P"])
        .arg(root)
        .env("LC_ALL", "C")
        .output()
        .map_err(|error| format!("could not start df: {error}"))?;
    if !output.status.success() {
        return Err("df could not inspect the store filesystem".to_owned());
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .and_then(|line| line.split_whitespace().next())
        .filter(|device| device.starts_with("/dev/disk"))
        .map(ToOwned::to_owned)
        .ok_or_else(|| "the store is not backed by an inspectable local disk device".to_owned())
}

fn convert_plist(data: &[u8]) -> Result<Value, String> {
    let mut child = Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not start plutil: {error}"))?;
    let Some(mut input) = child.stdin.take() else {
        return Err("plutil stdin was unavailable".to_owned());
    };
    input
        .write_all(data)
        .map_err(|error| format!("could not send diskutil metadata to plutil: {error}"))?;
    drop(input);
    let converted = child
        .wait_with_output()
        .map_err(|error| format!("could not wait for plutil: {error}"))?;
    if !converted.status.success() {
        return Err("plutil could not decode diskutil metadata".to_owned());
    }
    serde_json::from_slice(&converted.stdout)
        .map_err(|error| format!("diskutil property list was not valid JSON: {error}"))
}

fn apfs_volume<'a>(topology: &'a Value, volume_id: &str) -> Option<&'a Value> {
    topology
        .get("Containers")?
        .as_array()?
        .iter()
        .filter_map(|container| container.get("Volumes").and_then(Value::as_array))
        .flatten()
        .find(|volume| volume.get("DeviceIdentifier").and_then(Value::as_str) == Some(volume_id))
}

pub(super) fn observation(root: &Path, provider: QuotaProvider, info: &Value) -> QuotaObservation {
    let filesystem = text(
        info,
        &["FilesystemType", "FilesystemName", "FileSystemPersonality"],
    );
    if !filesystem.is_some_and(|value| value.to_ascii_lowercase().contains("apfs")) {
        return QuotaObservation::unavailable(
            provider,
            root.to_path_buf(),
            QuotaHealth::Unsupported,
            "the store is not on APFS",
        );
    }
    let mount = text(info, &["MountPoint"]).map_or_else(|| root.to_path_buf(), PathBuf::from);
    let exact_scope = mount == root;
    let filesystem_id = text(
        info,
        &["VolumeUUID", "APFSContainerReference", "DeviceIdentifier"],
    )
    .map(ToOwned::to_owned);
    let quota_id = text(info, &["DeviceIdentifier", "VolumeUUID"]).map(ToOwned::to_owned);
    let limit = integer(info, &["CapacityQuota"])
        .filter(|value| *value > 0)
        .map(ByteSize::from_bytes);
    let usage = integer(info, &["CapacityInUse"]).map(ByteSize::from_bytes);
    let configured =
        exact_scope && limit.is_some() && filesystem_id.is_some() && quota_id.is_some();
    QuotaObservation {
        provider,
        health: if configured {
            QuotaHealth::Configured
        } else {
            QuotaHealth::AvailableUnconfigured
        },
        scope: mount,
        filesystem_id,
        quota_id,
        exact_scope,
        hard_enforcement: configured,
        usage,
        limit,
        detail: if exact_scope {
            if configured {
                "dedicated APFS volume quota is configured".to_owned()
            } else {
                "dedicated APFS volume has no verifiable nonzero quota".to_owned()
            }
        } else {
            "store root is not the mount root of a dedicated APFS volume".to_owned()
        },
    }
}

fn text<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
}

fn integer(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|entry| {
            entry
                .as_u64()
                .or_else(|| entry.as_str().and_then(|text| text.parse::<u64>().ok()))
        })
    })
}

fn status_suffix(code: Option<i32>) -> String {
    code.map_or_else(String::new, |value| format!(" with status {value}"))
}
