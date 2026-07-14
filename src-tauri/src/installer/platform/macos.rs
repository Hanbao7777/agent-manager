use super::{CommandSpec, PlatformAdapter};
use crate::installer::{Architecture, CleanEnvironment, InstallFailure, Platform};
use std::path::{Path, PathBuf};

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
        let output = std::process::Command::new(login_shell())
            .args(["-lc", "/usr/libexec/path_helper -s"])
            .output()
            .map_err(|error| path_not_visible(None, &error.to_string()))?;
        let script = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() {
            return Err(path_not_visible(
                output.status.code(),
                &String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(CleanEnvironment {
            path_entries: path_entries_from_path_helper(&script)?,
        })
    }
}

fn login_shell() -> PathBuf {
    std::env::var_os("SHELL")
        .filter(|shell| is_macos_absolute_path(&shell.to_string_lossy()))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/bin/zsh"))
}

fn is_macos_absolute_path(path: &str) -> bool {
    path.starts_with('/')
}

fn path_entries_from_path_helper(script: &str) -> Result<Vec<PathBuf>, InstallFailure> {
    let path = script
        .split("PATH=\"")
        .nth(1)
        .and_then(|value| value.split('"').next())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| path_not_visible(None, "path_helper returned no PATH"))?;
    let entries = macos_path_entries(path);
    (!entries.is_empty())
        .then_some(entries)
        .ok_or_else(|| path_not_visible(None, "path_helper returned an empty PATH"))
}

fn macos_path_entries(path: &str) -> Vec<PathBuf> {
    path.split(':')
        .filter(|entry| !entry.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn path_not_visible(exit_code: Option<i32>, detail: &str) -> InstallFailure {
    InstallFailure {
        code: crate::installer::InstallFailureCode::PathNotVisible,
        stage: crate::installer::InstallStage::Repairing,
        exit_code,
        retryable: true,
        requires_user_action: false,
        message_key: "installer.failure.path_not_visible".into(),
        recommended_action: crate::installer::RecommendedAction::Retry,
        detail: Some(crate::installer::redact_diagnostic(detail)),
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

    #[test]
    fn path_helper_refresh_requires_a_nonempty_path() {
        assert_eq!(
            path_entries_from_path_helper("PATH=\"/usr/local/bin:/usr/bin\"; export PATH;")
                .unwrap(),
            vec![PathBuf::from("/usr/local/bin"), PathBuf::from("/usr/bin")]
        );
        assert_eq!(
            path_entries_from_path_helper("PATH=\"\"; export PATH;")
                .unwrap_err()
                .code,
            crate::installer::InstallFailureCode::PathNotVisible
        );
    }

    #[test]
    fn configured_login_shell_is_used_when_absolute() {
        assert!(is_macos_absolute_path(&login_shell().to_string_lossy()));
    }

    #[test]
    fn macos_absolute_paths_do_not_depend_on_the_host_path_rules() {
        assert!(is_macos_absolute_path("/bin/zsh"));
        assert!(!is_macos_absolute_path("bin/zsh"));
    }

    #[test]
    fn path_helper_output_always_uses_colons_as_separators() {
        assert_eq!(
            macos_path_entries(":/usr/local/bin::/usr/bin:"),
            vec![PathBuf::from("/usr/local/bin"), PathBuf::from("/usr/bin")]
        );
    }
}
