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
        if matches!(row.get("lts"), None | Some(serde_json::Value::Bool(false))) {
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
        if selected.as_ref().is_none_or(|(current, _, _)| {
            major > *current
                || (major == *current && version > selected.as_ref().unwrap().1.as_str())
        }) {
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
    if sha256_hex(&bytes) != expected {
        let _ = fs::remove_dir_all(task_temp);
        return Err(failure(
            InstallFailureCode::DownloadIntegrityFailure,
            "SHA-256 mismatch",
        ));
    }
    let package = task_temp.join(&release.asset_name);
    fs::write(&package, bytes)
        .map_err(|error| failure(InstallFailureCode::PermissionDenied, &error.to_string()))?;
    Ok(package)
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

fn run_checked(command: CommandSpec, code: InstallFailureCode) -> Result<(), InstallFailure> {
    let status = std::process::Command::new(&command.program)
        .args(&command.args)
        .status()
        .map_err(|error| failure(code.clone(), &error.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(failure(code, "platform installer command failed"))
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
}
