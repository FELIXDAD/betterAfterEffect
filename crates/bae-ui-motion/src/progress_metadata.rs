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

use crate::common;
use crate::render_queue::{ItemStatus, SharedRenderQueue};

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
}

impl Panel for MetadataPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            bae_ui::widgets::search_field(ui, &mut self.search, ui.available_width() - 12.0);
        });
        let item = common::session(cx.resources)
            .lock()
            .selected_items
            .first()
            .cloned();
        let Some(item) = item else {
            return;
        };
        let query = self.search.to_lowercase();
        let rows: Vec<(&str, String)> = properties(cx.stage, &item)
            .into_iter()
            .filter(|(label, value)| {
                query.is_empty()
                    || tr(label).to_lowercase().contains(&query)
                    || value.to_lowercase().contains(&query)
            })
            .collect();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(tr("File Properties")).color(color::TEXT_STRONG));
        });
        let font = theme::body();
        for (label, value) in rows {
            let (r, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
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
