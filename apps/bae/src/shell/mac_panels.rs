//! Main-thread AppKit entry points and owned results from the modal panels.
use bae_ui::files::ImportSelection;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSString, NSURL};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::files::UnsavedChoice;
use crate::i18n::tr;

unsafe extern "C" {
    fn bae_confirm_close(view: *mut AnyObject, labels: &NSArray<NSString>) -> isize;
    fn bae_import_panel(
        view: *mut AnyObject,
        labels: &NSArray<NSString>,
        sequences: &mut bool,
        alphabetical: &mut bool,
        create_comp: &mut bool,
        kind: &mut isize,
    ) -> *mut NSArray<NSURL>;
    fn bae_dock_progress(percent: i32);
    fn bae_prepare_notifications();
    fn bae_render_notification(title: &NSString);
    fn bae_system_shortcuts(standard: bool);
}

fn view(window: Option<&Window>) -> *mut AnyObject {
    match window
        .and_then(|w| w.window_handle().ok())
        .map(|h| h.as_raw())
    {
        Some(RawWindowHandle::AppKit(h)) => h.ns_view.as_ptr().cast(),
        _ => std::ptr::null_mut(),
    }
}

fn labels(text: &[&str]) -> Retained<NSArray<NSString>> {
    NSArray::from_retained_slice(
        &text
            .iter()
            .map(|s| NSString::from_str(s))
            .collect::<Vec<_>>(),
    )
}

pub fn confirm(window: Option<&Window>, question: &str) -> UnsavedChoice {
    let labels = labels(&[
        question,
        tr("If you don't save, your changes will be lost."),
        tr("Save"),
        tr("Cancel"),
        tr("Don't Save"),
    ]);
    // SAFETY: the main-thread caller owns the window and strings throughout the modal call.
    match unsafe { bae_confirm_close(view(window), &labels) } {
        0 => UnsavedChoice::Save,
        2 => UnsavedChoice::DontSave,
        _ => UnsavedChoice::Cancel,
    }
}

pub fn import(window: Option<&Window>, title: &str) -> ImportSelection {
    let labels = labels(&[
        title,
        tr("Image Sequence"),
        tr("Create Composition"),
        tr("Force alphabetical order"),
        tr("Import As:"),
        tr("Footage"),
        tr("Composition"),
        tr("Composition - Retain Layer Sizes"),
    ]);
    let (mut sequences, mut alphabetical, mut create_comp) = (true, false, false);
    let mut kind = 0;
    // SAFETY: synchronous main-thread panel, nil on cancel or a retained array of file URLs.
    let urls = unsafe {
        Retained::from_raw(bae_import_panel(
            view(window),
            &labels,
            &mut sequences,
            &mut alphabetical,
            &mut create_comp,
            &mut kind,
        ))
    };
    let paths = urls
        .iter()
        .flat_map(|urls| (0..urls.count()).filter_map(|i| urls.objectAtIndex(i).to_file_path()))
        .collect();
    ImportSelection {
        paths,
        sequences,
        alphabetical,
        create_comp,
        photoshop_kind: match kind {
            1 => bae_model::psd::ImportKind::Composition,
            2 => bae_model::psd::ImportKind::RetainSize,
            _ => bae_model::psd::ImportKind::Footage,
        },
    }
}

pub struct NotificationWarmUp;

impl bae_gpu::WarmUp for NotificationWarmUp {
    fn name(&self) -> &str {
        "macOS notification center"
    }
    fn run(&self, _: &std::sync::Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        // SAFETY: UserNotifications supports background initialization. No AppKit windows are created.
        unsafe { bae_prepare_notifications() };
        Ok(())
    }
}

pub fn system_shortcuts(standard: bool) {
    // SAFETY: called after main-thread menu installation or a preferences change.
    unsafe { bae_system_shortcuts(standard) };
}

pub fn render_event(event: bae_ui_motion::render_queue::RenderEvent, notify: bool) {
    use bae_ui_motion::render_queue::RenderEvent;
    // SAFETY: winit delivers these events on the AppKit main thread.
    unsafe {
        match event {
            RenderEvent::Progress(percent) => bae_dock_progress(i32::from(percent)),
            RenderEvent::Finished { succeeded } => {
                bae_dock_progress(-1);
                if notify && succeeded {
                    bae_render_notification(&NSString::from_str(tr("Render Queue Complete")));
                }
            }
        }
    }
}
