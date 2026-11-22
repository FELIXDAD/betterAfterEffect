//! Save As on Windows: an IFileSaveDialog with our own IFileDialogEvents (issue
//! #126), because rfd cannot stop the dialog from walking into a `.bae` project
//! folder. The decisions live in [`SaveGuard`]; this module only wires them to
//! the callbacks:
//!
//! * `OnFolderChanging` into an existing project: navigation is refused, the
//!   dialog closes and the project's path is the answer.
//! * `OnFileOk`: a typed `proj` that the default extension turns into an
//!   existing `proj.bae` is that project. `FOS_NOTESTFILECREATE` keeps the dialog
//!   from first trying to create a file over the folder ("You don't have
//!   permission to save in this location"); write errors are reported by Save As
//!   in our own words instead.
//! * `OnOverwrite` for a project: accepted silently, Save As asks its own
//!   "Replace?" question. Existing `.baez` files keep the system's prompt.

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
    CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FDE_OVERWRITE_RESPONSE, FDE_SHAREVIOLATION_RESPONSE, FDEOR_ACCEPT, FDEOR_DEFAULT,
    FDESVR_DEFAULT, FOS_FORCEFILESYSTEM, FOS_NOTESTFILECREATE, FileSaveDialog, IFileDialog,
    IFileDialogEvents, IFileDialogEvents_Impl, IFileSaveDialog, IShellItem,
    SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows::core::{HRESULT, HSTRING, PCWSTR, PWSTR, Ref, implement};

use super::save_guard::{SaveGuard, typed_path};

/// One Save As dialog.
pub struct SaveRequest<'a> {
    pub title: &'a str,
    /// Suggested file name.
    pub name: &'a str,
    /// Folder to start in.
    pub dir: Option<&'a Path>,
    /// File types: label and extension (no dot); the first is the default.
    pub types: &'a [(&'a str, &'a str)],
    pub parent: Option<HWND>,
}

/// Show the dialog; `None` = cancelled (or the dialog could not be shown).
pub fn pick(request: &SaveRequest<'_>) -> Option<PathBuf> {
    let _com = Com::init();
    match show(request) {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!("save dialog: {error}");
            None
        }
    }
}

/// The dialog's events, sharing one [`SaveGuard`] with [`show`].
#[implement(IFileDialogEvents)]
struct Events {
    guard: Rc<RefCell<SaveGuard>>,
    /// File types' extensions, by the dialog's 1-based type index.
    extensions: Vec<String>,
}

fn show(request: &SaveRequest<'_>) -> windows::core::Result<Option<PathBuf>> {
    // SAFETY: COM is initialized on this thread (`pick`); the class and interface
    // are the documented FileSaveDialog / IFileSaveDialog pair.
    let dialog: IFileSaveDialog =
        unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)? };
    let guard = Rc::new(RefCell::new(SaveGuard::default()));
    let events: IFileDialogEvents = Events {
        guard: guard.clone(),
        extensions: request.types.iter().map(|(_, e)| (*e).to_owned()).collect(),
    }
    .into();
    configure(&dialog, request)?;
    // SAFETY: `dialog` and `events` are live COM objects on this thread; Show is
    // modal and returns before they are released, and the events are unadvised
    // before `guard` is read.
    unsafe {
        let cookie = dialog.Advise(&events)?;
        let shown = dialog.Show(request.parent);
        let _ = dialog.Unadvise(cookie);
        let result = match shown {
            Ok(()) => dialog.GetResult().ok().and_then(|item| item_path(&item)),
            Err(_) => None, // cancelled, or closed by OnFolderChanging
        };
        Ok(guard.borrow_mut().picked(result))
    }
}

fn configure(dialog: &IFileSaveDialog, request: &SaveRequest<'_>) -> windows::core::Result<()> {
    let names: Vec<HSTRING> = request
        .types
        .iter()
        .map(|(n, _)| HSTRING::from(*n))
        .collect();
    let specs: Vec<HSTRING> = request
        .types
        .iter()
        .map(|(_, e)| HSTRING::from(format!("*.{e}")))
        .collect();
    let types: Vec<COMDLG_FILTERSPEC> = names
        .iter()
        .zip(&specs)
        .map(|(name, spec)| COMDLG_FILTERSPEC {
            pszName: PCWSTR(name.as_ptr()),
            pszSpec: PCWSTR(spec.as_ptr()),
        })
        .collect();
    // SAFETY: every string passed (including the ones `types` points into) is an
    // HSTRING that outlives these calls; the dialog copies what it keeps.
    unsafe {
        dialog.SetTitle(&HSTRING::from(request.title))?;
        dialog.SetFileTypes(&types)?;
        dialog.SetFileTypeIndex(1)?;
        // Set before Show, the default extension follows the chosen file type.
        if let Some((_, ext)) = request.types.first() {
            dialog.SetDefaultExtension(&HSTRING::from(*ext))?;
        }
        dialog.SetFileName(&HSTRING::from(request.name))?;
        if let Some(dir) = request.dir
            && let Ok(folder) =
                SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(dir), None)
        {
            dialog.SetFolder(&folder)?;
        }
        dialog.SetOptions(dialog.GetOptions()? | FOS_FORCEFILESYSTEM | FOS_NOTESTFILECREATE)?;
    }
    Ok(())
}

impl IFileDialogEvents_Impl for Events_Impl {
    fn OnFileOk(&self, pfd: Ref<'_, IFileDialog>) -> windows::core::Result<()> {
        let Some(dialog) = pfd.as_ref() else {
            return Ok(());
        };
        let mut candidates = Vec::new();
        // SAFETY: the dialog calls back on its own thread with itself; these are
        // the getters documented as valid during OnFileOk.
        unsafe {
            if let Some(result) = dialog.GetResult().ok().and_then(|i| item_path(&i)) {
                candidates.push(result);
            }
            let folder = dialog.GetFolder().ok().and_then(|i| item_path(&i));
            let typed = dialog.GetFileName().ok().and_then(take_string);
            let ext = dialog
                .GetFileTypeIndex()
                .ok()
                .and_then(|i| self.extensions.get((i as usize).checked_sub(1)?))
                .map_or("", String::as_str);
            if let (Some(folder), Some(typed)) = (folder, typed)
                && let Some(path) = typed_path(&folder, &typed.to_string_lossy(), ext)
            {
                candidates.push(path);
            }
        }
        self.guard.borrow_mut().file_ok(&candidates);
        Ok(())
    }

    fn OnFolderChanging(
        &self,
        pfd: Ref<'_, IFileDialog>,
        psifolder: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        let (Some(dialog), Some(target)) = (pfd.as_ref(), psifolder.as_ref().and_then(item_path))
        else {
            return Ok(());
        };
        // SAFETY: `dialog` is the live dialog calling back.
        let current = unsafe { dialog.GetFolder() }
            .ok()
            .and_then(|i| item_path(&i));
        let chosen = self
            .guard
            .borrow_mut()
            .folder_changing(&target, current.as_deref());
        if !chosen {
            return Ok(());
        }
        // Refuse the navigation (any failure but E_NOTIMPL does) and close; Show
        // returns this code and `show` takes the project from the guard.
        let cancelled = HRESULT::from_win32(ERROR_CANCELLED.0);
        // SAFETY: Close is documented to be called from the dialog's callbacks.
        unsafe {
            let _ = dialog.Close(cancelled);
        }
        Err(cancelled.into())
    }

    fn OnFolderChange(&self, _pfd: Ref<'_, IFileDialog>) -> windows::core::Result<()> {
        self.guard.borrow_mut().folder_changed();
        Ok(())
    }

    fn OnSelectionChange(&self, _pfd: Ref<'_, IFileDialog>) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnShareViolation(
        &self,
        _pfd: Ref<'_, IFileDialog>,
        _psi: Ref<'_, IShellItem>,
    ) -> windows::core::Result<FDE_SHAREVIOLATION_RESPONSE> {
        Ok(FDESVR_DEFAULT)
    }

    fn OnTypeChange(&self, _pfd: Ref<'_, IFileDialog>) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnOverwrite(
        &self,
        _pfd: Ref<'_, IFileDialog>,
        psi: Ref<'_, IShellItem>,
    ) -> windows::core::Result<FDE_OVERWRITE_RESPONSE> {
        let ours = psi
            .as_ref()
            .and_then(item_path)
            .is_some_and(|p| self.guard.borrow().overwrite_is_ours(&p));
        Ok(if ours { FDEOR_ACCEPT } else { FDEOR_DEFAULT })
    }
}

/// A shell item's file-system path (`None` for virtual items).
fn item_path(item: &IShellItem) -> Option<PathBuf> {
    // SAFETY: `item` is a live shell item; the returned string is freed by
    // `take_string`.
    let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?;
    take_string(name).map(PathBuf::from)
}

/// A COM-allocated string, copied; frees it.
fn take_string(s: PWSTR) -> Option<OsString> {
    if s.is_null() {
        return None;
    }
    // SAFETY: `s` is a non-null, NUL-terminated string the shell allocated with
    // CoTaskMemAlloc and handed to us; it is copied, then freed exactly once.
    unsafe {
        let text = s.to_hstring().to_os_string();
        CoTaskMemFree(Some(s.0 as *const std::ffi::c_void));
        Some(text)
    }
}

/// COM for this thread while a dialog is up (winit has usually initialized it
/// already; then this only balances its own reference).
struct Com(bool);

impl Com {
    fn init() -> Self {
        // SAFETY: plain per-thread COM initialization, balanced in `drop`.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        Com(hr.is_ok())
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful CoInitializeEx in `init`; a `Com`
            // lives in one function call, so it is dropped on the same thread.
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(test)]
mod tests {
    //! The callbacks driven through the COM interface, without showing the
    //! dialog (CI has no desktop to click on).

    use std::fs;

    use super::*;

    fn item(path: &Path) -> IShellItem {
        // SAFETY: COM is initialized by `setup` on this test thread.
        unsafe { SHCreateItemFromParsingName(&HSTRING::from(path), None) }.unwrap()
    }

    fn setup() -> (
        Com,
        IFileSaveDialog,
        IFileDialogEvents,
        Rc<RefCell<SaveGuard>>,
    ) {
        let com = Com::init();
        // SAFETY: COM was just initialized on this thread.
        let dialog: IFileSaveDialog =
            unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER) }.unwrap();
        let guard = Rc::new(RefCell::new(SaveGuard::default()));
        let events: IFileDialogEvents = Events {
            guard: guard.clone(),
            extensions: vec!["bae".into(), "baez".into()],
        }
        .into();
        (com, dialog, events, guard)
    }

    #[test]
    fn walking_into_a_project_is_refused_and_picks_it() {
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("proj.bae");
        fs::create_dir(&proj).unwrap();
        fs::write(proj.join("project.json"), b"{}").unwrap();
        let plain = dir.path().join("Videos");
        fs::create_dir(&plain).unwrap();
        let (_com, dialog, events, guard) = setup();
        let request = SaveRequest {
            title: "Save As",
            name: "Untitled Project.bae",
            dir: Some(dir.path()),
            types: &[("BAE Project", "bae"), ("BAE Package", "baez")],
            parent: None,
        };
        configure(&dialog, &request).unwrap();
        // SAFETY: live COM objects on this thread.
        let options = unsafe { dialog.GetOptions() }.unwrap();
        assert!(options.contains(FOS_NOTESTFILECREATE));
        // SAFETY: live COM objects on this thread; the events are called the way
        // the dialog would call them.
        unsafe {
            events.OnFolderChange(&dialog).unwrap();
            assert!(events.OnFolderChanging(&dialog, &item(&plain)).is_ok());
            let refused = events.OnFolderChanging(&dialog, &item(&proj)).unwrap_err();
            assert_eq!(refused.code(), HRESULT::from_win32(ERROR_CANCELLED.0));
            assert_eq!(
                events.OnOverwrite(&dialog, &item(&proj)).unwrap(),
                FDEOR_ACCEPT
            );
        }
        let picked = guard.borrow_mut().picked(None).unwrap();
        assert!(crate::document::same_path(&picked, &proj), "{picked:?}");
    }

    #[test]
    fn files_keep_the_system_overwrite_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("old.baez");
        fs::write(&file, b"PK").unwrap();
        let (_com, dialog, events, guard) = setup();
        // SAFETY: live COM objects on this thread.
        unsafe {
            assert_eq!(
                events.OnOverwrite(&dialog, &item(&file)).unwrap(),
                FDEOR_DEFAULT
            );
        }
        assert_eq!(guard.borrow_mut().picked(None), None);
    }
}
