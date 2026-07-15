use super::{
    node_asset_descriptor, CommandSpec, NodeAssetDescriptor, NodeInstallerArchitecture,
    NodeInstallerPlatform, PlatformAdapter,
};
use crate::installer::{
    Architecture, CleanEnvironment, InstallFailure, InstallFailureCode, InstallStage, Platform,
    RecommendedAction,
};
use std::path::Path;

pub struct WindowsAdapter;

impl PlatformAdapter for WindowsAdapter {
    fn platform(&self) -> Platform {
        Platform::Windows
    }
    fn node_asset(&self, version: &str, architecture: Architecture) -> Option<NodeAssetDescriptor> {
        Some(node_asset_descriptor(
            NodeInstallerPlatform::Windows,
            match architecture {
                Architecture::X64 => NodeInstallerArchitecture::X64,
                Architecture::Arm64 => NodeInstallerArchitecture::Arm64,
            },
            version,
        ))
    }
    fn verify_signature_command(&self, package: &Path) -> CommandSpec {
        authenticode_command(package)
    }
    fn install_command(&self, package: &Path) -> CommandSpec {
        msiexec_command(package)
    }
    fn refresh_clean_environment(&self) -> Result<CleanEnvironment, InstallFailure> {
        Ok(CleanEnvironment {
            path_entries: refreshed_path_entries()?,
        })
    }
}

#[cfg(target_os = "windows")]
fn refreshed_path_entries() -> Result<Vec<std::path::PathBuf>, InstallFailure> {
    use winreg::{enums::*, RegKey};
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let user = hkcu
        .open_subkey("Environment")
        .ok()
        .and_then(|key| key.get_value::<String, _>("Path").ok());
    let system = hklm
        .open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")
        .ok()
        .and_then(|key| key.get_value::<String, _>("Path").ok());
    path_entries_from_registry_values(user, system)
}

#[cfg(not(target_os = "windows"))]
fn refreshed_path_entries() -> Result<Vec<std::path::PathBuf>, InstallFailure> {
    path_entries_from_registry_values(std::env::var("PATH").ok(), None)
}

fn path_entries_from_registry_values(
    user: Option<String>,
    system: Option<String>,
) -> Result<Vec<std::path::PathBuf>, InstallFailure> {
    let joined = [user, system]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(";");
    let entries = std::env::split_paths(std::ffi::OsStr::new(&joined))
        .filter(|entry| !entry.as_os_str().is_empty())
        .collect::<Vec<_>>();
    (!entries.is_empty())
        .then_some(entries)
        .ok_or_else(|| InstallFailure {
            code: InstallFailureCode::PathNotVisible,
            stage: InstallStage::Repairing,
            exit_code: None,
            retryable: true,
            requires_user_action: false,
            message_key: "installer.failure.path_not_visible".into(),
            recommended_action: RecommendedAction::Retry,
            detail: Some("Windows user and system PATH are unavailable".into()),
        })
}

pub const AUTHENTICODE_SCRIPT: &str =
    "& { param([string]$path) $sig = Get-AuthenticodeSignature -LiteralPath $path; if ($sig.Status -ne 'Valid') { Write-Error $sig.Status; exit 1 } }";

pub fn authenticode_command(package: &Path) -> CommandSpec {
    CommandSpec {
        program: "powershell.exe".into(),
        args: vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            AUTHENTICODE_SCRIPT.into(),
            package.to_string_lossy().into_owned(),
        ],
    }
}
pub fn msiexec_command(package: &Path) -> CommandSpec {
    CommandSpec {
        program: "msiexec.exe".into(),
        args: vec![
            "/i".into(),
            package.to_string_lossy().into_owned(),
            "/passive".into(),
            "/norestart".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_keep_path_as_one_argument() {
        let path = Path::new(r"C:\a b\node.msi");
        assert_eq!(msiexec_command(path).args[1], path.to_string_lossy());
        assert!(authenticode_command(path).args[3].contains("-LiteralPath"));
    }

    #[test]
    fn authenticode_uses_fixed_script_and_hostile_path_argument() {
        let command = authenticode_command(Path::new(r"C:\a;$(whoami)\node.msi"));
        assert_eq!(command.args[3], AUTHENTICODE_SCRIPT);
        assert_eq!(command.args[4], r"C:\a;$(whoami)\node.msi");
    }

    #[test]
    fn registry_path_refresh_requires_at_least_one_nonempty_value() {
        assert!(
            path_entries_from_registry_values(None, Some("C:\\Node;C:\\Windows".into())).is_ok()
        );
        assert_eq!(
            path_entries_from_registry_values(Some(" ".into()), None)
                .unwrap_err()
                .code,
            InstallFailureCode::PathNotVisible
        );
    }
}
