use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::installer::{
    redact_diagnostic, unique_temporary_path, InstallStage, InstallTaskSnapshot, InstallTaskStore,
    ToolId,
};

const ISSUE_BASE_URL: &str = "https://github.com/Hanbao7777/agent-manager/issues/new";
const ISSUE_TITLE: &str = "[Diagnostics] Agent Manager report";
// Keep the body below 6 KiB and the complete encoded URL below 8 KiB so
// public handoff remains conservative across desktop browser URL limits.
pub const MAX_PUBLIC_ISSUE_BODY_BYTES: usize = 6_000;
pub const MAX_PUBLIC_ISSUE_URL_BYTES: usize = 8_000;
const MAX_LOCAL_SUMMARY_BYTES: usize = 64 * 1024;
const MAX_STORED_REPORTS: usize = 8;

#[derive(Debug, Clone, Deserialize)]
pub struct DiagnosticReportRequest {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    sensitive: bool,
    #[serde(default)]
    tools: Vec<DiagnosticToolInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiagnosticToolInput {
    name: String,
    version: Option<String>,
    installed_but_broken: bool,
    env_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticReportPreview {
    report_id: String,
    issue_title: String,
    issue_body: String,
    issue_url: Option<String>,
    public_block_reason: Option<PublicBlockReason>,
    public_body_limit_bytes: usize,
    public_url_limit_bytes: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PublicBlockReason {
    Sensitive,
    Oversized,
}

#[derive(Debug, Clone, Serialize)]
struct DiagnosticReport {
    schema_version: u8,
    generated_at_utc: String,
    application: ApplicationReport,
    platform: PlatformReport,
    tools: Vec<ToolReport>,
    installation: Option<InstallationReport>,
    user_summary: Option<String>,
    marked_sensitive: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ApplicationReport {
    name: &'static str,
    version: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct PlatformReport {
    os: &'static str,
    architecture: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct ToolReport {
    name: String,
    status: ToolStatus,
    version: Option<String>,
    environment: ToolEnvironment,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum ToolStatus {
    Installed,
    InstalledNotRunnable,
    NotInstalled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum ToolEnvironment {
    Windows,
    Wsl,
    Macos,
    Linux,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
struct InstallationReport {
    stage: InstallStage,
    tools: Vec<InstallationToolReport>,
    result_status: Option<String>,
    failure_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct InstallationToolReport {
    name: ToolId,
    status: String,
    failure_code: Option<String>,
}

struct StoredReport {
    document: String,
}

#[derive(Default)]
pub struct DiagnosticReportStore {
    reports: Mutex<HashMap<String, StoredReport>>,
    next_report_id: AtomicU64,
    next_export_id: AtomicU64,
}

impl DiagnosticReportStore {
    fn insert(&self, document: String) -> Result<String, String> {
        let report_id = format!(
            "diagnostic-{}",
            self.next_report_id.fetch_add(1, Ordering::Relaxed) + 1
        );
        let mut reports = self
            .reports
            .lock()
            .map_err(|_| "diagnostics.error.unavailable".to_string())?;
        if reports.len() >= MAX_STORED_REPORTS {
            if let Some(oldest) = reports
                .keys()
                .min_by_key(|id| {
                    id.strip_prefix("diagnostic-")
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or_default()
                })
                .cloned()
            {
                reports.remove(&oldest);
            }
        }
        reports.insert(report_id.clone(), StoredReport { document });
        Ok(report_id)
    }

    fn document(&self, report_id: &str) -> Result<String, String> {
        self.reports
            .lock()
            .map_err(|_| "diagnostics.error.unavailable".to_string())?
            .get(report_id)
            .map(|report| report.document.clone())
            .ok_or_else(|| "diagnostics.error.report_expired".to_string())
    }
}

#[tauri::command]
pub fn generate_diagnostic_report(
    request: DiagnosticReportRequest,
    install_tasks: tauri::State<'_, InstallTaskStore>,
    reports: tauri::State<'_, DiagnosticReportStore>,
) -> Result<DiagnosticReportPreview, String> {
    let latest_install = install_tasks.latest_task();
    generate_preview(
        request,
        latest_install.as_ref(),
        &reports,
        &Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    )
}

#[tauri::command]
pub fn export_diagnostic_report(
    report_id: String,
    path: String,
    reports: tauri::State<'_, DiagnosticReportStore>,
) -> Result<(), String> {
    let document = reports.document(&report_id)?;
    export_document(
        &document,
        Path::new(&path),
        reports.next_export_id.fetch_add(1, Ordering::Relaxed),
    )
}

fn generate_preview(
    request: DiagnosticReportRequest,
    latest_install: Option<&InstallTaskSnapshot>,
    reports: &DiagnosticReportStore,
    generated_at_utc: &str,
) -> Result<DiagnosticReportPreview, String> {
    if request.summary.len() > MAX_LOCAL_SUMMARY_BYTES {
        return Err("diagnostics.error.summary_too_large".to_string());
    }

    let report = DiagnosticReport {
        schema_version: 1,
        generated_at_utc: generated_at_utc.to_string(),
        application: ApplicationReport {
            name: "Agent Manager",
            version: env!("CARGO_PKG_VERSION"),
        },
        platform: PlatformReport {
            os: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
        },
        tools: sanitize_tools(request.tools),
        installation: latest_install.map(sanitize_installation),
        user_summary: nonempty_redacted(&request.summary),
        marked_sensitive: request.sensitive,
    };
    let document = serde_json::to_string_pretty(&report)
        .map_err(|_| "diagnostics.error.unavailable".to_string())?;
    let issue_body = format!(
        "## Diagnostic report\n\n```json\n{document}\n```\n\n> Generated locally by Agent Manager. Review this public issue before submitting."
    );
    let issue_url = build_issue_url(ISSUE_TITLE, &issue_body)?;
    let public_block_reason = if request.sensitive {
        Some(PublicBlockReason::Sensitive)
    } else if issue_body.len() > MAX_PUBLIC_ISSUE_BODY_BYTES
        || issue_url.len() > MAX_PUBLIC_ISSUE_URL_BYTES
    {
        Some(PublicBlockReason::Oversized)
    } else {
        None
    };
    let report_id = reports.insert(document)?;

    Ok(DiagnosticReportPreview {
        report_id,
        issue_title: ISSUE_TITLE.to_string(),
        issue_body,
        issue_url: public_block_reason.is_none().then_some(issue_url),
        public_block_reason,
        public_body_limit_bytes: MAX_PUBLIC_ISSUE_BODY_BYTES,
        public_url_limit_bytes: MAX_PUBLIC_ISSUE_URL_BYTES,
    })
}

fn sanitize_tools(inputs: Vec<DiagnosticToolInput>) -> Vec<ToolReport> {
    const TOOLS: [&str; 6] = [
        "claude", "codex", "gemini", "opencode", "openclaw", "hermes",
    ];
    TOOLS
        .into_iter()
        .filter_map(|name| inputs.iter().find(|input| input.name == name))
        .map(|input| ToolReport {
            name: input.name.clone(),
            status: if input.installed_but_broken {
                ToolStatus::InstalledNotRunnable
            } else if input.version.is_some() {
                ToolStatus::Installed
            } else {
                ToolStatus::NotInstalled
            },
            version: input.version.as_deref().and_then(sanitize_short_value),
            environment: match input.env_type.as_str() {
                "windows" => ToolEnvironment::Windows,
                "wsl" => ToolEnvironment::Wsl,
                "macos" => ToolEnvironment::Macos,
                "linux" => ToolEnvironment::Linux,
                _ => ToolEnvironment::Unknown,
            },
        })
        .collect()
}

fn sanitize_installation(task: &InstallTaskSnapshot) -> InstallationReport {
    let result = task.result.as_ref();
    InstallationReport {
        stage: task.stage.clone(),
        tools: result
            .map(|result| {
                result
                    .tools
                    .iter()
                    .map(|tool| InstallationToolReport {
                        name: tool.tool,
                        status: wire_value(&tool.status),
                        failure_code: tool
                            .failure
                            .as_ref()
                            .map(|failure| wire_value(&failure.code)),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        result_status: result.map(|result| wire_value(&result.status)),
        failure_code: result
            .and_then(|result| result.failure.as_ref())
            .map(|failure| wire_value(&failure.code)),
    }
}

fn wire_value<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

fn nonempty_redacted(value: &str) -> Option<String> {
    let value = redact_diagnostic(value.trim());
    (!value.is_empty()).then_some(value)
}

fn sanitize_short_value(value: &str) -> Option<String> {
    let value = redact_diagnostic(value.trim());
    (!value.is_empty()).then(|| value.chars().take(128).collect())
}

fn build_issue_url(title: &str, body: &str) -> Result<String, String> {
    let mut url =
        url::Url::parse(ISSUE_BASE_URL).map_err(|_| "diagnostics.error.unavailable".to_string())?;
    url.query_pairs_mut()
        .append_pair("title", title)
        .append_pair("body", body);
    Ok(url.into())
}

fn export_document(document: &str, destination: &Path, sequence: u64) -> Result<(), String> {
    validate_export_path(destination)?;
    let temporary = unique_temporary_path(destination, sequence);
    write_and_replace(
        document,
        destination,
        &temporary,
        |temporary, destination| crate::installer::replace_file(temporary, destination),
    )
}

fn write_and_replace<F>(
    document: &str,
    destination: &Path,
    temporary: &Path,
    replace: F,
) -> Result<(), String>
where
    F: FnOnce(&Path, &Path) -> std::io::Result<()>,
{
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(temporary)
        .map_err(|_| "diagnostics.error.export_failed".to_string())?;
    let write_result = file
        .write_all(document.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "diagnostics.error.export_failed".to_string());
    drop(file);
    if write_result.is_err() {
        let _ = std::fs::remove_file(temporary);
        return write_result;
    }

    let result =
        replace(temporary, destination).map_err(|_| "diagnostics.error.export_failed".to_string());
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

fn validate_export_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || path.file_name().is_none() {
        return Err("diagnostics.error.invalid_export_path".to_string());
    }
    if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
        return Err("diagnostics.error.invalid_export_path".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "diagnostics.error.invalid_export_path".to_string())?;
    let metadata = std::fs::symlink_metadata(parent)
        .map_err(|_| "diagnostics.error.invalid_export_path".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("diagnostics.error.invalid_export_path".to_string());
    }
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("diagnostics.error.invalid_export_path".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installer::{
        InstallAction, InstallFailure, InstallFailureCode, InstallRequest, InstallTaskResult,
        InstallTaskStatus, RecommendedAction, RepairPlan,
    };

    fn request(summary: &str, sensitive: bool) -> DiagnosticReportRequest {
        DiagnosticReportRequest {
            summary: summary.to_string(),
            sensitive,
            tools: vec![
                DiagnosticToolInput {
                    name: "claude".into(),
                    version: Some("1.2.3".into()),
                    installed_but_broken: false,
                    env_type: "windows".into(),
                },
                DiagnosticToolInput {
                    name: "not-allowlisted".into(),
                    version: Some("secret".into()),
                    installed_but_broken: false,
                    env_type: "unknown".into(),
                },
            ],
        }
    }

    #[test]
    fn preview_is_allowlisted_redacted_and_url_encoded() {
        let store = DiagnosticReportStore::default();
        let preview = generate_preview(
            request(
                "home C:\\Users\\Alice\\work TOKEN=secret https://user:pass@example.com?a=1&token=hidden",
                false,
            ),
            None,
            &store,
            "2026-07-16T12:00:00Z",
        )
        .unwrap();

        assert!(preview
            .issue_body
            .contains(r#"home ~\\work TOKEN=[REDACTED]"#));
        assert!(!preview.issue_body.contains("Alice"));
        assert!(!preview.issue_body.contains("user:pass"));
        assert!(!preview.issue_body.contains("hidden"));
        assert!(!preview.issue_body.contains("not-allowlisted"));
        let url = url::Url::parse(preview.issue_url.as_deref().unwrap()).unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("github.com"));
        assert_eq!(url.path(), "/Hanbao7777/agent-manager/issues/new");
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query.get("title"), Some(&preview.issue_title));
        assert_eq!(query.get("body"), Some(&preview.issue_body));
        assert_eq!(query.len(), 2);
    }

    #[test]
    fn sensitive_and_oversized_reports_are_export_only() {
        let sensitive = generate_preview(
            request("reviewed", true),
            None,
            &DiagnosticReportStore::default(),
            "2026-07-16T12:00:00Z",
        )
        .unwrap();
        assert_eq!(
            sensitive.public_block_reason,
            Some(PublicBlockReason::Sensitive)
        );
        assert!(sensitive.issue_url.is_none());

        let oversized = generate_preview(
            request(&"x".repeat(MAX_PUBLIC_ISSUE_BODY_BYTES), false),
            None,
            &DiagnosticReportStore::default(),
            "2026-07-16T12:00:00Z",
        )
        .unwrap();
        assert_eq!(
            oversized.public_block_reason,
            Some(PublicBlockReason::Oversized)
        );
        assert!(oversized.issue_url.is_none());
    }

    #[test]
    fn installation_report_excludes_paths_and_raw_internal_errors() {
        let task = InstallTaskSnapshot {
            task_id: "install-9".into(),
            request: InstallRequest {
                task_id: Some("install-9".into()),
                tools: vec![ToolId::Claude],
                action: InstallAction::Install,
            },
            stage: InstallStage::Completed,
            plan: RepairPlan::default(),
            result: Some(InstallTaskResult {
                status: InstallTaskStatus::Failed,
                tools: Vec::new(),
                failure: Some(InstallFailure {
                    code: InstallFailureCode::PermissionDenied,
                    stage: InstallStage::InstallingTools,
                    exit_code: Some(1),
                    retryable: false,
                    requires_user_action: true,
                    message_key: "internal.message".into(),
                    recommended_action: RecommendedAction::GrantPermission,
                    detail: Some("secret raw error at C:\\Users\\Alice".into()),
                }),
            }),
            cancellation_requested: false,
            interrupted: false,
        };
        let preview = generate_preview(
            request("", false),
            Some(&task),
            &DiagnosticReportStore::default(),
            "2026-07-16T12:00:00Z",
        )
        .unwrap();

        assert!(preview.issue_body.contains("permission_denied"));
        assert!(!preview.issue_body.contains("secret raw error"));
        assert!(!preview.issue_body.contains("internal.message"));
        assert!(!preview.issue_body.contains("install-9"));
        assert!(!preview.issue_body.contains("exit_code"));
    }

    #[test]
    fn export_writes_the_exact_stored_report_and_cleans_failed_temporary_files() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("diagnostic.json");
        export_document("{\"safe\":true}", &destination, 7).unwrap();
        assert_eq!(
            std::fs::read_to_string(&destination).unwrap(),
            "{\"safe\":true}"
        );

        let blocked = directory.path().join("blocked.json");
        let temporary = directory.path().join(".blocked.json.test.tmp");
        assert_eq!(
            write_and_replace("data", &blocked, &temporary, |_, _| {
                Err(std::io::Error::other("injected replacement failure"))
            }),
            Err("diagnostics.error.export_failed".to_string())
        );
        assert!(!temporary.exists());
    }
}
