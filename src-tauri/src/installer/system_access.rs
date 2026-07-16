use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    InstallFailure, InstallFailureCode, InstallStage, PathAccess, PathAccessState,
    RecommendedAction,
};

static PROBE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[cfg(target_os = "windows")]
pub(super) fn available_disk_bytes(path: &Path) -> Result<u64, InstallFailure> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut wide_path: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide_path.contains(&0) {
        return Err(probe_failure(
            "failed to measure available disk space: path contains an interior null",
        ));
    }
    wide_path.push(0);

    let mut available_to_caller = 0_u64;
    // SAFETY: `wide_path` is null-terminated and remains alive for the call;
    // the output pointer refers to an initialized writable `u64`.
    let succeeded = unsafe {
        GetDiskFreeSpaceExW(
            wide_path.as_ptr(),
            &mut available_to_caller,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if succeeded == 0 {
        return Err(probe_failure(&format!(
            "failed to measure available disk space: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(available_to_caller)
}

#[cfg(target_os = "macos")]
pub(super) fn available_disk_bytes(path: &Path) -> Result<u64, InstallFailure> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        probe_failure("failed to measure available disk space: path contains an interior null")
    })?;
    let mut statistics = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is a valid null-terminated C string and `statistics`
    // points to writable storage that is read only after a successful call.
    let succeeded = unsafe { libc::statvfs(path.as_ptr(), statistics.as_mut_ptr()) };
    if succeeded != 0 {
        return Err(probe_failure(&format!(
            "failed to measure available disk space: {}",
            std::io::Error::last_os_error()
        )));
    }
    // SAFETY: `statvfs` returned success and initialized the output structure.
    let statistics = unsafe { statistics.assume_init() };
    let available_blocks = u64::try_from(statistics.f_bavail).map_err(|_| {
        probe_failure("failed to measure available disk space: available blocks exceed u64")
    })?;
    let fragment_size = u64::try_from(statistics.f_frsize).map_err(|_| {
        probe_failure("failed to measure available disk space: fragment size exceeds u64")
    })?;
    available_blocks.checked_mul(fragment_size).ok_or_else(|| {
        probe_failure("failed to measure available disk space: byte count exceeds u64")
    })
}

pub(super) fn directory_writable(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let probe_path = path.join(format!(
        ".agent-manager-write-probe-{}-{timestamp}-{}",
        std::process::id(),
        PROBE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let Ok(file) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe_path)
    else {
        return false;
    };
    drop(file);
    std::fs::remove_file(probe_path).is_ok()
}

pub(super) fn path_access(path: &Path) -> PathAccess {
    path_access_with(path, &directory_writable)
}

enum PathKind {
    Directory,
    Missing,
    Other,
    InspectionBlocked,
}

fn path_kind(path: &Path) -> PathKind {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => PathKind::Directory,
        Ok(_) => PathKind::Other,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::symlink_metadata(path) {
                Ok(_) => PathKind::Other,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => PathKind::Missing,
                Err(_) => PathKind::InspectionBlocked,
            }
        }
        Err(_) => PathKind::InspectionBlocked,
    }
}

fn path_access_with(path: &Path, writable: &dyn Fn(&Path) -> bool) -> PathAccess {
    let path_value = Some(path.to_string_lossy().into_owned());
    match path_kind(path) {
        PathKind::Directory if writable(path) => PathAccess {
            path: path_value,
            state: PathAccessState::Writable,
            detail: None,
        },
        PathKind::Directory => PathAccess {
            path: path_value,
            state: PathAccessState::Blocked,
            detail: Some("existing directory failed the write probe".into()),
        },
        PathKind::Other => PathAccess {
            path: path_value,
            state: PathAccessState::Blocked,
            detail: Some("path exists but is not a directory".into()),
        },
        PathKind::InspectionBlocked => PathAccess {
            path: path_value,
            state: PathAccessState::Blocked,
            detail: Some("path metadata could not be inspected".into()),
        },
        PathKind::Missing => path_access_for_missing(path, path_value, writable),
    }
}

fn path_access_for_missing(
    path: &Path,
    path_value: Option<String>,
    writable: &dyn Fn(&Path) -> bool,
) -> PathAccess {
    let mut ancestor = path.parent();
    while let Some(candidate) = ancestor {
        match path_kind(candidate) {
            PathKind::Directory if writable(candidate) => {
                return PathAccess {
                    path: path_value,
                    state: PathAccessState::NeedsCreation,
                    detail: Some(
                        "target directory is missing and its nearest existing ancestor passed the write probe"
                            .into(),
                    ),
                };
            }
            PathKind::Directory => {
                return PathAccess {
                    path: path_value,
                    state: PathAccessState::Blocked,
                    detail: Some("nearest existing ancestor failed the write probe".into()),
                };
            }
            PathKind::Other => {
                return PathAccess {
                    path: path_value,
                    state: PathAccessState::Blocked,
                    detail: Some("nearest existing ancestor is not a directory".into()),
                };
            }
            PathKind::InspectionBlocked => {
                return PathAccess {
                    path: path_value,
                    state: PathAccessState::Blocked,
                    detail: Some("nearest ancestor metadata could not be inspected".into()),
                };
            }
            PathKind::Missing => ancestor = candidate.parent(),
        }
    }

    PathAccess {
        path: path_value,
        state: PathAccessState::Blocked,
        detail: Some("no existing directory ancestor is available".into()),
    }
}

pub(super) fn nearest_existing_directory(path: &Path) -> Result<PathBuf, InstallFailure> {
    let mut candidate = Some(path);
    while let Some(current) = candidate {
        match path_kind(current) {
            PathKind::Directory => return Ok(current.to_path_buf()),
            PathKind::Missing => candidate = current.parent(),
            PathKind::Other => {
                return Err(probe_failure(
                    "disk measurement path exists but is not a directory",
                ));
            }
            PathKind::InspectionBlocked => {
                return Err(probe_failure(
                    "disk measurement path metadata could not be inspected",
                ));
            }
        }
    }
    Err(probe_failure(
        "failed to locate an existing directory for disk measurement",
    ))
}

fn probe_failure(detail: &str) -> InstallFailure {
    InstallFailure {
        code: InstallFailureCode::InstallerFailure,
        stage: InstallStage::Preflight,
        exit_code: None,
        retryable: false,
        requires_user_action: true,
        message_key: "installer.failure.orchestrator".into(),
        recommended_action: RecommendedAction::ViewDiagnostics,
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installer::PathAccessState;

    #[test]
    fn writability_probe_accepts_a_directory_and_cleans_up() {
        let directory = tempfile::tempdir().unwrap();
        let entries_before = std::fs::read_dir(directory.path()).unwrap().count();

        assert!(directory_writable(directory.path()));

        let entries_after = std::fs::read_dir(directory.path()).unwrap().count();
        assert_eq!(entries_after, entries_before);
    }

    #[test]
    fn writability_probe_rejects_a_file_path() {
        let directory = tempfile::tempdir().unwrap();
        let file_path = directory.path().join("not-a-directory");
        std::fs::write(&file_path, b"fixture").unwrap();

        assert!(!directory_writable(&file_path));
    }

    #[test]
    fn path_access_accepts_a_writable_existing_directory() {
        let directory = tempfile::tempdir().unwrap();
        let entries_before = std::fs::read_dir(directory.path()).unwrap().count();

        let access = path_access(directory.path());

        assert_eq!(access.state, PathAccessState::Writable);
        assert_eq!(access.path.as_deref(), directory.path().to_str());
        assert!(access.detail.is_none());
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            entries_before
        );
    }

    #[test]
    fn path_access_marks_a_missing_child_for_creation() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("parent").join("child");

        let access = path_access(&missing);

        assert_eq!(access.state, PathAccessState::NeedsCreation);
        assert_eq!(access.path.as_deref(), missing.to_str());
        assert_eq!(
            access.detail.as_deref(),
            Some(
                "target directory is missing and its nearest existing ancestor passed the write probe"
            )
        );
        assert!(!missing.exists());
    }

    #[test]
    fn path_access_blocks_a_file_where_a_directory_is_expected() {
        let directory = tempfile::tempdir().unwrap();
        let file_path = directory.path().join("not-a-directory");
        std::fs::write(&file_path, b"fixture").unwrap();

        let access = path_access_with(&file_path, &|_| true);

        assert_eq!(access.state, PathAccessState::Blocked);
        assert_eq!(
            access.detail.as_deref(),
            Some("path exists but is not a directory")
        );
    }

    #[test]
    fn path_access_blocks_a_missing_target_when_ancestor_probe_fails() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("parent").join("child");

        let access = path_access_with(&missing, &|_| false);

        assert_eq!(access.state, PathAccessState::Blocked);
        assert_eq!(
            access.detail.as_deref(),
            Some("nearest existing ancestor failed the write probe")
        );
    }

    #[test]
    fn disk_probe_reports_a_measured_value() {
        let available = available_disk_bytes(&std::env::temp_dir()).unwrap();

        assert!(available > 0);
        assert!(available < u64::MAX);
    }

    #[test]
    fn nearest_existing_directory_walks_only_through_missing_paths() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("parent").join("child");

        assert_eq!(
            nearest_existing_directory(&missing).unwrap(),
            directory.path()
        );
    }

    #[test]
    fn nearest_existing_directory_rejects_a_file_path() {
        let directory = tempfile::tempdir().unwrap();
        let file_path = directory.path().join("not-a-directory");
        std::fs::write(&file_path, b"fixture").unwrap();

        let failure = nearest_existing_directory(&file_path).unwrap_err();

        assert_eq!(failure.code, InstallFailureCode::InstallerFailure);
        assert_eq!(failure.stage, InstallStage::Preflight);
        assert_eq!(
            failure.detail.as_deref(),
            Some("disk measurement path exists but is not a directory")
        );
    }

    #[test]
    fn disk_probe_returns_a_structured_failure_for_a_missing_path() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing");

        let failure = available_disk_bytes(&missing).unwrap_err();

        assert_eq!(failure.code, InstallFailureCode::InstallerFailure);
        assert_eq!(failure.stage, InstallStage::Preflight);
        assert!(failure
            .detail
            .as_deref()
            .is_some_and(|detail| detail.starts_with("failed to measure available disk space:")));
    }
}
