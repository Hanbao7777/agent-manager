use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::{
    parse_command_version_output, InstallFailure, InstallFailureCode, InstallStage,
    RecommendedAction,
};

const BLOCK_START: &[u8] = b"# >>> Agent Manager managed PATH >>>";
const BLOCK_END: &[u8] = b"# <<< Agent Manager managed PATH <<<";
const REG_SZ: u32 = 1;
const REG_EXPAND_SZ: u32 = 2;
#[cfg(target_os = "macos")]
static PROFILE_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PathPersistenceRequest {
    pub(crate) managed_bin: PathBuf,
    pub(crate) approved_runtime_dirs: Vec<PathBuf>,
    pub(crate) managed_executable: PathBuf,
    pub(crate) executable_name: String,
    pub(crate) expected_version: String,
    pub(crate) external_candidates: Vec<PathBuf>,
}

#[cfg(test)]
impl PathPersistenceRequest {
    fn windows_fixture() -> Self {
        Self {
            managed_bin: PathBuf::from(r"C:\managed\bin"),
            approved_runtime_dirs: vec![PathBuf::from(r"C:\runtime")],
            managed_executable: PathBuf::from(r"C:\managed\bin\codex.cmd"),
            executable_name: "codex".into(),
            expected_version: "1.0.0".into(),
            external_candidates: vec![PathBuf::from(r"C:\external\codex.cmd")],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PathProbeRequest {
    pub(crate) managed_bin: PathBuf,
    pub(crate) approved_runtime_dirs: Vec<PathBuf>,
    pub(crate) managed_executable: PathBuf,
    pub(crate) executable_name: String,
    pub(crate) expected_version: String,
    pub(crate) external_candidates: Vec<PathBuf>,
}

impl From<&PathPersistenceRequest> for PathProbeRequest {
    fn from(request: &PathPersistenceRequest) -> Self {
        Self {
            managed_bin: request.managed_bin.clone(),
            approved_runtime_dirs: request.approved_runtime_dirs.clone(),
            managed_executable: request.managed_executable.clone(),
            executable_name: request.executable_name.clone(),
            expected_version: request.expected_version.clone(),
            external_candidates: request.external_candidates.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PathProbeResult {
    pub(crate) selected: PathBuf,
    pub(crate) shadowed: Vec<PathBuf>,
}

pub(crate) trait CleanPathProbe {
    fn probe(&self, request: &PathProbeRequest) -> Result<PathProbeResult, InstallFailure>;
}

pub(crate) struct ProcessPathProbe;

impl CleanPathProbe for ProcessPathProbe {
    fn probe(&self, request: &PathProbeRequest) -> Result<PathProbeResult, InstallFailure> {
        let path = std::env::join_paths(clean_probe_paths(request))
            .map_err(|error| path_failure(&format!("failed to construct clean PATH: {error}")))?;
        let mut command = std::process::Command::new(&request.executable_name);
        command.arg("--version").env_clear().env("PATH", path);
        #[cfg(target_os = "windows")]
        for name in ["SYSTEMROOT", "COMSPEC"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let output = command.output().map_err(|error| {
            path_failure(&format!("failed to start managed executable: {error}"))
        })?;
        if !output.status.success() {
            return Err(path_failure(
                "managed executable failed PATH postcondition verification",
            ));
        }
        let reported = parse_command_version_output(&format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
        if reported.as_deref() != Some(request.expected_version.as_str()) {
            return Err(path_failure(
                "managed executable version failed PATH postcondition verification",
            ));
        }
        let shadowed = request
            .external_candidates
            .iter()
            .filter(|candidate| !paths_equivalent(candidate, &request.managed_executable))
            .cloned()
            .collect();
        Ok(PathProbeResult {
            selected: request.managed_executable.clone(),
            shadowed,
        })
    }
}

fn clean_probe_paths(request: &PathProbeRequest) -> Vec<PathBuf> {
    let mut paths = vec![request.managed_bin.clone()];
    for runtime in &request.approved_runtime_dirs {
        if !paths
            .iter()
            .any(|existing| paths_equivalent(existing, runtime))
        {
            paths.push(runtime.clone());
        }
    }
    paths
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowsRegistryValue {
    pub(crate) bytes: Vec<u8>,
    pub(crate) value_type: u32,
}

impl WindowsRegistryValue {
    pub(crate) fn from_string(value: &str, value_type: u32) -> Self {
        let mut bytes = Vec::with_capacity((value.encode_utf16().count() + 1) * 2);
        for unit in value.encode_utf16().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        Self { bytes, value_type }
    }

    fn to_string(&self) -> Result<String, InstallFailure> {
        if !matches!(self.value_type, REG_SZ | REG_EXPAND_SZ) || self.bytes.len() % 2 != 0 {
            return Err(path_failure(
                "current-user PATH has an unsupported registry representation",
            ));
        }
        let mut units = self
            .bytes
            .chunks_exact(2)
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
            .collect::<Vec<_>>();
        while units.last() == Some(&0) {
            units.pop();
        }
        String::from_utf16(&units)
            .map_err(|_| path_failure("current-user PATH is not valid UTF-16"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowsFailurePoint {
    RegistryWrite,
    ProcessUpdate,
    Broadcast,
}

pub(crate) trait WindowsPathStore {
    fn read_user_path(&self) -> Result<Option<WindowsRegistryValue>, InstallFailure>;
    fn write_user_path(&mut self, value: &WindowsRegistryValue) -> Result<(), InstallFailure>;
    fn delete_user_path(&mut self) -> Result<(), InstallFailure>;
    fn process_path(&self) -> Option<OsString>;
    fn set_process_path(&mut self, value: Option<&OsStr>) -> Result<(), InstallFailure>;
    fn broadcast_environment_change(&mut self) -> Result<(), InstallFailure>;
}

pub(crate) fn persist_windows_path<S: WindowsPathStore, P: CleanPathProbe>(
    store: &mut S,
    probe: &P,
    request: &PathPersistenceRequest,
) -> Result<PathProbeResult, InstallFailure> {
    let prior_registry = store.read_user_path()?;
    let prior_process = store.process_path();
    let prior_text = prior_registry
        .as_ref()
        .map(WindowsRegistryValue::to_string)
        .transpose()?
        .unwrap_or_default();
    let updated_text = prepend_windows_path(&prior_text, &request.managed_bin)?;
    let updated_registry = WindowsRegistryValue::from_string(
        &updated_text,
        prior_registry
            .as_ref()
            .map(|value| value.value_type)
            .unwrap_or(REG_EXPAND_SZ),
    );

    let updated_process = prepend_windows_path(
        &process_path_text(prior_process.as_deref())?,
        &request.managed_bin,
    )?;
    let mutation = (|| {
        store.write_user_path(&updated_registry)?;
        store.set_process_path(Some(OsStr::new(&updated_process)))?;
        store.broadcast_environment_change()?;
        if store.read_user_path()? != Some(updated_registry.clone())
            || store.process_path().as_deref() != Some(OsStr::new(&updated_process))
        {
            return Err(path_failure(
                "PATH persistence postcondition did not match written state",
            ));
        }
        probe.probe(&PathProbeRequest::from(request))
    })();

    match mutation {
        Ok(result) => Ok(result),
        Err(primary) => {
            let rollback =
                restore_windows_state(store, prior_registry.as_ref(), prior_process.as_deref());
            match rollback {
                Ok(()) => Err(primary),
                Err(rollback) => Err(path_failure(&format!(
                    "{}; rollback also failed: {}",
                    primary
                        .detail
                        .as_deref()
                        .unwrap_or("PATH persistence failed"),
                    rollback
                        .detail
                        .as_deref()
                        .unwrap_or("unknown rollback failure")
                ))),
            }
        }
    }
}

fn process_path_text(path: Option<&OsStr>) -> Result<String, InstallFailure> {
    match path {
        Some(path) => path
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| path_failure("running process PATH is not valid Unicode")),
        None => Ok(String::new()),
    }
}

fn restore_windows_state<S: WindowsPathStore>(
    store: &mut S,
    registry: Option<&WindowsRegistryValue>,
    process: Option<&OsStr>,
) -> Result<(), InstallFailure> {
    match registry {
        Some(value) => store.write_user_path(value)?,
        None => store.delete_user_path()?,
    }
    store.set_process_path(process)?;
    store.broadcast_environment_change()?;
    if store.read_user_path()?.as_ref() != registry || store.process_path().as_deref() != process {
        return Err(path_failure(
            "PATH rollback postcondition did not restore exact state",
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn persist_host_path(
    request: &PathPersistenceRequest,
) -> Result<PathProbeResult, InstallFailure> {
    let expected = dirs::data_local_dir()
        .ok_or_else(|| path_failure("current-user local application data is unavailable"))?
        .join("Agent-Manager")
        .join("npm")
        .join("bin");
    if !paths_equivalent(&request.managed_bin, &expected) {
        return Err(path_failure(
            "managed Windows bin is outside the approved root",
        ));
    }
    persist_windows_path(&mut WindowsRegistryStore, &ProcessPathProbe, request)
}

#[cfg(target_os = "windows")]
struct WindowsRegistryStore;

#[cfg(target_os = "windows")]
impl WindowsPathStore for WindowsRegistryStore {
    fn read_user_path(&self) -> Result<Option<WindowsRegistryValue>, InstallFailure> {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let environment = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Environment");
        let environment = match environment {
            Ok(environment) => environment,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(path_failure(&format!(
                    "failed to open current-user environment: {error}"
                )))
            }
        };
        match environment.get_raw_value("Path") {
            Ok(value) => Ok(Some(WindowsRegistryValue {
                bytes: value.bytes,
                value_type: value.vtype as u32,
            })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(path_failure(&format!(
                "failed to read current-user PATH: {error}"
            ))),
        }
    }

    fn write_user_path(&mut self, value: &WindowsRegistryValue) -> Result<(), InstallFailure> {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey, RegValue};
        let (environment, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey("Environment")
            .map_err(|error| {
                path_failure(&format!("failed to open current-user environment: {error}"))
            })?;
        let value_type = match value.value_type {
            REG_SZ => winreg::enums::RegType::REG_SZ,
            REG_EXPAND_SZ => winreg::enums::RegType::REG_EXPAND_SZ,
            _ => {
                return Err(path_failure(
                    "refusing to change unsupported current-user PATH type",
                ))
            }
        };
        environment
            .set_raw_value(
                "Path",
                &RegValue {
                    bytes: value.bytes.clone(),
                    vtype: value_type,
                },
            )
            .map_err(|error| path_failure(&format!("failed to write current-user PATH: {error}")))
    }

    fn delete_user_path(&mut self) -> Result<(), InstallFailure> {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let environment = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Environment", winreg::enums::KEY_SET_VALUE);
        let environment = match environment {
            Ok(environment) => environment,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(path_failure(&format!(
                    "failed to open current-user environment: {error}"
                )))
            }
        };
        match environment.delete_value("Path") {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(path_failure(&format!(
                "failed to restore absent current-user PATH: {error}"
            ))),
        }
    }

    fn process_path(&self) -> Option<OsString> {
        std::env::var_os("PATH")
    }

    fn set_process_path(&mut self, value: Option<&OsStr>) -> Result<(), InstallFailure> {
        match value {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        Ok(())
    }

    fn broadcast_environment_change(&mut self) -> Result<(), InstallFailure> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        };
        let environment: Vec<u16> = "Environment\0".encode_utf16().collect();
        let result = unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                environment.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5_000,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            Err(path_failure(
                "failed to broadcast the current-user environment change",
            ))
        } else {
            Ok(())
        }
    }
}

pub(crate) fn prepend_windows_path(
    prior: &str,
    managed_bin: &Path,
) -> Result<String, InstallFailure> {
    let managed = managed_bin
        .to_str()
        .ok_or_else(|| path_failure("managed Windows bin path is not valid Unicode"))?;
    if managed.contains([';', '\r', '\n']) {
        return Err(path_failure(
            "managed Windows bin path cannot be represented safely",
        ));
    }
    let equivalent = windows_path_key(managed);
    let unrelated = prior
        .split(';')
        .filter(|entry| windows_path_key(entry) != equivalent)
        .collect::<Vec<_>>()
        .join(";");
    if unrelated.is_empty() {
        Ok(managed.to_string())
    } else {
        Ok(format!("{managed};{unrelated}"))
    }
}

fn windows_path_key(path: &str) -> String {
    path.trim_matches('"')
        .replace('/', "\\")
        .trim()
        .trim_end_matches('\\')
        .to_lowercase()
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        windows_path_key(&left.to_string_lossy()) == windows_path_key(&right.to_string_lossy())
    }
    #[cfg(not(target_os = "windows"))]
    {
        left == right
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProfileSnapshot {
    pub(crate) existed: bool,
    pub(crate) contents: Vec<u8>,
    pub(crate) permissions: Option<u32>,
}

pub(crate) trait MacosProfileStore {
    fn read_profile(&self) -> Result<ProfileSnapshot, InstallFailure>;
    fn atomic_replace(
        &mut self,
        contents: &[u8],
        permissions: Option<u32>,
    ) -> Result<(), InstallFailure>;
    fn remove_profile(&mut self) -> Result<(), InstallFailure>;
}

#[cfg(target_os = "macos")]
struct StdMacosProfileStore {
    profile: PathBuf,
}

#[cfg(target_os = "macos")]
impl StdMacosProfileStore {
    fn for_login_shell() -> Result<Self, InstallFailure> {
        let home =
            dirs::home_dir().ok_or_else(|| path_failure("current-user home is unavailable"))?;
        let shell = std::env::var_os("SHELL")
            .ok_or_else(|| path_failure("current shell is unavailable; repair PATH manually"))?;
        Ok(Self {
            profile: login_profile_for_shell(&home, &shell)?,
        })
    }
}

fn login_profile_for_shell(home: &Path, shell: &OsStr) -> Result<PathBuf, InstallFailure> {
    match Path::new(shell).file_name().and_then(OsStr::to_str) {
        Some("zsh") => Ok(home.join(".zprofile")),
        Some("bash") => Ok(home.join(".bash_profile")),
        _ => Err(path_failure(
            "only zsh and bash login profiles can be updated automatically",
        )),
    }
}

#[cfg(target_os = "macos")]
impl MacosProfileStore for StdMacosProfileStore {
    fn read_profile(&self) -> Result<ProfileSnapshot, InstallFailure> {
        match std::fs::symlink_metadata(&self.profile) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(path_failure(
                        "shell profile is indirect or not a regular file",
                    ));
                }
                use std::os::unix::fs::PermissionsExt;
                Ok(ProfileSnapshot {
                    existed: true,
                    contents: std::fs::read(&self.profile).map_err(|error| {
                        path_failure(&format!("failed to read shell profile: {error}"))
                    })?,
                    permissions: Some(metadata.permissions().mode()),
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ProfileSnapshot {
                existed: false,
                contents: Vec::new(),
                permissions: None,
            }),
            Err(error) => Err(path_failure(&format!(
                "failed to inspect shell profile: {error}"
            ))),
        }
    }

    fn atomic_replace(
        &mut self,
        contents: &[u8],
        permissions: Option<u32>,
    ) -> Result<(), InstallFailure> {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let _parent = self
            .profile
            .parent()
            .ok_or_else(|| path_failure("shell profile has no parent"))?;
        let temporary = super::unique_temporary_path(
            &self.profile,
            PROFILE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        );
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| {
                path_failure(&format!("failed to create profile replacement: {error}"))
            })?;
        if let Err(error) = file.write_all(contents).and_then(|_| file.sync_all()) {
            let _ = std::fs::remove_file(&temporary);
            return Err(path_failure(&format!(
                "failed to write profile replacement: {error}"
            )));
        }
        if let Some(mode) = permissions {
            if let Err(error) =
                std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(mode))
            {
                let _ = std::fs::remove_file(&temporary);
                return Err(path_failure(&format!(
                    "failed to preserve shell profile permissions: {error}"
                )));
            }
        }
        std::fs::rename(&temporary, &self.profile).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            path_failure(&format!(
                "failed to replace shell profile atomically: {error}"
            ))
        })
    }

    fn remove_profile(&mut self) -> Result<(), InstallFailure> {
        match std::fs::remove_file(&self.profile) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(path_failure(&format!(
                "failed to remove new shell profile during rollback: {error}"
            ))),
        }
    }
}

pub(crate) fn persist_macos_path<S: MacosProfileStore, P: CleanPathProbe>(
    store: &mut S,
    probe: &P,
    request: &PathPersistenceRequest,
) -> Result<PathProbeResult, InstallFailure> {
    let prior = store.read_profile()?;
    let updated = update_macos_profile(&prior.contents, &request.managed_bin)?;
    let mutation = store
        .atomic_replace(&updated, prior.permissions)
        .and_then(|_| probe.probe(&PathProbeRequest::from(request)));
    match mutation {
        Ok(result) => Ok(result),
        Err(primary) => {
            let rollback = if prior.existed {
                store.atomic_replace(&prior.contents, prior.permissions)
            } else {
                store.remove_profile()
            };
            rollback.map_err(|rollback| {
                path_failure(&format!(
                    "{}; rollback also failed: {}",
                    primary
                        .detail
                        .as_deref()
                        .unwrap_or("PATH persistence failed"),
                    rollback
                        .detail
                        .as_deref()
                        .unwrap_or("unknown rollback failure")
                ))
            })?;
            Err(primary)
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn persist_host_path(
    request: &PathPersistenceRequest,
) -> Result<PathProbeResult, InstallFailure> {
    persist_macos_path(
        &mut StdMacosProfileStore::for_login_shell()?,
        &ProcessPathProbe,
        request,
    )
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) fn persist_host_path(
    _: &PathPersistenceRequest,
) -> Result<PathProbeResult, InstallFailure> {
    Err(path_failure(
        "PATH persistence is limited to Windows and macOS",
    ))
}

pub(crate) fn update_macos_profile(
    original: &[u8],
    managed_bin: &Path,
) -> Result<Vec<u8>, InstallFailure> {
    let line_ending = if original.windows(2).any(|bytes| bytes == b"\r\n") {
        b"\r\n".as_slice()
    } else {
        b"\n".as_slice()
    };
    let block = canonical_macos_block(original, managed_bin)?;

    match managed_block_range(original, &block)? {
        Some((start, end)) => {
            let mut updated = Vec::with_capacity(original.len() - (end - start) + block.len());
            updated.extend_from_slice(&original[..start]);
            updated.extend_from_slice(&block);
            updated.extend_from_slice(&original[end..]);
            Ok(updated)
        }
        None => {
            let had_final_newline = original.ends_with(b"\n");
            let mut updated = original.to_vec();
            if !updated.is_empty() && !had_final_newline {
                updated.extend_from_slice(line_ending);
            }
            updated.extend_from_slice(&block);
            if had_final_newline || original.is_empty() {
                updated.extend_from_slice(line_ending);
            }
            Ok(updated)
        }
    }
}

fn managed_block_range(
    contents: &[u8],
    canonical_block: &[u8],
) -> Result<Option<(usize, usize)>, InstallFailure> {
    for line in contents.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line
            .windows(b"Agent Manager managed PATH".len())
            .any(|candidate| candidate == b"Agent Manager managed PATH")
            && line != BLOCK_START
            && line != BLOCK_END
        {
            return Err(path_failure(
                "managed PATH block markers are malformed, duplicated, or ambiguous",
            ));
        }
    }
    let starts = find_all(contents, BLOCK_START);
    let ends = find_all(contents, BLOCK_END);
    match (starts.as_slice(), ends.as_slice()) {
        ([], []) => Ok(None),
        ([start], [end]) if start < end => {
            if (*start != 0 && contents[*start - 1] != b'\n')
                || (*end + BLOCK_END.len() != contents.len()
                    && contents[*end + BLOCK_END.len()] != b'\r'
                    && contents[*end + BLOCK_END.len()] != b'\n')
            {
                return Err(path_failure(
                    "managed PATH block markers are malformed, duplicated, or ambiguous",
                ));
            }
            let after_end = end + BLOCK_END.len();
            if contents.get(*start..after_end) != Some(canonical_block) {
                return Err(path_failure(
                    "managed PATH block is unexpected or has been modified; repair it manually",
                ));
            }
            Ok(Some((*start, after_end)))
        }
        _ => Err(path_failure(
            "managed PATH block markers are malformed, duplicated, or ambiguous",
        )),
    }
}

fn count_managed_blocks(contents: &[u8], managed_bin: &Path) -> Result<usize, InstallFailure> {
    let block = canonical_macos_block(contents, managed_bin)?;
    Ok(usize::from(
        managed_block_range(contents, &block)?.is_some(),
    ))
}

fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    haystack
        .windows(needle.len())
        .enumerate()
        .filter_map(|(index, candidate)| (candidate == needle).then_some(index))
        .collect()
}

pub(crate) fn quote_shell_path(path: &Path) -> Result<String, InstallFailure> {
    let path = path
        .to_str()
        .ok_or_else(|| path_failure("managed macOS bin path is not valid UTF-8"))?;
    let home = dirs::home_dir().ok_or_else(|| path_failure("current-user home is unavailable"))?;
    let approved = home.join(".agent-manager").join("npm").join("bin");
    if path != approved.to_string_lossy()
        || !Path::new(path).is_absolute()
        || path.contains(['\r', '\n', '\0'])
    {
        return Err(path_failure(
            "managed macOS bin path cannot be represented safely",
        ));
    }
    Ok(format!("'{}'", path.replace('\'', "'\\''")))
}

fn canonical_macos_block(original: &[u8], managed_bin: &Path) -> Result<Vec<u8>, InstallFailure> {
    let line_ending = if original.windows(2).any(|bytes| bytes == b"\r\n") {
        b"\r\n".as_slice()
    } else {
        b"\n".as_slice()
    };
    let mut block = Vec::new();
    block.extend_from_slice(BLOCK_START);
    block.extend_from_slice(line_ending);
    block.extend_from_slice(
        format!("export PATH={}:\"$PATH\"", quote_shell_path(managed_bin)?).as_bytes(),
    );
    block.extend_from_slice(line_ending);
    block.extend_from_slice(BLOCK_END);
    Ok(block)
}

pub(crate) fn path_failure(detail: &str) -> InstallFailure {
    InstallFailure {
        code: InstallFailureCode::PathNotVisible,
        stage: InstallStage::Verifying,
        exit_code: None,
        retryable: false,
        requires_user_action: true,
        message_key: "installer.failure.path_persistence".into(),
        recommended_action: RecommendedAction::ViewDiagnostics,
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn windows_path_prepends_once_and_preserves_unrelated_entries() {
        let prior = r"C:\Tools;;c:/users/me/agent-manager/npm/bin\;C:\Other ";
        let managed = Path::new(r"C:\Users\Me\Agent-Manager\npm\bin");

        assert_eq!(
            prepend_windows_path(prior, managed).unwrap(),
            r"C:\Users\Me\Agent-Manager\npm\bin;C:\Tools;;C:\Other "
        );
    }

    #[test]
    fn clean_probe_path_keeps_managed_bin_first_and_only_approved_runtime_dirs() {
        let request = PathProbeRequest {
            managed_bin: PathBuf::from("managed/bin"),
            approved_runtime_dirs: vec![
                PathBuf::from("runtime/node"),
                PathBuf::from("runtime/npm"),
                PathBuf::from("runtime/node"),
            ],
            managed_executable: PathBuf::from("managed/bin/codex"),
            executable_name: "codex".into(),
            expected_version: "1.0.0".into(),
            external_candidates: vec![PathBuf::from("ambient/codex")],
        };
        assert_eq!(
            clean_probe_paths(&request),
            vec![
                PathBuf::from("managed/bin"),
                PathBuf::from("runtime/node"),
                PathBuf::from("runtime/npm"),
            ]
        );
    }

    #[test]
    fn macos_profile_append_and_replace_preserve_unrelated_bytes() {
        let managed = dirs::home_dir().unwrap().join(".agent-manager/npm/bin");
        let original = b"export EDITOR=vim\r\n";
        let appended = update_macos_profile(original, &managed).unwrap();
        assert!(appended.starts_with(original));
        assert!(String::from_utf8(appended.clone())
            .unwrap()
            .contains("export PATH='"));

        let replaced = update_macos_profile(&appended, &managed).unwrap();
        assert!(replaced.starts_with(original));
        assert_eq!(count_managed_blocks(&replaced, &managed).unwrap(), 1);
    }

    #[test]
    fn macos_profile_rejects_ambiguous_markers_without_output() {
        for contents in [
            b"# >>> Agent Manager managed PATH >>>\n".as_slice(),
            b"# <<< Agent Manager managed PATH <<<\n".as_slice(),
            b"# >>> Agent Manager managed PATH >>>\n# >>> Agent Manager managed PATH >>>\n# <<< Agent Manager managed PATH <<<\n# <<< Agent Manager managed PATH <<<\n".as_slice(),
        ] {
            assert!(update_macos_profile(contents, &dirs::home_dir().unwrap().join(".agent-manager/npm/bin")).is_err());
        }
    }

    #[test]
    fn shell_path_quoting_does_not_interpolate_metacharacters() {
        assert!(quote_shell_path(Path::new("/Users/o'hare/$HOME;$(id)/bin")).is_err());
    }

    #[test]
    fn macos_managed_root_is_the_only_quotable_path() {
        let managed = dirs::home_dir().unwrap().join(".agent-manager/npm/bin");
        assert!(quote_shell_path(&managed).is_ok());
        assert!(quote_shell_path(&managed.parent().unwrap().join("other-bin")).is_err());
    }

    #[test]
    fn login_shell_selection_supports_zsh_and_bash_only() {
        let home = Path::new("/Users/tester");
        assert_eq!(
            login_profile_for_shell(home, OsStr::new("/bin/zsh")).unwrap(),
            home.join(".zprofile")
        );
        assert_eq!(
            login_profile_for_shell(home, OsStr::new("/bin/bash")).unwrap(),
            home.join(".bash_profile")
        );
        assert!(login_profile_for_shell(home, OsStr::new("/bin/fish")).is_err());
    }

    #[test]
    fn macos_profile_rejects_tampered_nested_reversed_and_duplicate_blocks() {
        let managed = dirs::home_dir().unwrap().join(".agent-manager/npm/bin");
        let canonical = update_macos_profile(b"export EDITOR=vim\r\n", &managed).unwrap();
        let tampered = String::from_utf8(canonical.clone())
            .unwrap()
            .replace("export PATH=", "export PATH=/tmp:");
        for contents in [
            tampered.as_bytes(),
            b"# <<< Agent Manager managed PATH <<<\n# >>> Agent Manager managed PATH >>>\n".as_slice(),
            b"# >>> Agent Manager managed PATH >>>\n# >>> Agent Manager managed PATH >>>\n# <<< Agent Manager managed PATH <<<\n# <<< Agent Manager managed PATH <<<\n".as_slice(),
        ] {
            assert!(update_macos_profile(contents, &managed).is_err());
        }
    }

    #[test]
    fn malformed_macos_block_stops_before_profile_write() {
        let managed = dirs::home_dir().unwrap().join(".agent-manager/npm/bin");
        let request = PathPersistenceRequest {
            managed_bin: managed.clone(),
            approved_runtime_dirs: Vec::new(),
            managed_executable: managed.join("codex"),
            executable_name: "codex".into(),
            expected_version: "1.0.0".into(),
            external_candidates: Vec::new(),
        };
        let mut store = RecordingMacosStore {
            snapshot: ProfileSnapshot {
                existed: true,
                contents: b"# >>> Agent Manager managed PATH >>>\nexport PATH=/tmp:$PATH\n# <<< Agent Manager managed PATH <<<\n".to_vec(),
                permissions: Some(0o600),
            },
            ..Default::default()
        };
        assert!(persist_macos_path(&mut store, &Probe { fail: false }, &request).is_err());
        assert!(store.replacements.is_empty());
    }

    #[test]
    fn macos_profile_preserves_crlf_and_final_newline_state() {
        let managed = dirs::home_dir().unwrap().join(".agent-manager/npm/bin");
        let crlf = update_macos_profile(b"export EDITOR=vim\r\n", &managed).unwrap();
        assert!(crlf.windows(2).any(|pair| pair == b"\r\n"));
        assert!(crlf.ends_with(b"\r\n"));
        assert_eq!(update_macos_profile(&crlf, &managed).unwrap(), crlf);

        let no_final = update_macos_profile(b"export EDITOR=vim", &managed).unwrap();
        assert!(!no_final.ends_with(b"\n"));
    }

    #[derive(Default)]
    struct RecordingWindowsStore {
        registry: Option<WindowsRegistryValue>,
        process: Option<String>,
        broadcasts: usize,
        fail: Option<WindowsFailurePoint>,
    }

    impl WindowsPathStore for RecordingWindowsStore {
        fn read_user_path(&self) -> Result<Option<WindowsRegistryValue>, InstallFailure> {
            Ok(self.registry.clone())
        }
        fn write_user_path(&mut self, value: &WindowsRegistryValue) -> Result<(), InstallFailure> {
            if self.fail == Some(WindowsFailurePoint::RegistryWrite) {
                return Err(path_failure("write"));
            }
            self.registry = Some(value.clone());
            Ok(())
        }
        fn delete_user_path(&mut self) -> Result<(), InstallFailure> {
            self.registry = None;
            Ok(())
        }
        fn process_path(&self) -> Option<OsString> {
            self.process.clone().map(OsString::from)
        }
        fn set_process_path(&mut self, value: Option<&OsStr>) -> Result<(), InstallFailure> {
            if self.fail == Some(WindowsFailurePoint::ProcessUpdate) {
                self.fail = None;
                return Err(path_failure("process"));
            }
            self.process = value.map(|value| value.to_string_lossy().into_owned());
            Ok(())
        }
        fn broadcast_environment_change(&mut self) -> Result<(), InstallFailure> {
            if self.fail == Some(WindowsFailurePoint::Broadcast) {
                self.fail = None;
                return Err(path_failure("broadcast"));
            }
            self.broadcasts += 1;
            Ok(())
        }
    }

    struct Probe {
        fail: bool,
    }
    impl CleanPathProbe for Probe {
        fn probe(&self, request: &PathProbeRequest) -> Result<PathProbeResult, InstallFailure> {
            if self.fail {
                return Err(path_failure("probe"));
            }
            Ok(PathProbeResult {
                selected: request.managed_executable.clone(),
                shadowed: vec![PathBuf::from(r"C:\external\codex.cmd")],
            })
        }
    }

    fn registry_value(value: &str, value_type: u32) -> WindowsRegistryValue {
        WindowsRegistryValue::from_string(value, value_type)
    }

    #[test]
    fn windows_persistence_preserves_type_refreshes_and_reports_shadowing() {
        let prior = registry_value(r"C:\Tools", 2);
        let mut store = RecordingWindowsStore {
            registry: Some(prior),
            process: Some(r"C:\Tools".into()),
            ..Default::default()
        };
        let result = persist_windows_path(
            &mut store,
            &Probe { fail: false },
            &PathPersistenceRequest::windows_fixture(),
        )
        .unwrap();
        assert_eq!(store.registry.as_ref().unwrap().value_type, 2);
        assert_eq!(store.process.as_deref(), Some(r"C:\managed\bin;C:\Tools"));
        assert_eq!(store.broadcasts, 1);
        assert_eq!(
            result.shadowed,
            vec![PathBuf::from(r"C:\external\codex.cmd")]
        );
    }

    #[test]
    fn windows_failures_restore_exact_registry_and_process_state() {
        for failure_point in [
            WindowsFailurePoint::RegistryWrite,
            WindowsFailurePoint::ProcessUpdate,
            WindowsFailurePoint::Broadcast,
        ] {
            let prior = registry_value(r"C:\Tools;;C:\Other ", 1);
            let mut store = RecordingWindowsStore {
                registry: Some(prior.clone()),
                process: None,
                fail: Some(failure_point),
                ..Default::default()
            };
            assert!(persist_windows_path(
                &mut store,
                &Probe { fail: false },
                &PathPersistenceRequest::windows_fixture()
            )
            .is_err());
            assert_eq!(store.registry, Some(prior));
            assert_eq!(store.process, None);
        }

        let prior = registry_value(r"C:\Tools", 2);
        let mut store = RecordingWindowsStore {
            registry: Some(prior.clone()),
            process: Some("process-only".into()),
            ..Default::default()
        };
        assert!(persist_windows_path(
            &mut store,
            &Probe { fail: true },
            &PathPersistenceRequest::windows_fixture()
        )
        .is_err());
        assert_eq!(store.registry, Some(prior));
        assert_eq!(store.process.as_deref(), Some("process-only"));
    }

    #[test]
    fn windows_absent_path_uses_expandable_type_and_restores_absence() {
        let mut store = RecordingWindowsStore {
            process: Some(r"C:\Tools".into()),
            fail: Some(WindowsFailurePoint::Broadcast),
            ..Default::default()
        };

        assert!(persist_windows_path(
            &mut store,
            &Probe { fail: false },
            &PathPersistenceRequest::windows_fixture(),
        )
        .is_err());
        assert_eq!(store.registry, None);
        assert_eq!(store.process.as_deref(), Some(r"C:\Tools"));
    }

    #[cfg(unix)]
    #[test]
    fn windows_non_unicode_process_path_stops_before_registry_mutation() {
        use std::os::unix::ffi::OsStringExt;
        struct NonUnicodeStore {
            registry: WindowsRegistryValue,
            writes: usize,
        }
        impl WindowsPathStore for NonUnicodeStore {
            fn read_user_path(&self) -> Result<Option<WindowsRegistryValue>, InstallFailure> {
                Ok(Some(self.registry.clone()))
            }
            fn write_user_path(&mut self, _: &WindowsRegistryValue) -> Result<(), InstallFailure> {
                self.writes += 1;
                Ok(())
            }
            fn delete_user_path(&mut self) -> Result<(), InstallFailure> {
                self.writes += 1;
                Ok(())
            }
            fn process_path(&self) -> Option<OsString> {
                Some(OsString::from_vec(vec![0xff]))
            }
            fn set_process_path(&mut self, _: Option<&OsStr>) -> Result<(), InstallFailure> {
                self.writes += 1;
                Ok(())
            }
            fn broadcast_environment_change(&mut self) -> Result<(), InstallFailure> {
                self.writes += 1;
                Ok(())
            }
        }
        let prior = registry_value(r"C:\Tools", REG_SZ);
        let mut store = NonUnicodeStore {
            registry: prior.clone(),
            writes: 0,
        };
        assert!(persist_windows_path(
            &mut store,
            &Probe { fail: false },
            &PathPersistenceRequest::windows_fixture()
        )
        .is_err());
        assert_eq!(store.registry, prior);
        assert_eq!(store.writes, 0);
    }

    struct RecordingMacosStore {
        snapshot: ProfileSnapshot,
        replacements: Vec<(Vec<u8>, Option<u32>)>,
        removed: usize,
        fail_replace: bool,
    }

    impl Default for RecordingMacosStore {
        fn default() -> Self {
            Self {
                snapshot: ProfileSnapshot {
                    existed: false,
                    contents: Vec::new(),
                    permissions: None,
                },
                replacements: Vec::new(),
                removed: 0,
                fail_replace: false,
            }
        }
    }

    impl MacosProfileStore for RecordingMacosStore {
        fn read_profile(&self) -> Result<ProfileSnapshot, InstallFailure> {
            Ok(self.snapshot.clone())
        }

        fn atomic_replace(
            &mut self,
            contents: &[u8],
            permissions: Option<u32>,
        ) -> Result<(), InstallFailure> {
            if self.fail_replace {
                return Err(path_failure("replace"));
            }
            self.replacements.push((contents.to_vec(), permissions));
            Ok(())
        }

        fn remove_profile(&mut self) -> Result<(), InstallFailure> {
            self.removed += 1;
            Ok(())
        }
    }

    #[test]
    fn macos_postcondition_failure_rolls_back_existing_profile_bytes_and_permissions() {
        let managed = dirs::home_dir().unwrap().join(".agent-manager/npm/bin");
        let request = PathPersistenceRequest {
            managed_bin: managed.clone(),
            approved_runtime_dirs: Vec::new(),
            managed_executable: managed.join("codex"),
            executable_name: "codex".into(),
            expected_version: "1.0.0".into(),
            external_candidates: Vec::new(),
        };
        let prior = ProfileSnapshot {
            existed: true,
            contents: b"export EDITOR=vim\r\n".to_vec(),
            permissions: Some(0o640),
        };
        let mut store = RecordingMacosStore {
            snapshot: prior.clone(),
            ..Default::default()
        };

        assert!(persist_macos_path(&mut store, &Probe { fail: true }, &request).is_err());
        assert_eq!(store.replacements.len(), 2);
        assert_eq!(store.replacements[1], (prior.contents, prior.permissions));
        assert_eq!(store.removed, 0);
    }

    #[test]
    fn macos_missing_profile_is_removed_when_postcondition_fails() {
        let managed = dirs::home_dir().unwrap().join(".agent-manager/npm/bin");
        let request = PathPersistenceRequest {
            managed_bin: managed.clone(),
            approved_runtime_dirs: Vec::new(),
            managed_executable: managed.join("codex"),
            executable_name: "codex".into(),
            expected_version: "1.0.0".into(),
            external_candidates: Vec::new(),
        };
        let mut store = RecordingMacosStore::default();

        assert!(persist_macos_path(&mut store, &Probe { fail: true }, &request).is_err());
        assert_eq!(store.removed, 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn std_macos_store_replaces_in_same_directory_and_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join(".zprofile");
        std::fs::write(&profile, b"old").unwrap();
        std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o640)).unwrap();
        let mut store = StdMacosProfileStore {
            profile: profile.clone(),
        };

        store.atomic_replace(b"new", Some(0o640)).unwrap();

        assert_eq!(std::fs::read(&profile).unwrap(), b"new");
        assert_eq!(
            std::fs::metadata(&profile).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".zprofile.")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn std_macos_store_cleans_temporary_file_after_rename_failure() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join(".bash_profile");
        std::fs::create_dir(&profile).unwrap();
        let mut store = StdMacosProfileStore { profile };

        assert!(store.atomic_replace(b"new", None).is_err());
        assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".bash_profile.")));
    }
}
