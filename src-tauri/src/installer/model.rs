use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    X64,
    Arm64,
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
    pub tools: Vec<String>,
    pub action: InstallAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfirmedInstallRequest {
    pub request: InstallRequest,
    pub confirmed_action_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentSnapshot {
    pub platform: Platform,
    pub architecture: Architecture,
    pub node_version: Option<String>,
    pub npm_version: Option<String>,
    pub node_path: Option<String>,
    pub npm_path: Option<String>,
    pub path: Vec<String>,
}

impl EnvironmentSnapshot {
    pub fn without_node(platform: Platform, architecture: Architecture) -> Self {
        Self {
            platform,
            architecture,
            node_version: None,
            npm_version: None,
            node_path: None,
            npm_path: None,
            path: Vec::new(),
        }
    }

    pub fn healthy_node(node_version: &str, npm_version: &str) -> Self {
        Self {
            platform: Platform::Windows,
            architecture: Architecture::X64,
            node_version: Some(node_version.into()),
            npm_version: Some(npm_version.into()),
            node_path: None,
            npm_path: None,
            path: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepairAction {
    pub id: String,
    pub kind: RepairActionKind,
    pub requires_authorization: bool,
    pub status: ActionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RepairPlan {
    pub actions: Vec<RepairAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolInstallResult {
    pub tool: String,
    pub status: ToolInstallStatus,
    pub version: Option<String>,
    pub path: Option<String>,
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
    DependencyMissing,
    DependencyUnsupported,
    PrivilegeDeclined,
    DownloadIntegrityFailure,
    NetworkFailure,
    ToolInstallFailed,
    VerificationFailed,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallFailure {
    pub code: InstallFailureCode,
    pub stage: InstallStage,
    pub exit_code: Option<i32>,
    pub retryable: bool,
    pub requires_user_action: bool,
    pub message_key: String,
    pub recommended_action: Option<String>,
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
    use super::{InstallStage, InstallTaskEvent};

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
}
