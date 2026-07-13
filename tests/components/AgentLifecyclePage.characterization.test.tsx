import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AgentLifecyclePage } from "@/components/AgentLifecyclePage";

const {
  getToolVersions,
  runToolLifecycleAction,
  probeToolInstallations,
  prepareInstall,
  startInstall,
  getActiveInstall,
  getInstallTask,
  cancelInstall,
  listenInstall,
  replayInstall,
  listenAllInstall,
  listenExitBlocked,
} = vi.hoisted(() => ({
  getToolVersions: vi.fn(),
  runToolLifecycleAction: vi.fn(),
  probeToolInstallations: vi.fn(),
  prepareInstall: vi.fn(),
  startInstall: vi.fn(),
  getActiveInstall: vi.fn(),
  getInstallTask: vi.fn(),
  cancelInstall: vi.fn(),
  listenInstall: vi.fn(),
  replayInstall: vi.fn(),
  listenAllInstall: vi.fn(),
  listenExitBlocked: vi.fn(),
}));
const toastError = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api", () => ({
  settingsApi: {
    getToolVersions,
    runToolLifecycleAction,
    probeToolInstallations,
  },
  installerApi: {
    prepare: prepareInstall,
    start: startInstall,
    getActiveTask: getActiveInstall,
    getTask: getInstallTask,
    cancel: cancelInstall,
    listen: listenInstall,
    replayStartupRecovery: replayInstall,
    listenAll: listenAllInstall,
    listenExitBlocked,
  },
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, unknown>) =>
      values ? `${key}:${JSON.stringify(values)}` : key,
  }),
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: toastError,
    warning: vi.fn(),
    info: vi.fn(),
  },
}));

const toolVersions = [
  "claude",
  "codex",
  "gemini",
  "opencode",
  "openclaw",
  "hermes",
].map((name) => ({
  name,
  version: null,
  latest_version: "1.0.0",
  error: null,
  installed_but_broken: false,
  env_type: "macos" as const,
  wsl_distro: null,
}));

const makeToolVersions = (
  installed: Set<string> = new Set(),
  outdated: Set<string> = new Set(),
) =>
  toolVersions.map((tool) => ({
    ...tool,
    version: installed.has(tool.name) ? "0.9.0" : null,
    latest_version: outdated.has(tool.name) ? "1.0.0" : tool.latest_version,
  }));

describe("AgentLifecyclePage characterization", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getToolVersions.mockResolvedValue(toolVersions);
    runToolLifecycleAction.mockResolvedValue(undefined);
    probeToolInstallations.mockResolvedValue([
      {
        tool: "claude",
        installs: [
          {
            path: "/tmp/claude",
            version: "1.0.0",
            runnable: true,
          },
        ],
        is_conflict: true,
        needs_confirmation: true,
        command: "npm install -g @anthropic-ai/claude-code@latest",
        anchored: true,
      },
    ]);
    getActiveInstall.mockResolvedValue(null);
    replayInstall.mockResolvedValue(0);
    listenInstall.mockResolvedValue(vi.fn());
    listenAllInstall.mockResolvedValue(vi.fn());
    listenExitBlocked.mockResolvedValue(vi.fn());
    prepareInstall.mockResolvedValue({
      task_id: "install-1",
      requires_confirmation: false,
      plan: { actions: [] },
    });
    startInstall.mockResolvedValue("install-1");
  });

  it("loads all six tools through the existing version API", async () => {
    render(<AgentLifecyclePage />);

    await waitFor(() => expect(getToolVersions).toHaveBeenCalledTimes(6));

    expect(getToolVersions.mock.calls).toEqual(
      ["claude", "codex", "gemini", "opencode", "openclaw", "hermes"].map(
        (tool) => [[tool], {}],
      ),
    );

    expect(screen.getByText("Claude Code")).toBeInTheDocument();
    expect(screen.getByText("Codex")).toBeInTheDocument();
    expect(screen.getByText("Gemini CLI")).toBeInTheDocument();
    expect(screen.getAllByText("OpenCode").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("OpenClaw")).toBeInTheDocument();
    expect(screen.getByText("Hermes")).toBeInTheDocument();
  });

  it("routes native installs through the installer API and preserves diagnosis after closing the flow", async () => {
    render(<AgentLifecyclePage />);
    const installButtons = await screen.findAllByText("settings.toolInstall");

    fireEvent.click(installButtons[0]);
    await waitFor(() => {
      expect(prepareInstall).toHaveBeenCalledWith({
        task_id: null,
        tools: ["claude"],
        action: "install",
      });
      expect(startInstall).toHaveBeenCalled();
    });

    const completedTask = {
      task_id: "install-1",
      request: { task_id: "install-1", tools: ["claude"], action: "install" },
      stage: "completed",
      plan: { actions: [] },
      cancellation_requested: false,
      interrupted: false,
      result: {
        status: "succeeded",
        failure: null,
        tools: [
          {
            tool: "claude",
            status: "succeeded",
            version: "1.0.0",
            path: "/tmp/claude",
            failure: null,
          },
        ],
      },
    };
    getInstallTask.mockResolvedValue(completedTask);
    await waitFor(() => expect(listenAllInstall).toHaveBeenCalled());
    const taskListener = listenAllInstall.mock.calls.at(-1)?.[0] as (event: {
      type: "finished";
      task_id: string;
      result: unknown;
    }) => Promise<void>;
    await taskListener({
      type: "finished",
      task_id: "install-1",
      result: completedTask.result,
    });
    await screen.findByText("settings.installer.result");
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() =>
      expect(screen.getByText("settings.toolDiagnose")).not.toBeDisabled(),
    );
    fireEvent.click(screen.getByText("settings.toolDiagnose"));
    await waitFor(() => {
      expect(probeToolInstallations).toHaveBeenCalledWith([
        "claude",
        "codex",
        "gemini",
        "opencode",
        "openclaw",
        "hermes",
      ]);
    });
  });

  it("updates an installed outdated tool through preflight confirmation", async () => {
    getToolVersions.mockResolvedValue(
      makeToolVersions(new Set(["claude"]), new Set(["claude"])),
    );
    probeToolInstallations.mockResolvedValue([
      {
        tool: "claude",
        installs: [
          {
            path: "/tmp/claude",
            source: "npm",
            version: "0.9.0",
            runnable: true,
          },
        ],
        is_conflict: true,
        needs_confirmation: true,
        command: "npm install -g @anthropic-ai/claude-code@latest",
        anchored: true,
      },
    ]);
    render(<AgentLifecyclePage />);

    fireEvent.click(await screen.findByText("common.refresh"));
    await waitFor(() =>
      expect(screen.getAllByText("settings.toolUpdate")).toHaveLength(1),
    );

    fireEvent.click(screen.getByText("settings.toolUpdate"));
    await waitFor(() =>
      expect(probeToolInstallations).toHaveBeenCalledWith(["claude"]),
    );
    fireEvent.click(screen.getByText("settings.toolUpgradeConfirmBtn"));

    await waitFor(() => {
      expect(runToolLifecycleAction).toHaveBeenCalledWith(
        ["claude"],
        "update",
        {},
      );
    });
  });

  it("renders the diagnosed installation path in the conflict UI", async () => {
    render(<AgentLifecyclePage />);

    fireEvent.click(await screen.findByText("settings.toolDiagnose"));

    await waitFor(() => {
      expect(screen.getByText("/tmp/claude")).toBeInTheDocument();
    });
  });

  it("continues a batch update after one tool fails", async () => {
    getToolVersions.mockResolvedValue(
      makeToolVersions(
        new Set(["claude", "codex"]),
        new Set(["claude", "codex"]),
      ),
    );
    probeToolInstallations.mockResolvedValue([]);
    runToolLifecycleAction
      .mockRejectedValueOnce(new Error("claude failed"))
      .mockResolvedValueOnce(undefined);
    render(<AgentLifecyclePage />);

    fireEvent.click(await screen.findByText("common.refresh"));
    await waitFor(() =>
      expect(
        screen.getByText('settings.updateAllTools:{"count":2}'),
      ).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByText('settings.updateAllTools:{"count":2}'));

    await waitFor(() =>
      expect(runToolLifecycleAction).toHaveBeenCalledTimes(2),
    );
    expect(runToolLifecycleAction.mock.calls[0]).toEqual([
      ["claude"],
      "update",
      {},
    ]);
    expect(runToolLifecycleAction.mock.calls[1]).toEqual([
      ["codex"],
      "update",
      {},
    ]);
    expect(runToolLifecycleAction.mock.invocationCallOrder[1]).toBeGreaterThan(
      runToolLifecycleAction.mock.invocationCallOrder[0],
    );
  });

  it("opens the guided native install flow instead of the legacy executor", async () => {
    render(<AgentLifecyclePage />);
    const installButtons = await screen.findAllByText("settings.toolInstall");

    fireEvent.click(installButtons[0]);
    await waitFor(() => expect(prepareInstall).toHaveBeenCalledOnce());
    expect(runToolLifecycleAction).not.toHaveBeenCalledWith(
      ["claude"],
      "install",
      {},
    );
  });
});
