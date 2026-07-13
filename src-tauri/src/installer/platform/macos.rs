use super::{CommandSpec, PlatformAdapter};
use crate::installer::{Architecture, CleanEnvironment, InstallFailure, Platform};
use std::path::Path;

pub struct MacosAdapter;
impl PlatformAdapter for MacosAdapter {
    fn platform(&self) -> Platform {
        Platform::Macos
    }
    fn asset_name(&self, version: &str, architecture: Architecture) -> Option<String> {
        Some(format!(
            "node-{version}-{}.pkg",
            match architecture {
                Architecture::X64 => "x64",
                Architecture::Arm64 => "arm64",
            }
        ))
    }
    fn verify_signature_command(&self, package: &Path) -> CommandSpec {
        pkgutil_command(package)
    }
    fn install_command(&self, package: &Path) -> CommandSpec {
        osascript_install_command(package)
    }
    fn refresh_clean_environment(&self) -> Result<CleanEnvironment, InstallFailure> {
        Ok(CleanEnvironment::default())
    }
}
pub fn apple_script_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
pub fn pkgutil_command(package: &Path) -> CommandSpec {
    CommandSpec {
        program: "/usr/sbin/pkgutil".into(),
        args: vec![
            "--check-signature".into(),
            package.to_string_lossy().into_owned(),
        ],
    }
}
pub fn osascript_install_command(package: &Path) -> CommandSpec {
    let path = apple_script_literal(&package.to_string_lossy());
    CommandSpec { program: "/usr/bin/osascript".into(), args: vec!["-e".into(), format!("do shell script \"/usr/sbin/installer -pkg \\\"{path}\\\" -target /\" with administrator privileges")] }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn applescript_escapes_quote_and_backslash() {
        assert_eq!(apple_script_literal("a\\b\"c"), "a\\\\b\\\"c");
    }
}
