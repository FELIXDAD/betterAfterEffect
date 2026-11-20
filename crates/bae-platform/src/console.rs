//! Console handling for the Windows GUI-subsystem binary.
//!
//! Release builds of `bae.exe` use the Windows GUI subsystem so double-clicking
//! does not open a console window. When the same binary is started from a terminal
//! (`bae gpu-info`, `bae --headless …`), [`attach_parent_console`] reconnects
//! stdout/stderr to that terminal. On other platforms it is a no-op.

/// Attaches to the parent process's console if this process has no usable stderr.
/// Returns `true` if a console was attached.
pub fn attach_parent_console() -> bool {
    #[cfg(windows)]
    {
        windows_impl::attach()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
mod windows_impl {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE,
    };

    pub(super) fn attach() -> bool {
        // SAFETY: plain Win32 calls without pointers; failure is reported through
        // the return values, which we check.
        unsafe {
            let handle = GetStdHandle(STD_ERROR_HANDLE);
            if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
                // Inherited handles (redirected to a file or pipe): keep them.
                return false;
            }
            AttachConsole(ATTACH_PARENT_PROCESS) != 0
        }
    }
}
