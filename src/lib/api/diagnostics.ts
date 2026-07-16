import { invoke } from "@tauri-apps/api/core";

export interface DiagnosticToolInput {
  name: string;
  version: string | null;
  installed_but_broken: boolean;
  env_type: string;
}

export interface DiagnosticReportRequest {
  summary: string;
  sensitive: boolean;
  tools: DiagnosticToolInput[];
}

export interface DiagnosticReportPreview {
  report_id: string;
  issue_title: string;
  issue_body: string;
  public_issue_allowed: boolean;
  public_block_reason: "sensitive" | "oversized" | null;
  public_body_limit_bytes: number;
  public_url_limit_bytes: number;
}

export const diagnosticsApi = {
  async generate(
    request: DiagnosticReportRequest,
  ): Promise<DiagnosticReportPreview> {
    return await invoke("generate_diagnostic_report", { request });
  },

  async export(reportId: string, path: string): Promise<void> {
    await invoke("export_diagnostic_report", { reportId, path });
  },

  async openIssue(reportId: string): Promise<void> {
    await invoke("open_diagnostic_issue", { reportId });
  },
};
