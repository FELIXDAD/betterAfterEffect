//! Window ▸ Media Browser: AE's file browser for importing. Favorites and the
//! local drives on the left, the current folder's subfolders and the media
//! files BAE can import on the right; back / forward, the folder path and a
//! search field on top. Double-click a folder to open it, a file to import it
//! (as File ▸ Import does).

use std::path::{Path, PathBuf};

use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use bae_ui::{Panel, PanelContext, tr};
use egui::{Align2, Color32, Rect, Sense, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

/// Width of the favorites column and height of a row.
const SIDE: f32 = 150.0;
const ROW: f32 = 20.0;

/// One file or folder of the current folder.
#[derive(Clone, Debug)]
struct Entry {
    path: PathBuf,
    name: String,
    folder: bool,
    size: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    folder: Option<PathBuf>,
}

/// The Media Browser panel.
#[derive(Default)]
pub struct MediaBrowserPanel {
    state: State,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
    search: String,
    selected: Option<PathBuf>,
    /// The listing of `state.folder` (read when the folder changes).
    listing: Option<(PathBuf, Vec<Entry>)>,
    extensions: Vec<String>,
}

impl MediaBrowserPanel {
    pub fn from_state(state: Option<&serde_json::Value>) -> Self {
        MediaBrowserPanel {
            state: state
                .and_then(|s| serde_json::from_value(s.clone()).ok())
                .unwrap_or_default(),
            ..Default::default()
        }
    }

    fn folder(&self) -> PathBuf {
        self.state
            .folder
            .clone()
            .filter(|f| f.is_dir())
            .or_else(home)
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn go(&mut self, to: PathBuf) {
        let from = self.folder();
        if from != to {
            self.back.push(from);
            self.forward.clear();
            self.state.folder = Some(to);
            self.selected = None;
        }
    }

    /// The current folder's entries: folders, then the files BAE imports.
    fn entries(&mut self) -> Vec<Entry> {
        let folder = self.folder();
        if self.listing.as_ref().map(|(f, _)| f) != Some(&folder) {
            if self.extensions.is_empty() {
                self.extensions = bae_media::builtin_formats()
                    .iter()
                    .flat_map(|f| f.extensions().iter().map(|e| e.to_ascii_lowercase()))
                    .collect();
            }
            let mut list: Vec<Entry> = std::fs::read_dir(&folder)
                .map(|dir| {
                    dir.flatten()
                        .filter_map(|e| {
                            let name = e.file_name().to_string_lossy().into_owned();
                            if name.starts_with('.') {
                                return None;
                            }
                            let meta = e.metadata().ok()?;
                            let folder = meta.is_dir();
                            let ext = Path::new(&name)
                                .extension()
                                .map(|x| x.to_string_lossy().to_ascii_lowercase());
                            let media = ext.is_some_and(|x| self.extensions.contains(&x));
                            (folder || media).then(|| Entry {
                                path: e.path(),
                                name,
                                folder,
                                size: meta.len(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            list.sort_by(|a, b| {
                b.folder
                    .cmp(&a.folder)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            self.listing = Some((folder, list));
        }
        let query = self.search.to_lowercase();
        self.listing
            .as_ref()
            .map(|(_, l)| {
                l.iter()
                    .filter(|e| query.is_empty() || e.name.to_lowercase().contains(&query))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Panel for MediaBrowserPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let folder = self.folder();
        // Top bar: back, forward, the folder, search.
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            let back = ui
                .add_enabled_ui(!self.back.is_empty(), |ui| {
                    icons::button(ui, Icon::ChevronLeft, 18.0, false)
                })
                .inner;
            if back.on_hover_text(tr("Go Back")).clicked()
                && let Some(prev) = self.back.pop()
            {
                self.forward.push(folder.clone());
                self.state.folder = Some(prev);
            }
            let fwd = ui
                .add_enabled_ui(!self.forward.is_empty(), |ui| {
                    icons::button(ui, Icon::ChevronRight, 18.0, false)
                })
                .inner;
            if fwd.on_hover_text(tr("Go Forward")).clicked()
                && let Some(next) = self.forward.pop()
            {
                self.back.push(folder.clone());
                self.state.folder = Some(next);
            }
            let name = folder.file_name().map_or_else(
                || folder.to_string_lossy().into_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
            let up = ui.button(name).on_hover_text(folder.to_string_lossy());
            if up.clicked()
                && let Some(parent) = folder.parent()
            {
                self.go(parent.to_path_buf());
            }
            bae_ui::widgets::search_field(
                ui,
                &mut self.search,
                (ui.available_width() - 6.0).max(60.0),
            );
        });
        ui.add_space(4.0);
        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, Sense::hover());
        let side = Rect::from_min_size(area.min, vec2(SIDE.min(area.width() * 0.4), area.height()));
        let list = Rect::from_min_max(pos2(side.right() + 1.0, area.top()), area.max);
        let p = ui.painter_at(area);
        p.rect_filled(side, 0.0, color::PANEL);
        p.rect_filled(list, 0.0, color::FIELD);
        self.places(ui, side);
        self.list(ui, cx, list);
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.state).ok()
    }
}

impl MediaBrowserPanel {
    /// Favorites and local drives.
    fn places(&mut self, ui: &mut Ui, r: Rect) {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink(4.0)));
        let ui = &mut child;
        let title = |ui: &mut Ui, t: &str| {
            ui.label(egui::RichText::new(tr(t)).color(color::TEXT_DIM).small());
        };
        title(ui, "Favorites");
        let home = home();
        let mut places: Vec<(String, PathBuf)> = Vec::new();
        if let Some(h) = &home {
            places.push((tr("Home").to_owned(), h.clone()));
            for (en, dir) in [
                ("Desktop", "Desktop"),
                ("Documents", "Documents"),
                ("Downloads", "Downloads"),
                ("Movies", if cfg!(windows) { "Videos" } else { "Movies" }),
                ("Pictures", "Pictures"),
            ] {
                let path = h.join(dir);
                if path.is_dir() {
                    places.push((tr(en).to_owned(), path));
                }
            }
        }
        let current = self.folder();
        for (name, path) in places {
            if ui.selectable_label(current == path, name).clicked() {
                self.go(path);
            }
        }
        ui.add_space(6.0);
        title(ui, "Local Drives");
        for drive in drives() {
            let name = drive.to_string_lossy().into_owned();
            if ui.selectable_label(current == drive, name).clicked() {
                self.go(drive);
            }
        }
    }

    /// The folder's entries: Name, Size.
    fn list(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>, r: Rect) {
        let entries = self.entries();
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
        let font = theme::body();
        let mut open: Option<Entry> = None;
        egui::ScrollArea::vertical()
            .id_salt("media-browser-list")
            .auto_shrink([false, false])
            .show_rows(&mut child, ROW, entries.len(), |ui, range| {
                for e in &entries[range] {
                    let (row, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                    let selected = self.selected.as_ref() == Some(&e.path);
                    let p = ui.painter_at(row);
                    if selected {
                        p.rect_filled(row, 0.0, color::ACCENT_DIM);
                    } else if resp.hovered() {
                        p.rect_filled(row, 0.0, Color32::from_white_alpha(8));
                    }
                    let icon = if e.folder {
                        Icon::Folder
                    } else {
                        Icon::Footage
                    };
                    icons::paint(
                        &p,
                        Rect::from_center_size(
                            pos2(row.left() + 14.0, row.center().y),
                            vec2(14.0, 14.0),
                        ),
                        icon,
                        color::ICON,
                    );
                    p.text(
                        pos2(row.left() + 28.0, row.center().y),
                        Align2::LEFT_CENTER,
                        &e.name,
                        font.clone(),
                        color::TEXT,
                    );
                    if !e.folder {
                        p.text(
                            pos2(row.right() - 8.0, row.center().y),
                            Align2::RIGHT_CENTER,
                            size_text(e.size),
                            font.clone(),
                            color::TEXT_DIM,
                        );
                    }
                    if resp.clicked() {
                        self.selected = Some(e.path.clone());
                    }
                    if resp.double_clicked() {
                        open = Some(e.clone());
                    }
                }
            });
        if let Some(e) = open {
            if e.folder {
                self.go(e.path);
            } else {
                crate::import::import_paths(cx, &[e.path], None);
            }
        }
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// The local drives: `/` (and mounted volumes) on macOS, the drive letters on Windows.
fn drives() -> Vec<PathBuf> {
    if cfg!(windows) {
        ('A'..='Z')
            .map(|c| PathBuf::from(format!("{c}:\\")))
            .filter(|p| p.is_dir())
            .collect()
    } else {
        let mut out = vec![PathBuf::from("/")];
        if let Ok(dir) = std::fs::read_dir("/Volumes") {
            out.extend(dir.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
        }
        out
    }
}

fn size_text(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b => format!("{} KB", b.div_ceil(1024)),
    }
}

/// Chinese for the panel (AE 26).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Go Back", "上一步"),
    ("Go Forward", "前进"),
    ("Favorites", "收藏夹"),
    ("Local Drives", "本地驱动器"),
    ("Home", "主目录"),
    ("Desktop", "桌面"),
    ("Documents", "文稿"),
    ("Downloads", "下载"),
    ("Movies", "影片"),
    ("Pictures", "图片"),
];
