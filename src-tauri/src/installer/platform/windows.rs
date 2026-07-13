use super::{CommandSpec, PlatformAdapter};
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
    fn asset_name(&self, version: &str, architecture: Architecture) -> Option<String> {
        Some(format!(
            "node-{version}-{}.msi",
            match architecture {
                Architecture::X64 => "x64",
                Architecture::Arm64 => "arm64",
            }
        ))
    }
    fn verify_signature_command(&self, package: &Path) -> CommandSpec {
        authenticode_command(package)
    }
    fn install_command(&self, package: &Path) -> CommandSpec {
        msiexec_command(package)
    }
    fn refresh_clean_environment(&self) -> Result<CleanEnvironment, InstallFailure> {
        Ok(CleanEnvironment::default())
    }
}

pub fn authenticode_command(package: &Path) -> CommandSpec {
    CommandSpec { program: "powershell.exe".into(), args: vec!["-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), format!("if ((Get-AuthenticodeSignature -LiteralPath '{}').Status -ne 'Valid') {{ exit 1 }}", package.to_string_lossy().replace('\'', "''"))] }
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
}
