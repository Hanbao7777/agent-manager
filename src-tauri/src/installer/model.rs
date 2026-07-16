use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    Macos,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    X64,
    Arm64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathAccessState {
    Writable,
    NeedsCreation,
    RequiresElevation,
    Blocked,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathAccess {
    pub path: Option<String>,
    pub state: PathAccessState,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToolId {
    Claude,
    Codex,
    Gemini,
    Opencode,
    Openclaw,
    Hermes,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallAction {
    Install,
    Update,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallStage {
    Preflight,
    AwaitingConfirmation,
    Repairing,
    InstallingTools,
    Verifying,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepairActionKind {
    InstallNode,
    UpgradeNode,
    RepairNode,
    RefreshEnvironment,
    InstallNpm,
    SwitchToManagedInstallation,
    UpdatePath,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolInstallStatus {
    Succeeded,
    Failed,
    InstalledNotRunnable,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallTaskStatus {
    Succeeded,
    SucceededWithConflicts,
    InstalledNotRunnable,
    CancelledByUser,
    NeedsUserAction,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallRequest {
    pub task_id: Option<String>,
    pub tools: Vec<ToolId>,
    pub action: InstallAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfirmedInstallRequest {
    pub request: InstallRequest,
    pub confirmed_action_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallPreparation {
    pub task_id: String,
    pub plan: RepairPlan,
    pub requires_confirmation: bool,
    #[serde(default)]
    pub refresh_generation: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StartInstallOutcome {
    Started { task_id: String },
    Refreshed { preparation: InstallPreparation },
    StateChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallTaskSnapshot {
    pub task_id: String,
    pub request: InstallRequest,
    pub stage: InstallStage,
    pub plan: RepairPlan,
    pub result: Option<InstallTaskResult>,
    pub cancellation_requested: bool,
    pub interrupted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentSnapshot {
    pub platform: Platform,
    pub architecture: Architecture,
    pub architecture_supported: bool,
    pub node_version: Option<String>,
    pub npm_version: Option<String>,
    pub node_path: Option<String>,
    pub npm_path: Option<String>,
    pub node_runnable: bool,
    pub npm_runnable: bool,
    pub node_path_visible: bool,
    pub npm_path_visible: bool,
    pub node_installations: Vec<String>,
    pub npm_installations: Vec<String>,
    pub path: Vec<String>,
    pub available_disk_bytes: u64,
    pub managed_available_disk_bytes: u64,
    pub temporary_directory_access: PathAccess,
    pub managed_install_directory_access: PathAccess,
    pub managed_cache_directory_access: PathAccess,
}

impl EnvironmentSnapshot {
    pub fn without_node(platform: Platform, architecture: Architecture) -> Self {
        Self {
            platform,
            architecture,
            architecture_supported: true,
            node_version: None,
            npm_version: None,
            node_path: None,
            npm_path: None,
            node_runnable: false,
            npm_runnable: false,
            node_path_visible: false,
            npm_path_visible: false,
            node_installations: Vec::new(),
            npm_installations: Vec::new(),
            path: Vec::new(),
            available_disk_bytes: u64::MAX,
            managed_available_disk_bytes: u64::MAX,
            temporary_directory_access: PathAccess {
                path: None,
                state: PathAccessState::Writable,
                detail: None,
            },
            managed_install_directory_access: PathAccess {
                path: None,
                state: PathAccessState::Writable,
                detail: None,
            },
            managed_cache_directory_access: PathAccess {
                path: None,
                state: PathAccessState::Writable,
                detail: None,
            },
        }
    }

    pub fn healthy_node(node_version: &str, npm_version: &str) -> Self {
        Self {
            platform: Platform::Windows,
            architecture: Architecture::X64,
            architecture_supported: true,
            node_version: Some(node_version.into()),
            npm_version: Some(npm_version.into()),
            node_path: None,
            npm_path: None,
            node_runnable: true,
            npm_runnable: true,
            node_path_visible: true,
            npm_path_visible: true,
            node_installations: Vec::new(),
            npm_installations: Vec::new(),
            path: Vec::new(),
            available_disk_bytes: u64::MAX,
            managed_available_disk_bytes: u64::MAX,
            temporary_directory_access: PathAccess {
                path: None,
                state: PathAccessState::Writable,
                detail: None,
            },
            managed_install_directory_access: PathAccess {
                path: None,
                state: PathAccessState::Writable,
                detail: None,
            },
            managed_cache_directory_access: PathAccess {
                path: None,
                state: PathAccessState::Writable,
                detail: None,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepairAction {
    pub id: String,
    pub kind: RepairActionKind,
    pub requires_confirmation: bool,
    pub requires_elevation: bool,
    pub status: ActionStatus,
    #[serde(default)]
    pub target_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RepairPlan {
    pub actions: Vec<RepairAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolInstallResult {
    pub tool: ToolId,
    pub status: ToolInstallStatus,
    pub version: Option<String>,
    pub path: Option<String>,
    #[serde(default)]
    pub shadowed_paths: Vec<String>,
    pub failure: Option<InstallFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallTaskResult {
    pub status: InstallTaskStatus,
    pub tools: Vec<ToolInstallResult>,
    pub failure: Option<InstallFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallFailureCode {
    UnsupportedPlatform,
    UnsupportedArchitecture,
    InsufficientDiskSpace,
    DependencyMissing,
    DependencyTooOld,
    DependencyBroken,
    PathNotVisible,
    MultipleInstallations,
    PermissionDenied,
    PrivilegeDeclined,
    FileInUse,
    DnsFailure,
    NetworkTimeout,
    ProxyUnreachable,
    TlsFailure,
    DownloadIntegrityFailure,
    SignatureVerificationFailure,
    InstallerFailure,
    ToolInstallFailure,
    VerificationFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecommendedAction {
    Retry,
    RepairDependencies,
    FreeDiskSpace,
    GrantPermission,
    CloseBlockingProcess,
    CheckNetwork,
    CheckProxy,
    ResolveMultipleInstallations,
    Reinstall,
    ViewDiagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallFailure {
    pub code: InstallFailureCode,
    pub stage: InstallStage,
    pub exit_code: Option<i32>,
    pub retryable: bool,
    pub requires_user_action: bool,
    pub message_key: String,
    pub recommended_action: RecommendedAction,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InstallTaskEvent {
    StageChanged {
        task_id: String,
        stage: InstallStage,
    },
    ActionChanged {
        task_id: String,
        action_id: String,
        status: ActionStatus,
    },
    ToolFinished {
        task_id: String,
        result: ToolInstallResult,
    },
    Finished {
        task_id: String,
        result: InstallTaskResult,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        ActionStatus, Architecture, EnvironmentSnapshot, InstallFailureCode, InstallStage,
        InstallTaskEvent, PathAccess, PathAccessState, Platform, RecommendedAction, RepairAction,
        RepairActionKind, ToolId,
    };

    #[test]
    fn task_event_serializes_with_stable_wire_names() {
        let event = InstallTaskEvent::StageChanged {
            task_id: "task-1".into(),
            stage: InstallStage::Preflight,
        };
        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["type"], "stage_changed");
        assert_eq!(value["stage"], "preflight");
    }

    #[test]
    fn tool_ids_serialize_with_stable_wire_names() {
        let ids = [
            ToolId::Claude,
            ToolId::Codex,
            ToolId::Gemini,
            ToolId::Opencode,
            ToolId::Openclaw,
            ToolId::Hermes,
        ];
        let value = serde_json::to_value(ids).unwrap();
        assert_eq!(
            value,
            serde_json::json!(["claude", "codex", "gemini", "opencode", "openclaw", "hermes"])
        );
    }

    #[test]
    fn failure_taxonomy_serializes_with_approved_wire_names() {
        let cases = [
            (
                InstallFailureCode::UnsupportedPlatform,
                "unsupported_platform",
            ),
            (
                InstallFailureCode::UnsupportedArchitecture,
                "unsupported_architecture",
            ),
            (
                InstallFailureCode::InsufficientDiskSpace,
                "insufficient_disk_space",
            ),
            (InstallFailureCode::DependencyMissing, "dependency_missing"),
            (InstallFailureCode::DependencyTooOld, "dependency_too_old"),
            (InstallFailureCode::DependencyBroken, "dependency_broken"),
            (InstallFailureCode::PathNotVisible, "path_not_visible"),
            (
                InstallFailureCode::MultipleInstallations,
                "multiple_installations",
            ),
            (InstallFailureCode::PermissionDenied, "permission_denied"),
            (InstallFailureCode::PrivilegeDeclined, "privilege_declined"),
            (InstallFailureCode::FileInUse, "file_in_use"),
            (InstallFailureCode::DnsFailure, "dns_failure"),
            (InstallFailureCode::NetworkTimeout, "network_timeout"),
            (InstallFailureCode::ProxyUnreachable, "proxy_unreachable"),
            (InstallFailureCode::TlsFailure, "tls_failure"),
            (
                InstallFailureCode::DownloadIntegrityFailure,
                "download_integrity_failure",
            ),
            (
                InstallFailureCode::SignatureVerificationFailure,
                "signature_verification_failure",
            ),
            (InstallFailureCode::InstallerFailure, "installer_failure"),
            (
                InstallFailureCode::ToolInstallFailure,
                "tool_install_failure",
            ),
            (
                InstallFailureCode::VerificationFailure,
                "verification_failure",
            ),
        ];

        for (code, expected) in cases {
            assert_eq!(serde_json::to_value(code).unwrap(), expected);
        }
    }

    #[test]
    fn recommended_actions_use_typed_wire_names() {
        assert_eq!(
            serde_json::to_value(RecommendedAction::RepairDependencies).unwrap(),
            "repair_dependencies"
        );
    }

    #[test]
    fn platform_is_limited_to_windows_and_macos() {
        assert_eq!(
            serde_json::from_str::<Platform>(r#""windows""#).unwrap(),
            Platform::Windows
        );
        assert_eq!(
            serde_json::from_str::<Platform>(r#""macos""#).unwrap(),
            Platform::Macos
        );
        assert!(serde_json::from_str::<Platform>(r#""linux""#).is_err());
    }

    #[test]
    fn path_access_types_serialize_with_stable_wire_names_and_shape() {
        let states = [
            PathAccessState::Writable,
            PathAccessState::NeedsCreation,
            PathAccessState::RequiresElevation,
            PathAccessState::Blocked,
            PathAccessState::NotApplicable,
        ];
        assert_eq!(
            serde_json::to_value(states).unwrap(),
            serde_json::json!([
                "writable",
                "needs_creation",
                "requires_elevation",
                "blocked",
                "not_applicable"
            ])
        );

        let access = PathAccess {
            path: Some("managed/npm".into()),
            state: PathAccessState::NeedsCreation,
            detail: Some("nearest existing ancestor is writable".into()),
        };
        assert_eq!(
            serde_json::to_value(access).unwrap(),
            serde_json::json!({
                "path": "managed/npm",
                "state": "needs_creation",
                "detail": "nearest existing ancestor is writable"
            })
        );
    }

    #[test]
    fn environment_snapshot_carries_repair_planner_inputs() {
        let mut snapshot = EnvironmentSnapshot::without_node(Platform::Windows, Architecture::X64);
        snapshot.architecture_supported = false;
        snapshot.node_version = Some("24.4.1".into());
        snapshot.node_runnable = false;
        snapshot.node_path_visible = false;
        snapshot.node_installations = vec!["C:\\node-a".into(), "C:\\node-b".into()];
        snapshot.available_disk_bytes = 1024;
        snapshot.managed_available_disk_bytes = 2048;
        snapshot.managed_install_directory_access.state = PathAccessState::Blocked;

        assert!(!snapshot.architecture_supported);
        assert!(!snapshot.node_runnable);
        assert!(!snapshot.node_path_visible);
        assert_eq!(snapshot.node_installations.len(), 2);
        assert_eq!(snapshot.available_disk_bytes, 1024);
        assert_eq!(snapshot.managed_available_disk_bytes, 2048);
        assert_eq!(
            snapshot.managed_install_directory_access.state,
            PathAccessState::Blocked
        );
    }

    #[test]
    fn repair_actions_separate_confirmation_and_elevation() {
        let action = RepairAction {
            id: "refresh-path".into(),
            kind: RepairActionKind::RefreshEnvironment,
            requires_confirmation: false,
            requires_elevation: true,
            status: ActionStatus::Pending,
            target_paths: vec!["managed/npm".into()],
        };

        assert!(!action.requires_confirmation);
        assert!(action.requires_elevation);
    }

    #[test]
    fn managed_switch_action_has_a_stable_wire_name() {
        assert_eq!(
            serde_json::to_value(RepairActionKind::SwitchToManagedInstallation).unwrap(),
            "switch_to_managed_installation"
        );
    }

    #[test]
    fn tool_ids_reject_values_outside_the_supported_set() {
        assert!(serde_json::from_str::<ToolId>(r#""unknown""#).is_err());
    }
}
