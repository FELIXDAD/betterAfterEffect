//! Free space available to the current user on the cache volume.
use std::path::Path;

/// The cache directory may be created later. Its closest existing parent is on the same volume.
pub fn available_bytes(dir: &Path) -> Option<u64> {
    query(dir.ancestors().find(|parent| parent.is_dir())?)
}

#[cfg(windows)]
fn query(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let path: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0;
    // SAFETY: path is NUL-terminated and available is writable for one u64.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(available)
}

#[cfg(unix)]
fn query(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut buf = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: the pathname is terminated and buf has room for the statvfs result.
    if unsafe { libc::statvfs(path.as_ptr(), buf.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: statvfs initialized buf on success.
    let stat = unsafe { buf.assume_init() };
    u64::try_from(u128::from(stat.f_bavail) * u128::from(stat.f_frsize)).ok()
}

#[cfg(not(any(windows, unix)))]
fn query(_: &Path) -> Option<u64> {
    None
}
