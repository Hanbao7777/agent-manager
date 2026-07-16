use std::path::{Path, PathBuf};

use super::{Architecture, EnvironmentSnapshot, InstallFailure, PathAccess, Platform};

/// Result of running a version command without exposing process details to the planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandProbe {
    pub version: Option<String>,
    pub runnable: bool,
    pub exit_code: Option<i32>,
    pub detail: Option<String>,
}

impl CommandProbe {
    pub fn unavailable() -> Self {
        Self {
            version: None,
            runnable: false,
            exit_code: None,
            detail: None,
        }
    }
}

pub trait ProbeRunner: Send + Sync {
    fn resolve_command(&self, name: &str) -> Vec<PathBuf>;
    fn run_version(&self, path: &Path) -> CommandProbe;
    fn disk_available(&self, path: &Path) -> Result<u64, InstallFailure>;
    fn path_access(&self, path: &Path) -> PathAccess;

    fn path_entries(&self) -> Vec<PathBuf> {
        Vec::new()
    }
}

#[derive(Debug, Clone)]
pub struct ProbeConfig {
    pub node_command: String,
    pub npm_command: String,
    pub path_entries: Vec<PathBuf>,
    pub disk_path: PathBuf,
    pub managed_disk_path: PathBuf,
    pub temporary_directory: PathBuf,
    pub managed_install_directory: PathBuf,
    pub managed_cache_directory: PathBuf,
}

impl ProbeConfig {
    pub fn new(target_directory: impl Into<PathBuf>) -> Self {
        let target_directory = target_directory.into();
        Self {
            node_command: "node".into(),
            npm_command: "npm".into(),
            path_entries: Vec::new(),
            disk_path: target_directory.clone(),
            managed_disk_path: target_directory.clone(),
            temporary_directory: target_directory.clone(),
            managed_install_directory: target_directory.clone(),
            managed_cache_directory: target_directory,
        }
    }
}

pub struct SystemProbe<R: ProbeRunner> {
    runner: R,
}

impl<R: ProbeRunner> SystemProbe<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    pub fn probe(
        &self,
        platform: Platform,
        architecture: Architecture,
        target_directory: &Path,
    ) -> Result<EnvironmentSnapshot, InstallFailure> {
        let mut config = ProbeConfig::new(target_directory);
        config.path_entries = self.runner.path_entries();
        self.probe_with_config(platform, architecture, &config)
    }

    pub fn probe_with_config(
        &self,
        platform: Platform,
        architecture: Architecture,
        config: &ProbeConfig,
    ) -> Result<EnvironmentSnapshot, InstallFailure> {
        let node_paths = self.runner.resolve_command(&config.node_command);
        let npm_paths = self.runner.resolve_command(&config.npm_command);
        let node_path = node_paths.first();
        let npm_path = npm_paths.first();
        let node_probe = node_path
            .map(|path| self.runner.run_version(path))
            .unwrap_or_else(CommandProbe::unavailable);
        let npm_probe = npm_path
            .map(|path| self.runner.run_version(path))
            .unwrap_or_else(CommandProbe::unavailable);

        Ok(EnvironmentSnapshot {
            platform,
            architecture,
            architecture_supported: true,
            node_version: node_probe.version.clone(),
            npm_version: npm_probe.version.clone(),
            node_path: node_path.map(|path| path.to_string_lossy().into_owned()),
            npm_path: npm_path.map(|path| path.to_string_lossy().into_owned()),
            node_runnable: node_probe.runnable,
            npm_runnable: npm_probe.runnable,
            node_path_visible: !node_paths.is_empty(),
            npm_path_visible: !npm_paths.is_empty(),
            node_installations: node_paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
            npm_installations: npm_paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
            path: config
                .path_entries
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
            available_disk_bytes: self.runner.disk_available(&config.disk_path)?,
            managed_available_disk_bytes: self.runner.disk_available(&config.managed_disk_path)?,
            temporary_directory_access: self.runner.path_access(&config.temporary_directory),
            managed_install_directory_access: self
                .runner
                .path_access(&config.managed_install_directory),
            managed_cache_directory_access: self
                .runner
                .path_access(&config.managed_cache_directory),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::PathAccessState;
    use super::*;

    struct FakeRunner {
        node: Vec<PathBuf>,
        npm: Vec<PathBuf>,
        path: Vec<PathBuf>,
        blocked: Vec<String>,
    }

    impl ProbeRunner for FakeRunner {
        fn resolve_command(&self, name: &str) -> Vec<PathBuf> {
            match name {
                "node" | "node-custom" => self.node.clone(),
                "npm" | "npm-custom" => self.npm.clone(),
                _ => Vec::new(),
            }
        }

        fn run_version(&self, path: &Path) -> CommandProbe {
            CommandProbe {
                version: Some(if path.to_string_lossy().contains("npm") {
                    "11.4.2".into()
                } else {
                    "24.4.1".into()
                }),
                runnable: true,
                exit_code: Some(0),
                detail: None,
            }
        }

        fn disk_available(&self, path: &Path) -> Result<u64, InstallFailure> {
            assert!(path.to_string_lossy().contains("disk"));
            Ok(2 * 1024 * 1024 * 1024)
        }

        fn path_access(&self, path: &Path) -> PathAccess {
            let value = path.to_string_lossy();
            let state = if self.blocked.iter().any(|blocked| value.contains(blocked)) {
                PathAccessState::Blocked
            } else {
                PathAccessState::Writable
            };
            PathAccess {
                path: Some(value.into_owned()),
                state,
                detail: None,
            }
        }

        fn path_entries(&self) -> Vec<PathBuf> {
            self.path.clone()
        }
    }

    #[test]
    fn probe_uses_distinct_commands_and_injected_path_entries() {
        let runner = FakeRunner {
            node: vec![PathBuf::from("C:/node/node.exe")],
            npm: vec![PathBuf::from("C:/node/npm.cmd")],
            path: vec![PathBuf::from("C:/node"), PathBuf::from("C:/npm")],
            blocked: Vec::new(),
        };
        let mut config = ProbeConfig::new("C:/disk");
        config.node_command = "node-custom".into();
        config.npm_command = "npm-custom".into();
        config.path_entries = vec![PathBuf::from("C:/path-one"), PathBuf::from("C:/path-two")];
        config.temporary_directory = PathBuf::from("C:/temp");
        config.managed_install_directory = PathBuf::from("C:/install");
        config.managed_cache_directory = PathBuf::from("C:/cache");
        let snapshot = SystemProbe::new(runner)
            .probe_with_config(Platform::Windows, Architecture::X64, &config)
            .unwrap();
        assert_eq!(snapshot.node_path.as_deref(), Some("C:/node/node.exe"));
        assert_eq!(snapshot.npm_path.as_deref(), Some("C:/node/npm.cmd"));
        assert_eq!(snapshot.node_version.as_deref(), Some("24.4.1"));
        assert_eq!(snapshot.npm_version.as_deref(), Some("11.4.2"));
        assert!(snapshot.node_runnable);
        assert!(snapshot.npm_runnable);
        assert_eq!(snapshot.node_installations, vec!["C:/node/node.exe"]);
        assert_eq!(snapshot.npm_installations, vec!["C:/node/npm.cmd"]);
        assert!(snapshot.node_path_visible);
        assert!(snapshot.npm_path_visible);
        assert_eq!(snapshot.path.len(), 2);
        assert_eq!(snapshot.available_disk_bytes, 2 * 1024 * 1024 * 1024);
        assert_eq!(
            snapshot.managed_available_disk_bytes,
            2 * 1024 * 1024 * 1024
        );
        assert_eq!(
            snapshot.temporary_directory_access.state,
            PathAccessState::Writable
        );
        assert_eq!(
            snapshot.managed_install_directory_access.state,
            PathAccessState::Writable
        );
        assert_eq!(
            snapshot.managed_cache_directory_access.state,
            PathAccessState::Writable
        );
    }

    #[test]
    fn probe_propagates_disk_errors() {
        struct ErrorRunner;
        impl ProbeRunner for ErrorRunner {
            fn resolve_command(&self, _name: &str) -> Vec<PathBuf> {
                Vec::new()
            }
            fn run_version(&self, _path: &Path) -> CommandProbe {
                CommandProbe::unavailable()
            }
            fn disk_available(&self, _path: &Path) -> Result<u64, InstallFailure> {
                Err(InstallFailure {
                    code: super::super::InstallFailureCode::InsufficientDiskSpace,
                    stage: super::super::InstallStage::Preflight,
                    exit_code: None,
                    retryable: false,
                    requires_user_action: true,
                    message_key: "disk.error".into(),
                    recommended_action: super::super::RecommendedAction::FreeDiskSpace,
                    detail: Some("disk probe failed".into()),
                })
            }
            fn path_access(&self, path: &Path) -> PathAccess {
                PathAccess {
                    path: Some(path.to_string_lossy().into_owned()),
                    state: PathAccessState::Writable,
                    detail: None,
                }
            }
        }
        let error = SystemProbe::new(ErrorRunner)
            .probe(Platform::Windows, Architecture::X64, Path::new("C:/disk"))
            .unwrap_err();
        assert_eq!(error.detail.as_deref(), Some("disk probe failed"));
    }

    #[test]
    fn no_command_probe_is_negative() {
        let snapshot = SystemProbe::new(FakeRunner {
            node: Vec::new(),
            npm: Vec::new(),
            path: Vec::new(),
            blocked: Vec::new(),
        })
        .probe(Platform::Windows, Architecture::X64, Path::new("C:/disk"))
        .unwrap();
        assert!(!snapshot.node_runnable);
        assert!(!snapshot.npm_runnable);
        assert!(!snapshot.node_path_visible);
        assert!(!snapshot.npm_path_visible);
        assert!(snapshot.node_installations.is_empty());
        assert!(snapshot.npm_installations.is_empty());
    }

    #[test]
    fn each_writability_path_maps_independently() {
        let runner = FakeRunner {
            node: vec![PathBuf::from("C:/node/node.exe")],
            npm: vec![PathBuf::from("C:/node/npm.cmd")],
            path: Vec::new(),
            blocked: vec!["blocked-temp".into()],
        };
        let mut config = ProbeConfig::new("C:/disk");
        config.temporary_directory = PathBuf::from("C:/blocked-temp");
        config.managed_install_directory = PathBuf::from("C:/install");
        config.managed_cache_directory = PathBuf::from("C:/cache");
        let snapshot = SystemProbe::new(runner)
            .probe_with_config(Platform::Windows, Architecture::X64, &config)
            .unwrap();
        assert_eq!(
            snapshot.temporary_directory_access.state,
            PathAccessState::Blocked
        );
        assert_eq!(
            snapshot.managed_install_directory_access.state,
            PathAccessState::Writable
        );
        assert_eq!(
            snapshot.managed_cache_directory_access.state,
            PathAccessState::Writable
        );
    }
}
