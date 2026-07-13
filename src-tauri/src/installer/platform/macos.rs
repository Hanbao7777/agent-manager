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
        let output = std::process::Command::new("/bin/zsh")
            .args(["-lc", "/usr/libexec/path_helper -s"])
            .output()
            .map_err(|error| InstallFailure {
                code: crate::installer::InstallFailureCode::PathNotVisible,
                stage: crate::installer::InstallStage::Repairing,
                exit_code: None,
                retryable: true,
                requires_user_action: false,
                message_key: "installer.failure.path_not_visible".into(),
                recommended_action: crate::installer::RecommendedAction::Retry,
                detail: Some(crate::installer::redact_diagnostic(&error.to_string())),
            })?;
        let script = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() {
            return Err(InstallFailure {
                code: crate::installer::InstallFailureCode::PathNotVisible,
                stage: crate::installer::InstallStage::Repairing,
                exit_code: output.status.code(),
                retryable: true,
                requires_user_action: false,
                message_key: "installer.failure.path_not_visible".into(),
                recommended_action: crate::installer::RecommendedAction::Retry,
                detail: Some(crate::installer::redact_diagnostic(
                    &String::from_utf8_lossy(&output.stderr),
                )),
            });
        }
        let path = script
            .split("PATH=\"")
            .nth(1)
            .and_then(|value| value.split('"').next())
            .unwrap_or_default();
        Ok(CleanEnvironment {
            path_entries: std::env::split_paths(path).collect(),
        })
    }
}
pub fn apple_script_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
pub fn pkgutil_command(package: &Path) -> CommandSpec {
    CommandSpec {
        program: "/bin/sh".into(),
        args: vec![
            "-c".into(),
            r#"set -eu; output=$(/usr/sbin/pkgutil --check-signature -- "$1"); printf '%s\n' "$output" | grep -Fq 'Developer ID Installer'"#.into(),
            "cc-switch-verify-pkg".into(),
            package.to_string_lossy().into_owned(),
        ],
    }
}
pub fn osascript_install_command(package: &Path) -> CommandSpec {
    CommandSpec {
        program: "/usr/bin/osascript".into(),
        args: vec![
            "-e".into(),
            "on run argv\ndo shell script \"/usr/sbin/installer -pkg \" & quoted form of item 1 of argv & \" -target /\" with administrator privileges\nend run".into(),
            package.to_string_lossy().into_owned(),
        ],
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn applescript_escapes_quote_and_backslash() {
        assert_eq!(apple_script_literal("a\\b\"c"), "a\\\\b\\\"c");
    }

    #[test]
    fn package_path_is_a_separate_shell_argument() {
        let command = pkgutil_command(Path::new("/tmp/a hostile;name.pkg"));
        assert_eq!(command.program, "/bin/sh");
        assert_eq!(command.args[3], "/tmp/a hostile;name.pkg");
        assert!(command.args[1].contains("Developer ID Installer"));
    }
}
