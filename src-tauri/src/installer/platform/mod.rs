use std::future::Future;
use std::time::Duration;
use std::{
    error::Error as _,
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::installer::{
    Architecture, CleanEnvironment, InstallFailure, InstallFailureCode, InstallStage, NodePolicy,
    Platform, RecommendedAction, ResolvedNodeNpmPair,
};

pub mod macos;
mod node_assets;
pub mod windows;

pub use node_assets::{
    node_asset_descriptor, NodeAssetDescriptor, NodeInstallerArchitecture, NodeInstallerPlatform,
};

const NODE_INDEX_URL: &str = "https://nodejs.org/dist/index.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRelease {
    pub version: String,
    pub major: u64,
    pub asset_name: String,
    pub download_url: String,
    pub checksums_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

/// Owns a per-install temporary directory and removes it on every return path.
pub struct TaskTempGuard {
    path: PathBuf,
}

impl TaskTempGuard {
    pub fn new(path: PathBuf) -> Result<Self, InstallFailure> {
        fs::create_dir_all(&path)
            .map_err(|error| failure(InstallFailureCode::PermissionDenied, &error.to_string()))?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TaskTempGuard {
    fn drop(&mut self) {
        cleanup_task_temp(&self.path);
    }
}

pub trait PlatformAdapter {
    fn platform(&self) -> Platform;
    fn node_asset(&self, version: &str, architecture: Architecture) -> Option<NodeAssetDescriptor>;
    fn verify_signature_command(&self, package: &Path) -> CommandSpec;
    fn install_command(&self, package: &Path) -> CommandSpec;
    fn refresh_clean_environment(&self) -> Result<CleanEnvironment, InstallFailure>;
}

pub fn resolve_node_release(
    index: &str,
    policy: &NodePolicy,
    adapter: &dyn PlatformAdapter,
    architecture: Architecture,
) -> Result<NodeRelease, InstallFailure> {
    let rows: serde_json::Value = serde_json::from_str(index).map_err(|_| {
        failure(
            InstallFailureCode::InstallerFailure,
            "invalid Node release index",
        )
    })?;
    let rows = rows.as_array().ok_or_else(|| {
        failure(
            InstallFailureCode::InstallerFailure,
            "invalid Node release index",
        )
    })?;
    let mut selected: Option<(u64, String, String)> = None;
    for row in rows {
        if !matches!(row.get("lts"), Some(serde_json::Value::String(value)) if !value.is_empty()) {
            continue;
        }
        let version = match row.get("version").and_then(serde_json::Value::as_str) {
            Some(version) => version,
            None => continue,
        };
        let major = version
            .trim_start_matches('v')
            .split('.')
            .next()
            .and_then(|value| value.parse().ok());
        let Some(major) = major else { continue };
        if !policy.accepts_major(major) {
            continue;
        }
        let Some(asset) = adapter.node_asset(version, architecture.clone()) else {
            continue;
        };
        if !row
            .get("files")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|files| {
                files
                    .iter()
                    .any(|file| file.as_str() == Some(asset.index_file))
            })
        {
            continue;
        }
        if selected
            .as_ref()
            .is_none_or(|(_, current, _)| version_key(version) > version_key(current))
        {
            selected = Some((major, version.to_string(), asset.package_name));
        }
    }
    let (major, version, asset_name) = selected.ok_or_else(|| {
        failure(
            InstallFailureCode::UnsupportedArchitecture,
            "no supported Node release asset",
        )
    })?;
    let base = format!("https://nodejs.org/dist/{version}");
    Ok(NodeRelease {
        version,
        major,
        download_url: format!("{base}/{asset_name}"),
        checksums_url: format!("{base}/SHASUMS256.txt"),
        asset_name,
    })
}

fn version_key(version: &str) -> (u64, u64, u64) {
    let mut values = version
        .trim_start_matches('v')
        .split('.')
        .map(|part| part.parse().unwrap_or(0));
    (
        values.next().unwrap_or(0),
        values.next().unwrap_or(0),
        values.next().unwrap_or(0),
    )
}

pub async fn fetch_node_index() -> Result<String, InstallFailure> {
    retry_transient_download(|| async {
        crate::proxy::http_client::get()
            .get(NODE_INDEX_URL)
            .send()
            .await
            .map_err(network_failure)?
            .error_for_status()
            .map_err(network_failure)?
            .text()
            .await
            .map_err(network_failure)
    })
    .await
}

async fn retry_transient_download<T, F, Fut>(mut operation: F) -> Result<T, InstallFailure>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, InstallFailure>>,
{
    for attempt in 0..3 {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) if attempt < 2 && is_transient_download_failure(&error) => {
                tokio::time::sleep(Duration::from_millis(100 * (attempt + 1))).await;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("the retry loop always returns")
}

fn is_transient_download_failure(error: &InstallFailure) -> bool {
    matches!(
        error.code,
        InstallFailureCode::DnsFailure
            | InstallFailureCode::NetworkTimeout
            | InstallFailureCode::ProxyUnreachable
    )
}

pub async fn download_and_verify_node(
    release: &NodeRelease,
    task_temp: &Path,
) -> Result<PathBuf, InstallFailure> {
    fs::create_dir_all(task_temp)
        .map_err(|error| failure(InstallFailureCode::PermissionDenied, &error.to_string()))?;
    let client = crate::proxy::http_client::get();
    let checksum_text = retry_transient_download(|| async {
        client
            .get(&release.checksums_url)
            .send()
            .await
            .map_err(network_failure)?
            .error_for_status()
            .map_err(|error| {
                failure(
                    InstallFailureCode::DownloadIntegrityFailure,
                    &error.to_string(),
                )
            })?
            .text()
            .await
            .map_err(network_failure)
    })
    .await?;
    let expected = checksum_for_asset(&checksum_text, &release.asset_name).ok_or_else(|| {
        failure(
            InstallFailureCode::DownloadIntegrityFailure,
            "checksum entry missing",
        )
    })?;
    let bytes = retry_transient_download(|| async {
        client
            .get(&release.download_url)
            .send()
            .await
            .map_err(network_failure)?
            .error_for_status()
            .map_err(|error| {
                failure(
                    InstallFailureCode::DownloadIntegrityFailure,
                    &error.to_string(),
                )
            })?
            .bytes()
            .await
            .map_err(network_failure)
    })
    .await?;
    if let Err(error) = verify_sha256(&bytes, &expected) {
        let _ = fs::remove_dir_all(task_temp);
        return Err(error);
    }
    let package = task_temp.join(&release.asset_name);
    fs::write(&package, bytes)
        .map_err(|error| failure(InstallFailureCode::PermissionDenied, &error.to_string()))?;
    Ok(package)
}

pub async fn install_node_release(
    adapter: &dyn PlatformAdapter,
    release: &NodeRelease,
    task_temp: PathBuf,
) -> Result<(), InstallFailure> {
    let temp = TaskTempGuard::new(task_temp)?;
    continue_after_verified_download(
        adapter,
        download_and_verify_node(release, temp.path()).await,
        install_node,
    )
}

fn continue_after_verified_download<F>(
    adapter: &dyn PlatformAdapter,
    package: Result<PathBuf, InstallFailure>,
    install: F,
) -> Result<(), InstallFailure>
where
    F: FnOnce(&dyn PlatformAdapter, &Path) -> Result<(), InstallFailure>,
{
    let package = package?;
    install(adapter, &package)
}

pub fn cleanup_task_temp(task_temp: &Path) {
    let _ = fs::remove_dir_all(task_temp);
}

pub fn checksum_for_asset(checksums: &str, asset_name: &str) -> Option<String> {
    checksums.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let checksum = fields.next()?;
        let name = fields.next()?;
        (name == asset_name
            && checksum.len() == 64
            && checksum.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| checksum.to_ascii_lowercase())
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn verify_sha256(bytes: &[u8], expected: &str) -> Result<(), InstallFailure> {
    (sha256_hex(bytes) == expected)
        .then_some(())
        .ok_or_else(|| {
            failure(
                InstallFailureCode::DownloadIntegrityFailure,
                "SHA-256 mismatch",
            )
        })
}

pub fn install_node(adapter: &dyn PlatformAdapter, package: &Path) -> Result<(), InstallFailure> {
    run_checked(
        adapter.verify_signature_command(package),
        InstallFailureCode::SignatureVerificationFailure,
    )?;
    run_checked(
        adapter.install_command(package),
        InstallFailureCode::InstallerFailure,
    )
}

pub fn refresh_environment(
    adapter: &dyn PlatformAdapter,
) -> Result<CleanEnvironment, InstallFailure> {
    adapter.refresh_clean_environment()
}

pub fn selected_node_npm_pair(
    node: PathBuf,
    npm: PathBuf,
    source: &'static str,
) -> ResolvedNodeNpmPair {
    ResolvedNodeNpmPair { node, npm, source }
}

pub fn selected_environment(
    environment: &CleanEnvironment,
    pair: &ResolvedNodeNpmPair,
) -> CleanEnvironment {
    environment.with_dependency_paths(&[pair.node.clone()], &[pair.npm.clone()])
}

fn run_checked(command: CommandSpec, code: InstallFailureCode) -> Result<(), InstallFailure> {
    let output = std::process::Command::new(&command.program)
        .args(&command.args)
        .output()
        .map_err(|error| failure(code.clone(), &error.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        let classified = crate::installer::classify_process_failure(
            InstallStage::Repairing,
            output.status.code(),
            &String::from_utf8_lossy(&output.stdout),
            &String::from_utf8_lossy(&output.stderr),
        );
        if classified.code == InstallFailureCode::InstallerFailure
            && code != InstallFailureCode::InstallerFailure
        {
            Err(failure(code, "platform installer command failed"))
        } else {
            Err(classified)
        }
    }
}

fn network_failure(error: reqwest::Error) -> InstallFailure {
    let mut details = vec![error.to_string()];
    let mut source = error.source();
    while let Some(cause) = source {
        details.push(cause.to_string());
        source = cause.source();
    }
    let detail = details.join(": ");
    let classified =
        crate::installer::classify_process_failure(InstallStage::Repairing, None, "", &detail);
    if matches!(
        classified.code,
        InstallFailureCode::DnsFailure
            | InstallFailureCode::NetworkTimeout
            | InstallFailureCode::ProxyUnreachable
            | InstallFailureCode::TlsFailure
    ) {
        classified
    } else {
        InstallFailure {
            code: InstallFailureCode::InstallerFailure,
            stage: InstallStage::Repairing,
            exit_code: None,
            retryable: false,
            requires_user_action: true,
            message_key: "installer.failure.repairing".into(),
            recommended_action: RecommendedAction::CheckNetwork,
            detail: Some(crate::installer::redact_diagnostic(&detail)),
        }
    }
}

fn failure(code: InstallFailureCode, detail: &str) -> InstallFailure {
    InstallFailure {
        code,
        stage: InstallStage::Repairing,
        exit_code: None,
        retryable: false,
        requires_user_action: true,
        message_key: "installer.failure.platform".into(),
        recommended_action: RecommendedAction::ViewDiagnostics,
        detail: Some(crate::installer::redact_diagnostic(detail)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestAdapter;
    impl PlatformAdapter for TestAdapter {
        fn platform(&self) -> Platform {
            Platform::Windows
        }
        fn node_asset(
            &self,
            version: &str,
            architecture: Architecture,
        ) -> Option<NodeAssetDescriptor> {
            Some(node_asset_descriptor(
                NodeInstallerPlatform::Windows,
                match architecture {
                    Architecture::X64 => NodeInstallerArchitecture::X64,
                    Architecture::Arm64 => NodeInstallerArchitecture::Arm64,
                },
                version,
            ))
        }
        fn verify_signature_command(&self, _: &Path) -> CommandSpec {
            CommandSpec {
                program: "verify".into(),
                args: vec![],
            }
        }
        fn install_command(&self, _: &Path) -> CommandSpec {
            CommandSpec {
                program: "install".into(),
                args: vec![],
            }
        }
        fn refresh_clean_environment(&self) -> Result<CleanEnvironment, InstallFailure> {
            Ok(CleanEnvironment::default())
        }
    }
    #[test]
    fn selects_newest_allowed_lts_exact_asset() {
        let index = r#"[{"version":"v24.1.0","lts":"Krypton","files":["win-x64-msi"]},{"version":"v22.9.0","lts":"Jod","files":["win-x64-msi"]}]"#;
        assert_eq!(
            resolve_node_release(
                index,
                &crate::installer::node_policy(),
                &TestAdapter,
                Architecture::X64
            )
            .unwrap()
            .major,
            24
        );
    }

    #[test]
    fn accepts_official_node_index_platform_identifiers() {
        let index =
            r#"[{"version":"v24.18.0","lts":"Krypton","files":["osx-x64-pkg","win-x64-msi"]}]"#;

        let windows = resolve_node_release(
            index,
            &crate::installer::node_policy(),
            &windows::WindowsAdapter,
            Architecture::X64,
        )
        .unwrap();
        assert_eq!(windows.asset_name, "node-v24.18.0-x64.msi");

        let macos = resolve_node_release(
            index,
            &crate::installer::node_policy(),
            &macos::MacosAdapter,
            Architecture::X64,
        )
        .unwrap();
        assert_eq!(macos.asset_name, "node-v24.18.0.pkg");
    }

    #[test]
    fn skips_newer_lts_without_the_exact_platform_asset() {
        let index = r#"[{"version":"v24.2.0","lts":"Iron","files":["win-arm64-msi"]},{"version":"v24.1.0","lts":"Iron","files":["win-x64-msi"]}]"#;

        let release = resolve_node_release(
            index,
            &crate::installer::node_policy(),
            &TestAdapter,
            Architecture::X64,
        )
        .unwrap();

        assert_eq!(release.version, "v24.1.0");
        assert_eq!(release.asset_name, "node-v24.1.0-x64.msi");
    }

    #[test]
    fn rejects_non_string_lts_rows_even_when_the_asset_exists() {
        let index = r#"[{"version":"v24.2.0","lts":true,"files":["win-x64-msi"]},{"version":"v24.1.0","lts":false,"files":["win-x64-msi"]}]"#;

        assert_eq!(
            resolve_node_release(
                index,
                &crate::installer::node_policy(),
                &TestAdapter,
                Architecture::X64,
            )
            .unwrap_err()
            .code,
            InstallFailureCode::UnsupportedArchitecture
        );
    }
    #[test]
    fn checksum_requires_exact_asset() {
        let checksum = "a".repeat(64);
        assert_eq!(
            checksum_for_asset(&format!("{checksum}  node-v24.msi"), "node-v24.msi").as_deref(),
            Some(checksum.as_str())
        );
        assert!(
            checksum_for_asset(&format!("{}  other.msi", "a".repeat(64)), "node-v24.msi").is_none()
        );
    }

    #[test]
    fn checksum_mismatch_is_detected_before_a_package_is_written() {
        let downloaded = b"tampered package";
        let expected = sha256_hex(b"official package");

        assert_eq!(
            verify_sha256(downloaded, &expected).unwrap_err().code,
            InstallFailureCode::DownloadIntegrityFailure
        );
    }

    #[test]
    fn integrity_failure_stops_before_signature_or_install() {
        let package = PathBuf::from("untrusted.msi");
        let integrity_failure = verify_sha256(b"tampered", &sha256_hex(b"official"));
        let mut install_calls = 0;

        let result = continue_after_verified_download(
            &TestAdapter,
            integrity_failure.map(|_| package),
            |_, _| {
                install_calls += 1;
                Ok(())
            },
        );

        assert_eq!(
            result.unwrap_err().code,
            InstallFailureCode::DownloadIntegrityFailure
        );
        assert_eq!(install_calls, 0);
    }

    #[test]
    fn signature_rejection_and_authorization_decline_keep_their_failure_codes() {
        let signature = crate::installer::classify_process_failure(
            InstallStage::Repairing,
            Some(1),
            "",
            "Authenticode signature verification failed",
        );
        let declined = crate::installer::classify_process_failure(
            InstallStage::Repairing,
            Some(1),
            "",
            "installer cancelled by the user",
        );

        assert_eq!(
            signature.code,
            InstallFailureCode::SignatureVerificationFailure
        );
        assert_eq!(declined.code, InstallFailureCode::PrivilegeDeclined);
    }

    #[test]
    fn numeric_version_ordering_does_not_use_lexical_minor_order() {
        assert!(version_key("v24.10.0") > version_key("v24.9.0"));
    }

    #[test]
    fn task_temp_guard_cleans_success_failure_and_cancellation_paths() {
        let path = std::env::temp_dir().join(format!("installer-test-{}", std::process::id()));
        {
            let guard = TaskTempGuard::new(path.clone()).unwrap();
            fs::write(guard.path().join("artifact"), b"temporary").unwrap();
        }
        assert!(!path.exists());

        let guard = TaskTempGuard::new(path.clone()).unwrap();
        fs::write(guard.path().join("failed-artifact"), b"temporary").unwrap();
        let _: Result<(), InstallFailure> = Err(failure(
            InstallFailureCode::InstallerFailure,
            "simulated install failure",
        ));
        drop(guard);
        assert!(!path.exists());

        let guard = TaskTempGuard::new(path.clone()).unwrap();
        fs::write(guard.path().join("cancelled-artifact"), b"temporary").unwrap();
        drop(guard);
        assert!(!path.exists());
    }

    #[test]
    fn only_network_dns_timeout_and_proxy_failures_are_download_retryable() {
        for code in [
            InstallFailureCode::DnsFailure,
            InstallFailureCode::NetworkTimeout,
            InstallFailureCode::ProxyUnreachable,
        ] {
            assert!(is_transient_download_failure(&failure(code, "transient")));
        }
        for code in [
            InstallFailureCode::TlsFailure,
            InstallFailureCode::DownloadIntegrityFailure,
            InstallFailureCode::SignatureVerificationFailure,
            InstallFailureCode::PrivilegeDeclined,
            InstallFailureCode::InstallerFailure,
        ] {
            assert!(!is_transient_download_failure(&failure(code, "terminal")));
        }
    }

    #[test]
    fn transient_download_retry_stops_after_three_total_attempts() {
        let attempts = std::sync::atomic::AtomicUsize::new(0);
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(retry_transient_download(|| {
                attempts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                async { Err::<(), _>(failure(InstallFailureCode::DnsFailure, "offline")) }
            }));

        assert_eq!(result.unwrap_err().code, InstallFailureCode::DnsFailure);
        assert_eq!(attempts.load(std::sync::atomic::Ordering::Relaxed), 3);
    }

    #[test]
    fn integrity_failure_does_not_retry_download() {
        let attempts = std::sync::atomic::AtomicUsize::new(0);
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(retry_transient_download(|| {
                attempts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                async {
                    Err::<(), _>(failure(
                        InstallFailureCode::DownloadIntegrityFailure,
                        "checksum mismatch",
                    ))
                }
            }));

        assert_eq!(
            result.unwrap_err().code,
            InstallFailureCode::DownloadIntegrityFailure
        );
        assert_eq!(attempts.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
}
