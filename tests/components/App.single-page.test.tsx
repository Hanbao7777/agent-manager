import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "@/App";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    isDecorated: vi.fn().mockResolvedValue(true),
    isMaximized: vi.fn().mockResolvedValue(false),
    onResized: vi.fn().mockResolvedValue(() => undefined),
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
  }),
}));

vi.mock("@/components/AgentLifecyclePage", () => ({
  AgentLifecyclePage: () => (
    <section data-testid="agent-lifecycle-page">
      {[
        "Claude Code",
        "Codex",
        "Gemini CLI",
        "OpenCode",
        "OpenClaw",
        "Hermes",
      ].map((name) => (
        <span key={name}>{name}</span>
      ))}
    </section>
  ),
}));

describe("App single-page shell", () => {
  it("renders only the six-agent lifecycle product surface", () => {
    render(<App />);

    expect(screen.getByTestId("agent-lifecycle-page")).toBeInTheDocument();
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
    expect(screen.getByText("Codex")).toBeInTheDocument();
    expect(screen.getByText("Gemini CLI")).toBeInTheDocument();
    expect(screen.getByText("OpenCode")).toBeInTheDocument();
    expect(screen.getByText("OpenClaw")).toBeInTheDocument();
    expect(screen.getByText("Hermes")).toBeInTheDocument();

    expect(screen.queryByText("CC Switch")).not.toBeInTheDocument();
    expect(screen.queryByTitle("common.settings")).not.toBeInTheDocument();
    expect(screen.queryByTitle("usage.title")).not.toBeInTheDocument();
    expect(screen.queryByTitle("mcp.title")).not.toBeInTheDocument();
    expect(screen.queryByTitle("skills.manage")).not.toBeInTheDocument();
    expect(screen.queryByTitle("sessionManager.title")).not.toBeInTheDocument();
    expect(screen.queryByTitle("workspace.manage")).not.toBeInTheDocument();
  });
});
