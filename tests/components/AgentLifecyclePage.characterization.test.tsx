import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AgentLifecyclePage } from "@/components/AgentLifecyclePage";

const {
  getToolVersions,
  runToolLifecycleAction,
  probeToolInstallations,
} = vi.hoisted(() => ({
  getToolVersions: vi.fn(),
  runToolLifecycleAction: vi.fn(),
  probeToolInstallations: vi.fn(),
}));
const toastError = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api", () => ({
  settingsApi: {
    getToolVersions,
    runToolLifecycleAction,
    probeToolInstallations,
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

  it("preserves install and diagnosis calls through the existing APIs", async () => {
    render(<AgentLifecyclePage />);
    const installButtons = await screen.findAllByText("settings.toolInstall");

    fireEvent.click(installButtons[0]);
    await waitFor(() => {
      expect(runToolLifecycleAction).toHaveBeenCalledWith(
        ["claude"],
        "install",
        {},
      );
    });

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

  it("preserves the lifecycle error toast path", async () => {
    runToolLifecycleAction.mockRejectedValueOnce(new Error("install failed"));
    render(<AgentLifecyclePage />);
    const installButtons = await screen.findAllByText("settings.toolInstall");

    fireEvent.click(installButtons[0]);
    await waitFor(() => expect(toastError).toHaveBeenCalled());
  });
});
