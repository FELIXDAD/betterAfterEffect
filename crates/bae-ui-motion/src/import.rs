//! Importing files: File > Import > File (Cmd/Ctrl+I), Import > Multiple
//! Files (Cmd/Ctrl+Alt+I) and files or folders dragged from Finder / Explorer
//! onto the Project panel.
//!
//! Files are probed here with `bae-media` (size, frame rate, duration, alpha,
//! sequences) and imported with one `item.import` (one undo step). A numbered
//! image picked or dropped on its own imports its whole sequence, named like
//! AE (`shot_[0001-0024].png`); a dropped folder keeps its structure. Files no
//! format can read are reported in an alert, like AE's import error.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use bae_media::sequence::{ScannedItem, scan_directory};
use bae_media::{MediaFormats, OpenOptions, SequenceInfo, SequenceMode};
use bae_model::PrimPath;
use bae_motion::{ImportFile, query};
use bae_ui::files::{OpenRequest, SharedFilePicker};
use bae_ui::{PanelContext, tr};
use serde_json::json;

use crate::session::SharedSession;

fn formats() -> &'static MediaFormats {
    static FORMATS: OnceLock<MediaFormats> = OnceLock::new();
    FORMATS.get_or_init(MediaFormats::builtin)
}

/// Probed files ready for `item.import`, and the names of those that cannot
/// be imported.
#[derive(Debug, Default)]
pub struct Probed {
    pub files: Vec<ImportFile>,
    pub failed: Vec<String>,
}

/// AE's name for a sequence item: the frame range in brackets
/// (`shot_####.png`, 1–24 → `shot_[0001-0024].png`).
pub fn sequence_name(seq: &SequenceInfo) -> String {
    let Some(start) = seq.pattern.find('#') else {
        return seq.pattern.clone();
    };
    let width = seq.pattern[start..]
        .chars()
        .take_while(|c| *c == '#')
        .count();
    format!(
        "{}[{:0w$}-{:0w$}]{}",
        &seq.pattern[..start],
        seq.first,
        seq.last,
        &seq.pattern[start + width..],
        w = width
    )
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Import defaults (Preferences > Import and Labels).
#[derive(Clone, Debug)]
pub struct ImportDefaults {
    /// Frames per second of image sequences.
    pub sequence_fps: f64,
    /// Duration given to stills (`None`: the length of the composition).
    pub still_duration: Option<bae_model::Time>,
    /// Labels of stills, moving footage and audio.
    pub labels: Option<[i64; 3]>,
}

impl Default for ImportDefaults {
    /// AE's factory defaults.
    fn default() -> Self {
        ImportDefaults {
            sequence_fps: 30.0,
            still_duration: None,
            labels: None,
        }
    }
}

impl ImportDefaults {
    pub(crate) fn of(prefs: &bae_prefs::Preferences) -> Self {
        use bae_media::MediaKind;
        ImportDefaults {
            sequence_fps: crate::prefs::sequence_fps(prefs),
            still_duration: crate::prefs::still_duration(prefs),
            labels: Some([
                crate::prefs::footage_label(prefs, MediaKind::Still),
                crate::prefs::footage_label(prefs, MediaKind::Movie),
                crate::prefs::footage_label(prefs, MediaKind::Audio),
            ]),
        }
    }
}

/// Probe one file (or the sequence it belongs to when `sequences`).
fn probe_file(
    path: &Path,
    folder: Option<&str>,
    sequences: bool,
    defaults: &ImportDefaults,
    seen: &mut HashSet<PathBuf>,
    out: &mut Probed,
) {
    let options = OpenOptions {
        sequence: if sequences {
            SequenceMode::Sequence
        } else {
            SequenceMode::Single
        },
        sequence_frame_rate: bae_model::FrameRate::from_fps(defaults.sequence_fps),
    };
    let info = bae_media::audio::file_info(path).or_else(|| {
        formats()
            .open(path, &options)
            .ok()
            .map(|source| source.info().clone())
    });
    match info {
        Some(info) => {
            // Several frames of one sequence picked: one item.
            if !seen.insert(info.path.clone()) {
                return;
            }
            let name = match &info.sequence {
                Some(seq) => sequence_name(seq),
                None => file_name(path),
            };
            let mut attrs: std::collections::BTreeMap<String, bae_model::Value> = info
                .asset_attributes()
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect();
            use bae_media::MediaKind;
            if info.kind == MediaKind::Still
                && let Some(d) = defaults.still_duration
            {
                attrs.insert("duration".into(), bae_model::Value::Time(d));
            }
            if let Some([still, video, audio]) = defaults.labels {
                let label = match info.kind {
                    MediaKind::Still => still,
                    MediaKind::Audio => audio,
                    MediaKind::ImageSequence | MediaKind::Movie => video,
                };
                attrs.insert("label".into(), bae_model::Value::Int(label));
            }
            out.files.push(ImportFile {
                path: info.path.to_string_lossy().into_owned(),
                name: Some(name),
                attrs,
                folder: folder.map(str::to_owned),
            });
        }
        None => out.failed.push(file_name(path)),
    }
}

/// Probe a dropped folder: its sequences and files, then its subfolders, in
/// a project folder of the same name (`rel`).
fn probe_dir(
    dir: &Path,
    rel: &str,
    defaults: &ImportDefaults,
    seen: &mut HashSet<PathBuf>,
    out: &mut Probed,
) {
    let Ok(items) = scan_directory(dir, &[]) else {
        out.failed.push(file_name(dir));
        return;
    };
    for item in items {
        match item {
            ScannedItem::File(p) => {
                // Hidden and system files (".DS_Store", "Thumbs.db") are not footage.
                let name = file_name(&p);
                if name.starts_with('.') || name.eq_ignore_ascii_case("thumbs.db") {
                    continue;
                }
                probe_file(&p, Some(rel), false, defaults, seen, out);
            }
            ScannedItem::Sequence(seq) => probe_file(
                &seq.path_for(seq.first()),
                Some(rel),
                true,
                defaults,
                seen,
                out,
            ),
        }
    }
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !file_name(p).starts_with('.'))
        .collect();
    subdirs.sort();
    for sub in subdirs {
        probe_dir(
            &sub,
            &format!("{rel}/{}", file_name(&sub)),
            defaults,
            seen,
            out,
        );
    }
}

/// Probe chosen or dropped paths: files (with their sequences) and folders.
pub fn probe(paths: &[PathBuf], defaults: &ImportDefaults) -> Probed {
    let mut out = Probed::default();
    let mut seen = HashSet::new();
    for path in paths {
        if path.is_dir() {
            probe_dir(path, &file_name(path), defaults, &mut seen, &mut out);
        } else {
            probe_file(path, None, true, defaults, &mut seen, &mut out);
        }
    }
    out
}

/// Import `paths` into `parent` (default the assets folder): one
/// `item.import`; unreadable files are reported in an alert.
pub(crate) fn import_paths(cx: &mut PanelContext<'_>, paths: &[PathBuf], parent: Option<PrimPath>) {
    if paths.is_empty() {
        return;
    }
    let defaults = ImportDefaults::of(&crate::prefs::current(cx.resources));
    let probed = probe(paths, &defaults);
    if !probed.files.is_empty() {
        let mut params = json!({ "files": probed.files });
        if let Some(p) = parent {
            params["parent"] = json!(p);
        }
        cx.invoke_with("item.import", params);
    }
    if !probed.failed.is_empty() {
        let message =
            tr("These files could not be imported because their format is not supported:")
                .to_owned()
                + "\n"
                + &probed.failed.join("\n");
        crate::dialogs::alert(cx, tr("Import File"), message);
    }
}

/// Where File > Import puts the items: the folder selected in the Project
/// panel, else the default (the assets folder).
fn import_folder(cx: &PanelContext<'_>, session: &SharedSession) -> Option<PrimPath> {
    let s = session.lock();
    match s.selected_items.as_slice() {
        [item]
            if cx
                .stage
                .prim(item)
                .is_some_and(|v| v.is_a(&bae_model::PrimType::FOLDER))
                && query::comp_info(cx.stage, item).is_none() =>
        {
            Some(item.clone())
        }
        _ => None,
    }
}

/// The open panel of File > Import > File.
fn request() -> OpenRequest {
    OpenRequest {
        title: tr("Import File").to_owned(),
        filters: Vec::new(),
        multiple: true,
        folders: true,
    }
}

/// File > Import > File (Cmd/Ctrl+I): one open panel.
pub(crate) fn import_file(cx: &mut PanelContext<'_>, session: &SharedSession) {
    let paths = SharedFilePicker::open(cx.resources, &request());
    let folder = import_folder(cx, session);
    import_paths(cx, &paths, folder);
}

/// File > Import > Multiple Files (Cmd/Ctrl+Alt+I): the open panel comes back
/// after each import until it is cancelled (AE).
pub(crate) fn import_multiple(cx: &mut PanelContext<'_>, session: &SharedSession) {
    let folder = import_folder(cx, session);
    let mut request = request();
    request.title = tr("Import Multiple Files").to_owned();
    // A guard against a picker that never cancels (a script).
    for _ in 0..1000 {
        let paths = SharedFilePicker::open(cx.resources, &request);
        if paths.is_empty() {
            break;
        }
        import_paths(cx, &paths, folder.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_names_follow_ae() {
        let seq = |pattern: &str, first, last| SequenceInfo {
            pattern: pattern.into(),
            first,
            last,
            missing: Vec::new(),
        };
        assert_eq!(
            sequence_name(&seq("shot_####.png", 1, 24)),
            "shot_[0001-0024].png"
        );
        assert_eq!(sequence_name(&seq("f#.exr", 7, 120)), "f[7-120].exr");
        assert_eq!(sequence_name(&seq("plain.png", 0, 0)), "plain.png");
    }
}
