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
const { toastSuccess, toastError, toastWarning, toastInfo } = vi.hoisted(
  () => ({
    toastSuccess: vi.fn(),
    toastError: vi.fn(),
    toastWarning: vi.fn(),
    toastInfo: vi.fn(),
  }),
);
const translate = vi.hoisted(
  () => (key: string, values?: Record<string, unknown>) =>
    values ? `${key}:${JSON.stringify(values)}` : key,
);

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
    t: translate,
  }),
}));

vi.mock("sonner", () => ({
  toast: {
    success: toastSuccess,
    error: toastError,
    warning: toastWarning,
    info: toastInfo,
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
    startInstall.mockResolvedValue({ type: "started", task_id: "install-1" });
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

  it("cleans a listener that resolves after unmount without replaying recovery", async () => {
    let resolveListener!: (unlisten: () => void) => void;
    const lateListener = new Promise<() => void>((resolve) => {
      resolveListener = resolve;
    });
    const unlisten = vi.fn();
    listenAllInstall.mockReturnValue(lateListener);

    const view = render(<AgentLifecyclePage />);
    await waitFor(() => expect(listenAllInstall).toHaveBeenCalledOnce());
    view.unmount();
    resolveListener(unlisten);
    await lateListener;

    expect(unlisten).toHaveBeenCalledOnce();
    expect(listenExitBlocked).not.toHaveBeenCalled();
    expect(getActiveInstall).not.toHaveBeenCalled();
    expect(replayInstall).not.toHaveBeenCalled();
  });

  it("does not replay recovery when getActiveTask resolves after unmount", async () => {
    let resolveActive!: (task: unknown) => void;
    const activeTask = new Promise<unknown>((resolve) => {
      resolveActive = resolve;
    });
    const unlistenInstall = vi.fn();
    const unlistenExit = vi.fn();
    getActiveInstall.mockReturnValue(activeTask);
    listenAllInstall.mockResolvedValue(unlistenInstall);
    listenExitBlocked.mockResolvedValue(unlistenExit);

    const view = render(<AgentLifecyclePage />);
    await waitFor(() => expect(getActiveInstall).toHaveBeenCalledOnce());
    view.unmount();
    resolveActive({
      task_id: "install-1",
      request: { task_id: "install-1", tools: ["claude"], action: "install" },
      stage: "installing_tools",
      plan: { actions: [] },
      result: null,
      cancellation_requested: false,
      interrupted: false,
    });
    await activeTask;

    expect(unlistenInstall).toHaveBeenCalledOnce();
    expect(unlistenExit).toHaveBeenCalledOnce();
    expect(replayInstall).not.toHaveBeenCalled();
    expect(toastSuccess).not.toHaveBeenCalled();
    expect(toastWarning).not.toHaveBeenCalled();
    expect(toastError).not.toHaveBeenCalled();
  });

  it("does not apply a task event that resolves after unmount", async () => {
    let resolveTask!: (task: unknown) => void;
    const taskSnapshot = new Promise<unknown>((resolve) => {
      resolveTask = resolve;
    });
    const unlisten = vi.fn();
    getInstallTask.mockReturnValue(taskSnapshot);
    listenAllInstall.mockResolvedValue(unlisten);
    const view = render(<AgentLifecyclePage />);
    await waitFor(() => expect(replayInstall).toHaveBeenCalledOnce());
    const listener = listenAllInstall.mock.calls[0][0] as (
      event: unknown,
    ) => Promise<void>;
    const applyEvent = listener({
      type: "finished",
      task_id: "install-1",
      result: { status: "needs_user_action", tools: [] },
    });
    await waitFor(() =>
      expect(getInstallTask).toHaveBeenCalledWith("install-1"),
    );
    view.unmount();
    resolveTask({
      task_id: "install-1",
      request: { task_id: "install-1", tools: ["claude"], action: "install" },
      stage: "completed",
      plan: { actions: [] },
      result: { status: "needs_user_action", tools: [] },
      cancellation_requested: false,
      interrupted: false,
    });
    await applyEvent;

    expect(unlisten).toHaveBeenCalledOnce();
    expect(toastSuccess).not.toHaveBeenCalled();
    expect(toastWarning).not.toHaveBeenCalled();
    expect(toastError).not.toHaveBeenCalled();
    expect(probeToolInstallations).not.toHaveBeenCalled();
  });

  it("does not continue an installer result after refresh resolves post-unmount", async () => {
    let resolveRefresh!: (versions: typeof toolVersions) => void;
    const refresh = new Promise<typeof toolVersions>((resolve) => {
      resolveRefresh = resolve;
    });
    const unlisten = vi.fn();
    const completedTask = {
      task_id: "install-1",
      request: { task_id: "install-1", tools: ["claude"], action: "install" },
      stage: "completed",
      plan: { actions: [] },
      result: {
        status: "needs_user_action",
        tools: [{ tool: "claude", status: "installed_not_runnable" }],
      },
      cancellation_requested: false,
      interrupted: false,
    };
    getInstallTask.mockResolvedValue(completedTask);
    listenAllInstall.mockResolvedValue(unlisten);
    const view = render(<AgentLifecyclePage />);
    await waitFor(() => expect(replayInstall).toHaveBeenCalledOnce());
    await waitFor(() =>
      expect(screen.getAllByText("common.notInstalled")).toHaveLength(6),
    );
    getToolVersions.mockClear();
    getToolVersions.mockReturnValueOnce(refresh);
    const listener = listenAllInstall.mock.calls[0][0] as (
      event: unknown,
    ) => Promise<void>;
    const applyEvent = listener({
      type: "finished",
      task_id: "install-1",
      result: completedTask.result,
    });
    await waitFor(() => expect(getToolVersions).toHaveBeenCalledOnce());
    view.unmount();
    resolveRefresh(toolVersions);
    await applyEvent;

    expect(unlisten).toHaveBeenCalledOnce();
    expect(toastSuccess).not.toHaveBeenCalled();
    expect(toastWarning).not.toHaveBeenCalled();
    expect(toastError).not.toHaveBeenCalled();
    expect(probeToolInstallations).not.toHaveBeenCalled();
  });

  it("does not write silent diagnosis results after unmount", async () => {
    let resolveDiagnosis!: (reports: unknown[]) => void;
    const diagnosis = new Promise<unknown[]>((resolve) => {
      resolveDiagnosis = resolve;
    });
    const completedTask = {
      task_id: "install-1",
      request: { task_id: "install-1", tools: ["claude"], action: "install" },
      stage: "completed",
      plan: { actions: [] },
      result: {
        status: "succeeded_with_conflicts",
        tools: [{ tool: "claude", status: "installed_not_runnable" }],
      },
      cancellation_requested: false,
      interrupted: false,
    };
    getInstallTask.mockResolvedValue(completedTask);
    probeToolInstallations.mockReturnValue(diagnosis);
    const view = render(<AgentLifecyclePage />);
    await waitFor(() => expect(replayInstall).toHaveBeenCalledOnce());
    const listener = listenAllInstall.mock.calls[0][0] as (
      event: unknown,
    ) => Promise<void>;
    await listener({
      type: "finished",
      task_id: "install-1",
      result: completedTask.result,
    });
    await waitFor(() =>
      expect(probeToolInstallations).toHaveBeenCalledWith(["claude"]),
    );
    toastSuccess.mockClear();
    toastWarning.mockClear();
    toastError.mockClear();
    view.unmount();
    resolveDiagnosis([
      {
        tool: "claude",
        installs: [{ path: "/tmp/claude", version: "1.0.0", runnable: true }],
        is_conflict: true,
      },
    ]);
    await diagnosis;

    expect(toastSuccess).not.toHaveBeenCalled();
    expect(toastWarning).not.toHaveBeenCalled();
    expect(toastError).not.toHaveBeenCalled();
  });

  it("enters progress after privileged confirmation despite a racing stage snapshot", async () => {
    let resolveTask!: (task: unknown) => void;
    const taskSnapshot = new Promise<unknown>((resolve) => {
      resolveTask = resolve;
    });
    prepareInstall.mockResolvedValue({
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
    });
    getInstallTask.mockReturnValue(taskSnapshot);
    render(<AgentLifecyclePage />);
    const installButtons = await screen.findAllByText("settings.toolInstall");

    fireEvent.click(installButtons[0]);
    const listener = listenAllInstall.mock.calls[0][0] as (
      event: unknown,
    ) => Promise<void>;
    const applyEvent = listener({
      type: "stage_changed",
      task_id: "install-1",
      stage: "repairing",
    });
    await waitFor(() =>
      expect(getInstallTask).toHaveBeenCalledWith("install-1"),
    );
    fireEvent.click(screen.getByText("settings.installer.confirmAndContinue"));
    await waitFor(() => expect(startInstall).toHaveBeenCalledTimes(1));
    expect(screen.getByText("settings.installer.progress")).toBeInTheDocument();
    resolveTask({
      task_id: "install-1",
      request: { task_id: "install-1", tools: ["claude"], action: "install" },
      stage: "repairing",
      plan: { actions: [] },
      result: null,
      cancellation_requested: false,
      interrupted: false,
    });
    await applyEvent;

    expect(startInstall).toHaveBeenCalledTimes(1);
  });

  it("does not continue an update after unmount", async () => {
    let rejectUpdate!: (error: Error) => void;
    const update = new Promise<void>((_resolve, reject) => {
      rejectUpdate = reject;
    });
    getToolVersions.mockResolvedValue(
      makeToolVersions(new Set(["claude"]), new Set(["claude"])),
    );
    probeToolInstallations.mockResolvedValue([]);
    runToolLifecycleAction.mockReturnValue(update);
    const view = render(<AgentLifecyclePage />);
    fireEvent.click(await screen.findByText("common.refresh"));
    await screen.findByText("settings.toolUpdate");
    getToolVersions.mockClear();

    fireEvent.click(screen.getByText("settings.toolUpdate"));
    await waitFor(() => expect(runToolLifecycleAction).toHaveBeenCalledOnce());
    view.unmount();
    rejectUpdate(new Error("update failed"));
    await update.catch(() => undefined);

    expect(getToolVersions).not.toHaveBeenCalled();
    expect(toastSuccess).not.toHaveBeenCalled();
    expect(toastWarning).not.toHaveBeenCalled();
    expect(toastError).not.toHaveBeenCalled();
  });

  it("does not toast when confirmation rejects after unmount", async () => {
    let rejectStart!: (error: Error) => void;
    const start = new Promise<string>((_resolve, reject) => {
      rejectStart = reject;
    });
    prepareInstall.mockResolvedValue({
      task_id: "install-1",
      requires_confirmation: true,
      plan: { actions: [] },
    });
    startInstall.mockReturnValue(start);
    const view = render(<AgentLifecyclePage />);
    fireEvent.click((await screen.findAllByText("settings.toolInstall"))[0]);
    await screen.findByText("settings.installer.confirmAndContinue");

    fireEvent.click(screen.getByText("settings.installer.confirmAndContinue"));
    await waitFor(() => expect(startInstall).toHaveBeenCalledOnce());
    view.unmount();
    rejectStart(new Error("start failed"));
    await start.catch(() => undefined);

    expect(toastError).not.toHaveBeenCalled();
  });

  it("does not toast when cancellation rejects after unmount", async () => {
    let rejectCancel!: (error: Error) => void;
    const cancellation = new Promise<void>((_resolve, reject) => {
      rejectCancel = reject;
    });
    prepareInstall.mockResolvedValue({
      task_id: "install-1",
      requires_confirmation: true,
      plan: { actions: [] },
    });
    cancelInstall.mockReturnValue(cancellation);
    getInstallTask.mockResolvedValue({
      task_id: "install-1",
      request: { task_id: "install-1", tools: ["claude"], action: "install" },
      stage: "installing_tools",
      plan: { actions: [] },
      result: null,
      cancellation_requested: false,
      interrupted: false,
    });
    const view = render(<AgentLifecyclePage />);
    fireEvent.click((await screen.findAllByText("settings.toolInstall"))[0]);
    fireEvent.click(
      await screen.findByText("settings.installer.confirmAndContinue"),
    );
    await waitFor(() => expect(startInstall).toHaveBeenCalledOnce());
    const listener = listenAllInstall.mock.calls[0][0] as (
      event: unknown,
    ) => Promise<void>;
    await listener({
      type: "stage_changed",
      task_id: "install-1",
      stage: "installing_tools",
    });
    await screen.findByText("settings.installer.cancel");

    fireEvent.click(screen.getByText("settings.installer.cancel"));
    await waitFor(() =>
      expect(cancelInstall).toHaveBeenCalledWith("install-1"),
    );
    view.unmount();
    rejectCancel(new Error("cancel failed"));
    await cancellation.catch(() => undefined);

    expect(toastError).not.toHaveBeenCalled();
  });

  it("does not finish a bulk diagnostic after unmount", async () => {
    let resolveDiagnosis!: (reports: unknown[]) => void;
    const diagnosis = new Promise<unknown[]>((resolve) => {
      resolveDiagnosis = resolve;
    });
    probeToolInstallations.mockReturnValue(diagnosis);
    const view = render(<AgentLifecyclePage />);

    fireEvent.click(await screen.findByText("settings.toolDiagnose"));
    await waitFor(() =>
      expect(probeToolInstallations).toHaveBeenCalledWith([
        "claude",
        "codex",
        "gemini",
        "opencode",
        "openclaw",
        "hermes",
      ]),
    );
    view.unmount();
    resolveDiagnosis([]);
    await diagnosis;

    expect(toastInfo).not.toHaveBeenCalled();
    expect(toastError).not.toHaveBeenCalled();
  });

  it("does not finish a bulk refresh after unmount", async () => {
    let resolveRefresh!: (versions: typeof toolVersions) => void;
    const refresh = new Promise<typeof toolVersions>((resolve) => {
      resolveRefresh = resolve;
    });
    const view = render(<AgentLifecyclePage />);
    getToolVersions.mockClear();
    getToolVersions.mockReturnValue(refresh);

    fireEvent.click(await screen.findByText("common.refresh"));
    await waitFor(() => expect(getToolVersions).toHaveBeenCalledTimes(6));
    view.unmount();
    resolveRefresh(toolVersions);
    await refresh;

    expect(toastSuccess).not.toHaveBeenCalled();
    expect(toastWarning).not.toHaveBeenCalled();
    expect(toastError).not.toHaveBeenCalled();
  });

  it("does not continue native preparation after unmount", async () => {
    let resolvePreparation!: (preparation: unknown) => void;
    const preparation = new Promise<unknown>((resolve) => {
      resolvePreparation = resolve;
    });
    prepareInstall.mockReturnValue(preparation);
    const view = render(<AgentLifecyclePage />);
    const installButtons = await screen.findAllByText("settings.toolInstall");

    fireEvent.click(installButtons[0]);
    await waitFor(() => expect(prepareInstall).toHaveBeenCalledOnce());
    view.unmount();
    resolvePreparation({
      task_id: "install-1",
      requires_confirmation: false,
      plan: { actions: [] },
    });
    await preparation;

    expect(startInstall).not.toHaveBeenCalled();
    expect(toastError).not.toHaveBeenCalled();
  });

  it("routes native installs through the installer API and preserves diagnosis after closing the flow", async () => {
    render(<AgentLifecyclePage />);
    fireEvent.click(await screen.findByText("common.refresh"));
    await waitFor(() =>
      expect(screen.getAllByText("common.notInstalled")).toHaveLength(6),
    );
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

  it("refreshes one stale confirmation into a no-confirmation progress flow", async () => {
    prepareInstall
      .mockResolvedValueOnce({
        task_id: "stale-install",
        requires_confirmation: true,
        plan: { actions: [] },
      })
      .mockResolvedValueOnce({
        task_id: "refreshed-install",
        requires_confirmation: false,
        plan: { actions: [] },
      });
    startInstall
      .mockResolvedValueOnce({
        type: "refreshed",
        preparation: {
          task_id: "refreshed-install",
          requires_confirmation: false,
          plan: { actions: [] },
          refresh_generation: 1,
        },
      })
      .mockResolvedValueOnce({ type: "started", task_id: "refreshed-install" });
    render(<AgentLifecyclePage />);

    fireEvent.click((await screen.findAllByText("settings.toolInstall"))[0]);
    fireEvent.click(
      await screen.findByText("settings.installer.confirmAndContinue"),
    );

    await waitFor(() => expect(prepareInstall).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(startInstall).toHaveBeenCalledTimes(2));
    expect(screen.getByText("settings.installer.progress")).toBeInTheDocument();
    expect(
      screen.queryByText("installer.state_changed"),
    ).not.toBeInTheDocument();
  });

  it("starts a refreshed no-confirmation plan from the initial path", async () => {
    startInstall
      .mockResolvedValueOnce({
        type: "refreshed",
        preparation: {
          task_id: "refreshed-install",
          requires_confirmation: false,
          plan: { actions: [] },
          refresh_generation: 1,
        },
      })
      .mockResolvedValueOnce({ type: "started", task_id: "refreshed-install" });
    render(<AgentLifecyclePage />);

    fireEvent.click((await screen.findAllByText("settings.toolInstall"))[0]);

    await waitFor(() => expect(startInstall).toHaveBeenCalledTimes(2));
    const initiallyRequestedTools = startInstall.mock.calls[0][0].request.tools;
    expect(startInstall.mock.calls[1][0]).toEqual({
      request: {
        task_id: "refreshed-install",
        tools: initiallyRequestedTools,
        action: "install",
      },
      confirmed_action_ids: [],
    });
    expect(screen.getByText("settings.installer.progress")).toBeInTheDocument();
  });

  it("clears busy state when an immediate no-confirmation start rejects", async () => {
    startInstall.mockRejectedValueOnce(new Error("start failed"));
    render(<AgentLifecyclePage />);

    fireEvent.click((await screen.findAllByText("settings.toolInstall"))[0]);

    await waitFor(() => expect(toastError).toHaveBeenCalled());
    expect(screen.getByText("settings.toolDiagnose")).not.toBeDisabled();
  });

  it("clears the flow after the backend rejects a second stale confirmation", async () => {
    prepareInstall.mockResolvedValueOnce({
      task_id: "refreshed-install",
      requires_confirmation: true,
      plan: { actions: [] },
      refresh_generation: 1,
    });
    startInstall.mockResolvedValueOnce({ type: "state_changed" });
    render(<AgentLifecyclePage />);

    fireEvent.click((await screen.findAllByText("settings.toolInstall"))[0]);
    fireEvent.click(
      await screen.findByText("settings.installer.confirmAndContinue"),
    );

    await waitFor(() =>
      expect(toastError).toHaveBeenCalledWith(
        "settings.installer.stateChanged",
        expect.anything(),
      ),
    );
    expect(screen.getByText("settings.toolDiagnose")).not.toBeDisabled();
  });
});
