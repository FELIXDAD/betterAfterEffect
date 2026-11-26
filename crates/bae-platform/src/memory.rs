//! Physical memory exposed by the OS, in bytes.

/// Total usable physical memory of this machine.
pub fn total_bytes() -> Option<u64> {
    static TOTAL: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *TOTAL.get_or_init(platform::total_bytes)
}

pub use platform::resident_bytes;

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::System::{
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Threading::GetCurrentProcess,
    };

    pub(super) fn total_bytes() -> Option<u64> {
        let mut status = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        // SAFETY: status is writable and carries its own size as the API requires.
        (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.ullTotalPhys)
    }

    /// Current resident working set of the BAE process.
    pub fn resident_bytes() -> Option<u64> {
        let size = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let mut usage = PROCESS_MEMORY_COUNTERS {
            cb: size,
            ..Default::default()
        };
        // SAFETY: the pseudo-handle denotes this process and usage has size bytes.
        (unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut usage, size) } != 0)
            .then_some(usage.WorkingSetSize as u64)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::mem::MaybeUninit;
    pub(super) fn total_bytes() -> Option<u64> {
        let mut bytes = 0_u64;
        let mut size = size_of::<u64>();
        // SAFETY: hw.memsize returns a u64 and both output pointers are valid.
        let result = unsafe {
            libc::sysctlbyname(
                c"hw.memsize".as_ptr(),
                (&raw mut bytes).cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        (result == 0 && size == size_of::<u64>()).then_some(bytes)
    }

    /// Current resident working set of the BAE process.
    pub fn resident_bytes() -> Option<u64> {
        let mut info = MaybeUninit::<libc::proc_taskinfo>::uninit();
        let size = size_of::<libc::proc_taskinfo>() as libc::c_int;
        // SAFETY: the flavor, buffer type and byte count agree.
        let result = unsafe {
            libc::proc_pidinfo(
                libc::getpid(),
                libc::PROC_PIDTASKINFO,
                0,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        if result != size {
            return None;
        }
        // SAFETY: proc_pidinfo returned the full byte count.
        Some(unsafe { info.assume_init() }.pti_resident_size)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    fn proc_bytes(file: &str, key: &str) -> Option<u64> {
        std::fs::read_to_string(file)
            .ok()?
            .lines()
            .find_map(|line| {
                line.strip_prefix(key)?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()?
                    .checked_mul(1024)
            })
    }

    pub(super) fn total_bytes() -> Option<u64> {
        proc_bytes("/proc/meminfo", "MemTotal:")
    }
    /// Current resident working set of the BAE process.
    pub fn resident_bytes() -> Option<u64> {
        proc_bytes("/proc/self/status", "VmRSS:")
    }
}
