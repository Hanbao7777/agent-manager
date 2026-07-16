use once_cell::sync::Lazy;
use regex::Regex;

use super::model::{InstallFailure, InstallFailureCode, InstallStage, RecommendedAction};

static SECRET_ASSIGNMENT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?i)\b([A-Z0-9_]*(?:API_KEY|ACCESS_KEY|SECRET|TOKEN|PASSWORD|PASS|CREDENTIALS?|AUTH|PRIVATE_KEY)[A-Z0-9_]*)\s*=\s*(?:'([^']*)'|"([^"]*)"|`([^`]*)`|([^\s'"`]+))"#,
    )
    .expect("valid secret assignment regex")
});

static BEARER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]+").expect("valid bearer regex"));

static NPM_TOKEN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bnpm_[A-Za-z0-9]{4,}\b").expect("valid npm token regex"));

static URL_USERINFO_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b([a-z][a-z0-9+.-]*://)([^/@\s]+)@([^/\s?#]+)")
        .expect("valid url userinfo regex")
});

static URL_SECRET_QUERY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)([?&](?:token|access_token|api_key|key|signature|sig|password|pass|secret|client_secret|credential|auth|authorization|code|session)=)[^&#\s]+")
        .expect("valid url secret query regex")
});

static ENV_ASSIGNMENT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?im)(\b[A-Z_][A-Z0-9_]*\s*=\s*)(?:'[^'\r\n]*'|"[^"\r\n]*"|`[^`\r\n]*`|[^\s\r\n]+)"#,
    )
    .expect("valid environment assignment regex")
});

static JSON_SECRET_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?i)((?:"|')?[A-Z0-9_]*(?:API_KEY|ACCESS_KEY|SECRET|TOKEN|PASSWORD|PASS|CREDENTIALS?|AUTH|PRIVATE_KEY)[A-Z0-9_]*(?:"|')?\s*:\s*)(?:"[^"]*"|'[^']*'|[^\s,}\]]+)"#,
    )
    .expect("valid JSON secret regex")
});

static AUTH_HEADER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?im)\b(Authorization|Proxy-Authorization)\s*:\s*[^\r\n]+")
        .expect("valid authorization header regex")
});

static COMMON_TOKEN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(?:github_pat_[A-Za-z0-9_]{8,}|gh[pousr]_[A-Za-z0-9]{8,}|sk-[A-Za-z0-9_-]{8,}|xox[baprs]-[A-Za-z0-9-]{8,}|AIza[A-Za-z0-9_-]{8,})\b",
    )
    .expect("valid common token regex")
});

static WINDOWS_HOME_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\b[A-Z]:[\\/](?:Users|Documents and Settings)[\\/][^\\/\s"'<>|]+"#)
        .expect("valid Windows home regex")
});

static POSIX_HOME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"/(?:Users|home)/[^/\s"'<>]+"#).expect("valid POSIX home regex"));

pub fn redact_diagnostic(text: &str) -> String {
    let value = normalize_home_path(text);
    let value = redact_home_username(&value);
    let value = ENV_ASSIGNMENT_RE.replace_all(&value, "$1[REDACTED]");
    let value = SECRET_ASSIGNMENT_RE.replace_all(&value, "$1=[REDACTED]");
    let value = JSON_SECRET_RE.replace_all(&value, "$1\"[REDACTED]\"");
    let value = BEARER_RE.replace_all(&value, "Bearer [REDACTED]");
    let value = NPM_TOKEN_RE.replace_all(&value, "npm_[REDACTED]");
    let value = COMMON_TOKEN_RE.replace_all(&value, "[REDACTED]");
    let value = AUTH_HEADER_RE.replace_all(&value, "$1: [REDACTED]");
    let value = URL_USERINFO_RE.replace_all(&value, "$1[REDACTED]@$3");
    URL_SECRET_QUERY_RE
        .replace_all(&value, "$1[REDACTED]")
        .into_owned()
}

fn normalize_home_path(text: &str) -> String {
    let value = dirs::home_dir()
        .and_then(|home| home.to_str().map(str::to_owned))
        .map(|home| replace_literal_case_insensitive(text, &home, "~"))
        .unwrap_or_else(|| text.to_string());
    let value = WINDOWS_HOME_RE.replace_all(&value, "~");
    POSIX_HOME_RE.replace_all(&value, "~").into_owned()
}

fn redact_home_username(text: &str) -> String {
    let Some(username) = dirs::home_dir()
        .and_then(|home| {
            home.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|username| username.len() >= 3)
    else {
        return text.to_string();
    };

    Regex::new(&format!(
        "(?i)(^|[^A-Za-z0-9_]){}([^A-Za-z0-9_]|$)",
        regex::escape(&username)
    ))
    .expect("escaped username is a valid regex")
    .replace_all(text, "${1}[USER]${2}")
    .into_owned()
}

fn replace_literal_case_insensitive(text: &str, literal: &str, replacement: &str) -> String {
    Regex::new(&format!("(?i){}", regex::escape(literal)))
        .expect("escaped literal is a valid regex")
        .replace_all(text, replacement)
        .into_owned()
}

pub fn classify_process_failure(
    stage: InstallStage,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> InstallFailure {
    let redacted_stdout = redact_diagnostic(stdout);
    let redacted_stderr = redact_diagnostic(stderr);
    let full_redacted = join_redacted_logs(&redacted_stdout, &redacted_stderr);
    let lower = full_redacted.to_ascii_lowercase();

    let (code, recommended_action, retryable, requires_user_action, message_key) =
        if is_dependency_missing(stdout, stderr, exit_code, &lower) {
            (
                InstallFailureCode::DependencyMissing,
                RecommendedAction::RepairDependencies,
                true,
                false,
                "installer.failure.dependency_missing",
            )
        } else if is_privilege_declined(exit_code, &lower) {
            (
                InstallFailureCode::PrivilegeDeclined,
                RecommendedAction::GrantPermission,
                false,
                true,
                "installer.failure.privilege_declined",
            )
        } else if is_permission_denied(&lower) {
            (
                InstallFailureCode::PermissionDenied,
                RecommendedAction::GrantPermission,
                false,
                true,
                "installer.failure.permission_denied",
            )
        } else if is_file_in_use(&lower) {
            (
                InstallFailureCode::FileInUse,
                RecommendedAction::CloseBlockingProcess,
                true,
                true,
                "installer.failure.file_in_use",
            )
        } else if is_multiple_installations(&lower) {
            (
                InstallFailureCode::MultipleInstallations,
                RecommendedAction::ResolveMultipleInstallations,
                false,
                true,
                "installer.failure.multiple_installations",
            )
        } else if is_dependency_too_old(&lower) {
            (
                InstallFailureCode::DependencyTooOld,
                RecommendedAction::Reinstall,
                false,
                true,
                "installer.failure.dependency_too_old",
            )
        } else if is_proxy_unreachable(&lower) {
            (
                InstallFailureCode::ProxyUnreachable,
                RecommendedAction::CheckProxy,
                true,
                false,
                "installer.failure.proxy_unreachable",
            )
        } else if is_dns_failure(&lower) {
            (
                InstallFailureCode::DnsFailure,
                RecommendedAction::CheckNetwork,
                true,
                false,
                "installer.failure.dns_failure",
            )
        } else if is_network_timeout(&lower) {
            (
                InstallFailureCode::NetworkTimeout,
                RecommendedAction::Retry,
                true,
                false,
                "installer.failure.network_timeout",
            )
        } else if is_signature_verification_failure(&lower) {
            (
                InstallFailureCode::SignatureVerificationFailure,
                RecommendedAction::ViewDiagnostics,
                false,
                true,
                "installer.failure.signature_verification_failure",
            )
        } else if is_download_integrity_failure(&lower) {
            (
                InstallFailureCode::DownloadIntegrityFailure,
                RecommendedAction::Retry,
                false,
                false,
                "installer.failure.download_integrity_failure",
            )
        } else if is_tls_failure(&lower) {
            (
                InstallFailureCode::TlsFailure,
                RecommendedAction::CheckNetwork,
                false,
                true,
                "installer.failure.tls_failure",
            )
        } else {
            fallback_classification(stage.clone())
        };

    let detail = truncate_tail(&full_redacted, 12);

    InstallFailure {
        code,
        stage,
        exit_code,
        retryable,
        requires_user_action,
        message_key: message_key.to_string(),
        recommended_action,
        detail: if detail.is_empty() {
            None
        } else {
            Some(detail)
        },
    }
}

fn fallback_classification(
    stage: InstallStage,
) -> (
    InstallFailureCode,
    RecommendedAction,
    bool,
    bool,
    &'static str,
) {
    match stage {
        InstallStage::Preflight => (
            InstallFailureCode::InstallerFailure,
            RecommendedAction::ViewDiagnostics,
            false,
            false,
            "installer.failure.preflight",
        ),
        InstallStage::AwaitingConfirmation => (
            InstallFailureCode::InstallerFailure,
            RecommendedAction::ViewDiagnostics,
            false,
            false,
            "installer.failure.awaiting_confirmation",
        ),
        InstallStage::Repairing => (
            InstallFailureCode::InstallerFailure,
            RecommendedAction::Retry,
            true,
            false,
            "installer.failure.repairing",
        ),
        InstallStage::InstallingTools => (
            InstallFailureCode::ToolInstallFailure,
            RecommendedAction::Retry,
            true,
            false,
            "installer.failure.installing_tools",
        ),
        InstallStage::Verifying => (
            InstallFailureCode::VerificationFailure,
            RecommendedAction::Retry,
            true,
            false,
            "installer.failure.verifying",
        ),
        InstallStage::Completed => (
            InstallFailureCode::VerificationFailure,
            RecommendedAction::ViewDiagnostics,
            false,
            false,
            "installer.failure.completed",
        ),
    }
}

fn join_redacted_logs(stdout: &str, stderr: &str) -> String {
    let mut parts = Vec::new();
    let stdout = stdout.trim();
    if !stdout.is_empty() {
        parts.push(stdout);
    }
    let stderr = stderr.trim();
    if !stderr.is_empty() {
        parts.push(stderr);
    }

    parts.join("\n")
}

fn truncate_tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

fn is_dependency_missing(stdout: &str, stderr: &str, _exit_code: Option<i32>, lower: &str) -> bool {
    command_missing_evidence(stdout)
        || command_missing_evidence(stderr)
        || lower.contains("enoent")
        || lower.contains("no such file or directory")
        || lower.contains("the system cannot find the file specified")
        || lower.contains("cannot find the file specified")
        || lower.contains("is not recognized as an internal or external command")
}

fn command_missing_evidence(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("command not found")
        || lower.contains("is not recognized as an internal or external command")
        || lower.contains("cannot find the file specified")
        || lower.contains("the system cannot find the file specified")
        || lower.contains("no such file or directory")
        || lower.contains("enoent")
}

fn is_privilege_declined(exit_code: Option<i32>, lower: &str) -> bool {
    matches!(exit_code, Some(1223) | Some(1602))
        || lower.contains("canceled by the user")
        || lower.contains("cancelled by the user")
        || lower.contains("user cancelled")
        || lower.contains("user canceled")
        || lower.contains("the operation was canceled")
}

fn is_permission_denied(lower: &str) -> bool {
    lower.contains("permission denied")
        || lower.contains("access is denied")
        || lower.contains("eacces")
        || lower.contains("operation not permitted")
}

fn is_file_in_use(lower: &str) -> bool {
    lower.contains("file in use")
        || lower.contains("being used by another process")
        || lower.contains("resource is in use")
        || lower.contains("text file busy")
}

fn is_multiple_installations(lower: &str) -> bool {
    lower.contains("multiple installation")
        || lower.contains("multiple installations")
        || lower.contains("more than one installation")
        || lower.contains("ambiguous command")
}

fn is_proxy_unreachable(lower: &str) -> bool {
    lower.contains("proxy")
        && (lower.contains("connect")
            || lower.contains("unreachable")
            || lower.contains("econnrefused")
            || lower.contains("connection refused"))
}

fn is_network_timeout(lower: &str) -> bool {
    lower.contains("timed out")
        || lower.contains("etimedout")
        || lower.contains("socket hang up")
        || lower.contains("connection timed out")
}

fn is_dns_failure(lower: &str) -> bool {
    lower.contains("enotfound")
        || lower.contains("name or service not known")
        || lower.contains("could not resolve host")
        || lower.contains("temporary failure in name resolution")
}

fn is_tls_failure(lower: &str) -> bool {
    lower.contains("certificate")
        || lower.contains("self signed certificate")
        || lower.contains("unable to verify the first certificate")
        || lower.contains("tls")
        || lower.contains("certificate verify failed")
}

fn is_download_integrity_failure(lower: &str) -> bool {
    lower.contains("sha256")
        || lower.contains("checksum")
        || lower.contains("integrity")
        || lower.contains("hash mismatch")
}

fn is_signature_verification_failure(lower: &str) -> bool {
    lower.contains("authenticode")
        || lower.contains("developer id")
        || lower.contains("signature verification")
        || lower.contains("not signed")
        || lower.contains("signature check failed")
}

fn is_dependency_too_old(lower: &str) -> bool {
    lower.contains("requires node")
        || lower.contains("unsupported node")
        || lower.contains("node version")
        || lower.contains("engine ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long_tail(secret_tail: &str) -> String {
        let mut text = String::new();
        for i in 0..40 {
            text.push_str(&format!("line {i}\n"));
        }
        text.push_str(secret_tail);
        text
    }

    #[test]
    fn classifies_missing_npm_without_losing_details() {
        let failure = classify_process_failure(
            InstallStage::InstallingTools,
            Some(127),
            "",
            "npm: command not found",
        );

        assert_eq!(failure.code, InstallFailureCode::DependencyMissing);
        assert_eq!(
            failure.recommended_action,
            RecommendedAction::RepairDependencies
        );
        assert_eq!(failure.stage, InstallStage::InstallingTools);
        assert_eq!(failure.exit_code, Some(127));
        assert!(failure.retryable);
        assert!(!failure.requires_user_action);
        assert!(failure
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("npm: command not found"));
    }

    #[test]
    fn exit_127_without_command_missing_evidence_falls_back() {
        let failure = classify_process_failure(
            InstallStage::InstallingTools,
            Some(127),
            "unrelated stdout",
            "still unrelated stderr",
        );

        assert_eq!(failure.code, InstallFailureCode::ToolInstallFailure);
        assert_eq!(failure.stage, InstallStage::InstallingTools);
        assert!(failure.retryable);
        assert!(!failure.requires_user_action);
    }

    #[test]
    fn classifies_permission_privilege_dns_timeout_proxy_tls_integrity_and_signature_failures() {
        let cases = [
            (
                InstallStage::InstallingTools,
                None,
                "permission denied",
                InstallFailureCode::PermissionDenied,
                RecommendedAction::GrantPermission,
                false,
                true,
            ),
            (
                InstallStage::AwaitingConfirmation,
                Some(1602),
                "setup canceled by the user",
                InstallFailureCode::PrivilegeDeclined,
                RecommendedAction::GrantPermission,
                false,
                true,
            ),
            (
                InstallStage::InstallingTools,
                None,
                "getaddrinfo ENOTFOUND registry.npmjs.org",
                InstallFailureCode::DnsFailure,
                RecommendedAction::CheckNetwork,
                true,
                false,
            ),
            (
                InstallStage::InstallingTools,
                None,
                "request timed out after 30s",
                InstallFailureCode::NetworkTimeout,
                RecommendedAction::Retry,
                true,
                false,
            ),
            (
                InstallStage::InstallingTools,
                None,
                "proxy connect ECONNREFUSED 127.0.0.1:7890",
                InstallFailureCode::ProxyUnreachable,
                RecommendedAction::CheckProxy,
                true,
                false,
            ),
            (
                InstallStage::InstallingTools,
                None,
                "self signed certificate in certificate chain",
                InstallFailureCode::TlsFailure,
                RecommendedAction::CheckNetwork,
                false,
                true,
            ),
            (
                InstallStage::InstallingTools,
                None,
                "sha256 checksum mismatch",
                InstallFailureCode::DownloadIntegrityFailure,
                RecommendedAction::Retry,
                false,
                false,
            ),
            (
                InstallStage::InstallingTools,
                None,
                "Authenticode signature verification failed",
                InstallFailureCode::SignatureVerificationFailure,
                RecommendedAction::ViewDiagnostics,
                false,
                true,
            ),
        ];

        for (stage, exit_code, stderr, expected_code, expected_action, retryable, user_action) in
            cases
        {
            let failure = classify_process_failure(stage, exit_code, "", stderr);
            assert_eq!(failure.code, expected_code, "stderr={stderr}");
            assert_eq!(
                failure.recommended_action, expected_action,
                "stderr={stderr}"
            );
            assert_eq!(failure.retryable, retryable, "stderr={stderr}");
            assert_eq!(failure.requires_user_action, user_action, "stderr={stderr}");
        }
    }

    #[test]
    fn classifies_stage_fallbacks_for_repairing_installing_tools_and_verifying() {
        let repairing = classify_process_failure(
            InstallStage::Repairing,
            Some(1),
            "unexpected output",
            "unexpected output",
        );
        assert_eq!(repairing.code, InstallFailureCode::InstallerFailure);
        assert_eq!(repairing.recommended_action, RecommendedAction::Retry);

        let installing_tools = classify_process_failure(
            InstallStage::InstallingTools,
            Some(1),
            "unexpected output",
            "unexpected output",
        );
        assert_eq!(
            installing_tools.code,
            InstallFailureCode::ToolInstallFailure
        );
        assert_eq!(
            installing_tools.recommended_action,
            RecommendedAction::Retry
        );

        let verifying = classify_process_failure(
            InstallStage::Verifying,
            Some(1),
            "unexpected output",
            "unexpected output",
        );
        assert_eq!(verifying.code, InstallFailureCode::VerificationFailure);
        assert_eq!(verifying.recommended_action, RecommendedAction::Retry);
    }

    #[test]
    fn unrelated_output_falls_back_to_stage_specific_default() {
        let failure = classify_process_failure(
            InstallStage::Preflight,
            Some(1),
            "completely unrelated output",
            "still unrelated",
        );

        assert_eq!(failure.code, InstallFailureCode::InstallerFailure);
        assert_eq!(failure.stage, InstallStage::Preflight);
        assert!(!failure.retryable);
        assert!(!failure.requires_user_action);
    }

    #[test]
    fn redacts_tokens_proxy_passwords_and_url_credentials() {
        let raw = "ANTHROPIC_API_KEY=secret https://user:pass@example.com npm_abc123 Bearer tok_12345?token=secret2";
        let clean = redact_diagnostic(raw);

        assert!(!clean.contains("secret"));
        assert!(!clean.contains("user:pass"));
        assert!(!clean.contains("npm_abc123"));
        assert!(!clean.contains("tok_12345"));
        assert!(!clean.contains("secret2"));
        assert!(clean.contains("ANTHROPIC_API_KEY=[REDACTED]"));
        assert!(clean.contains("https://[REDACTED]@example.com"));
        assert!(clean.contains("npm_[REDACTED]"));
        assert!(clean.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn redacts_quoted_assignment_values() {
        let raw =
            r#"API_KEY="quoted-secret" PASSWORD='single-secret' secret_key=`backtick-secret`"#;
        let clean = redact_diagnostic(raw);

        assert!(!clean.contains("quoted-secret"));
        assert!(!clean.contains("single-secret"));
        assert!(!clean.contains("backtick-secret"));
        assert!(clean.contains("API_KEY=[REDACTED]"));
        assert!(clean.contains("PASSWORD=[REDACTED]"));
        assert!(clean.contains("secret_key=[REDACTED]"));
    }

    #[test]
    fn signature_precedence_beats_generic_certificate_noise() {
        let failure = classify_process_failure(
            InstallStage::InstallingTools,
            None,
            "certificate chain validation warning",
            "Authenticode signature verification failed",
        );

        assert_eq!(
            failure.code,
            InstallFailureCode::SignatureVerificationFailure
        );
        assert_eq!(
            failure.recommended_action,
            RecommendedAction::ViewDiagnostics
        );
    }

    #[test]
    fn integrity_precedence_beats_generic_tls_noise() {
        let failure = classify_process_failure(
            InstallStage::InstallingTools,
            None,
            "certificate chain warning",
            "sha256 checksum mismatch",
        );

        assert_eq!(failure.code, InstallFailureCode::DownloadIntegrityFailure);
        assert_eq!(failure.recommended_action, RecommendedAction::Retry);
    }

    #[test]
    fn early_evidence_is_detected_before_long_tail() {
        let stdout = [
            "npm: command not found".to_string(),
            long_tail("irrelevant tail"),
        ]
        .join("\n");
        let stderr = long_tail("still irrelevant");
        let failure =
            classify_process_failure(InstallStage::InstallingTools, Some(1), &stdout, &stderr);

        assert_eq!(failure.code, InstallFailureCode::DependencyMissing);
        assert!(!failure
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("npm: command not found"));
        assert!(failure
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("still irrelevant"));
    }

    #[test]
    fn direct_redaction_handles_mixed_case_secrets_and_proxy_url_credentials() {
        let raw = "PaSsWoRd=SeCrEt value\nBearer abcdefghi\nhttps://user:pass@proxy.example.com:8080?api_key=secret2\n";
        let clean = redact_diagnostic(raw);

        assert!(!clean.contains("SeCrEt"));
        assert!(!clean.contains("abcdefghi"));
        assert!(!clean.contains("user:pass"));
        assert!(!clean.contains("secret2"));
        assert!(clean.contains("PaSsWoRd=[REDACTED]"));
        assert!(clean.contains("Bearer [REDACTED]"));
        assert!(clean.contains("https://[REDACTED]@proxy.example.com:8080?api_key=[REDACTED]"));
    }

    #[test]
    fn normalizes_cross_platform_home_paths_and_environment_values() {
        let raw = r#"C:\Users\Alice\AppData /Users/bob/.config /home/carol/.cache PATH=/secret/bin HOME='/Users/bob'"#;
        let clean = redact_diagnostic(raw);

        assert_eq!(
            clean,
            r#"~\AppData ~/.config ~/.cache PATH=[REDACTED] HOME=[REDACTED]"#
        );
    }

    #[test]
    fn redaction_is_deterministic_for_headers_and_common_tokens() {
        let raw = "Authorization: Basic abc123\nProxy-Authorization: secret\nghp_abcdefgh12345678 sk-abcdefgh12345678\n{\"api_key\":\"json-secret\"}";
        let first = redact_diagnostic(raw);

        assert_eq!(first, redact_diagnostic(raw));
        assert!(!first.contains("abc123"));
        assert!(!first.contains("ghp_"));
        assert!(!first.contains("sk-"));
        assert!(!first.contains("json-secret"));
    }

    #[test]
    fn unrelated_awaiting_confirmation_falls_back_neutrally() {
        let failure = classify_process_failure(
            InstallStage::AwaitingConfirmation,
            Some(1),
            "nothing to see here",
            "still nothing",
        );

        assert_eq!(failure.code, InstallFailureCode::InstallerFailure);
        assert_eq!(
            failure.recommended_action,
            RecommendedAction::ViewDiagnostics
        );
        assert!(!failure.requires_user_action);
        assert!(!failure.retryable);
    }
}
