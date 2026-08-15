pub(super) fn project_id_from_stat(text: &str) -> Option<u32> {
    text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("fsxattr.projid =")
            .and_then(|value| value.trim().parse::<u32>().ok())
            .filter(|value| *value > 0)
    })
}

pub(super) fn xfs_state_enabled(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("project quota state")
        && normalized.contains("accounting: on")
        && normalized.contains("enforcement: on")
}

pub(super) fn xfs_report_values(text: &str, project_id: u32) -> Option<(u64, u64)> {
    text.lines().find_map(|line| {
        let mut columns = line.split_whitespace();
        let identity = columns
            .next()?
            .trim_start_matches('#')
            .parse::<u32>()
            .ok()?;
        if identity != project_id {
            return None;
        }
        let numbers = columns
            .filter_map(|value| value.parse::<u64>().ok())
            .take(3)
            .collect::<Vec<_>>();
        kibibytes(numbers.first().copied()?, numbers.get(2).copied()?)
    })
}

pub(super) fn ext4_attribute(text: &str) -> Option<(&str, u32)> {
    text.lines().find_map(|line| {
        let mut columns = line.split_whitespace();
        let flags = columns.next()?;
        let project_id = columns.next()?.parse::<u32>().ok()?;
        let flags_valid = flags.len() >= 16
            && flags
                .bytes()
                .all(|value| value == b'-' || value.is_ascii_alphabetic());
        (flags_valid && project_id > 0).then_some((flags, project_id))
    })
}

pub(super) fn quota_state_enabled(text: &str) -> bool {
    text.lines().any(|line| {
        let normalized = line.trim().to_ascii_lowercase();
        normalized.starts_with("project quota on ") && normalized.ends_with(" is on")
    })
}

pub(super) fn ext4_report_values(text: &str, project_id: u32) -> Option<(u64, u64)> {
    text.lines().find_map(|line| {
        let columns = line.split(',').map(str::trim).collect::<Vec<_>>();
        let identity = columns
            .first()?
            .trim_start_matches('#')
            .parse::<u32>()
            .ok()?;
        if identity != project_id || columns.len() < 6 {
            return None;
        }
        kibibytes(columns[3].parse().ok()?, columns[5].parse().ok()?)
    })
}

fn kibibytes(usage: u64, hard: u64) -> Option<(u64, u64)> {
    let usage = usage.checked_mul(1024)?;
    let hard = hard.checked_mul(1024)?;
    (hard > 0).then_some((usage, hard))
}
