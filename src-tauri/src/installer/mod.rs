pub mod failure;
mod model;
pub mod orchestrator;
pub mod platform;
mod policy;
pub mod probe;
pub mod repair;
pub mod tools;
pub mod verifier;

use tauri::Emitter;

pub use failure::{classify_process_failure, redact_diagnostic};
pub use model::*;
pub use orchestrator::{CommandRuntime, InstallTaskStore, OrchestratorRuntime};
pub use platform::{
    cleanup_task_temp, download_and_verify_node, install_node, install_node_release,
    refresh_environment, resolve_node_release, selected_environment, NodeRelease, PlatformAdapter,
    TaskTempGuard,
};
pub use policy::{node_policy, NodePolicy};
pub use probe::{CommandProbe, ProbeConfig, ProbeRunner, SystemProbe};
pub use repair::build_repair_plan;
pub use tools::{tool_strategy, SharedDependency, ToolInstallMethod, ToolInstallStrategy, ToolKey};
pub use verifier::{
    verify_tool, verify_tool_detailed, CleanEnvironment, DependencyCandidates, ExecutableBaseline,
    ResolvedNodeNpmPair, ToolCommandOutput, ToolRunner, ToolVerification,
};

#[tauri::command]
pub fn prepare_tool_install(
    request: InstallRequest,
    store: tauri::State<'_, InstallTaskStore>,
) -> Result<InstallPreparation, String> {
    store
        .prepare(request, &CommandRuntime)
        .map_err(|error| error.detail.unwrap_or_else(|| error.message_key))
}

#[tauri::command]
pub fn start_tool_install(
    request: ConfirmedInstallRequest,
    store: tauri::State<'_, InstallTaskStore>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let task_id = store
        .start(request, &CommandRuntime)
        .map_err(|error| error.detail.unwrap_or_else(|| error.message_key))?;
    if let Some(task) = store.get(&task_id) {
        let _ = app.emit(
            "agent-manager://install-task",
            InstallTaskEvent::Finished {
                task_id: task_id.clone(),
                result: task.result.unwrap_or(InstallTaskResult {
                    status: InstallTaskStatus::Failed,
                    tools: Vec::new(),
                    failure: None,
                }),
            },
        );
    }
    Ok(task_id)
}

#[tauri::command]
pub fn get_install_task(
    task_id: String,
    store: tauri::State<'_, InstallTaskStore>,
) -> Result<InstallTaskSnapshot, String> {
    store
        .get(&task_id)
        .ok_or_else(|| "unknown install task".to_string())
}

#[tauri::command]
pub fn cancel_install_task(
    task_id: String,
    store: tauri::State<'_, InstallTaskStore>,
) -> Result<(), String> {
    store.cancel(&task_id)
}
