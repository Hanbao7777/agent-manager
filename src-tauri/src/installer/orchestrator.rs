use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
};

use super::{
    aggregate_tool_outcomes, build_repair_plan, next_sequence_from_task_ids, node_policy,
    parse_command_version_output, replace_file, resolve_pair_with_refresh, system_access,
    unique_temporary_path, AggregatedTaskStatus, ConfirmedInstallRequest, EnvironmentSnapshot,
    InstallFailure, InstallFailureCode, InstallPreparation, InstallRequest, InstallStage,
    InstallTaskResult, InstallTaskSnapshot, InstallTaskStatus, PairSource, RecommendedAction,
    RepairPlan, ResolvedNodeNpmPair, ToolId, ToolInstallResult, ToolInstallStatus, ToolOutcome,
};

use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

pub trait OrchestratorRuntime: Send + Sync {
    fn snapshot(&self) -> Result<EnvironmentSnapshot, InstallFailure>;
    fn repair(&self, _plan: &RepairPlan) -> Result<(), InstallFailure>;
    fn managed_switch_required(&self, _tool: ToolId) -> Result<bool, InstallFailure> {
        Ok(false)
    }
    fn install_tool(&self, tool: ToolId, managed_switch_confirmed: bool) -> ToolInstallResult;
}

#[derive(Clone)]
pub struct InstallTaskStore {
    tasks: Arc<RwLock<HashMap<String, InstallTaskSnapshot>>>,
    cancellations: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
    claims: Arc<RwLock<HashSet<String>>>,
    startup_recovery_events: Arc<RwLock<Vec<super::InstallTaskEvent>>>,
    persistence_path: Option<Arc<PathBuf>>,
    next_task_id: Arc<AtomicU64>,
    persistence_sequence: Arc<AtomicU64>,
}

impl Default for InstallTaskStore {
    fn default() -> Self {
        Self {
            tasks: Arc::default(),
            cancellations: Arc::default(),
            claims: Arc::default(),
            startup_recovery_events: Arc::default(),
            persistence_path: None,
            next_task_id: Arc::new(AtomicU64::new(1)),
            persistence_sequence: Arc::new(AtomicU64::new(1)),
        }
    }
}

impl InstallTaskStore {
    pub fn with_persistence(config_dir: PathBuf) -> Self {
        let path = config_dir.join("install-tasks.json");
        let mut startup_recovery_events = Vec::new();
        let tasks: HashMap<String, InstallTaskSnapshot> = std::fs::read(&path)
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
                    startup_recovery_events.extend(terminal_events(&task));
                }
                (task.task_id.clone(), task)
            })
            .collect();
        let next_task_id = next_sequence_from_task_ids(tasks.keys().map(String::as_str));
        let store = Self {
            tasks: Arc::new(RwLock::new(tasks)),
            cancellations: Arc::default(),
            claims: Arc::default(),
            startup_recovery_events: Arc::new(RwLock::new(startup_recovery_events)),
            persistence_path: Some(Arc::new(path)),
            next_task_id: Arc::new(AtomicU64::new(next_task_id)),
            persistence_sequence: Arc::new(AtomicU64::new(1)),
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
            let temp = unique_temporary_path(
                path,
                self.persistence_sequence.fetch_add(1, Ordering::Relaxed),
            );
            if std::fs::write(&temp, data).is_ok() {
                if replace_file(&temp, path.as_ref()).is_err() {
                    let _ = std::fs::remove_file(temp);
                }
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

    /// Returns terminal events for work interrupted before this application
    /// instance started. Draining prevents duplicate startup notifications.
    pub fn take_startup_recovery_events(&self) -> Vec<super::InstallTaskEvent> {
        self.startup_recovery_events
            .write()
            .map(|mut events| std::mem::take(&mut *events))
            .unwrap_or_default()
    }

    pub fn prepare<R: OrchestratorRuntime>(
        &self,
        mut request: InstallRequest,
        runtime: &R,
    ) -> Result<InstallPreparation, InstallFailure> {
        let mut plan = build_repair_plan(&runtime.snapshot()?, &node_policy())?;
        let mut switch_tools = HashSet::new();
        for tool in request.tools.iter().copied() {
            if switch_tools.insert(tool) && runtime.managed_switch_required(tool)? {
                plan.actions.push(super::RepairAction {
                    id: managed_switch_action_id(tool),
                    kind: super::RepairActionKind::SwitchToManagedInstallation,
                    requires_confirmation: true,
                    requires_elevation: false,
                    status: super::ActionStatus::Pending,
                });
            }
        }
        let task_id = request.task_id.clone().unwrap_or_else(|| {
            format!(
                "install-{}",
                self.next_task_id.fetch_add(1, Ordering::Relaxed)
            )
        });
        // The frontend prepares new tasks without an id, then confirms them
        // with the generated id returned below. Persist the canonical request
        // so confirmation compares the same task identity instead of `None`
        // against `Some(generated_id)`.
        request.task_id = Some(task_id.clone());
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
        self.start_with_events(confirmed, runtime, |_| {})
    }

    /// Atomically validate confirmation and reserve a task for the background
    /// worker. Commands call this before returning a task id to the frontend.
    pub fn claim_start(
        &self,
        confirmed: &ConfirmedInstallRequest,
    ) -> Result<String, InstallFailure> {
        let task_id = confirmed
            .request
            .task_id
            .clone()
            .ok_or_else(|| failure(InstallFailureCode::PrivilegeDeclined, "missing task id"))?;
        let task = self
            .get(&task_id)
            .ok_or_else(|| failure(InstallFailureCode::InstallerFailure, "unknown task"))?;
        validate_confirmed_request(&task, &confirmed.request)?;
        if !matches!(
            task.stage,
            InstallStage::Preflight | InstallStage::AwaitingConfirmation
        ) {
            return Err(failure(
                InstallFailureCode::InstallerFailure,
                "task has already started or completed",
            ));
        }
        if task
            .plan
            .actions
            .iter()
            .filter(|action| action.requires_confirmation || action.requires_elevation)
            .any(|action| {
                !confirmed
                    .confirmed_action_ids
                    .iter()
                    .any(|id| id == &action.id)
            })
        {
            return Err(failure(
                InstallFailureCode::PrivilegeDeclined,
                "required action was not confirmed",
            ));
        }
        let mut claims = self.claims.write().expect("task store poisoned");
        if !claims.insert(task_id.clone()) {
            return Err(failure(
                InstallFailureCode::InstallerFailure,
                "task has already started or completed",
            ));
        }
        Ok(task_id)
    }

    pub fn release_claim(&self, task_id: &str) {
        if let Ok(mut claims) = self.claims.write() {
            claims.remove(task_id);
        }
    }

    pub fn fail_background(
        &self,
        task_id: &str,
        error: InstallFailure,
    ) -> Option<InstallTaskResult> {
        let mut tasks = self.tasks.write().ok()?;
        let task = tasks.get_mut(task_id)?;
        task.stage = InstallStage::Completed;
        task.result = Some(InstallTaskResult {
            status: InstallTaskStatus::Failed,
            tools: Vec::new(),
            failure: Some(error),
        });
        let result = task.result.clone();
        drop(tasks);
        self.persist();
        result
    }

    pub fn start_with_events<R: OrchestratorRuntime, F: FnMut(super::InstallTaskEvent)>(
        &self,
        confirmed: ConfirmedInstallRequest,
        runtime: &R,
        mut emit: F,
    ) -> Result<String, InstallFailure> {
        let task_id = confirmed
            .request
            .task_id
            .clone()
            .ok_or_else(|| failure(InstallFailureCode::PrivilegeDeclined, "missing task id"))?;
        let mut task = self
            .get(&task_id)
            .ok_or_else(|| failure(InstallFailureCode::InstallerFailure, "unknown task"))?;
        validate_confirmed_request(&task, &confirmed.request)?;
        if is_terminal_cancellation(&task) {
            return Ok(task_id);
        }
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
            if is_terminal_cancellation(current) {
                return Ok(task_id);
            }
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
        emit(super::InstallTaskEvent::StageChanged {
            task_id: task_id.clone(),
            stage: task.stage.clone(),
        });
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
        let mut results = Vec::new();
        if cancelled.load(Ordering::Acquire) {
            if self.has_terminal_cancellation(&task_id) {
                return Ok(task_id);
            }
            let task_id = self.finish_cancelled(task_id, task, results)?;
            self.emit_terminal(&task_id, &mut emit);
            return Ok(task_id);
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
                self.emit_terminal(&task_id, &mut emit);
                return Ok(task_id);
            }
        }
        if cancelled.load(Ordering::Acquire) {
            if self.has_terminal_cancellation(&task_id) {
                return Ok(task_id);
            }
            let task_id = self.finish_cancelled(task_id, task, results)?;
            self.emit_terminal(&task_id, &mut emit);
            return Ok(task_id);
        }
        if !task.plan.actions.is_empty() {
            task.stage = InstallStage::InstallingTools;
            self.tasks
                .write()
                .expect("task store poisoned")
                .insert(task_id.clone(), task.clone());
            self.persist();
            emit(super::InstallTaskEvent::StageChanged {
                task_id: task_id.clone(),
                stage: InstallStage::InstallingTools,
            });
        }
        for tool in task.request.tools.iter().copied() {
            if cancelled.load(Ordering::Acquire) {
                if self.has_terminal_cancellation(&task_id) {
                    return Ok(task_id);
                }
                let task_id = self.finish_cancelled(task_id, task, results)?;
                self.emit_terminal(&task_id, &mut emit);
                return Ok(task_id);
            }
            let result = runtime.install_tool(
                tool,
                task.plan
                    .actions
                    .iter()
                    .any(|action| action.id == managed_switch_action_id(tool)),
            );
            emit(super::InstallTaskEvent::ToolFinished {
                task_id: task_id.clone(),
                result: result.clone(),
            });
            results.push(result);
            if cancelled.load(Ordering::Acquire) {
                if self.has_terminal_cancellation(&task_id) {
                    return Ok(task_id);
                }
                let task_id = self.finish_cancelled(task_id, task, results)?;
                self.emit_terminal(&task_id, &mut emit);
                return Ok(task_id);
            }
        }
        task.stage = InstallStage::Verifying;
        self.tasks
            .write()
            .expect("task store poisoned")
            .insert(task_id.clone(), task.clone());
        self.persist();
        emit(super::InstallTaskEvent::StageChanged {
            task_id: task_id.clone(),
            stage: InstallStage::Verifying,
        });
        if cancelled.load(Ordering::Acquire) {
            if self.has_terminal_cancellation(&task_id) {
                return Ok(task_id);
            }
            let task_id = self.finish_cancelled(task_id, task, results)?;
            self.emit_terminal(&task_id, &mut emit);
            return Ok(task_id);
        }
        let status = aggregate_tool_outcomes(results.iter().map(|result| match result.status {
            ToolInstallStatus::Succeeded => ToolOutcome::Succeeded,
            ToolInstallStatus::Failed => ToolOutcome::Failed,
            ToolInstallStatus::InstalledNotRunnable => ToolOutcome::InstalledNotRunnable,
            ToolInstallStatus::Skipped => ToolOutcome::Skipped,
        }));
        task.result = Some(InstallTaskResult {
            status: match status {
                AggregatedTaskStatus::Succeeded => InstallTaskStatus::Succeeded,
                AggregatedTaskStatus::InstalledNotRunnable => {
                    InstallTaskStatus::InstalledNotRunnable
                }
                AggregatedTaskStatus::NeedsUserAction => InstallTaskStatus::NeedsUserAction,
                AggregatedTaskStatus::Failed => InstallTaskStatus::Failed,
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
        self.emit_terminal(&task_id, &mut emit);
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
        let safe_to_finish = matches!(
            task.stage,
            InstallStage::Preflight | InstallStage::AwaitingConfirmation
        );
        if safe_to_finish {
            task.stage = InstallStage::Completed;
            task.result = Some(InstallTaskResult {
                status: InstallTaskStatus::CancelledByUser,
                tools: Vec::new(),
                failure: None,
            });
        }
        drop(tasks);
        self.persist();
        Ok(())
    }

    pub fn cancel_with_events<F: FnMut(super::InstallTaskEvent)>(
        &self,
        task_id: &str,
        mut emit: F,
    ) -> Result<(), String> {
        self.cancel(task_id)?;
        if self.has_terminal_cancellation(task_id) {
            self.emit_terminal(task_id, &mut emit);
        }
        Ok(())
    }

    fn finish_cancelled(
        &self,
        task_id: String,
        mut task: InstallTaskSnapshot,
        tools: Vec<ToolInstallResult>,
    ) -> Result<String, InstallFailure> {
        task.cancellation_requested = true;
        task.stage = InstallStage::Completed;
        task.result = Some(InstallTaskResult {
            status: InstallTaskStatus::CancelledByUser,
            tools,
            failure: None,
        });
        self.tasks
            .write()
            .expect("task store poisoned")
            .insert(task_id.clone(), task);
        self.persist();
        Ok(task_id)
    }

    fn has_terminal_cancellation(&self, task_id: &str) -> bool {
        self.get(task_id)
            .as_ref()
            .is_some_and(is_terminal_cancellation)
    }

    fn emit_terminal<F: FnMut(super::InstallTaskEvent)>(&self, task_id: &str, emit: &mut F) {
        let Some(task) = self.get(task_id) else {
            return;
        };
        for event in terminal_events(&task) {
            emit(event);
        }
    }
}

fn validate_confirmed_request(
    task: &InstallTaskSnapshot,
    request: &InstallRequest,
) -> Result<(), InstallFailure> {
    if task.request != *request {
        return Err(failure(
            InstallFailureCode::PrivilegeDeclined,
            "confirmed request does not match the prepared task",
        ));
    }
    Ok(())
}

fn managed_switch_action_id(tool: ToolId) -> String {
    let identity = super::tool_strategy(tool)
        .map(|strategy| strategy.command_name)
        .unwrap_or("unsupported");
    format!("switch-{identity}-to-managed")
}

fn is_terminal_cancellation(task: &InstallTaskSnapshot) -> bool {
    task.cancellation_requested
        && task.stage == InstallStage::Completed
        && matches!(
            task.result.as_ref().map(|result| &result.status),
            Some(InstallTaskStatus::CancelledByUser)
        )
}

fn terminal_events(task: &InstallTaskSnapshot) -> Vec<super::InstallTaskEvent> {
    let Some(result) = task.result.clone() else {
        return Vec::new();
    };
    vec![
        super::InstallTaskEvent::StageChanged {
            task_id: task.task_id.clone(),
            stage: InstallStage::Completed,
        },
        super::InstallTaskEvent::Finished {
            task_id: task.task_id.clone(),
            result,
        },
    ]
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
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let temporary_directory = std::env::temp_dir();
            let (managed_install_directory, managed_cache_directory) = managed_directories()?;
            let temporary_disk_path =
                system_access::nearest_existing_directory(&temporary_directory)?;
            let managed_disk_path =
                system_access::nearest_existing_directory(&managed_install_directory)?;
            let available_disk_bytes = system_access::available_disk_bytes(&temporary_disk_path)?;
            let managed_available_disk_bytes =
                system_access::available_disk_bytes(&managed_disk_path)?;

            return Ok(EnvironmentSnapshot {
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
                available_disk_bytes,
                managed_available_disk_bytes,
                temporary_directory_access: system_access::path_access(&temporary_directory),
                managed_install_directory_access: system_access::path_access(
                    &managed_install_directory,
                ),
                managed_cache_directory_access: system_access::path_access(
                    &managed_cache_directory,
                ),
            });
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        Err(failure(
            InstallFailureCode::UnsupportedPlatform,
            "native installer is limited to Windows and macOS",
        ))
    }

    fn repair(&self, plan: &RepairPlan) -> Result<(), InstallFailure> {
        use super::RepairActionKind;
        let dependency_actions: Vec<_> = plan
            .actions
            .iter()
            .filter(|action| action.kind != RepairActionKind::SwitchToManagedInstallation)
            .collect();
        if dependency_actions.is_empty() {
            return Ok(());
        }
        if dependency_actions
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
        // CommandRuntime is run by the dedicated install worker. Its runtime
        // boundary must not be nested in Tauri's Tokio executor.
        tokio::runtime::Runtime::new()
            .map_err(|error| failure(InstallFailureCode::InstallerFailure, &error.to_string()))?
            .block_on(install)
    }

    fn managed_switch_required(&self, tool: ToolId) -> Result<bool, InstallFailure> {
        let strategy = super::tool_strategy(tool)
            .ok_or_else(|| failure(InstallFailureCode::ToolInstallFailure, "unsupported tool"))?;
        if strategy.method != super::ToolInstallMethod::Npm {
            return Ok(false);
        }
        let (managed_root, _) = managed_directories()?;
        Ok(external_installation(strategy.command_name, &managed_root)
            .is_some_and(|(_, _, writable)| !writable))
    }

    fn install_tool(&self, tool: ToolId, managed_switch_confirmed: bool) -> ToolInstallResult {
        let Some(strategy) = super::tool_strategy(tool) else {
            return tool_failure(tool, "unsupported tool");
        };
        if strategy.method != super::ToolInstallMethod::Npm {
            return tool_failure(
                tool,
                "no approved verified official installer is available; npm fallback was not selected",
            );
        }
        let (managed_root, cache_root) = match managed_directories() {
            Ok(paths) => paths,
            Err(error) => {
                return ToolInstallResult {
                    tool,
                    status: ToolInstallStatus::Failed,
                    version: None,
                    path: None,
                    shadowed_paths: Vec::new(),
                    failure: Some(error),
                };
            }
        };
        let shadowed_external = external_candidates(strategy.command_name, &managed_root);
        if let Some((existing, version, writable)) =
            external_installation(strategy.command_name, &managed_root)
        {
            let decision = super::managed_npm::decide_managed_install(
                super::managed_npm::ExistingCliInstallation::External {
                    runnable: version.is_some(),
                    writable,
                },
            );
            match decision {
                super::managed_npm::ManagedInstallDecision::PreserveExternal => {
                    return ToolInstallResult {
                        tool,
                        status: ToolInstallStatus::Succeeded,
                        version,
                        path: Some(existing.display().to_string()),
                        shadowed_paths: Vec::new(),
                        failure: None,
                    };
                }
                super::managed_npm::ManagedInstallDecision::SwitchToManagedRequiresConfirmation
                    if !managed_switch_confirmed =>
                {
                    return tool_failure(
                        tool,
                        "external installation is unwritable; switching to a managed installation requires a distinct confirmed action",
                    );
                }
                _ => {}
            }
        }
        let pair = match resolve_node_npm_pair() {
            Ok(pair) => pair,
            Err(detail) => return tool_failure(tool, &detail),
        };
        let managed_tool = match super::managed_npm::ManagedNpmTool::from_tool(tool) {
            Ok(tool) => tool,
            Err(error) => {
                return ToolInstallResult {
                    tool,
                    status: ToolInstallStatus::Failed,
                    version: None,
                    path: None,
                    shadowed_paths: Vec::new(),
                    failure: Some(error),
                };
            }
        };
        let managed_bin = managed_root.join("bin");
        let approved_runtime_dirs = approved_runtime_directories(&pair);
        let result =
            super::managed_npm::install_managed_npm_tool(super::managed_npm::ManagedNpmRequest {
                tool: managed_tool,
                platform: if cfg!(target_os = "windows") {
                    super::Platform::Windows
                } else {
                    super::Platform::Macos
                },
                npm_path: pair.npm,
                node_path: pair.node,
                managed_root: managed_root.clone(),
                cache_root,
            });
        if result.status != ToolInstallStatus::Succeeded {
            return result;
        }
        let Some(version) = result.version.clone() else {
            return tool_failure(tool, "managed install succeeded without a version");
        };
        let Some(executable) = result.path.as_ref().map(PathBuf::from) else {
            return tool_failure(tool, "managed install succeeded without an entry point");
        };
        let request = super::path_persistence::PathPersistenceRequest {
            managed_bin,
            approved_runtime_dirs,
            managed_executable: executable,
            executable_name: strategy.command_name.into(),
            expected_version: version,
            external_candidates: shadowed_external,
        };
        finalize_managed_install(result, request, super::path_persistence::persist_host_path)
    }
}

fn finalize_managed_install<F>(
    mut result: ToolInstallResult,
    request: super::path_persistence::PathPersistenceRequest,
    persist: F,
) -> ToolInstallResult
where
    F: FnOnce(
        &super::path_persistence::PathPersistenceRequest,
    ) -> Result<super::path_persistence::PathProbeResult, InstallFailure>,
{
    match persist(&request) {
        Ok(probe) => {
            result.path = Some(probe.selected.to_string_lossy().into_owned());
            result.shadowed_paths = probe
                .shadowed
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
            result
        }
        Err(error) => ToolInstallResult {
            status: ToolInstallStatus::InstalledNotRunnable,
            failure: Some(error),
            ..result
        },
    }
}

fn approved_runtime_directories(pair: &ResolvedNodeNpmPair) -> Vec<PathBuf> {
    [pair.node.parent(), pair.npm.parent()]
        .into_iter()
        .flatten()
        .fold(Vec::new(), |mut directories, directory| {
            if !directories
                .iter()
                .any(|existing| paths_equal(existing, directory))
            {
                directories.push(directory.to_path_buf());
            }
            directories
        })
}

#[cfg(target_os = "windows")]
fn managed_directories() -> Result<(PathBuf, PathBuf), InstallFailure> {
    let base = dirs::data_local_dir().ok_or_else(|| {
        failure(
            InstallFailureCode::InstallerFailure,
            "required local application data directory is unavailable",
        )
    })?;
    let root = base.join("Agent-Manager");
    Ok((root.join("npm"), root.join("cache").join("npm")))
}

#[cfg(target_os = "macos")]
fn managed_directories() -> Result<(PathBuf, PathBuf), InstallFailure> {
    let home = dirs::home_dir().ok_or_else(|| {
        failure(
            InstallFailureCode::InstallerFailure,
            "required current-user home directory is unavailable",
        )
    })?;
    let root = home.join(".agent-manager");
    Ok((root.join("npm"), root.join("cache").join("npm")))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn managed_directories() -> Result<(PathBuf, PathBuf), InstallFailure> {
    Err(failure(
        InstallFailureCode::UnsupportedPlatform,
        "native installer is limited to Windows and macOS",
    ))
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
        shadowed_paths: Vec::new(),
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

fn external_installation(
    command_name: &str,
    managed_root: &Path,
) -> Option<(PathBuf, Option<String>, bool)> {
    let entries: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    let existing = resolve_host_command(command_name, &entries)?;
    if path_is_within(&existing, &managed_root.join("bin")) {
        return None;
    }
    let version = command_version(&existing);
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let writable = std::fs::OpenOptions::new()
        .write(true)
        .open(&existing)
        .is_ok();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let writable = false;
    Some((existing, version, writable))
}

fn external_candidates(command_name: &str, managed_root: &Path) -> Vec<PathBuf> {
    let entries: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    let managed_bin = managed_root.join("bin");
    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in entries {
        for candidate in [
            entry.join(command_name),
            entry.join(format!("{command_name}.exe")),
            entry.join(format!("{command_name}.cmd")),
        ] {
            if candidate.is_file()
                && !path_is_within(&candidate, &managed_bin)
                && !candidates.iter().any(|seen| paths_equal(seen, &candidate))
            {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

fn path_is_within(candidate: &Path, root: &Path) -> bool {
    path_is_within_for_platform(candidate, root, cfg!(target_os = "windows"))
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    if !cfg!(target_os = "windows") {
        return left == right;
    }
    windows_path_key(&left.to_string_lossy()) == windows_path_key(&right.to_string_lossy())
}

fn path_is_within_for_platform(candidate: &Path, root: &Path, windows: bool) -> bool {
    if !windows {
        return candidate.starts_with(root);
    }
    let candidate = windows_path_key(&candidate.to_string_lossy());
    let root = windows_path_key(&root.to_string_lossy());
    candidate == root || candidate.strip_prefix(&format!("{root}\\")).is_some()
}

fn windows_path_key(path: &str) -> String {
    path.trim_matches('"')
        .replace('/', "\\")
        .trim()
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn resolve_node_npm_pair() -> Result<ResolvedNodeNpmPair, String> {
    let entries: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    let resolved = resolve_pair_with_refresh(&entries, || {
        let adapter = host_adapter()
            .ok_or_else(|| "native installer is limited to Windows and macOS".to_string())?;
        super::refresh_environment(adapter.as_ref())
            .map(|environment| environment.path_entries)
            .map_err(|error| {
                error
                    .detail
                    .unwrap_or_else(|| "failed to refresh the system environment".into())
            })
    })?;
    Ok(ResolvedNodeNpmPair {
        node: resolved.node,
        npm: resolved.npm,
        source: match resolved.source {
            PairSource::ProcessPath => "PATH",
            PairSource::RefreshedEnvironment => "refreshed-environment",
        },
    })
}

fn command_version(path: &Path) -> Option<String> {
    let output = std::process::Command::new(path)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_command_version_output(&String::from_utf8_lossy(&output.stdout))
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
        fn install_tool(&self, tool: ToolId, _: bool) -> ToolInstallResult {
            ToolInstallResult {
                tool,
                status: ToolInstallStatus::Succeeded,
                version: Some("1.0.0".into()),
                path: None,
                shadowed_paths: Vec::new(),
                failure: None,
            }
        }
    }

    fn succeeded_result() -> ToolInstallResult {
        ToolInstallResult {
            tool: ToolId::Codex,
            status: ToolInstallStatus::Succeeded,
            version: Some("1.0.0".into()),
            path: Some("managed/bin/codex".into()),
            shadowed_paths: Vec::new(),
            failure: None,
        }
    }

    fn persistence_request() -> super::super::path_persistence::PathPersistenceRequest {
        super::super::path_persistence::PathPersistenceRequest {
            managed_bin: PathBuf::from("managed/bin"),
            approved_runtime_dirs: vec![PathBuf::from("runtime")],
            managed_executable: PathBuf::from("managed/bin/codex"),
            executable_name: "codex".into(),
            expected_version: "1.0.0".into(),
            external_candidates: vec![PathBuf::from("external/codex")],
        }
    }

    #[test]
    fn preflight_does_not_invoke_tool_or_path_persistence() {
        struct CountingRuntime(Arc<std::sync::atomic::AtomicUsize>);
        impl OrchestratorRuntime for CountingRuntime {
            fn snapshot(&self) -> Result<EnvironmentSnapshot, InstallFailure> {
                Fake.snapshot()
            }
            fn repair(&self, _: &RepairPlan) -> Result<(), InstallFailure> {
                Ok(())
            }
            fn install_tool(&self, _: ToolId, _: bool) -> ToolInstallResult {
                self.0.fetch_add(1, Ordering::Relaxed);
                succeeded_result()
            }
        }
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let store = InstallTaskStore::default();
        store
            .prepare(
                InstallRequest {
                    task_id: Some("no-path-write".into()),
                    tools: vec![ToolId::Codex],
                    action: super::super::InstallAction::Install,
                },
                &CountingRuntime(calls.clone()),
            )
            .unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn confirmed_managed_install_invokes_injected_path_persistence() {
        let mut calls = 0;
        let result =
            finalize_managed_install(succeeded_result(), persistence_request(), |request| {
                calls += 1;
                Ok(super::super::path_persistence::PathProbeResult {
                    selected: request.managed_executable.clone(),
                    shadowed: request.external_candidates.clone(),
                })
            });
        assert_eq!(calls, 1);
        assert_eq!(result.status, ToolInstallStatus::Succeeded);
        assert_eq!(result.shadowed_paths, vec!["external/codex"]);
    }

    #[test]
    fn preserved_writable_external_install_does_not_reach_persistence_boundary() {
        let decision = super::super::managed_npm::decide_managed_install(
            super::super::managed_npm::ExistingCliInstallation::External {
                runnable: true,
                writable: true,
            },
        );
        let mut persistence_calls = 0;
        if decision != super::super::managed_npm::ManagedInstallDecision::PreserveExternal {
            let _ = finalize_managed_install(succeeded_result(), persistence_request(), |_| {
                persistence_calls += 1;
                Err(super::super::path_persistence::path_failure(
                    "unexpected persistence",
                ))
            });
        }
        assert_eq!(persistence_calls, 0);
    }

    #[test]
    fn windows_managed_path_exclusion_is_case_insensitive() {
        assert!(path_is_within_for_platform(
            Path::new(r"C:\USERS\ME\APPDATA\LOCAL\AGENT-MANAGER\NPM\BIN\codex.cmd"),
            Path::new(r"c:\users\me\appdata\local\Agent-Manager\npm\bin"),
            true,
        ));
    }

    struct ManagedSwitchFake;

    impl OrchestratorRuntime for ManagedSwitchFake {
        fn snapshot(&self) -> Result<EnvironmentSnapshot, InstallFailure> {
            Fake.snapshot()
        }

        fn repair(&self, plan: &RepairPlan) -> Result<(), InstallFailure> {
            Fake.repair(plan)
        }

        fn managed_switch_required(&self, tool: ToolId) -> Result<bool, InstallFailure> {
            Ok(tool == ToolId::Codex)
        }

        fn install_tool(&self, tool: ToolId, confirmed: bool) -> ToolInstallResult {
            assert!(confirmed);
            Fake.install_tool(tool, false)
        }
    }

    #[test]
    fn unwritable_external_installation_adds_a_distinct_confirmed_switch_action() {
        let store = InstallTaskStore::default();
        let request = InstallRequest {
            task_id: Some("managed-switch".into()),
            tools: vec![ToolId::Codex],
            action: super::super::InstallAction::Install,
        };
        let preparation = store.prepare(request.clone(), &ManagedSwitchFake).unwrap();
        let action = preparation
            .plan
            .actions
            .iter()
            .find(|action| action.id == "switch-codex-to-managed")
            .unwrap();
        assert_eq!(
            action.kind,
            super::super::RepairActionKind::SwitchToManagedInstallation
        );
        assert!(action.requires_confirmation);
        assert!(!action.requires_elevation);
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
                &ManagedSwitchFake,
            )
            .unwrap();
    }

    #[test]
    fn managed_switch_action_does_not_trigger_dependency_repair() {
        let plan = RepairPlan {
            actions: vec![super::super::RepairAction {
                id: "switch-codex-to-managed".into(),
                kind: super::super::RepairActionKind::SwitchToManagedInstallation,
                requires_confirmation: true,
                requires_elevation: false,
                status: super::super::ActionStatus::Pending,
            }],
        };

        CommandRuntime.repair(&plan).unwrap();
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
    fn confirmed_request_must_match_the_prepared_task() {
        let store = InstallTaskStore::default();
        let prepared = InstallRequest {
            task_id: Some("bound-request".into()),
            tools: vec![ToolId::Codex],
            action: super::super::InstallAction::Install,
        };
        let preparation = store.prepare(prepared.clone(), &Fake).unwrap();
        let substituted = InstallRequest {
            tools: vec![ToolId::Claude],
            action: super::super::InstallAction::Update,
            ..prepared
        };
        let confirmed = ConfirmedInstallRequest {
            request: substituted,
            confirmed_action_ids: preparation
                .plan
                .actions
                .iter()
                .map(|action| action.id.clone())
                .collect(),
        };

        assert_eq!(
            store.claim_start(&confirmed).unwrap_err().code,
            InstallFailureCode::PrivilegeDeclined
        );
        assert_eq!(
            store
                .start_with_events(confirmed, &Fake, |_| {})
                .unwrap_err()
                .code,
            InstallFailureCode::PrivilegeDeclined
        );
    }

    #[test]
    fn generated_task_id_is_bound_before_confirmation() {
        let store = InstallTaskStore::default();
        let prepared = InstallRequest {
            task_id: None,
            tools: vec![ToolId::Codex],
            action: super::super::InstallAction::Install,
        };
        let preparation = store.prepare(prepared, &Fake).unwrap();
        let confirmed = ConfirmedInstallRequest {
            request: InstallRequest {
                task_id: Some(preparation.task_id.clone()),
                tools: vec![ToolId::Codex],
                action: super::super::InstallAction::Install,
            },
            confirmed_action_ids: preparation
                .plan
                .actions
                .iter()
                .map(|action| action.id.clone())
                .collect(),
        };

        assert_eq!(
            store
                .get(&preparation.task_id)
                .unwrap()
                .request
                .task_id
                .as_deref(),
            Some(preparation.task_id.as_str())
        );
        assert_eq!(store.claim_start(&confirmed).unwrap(), preparation.task_id);
    }

    #[test]
    fn hermes_install_fails_closed_without_a_verified_installer() {
        let result = CommandRuntime.install_tool(ToolId::Hermes, false);

        assert_eq!(result.status, ToolInstallStatus::Failed);
        assert_eq!(
            result.failure.unwrap().detail.as_deref(),
            Some(
                "no approved verified official installer is available; npm fallback was not selected"
            )
        );
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn production_snapshot_reports_measured_disk_space() {
        let (managed_install_directory, managed_cache_directory) = managed_directories().unwrap();
        let install_existed = managed_install_directory.exists();
        let cache_existed = managed_cache_directory.exists();
        let snapshot = CommandRuntime.snapshot().unwrap();

        assert!(snapshot.available_disk_bytes > 0);
        assert!(snapshot.available_disk_bytes < u64::MAX);
        assert!(snapshot.managed_available_disk_bytes > 0);
        assert!(snapshot.managed_available_disk_bytes < u64::MAX);
        assert_eq!(
            snapshot.temporary_directory_access.path.as_deref(),
            std::env::temp_dir().to_str()
        );
        assert_eq!(
            snapshot.managed_install_directory_access.path.as_deref(),
            managed_install_directory.to_str()
        );
        assert_eq!(
            snapshot.managed_cache_directory_access.path.as_deref(),
            managed_cache_directory.to_str()
        );
        assert_ne!(
            snapshot.temporary_directory_access.state,
            super::super::PathAccessState::Blocked
        );
        assert!(matches!(
            snapshot.managed_install_directory_access.state,
            super::super::PathAccessState::Writable | super::super::PathAccessState::NeedsCreation
        ));
        assert!(matches!(
            snapshot.managed_cache_directory_access.state,
            super::super::PathAccessState::Writable | super::super::PathAccessState::NeedsCreation
        ));
        assert_eq!(managed_install_directory.exists(), install_existed);
        assert_eq!(managed_cache_directory.exists(), cache_existed);
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
    fn successful_install_emits_the_exact_transition_sequence() {
        let store = InstallTaskStore::default();
        let request = InstallRequest {
            task_id: Some("events".into()),
            tools: vec![ToolId::Codex],
            action: super::super::InstallAction::Install,
        };
        let preparation = store.prepare(request.clone(), &Fake).unwrap();
        let mut events = Vec::new();

        store
            .start_with_events(
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
                |event| events.push(event),
            )
            .unwrap();

        assert!(matches!(
            events.as_slice(),
            [
                super::super::InstallTaskEvent::StageChanged {
                    stage: InstallStage::Repairing,
                    ..
                },
                super::super::InstallTaskEvent::StageChanged {
                    stage: InstallStage::InstallingTools,
                    ..
                },
                super::super::InstallTaskEvent::ToolFinished { .. },
                super::super::InstallTaskEvent::StageChanged {
                    stage: InstallStage::Verifying,
                    ..
                },
                super::super::InstallTaskEvent::StageChanged {
                    stage: InstallStage::Completed,
                    ..
                },
                super::super::InstallTaskEvent::Finished {
                    result: InstallTaskResult {
                        status: InstallTaskStatus::Succeeded,
                        ..
                    },
                    ..
                }
            ]
        ));
    }

    #[test]
    fn startup_recovery_emits_completed_then_interrupted_result() {
        let root =
            std::env::temp_dir().join(format!("installer-startup-events-{}", std::process::id()));
        let store = InstallTaskStore::with_persistence(root.clone());
        let request = InstallRequest {
            task_id: Some("persisted-events".into()),
            tools: vec![ToolId::Codex],
            action: super::super::InstallAction::Install,
        };
        store.prepare(request, &Fake).unwrap();
        drop(store);

        let restored = InstallTaskStore::with_persistence(root.clone());
        let events = restored.take_startup_recovery_events();

        assert!(matches!(
            events.as_slice(),
            [
                super::super::InstallTaskEvent::StageChanged {
                    task_id,
                    stage: InstallStage::Completed,
                },
                super::super::InstallTaskEvent::Finished {
                    task_id: finished_task_id,
                    result: InstallTaskResult {
                        status: InstallTaskStatus::NeedsUserAction,
                        ..
                    },
                }
            ] if task_id == "persisted-events" && finished_task_id == "persisted-events"
        ));
        assert!(restored.take_startup_recovery_events().is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cancellation_before_the_worker_starts_emits_one_terminal_sequence() {
        let store = InstallTaskStore::default();
        let request = InstallRequest {
            task_id: Some("cancelled".into()),
            tools: vec![ToolId::Codex],
            action: super::super::InstallAction::Install,
        };
        let preparation = store.prepare(request.clone(), &Fake).unwrap();
        let confirmed = ConfirmedInstallRequest {
            request,
            confirmed_action_ids: preparation
                .plan
                .actions
                .iter()
                .map(|action| action.id.clone())
                .collect(),
        };
        store.claim_start(&confirmed).unwrap();
        let mut cancellation_events = Vec::new();

        store
            .cancel_with_events("cancelled", |event| cancellation_events.push(event))
            .unwrap();
        let mut worker_events = Vec::new();
        store
            .start_with_events(confirmed, &Fake, |event| worker_events.push(event))
            .unwrap();

        assert!(matches!(
            cancellation_events.as_slice(),
            [
                super::super::InstallTaskEvent::StageChanged {
                    task_id,
                    stage: InstallStage::Completed,
                },
                super::super::InstallTaskEvent::Finished {
                    task_id: finished_task_id,
                    result: InstallTaskResult {
                        status: InstallTaskStatus::CancelledByUser,
                        ..
                    },
                }
            ] if task_id == "cancelled" && finished_task_id == "cancelled"
        ));
        assert!(worker_events.is_empty());
    }

    #[test]
    fn cancellation_after_the_last_tool_skips_verification() {
        struct CancellingFake {
            store: InstallTaskStore,
        }

        impl OrchestratorRuntime for CancellingFake {
            fn snapshot(&self) -> Result<EnvironmentSnapshot, InstallFailure> {
                Fake.snapshot()
            }

            fn repair(&self, plan: &RepairPlan) -> Result<(), InstallFailure> {
                Fake.repair(plan)
            }

            fn install_tool(&self, tool: ToolId, _: bool) -> ToolInstallResult {
                self.store.cancel("cancel-during-tool").unwrap();
                let task = self.store.get("cancel-during-tool").unwrap();
                assert_eq!(task.stage, InstallStage::InstallingTools);
                assert!(task.cancellation_requested);
                assert!(task.result.is_none());
                Fake.install_tool(tool, false)
            }
        }

        let store = InstallTaskStore::default();
        let request = InstallRequest {
            task_id: Some("cancel-during-tool".into()),
            tools: vec![ToolId::Codex],
            action: super::super::InstallAction::Install,
        };
        let preparation = store.prepare(request.clone(), &Fake).unwrap();
        let mut events = Vec::new();

        store
            .start_with_events(
                ConfirmedInstallRequest {
                    request,
                    confirmed_action_ids: preparation
                        .plan
                        .actions
                        .iter()
                        .map(|action| action.id.clone())
                        .collect(),
                },
                &CancellingFake {
                    store: store.clone(),
                },
                |event| events.push(event),
            )
            .unwrap();

        assert!(matches!(
            events.as_slice(),
            [
                super::super::InstallTaskEvent::StageChanged {
                    stage: InstallStage::Repairing,
                    ..
                },
                super::super::InstallTaskEvent::StageChanged {
                    stage: InstallStage::InstallingTools,
                    ..
                },
                super::super::InstallTaskEvent::ToolFinished {
                    result: ToolInstallResult {
                        tool: ToolId::Codex,
                        status: ToolInstallStatus::Succeeded,
                        ..
                    },
                    ..
                },
                super::super::InstallTaskEvent::StageChanged {
                    stage: InstallStage::Completed,
                    ..
                },
                super::super::InstallTaskEvent::Finished {
                    result: InstallTaskResult {
                        status: InstallTaskStatus::CancelledByUser,
                        tools,
                        ..
                    },
                    ..
                },
            ] if tools.len() == 1 && tools[0].tool == ToolId::Codex
        ));
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
