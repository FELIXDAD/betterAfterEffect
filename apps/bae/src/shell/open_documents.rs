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
