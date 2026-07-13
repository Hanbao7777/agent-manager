use std::{
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::installer::{
    Architecture, CleanEnvironment, InstallFailure, InstallFailureCode, InstallStage, NodePolicy,
    Platform, RecommendedAction, ResolvedNodeNpmPair,
};

pub mod macos;
pub mod windows;

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
    fn asset_name(&self, version: &str, architecture: Architecture) -> Option<String>;
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
        let Some(asset_name) = adapter.asset_name(version, architecture) else {
            continue;
        };
        if !row
            .get("files")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|files| {
                files
                    .iter()
                    .any(|file| file.as_str() == Some(asset_name.as_str()))
            })
        {
            continue;
        }
        if selected
            .as_ref()
            .is_none_or(|(_, current, _)| version_key(version) > version_key(current))
        {
            selected = Some((major, version.to_string(), asset_name));
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
    crate::proxy::http_client::get()
        .get(NODE_INDEX_URL)
        .send()
        .await
        .map_err(|error| failure(InstallFailureCode::NetworkTimeout, &error.to_string()))?
        .error_for_status()
        .map_err(|error| failure(InstallFailureCode::NetworkTimeout, &error.to_string()))?
        .text()
        .await
        .map_err(|error| failure(InstallFailureCode::NetworkTimeout, &error.to_string()))
}

pub async fn download_and_verify_node(
    release: &NodeRelease,
    task_temp: &Path,
) -> Result<PathBuf, InstallFailure> {
    fs::create_dir_all(task_temp)
        .map_err(|error| failure(InstallFailureCode::PermissionDenied, &error.to_string()))?;
    let client = crate::proxy::http_client::get();
    let checksum_text = client
        .get(&release.checksums_url)
        .send()
        .await
        .map_err(|error| failure(InstallFailureCode::NetworkTimeout, &error.to_string()))?
        .error_for_status()
        .map_err(|error| {
            failure(
                InstallFailureCode::DownloadIntegrityFailure,
                &error.to_string(),
            )
        })?
        .text()
        .await
        .map_err(|error| failure(InstallFailureCode::NetworkTimeout, &error.to_string()))?;
    let expected = checksum_for_asset(&checksum_text, &release.asset_name).ok_or_else(|| {
        failure(
            InstallFailureCode::DownloadIntegrityFailure,
            "checksum entry missing",
        )
    })?;
    let bytes = client
        .get(&release.download_url)
        .send()
        .await
        .map_err(|error| failure(InstallFailureCode::NetworkTimeout, &error.to_string()))?
        .error_for_status()
        .map_err(|error| {
            failure(
                InstallFailureCode::DownloadIntegrityFailure,
                &error.to_string(),
            )
        })?
        .bytes()
        .await
        .map_err(|error| failure(InstallFailureCode::NetworkTimeout, &error.to_string()))?;
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
    let package = download_and_verify_node(release, temp.path()).await?;
    install_node(adapter, &package)
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
        if classified.code == InstallFailureCode::PrivilegeDeclined
            || classified.code == InstallFailureCode::SignatureVerificationFailure
        {
            Err(classified)
        } else {
            Err(failure(code, "platform installer command failed"))
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
        fn asset_name(&self, version: &str, architecture: Architecture) -> Option<String> {
            Some(
                format!(
                    "node-{version}-{} .msi",
                    if architecture == Architecture::X64 {
                        "x64"
                    } else {
                        "arm64"
                    }
                )
                .replace(" ", ""),
            )
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
        let index = r#"[{"version":"v24.1.0","lts":true,"files":["node-v24.1.0-x64.msi"]},{"version":"v22.9.0","lts":true,"files":["node-v22.9.0-x64.msi"]}]"#;
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
    fn skips_newer_lts_without_the_exact_platform_asset() {
        let index = r#"[{"version":"v24.2.0","lts":"Iron","files":["node-v24.2.0-arm64.msi"]},{"version":"v24.1.0","lts":"Iron","files":["node-v24.1.0-x64.msi"]}]"#;

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
    fn checksum_requires_exact_asset() {
        assert_eq!(
            checksum_for_asset(&format!("{}  node-v24.msi", "a".repeat(64)), "node-v24.msi")
                .as_deref(),
            Some(&"a".repeat(64))
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
    fn numeric_version_ordering_does_not_use_lexical_minor_order() {
        assert!(version_key("v24.10.0") > version_key("v24.9.0"));
    }

    #[test]
    fn task_temp_guard_cleans_success_and_failure_paths() {
        let path = std::env::temp_dir().join(format!("installer-test-{}", std::process::id()));
        {
            let guard = TaskTempGuard::new(path.clone()).unwrap();
            fs::write(guard.path().join("artifact"), b"temporary").unwrap();
        }
        assert!(!path.exists());
    }
}
