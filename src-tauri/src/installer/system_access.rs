use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{InstallFailure, InstallFailureCode, InstallStage, RecommendedAction};

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
    fn disk_probe_reports_a_measured_value() {
        let available = available_disk_bytes(&std::env::temp_dir()).unwrap();

        assert!(available > 0);
        assert!(available < u64::MAX);
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
