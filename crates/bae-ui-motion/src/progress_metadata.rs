//! Window ▸ Progress and Window ▸ Metadata.
//!
//! * Progress: AE's list of background tasks — the Render Queue's renders, one
//!   row each with its progress bar, frames done and state.
//! * Metadata: the File Properties of the item selected in the Project panel
//!   (footage: file, kind, size, dates, dimensions, rate, duration; comps:
//!   their settings), filtered by the search field like AE's panel.

use bae_model::{FrameRate, PrimPath, Stage, Time, Value};
use bae_ui::theme::{self, color};
use bae_ui::{Panel, PanelContext, tr};
use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use std::collections::BTreeSet;

use crate::common;
use crate::render_queue::{ItemStatus, SharedRenderQueue};

/// Registers the panels and their Chinese strings (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    crate::register_panel(
        app,
        "progress",
        bae_ui::PanelKind::new("Progress", |_| Box::new(ProgressPanel)),
    );
    crate::register_panel(
        app,
        "metadata",
        bae_ui::PanelKind::new("Metadata", |spawn| {
            Box::new(MetadataPanel {
                hidden: spawn
                    .state
                    .and_then(|s| serde_json::from_value(s.clone()).ok())
                    .unwrap_or_default(),
                ..Default::default()
            })
        }),
    );
}

/// The Progress panel.
#[derive(Default)]
pub struct ProgressPanel;

impl Panel for ProgressPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let Some(queue) = cx.resources.get::<SharedRenderQueue>() else {
            return;
        };
        let rows: Vec<(String, f32, String)> = queue
            .lock()
            .items()
            .iter()
            .filter_map(|item| {
                let name = format!("{} {}", tr("Rendering"), item.comp_name);
                match &item.status {
                    ItemStatus::Rendering { done, total } => Some((
                        name,
                        *done as f32 / (*total).max(1) as f32,
                        format!("{done} / {total}"),
                    )),
                    ItemStatus::Done { .. } => Some((name, 1.0, tr("Done").to_owned())),
                    ItemStatus::Failed(_) => Some((name, 0.0, tr("Failed").to_owned())),
                    ItemStatus::Stopped { done, total, .. } => Some((
                        name,
                        *done as f32 / (*total).max(1) as f32,
                        tr("Stopped").to_owned(),
                    )),
                    ItemStatus::Queued => None,
                }
            })
            .collect();
        let font = theme::body();
        ui.add_space(6.0);
        for (name, fraction, state) in rows {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
            let p = ui.painter_at(r);
            let inner = r.shrink2(vec2(8.0, 3.0));
            p.text(
                inner.left_top(),
                Align2::LEFT_TOP,
                name,
                font.clone(),
                color::TEXT,
            );
            p.text(
                pos2(inner.right(), inner.top()),
                Align2::RIGHT_TOP,
                state,
                font.clone(),
                color::TEXT_DIM,
            );
            let bar = Rect::from_min_max(
                pos2(inner.left(), inner.bottom() - 6.0),
                inner.right_bottom(),
            );
            p.rect_filled(bar, 2.0, color::FIELD);
            let mut done = bar;
            done.set_width(bar.width() * fraction.clamp(0.0, 1.0));
            p.rect_filled(done, 2.0, color::ACCENT);
        }
        if ui.is_rect_visible(ui.max_rect()) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

/// The Metadata panel.
#[derive(Default)]
pub struct MetadataPanel {
    search: String,
    hidden: [BTreeSet<String>; 2],
    display: Option<(usize, BTreeSet<String>)>,
}

const DC: &str = "http://purl.org/dc/elements/1.1/";
const DM: &str = "http://ns.adobe.com/xmp/1.0/DynamicMedia/";
const FIELDS: &[(&str, &str, &str)] = &[
    (DC, "dc:title", "Document Title"),
    (DC, "dc:creator", "Author"),
    (DC, "dc:description", "Description"),
    (DC, "dc:subject", "Keywords"),
    (DC, "dc:rights", "Copyright Notice"),
    (DM, "xmpDM:scene", "Scene"),
    (DM, "xmpDM:shotName", "Shot Name"),
    (DM, "xmpDM:logComment", "Log Comment"),
];

/// Read descriptive fields at import, alongside the decoder's technical properties.
pub(crate) fn import_xmp(
    path: &std::path::Path,
    attrs: &mut std::collections::BTreeMap<String, Value>,
) {
    use std::io::Read as _;
    let Ok(file) = std::fs::File::open(path.with_extension("xmp")) else {
        return;
    };
    let mut xml = String::new();
    if file.take(2 << 20).read_to_string(&mut xml).is_err() {
        return;
    }
    let Ok(doc) = roxmltree::Document::parse(&xml) else {
        return;
    };
    for &(ns, field, _) in FIELDS {
        let name = field.split_once(':').unwrap().1;
        let key = (ns, name);
        let value = doc.descendants().find_map(|n| {
            if let Some(s) = n.attribute(key) {
                return Some(s.to_owned());
            }
            if !n.has_tag_name(key) {
                return None;
            }
            let strings: Vec<_> = n
                .descendants()
                .filter(|v| v.is_text())
                .filter_map(|v| v.text())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            Some(strings.join("; "))
        });
        if let Some(s) = value {
            attrs.insert(field.to_owned(), Value::String(s));
        }
    }
}

impl Panel for MetadataPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            bae_ui::widgets::search_field(ui, &mut self.search, ui.available_width() - 12.0);
        });
        let items = common::session(cx.resources).lock().selected_items.clone();
        let query = self.search.to_lowercase();
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::CollapsingHeader::new(tr("Project"))
                .default_open(items.is_empty())
                .show(ui, |ui| self.fields(ui, cx, &[], &query));
            egui::CollapsingHeader::new(tr("Files"))
                .default_open(true)
                .show(ui, |ui| {
                    if items.is_empty() {
                        return;
                    }
                    self.fields(ui, cx, &items, &query);
                    if let [item] = items.as_slice() {
                        let rows = properties(cx.stage, item)
                            .into_iter()
                            .filter(|(label, value)| {
                                tr(label).to_lowercase().contains(&query)
                                    || value.to_lowercase().contains(&query)
                            })
                            .collect::<Vec<_>>();
                        if !rows.is_empty() {
                            ui.horizontal(|ui| {
                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new(tr("File Properties"))
                                        .color(color::TEXT_STRONG),
                                );
                            });
                            let font = theme::body();
                            for (label, value) in rows {
                                let (r, resp) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), 20.0),
                                    Sense::hover(),
                                );
                                let p = ui.painter_at(r);
                                let split = r.left() + (r.width() * 0.4).clamp(90.0, 160.0);
                                p.text(
                                    pos2(r.left() + 16.0, r.center().y),
                                    Align2::LEFT_CENTER,
                                    tr(label),
                                    font.clone(),
                                    color::TEXT_DIM,
                                );
                                p.text(
                                    pos2(split, r.center().y),
                                    Align2::LEFT_CENTER,
                                    &value,
                                    font.clone(),
                                    color::TEXT,
                                );
                                resp.on_hover_text(value);
                            }
                        }
                    }
                });
        });
        self.display_dialog(ui.ctx());
    }
    fn menu(&mut self, ui: &mut Ui, _: &mut PanelContext<'_>) {
        for (side, title) in DISPLAY.iter().enumerate() {
            if ui.button(tr(title)).clicked() {
                self.display = Some((side, self.hidden[side].clone()));
                ui.close();
            }
        }
    }
    fn save_state(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.hidden).ok()
    }
}

const DISPLAY: [&str; 2] = ["Project Metadata Display...", "Files Metadata Display..."];
impl MetadataPanel {
    fn fields(&self, ui: &mut Ui, cx: &mut PanelContext<'_>, items: &[PrimPath], query: &str) {
        let side = usize::from(!items.is_empty());
        let project = PrimPath::new(bae_motion::project_settings::PROJECT).unwrap();
        let paths = if items.is_empty() {
            std::slice::from_ref(&project)
        } else {
            items
        };
        for (ns, group) in [(DC, "Basic"), (DM, "Dynamic Media")] {
            egui::CollapsingHeader::new(tr(group))
                .id_salt((side, group))
                .default_open(true)
                .show(ui, |ui| {
                    for &(_, field, label) in
                        FIELDS.iter().filter(|(namespace, _, _)| *namespace == ns)
                    {
                        if self.hidden[side].contains(field) {
                            continue;
                        }
                        let mut values = paths.iter().map(|p| {
                            cx.stage
                                .prim(p)
                                .and_then(|v| bae_motion::query::static_value(&v, field))
                                .and_then(|v| {
                                    if let Value::String(s) = v {
                                        Some(s)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or_default()
                        });
                        let mut value = values.next().unwrap_or_default();
                        let mixed = values.any(|s| s != value);
                        if mixed {
                            value.clear();
                        }
                        if !tr(label).to_lowercase().contains(query)
                            && !value.to_lowercase().contains(query)
                        {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            let caption = ui.label(tr(label));
                            // Typing edits a local copy; Enter or clicking away
                            // commits it as one step (one undo), Esc drops it.
                            let key = egui::Id::new(("metadata-edit", items, field));
                            let mut text = ui
                                .data(|d| d.get_temp::<String>(key))
                                .unwrap_or_else(|| value.clone());
                            let edit = egui::TextEdit::singleline(&mut text)
                                .id_salt((items, field))
                                .hint_text(if mixed { tr("<Multiple Values>") } else { "" })
                                .desired_width(ui.available_width());
                            let resp = ui.add(edit).labelled_by(caption.id);
                            if resp.changed() {
                                ui.data_mut(|d| d.insert_temp(key, text.clone()));
                            }
                            if resp.lost_focus() {
                                ui.data_mut(|d| d.remove::<String>(key));
                                let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
                                if !cancelled && text != value {
                                    cx.invoke_with(
                                        "item.set_metadata",
                                        serde_json::json!({"items":items,"field":field,"text":text}),
                                    );
                                }
                            }
                        });
                    }
                });
        }
    }
    fn display_dialog(&mut self, ctx: &egui::Context) {
        let Some((side, hidden)) = self.display.as_mut() else {
            return;
        };
        let mut save = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("metadata-display")).show(ctx, |ui| {
            ui.heading(tr(DISPLAY[*side]).trim_end_matches("..."));
            for &(_, field, label) in FIELDS {
                let mut show = !hidden.contains(field);
                if ui.checkbox(&mut show, tr(label)).changed() {
                    if show {
                        hidden.remove(field);
                    } else {
                        hidden.insert(field.into());
                    }
                }
            }
            ui.horizontal(|ui| {
                cancel = ui.button(tr("Cancel")).clicked();
                save = ui.button(tr("OK")).clicked();
            });
        });
        if save {
            self.hidden[*side] = std::mem::take(hidden);
        }
        if save || cancel || modal.should_close() {
            self.display = None;
        }
    }
}

/// `(English label, value)` rows of `item`'s File Properties.
fn properties(stage: &Stage, item: &PrimPath) -> Vec<(&'static str, String)> {
    let Some(view) = stage.prim(item) else {
        return Vec::new();
    };
    let get = |attr: &str| bae_motion::query::static_value(&view, attr);
    let mut rows = vec![("Name", bae_motion::query::display_name_of(stage, item))];
    if let Some(info) = bae_motion::query::comp_info(stage, item) {
        rows.push(("Document Kind", tr("Composition").to_owned()));
        rows.push(("Dimensions", format!("{} × {}", info.width, info.height)));
        rows.push(("Pixel Aspect Ratio", format!("{:.2}", info.pixel_aspect)));
        rows.push(("Frame Rate", fps(info.frame_rate)));
        rows.push(("Duration", duration(info.duration, info.frame_rate)));
        return rows;
    }
    let kind = match get("kind") {
        Some(Value::String(s) | Value::Enum(s)) => s,
        _ => "footage".into(),
    };
    rows.push((
        "Document Kind",
        tr(match kind.as_str() {
            "solid" => "Solid",
            "sequence" => "Image Sequence",
            "placeholder" => "Placeholder",
            _ => "Footage",
        })
        .to_owned(),
    ));
    if let Some(file) = bae_model::AssetFile::from_prim(&view) {
        let path = std::path::Path::new(&file.original);
        if let Some(name) = path.file_name() {
            rows.push(("File Name", name.to_string_lossy().into_owned()));
        }
        rows.push(("Path", file.original.clone()));
        if file.size > 0 {
            rows.push(("File Size", file_size(file.size)));
        }
        if let Some(ms) = file.mtime_ms {
            let t = std::time::UNIX_EPOCH
                + std::time::Duration::from_millis(u64::try_from(ms).unwrap_or(0));
            rows.push(("Date Modified", date(t)));
        }
    }
    let dim = |attr: &str| get(attr).and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
    if dim("width") > 0 {
        rows.push((
            "Dimensions",
            format!("{} × {}", dim("width"), dim("height")),
        ));
    }
    if let Some(par) = get("pixel_aspect").and_then(|v| v.as_f64()) {
        rows.push(("Pixel Aspect Ratio", format!("{par:.2}")));
    }
    let rate = match get("frame_rate") {
        Some(Value::FrameRate(r)) => Some(r),
        Some(v) => v.as_f64().filter(|f| *f > 0.0).map(FrameRate::from_fps),
        None => None,
    };
    if let Some(rate) = rate {
        rows.push(("Frame Rate", fps(rate)));
    }
    if let Some(Value::Time(d)) = get("duration") {
        rows.push(("Duration", duration(d, rate.unwrap_or(FrameRate::FPS_30))));
    }
    rows
}

fn fps(rate: FrameRate) -> String {
    format!("{:.2} fps", rate.as_f64())
}

fn duration(d: Time, rate: FrameRate) -> String {
    bae_ui::widgets::timecode(rate.time_to_frame(d), rate.as_f64())
}

fn file_size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.2} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.2} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{b} B"),
    }
}

/// `YYYY-MM-DD hh:mm` (UTC).
fn date(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

/// Chinese for the panels (AE 26).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Files", "文件"),
    ("Project Metadata Display...", "项目元数据显示..."),
    ("Files Metadata Display...", "文件元数据显示..."),
    ("<Multiple Values>", "<多个值>"),
    ("Edit Metadata", "编辑元数据"),
    ("Basic", "基本"),
    ("Dynamic Media", "动态媒体"),
    ("Document Title", "文档标题"),
    ("Author", "作者"),
    ("Description", "描述"),
    ("Keywords", "关键字"),
    ("Copyright Notice", "版权公告"),
    ("Scene", "场景"),
    ("Shot Name", "拍摄名称"),
    ("Log Comment", "日志注释"),
    ("Rendering", "渲染"),
    ("Done", "完成"),
    ("Failed", "失败"),
    ("Stopped", "已停止"),
    ("File Properties", "文件属性"),
    ("Document Kind", "文档类型"),
    ("File Name", "文件名"),
    ("File Size", "文件大小"),
    ("Date Modified", "修改日期"),
    ("Dimensions", "尺寸"),
    ("Pixel Aspect Ratio", "像素长宽比"),
    ("Frame Rate", "帧速率"),
    ("Image Sequence", "图像序列"),
    ("Placeholder", "占位符"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_sizes_read_like_ae() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        assert_eq!(date(t), "2023-11-14 22:13");
        assert_eq!(file_size(1536), "1.5 KB");
        assert_eq!(file_size(3 << 20), "3.00 MB");
    }
}
