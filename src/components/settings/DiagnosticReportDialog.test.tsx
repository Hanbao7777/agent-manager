import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DiagnosticReportDialog } from "./DiagnosticReportDialog";

const { generate, exportReport, openIssue, toastSuccess, toastError } =
  vi.hoisted(() => ({
    generate: vi.fn(),
    exportReport: vi.fn(),
    openIssue: vi.fn(),
    toastSuccess: vi.fn(),
    toastError: vi.fn(),
  }));

vi.mock("@/lib/api", () => ({
  diagnosticsApi: { generate, export: exportReport, openIssue },
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
  public_issue_allowed: true,
  public_block_reason: null,
  public_body_limit_bytes: 6000,
  public_url_limit_bytes: 8000,
};

describe("DiagnosticReportDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    generate.mockResolvedValue(publicPreview);
    exportReport.mockResolvedValue(undefined);
    openIssue.mockResolvedValue(undefined);
  });

  it("requires preview review before opening by report ID only", async () => {
    render(<DiagnosticReportDialog tools={tools} />);

    fireEvent.click(screen.getByText("settings.diagnostics.create"));
    expect(
      screen.queryByText("settings.diagnostics.send"),
    ).not.toBeInTheDocument();
    expect(openIssue).not.toHaveBeenCalled();

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
    expect(openIssue).not.toHaveBeenCalled();
    expect(
      screen.getByText("settings.diagnostics.reviewNotice"),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByText("settings.diagnostics.send"));
    await waitFor(() => expect(openIssue).toHaveBeenCalledWith("diagnostic-1"));
  });

  it("keeps sensitive and oversized previews export-only", async () => {
    generate.mockResolvedValue({
      ...publicPreview,
      public_issue_allowed: false,
      public_block_reason: "oversized",
    });
    render(<DiagnosticReportDialog tools={tools} />);

    fireEvent.click(screen.getByText("settings.diagnostics.create"));
    fireEvent.click(screen.getByText("settings.diagnostics.preview"));
    await screen.findByText("settings.diagnostics.blocked.oversized");

    expect(
      screen.queryByText("settings.diagnostics.send"),
    ).not.toBeInTheDocument();
    expect(openIssue).not.toHaveBeenCalled();
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
    expect(toastSuccess).toHaveBeenCalledWith("settings.diagnostics.exported");
  });

  it("shows localized feedback when public issue opening fails", async () => {
    openIssue.mockRejectedValue("settings.diagnostics.error.issueOpenFailed");
    render(<DiagnosticReportDialog tools={tools} />);

    fireEvent.click(screen.getByText("settings.diagnostics.create"));
    fireEvent.click(screen.getByText("settings.diagnostics.preview"));
    await screen.findByText("exact reviewed body");
    fireEvent.click(screen.getByText("settings.diagnostics.send"));

    await waitFor(() =>
      expect(toastError).toHaveBeenCalledWith(
        "settings.diagnostics.error.issueOpenFailed",
      ),
    );
  });
});
