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
    cleanup_task_temp, download_and_verify_node, fetch_node_index, install_node,
    install_node_release, refresh_environment, resolve_node_release, selected_environment,
    NodeRelease, PlatformAdapter, TaskTempGuard,
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
    app: tauri::AppHandle,
) -> Result<InstallPreparation, String> {
    let preparation = store
        .prepare(request, &CommandRuntime)
        .map_err(|error| error.detail.unwrap_or_else(|| error.message_key))?;
    emit_stage(&app, &preparation.task_id, InstallStage::Preflight);
    if preparation.requires_confirmation {
        emit_stage(
            &app,
            &preparation.task_id,
            InstallStage::AwaitingConfirmation,
        );
    }
    Ok(preparation)
}

#[tauri::command]
pub fn start_tool_install(
    request: ConfirmedInstallRequest,
    store: tauri::State<'_, InstallTaskStore>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let task_id = request
        .request
        .task_id
        .clone()
        .ok_or_else(|| "missing task id".to_string())?;
    store
        .claim_start(&request)
        .map_err(|error| error.detail.unwrap_or_else(|| error.message_key))?;
    let task_store = store.inner().clone();
    let emitted_task_id = task_id.clone();
    std::thread::spawn(move || {
        let event_app = app.clone();
        let result = task_store.start_with_events(request, &CommandRuntime, move |event| {
            let _ = event_app.emit("agent-manager://install-task", event);
        });
        task_store.release_claim(&emitted_task_id);
        if let Err(error) = result {
            if let Some(result) = task_store.fail_background(&emitted_task_id, error) {
                let _ = app.emit(
                    "agent-manager://install-task",
                    InstallTaskEvent::StageChanged {
                        task_id: emitted_task_id.clone(),
                        stage: InstallStage::Completed,
                    },
                );
                let _ = app.emit(
                    "agent-manager://install-task",
                    InstallTaskEvent::Finished {
                        task_id: emitted_task_id,
                        result,
                    },
                );
            }
        }
        // The validation failure is retained in the store only when a task
        // existed; no synthetic stage sequence is emitted after the fact.
        let _ = emitted_task_id;
    });
    Ok(task_id)
}

fn emit_stage(app: &tauri::AppHandle, task_id: &str, stage: InstallStage) {
    let _ = app.emit(
        "agent-manager://install-task",
        InstallTaskEvent::StageChanged {
            task_id: task_id.into(),
            stage,
        },
    );
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
    app: tauri::AppHandle,
) -> Result<(), String> {
    store.cancel(&task_id)?;
    if let Some(task) = store.get(&task_id) {
        if let Some(result) = task.result {
            let _ = app.emit(
                "agent-manager://install-task",
                InstallTaskEvent::StageChanged {
                    task_id: task_id.clone(),
                    stage: InstallStage::Completed,
                },
            );
            let _ = app.emit(
                "agent-manager://install-task",
                InstallTaskEvent::Finished { task_id, result },
            );
        }
    }
    Ok(())
}
