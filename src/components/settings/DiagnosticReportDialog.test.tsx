import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DiagnosticReportDialog } from "./DiagnosticReportDialog";

const { generate, exportReport, toastSuccess, toastError } = vi.hoisted(() => ({
  generate: vi.fn(),
  exportReport: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  diagnosticsApi: { generate, export: exportReport },
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("sonner", () => ({
  toast: { success: toastSuccess, error: toastError },
}));

const tools = [
  {
    name: "claude",
    version: "1.2.3",
    installed_but_broken: false,
    env_type: "windows",
  },
];

const publicPreview = {
  report_id: "diagnostic-1",
  issue_title: "[Diagnostics] Agent Manager report",
  issue_body: "exact reviewed body",
  issue_url:
    "https://github.com/Hanbao7777/agent-manager/issues/new?title=reviewed&body=exact",
  public_block_reason: null,
  public_body_limit_bytes: 6000,
  public_url_limit_bytes: 8000,
};

describe("DiagnosticReportDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    generate.mockResolvedValue(publicPreview);
    exportReport.mockResolvedValue(undefined);
  });

  it("requires preview review before opening the exact issue URL", async () => {
    const open = vi.spyOn(window, "open").mockImplementation(() => null);
    render(<DiagnosticReportDialog tools={tools} />);

    fireEvent.click(screen.getByText("settings.diagnostics.create"));
    expect(screen.queryByText("settings.diagnostics.send")).not.toBeInTheDocument();
    expect(open).not.toHaveBeenCalled();

    fireEvent.change(
      screen.getByPlaceholderText("settings.diagnostics.summaryPlaceholder"),
      { target: { value: "TOKEN=secret" } },
    );
    fireEvent.click(screen.getByText("settings.diagnostics.preview"));

    await screen.findByText("exact reviewed body");
    expect(generate).toHaveBeenCalledWith({
      summary: "TOKEN=secret",
      sensitive: false,
      tools,
    });
    expect(open).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText("settings.diagnostics.send"));
    expect(open).toHaveBeenCalledWith(
      publicPreview.issue_url,
      "_blank",
      "noopener,noreferrer",
    );
  });

  it("keeps sensitive and oversized previews export-only", async () => {
    generate.mockResolvedValue({
      ...publicPreview,
      issue_url: null,
      public_block_reason: "oversized",
    });
    render(<DiagnosticReportDialog tools={tools} />);

    fireEvent.click(screen.getByText("settings.diagnostics.create"));
    fireEvent.click(screen.getByText("settings.diagnostics.preview"));
    await screen.findByText("settings.diagnostics.blocked.oversized");

    expect(screen.queryByText("settings.diagnostics.send")).not.toBeInTheDocument();
    fireEvent.change(
      screen.getByPlaceholderText("settings.diagnostics.exportPlaceholder"),
      { target: { value: "C:\\Users\\Alice\\report.json" } },
    );
    fireEvent.click(screen.getByText("settings.diagnostics.export"));

    await waitFor(() =>
      expect(exportReport).toHaveBeenCalledWith(
        "diagnostic-1",
        "C:\\Users\\Alice\\report.json",
      ),
    );
    expect(toastSuccess).toHaveBeenCalledWith(
      "settings.diagnostics.exported",
    );
  });
});
