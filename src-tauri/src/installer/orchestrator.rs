use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use super::{
    build_repair_plan, node_policy, ConfirmedInstallRequest, EnvironmentSnapshot, InstallFailure,
    InstallFailureCode, InstallPreparation, InstallRequest, InstallStage, InstallTaskResult,
    InstallTaskSnapshot, InstallTaskStatus, RecommendedAction, RepairPlan, ToolId,
    ToolInstallResult, ToolInstallStatus,
};

use std::path::{Path, PathBuf};

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

pub trait OrchestratorRuntime: Send + Sync {
    fn snapshot(&self) -> Result<EnvironmentSnapshot, InstallFailure>;
    fn repair(&self, _plan: &RepairPlan) -> Result<(), InstallFailure>;
    fn install_tool(&self, tool: ToolId) -> ToolInstallResult;
}

#[derive(Clone, Default)]
pub struct InstallTaskStore {
    tasks: Arc<Mutex<HashMap<String, InstallTaskSnapshot>>>,
}

impl InstallTaskStore {
    pub fn get(&self, task_id: &str) -> Option<InstallTaskSnapshot> {
        self.tasks.lock().ok()?.get(task_id).cloned()
    }

    pub fn active_task(&self) -> Option<InstallTaskSnapshot> {
        self.tasks
            .lock()
            .ok()?
            .values()
            .find(|task| !matches!(task.stage, InstallStage::Completed))
            .cloned()
    }

    pub fn prepare<R: OrchestratorRuntime>(
        &self,
        request: InstallRequest,
        runtime: &R,
    ) -> Result<InstallPreparation, InstallFailure> {
        let plan = build_repair_plan(&runtime.snapshot()?, &node_policy())?;
        let task_id = request
            .task_id
            .clone()
            .unwrap_or_else(|| format!("install-{}", NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed)));
        let requires_confirmation = plan
            .actions
            .iter()
            .any(|action| action.requires_confirmation || action.requires_elevation);
        let stage = if requires_confirmation {
            InstallStage::AwaitingConfirmation
        } else {
            InstallStage::Preflight
        };
        let snapshot = InstallTaskSnapshot {
            task_id: task_id.clone(),
            request,
            stage,
            plan: plan.clone(),
            result: None,
            cancellation_requested: false,
            interrupted: false,
        };
        self.tasks
            .lock()
            .expect("task store poisoned")
            .insert(task_id.clone(), snapshot);
        Ok(InstallPreparation {
            task_id,
            plan,
            requires_confirmation,
        })
    }

    pub fn start<R: OrchestratorRuntime>(
        &self,
        confirmed: ConfirmedInstallRequest,
        runtime: &R,
    ) -> Result<String, InstallFailure> {
        let task_id = confirmed
            .request
            .task_id
            .clone()
            .ok_or_else(|| failure(InstallFailureCode::PrivilegeDeclined, "missing task id"))?;
        let mut task = self
            .get(&task_id)
            .ok_or_else(|| failure(InstallFailureCode::InstallerFailure, "unknown task"))?;
        let required: Vec<_> = task
            .plan
            .actions
            .iter()
            .filter(|action| action.requires_confirmation || action.requires_elevation)
            .map(|action| action.id.as_str())
            .collect();
        if required.iter().any(|id| {
            !confirmed
                .confirmed_action_ids
                .iter()
                .any(|confirmed| confirmed == *id)
        }) {
            return Err(failure(
                InstallFailureCode::PrivilegeDeclined,
                "required action was not confirmed",
            ));
        }
        if task.stage == InstallStage::Completed {
            return Err(failure(
                InstallFailureCode::InstallerFailure,
                "task already completed",
            ));
        }
        task.stage = if task.plan.actions.is_empty() {
            InstallStage::InstallingTools
        } else {
            InstallStage::Repairing
        };
        self.tasks
            .lock()
            .expect("task store poisoned")
            .insert(task_id.clone(), task.clone());
        if !task.plan.actions.is_empty() {
            if let Err(error) = runtime.repair(&task.plan) {
                task.result = Some(InstallTaskResult {
                    status: InstallTaskStatus::Failed,
                    tools: Vec::new(),
                    failure: Some(error),
                });
                task.stage = InstallStage::Completed;
                self.tasks
                    .lock()
                    .expect("task store poisoned")
                    .insert(task_id.clone(), task);
                return Ok(task_id);
            }
        }
        task.stage = InstallStage::InstallingTools;
        let results: Vec<_> = task
            .request
            .tools
            .iter()
            .copied()
            .map(|tool| runtime.install_tool(tool))
            .collect();
        task.stage = InstallStage::Verifying;
        let failed = results
            .iter()
            .any(|result| result.status != ToolInstallStatus::Succeeded);
        task.result = Some(InstallTaskResult {
            status: if failed {
                InstallTaskStatus::NeedsUserAction
            } else {
                InstallTaskStatus::Succeeded
            },
            tools: results,
            failure: None,
        });
        task.stage = InstallStage::Completed;
        self.tasks
            .lock()
            .expect("task store poisoned")
            .insert(task_id.clone(), task);
        Ok(task_id)
    }

    pub fn cancel(&self, task_id: &str) -> Result<(), String> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| "task store unavailable".to_string())?;
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| "unknown task".to_string())?;
        if matches!(
            task.stage,
            InstallStage::Repairing | InstallStage::Completed
        ) {
            return Err(
                "task cannot be cancelled during system installation or after completion".into(),
            );
        }
        task.cancellation_requested = true;
        task.stage = InstallStage::Completed;
        task.result = Some(InstallTaskResult {
            status: InstallTaskStatus::CancelledByUser,
            tools: Vec::new(),
            failure: None,
        });
        Ok(())
    }
}

/// Host runtime used by the command facade. Actual privileged repair remains
/// owned by the platform adapters; this runtime deliberately does not invent
/// an unapproved fallback installer.
pub struct CommandRuntime;

impl OrchestratorRuntime for CommandRuntime {
    fn snapshot(&self) -> Result<EnvironmentSnapshot, InstallFailure> {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let entries: Vec<PathBuf> = std::env::split_paths(&path).collect();
        let node = resolve_host_command("node", &entries);
        let npm = resolve_host_command("npm", &entries);
        let node_version = node.as_deref().and_then(|path| command_version(path));
        let npm_version = npm.as_deref().and_then(|path| command_version(path));
        let platform = if cfg!(target_os = "windows") {
            super::super::Platform::Windows
        } else if cfg!(target_os = "macos") {
            super::super::Platform::Macos
        } else {
            return Err(failure(
                InstallFailureCode::UnsupportedPlatform,
                "native installer is limited to Windows and macOS",
            ));
        };
        Ok(EnvironmentSnapshot {
            platform,
            architecture: if cfg!(target_arch = "aarch64") {
                super::super::Architecture::Arm64
            } else {
                super::super::Architecture::X64
            },
            architecture_supported: true,
            node_version: node_version.clone(),
            npm_version: npm_version.clone(),
            node_path: node.as_ref().map(|p| p.display().to_string()),
            npm_path: npm.as_ref().map(|p| p.display().to_string()),
            node_runnable: node_version.is_some(),
            npm_runnable: npm_version.is_some(),
            node_path_visible: node.is_some(),
            npm_path_visible: npm.is_some(),
            node_installations: node.into_iter().map(|p| p.display().to_string()).collect(),
            npm_installations: npm.into_iter().map(|p| p.display().to_string()).collect(),
            path: entries.iter().map(|p| p.display().to_string()).collect(),
            available_disk_bytes: u64::MAX,
            temporary_directory_writable: true,
            install_directory_writable: true,
            npm_prefix_writable: true,
            npm_cache_writable: true,
        })
    }

    fn repair(&self, _plan: &RepairPlan) -> Result<(), InstallFailure> {
        Err(failure(
            InstallFailureCode::InstallerFailure,
            "platform repair must be authorized by the native adapter",
        ))
    }

    fn install_tool(&self, tool: ToolId) -> ToolInstallResult {
        ToolInstallResult {
            tool,
            status: ToolInstallStatus::Failed,
            version: None,
            path: None,
            failure: Some(failure(
                InstallFailureCode::ToolInstallFailure,
                "tool installer is not available",
            )),
        }
    }
}

fn resolve_host_command(name: &str, entries: &[PathBuf]) -> Option<PathBuf> {
    entries
        .iter()
        .flat_map(|entry| {
            [
                entry.join(name),
                entry.join(format!("{name}.exe")),
                entry.join(format!("{name}.cmd")),
            ]
        })
        .find(|candidate| candidate.is_file())
}

fn command_version(path: &Path) -> Option<String> {
    let output = std::process::Command::new(path)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(str::to_owned)
}

fn failure(code: InstallFailureCode, detail: &str) -> InstallFailure {
    InstallFailure {
        code,
        stage: InstallStage::Preflight,
        exit_code: None,
        retryable: false,
        requires_user_action: true,
        message_key: "installer.failure.orchestrator".into(),
        recommended_action: RecommendedAction::ViewDiagnostics,
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake;
    impl OrchestratorRuntime for Fake {
        fn snapshot(&self) -> Result<EnvironmentSnapshot, InstallFailure> {
            Ok(EnvironmentSnapshot::without_node(
                super::super::Platform::Windows,
                super::super::Architecture::X64,
            ))
        }
        fn repair(&self, _: &RepairPlan) -> Result<(), InstallFailure> {
            Ok(())
        }
        fn install_tool(&self, tool: ToolId) -> ToolInstallResult {
            ToolInstallResult {
                tool,
                status: ToolInstallStatus::Succeeded,
                version: Some("1.0.0".into()),
                path: None,
                failure: None,
            }
        }
    }
    #[test]
    fn unconfirmed_privileged_plan_is_rejected() {
        let store = InstallTaskStore::default();
        let request = InstallRequest {
            task_id: Some("task".into()),
            tools: vec![ToolId::Codex],
            action: super::super::InstallAction::Install,
        };
        store.prepare(request.clone(), &Fake).unwrap();
        assert_eq!(
            store
                .start(
                    ConfirmedInstallRequest {
                        request,
                        confirmed_action_ids: vec![]
                    },
                    &Fake
                )
                .unwrap_err()
                .code,
            InstallFailureCode::PrivilegeDeclined
        );
    }
    #[test]
    fn confirmed_batch_repairs_once_and_finishes() {
        let store = InstallTaskStore::default();
        let request = InstallRequest {
            task_id: Some("task".into()),
            tools: vec![ToolId::Codex],
            action: super::super::InstallAction::Install,
        };
        let preparation = store.prepare(request.clone(), &Fake).unwrap();
        store
            .start(
                ConfirmedInstallRequest {
                    request,
                    confirmed_action_ids: preparation
                        .plan
                        .actions
                        .iter()
                        .map(|action| action.id.clone())
                        .collect(),
                },
                &Fake,
            )
            .unwrap();
        assert_eq!(
            store.get("task").unwrap().result.unwrap().status,
            InstallTaskStatus::Succeeded
        );
    }
}
