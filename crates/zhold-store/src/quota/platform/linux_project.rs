use std::{path::Path, process::Command};

use zhold_core::{ByteSize, QuotaHealth, QuotaProvider};

use super::super::QuotaObservation;
use super::{
    linux_parser::{
        ext4_attribute, ext4_report_values, project_id_from_stat, quota_state_enabled,
        xfs_report_values, xfs_state_enabled,
    },
    linux_tree::{ext4_tree_matches, xfs_tree_matches},
};

pub(super) fn xfs(root: &Path, mount: &Path, filesystem_id: Option<String>) -> QuotaObservation {
    let stat = command("xfs_io")
        .args(["-r", "-c", "stat"])
        .arg(root)
        .output();
    let Ok(stat) = stat else {
        return unavailable(
            root,
            filesystem_id,
            QuotaProvider::XfsProject,
            QuotaHealth::ProviderUnavailable,
            "xfs_io is unavailable",
        );
    };
    let Some(project_id) = successful_text(&stat).and_then(project_id_from_stat) else {
        return unavailable(
            root,
            filesystem_id,
            QuotaProvider::XfsProject,
            failure_health(&stat),
            "store root has no verifiable nonzero XFS project identity",
        );
    };
    let state = command("xfs_quota")
        .args(["-x", "-c", "state -p"])
        .arg(mount)
        .output();
    let Ok(state) = state else {
        return unavailable(
            root,
            filesystem_id,
            QuotaProvider::XfsProject,
            QuotaHealth::ProviderUnavailable,
            "xfs_quota is unavailable",
        );
    };
    let project_id_text = project_id.to_string();
    let report = command("xfs_quota")
        .args(["-x", "-d", &project_id_text, "-c", "report -p -b -n -N"])
        .arg(mount)
        .output();
    let values = report.as_ref().ok().and_then(|value| {
        successful_text(value).and_then(|text| xfs_report_values(text, project_id))
    });
    if !xfs_state_enabled(&String::from_utf8_lossy(&state.stdout)) {
        return unavailable(
            root,
            filesystem_id,
            QuotaProvider::XfsProject,
            failure_health(&state),
            "XFS project accounting, enforcement, usage, or hard limit is not verifiable",
        );
    }
    let Some(values) = values else {
        return unavailable(
            root,
            filesystem_id,
            QuotaProvider::XfsProject,
            report
                .as_ref()
                .map_or(QuotaHealth::ProviderUnavailable, failure_health),
            "XFS project usage or hard limit is not verifiable",
        );
    };
    if !xfs_tree_matches(root, mount, project_id) {
        return unavailable(
            root,
            filesystem_id,
            QuotaProvider::XfsProject,
            QuotaHealth::Inconsistent,
            "XFS project identity or inheritance is inconsistent in the store tree",
        );
    }
    configured(
        root,
        filesystem_id,
        QuotaProvider::XfsProject,
        format!("xfs-project:{project_id}"),
        values,
    )
}

pub(super) fn ext4(root: &Path, mount: &Path, filesystem_id: Option<String>) -> QuotaObservation {
    let attributes = command("lsattr").args(["-dp"]).arg(root).output();
    let Some((flags, project_id)) = attributes
        .as_ref()
        .ok()
        .and_then(|value| successful_text(value))
        .and_then(ext4_attribute)
    else {
        return unavailable(
            root,
            filesystem_id,
            QuotaProvider::Ext4Project,
            attributes
                .as_ref()
                .map_or(QuotaHealth::ProviderUnavailable, failure_health),
            "store root has no verifiable ext4 project identity",
        );
    };
    let state = command("quotaon").args(["-P", "-p"]).arg(mount).output();
    let report = command("repquota")
        .args(["-P", "-v", "-n", "-O", "csv"])
        .arg(mount)
        .output();
    let values = report.as_ref().ok().and_then(|value| {
        successful_text(value).and_then(|text| ext4_report_values(text, project_id))
    });
    let state_enabled = state
        .as_ref()
        .is_ok_and(|value| quota_state_enabled(&String::from_utf8_lossy(&value.stdout)));
    if !flags.contains('P') {
        return unavailable(
            root,
            filesystem_id,
            QuotaProvider::Ext4Project,
            QuotaHealth::AvailableUnconfigured,
            "ext4 project inheritance is not enabled on the store root",
        );
    }
    if !state_enabled {
        return unavailable(
            root,
            filesystem_id,
            QuotaProvider::Ext4Project,
            state
                .as_ref()
                .map_or(QuotaHealth::ProviderUnavailable, failure_health),
            "ext4 project quota enforcement is not verifiable",
        );
    }
    let Some(values) = values else {
        return unavailable(
            root,
            filesystem_id,
            QuotaProvider::Ext4Project,
            report
                .as_ref()
                .map_or(QuotaHealth::ProviderUnavailable, failure_health),
            "ext4 project usage or hard limit is not verifiable",
        );
    };
    if !ext4_tree_matches(root, project_id) {
        return unavailable(
            root,
            filesystem_id,
            QuotaProvider::Ext4Project,
            QuotaHealth::Inconsistent,
            "ext4 project identity is inconsistent in the store tree",
        );
    }
    configured(
        root,
        filesystem_id,
        QuotaProvider::Ext4Project,
        format!("ext4-project:{project_id}"),
        values,
    )
}

fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    command.env("LC_ALL", "C").env("LANG", "C");
    command
}

fn successful_text(output: &std::process::Output) -> Option<&str> {
    output
        .status
        .success()
        .then(|| std::str::from_utf8(&output.stdout).ok())
        .flatten()
}

fn configured(
    root: &Path,
    filesystem_id: Option<String>,
    provider: QuotaProvider,
    quota_id: String,
    values: (u64, u64),
) -> QuotaObservation {
    let (usage, limit) = values;
    QuotaObservation {
        provider,
        health: QuotaHealth::Configured,
        scope: root.to_path_buf(),
        filesystem_id,
        quota_id: Some(quota_id),
        exact_scope: true,
        hard_enforcement: true,
        usage: Some(ByteSize::from_bytes(usage)),
        limit: Some(ByteSize::from_bytes(limit)),
        detail: format!("exact-scope {provider} hard quota is configured"),
    }
}

fn unavailable(
    root: &Path,
    filesystem_id: Option<String>,
    provider: QuotaProvider,
    health: QuotaHealth,
    detail: &str,
) -> QuotaObservation {
    let mut value = QuotaObservation::unavailable(provider, root.to_path_buf(), health, detail);
    value.filesystem_id = filesystem_id;
    value
}

fn failure_health(output: &std::process::Output) -> QuotaHealth {
    let error = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    if error.contains("permission denied") || error.contains("operation not permitted") {
        QuotaHealth::PermissionRequired
    } else if output.status.success() {
        QuotaHealth::AvailableUnconfigured
    } else {
        QuotaHealth::ProviderUnavailable
    }
}
