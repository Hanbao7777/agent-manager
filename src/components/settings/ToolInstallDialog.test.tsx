import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ToolInstallDialog } from "./ToolInstallDialog";
import type { InstallPreparation } from "@/lib/api/installer";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      ({
        "settings.installer.administrator": "Administrator permission required",
        "settings.installer.continue": "Continue",
        "settings.installer.installed": "Installed",
        "settings.installer.failed": "Failed",
        "settings.installer.action.install_node": "Install Node.js",
        "settings.installer.retry": "Retry",
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
      />,
    );

    expect(screen.getByText("Codex")).toBeInTheDocument();
    expect(screen.getByText("Gemini CLI")).toBeInTheDocument();
    expect(screen.getByText("Installed")).toBeInTheDocument();
    expect(screen.getByText("Failed")).toBeInTheDocument();
    expect(screen.getByText("redacted diagnostic")).toBeInTheDocument();
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

  it("shows a retryable task failure code and retry action", () => {
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

    expect(screen.getByText("network_timeout")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledOnce();
  });
});
