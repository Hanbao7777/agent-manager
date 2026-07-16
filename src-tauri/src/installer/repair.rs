use super::{
    Architecture, EnvironmentSnapshot, InstallFailure, InstallFailureCode, InstallStage,
    NodePolicy, PathAccessState, Platform, RecommendedAction, RepairAction, RepairActionKind,
    RepairPlan,
};

// Keep one GiB available for the Node installer, unpacking, and npm metadata.
const MINIMUM_DISK_BYTES: u64 = 1_073_741_824;

pub fn build_repair_plan(
    snapshot: &EnvironmentSnapshot,
    policy: &NodePolicy,
) -> Result<RepairPlan, InstallFailure> {
    if !matches!(snapshot.platform, Platform::Windows | Platform::Macos) {
        return Err(failure(
            InstallFailureCode::UnsupportedPlatform,
            RecommendedAction::ViewDiagnostics,
            "unsupported_platform",
        ));
    }
    if !snapshot.architecture_supported
        || !matches!(
            snapshot.architecture,
            Architecture::X64 | Architecture::Arm64
        )
    {
        return Err(failure(
            InstallFailureCode::UnsupportedArchitecture,
            RecommendedAction::ViewDiagnostics,
            "unsupported_architecture",
        ));
    }
    if snapshot.available_disk_bytes < MINIMUM_DISK_BYTES
        || snapshot.managed_available_disk_bytes < MINIMUM_DISK_BYTES
    {
        return Err(failure(
            InstallFailureCode::InsufficientDiskSpace,
            RecommendedAction::FreeDiskSpace,
            "insufficient_disk_space",
        ));
    }
    if snapshot.node_installations.len() > 1 {
        return Err(failure(
            InstallFailureCode::MultipleInstallations,
            RecommendedAction::ResolveMultipleInstallations,
            "multiple_installations",
        ));
    }
    if snapshot.npm_installations.len() > 1 {
        return Err(failure(
            InstallFailureCode::MultipleInstallations,
            RecommendedAction::ResolveMultipleInstallations,
            "multiple_npm_installations",
        ));
    }
    if !path_access_is_acceptable(&snapshot.temporary_directory_access.state)
        || !path_access_is_acceptable(&snapshot.managed_install_directory_access.state)
        || !path_access_is_acceptable(&snapshot.managed_cache_directory_access.state)
    {
        return Err(failure(
            InstallFailureCode::PermissionDenied,
            RecommendedAction::GrantPermission,
            "permission_denied",
        ));
    }

    let mut actions = Vec::new();
    let has_node_evidence = snapshot.node_path.is_some()
        || !snapshot.node_installations.is_empty()
        || snapshot.node_runnable;
    if snapshot.node_version.is_none() && !has_node_evidence {
        actions.push(action(
            "install-node",
            RepairActionKind::InstallNode,
            true,
            true,
        ));
    } else if snapshot.node_version.is_none() || !snapshot.node_runnable {
        actions.push(action(
            "repair-node",
            RepairActionKind::RepairNode,
            true,
            true,
        ));
    } else if !snapshot.node_path_visible {
        actions.push(action(
            "refresh-environment",
            RepairActionKind::RefreshEnvironment,
            false,
            false,
        ));
    } else if !supported_node(snapshot.node_version.as_deref(), policy) {
        actions.push(action(
            "upgrade-node",
            RepairActionKind::UpgradeNode,
            true,
            true,
        ));
    } else if !snapshot.npm_path_visible {
        actions.push(action(
            "refresh-environment",
            RepairActionKind::RefreshEnvironment,
            false,
            false,
        ));
    } else if snapshot.npm_version.is_none() || !snapshot.npm_runnable {
        actions.push(action(
            "repair-node",
            RepairActionKind::RepairNode,
            true,
            true,
        ));
    }

    Ok(RepairPlan { actions })
}

fn path_access_is_acceptable(state: &PathAccessState) -> bool {
    matches!(
        state,
        PathAccessState::Writable | PathAccessState::NeedsCreation | PathAccessState::NotApplicable
    )
}

fn supported_node(version: Option<&str>, policy: &NodePolicy) -> bool {
    version
        .and_then(|version| version.trim_start_matches('v').split('.').next())
        .and_then(|major| major.parse::<u64>().ok())
        .is_some_and(|major| policy.accepts_major(major))
}

fn action(
    id: &str,
    kind: RepairActionKind,
    requires_confirmation: bool,
    requires_elevation: bool,
) -> RepairAction {
    RepairAction {
        id: id.into(),
        kind,
        requires_confirmation,
        requires_elevation,
        status: super::ActionStatus::Pending,
        target_paths: Vec::new(),
    }
}

fn failure(
    code: InstallFailureCode,
    recommended_action: RecommendedAction,
    detail: &str,
) -> InstallFailure {
    InstallFailure {
        code,
        stage: InstallStage::Preflight,
        exit_code: None,
        retryable: false,
        requires_user_action: true,
        message_key: format!("installer.failure.{detail}"),
        recommended_action,
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_node_requires_confirmed_official_install() {
        let snapshot = EnvironmentSnapshot::without_node(Platform::Windows, Architecture::X64);
        let plan = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap();
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].kind, RepairActionKind::InstallNode);
        assert!(plan.actions[0].requires_confirmation);
        assert!(plan.actions[0].requires_elevation);
    }

    #[test]
    fn healthy_node_and_npm_need_no_repair() {
        let snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
        assert!(build_repair_plan(&snapshot, &super::super::node_policy())
            .unwrap()
            .actions
            .is_empty());
    }

    #[test]
    fn broken_node_is_repaired_not_installed() {
        let mut snapshot = EnvironmentSnapshot::without_node(Platform::Windows, Architecture::X64);
        snapshot.node_installations.push("C:/node/node.exe".into());
        let plan = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap();
        assert_eq!(plan.actions[0].kind, RepairActionKind::RepairNode);
        assert_eq!(plan.actions[0].id, "repair-node");
        assert_eq!(plan.actions[0].status, super::super::ActionStatus::Pending);
        assert!(plan.actions[0].requires_confirmation);
        assert!(plan.actions[0].requires_elevation);
    }

    #[test]
    fn old_node_is_upgraded() {
        let mut snapshot = EnvironmentSnapshot::healthy_node("18.20.0", "10.0.0");
        snapshot.node_installations.push("C:/node/node.exe".into());
        let plan = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap();
        assert_eq!(plan.actions[0].kind, RepairActionKind::UpgradeNode);
        assert_eq!(plan.actions[0].id, "upgrade-node");
        assert_eq!(plan.actions[0].status, super::super::ActionStatus::Pending);
        assert!(plan.actions[0].requires_confirmation);
        assert!(plan.actions[0].requires_elevation);
    }

    #[test]
    fn malformed_node_version_is_repaired_by_upgrade() {
        let mut snapshot = EnvironmentSnapshot::healthy_node("not-a-version", "11.4.2");
        snapshot.node_installations.push("C:/node/node.exe".into());
        let plan = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap();
        assert_eq!(plan.actions[0].kind, RepairActionKind::UpgradeNode);
    }

    #[test]
    fn invisible_node_path_is_refreshed() {
        let mut snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
        snapshot.node_path_visible = false;
        let plan = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap();
        assert_eq!(plan.actions[0].kind, RepairActionKind::RefreshEnvironment);
        assert_eq!(plan.actions[0].id, "refresh-environment");
    }

    #[test]
    fn existing_node_path_without_version_is_repaired() {
        let mut snapshot = EnvironmentSnapshot::without_node(Platform::Windows, Architecture::X64);
        snapshot.node_path = Some("C:/node/node.exe".into());
        let plan = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap();
        assert_eq!(plan.actions[0].kind, RepairActionKind::RepairNode);
    }

    #[test]
    fn multiple_node_installations_are_not_selected_silently() {
        let mut snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
        snapshot.node_installations = vec!["a/node".into(), "b/node".into()];
        let error = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap_err();
        assert_eq!(error.code, InstallFailureCode::MultipleInstallations);
    }

    #[test]
    fn unsupported_architecture_is_blocked() {
        let mut snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
        snapshot.architecture_supported = false;
        let error = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap_err();
        assert_eq!(error.code, InstallFailureCode::UnsupportedArchitecture);
    }

    #[test]
    fn npm_path_invisible_is_refreshed() {
        let mut snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
        snapshot.npm_path_visible = false;
        let plan = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap();
        assert_eq!(plan.actions[0].kind, RepairActionKind::RefreshEnvironment);
        assert_eq!(plan.actions[0].id, "refresh-environment");
        assert_eq!(plan.actions[0].status, super::super::ActionStatus::Pending);
        assert!(!plan.actions[0].requires_confirmation);
        assert!(!plan.actions[0].requires_elevation);
    }

    #[test]
    fn broken_npm_repairs_existing_node_installation() {
        let mut snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
        snapshot.npm_version = None;
        snapshot.npm_runnable = false;
        let plan = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap();
        assert_eq!(plan.actions[0].kind, RepairActionKind::RepairNode);
        assert_eq!(plan.actions[0].id, "repair-node");
    }

    #[test]
    fn multiple_npm_installations_are_not_selected_silently() {
        let mut snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
        snapshot.npm_installations = vec!["a/npm".into(), "b/npm".into()];
        let error = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap_err();
        assert_eq!(error.code, InstallFailureCode::MultipleInstallations);
    }

    #[test]
    fn temporary_disk_threshold_is_independent_and_inclusive() {
        let mut snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
        snapshot.available_disk_bytes = MINIMUM_DISK_BYTES;
        assert!(build_repair_plan(&snapshot, &super::super::node_policy())
            .unwrap()
            .actions
            .is_empty());
        snapshot.available_disk_bytes = MINIMUM_DISK_BYTES - 1;
        assert_eq!(
            build_repair_plan(&snapshot, &super::super::node_policy())
                .unwrap_err()
                .code,
            InstallFailureCode::InsufficientDiskSpace
        );
    }

    #[test]
    fn managed_disk_threshold_is_independent_and_inclusive() {
        let mut snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
        snapshot.managed_available_disk_bytes = MINIMUM_DISK_BYTES;
        assert!(build_repair_plan(&snapshot, &super::super::node_policy())
            .unwrap()
            .actions
            .is_empty());
        snapshot.managed_available_disk_bytes = MINIMUM_DISK_BYTES - 1;
        assert_eq!(
            build_repair_plan(&snapshot, &super::super::node_policy())
                .unwrap_err()
                .code,
            InstallFailureCode::InsufficientDiskSpace
        );
    }

    #[test]
    fn needs_creation_and_not_applicable_path_access_are_accepted() {
        let mut snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
        snapshot.temporary_directory_access.state = PathAccessState::NeedsCreation;
        snapshot.managed_install_directory_access.state = PathAccessState::NeedsCreation;
        snapshot.managed_cache_directory_access.state = PathAccessState::NotApplicable;

        assert!(build_repair_plan(&snapshot, &super::super::node_policy())
            .unwrap()
            .actions
            .is_empty());
    }

    #[test]
    fn blocked_and_elevated_path_access_fail_with_permission_denied() {
        for state in [PathAccessState::Blocked, PathAccessState::RequiresElevation] {
            let mut snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
            snapshot.managed_install_directory_access.state = state;
            let error = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap_err();
            assert_eq!(error.code, InstallFailureCode::PermissionDenied);
        }
    }

    #[test]
    fn healthy_node_is_deterministic_across_platform_architecture_pairs() {
        for (platform, architecture) in [
            (Platform::Windows, Architecture::X64),
            (Platform::Windows, Architecture::Arm64),
            (Platform::Macos, Architecture::X64),
            (Platform::Macos, Architecture::Arm64),
        ] {
            let mut snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
            snapshot.platform = platform;
            snapshot.architecture = architecture;
            assert!(build_repair_plan(&snapshot, &super::super::node_policy())
                .unwrap()
                .actions
                .is_empty());
        }
    }

    #[test]
    fn repeated_plan_calls_are_identical() {
        let snapshot = EnvironmentSnapshot::without_node(Platform::Windows, Architecture::X64);
        let first = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap();
        let second = build_repair_plan(&snapshot, &super::super::node_policy()).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.actions[0].id, "install-node");
        assert_eq!(first.actions[0].status, super::super::ActionStatus::Pending);
        assert!(first.actions[0].requires_confirmation);
        assert!(first.actions[0].requires_elevation);
    }
}
