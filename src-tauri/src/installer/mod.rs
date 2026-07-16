mod atomic_file;
pub mod failure;
mod managed_npm;
mod model;
mod node_pair_resolution;
pub mod orchestrator;
mod path_persistence;
pub mod platform;
mod policy;
pub mod probe;
pub mod repair;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod system_access;
mod task_ids;
mod task_status;
pub mod tools;
pub mod verifier;
mod version_output;

use tauri::Emitter;

pub(crate) use atomic_file::{replace_file, unique_temporary_path};
pub use failure::{classify_process_failure, redact_diagnostic};
pub use model::*;
pub(crate) use node_pair_resolution::{resolve_pair_with_refresh, PairSource};
pub use orchestrator::{CommandRuntime, InstallTaskStore, OrchestratorRuntime};
pub use platform::{
    cleanup_task_temp, download_and_verify_node, fetch_node_index, install_node,
    install_node_release, refresh_environment, resolve_node_release, selected_environment,
    NodeRelease, PlatformAdapter, TaskTempGuard,
};
pub use policy::{node_policy, NodePolicy};
pub use probe::{CommandProbe, ProbeConfig, ProbeRunner, SystemProbe};
pub use repair::build_repair_plan;
pub(crate) use task_ids::next_sequence_from_task_ids;
pub(crate) use task_status::{aggregate_tool_outcomes, AggregatedTaskStatus, ToolOutcome};
pub use tools::{tool_strategy, SharedDependency, ToolInstallMethod, ToolInstallStrategy, ToolKey};
pub use verifier::{
    verify_tool, verify_tool_detailed, CleanEnvironment, DependencyCandidates, ExecutableBaseline,
    ResolvedNodeNpmPair, ToolCommandOutput, ToolRunner, ToolVerification,
};
pub(crate) use version_output::parse_command_version_output;

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
) -> Result<StartInstallOutcome, String> {
    let outcome = store
        .claim_or_refresh(&request, &CommandRuntime)
        .map_err(|error| error.message_key)?;
    let StartInstallOutcome::Started {
        task_id: claimed_task_id,
    } = outcome
    else {
        return Ok(outcome);
    };
    let task_store = store.inner().clone();
    let emitted_task_id = claimed_task_id.clone();
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
    });
    Ok(StartInstallOutcome::Started {
        task_id: claimed_task_id,
    })
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
pub fn get_active_install_task(
    store: tauri::State<'_, InstallTaskStore>,
) -> Option<InstallTaskSnapshot> {
    store.active_task()
}

/// Frontend listeners call this after subscribing to installation events so
/// recovered terminal state cannot be lost during Tauri application setup.
#[tauri::command]
pub fn replay_startup_install_recovery(
    store: tauri::State<'_, InstallTaskStore>,
    app: tauri::AppHandle,
) -> usize {
    let events = store.take_startup_recovery_events();
    let count = events.len();
    for event in events {
        let _ = app.emit("agent-manager://install-task", event);
    }
    count
}

#[tauri::command]
pub fn cancel_install_task(
    task_id: String,
    store: tauri::State<'_, InstallTaskStore>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    store.cancel_with_events(&task_id, |event| {
        let _ = app.emit("agent-manager://install-task", event);
    })
}
