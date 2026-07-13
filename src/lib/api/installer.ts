import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

export type ToolId =
  | "claude"
  | "codex"
  | "gemini"
  | "opencode"
  | "openclaw"
  | "hermes";
export type InstallAction = "install" | "update";
export type InstallStage =
  | "preflight"
  | "awaiting_confirmation"
  | "repairing"
  | "installing_tools"
  | "verifying"
  | "completed";
export type ActionStatus =
  | "pending"
  | "running"
  | "succeeded"
  | "failed"
  | "skipped";
export type RepairActionKind =
  | "install_node"
  | "upgrade_node"
  | "repair_node"
  | "refresh_environment"
  | "install_npm"
  | "update_path";
export type ToolInstallStatus =
  | "succeeded"
  | "failed"
  | "installed_not_runnable"
  | "skipped";
export type InstallTaskStatus =
  | "succeeded"
  | "succeeded_with_conflicts"
  | "installed_not_runnable"
  | "cancelled_by_user"
  | "needs_user_action"
  | "failed";
export type InstallFailureCode =
  | "unsupported_platform"
  | "unsupported_architecture"
  | "insufficient_disk_space"
  | "dependency_missing"
  | "dependency_too_old"
  | "dependency_broken"
  | "path_not_visible"
  | "multiple_installations"
  | "permission_denied"
  | "privilege_declined"
  | "file_in_use"
  | "dns_failure"
  | "network_timeout"
  | "proxy_unreachable"
  | "tls_failure"
  | "download_integrity_failure"
  | "signature_verification_failure"
  | "installer_failure"
  | "tool_install_failure"
  | "verification_failure";
export type RecommendedAction =
  | "retry"
  | "repair_dependencies"
  | "free_disk_space"
  | "grant_permission"
  | "close_blocking_process"
  | "check_network"
  | "check_proxy"
  | "resolve_multiple_installations"
  | "reinstall"
  | "view_diagnostics";

export interface InstallRequest {
  task_id: string | null;
  tools: ToolId[];
  action: InstallAction;
}
export interface ConfirmedInstallRequest {
  request: InstallRequest;
  confirmed_action_ids: string[];
}
export interface RepairAction {
  id: string;
  kind: RepairActionKind;
  requires_confirmation: boolean;
  requires_elevation: boolean;
  status: ActionStatus;
}
export interface RepairPlan {
  actions: RepairAction[];
}
export interface InstallPreparation {
  task_id: string;
  plan: RepairPlan;
  requires_confirmation: boolean;
}
export interface InstallFailure {
  code: InstallFailureCode;
  stage: InstallStage;
  exit_code: number | null;
  retryable: boolean;
  requires_user_action: boolean;
  message_key: string;
  recommended_action: RecommendedAction;
  detail: string | null;
}
export interface ToolInstallResult {
  tool: ToolId;
  status: ToolInstallStatus;
  version: string | null;
  path: string | null;
  failure: InstallFailure | null;
}
export interface InstallTaskResult {
  status: InstallTaskStatus;
  tools: ToolInstallResult[];
  failure: InstallFailure | null;
}
export interface InstallTaskSnapshot {
  task_id: string;
  request: InstallRequest;
  stage: InstallStage;
  plan: RepairPlan;
  result: InstallTaskResult | null;
  cancellation_requested: boolean;
  interrupted: boolean;
}
export type InstallTaskEvent =
  | { type: "stage_changed"; task_id: string; stage: InstallStage }
  | {
      type: "action_changed";
      task_id: string;
      action_id: string;
      status: ActionStatus;
    }
  | { type: "tool_finished"; task_id: string; result: ToolInstallResult }
  | { type: "finished"; task_id: string; result: InstallTaskResult };

const eventName = "agent-manager://install-task";

export const installerApi = {
  prepare: (request: InstallRequest) =>
    invoke<InstallPreparation>("prepare_tool_install", { request }),
  start: (request: ConfirmedInstallRequest) =>
    invoke<string>("start_tool_install", { request }),
  getTask: (taskId: string) =>
    invoke<InstallTaskSnapshot>("get_install_task", { taskId }),
  cancel: (taskId: string) => invoke<void>("cancel_install_task", { taskId }),
  replayStartupRecovery: () =>
    invoke<number>("replay_startup_install_recovery"),
  listen: async (
    taskId: string,
    handler: (event: InstallTaskEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<InstallTaskEvent>(eventName, ({ payload }) => {
      if (payload.task_id === taskId) handler(payload);
    }),
};
