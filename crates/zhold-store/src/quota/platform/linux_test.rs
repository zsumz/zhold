use super::linux_parser::{
    ext4_attribute, ext4_report_values, project_id_from_stat, quota_state_enabled,
    xfs_report_values, xfs_state_enabled,
};

#[test]
fn xfs_parsers_require_project_enforcement_and_a_nonzero_hard_limit() {
    let stat = "fd.path = \"/srv/zhold\"\nfsxattr.projid = 42\n";
    let state = "Project quota state on /srv (/dev/vdb)\n  Accounting: ON\n  Enforcement: ON\n";
    let report = "42 -- 80 0 100 00 [--------]\n";

    assert_eq!(project_id_from_stat(stat), Some(42));
    assert!(xfs_state_enabled(state));
    assert_eq!(xfs_report_values(report, 42), Some((81_920, 102_400)));
    assert!(!xfs_state_enabled(
        &state.replace("Enforcement: ON", "Enforcement: OFF")
    ));
    assert_eq!(xfs_report_values("42 -- 80 0 0 00 [--------]", 42), None);
}

#[test]
fn ext4_parsers_require_inheritance_state_and_exact_numeric_project() {
    let attributes = "-------------------P-- 42 /srv/zhold\n";
    let state = "Project quota on /srv (/dev/vdb) is on\n";
    let report = concat!(
        "Project,BlockStatus,FileStatus,BlockUsed,BlockSoftLimit,BlockHardLimit,BlockGrace,FileUsed,FileSoftLimit,FileHardLimit,FileGrace\n",
        "#7,ok,ok,1,0,2,,1,0,0,\n",
        "#42,ok,ok,80,0,100,,4,0,0,\n",
    );

    assert_eq!(
        ext4_attribute(attributes),
        Some(("-------------------P--", 42))
    );
    assert_eq!(ext4_attribute("/srv/project 42 /nested:\n"), None);
    assert!(quota_state_enabled(state));
    assert_eq!(ext4_report_values(report, 42), Some((81_920, 102_400)));
    assert!(!quota_state_enabled(&state.replace("is on", "is off")));
    assert_eq!(ext4_report_values(report, 9), None);
}
