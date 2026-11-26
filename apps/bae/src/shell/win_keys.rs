//! F10 on Windows (#430, #460 retest). Windows sends F10 as a "system" key:
//! winit hands `WM_SYSKEYDOWN` / `WM_SYSKEYUP` on to `DefWindowProc`, which
//! turns the release into `SC_KEYMENU` and enters the native menu bar, so F10
//! (AE: Switch to 3D View A) and Shift+F10 (the Default workspace) never work
//! as shortcuts. AE treats F10 as an ordinary key. The event loop's message
//! hook rewrites F10 pressed without Alt into a plain key message before winit
//! sees it; Alt and Alt+F10 still open the menu bar.

#[cfg(windows)]
use std::ffi::c_void;

const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;
const VK_F10: usize = 0x79;
/// `lParam` bit 29 of a key message: Alt is held.
const ALT_HELD: isize = 1 << 29;

/// The leading fields of Win32's `MSG`.
#[cfg(windows)]
#[repr(C)]
struct MsgHead {
    hwnd: *mut c_void,
    message: u32,
    wparam: usize,
    lparam: isize,
}

/// The plain key message for a system key message, if it is F10 without Alt.
fn plain(message: u32, wparam: usize, lparam: isize) -> Option<u32> {
    if wparam != VK_F10 || lparam & ALT_HELD != 0 {
        return None;
    }
    match message {
        WM_SYSKEYDOWN => Some(WM_KEYDOWN),
        WM_SYSKEYUP => Some(WM_KEYUP),
        _ => None,
    }
}

/// winit's message hook (`EventLoopBuilderExtWindows::with_msg_hook`): never
/// consumes a message, only rewrites F10.
#[cfg(windows)]
pub fn hook(msg: *const c_void) -> bool {
    // SAFETY: winit passes `&mut msg as *mut MSG` of the MSG it is about to
    // translate and dispatch (event_loop.rs, `msg_hook`); MSG is `repr(C)` and
    // starts with these fields; only `message` is written, during this call.
    let m = unsafe { &mut *msg.cast::<MsgHead>().cast_mut() };
    if let Some(message) = plain(m.message, m.wparam, m.lparam) {
        m.message = message;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f10_without_alt_becomes_a_plain_key() {
        assert_eq!(plain(WM_SYSKEYDOWN, VK_F10, 0), Some(WM_KEYDOWN));
        assert_eq!(plain(WM_SYSKEYUP, VK_F10, 0), Some(WM_KEYUP));
        assert_eq!(plain(WM_SYSKEYDOWN, VK_F10, ALT_HELD), None, "Alt+F10");
        assert_eq!(plain(WM_SYSKEYDOWN, 0x12, 0), None, "Alt");
        assert_eq!(plain(WM_KEYDOWN, VK_F10, 0), None);
    }
}
