use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
};

use super::{
    build_repair_plan, node_policy, ConfirmedInstallRequest, EnvironmentSnapshot, InstallFailure,
    InstallFailureCode, InstallPreparation, InstallRequest, InstallStage, InstallTaskResult,
    InstallTaskSnapshot, InstallTaskStatus, RecommendedAction, RepairPlan, ToolId,
    ToolInstallResult, ToolInstallStatus,
};

use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

pub trait OrchestratorRuntime: Send + Sync {
    fn snapshot(&self) -> Result<EnvironmentSnapshot, InstallFailure>;
    fn repair(&self, _plan: &RepairPlan) -> Result<(), InstallFailure>;
    fn install_tool(&self, tool: ToolId) -> ToolInstallResult;
}

#[derive(Clone)]
pub struct InstallTaskStore {
    tasks: Arc<RwLock<HashMap<String, InstallTaskSnapshot>>>,
    cancellations: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
    persistence_path: Option<Arc<PathBuf>>,
}

impl Default for InstallTaskStore {
    fn default() -> Self {
        Self {
            tasks: Arc::default(),
            cancellations: Arc::default(),
            persistence_path: None,
        }
    }
}

impl InstallTaskStore {
    pub fn with_persistence(config_dir: PathBuf) -> Self {
        let path = config_dir.join("install-tasks.json");
        let tasks = std::fs::read(&path)
            .ok()
            .and_then(|data| serde_json::from_slice::<Vec<InstallTaskSnapshot>>(&data).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|mut task| {
                // Interrupted installer work is never resumed, especially not a
                // privileged package command. The next user action re-probes.
                if task.stage != InstallStage::Completed {
                    task.interrupted = true;
                    task.cancellation_requested = false;
                    task.stage = InstallStage::Completed;
                    task.result = Some(InstallTaskResult {
                        status: InstallTaskStatus::NeedsUserAction,
                        tools: Vec::new(),
                        failure: Some(failure(
                            InstallFailureCode::InstallerFailure,
                            "installation was interrupted; run preflight again",
                        )),
                    });
                }
                (task.task_id.clone(), task)
            })
            .collect();
        let store = Self {
            tasks: Arc::new(RwLock::new(tasks)),
            cancellations: Arc::default(),
            persistence_path: Some(Arc::new(path)),
        };
        store.persist();
        store
    }

    fn persist(&self) {
        let Some(path) = &self.persistence_path else {
            return;
        };
        let Ok(tasks) = self.tasks.read() else { return };
        let Ok(data) = serde_json::to_vec(&tasks.values().collect::<Vec<_>>()) else {
            return;
        };
        let Some(parent) = path.parent() else { return };
        if std::fs::create_dir_all(parent).is_ok() {
            let temp = path.with_extension("json.tmp");
            if std::fs::write(&temp, data).is_ok() {
                let _ = std::fs::rename(temp, path.as_ref());
            }
        }
    }
    pub fn get(&self, task_id: &str) -> Option<InstallTaskSnapshot> {
        self.tasks.read().ok()?.get(task_id).cloned()
    }

    pub fn active_task(&self) -> Option<InstallTaskSnapshot> {
        self.tasks
            .read()
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
        let mut tasks = self.tasks.write().expect("task store poisoned");
        if tasks.contains_key(&task_id) {
            return Err(failure(
                InstallFailureCode::InstallerFailure,
                "install task id already exists",
            ));
        }
        tasks.insert(task_id.clone(), snapshot);
        self.cancellations
            .write()
            .expect("task store poisoned")
            .insert(task_id.clone(), Arc::new(AtomicBool::new(false)));
        drop(tasks);
        self.persist();
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
        if !matches!(
            task.stage,
            InstallStage::Preflight | InstallStage::AwaitingConfirmation
        ) {
            return Err(failure(
                InstallFailureCode::InstallerFailure,
                "task has already started or completed",
            ));
        }
        task.stage = if task.plan.actions.is_empty() {
            InstallStage::InstallingTools
        } else {
            InstallStage::Repairing
        };
        {
            let mut tasks = self.tasks.write().expect("task store poisoned");
            let current = tasks
                .get(&task_id)
                .ok_or_else(|| failure(InstallFailureCode::InstallerFailure, "unknown task"))?;
            if !matches!(
                current.stage,
                InstallStage::Preflight | InstallStage::AwaitingConfirmation
            ) {
                return Err(failure(
                    InstallFailureCode::InstallerFailure,
                    "task has already started or completed",
                ));
            }
            tasks.insert(task_id.clone(), task.clone());
        }
        self.persist();
        let cancelled = self
            .cancellations
            .read()
            .expect("task store poisoned")
            .get(&task_id)
            .cloned()
            .ok_or_else(|| {
                failure(
                    InstallFailureCode::InstallerFailure,
                    "task cancellation token missing",
                )
            })?;
        if cancelled.load(Ordering::Acquire) {
            return self.finish_cancelled(task_id, task);
        }
        if !task.plan.actions.is_empty() {
            if let Err(error) = runtime.repair(&task.plan) {
                task.result = Some(InstallTaskResult {
                    status: InstallTaskStatus::Failed,
                    tools: Vec::new(),
                    failure: Some(error),
                });
                task.stage = InstallStage::Completed;
                self.tasks
                    .write()
                    .expect("task store poisoned")
                    .insert(task_id.clone(), task);
                self.persist();
                return Ok(task_id);
            }
        }
        if cancelled.load(Ordering::Acquire) {
            return self.finish_cancelled(task_id, task);
        }
        task.stage = InstallStage::InstallingTools;
        self.tasks
            .write()
            .expect("task store poisoned")
            .insert(task_id.clone(), task.clone());
        self.persist();
        let mut results = Vec::new();
        for tool in task.request.tools.iter().copied() {
            if cancelled.load(Ordering::Acquire) {
                return self.finish_cancelled(task_id, task);
            }
            results.push(runtime.install_tool(tool));
        }
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
            .write()
            .expect("task store poisoned")
            .insert(task_id.clone(), task);
        self.persist();
        Ok(task_id)
    }

    pub fn cancel(&self, task_id: &str) -> Result<(), String> {
        let mut tasks = self
            .tasks
            .write()
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
        if let Some(token) = self
            .cancellations
            .read()
            .map_err(|_| "task store unavailable".to_string())?
            .get(task_id)
        {
            token.store(true, Ordering::Release);
        }
        drop(tasks);
        self.persist();
        Ok(())
    }

    fn finish_cancelled(
        &self,
        task_id: String,
        mut task: InstallTaskSnapshot,
    ) -> Result<String, InstallFailure> {
        task.cancellation_requested = true;
        task.stage = InstallStage::Completed;
        task.result = Some(InstallTaskResult {
            status: InstallTaskStatus::CancelledByUser,
            tools: Vec::new(),
            failure: None,
        });
        self.tasks
            .write()
            .expect("task store poisoned")
            .insert(task_id.clone(), task);
        self.persist();
        Ok(task_id)
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
            super::Platform::Windows
        } else if cfg!(target_os = "macos") {
            super::Platform::Macos
        } else {
            return Err(failure(
                InstallFailureCode::UnsupportedPlatform,
                "native installer is limited to Windows and macOS",
            ));
        };
        Ok(EnvironmentSnapshot {
            platform,
            architecture: if cfg!(target_arch = "aarch64") {
                super::Architecture::Arm64
            } else {
                super::Architecture::X64
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

    fn repair(&self, plan: &RepairPlan) -> Result<(), InstallFailure> {
        use super::RepairActionKind;
        if plan
            .actions
            .iter()
            .all(|action| action.kind == RepairActionKind::RefreshEnvironment)
        {
            return match host_adapter() {
                Some(adapter) => super::refresh_environment(adapter.as_ref()).map(|_| ()),
                None => Err(failure(
                    InstallFailureCode::UnsupportedPlatform,
                    "native installer is limited to Windows and macOS",
                )),
            };
        }
        let Some(adapter) = host_adapter() else {
            return Err(failure(
                InstallFailureCode::UnsupportedPlatform,
                "native installer is limited to Windows and macOS",
            ));
        };
        let architecture = if cfg!(target_arch = "aarch64") {
            super::Architecture::Arm64
        } else {
            super::Architecture::X64
        };
        let install = async {
            let index = super::fetch_node_index().await?;
            let release = super::resolve_node_release(
                &index,
                &super::node_policy(),
                adapter.as_ref(),
                architecture,
            )?;
            let temp =
                std::env::temp_dir().join(format!("agent-manager-node-{}", std::process::id()));
            super::install_node_release(adapter.as_ref(), &release, temp).await
        };
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(install))
        } else {
            tokio::runtime::Runtime::new()
                .map_err(|error| failure(InstallFailureCode::InstallerFailure, &error.to_string()))?
                .block_on(install)
        }
    }

    fn install_tool(&self, tool: ToolId) -> ToolInstallResult {
        let Some(strategy) = super::tool_strategy(tool) else {
            return tool_failure(tool, "unsupported tool");
        };
        // Official installers without a Node dependency retain their npm
        // fallback where one is declared; this avoids silently selecting an
        // unrelated package for Hermes.
        let package = strategy.npm_package.or(strategy.fallback_npm_package);
        let output = match package {
            Some(package) => std::process::Command::new("npm")
                .args(["install", "--global", package])
                .output(),
            None if tool == ToolId::Hermes => {
                if cfg!(target_os = "windows") {
                    std::process::Command::new("powershell.exe")
                        .args(["-NoProfile", "-Command", "irm https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1 | iex"])
                        .output()
                } else {
                    std::process::Command::new("bash")
                        .args(["-c", "tmp=$(mktemp) && curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh -o \"$tmp\" && bash \"$tmp\"; status=$?; rm -f \"$tmp\"; exit $status"])
                        .output()
                }
            }
            None => return tool_failure(tool, "no approved installer available"),
        };
        match output {
            Ok(output) if output.status.success() => {
                let path = resolve_host_command(
                    strategy.command_name,
                    &std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                        .collect::<Vec<_>>(),
                );
                let version = path.as_deref().and_then(command_version);
                match version {
                    Some(version) => ToolInstallResult {
                        tool,
                        status: ToolInstallStatus::Succeeded,
                        version: Some(version),
                        path: path.map(|path| path.display().to_string()),
                        failure: None,
                    },
                    None => ToolInstallResult {
                        tool,
                        status: ToolInstallStatus::InstalledNotRunnable,
                        version: None,
                        path: path.map(|path| path.display().to_string()),
                        failure: Some(failure(
                            InstallFailureCode::VerificationFailure,
                            "installed command did not return a version",
                        )),
                    },
                }
            }
            Ok(output) => ToolInstallResult {
                tool,
                status: ToolInstallStatus::Failed,
                version: None,
                path: None,
                failure: Some(super::classify_process_failure(
                    InstallStage::InstallingTools,
                    output.status.code(),
                    &String::from_utf8_lossy(&output.stdout),
                    &String::from_utf8_lossy(&output.stderr),
                )),
            },
            Err(error) => tool_failure(tool, &error.to_string()),
        }
    }
}

fn host_adapter() -> Option<Box<dyn super::PlatformAdapter>> {
    #[cfg(target_os = "windows")]
    {
        Some(Box::new(super::platform::windows::WindowsAdapter))
    }
    #[cfg(target_os = "macos")]
    {
        Some(Box::new(super::platform::macos::MacosAdapter))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

fn tool_failure(tool: ToolId, detail: &str) -> ToolInstallResult {
    ToolInstallResult {
        tool,
        status: ToolInstallStatus::Failed,
        version: None,
        path: None,
        failure: Some(failure(InstallFailureCode::ToolInstallFailure, detail)),
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

    #[test]
    fn persisted_active_task_is_marked_interrupted_without_resuming() {
        let root = std::env::temp_dir().join(format!("installer-store-{}", std::process::id()));
        let store = InstallTaskStore::with_persistence(root.clone());
        let request = InstallRequest {
            task_id: Some("persisted".into()),
            tools: vec![ToolId::Codex],
            action: super::super::InstallAction::Install,
        };
        store.prepare(request, &Fake).unwrap();
        drop(store);

        let restored = InstallTaskStore::with_persistence(root.clone());
        let task = restored.get("persisted").unwrap();
        assert!(task.interrupted);
        assert_eq!(task.stage, InstallStage::Completed);
        assert_eq!(
            task.result.unwrap().status,
            InstallTaskStatus::NeedsUserAction
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
