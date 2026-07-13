import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ToolInstallDialog } from "./ToolInstallDialog";
import type {
  InstallPreparation,
  InstallTaskStatus,
} from "@/lib/api/installer";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTW from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      ({
        "settings.installer.administrator": "Administrator permission required",
        "settings.installer.continue": "Continue",
        "settings.installer.installed": "Installed",
        "settings.installer.failed": "Failed",
        "settings.installer.status.succeeded": "Installed",
        "settings.installer.status.failed": "Failed",
        "settings.installer.status.installed_not_runnable":
          "Installed but not runnable",
        "settings.installer.status.skipped": "Skipped",
        "settings.installer.resultStatus.succeeded": "Installation completed",
        "settings.installer.resultStatus.succeeded_with_conflicts":
          "Installation completed with conflicts",
        "settings.installer.resultStatus.installed_not_runnable":
          "Installation completed but tool cannot run",
        "settings.installer.resultStatus.cancelled_by_user":
          "Installation cancelled",
        "settings.installer.resultStatus.needs_user_action":
          "Installation needs your attention",
        "settings.installer.resultStatus.failed": "Installation failed",
        "settings.installer.retry": "Retry",
        "settings.installer.diagnostics": "Diagnostics",
        "settings.installer.failure.network_timeout":
          "Network request timed out",
        "settings.installer.action.install_node": "Install Node.js",
      })[key] ?? key,
  }),
}));

const preparation: InstallPreparation = {
  task_id: "install-1",
  requires_confirmation: true,
  plan: {
    actions: [
      {
        id: "install-node",
        kind: "install_node",
        requires_confirmation: true,
        requires_elevation: true,
        status: "pending",
      },
    ],
  },
};

const keyShape = (value: unknown): unknown => {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;

  return Object.fromEntries(
    Object.entries(value)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, nested]) => [key, keyShape(nested)]),
  );
};

describe("ToolInstallDialog", () => {
  it("requires confirmation for privileged repairs before continuing", () => {
    const onConfirm = vi.fn();

    render(
      <ToolInstallDialog
        open
        preparation={preparation}
        task={null}
        toolName={(tool) => tool}
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("Install Node.js")).toBeInTheDocument();
    expect(
      screen.getByText("Administrator permission required"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();

    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));

    expect(onConfirm).toHaveBeenCalledWith(["install-node"]);
  });

  it("shows successful and failed tools in one batch result", () => {
    const onRetry = vi.fn();

    render(
      <ToolInstallDialog
        open
        preparation={null}
        task={{
          task_id: "install-1",
          request: {
            task_id: "install-1",
            tools: ["codex", "gemini"],
            action: "install",
          },
          stage: "completed",
          plan: { actions: [] },
          cancellation_requested: false,
          interrupted: false,
          result: {
            status: "needs_user_action",
            failure: null,
            tools: [
              {
                tool: "codex",
                status: "succeeded",
                version: "1.0.0",
                path: null,
                failure: null,
              },
              {
                tool: "gemini",
                status: "failed",
                version: null,
                path: null,
                failure: {
                  code: "tool_install_failure",
                  stage: "installing_tools",
                  exit_code: 1,
                  retryable: true,
                  requires_user_action: false,
                  message_key: "installer.failure",
                  recommended_action: "retry",
                  detail: "redacted diagnostic",
                },
              },
            ],
          },
        }}
        toolName={(tool) => (tool === "gemini" ? "Gemini CLI" : "Codex")}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
        onRetry={onRetry}
      />,
    );

    expect(screen.getByText("Codex")).toBeInTheDocument();
    expect(screen.getByText("Gemini CLI")).toBeInTheDocument();
    expect(screen.getByText("Installed")).toBeInTheDocument();
    expect(screen.getAllByText("Failed")).toHaveLength(1);
    expect(screen.getByText("redacted diagnostic")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it("uses distinct labels for every tool installation status", () => {
    render(
      <ToolInstallDialog
        open
        preparation={null}
        task={{
          task_id: "install-1",
          request: {
            task_id: "install-1",
            tools: ["claude", "codex", "gemini", "opencode"],
            action: "install",
          },
          stage: "completed",
          plan: { actions: [] },
          cancellation_requested: false,
          interrupted: false,
          result: {
            status: "needs_user_action",
            failure: null,
            tools: [
              {
                tool: "claude",
                status: "succeeded",
                version: "1.0.0",
                path: null,
                failure: null,
              },
              {
                tool: "codex",
                status: "failed",
                version: null,
                path: null,
                failure: null,
              },
              {
                tool: "gemini",
                status: "installed_not_runnable",
                version: "1.0.0",
                path: null,
                failure: null,
              },
              {
                tool: "opencode",
                status: "skipped",
                version: null,
                path: null,
                failure: null,
              },
            ],
          },
        }}
        toolName={(tool) => tool}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("Installed")).toBeInTheDocument();
    expect(screen.getByText("Failed")).toBeInTheDocument();
    expect(screen.getByText("Installed but not runnable")).toBeInTheDocument();
    expect(screen.getByText("Skipped")).toBeInTheDocument();
  });

  it("shows the overall result status when no tool result is available", () => {
    const statuses: Array<[InstallTaskStatus, string]> = [
      ["succeeded", "Installation completed"],
      ["succeeded_with_conflicts", "Installation completed with conflicts"],
      ["installed_not_runnable", "Installation completed but tool cannot run"],
      ["cancelled_by_user", "Installation cancelled"],
      ["needs_user_action", "Installation needs your attention"],
      ["failed", "Installation failed"],
    ];
    const { rerender } = render(
      <ToolInstallDialog
        open
        preparation={null}
        task={null}
        toolName={(tool) => tool}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    for (const [status, label] of statuses) {
      rerender(
        <ToolInstallDialog
          open
          preparation={null}
          task={{
            task_id: "install-1",
            request: { task_id: "install-1", tools: [], action: "install" },
            stage: "completed",
            plan: { actions: [] },
            cancellation_requested: status === "cancelled_by_user",
            interrupted: false,
            result: { status, tools: [], failure: null },
          }}
          toolName={(tool) => tool}
          onConfirm={vi.fn()}
          onCancel={vi.fn()}
        />,
      );

      expect(screen.getByText(label)).toBeInTheDocument();
    }
  });

  it("requires fresh consent when a dialog is reopened for another task", () => {
    const onConfirm = vi.fn();
    const { rerender } = render(
      <ToolInstallDialog
        open
        preparation={preparation}
        task={null}
        toolName={(tool) => tool}
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("checkbox"));
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();

    rerender(
      <ToolInstallDialog
        open={false}
        preparation={preparation}
        task={null}
        toolName={(tool) => tool}
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );
    rerender(
      <ToolInstallDialog
        open
        preparation={{ ...preparation, task_id: "install-2" }}
        task={null}
        toolName={(tool) => tool}
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
  });

  it("renders a task-level failure when no tool result exists", () => {
    render(
      <ToolInstallDialog
        open
        preparation={null}
        task={{
          task_id: "install-1",
          request: { task_id: "install-1", tools: [], action: "install" },
          stage: "completed",
          plan: { actions: [] },
          cancellation_requested: false,
          interrupted: true,
          result: {
            status: "failed",
            tools: [],
            failure: {
              code: "installer_failure",
              stage: "repairing",
              exit_code: null,
              retryable: true,
              requires_user_action: true,
              message_key: "installer.failure",
              recommended_action: "retry",
              detail: "interrupted before installing tools",
            },
          },
        }}
        toolName={(tool) => tool}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(
      screen.getByText("interrupted before installing tools"),
    ).toBeInTheDocument();
  });

  it("shows diagnostics and retries a retryable task failure", () => {
    const onRetry = vi.fn();

    render(
      <ToolInstallDialog
        open
        preparation={null}
        task={{
          task_id: "install-1",
          request: { task_id: "install-1", tools: [], action: "install" },
          stage: "completed",
          plan: { actions: [] },
          cancellation_requested: false,
          interrupted: false,
          result: {
            status: "failed",
            tools: [],
            failure: {
              code: "network_timeout",
              stage: "repairing",
              exit_code: null,
              retryable: true,
              requires_user_action: false,
              message_key: "installer.failure",
              recommended_action: "retry",
              detail: "request timed out",
            },
          },
        }}
        toolName={(tool) => tool}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
        onRetry={onRetry}
      />,
    );

    expect(screen.getByText("Network request timed out")).toBeInTheDocument();
    expect(screen.getByText("Diagnostics")).toBeInTheDocument();
    expect(screen.queryByText("network_timeout")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it("falls back to the localized reason for a task failure without details", () => {
    render(
      <ToolInstallDialog
        open
        preparation={null}
        task={{
          task_id: "install-1",
          request: { task_id: "install-1", tools: [], action: "install" },
          stage: "completed",
          plan: { actions: [] },
          cancellation_requested: false,
          interrupted: false,
          result: {
            status: "failed",
            tools: [],
            failure: {
              code: "network_timeout",
              stage: "repairing",
              exit_code: null,
              retryable: true,
              requires_user_action: false,
              message_key: "installer.failure",
              recommended_action: "retry",
              detail: null,
            },
          },
        }}
        toolName={(tool) => tool}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("Network request timed out")).toBeInTheDocument();
    expect(screen.queryByText("network_timeout")).not.toBeInTheDocument();
  });

  it("falls back to the localized reason for a tool failure without details", () => {
    render(
      <ToolInstallDialog
        open
        preparation={null}
        task={{
          task_id: "install-1",
          request: {
            task_id: "install-1",
            tools: ["codex"],
            action: "install",
          },
          stage: "completed",
          plan: { actions: [] },
          cancellation_requested: false,
          interrupted: false,
          result: {
            status: "failed",
            failure: null,
            tools: [
              {
                tool: "codex",
                status: "failed",
                version: null,
                path: null,
                failure: {
                  code: "network_timeout",
                  stage: "installing_tools",
                  exit_code: null,
                  retryable: true,
                  requires_user_action: false,
                  message_key: "installer.failure",
                  recommended_action: "retry",
                  detail: null,
                },
              },
            ],
          },
        }}
        toolName={(tool) => tool}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("Network request timed out")).toBeInTheDocument();
    expect(screen.queryByText("network_timeout")).not.toBeInTheDocument();
  });

  it("keeps installer locale key sets aligned", () => {
    expect(keyShape(ja.settings.installer)).toEqual(
      keyShape(en.settings.installer),
    );
    expect(keyShape(zh.settings.installer)).toEqual(
      keyShape(en.settings.installer),
    );
    expect(keyShape(zhTW.settings.installer)).toEqual(
      keyShape(en.settings.installer),
    );
  });
});
