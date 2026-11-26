//! Native file choosers for modules (File > Import, Replace Footage, render
//! output …). The shell provides the platform's open panel as the app resource
//! [`SharedFilePicker`]; tests and hosts without native dialogs provide
//! another one (or none: choosing is then cancelled).
//!
//! Choosing blocks until the panel closes, like AE's modal file dialogs, so
//! call it from an action handler (after the frame), never while drawing.

use std::path::PathBuf;
use std::sync::Arc;

/// What to choose.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenRequest {
    /// Panel title (translated).
    pub title: String,
    /// File type filters: label and extensions without dots. Empty = all files.
    pub filters: Vec<(String, Vec<String>)>,
    /// Several files may be chosen.
    pub multiple: bool,
    /// Folders may be chosen too (macOS; Windows panels pick files only).
    pub folders: bool,
}

/// A native open panel.
pub trait FilePicker: Send + Sync {
    /// The chosen files (and folders, when allowed); empty when cancelled.
    fn open(&self, request: &OpenRequest) -> Vec<PathBuf>;
}

/// Where the open project is saved (`None`: not saved yet), as an app
/// resource the shell keeps up to date (Reveal Project in Finder).
#[derive(Clone, Debug, Default)]
pub struct ProjectFile(Arc<std::sync::Mutex<Option<PathBuf>>>);

impl ProjectFile {
    pub fn get(&self) -> Option<PathBuf> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set(&self, path: Option<PathBuf>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = path;
    }
}

/// The file picker as an app resource.
#[derive(Clone)]
pub struct SharedFilePicker(pub Arc<dyn FilePicker>);

impl SharedFilePicker {
    pub fn new(picker: impl FilePicker + 'static) -> Self {
        SharedFilePicker(Arc::new(picker))
    }

    /// Choose with the app's picker; cancelled (empty) when there is none.
    pub fn open(resources: &bae_app::Resources, request: &OpenRequest) -> Vec<PathBuf> {
        resources
            .get::<SharedFilePicker>()
            .map(|p| p.0.open(request))
            .unwrap_or_default()
    }
}

impl std::fmt::Debug for SharedFilePicker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SharedFilePicker")
    }
}
