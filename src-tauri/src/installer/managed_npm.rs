use std::{
    ffi::{OsStr, OsString},
    fs::OpenOptions,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use super::{
    classify_process_failure, parse_command_version_output, replace_file, InstallFailure,
    InstallFailureCode, InstallStage, Platform, RecommendedAction, ToolId, ToolInstallResult,
    ToolInstallStatus,
};

const MANAGED_CACHE_CAP_BYTES: u64 = 500 * 1024 * 1024;
const CACHE_OWNERSHIP_MARKER: &str = ".agent-manager-owned";
const CACHE_OWNERSHIP_CONTENTS: &[u8] = b"Agent Manager managed npm cache\n";
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ManagedNpmTool {
    tool: ToolId,
    identity: &'static str,
    package: &'static str,
    executable: &'static str,
}

impl ManagedNpmTool {
    pub(crate) fn from_tool(tool: ToolId) -> Result<Self, InstallFailure> {
        let identity = match tool {
            ToolId::Claude => "claude",
            ToolId::Codex => "codex",
            ToolId::Gemini => "gemini",
            ToolId::Opencode => "opencode",
            ToolId::Openclaw => "openclaw",
            ToolId::Hermes => return Err(allowlist_failure("unsupported managed npm tool")),
        };
        Self::from_identity(identity)
    }

    pub(crate) fn from_identity(identity: &str) -> Result<Self, InstallFailure> {
        match identity {
            "claude" => Ok(Self {
                tool: ToolId::Claude,
                identity: "claude",
                package: "@anthropic-ai/claude-code",
                executable: "claude",
            }),
            "codex" => Ok(Self {
                tool: ToolId::Codex,
                identity: "codex",
                package: "@openai/codex",
                executable: "codex",
            }),
            "gemini" => Ok(Self {
                tool: ToolId::Gemini,
                identity: "gemini",
                package: "@google/gemini-cli",
                executable: "gemini",
            }),
            "opencode" => Ok(Self {
                tool: ToolId::Opencode,
                identity: "opencode",
                package: "opencode-ai",
                executable: "opencode",
            }),
            "openclaw" => Ok(Self {
                tool: ToolId::Openclaw,
                identity: "openclaw",
                package: "openclaw",
                executable: "openclaw",
            }),
            _ => Err(allowlist_failure("unsupported managed npm identity")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExistingCliInstallation {
    Missing,
    Managed,
    External { runnable: bool, writable: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManagedInstallDecision {
    InstallManaged,
    PreserveManaged,
    PreserveExternal,
    SwitchToManagedRequiresConfirmation,
}

pub(crate) fn decide_managed_install(
    installation: ExistingCliInstallation,
) -> ManagedInstallDecision {
    match installation {
        ExistingCliInstallation::Missing => ManagedInstallDecision::InstallManaged,
        ExistingCliInstallation::Managed => ManagedInstallDecision::PreserveManaged,
        ExistingCliInstallation::External {
            runnable: true,
            writable: true,
        } => ManagedInstallDecision::PreserveExternal,
        ExistingCliInstallation::External {
            writable: false, ..
        } => ManagedInstallDecision::SwitchToManagedRequiresConfirmation,
        ExistingCliInstallation::External {
            runnable: false,
            writable: true,
        } => ManagedInstallDecision::InstallManaged,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessEnvironment {
    values: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessSpec {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<OsString>,
    pub(crate) environment: ProcessEnvironment,
    pub(crate) clear_environment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessOutput {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

pub(crate) trait LatestVersionResolver {
    fn resolve_latest(&self, spec: &ProcessSpec) -> Result<String, InstallFailure>;
}

pub(crate) trait ProcessRunner {
    fn run(&self, spec: &ProcessSpec) -> Result<ProcessOutput, InstallFailure>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathState {
    Missing,
    Directory,
    File,
    Indirection,
}

pub(crate) trait ManagedFilesystem {
    fn inspect(&self, path: &Path) -> Result<PathState, InstallFailure>;
    fn create_dir_all(&self, path: &Path) -> Result<(), InstallFailure>;
    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), InstallFailure>;
    fn read(&self, path: &Path) -> Result<Vec<u8>, InstallFailure>;
    fn read_directories(&self, path: &Path) -> Result<Vec<PathBuf>, InstallFailure>;
    fn rename(&self, source: &Path, destination: &Path) -> Result<(), InstallFailure>;
    fn replace(&self, source: &Path, destination: &Path) -> Result<(), InstallFailure>;
    fn remove_tree(&self, path: &Path) -> Result<(), InstallFailure>;
    fn tree_size(&self, path: &Path, allow_contained_links: bool) -> Result<u64, InstallFailure>;
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, InstallFailure>;
    fn make_executable(&self, path: &Path) -> Result<(), InstallFailure>;
}

pub(crate) struct StdManagedFilesystem;

impl ManagedFilesystem for StdManagedFilesystem {
    fn inspect(&self, path: &Path) -> Result<PathState, InstallFailure> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || metadata_is_reparse_point(&metadata) {
                    Ok(PathState::Indirection)
                } else if metadata.is_dir() {
                    Ok(PathState::Directory)
                } else {
                    Ok(PathState::File)
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(PathState::Missing),
            Err(error) => Err(fs_failure(
                InstallStage::InstallingTools,
                &format!("failed to inspect managed path: {error}"),
            )),
        }
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), InstallFailure> {
        std::fs::create_dir_all(path).map_err(|error| {
            fs_failure(
                InstallStage::InstallingTools,
                &format!("failed to create managed directory: {error}"),
            )
        })
    }

    fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), InstallFailure> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| {
                fs_failure(
                    InstallStage::InstallingTools,
                    &format!("failed to create managed file: {error}"),
                )
            })?;
        file.write_all(contents).map_err(|error| {
            fs_failure(
                InstallStage::InstallingTools,
                &format!("failed to write managed file: {error}"),
            )
        })?;
        file.sync_all().map_err(|error| {
            fs_failure(
                InstallStage::InstallingTools,
                &format!("failed to flush managed file: {error}"),
            )
        })
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, InstallFailure> {
        std::fs::read(path).map_err(|error| {
            fs_failure(
                InstallStage::InstallingTools,
                &format!("failed to read managed file: {error}"),
            )
        })
    }

    fn read_directories(&self, path: &Path) -> Result<Vec<PathBuf>, InstallFailure> {
        let mut directories = Vec::new();
        for entry in std::fs::read_dir(path).map_err(|error| {
            fs_failure(
                InstallStage::InstallingTools,
                &format!("failed to list managed directory: {error}"),
            )
        })? {
            let entry = entry.map_err(|error| {
                fs_failure(
                    InstallStage::InstallingTools,
                    &format!("failed to inspect managed directory entry: {error}"),
                )
            })?;
            directories.push(entry.path());
        }
        Ok(directories)
    }

    fn rename(&self, source: &Path, destination: &Path) -> Result<(), InstallFailure> {
        std::fs::rename(source, destination).map_err(|error| {
            fs_failure(
                InstallStage::InstallingTools,
                &format!("failed to finalize managed version: {error}"),
            )
        })
    }

    fn replace(&self, source: &Path, destination: &Path) -> Result<(), InstallFailure> {
        replace_file(source, destination).map_err(|error| {
            fs_failure(
                InstallStage::InstallingTools,
                &format!("failed to activate managed entry point: {error}"),
            )
        })
    }

    fn remove_tree(&self, path: &Path) -> Result<(), InstallFailure> {
        std::fs::remove_dir_all(path).map_err(|error| {
            fs_failure(
                InstallStage::InstallingTools,
                &format!("failed to remove coordinator-owned directory: {error}"),
            )
        })
    }

    fn tree_size(&self, path: &Path, allow_contained_links: bool) -> Result<u64, InstallFailure> {
        tree_size(path, path, allow_contained_links)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, InstallFailure> {
        std::fs::canonicalize(path).map_err(|error| {
            fs_failure(
                InstallStage::Verifying,
                &format!("failed to resolve staged executable: {error}"),
            )
        })
    }

    fn make_executable(&self, path: &Path) -> Result<(), InstallFailure> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(path)
                .map_err(|error| {
                    fs_failure(
                        InstallStage::InstallingTools,
                        &format!("failed to inspect managed entry point: {error}"),
                    )
                })?
                .permissions();
            permissions.set_mode(0o700);
            std::fs::set_permissions(path, permissions).map_err(|error| {
                fs_failure(
                    InstallStage::InstallingTools,
                    &format!("failed to mark managed entry point executable: {error}"),
                )
            })?;
        }
        #[cfg(not(unix))]
        let _ = path;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn metadata_is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(target_os = "windows"))]
fn metadata_is_reparse_point(_: &std::fs::Metadata) -> bool {
    false
}

fn tree_size(root: &Path, path: &Path, allow_contained_links: bool) -> Result<u64, InstallFailure> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        fs_failure(
            InstallStage::InstallingTools,
            &format!("failed to inspect coordinator-owned tree: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink() || metadata_is_reparse_point(&metadata) {
        if !allow_contained_links {
            return Err(fs_failure(
                InstallStage::InstallingTools,
                "managed cache contains filesystem indirection",
            ));
        }
        let target = std::fs::canonicalize(path).map_err(|error| {
            fs_failure(
                InstallStage::InstallingTools,
                &format!("failed to resolve coordinator-owned link: {error}"),
            )
        })?;
        let root = std::fs::canonicalize(root).map_err(|error| {
            fs_failure(
                InstallStage::InstallingTools,
                &format!("failed to resolve coordinator-owned root: {error}"),
            )
        })?;
        if !target.starts_with(root) {
            return Err(fs_failure(
                InstallStage::InstallingTools,
                "coordinator-owned link escapes its managed version directory",
            ));
        }
        return Ok(0);
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Err(fs_failure(
            InstallStage::InstallingTools,
            "coordinator-owned tree contains an unsupported filesystem object",
        ));
    }
    let mut total = 0_u64;
    for entry in std::fs::read_dir(path).map_err(|error| {
        fs_failure(
            InstallStage::InstallingTools,
            &format!("failed to list coordinator-owned tree: {error}"),
        )
    })? {
        let entry = entry.map_err(|error| {
            fs_failure(
                InstallStage::InstallingTools,
                &format!("failed to inspect coordinator-owned entry: {error}"),
            )
        })?;
        total = total
            .checked_add(tree_size(root, &entry.path(), allow_contained_links)?)
            .ok_or_else(|| {
                fs_failure(
                    InstallStage::InstallingTools,
                    "coordinator-owned tree size exceeds u64",
                )
            })?;
    }
    Ok(total)
}

pub(crate) struct StdProcessRunner;

impl ProcessRunner for StdProcessRunner {
    fn run(&self, spec: &ProcessSpec) -> Result<ProcessOutput, InstallFailure> {
        let stage = if spec.args.len() == 1 && spec.args[0] == "--version" {
            InstallStage::Verifying
        } else {
            InstallStage::InstallingTools
        };
        run_process(spec, stage)
    }
}

pub(crate) struct NpmLatestVersionResolver;

impl LatestVersionResolver for NpmLatestVersionResolver {
    fn resolve_latest(&self, spec: &ProcessSpec) -> Result<String, InstallFailure> {
        let output = run_process(spec, InstallStage::InstallingTools)?;
        if output.exit_code != Some(0) {
            return Err(classify_process_failure(
                InstallStage::InstallingTools,
                output.exit_code,
                &output.stdout,
                &output.stderr,
            ));
        }
        let value = output.stdout.trim().trim_matches('"').trim();
        if value.is_empty() {
            return Err(fs_failure(
                InstallStage::InstallingTools,
                "npm did not resolve a latest package version",
            ));
        }
        Ok(value.to_string())
    }
}

fn run_process(spec: &ProcessSpec, stage: InstallStage) -> Result<ProcessOutput, InstallFailure> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    if spec.clear_environment {
        command.env_clear();
    }
    command.envs(spec.environment.values.iter().cloned());
    let output = command.output().map_err(|error| {
        fs_failure(
            stage,
            &format!("failed to start managed npm process: {error}"),
        )
    })?;
    Ok(ProcessOutput {
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[derive(Debug, Clone)]
pub(crate) struct ManagedNpmRequest {
    pub(crate) tool: ManagedNpmTool,
    pub(crate) platform: Platform,
    pub(crate) npm_path: PathBuf,
    pub(crate) node_path: PathBuf,
    pub(crate) managed_root: PathBuf,
    pub(crate) cache_root: PathBuf,
}

pub(crate) fn install_managed_npm_tool(request: ManagedNpmRequest) -> ToolInstallResult {
    install_managed_npm_with(
        &NpmLatestVersionResolver,
        &StdProcessRunner,
        &StdManagedFilesystem,
        request,
    )
}

pub(crate) fn install_managed_npm_with<R, P, F>(
    resolver: &R,
    runner: &P,
    filesystem: &F,
    request: ManagedNpmRequest,
) -> ToolInstallResult
where
    R: LatestVersionResolver,
    P: ProcessRunner,
    F: ManagedFilesystem,
{
    match install_managed_npm(resolver, runner, filesystem, &request) {
        Ok((version, entry_point)) => ToolInstallResult {
            tool: request.tool.tool,
            status: ToolInstallStatus::Succeeded,
            version: Some(version),
            path: Some(entry_point.to_string_lossy().into_owned()),
            shadowed_paths: Vec::new(),
            failure: None,
        },
        Err(error) => ToolInstallResult {
            tool: request.tool.tool,
            status: ToolInstallStatus::Failed,
            version: None,
            path: None,
            shadowed_paths: Vec::new(),
            failure: Some(error),
        },
    }
}

fn install_managed_npm<R, P, F>(
    resolver: &R,
    runner: &P,
    filesystem: &F,
    request: &ManagedNpmRequest,
) -> Result<(String, PathBuf), InstallFailure>
where
    R: LatestVersionResolver,
    P: ProcessRunner,
    F: ManagedFilesystem,
{
    if !matches!(request.platform, Platform::Windows | Platform::Macos) {
        return Err(allowlist_failure(
            "managed npm installation is limited to Windows and macOS",
        ));
    }
    validate_fixed_component(request.tool.identity)?;
    validate_fixed_component(request.tool.executable)?;

    prepare_owned_cache(filesystem, &request.cache_root)?;
    enforce_cache_cap(filesystem, &request.cache_root)?;
    let base_environment = prepare_clean_environment(filesystem, request, None)?;
    let resolution_spec = ProcessSpec {
        program: request.npm_path.clone(),
        args: vec![
            OsString::from("view"),
            OsString::from(request.tool.package),
            OsString::from("version"),
            OsString::from("--json"),
            OsString::from("--cache"),
            request.cache_root.clone().into_os_string(),
        ],
        environment: base_environment.clone(),
        clear_environment: true,
    };
    let version = match resolver.resolve_latest(&resolution_spec) {
        Ok(version) => version,
        Err(error) => {
            return Err(failure_after_cache_cleanup(
                filesystem,
                &request.cache_root,
                error,
            ))
        }
    };
    enforce_cache_cap(filesystem, &request.cache_root)?;
    let base_environment = prepare_clean_environment(filesystem, request, None)?;
    validate_version_component(&version)?;

    let versions_root = request
        .managed_root
        .join("versions")
        .join(request.tool.identity);
    let version_directory = versions_root.join(&version);
    let final_executable = staged_executable(
        &version_directory,
        request.tool.executable,
        &request.platform,
    );
    let entry_point = managed_entry_point(
        &request.managed_root,
        request.tool.executable,
        &request.platform,
    );
    let previous_version = previous_active_version(
        filesystem,
        &entry_point,
        &versions_root,
        request.tool.executable,
        &request.platform,
        &version,
    )?;

    if filesystem.inspect(&version_directory)? == PathState::Directory {
        validate_staged_executable(filesystem, &version_directory, &final_executable)?;
        verify_exact_version(
            runner,
            &final_executable,
            &version,
            prepare_clean_environment(filesystem, request, final_executable.parent())?,
        )?;
    } else {
        if filesystem.inspect(&version_directory)? != PathState::Missing {
            return Err(fs_failure(
                InstallStage::InstallingTools,
                "managed version path is blocked or indirect",
            ));
        }
        let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let staging_directory = request
            .managed_root
            .join("staging")
            .join(request.tool.identity)
            .join(format!("{version}-{}-{sequence}", std::process::id()));
        create_safe_directory(filesystem, &request.managed_root, &staging_directory)?;
        let package_version = format!("{}@{version}", request.tool.package);
        let install_spec = ProcessSpec {
            program: request.npm_path.clone(),
            args: vec![
                OsString::from("install"),
                OsString::from("--global"),
                OsString::from("--no-audit"),
                OsString::from("--no-fund"),
                OsString::from("--prefix"),
                staging_directory.clone().into_os_string(),
                OsString::from("--cache"),
                request.cache_root.clone().into_os_string(),
                OsString::from(package_version),
            ],
            environment: base_environment,
            clear_environment: true,
        };
        let install_output = match runner.run(&install_spec) {
            Ok(output) => output,
            Err(error) => {
                cleanup_failed_staging(filesystem, &staging_directory);
                return Err(failure_after_cache_cleanup(
                    filesystem,
                    &request.cache_root,
                    error,
                ));
            }
        };
        if install_output.exit_code != Some(0) {
            let failure = classify_process_failure(
                InstallStage::InstallingTools,
                install_output.exit_code,
                &install_output.stdout,
                &install_output.stderr,
            );
            cleanup_failed_staging(filesystem, &staging_directory);
            return Err(failure_after_cache_cleanup(
                filesystem,
                &request.cache_root,
                failure,
            ));
        }
        let staging_executable = staged_executable(
            &staging_directory,
            request.tool.executable,
            &request.platform,
        );
        if let Err(error) =
            validate_staged_executable(filesystem, &staging_directory, &staging_executable)
                .and_then(|_| {
                    verify_exact_version(
                        runner,
                        &staging_executable,
                        &version,
                        prepare_clean_environment(
                            filesystem,
                            request,
                            staging_executable.parent(),
                        )?,
                    )
                })
        {
            cleanup_failed_staging(filesystem, &staging_directory);
            return Err(failure_after_cache_cleanup(
                filesystem,
                &request.cache_root,
                error,
            ));
        }
        create_safe_directory(filesystem, &request.managed_root, &versions_root)?;
        ensure_missing_file_boundary(filesystem, &request.managed_root, &version_directory)?;
        filesystem.rename(&staging_directory, &version_directory)?;
    }

    enforce_cache_cap(filesystem, &request.cache_root)?;
    prune_versions(
        filesystem,
        &request.managed_root,
        &versions_root,
        &version,
        previous_version.as_deref(),
    )?;
    activate_entry_point(
        filesystem,
        &request.managed_root,
        &entry_point,
        &final_executable,
        &request.platform,
    )?;
    Ok((version, entry_point))
}

fn prepare_clean_environment<F: ManagedFilesystem>(
    filesystem: &F,
    request: &ManagedNpmRequest,
    tool_directory: Option<&Path>,
) -> Result<ProcessEnvironment, InstallFailure> {
    let environment_root = request.cache_root.join("environment");
    let home = environment_root.join("home");
    let temp = environment_root.join("temp");
    create_safe_directory(filesystem, &request.cache_root, &home)?;
    create_safe_directory(filesystem, &request.cache_root, &temp)?;

    if request.platform == Platform::Windows {
        create_safe_directory(
            filesystem,
            &request.cache_root,
            &home.join("AppData").join("Roaming"),
        )?;
        create_safe_directory(
            filesystem,
            &request.cache_root,
            &home.join("AppData").join("Local"),
        )?;
    }

    clean_environment(
        &request.node_path,
        &request.npm_path,
        tool_directory,
        &request.platform,
        &home,
        &temp,
    )
}

fn clean_environment(
    node_path: &Path,
    npm_path: &Path,
    tool_directory: Option<&Path>,
    platform: &Platform,
    home: &Path,
    temp: &Path,
) -> Result<ProcessEnvironment, InstallFailure> {
    let mut path_entries = Vec::new();
    for path in [node_path, npm_path] {
        let parent = path.parent().ok_or_else(|| {
            fs_failure(
                InstallStage::InstallingTools,
                "approved Node/npm executable has no parent directory",
            )
        })?;
        if !path_entries.iter().any(|entry| entry == parent) {
            path_entries.push(parent.to_path_buf());
        }
    }
    if let Some(tool_directory) = tool_directory {
        if !path_entries.iter().any(|entry| entry == tool_directory) {
            path_entries.insert(0, tool_directory.to_path_buf());
        }
    }
    let path = std::env::join_paths(path_entries).map_err(|error| {
        fs_failure(
            InstallStage::InstallingTools,
            &format!("failed to construct clean process PATH: {error}"),
        )
    })?;
    let mut values = vec![(OsString::from("PATH"), path)];
    match platform {
        Platform::Windows => {
            values.extend([
                (OsString::from("USERPROFILE"), home.as_os_str().to_owned()),
                (OsString::from("HOME"), home.as_os_str().to_owned()),
                (
                    OsString::from("APPDATA"),
                    home.join("AppData").join("Roaming").into_os_string(),
                ),
                (
                    OsString::from("LOCALAPPDATA"),
                    home.join("AppData").join("Local").into_os_string(),
                ),
                (OsString::from("TEMP"), temp.as_os_str().to_owned()),
                (OsString::from("TMP"), temp.as_os_str().to_owned()),
            ]);
        }
        Platform::Macos => values.extend([
            (OsString::from("HOME"), home.as_os_str().to_owned()),
            (OsString::from("TMPDIR"), temp.as_os_str().to_owned()),
        ]),
    }
    if *platform == Platform::Windows {
        for name in ["SYSTEMROOT", "COMSPEC"] {
            if let Some(value) = std::env::var_os(name) {
                values.push((OsString::from(name), value));
            }
        }
    }
    Ok(ProcessEnvironment { values })
}

fn validate_fixed_component(component: &str) -> Result<(), InstallFailure> {
    if component.is_empty()
        || component.len() > 64
        || !component
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(allowlist_failure("invalid allowlisted path component"));
    }
    Ok(())
}

pub(crate) fn validate_version_component(version: &str) -> Result<(), InstallFailure> {
    if version.is_empty()
        || version.len() > 128
        || version == "."
        || version == ".."
        || version.starts_with('.')
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        || !version.bytes().any(|byte| byte.is_ascii_digit())
    {
        return Err(allowlist_failure(
            "resolved npm version is not a safe path component",
        ));
    }
    Ok(())
}

fn prepare_owned_cache<F: ManagedFilesystem>(
    filesystem: &F,
    cache_root: &Path,
) -> Result<(), InstallFailure> {
    match filesystem.inspect(cache_root)? {
        PathState::Missing => {
            create_safe_directory(filesystem, cache_root, cache_root)?;
            write_cache_marker(filesystem, cache_root)
        }
        PathState::Directory => {
            let marker = cache_root.join(CACHE_OWNERSHIP_MARKER);
            if filesystem.inspect(&marker)? != PathState::File
                || filesystem.read(&marker)? != CACHE_OWNERSHIP_CONTENTS
            {
                return Err(fs_failure(
                    InstallStage::InstallingTools,
                    "managed npm cache lacks the coordinator ownership marker",
                ));
            }
            Ok(())
        }
        PathState::File | PathState::Indirection => Err(fs_failure(
            InstallStage::InstallingTools,
            "managed npm cache path is blocked or indirect",
        )),
    }
}

fn write_cache_marker<F: ManagedFilesystem>(
    filesystem: &F,
    cache_root: &Path,
) -> Result<(), InstallFailure> {
    let marker = cache_root.join(CACHE_OWNERSHIP_MARKER);
    ensure_missing_file_boundary(filesystem, cache_root, &marker)?;
    filesystem.write_new(&marker, CACHE_OWNERSHIP_CONTENTS)
}

fn enforce_cache_cap<F: ManagedFilesystem>(
    filesystem: &F,
    cache_root: &Path,
) -> Result<(), InstallFailure> {
    if filesystem.tree_size(cache_root, false)? <= MANAGED_CACHE_CAP_BYTES {
        return Ok(());
    }
    ensure_directory_boundary(filesystem, cache_root, cache_root)?;
    filesystem.remove_tree(cache_root)?;
    create_safe_directory(filesystem, cache_root, cache_root)?;
    write_cache_marker(filesystem, cache_root)
}

fn failure_after_cache_cleanup<F: ManagedFilesystem>(
    filesystem: &F,
    cache_root: &Path,
    primary: InstallFailure,
) -> InstallFailure {
    enforce_cache_cap(filesystem, cache_root)
        .err()
        .unwrap_or(primary)
}

fn create_safe_directory<F: ManagedFilesystem>(
    filesystem: &F,
    boundary_root: &Path,
    target: &Path,
) -> Result<(), InstallFailure> {
    ensure_directory_boundary(filesystem, boundary_root, target)?;
    filesystem.create_dir_all(target)?;
    ensure_directory_boundary(filesystem, boundary_root, target)
}

fn ensure_directory_boundary<F: ManagedFilesystem>(
    filesystem: &F,
    boundary_root: &Path,
    target: &Path,
) -> Result<(), InstallFailure> {
    let relative = target.strip_prefix(boundary_root).map_err(|_| {
        fs_failure(
            InstallStage::InstallingTools,
            "managed path escapes its approved root",
        )
    })?;
    for ancestor in boundary_root
        .ancestors()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        match filesystem.inspect(ancestor)? {
            PathState::Missing | PathState::Directory => {}
            PathState::File | PathState::Indirection => {
                return Err(fs_failure(
                    InstallStage::InstallingTools,
                    "approved managed path has a blocked or indirect ancestor",
                ));
            }
        }
    }
    match filesystem.inspect(boundary_root)? {
        PathState::Missing | PathState::Directory => {}
        PathState::File | PathState::Indirection => {
            return Err(fs_failure(
                InstallStage::InstallingTools,
                "approved managed root is blocked or indirect",
            ));
        }
    }
    let mut current = boundary_root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match filesystem.inspect(&current)? {
            PathState::Missing | PathState::Directory => {}
            PathState::File | PathState::Indirection => {
                return Err(fs_failure(
                    InstallStage::InstallingTools,
                    "managed directory boundary is blocked or indirect",
                ));
            }
        }
    }
    Ok(())
}

fn ensure_missing_file_boundary<F: ManagedFilesystem>(
    filesystem: &F,
    boundary_root: &Path,
    target: &Path,
) -> Result<(), InstallFailure> {
    let parent = target.parent().ok_or_else(|| {
        fs_failure(
            InstallStage::InstallingTools,
            "managed file has no parent directory",
        )
    })?;
    ensure_directory_boundary(filesystem, boundary_root, parent)?;
    if filesystem.inspect(target)? != PathState::Missing {
        return Err(fs_failure(
            InstallStage::InstallingTools,
            "managed file boundary already exists or is indirect",
        ));
    }
    Ok(())
}

fn validate_staged_executable<F: ManagedFilesystem>(
    filesystem: &F,
    stage_root: &Path,
    executable: &Path,
) -> Result<(), InstallFailure> {
    ensure_directory_boundary(
        filesystem,
        stage_root,
        executable.parent().unwrap_or(stage_root),
    )?;
    match filesystem.inspect(executable)? {
        PathState::File => Ok(()),
        PathState::Indirection => {
            let root = filesystem.canonicalize(stage_root)?;
            let target = filesystem.canonicalize(executable)?;
            if target.starts_with(root) {
                Ok(())
            } else {
                Err(fs_failure(
                    InstallStage::Verifying,
                    "staged executable link escapes its managed version directory",
                ))
            }
        }
        PathState::Missing | PathState::Directory => Err(fs_failure(
            InstallStage::Verifying,
            "staged executable is missing or not a file",
        )),
    }
}

fn verify_exact_version<P: ProcessRunner>(
    runner: &P,
    executable: &Path,
    resolved_version: &str,
    environment: ProcessEnvironment,
) -> Result<(), InstallFailure> {
    let output = runner.run(&ProcessSpec {
        program: executable.to_path_buf(),
        args: vec![OsString::from("--version")],
        environment,
        clear_environment: true,
    })?;
    if output.exit_code != Some(0) {
        return Err(classify_process_failure(
            InstallStage::Verifying,
            output.exit_code,
            &output.stdout,
            &output.stderr,
        ));
    }
    let reported = parse_command_version_output(&format!("{}\n{}", output.stdout, output.stderr));
    if reported.as_deref() != Some(resolved_version) {
        return Err(fs_failure(
            InstallStage::Verifying,
            "staged executable version does not match the resolved npm version",
        ));
    }
    Ok(())
}

fn staged_executable(root: &Path, executable: &str, platform: &Platform) -> PathBuf {
    match platform {
        Platform::Windows => root.join(format!("{executable}.cmd")),
        Platform::Macos => root.join("bin").join(executable),
    }
}

fn managed_entry_point(root: &Path, executable: &str, platform: &Platform) -> PathBuf {
    match platform {
        Platform::Windows => root.join("bin").join(format!("{executable}.cmd")),
        Platform::Macos => root.join("bin").join(executable),
    }
}

fn previous_active_version<F: ManagedFilesystem>(
    filesystem: &F,
    entry_point: &Path,
    versions_root: &Path,
    executable: &str,
    platform: &Platform,
    current_version: &str,
) -> Result<Option<String>, InstallFailure> {
    let entries = version_directories(filesystem, versions_root)?;
    if entries.is_empty() {
        return Ok(None);
    }
    if filesystem.inspect(entry_point)? == PathState::File {
        let entry_point_bytes = filesystem.read(entry_point)?;
        let contents = String::from_utf8_lossy(&entry_point_bytes);
        for version in &entries {
            let candidate = staged_executable(&versions_root.join(version), executable, platform);
            if contents.contains(candidate.to_string_lossy().as_ref()) {
                if version == current_version {
                    let mut previous: Vec<_> = entries
                        .iter()
                        .filter(|candidate| *candidate != current_version)
                        .cloned()
                        .collect();
                    previous.sort();
                    return Ok(previous.pop());
                }
                return Ok(Some(version.clone()));
            }
        }
        return Err(fs_failure(
            InstallStage::InstallingTools,
            "managed entry point does not reference a coordinator-owned version",
        ));
    } else if matches!(
        filesystem.inspect(entry_point)?,
        PathState::Directory | PathState::Indirection
    ) {
        return Err(fs_failure(
            InstallStage::InstallingTools,
            "managed entry point is blocked or indirect",
        ));
    }
    let mut candidates: Vec<_> = entries
        .into_iter()
        .filter(|version| version != current_version)
        .collect();
    candidates.sort();
    Ok(candidates.pop())
}

fn version_directories<F: ManagedFilesystem>(
    filesystem: &F,
    versions_root: &Path,
) -> Result<Vec<String>, InstallFailure> {
    match filesystem.inspect(versions_root)? {
        PathState::Missing => return Ok(Vec::new()),
        PathState::Directory => {}
        PathState::File | PathState::Indirection => {
            return Err(fs_failure(
                InstallStage::InstallingTools,
                "managed versions root is blocked or indirect",
            ));
        }
    }
    let mut versions = Vec::new();
    for path in filesystem.read_directories(versions_root)? {
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            return Err(fs_failure(
                InstallStage::InstallingTools,
                "managed version name is not valid UTF-8",
            ));
        };
        validate_version_component(name)?;
        if filesystem.inspect(&path)? != PathState::Directory {
            return Err(fs_failure(
                InstallStage::InstallingTools,
                "managed version entry is blocked or indirect",
            ));
        }
        versions.push(name.to_string());
    }
    Ok(versions)
}

fn prune_versions<F: ManagedFilesystem>(
    filesystem: &F,
    managed_root: &Path,
    versions_root: &Path,
    current_version: &str,
    previous_version: Option<&str>,
) -> Result<(), InstallFailure> {
    for version in version_directories(filesystem, versions_root)? {
        if version == current_version || previous_version == Some(version.as_str()) {
            continue;
        }
        let path = versions_root.join(&version);
        ensure_directory_boundary(filesystem, managed_root, &path)?;
        filesystem.tree_size(&path, true)?;
        filesystem.remove_tree(&path)?;
    }
    Ok(())
}

fn activate_entry_point<F: ManagedFilesystem>(
    filesystem: &F,
    managed_root: &Path,
    entry_point: &Path,
    executable: &Path,
    platform: &Platform,
) -> Result<(), InstallFailure> {
    let parent = entry_point.parent().ok_or_else(|| {
        fs_failure(
            InstallStage::InstallingTools,
            "managed entry point has no parent directory",
        )
    })?;
    create_safe_directory(filesystem, managed_root, parent)?;
    match filesystem.inspect(entry_point)? {
        PathState::Missing | PathState::File => {}
        PathState::Directory | PathState::Indirection => {
            return Err(fs_failure(
                InstallStage::InstallingTools,
                "managed entry point is blocked or indirect",
            ));
        }
    }
    let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{}.{}.{sequence}.tmp",
        entry_point
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("managed-entry"),
        std::process::id()
    ));
    ensure_missing_file_boundary(filesystem, managed_root, &temporary)?;
    let contents = launcher_contents(executable, platform)?;
    filesystem.write_new(&temporary, contents.as_bytes())?;
    if let Err(error) = filesystem.make_executable(&temporary) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = filesystem.replace(&temporary, entry_point) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

fn launcher_contents(executable: &Path, platform: &Platform) -> Result<String, InstallFailure> {
    let executable = executable.to_string_lossy();
    match platform {
        Platform::Windows => {
            if executable.contains(['"', '%', '\r', '\n']) {
                return Err(fs_failure(
                    InstallStage::InstallingTools,
                    "managed Windows executable path cannot be represented safely",
                ));
            }
            Ok(format!("@\"{executable}\" %*\r\n"))
        }
        Platform::Macos => {
            if executable.contains(['\r', '\n']) {
                return Err(fs_failure(
                    InstallStage::InstallingTools,
                    "managed macOS executable path cannot be represented safely",
                ));
            }
            let quoted = executable.replace('\'', "'\\''");
            Ok(format!("#!/bin/sh\nexec '{quoted}' \"$@\"\n"))
        }
    }
}

fn cleanup_failed_staging<F: ManagedFilesystem>(filesystem: &F, staging_directory: &Path) {
    if filesystem.inspect(staging_directory) == Ok(PathState::Directory)
        && filesystem
            .tree_size(staging_directory, true)
            .and_then(|_| filesystem.remove_tree(staging_directory))
            .is_err()
    {
        // Preserve the primary structured install failure. A later confirmed
        // run can inspect and remove only this coordinator-owned staging path.
    }
}

fn allowlist_failure(detail: &str) -> InstallFailure {
    InstallFailure {
        code: InstallFailureCode::ToolInstallFailure,
        stage: InstallStage::InstallingTools,
        exit_code: None,
        retryable: false,
        requires_user_action: true,
        message_key: "installer.failure.managed_npm_allowlist".into(),
        recommended_action: RecommendedAction::ViewDiagnostics,
        detail: Some(detail.into()),
    }
}

fn fs_failure(stage: InstallStage, detail: &str) -> InstallFailure {
    InstallFailure {
        code: if stage == InstallStage::Verifying {
            InstallFailureCode::VerificationFailure
        } else {
            InstallFailureCode::ToolInstallFailure
        },
        stage,
        exit_code: None,
        retryable: false,
        requires_user_action: true,
        message_key: "installer.failure.managed_npm".into(),
        recommended_action: RecommendedAction::ViewDiagnostics,
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(name: &str) -> Self {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("managed-npm-tests")
                .join(format!(
                    "{}-{}-{name}",
                    std::process::id(),
                    TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
                ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct FakeResolver {
        version: String,
        calls: Mutex<Vec<ProcessSpec>>,
    }

    impl LatestVersionResolver for FakeResolver {
        fn resolve_latest(&self, spec: &ProcessSpec) -> Result<String, InstallFailure> {
            self.calls.lock().unwrap().push(spec.clone());
            Ok(self.version.clone())
        }
    }

    struct FakeRunner {
        platform: Platform,
        executable: &'static str,
        reported_version: String,
        materialize: bool,
        specs: Arc<Mutex<Vec<ProcessSpec>>>,
    }

    impl ProcessRunner for FakeRunner {
        fn run(&self, spec: &ProcessSpec) -> Result<ProcessOutput, InstallFailure> {
            self.specs.lock().unwrap().push(spec.clone());
            if spec.args.first() == Some(&OsString::from("install")) {
                if self.materialize {
                    let prefix_index = spec.args.iter().position(|arg| arg == "--prefix").unwrap();
                    let prefix = PathBuf::from(spec.args[prefix_index + 1].clone());
                    let executable = staged_executable(&prefix, self.executable, &self.platform);
                    std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
                    std::fs::write(executable, b"fake executable").unwrap();
                }
                Ok(ProcessOutput {
                    exit_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                })
            } else {
                Ok(ProcessOutput {
                    exit_code: Some(0),
                    stdout: self.reported_version.clone(),
                    stderr: String::new(),
                })
            }
        }
    }

    fn request(root: &TestDirectory, version_tool: ToolId) -> ManagedNpmRequest {
        ManagedNpmRequest {
            tool: ManagedNpmTool::from_tool(version_tool).unwrap(),
            platform: Platform::Windows,
            npm_path: root.0.join("runtime").join("npm.cmd"),
            node_path: root.0.join("runtime").join("node.exe"),
            managed_root: root.0.join("managed"),
            cache_root: root.0.join("cache"),
        }
    }

    fn install(
        root: &TestDirectory,
        version: &str,
        reported_version: &str,
        materialize: bool,
    ) -> (
        ToolInstallResult,
        FakeResolver,
        Arc<Mutex<Vec<ProcessSpec>>>,
    ) {
        let resolver = FakeResolver {
            version: version.into(),
            calls: Mutex::new(Vec::new()),
        };
        let specs = Arc::new(Mutex::new(Vec::new()));
        let runner = FakeRunner {
            platform: Platform::Windows,
            executable: "codex",
            reported_version: reported_version.into(),
            materialize,
            specs: specs.clone(),
        };
        let result = install_managed_npm_with(
            &resolver,
            &runner,
            &StdManagedFilesystem,
            request(root, ToolId::Codex),
        );
        (result, resolver, specs)
    }

    #[test]
    fn rejects_package_and_executable_identities_outside_the_allowlist() {
        assert!(ManagedNpmTool::from_identity("codex").is_ok());
        assert!(ManagedNpmTool::from_identity("../../evil").is_err());
        assert!(ManagedNpmTool::from_identity("arbitrary-package").is_err());
        assert!(ManagedNpmTool::from_tool(ToolId::Hermes).is_err());
    }

    #[test]
    fn resolves_latest_once_and_persists_the_exact_version() {
        let root = TestDirectory::new("single-resolution");
        let (result, resolver, specs) = install(&root, "2.3.4", "2.3.4", true);

        assert_eq!(result.status, ToolInstallStatus::Succeeded);
        assert_eq!(result.version.as_deref(), Some("2.3.4"));
        assert_eq!(resolver.calls.lock().unwrap().len(), 1);
        assert!(root
            .0
            .join("managed/versions/codex/2.3.4/codex.cmd")
            .is_file());
        let specs = specs.lock().unwrap();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[1].args, vec![OsString::from("--version")]);
    }

    #[test]
    fn npm_uses_explicit_prefix_cache_and_fixed_package_argv() {
        let root = TestDirectory::new("argv");
        let (_, resolver, specs) = install(&root, "1.2.3", "1.2.3", true);
        let resolution = resolver.calls.lock().unwrap();
        assert_eq!(resolution[0].program, root.0.join("runtime/npm.cmd"));
        assert_eq!(resolution[0].args[0], "view");
        assert_eq!(resolution[0].args[1], "@openai/codex");
        assert!(resolution[0].args.iter().any(|arg| arg == "--cache"));
        let specs = specs.lock().unwrap();
        let install = &specs[0];
        assert_eq!(install.program, root.0.join("runtime/npm.cmd"));
        assert!(install.args.iter().any(|arg| arg == "--prefix"));
        assert!(install.args.iter().any(|arg| arg == "--cache"));
        assert_eq!(install.args.last().unwrap(), "@openai/codex@1.2.3");
    }

    #[test]
    fn rejects_unsafe_resolved_versions_before_staging_or_npm_install() {
        for version in ["../1.2.3", "1.2.3 --prefix elsewhere", "1/2/3"] {
            let root = TestDirectory::new("unsafe-version");
            let (result, resolver, specs) = install(&root, version, version, true);
            assert_eq!(result.status, ToolInstallStatus::Failed);
            assert_eq!(resolver.calls.lock().unwrap().len(), 1);
            assert!(specs.lock().unwrap().is_empty());
            assert!(!root.0.join("managed/staging").exists());
        }
    }

    #[test]
    fn missing_staged_executable_fails_without_activation() {
        let root = TestDirectory::new("missing-stage");
        let (result, _, _) = install(&root, "1.2.3", "1.2.3", false);
        assert_eq!(result.status, ToolInstallStatus::Failed);
        assert_eq!(
            result.failure.unwrap().code,
            InstallFailureCode::VerificationFailure
        );
        assert!(!root.0.join("managed/bin/codex.cmd").exists());
    }

    #[test]
    fn blocking_file_is_rejected_before_managed_directory_creation() {
        let root = TestDirectory::new("blocking-file");
        std::fs::create_dir_all(root.0.join("managed")).unwrap();
        std::fs::write(root.0.join("managed/staging"), b"block").unwrap();
        let (result, _, specs) = install(&root, "1.2.3", "1.2.3", true);
        assert_eq!(result.status, ToolInstallStatus::Failed);
        assert!(specs.lock().unwrap().is_empty());
        assert!(root.0.join("managed/staging").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_managed_component_is_rejected_before_writes() {
        use std::os::unix::fs::symlink;
        let root = TestDirectory::new("symlink");
        let outside = root.0.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(root.0.join("managed")).unwrap();
        symlink(&outside, root.0.join("managed/staging")).unwrap();
        let (result, _, specs) = install(&root, "1.2.3", "1.2.3", true);
        assert_eq!(result.status, ToolInstallStatus::Failed);
        assert!(specs.lock().unwrap().is_empty());
        assert!(std::fs::read_dir(outside).unwrap().next().is_none());
    }

    #[test]
    fn verification_uses_a_clean_explicit_environment() {
        let root = TestDirectory::new("clean-env");
        let (_, resolver, specs) = install(&root, "1.2.3", "1.2.3", true);
        let isolated_home = root.0.join("cache/environment/home");
        let isolated_temp = root.0.join("cache/environment/temp");
        let resolution = resolver.calls.lock().unwrap();
        let specs = specs.lock().unwrap();
        for spec in [&resolution[0], &specs[0], &specs[1]] {
            assert!(spec.clear_environment);
            assert_eq!(
                environment_value(&spec.environment, "USERPROFILE"),
                Some(isolated_home.as_os_str())
            );
            assert_eq!(
                environment_value(&spec.environment, "HOME"),
                Some(isolated_home.as_os_str())
            );
            assert_eq!(
                environment_value(&spec.environment, "APPDATA"),
                Some(isolated_home.join("AppData/Roaming").as_os_str())
            );
            assert_eq!(
                environment_value(&spec.environment, "LOCALAPPDATA"),
                Some(isolated_home.join("AppData/Local").as_os_str())
            );
            assert_eq!(
                environment_value(&spec.environment, "TEMP"),
                Some(isolated_temp.as_os_str())
            );
            assert_eq!(
                environment_value(&spec.environment, "TMP"),
                Some(isolated_temp.as_os_str())
            );
            assert!(environment_value(&spec.environment, "PATH").is_some());
            assert!(!spec.environment.values.iter().any(|(name, _)| {
                name.to_string_lossy().contains("TOKEN") || name.to_string_lossy().contains("PROXY")
            }));
        }
        assert_eq!(specs[1].args, vec![OsString::from("--version")]);
        assert!(isolated_home.join("AppData/Roaming").is_dir());
        assert!(isolated_home.join("AppData/Local").is_dir());
        assert!(isolated_temp.is_dir());
    }

    #[test]
    fn macos_clean_environment_uses_only_isolated_home_and_temp() {
        let root = TestDirectory::new("macos-clean-env");
        let home = root.0.join("home");
        let temp = root.0.join("temp");
        let environment = clean_environment(
            &root.0.join("runtime/node"),
            &root.0.join("runtime/npm"),
            None,
            &Platform::Macos,
            &home,
            &temp,
        )
        .unwrap();

        assert_eq!(
            environment_value(&environment, "HOME"),
            Some(home.as_os_str())
        );
        assert_eq!(
            environment_value(&environment, "TMPDIR"),
            Some(temp.as_os_str())
        );
        for name in ["APPDATA", "LOCALAPPDATA", "USERPROFILE", "TEMP", "TMP"] {
            assert_eq!(environment_value(&environment, name), None);
        }
    }

    fn environment_value<'a>(environment: &'a ProcessEnvironment, name: &str) -> Option<&'a OsStr> {
        environment
            .values
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.as_os_str())
    }

    #[test]
    fn failed_verification_preserves_the_prior_entry_point() {
        let root = TestDirectory::new("rollback");
        let entry = root.0.join("managed/bin/codex.cmd");
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(&entry, b"prior entry point").unwrap();
        let (result, _, _) = install(&root, "2.0.0", "9.9.9", true);
        assert_eq!(result.status, ToolInstallStatus::Failed);
        assert_eq!(std::fs::read(&entry).unwrap(), b"prior entry point");
    }

    #[test]
    fn successful_verification_atomically_activates_one_entry_point() {
        let root = TestDirectory::new("activation");
        let entry = root.0.join("managed").join("bin").join("codex.cmd");
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(&entry, b"prior entry point").unwrap();
        let (result, _, _) = install(&root, "1.2.3", "1.2.3", true);
        assert_eq!(result.path.as_deref(), entry.to_str());
        let launcher = std::fs::read_to_string(entry).unwrap();
        assert!(launcher.contains("1.2.3"));
        assert!(launcher.contains("codex.cmd"));
        assert_eq!(
            std::fs::read_dir(root.0.join("managed/bin"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn macos_uses_prefix_bin_layout_and_an_atomic_launcher() {
        let root = TestDirectory::new("macos-layout");
        let resolver = FakeResolver {
            version: "1.2.3".into(),
            calls: Mutex::new(Vec::new()),
        };
        let specs = Arc::new(Mutex::new(Vec::new()));
        let runner = FakeRunner {
            platform: Platform::Macos,
            executable: "codex",
            reported_version: "1.2.3".into(),
            materialize: true,
            specs,
        };
        let mut request = request(&root, ToolId::Codex);
        request.platform = Platform::Macos;

        let result = install_managed_npm_with(&resolver, &runner, &StdManagedFilesystem, request);

        assert_eq!(result.status, ToolInstallStatus::Succeeded);
        assert!(root
            .0
            .join("managed/versions/codex/1.2.3/bin/codex")
            .is_file());
        let launcher = std::fs::read_to_string(root.0.join("managed/bin/codex")).unwrap();
        assert!(launcher.starts_with("#!/bin/sh\nexec '"));
        assert!(launcher.ends_with("' \"$@\"\n"));
    }

    #[test]
    fn retention_keeps_only_current_and_previous_active_versions() {
        let root = TestDirectory::new("retention");
        let versions = root.0.join("managed").join("versions").join("codex");
        for version in ["0.9.0", "1.0.0", "1.1.0"] {
            std::fs::create_dir_all(versions.join(version)).unwrap();
        }
        let entry = root.0.join("managed").join("bin").join("codex.cmd");
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        let previous_executable =
            staged_executable(&versions.join("1.0.0"), "codex", &Platform::Windows);
        std::fs::write(
            &entry,
            launcher_contents(&previous_executable, &Platform::Windows).unwrap(),
        )
        .unwrap();
        let (result, _, _) = install(&root, "2.0.0", "2.0.0", true);
        assert_eq!(
            result.status,
            ToolInstallStatus::Succeeded,
            "{:?}",
            result.failure
        );
        assert!(versions.join("2.0.0").is_dir());
        assert!(versions.join("1.0.0").is_dir());
        assert!(!versions.join("0.9.0").exists());
        assert!(!versions.join("1.1.0").exists());
    }

    #[test]
    fn cache_cap_clears_only_the_owned_managed_cache() {
        let root = TestDirectory::new("cache-cap");
        let cache = root.0.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            cache.join(CACHE_OWNERSHIP_MARKER),
            b"Agent Manager managed npm cache\n",
        )
        .unwrap();
        let oversized = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(cache.join("oversized"))
            .unwrap();
        oversized.set_len(MANAGED_CACHE_CAP_BYTES + 1).unwrap();
        drop(oversized);
        let unrelated = root.0.join("user-npm-cache");
        std::fs::create_dir_all(&unrelated).unwrap();
        std::fs::write(unrelated.join("keep"), b"user data").unwrap();
        let (result, _, _) = install(&root, "1.2.3", "1.2.3", true);
        assert_eq!(result.status, ToolInstallStatus::Succeeded);
        assert!(cache.join(CACHE_OWNERSHIP_MARKER).is_file());
        assert!(!cache.join("oversized").exists());
        assert_eq!(std::fs::read(unrelated.join("keep")).unwrap(), b"user data");
    }

    #[test]
    fn cache_without_ownership_marker_is_never_removed() {
        let root = TestDirectory::new("cache-ownership");
        std::fs::create_dir_all(root.0.join("cache")).unwrap();
        std::fs::write(root.0.join("cache/user-data"), b"keep").unwrap();
        let (result, resolver, specs) = install(&root, "1.2.3", "1.2.3", true);
        assert_eq!(result.status, ToolInstallStatus::Failed);
        assert!(resolver.calls.lock().unwrap().is_empty());
        assert!(specs.lock().unwrap().is_empty());
        assert_eq!(
            std::fs::read(root.0.join("cache/user-data")).unwrap(),
            b"keep"
        );
    }

    #[test]
    fn external_installations_have_explicit_preserve_and_switch_decisions() {
        assert_eq!(
            decide_managed_install(ExistingCliInstallation::External {
                runnable: true,
                writable: true,
            }),
            ManagedInstallDecision::PreserveExternal
        );
        assert_eq!(
            decide_managed_install(ExistingCliInstallation::External {
                runnable: true,
                writable: false,
            }),
            ManagedInstallDecision::SwitchToManagedRequiresConfirmation
        );
        assert_eq!(
            decide_managed_install(ExistingCliInstallation::External {
                runnable: false,
                writable: true,
            }),
            ManagedInstallDecision::InstallManaged
        );
        assert_eq!(
            decide_managed_install(ExistingCliInstallation::Managed),
            ManagedInstallDecision::PreserveManaged
        );
        assert_eq!(
            decide_managed_install(ExistingCliInstallation::Missing),
            ManagedInstallDecision::InstallManaged
        );
    }
}
