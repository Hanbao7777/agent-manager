import { beforeEach, describe, expect, it, vi } from "vitest";
import { diagnosticsApi } from "./diagnostics";

const invoke = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

describe("diagnosticsApi", () => {
  beforeEach(() => vi.clearAllMocks());

  it("uses only local generate and export commands", async () => {
    const request = { summary: "safe", sensitive: false, tools: [] };
    invoke.mockResolvedValueOnce({ report_id: "diagnostic-1" });

    await diagnosticsApi.generate(request);
    await diagnosticsApi.export("diagnostic-1", "C:\\report.json");

    expect(invoke.mock.calls).toEqual([
      ["generate_diagnostic_report", { request }],
      ["export_diagnostic_report", { reportId: "diagnostic-1", path: "C:\\report.json" }],
    ]);
  });
});
