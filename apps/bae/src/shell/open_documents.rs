//! Projects opened from Finder on macOS: double-clicking a `.bae` / `.baez`, dropping one
//! on the Dock icon, `open project.bae`.
//!
//! macOS delivers these as an Apple event, which `NSApplication` hands to its delegate's
//! `application:openURLs:`, not as command-line arguments. winit 0.30's delegate class
//! (`WinitApplicationDelegate`) does not implement that method, so [`install`] adds it to
//! the class at run time (winit replaces `sendEvent:` on `NSApplication` the same way).
//! The paths are handed to a sink; the shell turns them into the normal
//! `file.open_project` command, so the unsaved-changes question and error reporting are
//! the same as for File > Open. Works for the launch event too: the method is in place
//! before `run_app` starts the application.

use std::ffi::{CStr, c_char};
use std::path::PathBuf;
use std::sync::Mutex;

use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::sel;
use objc2_foundation::{NSArray, NSURL};

/// winit 0.30's `NSApplicationDelegate` subclass (`platform_impl/macos/app_state.rs`).
const WINIT_DELEGATE: &CStr = c"WinitApplicationDelegate";
/// `-(void)application:(NSApplication *)app openURLs:(NSArray<NSURL *> *)urls`.
const OPEN_URLS_TYPES: &CStr = c"v@:@@";

type Sink = Box<dyn Fn(Vec<PathBuf>) + Send>;

/// Where opened paths go. Set once by [`install`]; a `Mutex` so a test can swap it.
static SINK: Mutex<Option<Sink>> = Mutex::new(None);

/// Starts delivering Finder "open" requests to `sink`. Call after winit's event loop is
/// built (that registers the delegate class) and before `run_app`. Returns whether the
/// handler is in place.
pub fn install(sink: impl Fn(Vec<PathBuf>) + Send + 'static) -> bool {
    set_sink(Box::new(sink));
    let Some(class) = AnyClass::get(WINIT_DELEGATE) else {
        tracing::warn!("opening projects from Finder is unavailable: no winit app delegate class");
        return false;
    };
    if add_open_urls(class) {
        true
    } else {
        // Already there (a newer winit implementing it itself): leave it alone.
        tracing::warn!("WinitApplicationDelegate already handles application:openURLs:");
        false
    }
}

/// Open the first project in a Finder request, or import the entire footage selection.
pub fn invocation(paths: &[PathBuf]) -> Option<bae_ui::Invocation> {
    if let Some(project) = paths.iter().find(|p| {
        p.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bae") || ext.eq_ignore_ascii_case("baez"))
            || p.file_name().is_some_and(|n| n == "project.json")
    }) {
        super::open_files_invocation(std::slice::from_ref(project))
    } else if paths.is_empty() {
        None
    } else {
        Some(bae_ui::Invocation::with_params(
            bae_ui_motion::actions::ids::IMPORT_FILE,
            serde_json::json!({"paths": paths}),
        ))
    }
}

/// Winit reports paths but no cursor motion while Finder owns a drag. Convert
/// the current location from AppKit window coordinates to the flipped winit view.
pub fn pointer_in_view(window: &winit::window::Window) -> Option<egui::Pos2> {
    use objc2::{msg_send, runtime::AnyObject};
    use objc2_foundation::NSPoint;
    use winit::raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
    let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    let view = handle.ns_view.as_ptr().cast::<AnyObject>();
    // SAFETY: winit owns the NSView and NSWindow for the duration of this event.
    unsafe {
        let native: *mut AnyObject = msg_send![view, window];
        if native.is_null() {
            return None;
        }
        let pos: NSPoint = msg_send![native, mouseLocationOutsideOfEventStream];
        let local: NSPoint =
            msg_send![view, convertPoint: pos, fromView: std::ptr::null_mut::<AnyObject>()];
        Some(egui::pos2(local.x as f32, local.y as f32))
    }
}

/// Publish a successful open or save to the system recent-documents list (Dock).
pub fn note_recent(path: &std::path::Path) {
    use objc2::{class, msg_send};
    if let Some(url) = NSURL::from_file_path(path) {
        // SAFETY: the desktop shell calls this on the main thread with a live file URL.
        unsafe {
            let controller: *mut AnyObject =
                msg_send![class!(NSDocumentController), sharedDocumentController];
            let (): () = msg_send![controller, noteNewRecentDocumentURL: &*url];
        }
    }
}

fn set_sink(sink: Sink) {
    *SINK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sink);
}

/// Adds `application:openURLs:` to `class`; `false` when the class already has it.
fn add_open_urls(class: &AnyClass) -> bool {
    let imp = application_open_urls as unsafe extern "C-unwind" fn(_, _, _, _);
    // SAFETY: `imp` has the C signature `OPEN_URLS_TYPES` describes (self, _cmd, two
    // object arguments, void result), which is what the runtime calls it with. Adding a
    // method to a registered class is allowed at any time.
    unsafe {
        let imp: Imp = std::mem::transmute::<
            unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, *mut NSArray<NSURL>),
            Imp,
        >(imp);
        objc2::ffi::class_addMethod(
            std::ptr::from_ref(class).cast_mut(),
            sel!(application:openURLs:),
            imp,
            OPEN_URLS_TYPES.as_ptr().cast::<c_char>(),
        )
        .as_bool()
    }
}

/// The added method. Runs on the main thread, called by AppKit.
unsafe extern "C-unwind" fn application_open_urls(
    _this: *mut AnyObject,
    _cmd: Sel,
    _application: *mut AnyObject,
    urls: *mut NSArray<NSURL>,
) {
    // SAFETY: AppKit passes a valid `NSArray<NSURL *>` (or nil) for the call's duration.
    let Some(urls) = (unsafe { urls.as_ref() }) else {
        return;
    };
    let paths = file_paths(urls);
    if paths.is_empty() {
        return;
    }
    if let Some(sink) = SINK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
    {
        sink(paths);
    }
}

/// The local file paths among `urls` (Finder only sends file URLs; others are skipped).
fn file_paths(urls: &NSArray<NSURL>) -> Vec<PathBuf> {
    (0..urls.count())
        .map(|i| urls.objectAtIndex(i))
        .filter(|url| url.isFileURL())
        .filter_map(|url| url.path())
        .map(|path| PathBuf::from(path.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use objc2::rc::Retained;
    use objc2::runtime::{ClassBuilder, NSObject};
    use objc2::{ClassType as _, msg_send};
    use objc2_foundation::NSString;

    use super::*;

    #[test]
    fn dock_footage_drop_preserves_all_paths() {
        let paths = [
            PathBuf::from("/media/静帧.png"),
            PathBuf::from("/media/clip.mov"),
        ];
        let inv = invocation(&paths).unwrap();
        assert_eq!(inv.id, bae_ui_motion::actions::ids::IMPORT_FILE);
        assert_eq!(inv.params["paths"], serde_json::json!(paths));
        assert!(invocation(&[]).is_none());
    }

    /// The added method receives Finder's URL array and hands the file paths (with
    /// spaces and non-ASCII characters intact) to the sink.
    #[test]
    fn open_urls_reaches_the_sink() {
        let class = ClassBuilder::new(c"BaeOpenDocumentsTestDelegate", NSObject::class())
            .expect("test class name is unique")
            .register();
        assert!(add_open_urls(class));
        assert!(!add_open_urls(class), "a second add is refused");

        let (tx, rx) = mpsc::channel();
        set_sink(Box::new(move |paths| tx.send(paths).unwrap()));

        let paths = ["/tmp/验收 目录/打开 测试.bae", "/tmp/demo.baez"];
        let urls: Vec<Retained<NSURL>> = paths
            .iter()
            .map(|p| NSURL::fileURLWithPath(&NSString::from_str(p)))
            .chain([
                NSURL::URLWithString(&NSString::from_str("https://example.com/x.bae"))
                    .expect("valid URL"),
            ])
            .collect();
        let array = NSArray::from_retained_slice(&urls);
        // SAFETY: `class` is a plain NSObject subclass; `new` returns an owned instance.
        let delegate: Retained<NSObject> = unsafe { msg_send![class, new] };
        // SAFETY: the method was added above with this exact signature.
        let () = unsafe {
            msg_send![
                &*delegate,
                application: std::ptr::null_mut::<AnyObject>(),
                openURLs: &*array
            ]
        };

        let got = rx.try_recv().expect("the sink was called");
        assert_eq!(got, paths.map(PathBuf::from), "web URLs are dropped");
    }
}
