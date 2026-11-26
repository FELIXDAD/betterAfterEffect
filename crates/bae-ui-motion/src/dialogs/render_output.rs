//! The Render Queue's two settings dialogs, laid out as AE 26's
//! (docs/reference/ae/ui/dialogs/render-settings.json,
//! output-module-settings.json): Render Settings (the item's Best Settings
//! link) and Output Module Settings (its output module link). What BAE's
//! renderer follows is editable — Time Span (Work Area Only / Length of Comp /
//! Custom), Format, Channels and the video bit rate; AE's other settings show
//! their defaults, greyed. OK changes the queue item.

use bae_model::{PrimPath, Stage};
use bae_motion::query::{self, CompInfo};
use bae_render::ExportFormat;
use bae_ui::dialog::{self, Outcome};
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui};

use crate::render_queue::{self, Channels, FORMATS, SharedRenderQueue};

/// Render Settings ▸ Time Span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Span {
    Comp,
    WorkArea,
    Custom,
}

const LW: f32 = 150.0;

/// A greyed dropdown showing AE's default.
fn fixed(ui: &mut Ui, label: &str, value: &str) {
    dialog::row(ui, LW, tr(label), |ui| {
        ui.add_enabled_ui(false, |ui| {
            let mut i = 0;
            bae_ui::widgets::dropdown(ui, label, &mut i, &[tr(value)], 170.0);
        });
    });
}

/// The open Render Settings dialog.
#[derive(Clone, Debug)]
pub struct RenderSettings {
    item: u64,
    comp: PrimPath,
    title: String,
    span: Span,
    start: f64,
    end: f64,
}

impl RenderSettings {
    pub(crate) fn new(stage: &Stage, queue: &SharedRenderQueue, item: u64) -> Option<Self> {
        let q = queue.lock();
        let it = q.items().iter().find(|i| i.id == item)?;
        let info = query::comp_info(stage, &it.comp)?;
        let (wa_start, wa_end) = work_area(&info);
        let last = info.frame_count() - 1;
        let span = if (it.start, it.end) == (wa_start, wa_end) {
            Span::WorkArea
        } else if (it.start, it.end) == (0, last) {
            Span::Comp
        } else {
            Span::Custom
        };
        Some(RenderSettings {
            item,
            comp: it.comp.clone(),
            title: it.comp_name.clone(),
            span,
            start: it.start as f64,
            end: it.end as f64,
        })
    }

    pub(crate) fn ui(&mut self, ctx: &egui::Context, cx: &PanelContext<'_>) -> bool {
        let Some(info) = query::comp_info(cx.stage, &self.comp) else {
            return false;
        };
        let id = Id::new("bae-ui-motion.render-settings");
        let shown = dialog::window(ctx, id, tr("Render Settings"), |ui| {
            self.body(ui, id, &info)
        });
        match shown {
            Some(Outcome::Ok) => {
                let (start, end) = self.range(&info);
                let queue = render_queue::shared(cx.resources);
                if let Some(it) = queue.lock().item_mut(self.item) {
                    it.start = start;
                    it.end = end;
                }
                false
            }
            Some(Outcome::Cancel) => false,
            Some(Outcome::None) => true,
            None => !dialog::closed_by_title_bar(ctx, id),
        }
    }

    /// First and last frame (comp frames) of the time span.
    fn range(&self, info: &CompInfo) -> (i64, i64) {
        let last = info.frame_count() - 1;
        match self.span {
            Span::Comp => (0, last),
            Span::WorkArea => work_area(info),
            Span::Custom => {
                let s = (self.start.round() as i64).clamp(0, last);
                (s, (self.end.round() as i64).clamp(s, last))
            }
        }
    }

    fn body(&mut self, ui: &mut Ui, id: Id, info: &CompInfo) -> Outcome {
        ui.set_width(520.0);
        ui.label(egui::RichText::new(format!("{} “{}”", tr("Composition"), self.title)).strong());
        ui.indent(id.with("comp"), |ui| {
            fixed(ui, "Quality:", "Best");
            fixed(ui, "Resolution:", "Full");
            dialog::row(ui, LW, "", |ui| {
                ui.label(format!("{} {} × {}", tr("Size:"), info.width, info.height));
            });
            fixed(ui, "Disk Cache:", "Read Only");
            fixed(ui, "Proxy Use:", "Use No Proxies");
            fixed(ui, "Effects:", "Current Settings");
            fixed(ui, "Solo Switches:", "Current Settings");
            fixed(ui, "Guide Layers:", "All Off");
            fixed(ui, "Color Depth:", "Current Settings");
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new(tr("Time Sampling")).strong());
        ui.indent(id.with("time"), |ui| {
            fixed(ui, "Frame Blending:", "On for Checked Layers");
            fixed(ui, "Field Render:", "Off");
            fixed(ui, "Motion Blur:", "On for Checked Layers");
            dialog::row(ui, LW, tr("Time Span:"), |ui| {
                let spans = [Span::Comp, Span::WorkArea, Span::Custom];
                let names = [tr("Length of Comp"), tr("Work Area Only"), tr("Custom...")];
                let mut i = spans.iter().position(|s| *s == self.span).unwrap_or(1);
                if bae_ui::widgets::dropdown(ui, "time-span", &mut i, &names, 170.0).changed() {
                    self.span = spans[i];
                    let (s, e) = self.range(info);
                    self.start = s as f64;
                    self.end = e as f64;
                }
            });
            let (s, e) = self.range(info);
            let last = (info.frame_count() - 1) as f64;
            dialog::row(ui, LW, "", |ui| {
                ui.add_enabled_ui(self.span == Span::Custom, |ui| {
                    ui.label(tr("Start:"));
                    super::fields::number(ui, &mut self.start, 0.0..=last, 0, "", tr("Start"));
                    ui.label(tr("End:"));
                    super::fields::number(ui, &mut self.end, 0.0..=last, 0, "", tr("End"));
                });
                ui.label(format!(
                    "{} {}",
                    tr("Duration:"),
                    crate::common::duration_code(info, info.frame_rate.frame_to_time(e - s + 1))
                ));
            });
            ui.add_enabled_ui(false, |ui| {
                let _ = ui.radio(
                    true,
                    format!(
                        "{} ({:.2})",
                        tr("Use comp's frame rate"),
                        info.frame_rate.as_f64()
                    ),
                );
                let _ = ui.radio(false, tr("Use this frame rate:"));
            });
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new(tr("Options")).strong());
        ui.add_enabled_ui(false, |ui| {
            let mut skip = false;
            bae_ui::widgets::checkbox(
                ui,
                &mut skip,
                tr("Skip existing files (allows multi-machine rendering)"),
            );
        });
        ui.add_space(6.0);
        dialog::buttons(ui, true)
    }
}

/// The work area as comp frames (inclusive), as the queue adds items.
fn work_area(info: &CompInfo) -> (i64, i64) {
    let rate = info.frame_rate;
    let start = rate.time_to_frame(info.work_area.start);
    (
        start,
        (rate.time_to_frame(info.work_area.end) - 1).max(start),
    )
}

/// QuickTime is a container; the video codec is a separate output setting.
const QT_CODECS: [(ExportFormat, &str); 5] = [
    (ExportFormat::ProRes422, "Apple ProRes 422"),
    (ExportFormat::ProRes, "Apple ProRes 422 HQ"),
    (ExportFormat::ProRes4444, "Apple ProRes 4444"),
    (ExportFormat::QuickTimeH264, "H.264"),
    (ExportFormat::QuickTimeHevc, "HEVC"),
];

/// The open Output Module Settings dialog.
#[derive(Clone, Debug)]
pub struct OutputModule {
    item: u64,
    module: usize,
    format: ExportFormat,
    channels: Channels,
    bit_rate: f64,
    pcm_audio: bool,
}

impl OutputModule {
    pub(crate) fn new(queue: &SharedRenderQueue, item: u64, module: usize) -> Option<Self> {
        let q = queue.lock();
        let it = q.items().iter().find(|i| i.id == item)?;
        let it = it.outputs.get(module)?;
        Some(OutputModule {
            item,
            module,
            format: it.format,
            channels: it.channels,
            bit_rate: it.bit_rate_mbps,
            pcm_audio: it.pcm_audio,
        })
    }

    pub(crate) fn ui(&mut self, ctx: &egui::Context, cx: &PanelContext<'_>) -> bool {
        let id = Id::new("bae-ui-motion.output-module");
        let shown = dialog::window(ctx, id, tr("Output Module Settings"), |ui| {
            self.body(ui, id)
        });
        match shown {
            Some(Outcome::Ok) => {
                let queue = render_queue::shared(cx.resources);
                if let Some(it) = queue
                    .lock()
                    .item_mut(self.item)
                    .and_then(|i| i.outputs.get_mut(self.module))
                {
                    if it.format != self.format {
                        it.set_format(self.format);
                    }
                    it.channels = if render_queue::supports_alpha(self.format) {
                        self.channels
                    } else {
                        Channels::Rgb
                    };
                    it.pcm_audio = self.pcm_audio;
                    it.bit_rate_mbps = self.bit_rate;
                }
                false
            }
            Some(Outcome::Cancel) => false,
            Some(Outcome::None) => true,
            None => !dialog::closed_by_title_bar(ctx, id),
        }
    }

    fn body(&mut self, ui: &mut Ui, id: Id) -> Outcome {
        ui.set_width(520.0);
        dialog::row(ui, LW, tr("Format:"), |ui| {
            let available: Vec<ExportFormat> = FORMATS
                .iter()
                .copied()
                .filter(|f| {
                    render_queue::ui_format_available(*f)
                        && (!f.is_quicktime() || *f == ExportFormat::ProRes)
                })
                .collect();
            let names: Vec<&str> = available
                .iter()
                .map(|f| {
                    if f.is_quicktime() {
                        "QuickTime"
                    } else {
                        tr(render_queue::format_label(*f))
                    }
                })
                .collect();
            let mut i = available
                .iter()
                .position(|f| *f == self.format || (f.is_quicktime() && self.format.is_quicktime()))
                .unwrap_or(0);
            if bae_ui::widgets::dropdown(ui, "om-format", &mut i, &names, 220.0).changed() {
                self.format = available[i];
                self.channels = render_queue::default_channels(self.format);
            }
        });
        fixed(ui, "Post-Render Action:", "None");
        ui.add_space(4.0);
        ui.label(egui::RichText::new(tr("Video Output")).strong());
        ui.indent(id.with("video"), |ui| {
            if self.format.is_quicktime() {
                dialog::row(ui, LW, tr("Video Codec"), |ui| {
                    let names = QT_CODECS.map(|(_, name)| name);
                    let mut i = QT_CODECS
                        .iter()
                        .position(|(f, _)| *f == self.format)
                        .unwrap_or(1);
                    if bae_ui::widgets::dropdown(ui, "om-qt-codec", &mut i, &names, 220.0).changed()
                    {
                        self.format = QT_CODECS[i].0;
                        self.channels = render_queue::default_channels(self.format);
                    }
                });
            }
            dialog::row(ui, LW, tr("Channels:"), |ui| {
                let alpha = render_queue::supports_alpha(self.format);
                let options = if alpha {
                    vec![Channels::Rgb, Channels::RgbAlpha]
                } else {
                    vec![Channels::Rgb]
                };
                let names: Vec<&str> = options
                    .iter()
                    .map(|c| render_queue::channels_label(*c))
                    .collect();
                let mut i = options
                    .iter()
                    .position(|c| *c == self.channels)
                    .unwrap_or(0);
                if bae_ui::widgets::dropdown(ui, "om-channels", &mut i, &names, 160.0).changed() {
                    self.channels = options[i];
                }
            });
            fixed(ui, "Depth:", "Millions of Colors");
            fixed(ui, "Color:", "Premultiplied (Matted)");
            if render_queue::has_bit_rate(self.format) {
                dialog::row(ui, LW, tr("Bitrate:"), |ui| {
                    super::fields::number(
                        ui,
                        &mut self.bit_rate,
                        0.1..=800.0,
                        1,
                        "Mbps",
                        tr("Bitrate"),
                    );
                });
            }
        });
        ui.add_enabled_ui(false, |ui| {
            let mut off = false;
            bae_ui::widgets::checkbox(ui, &mut off, tr("Resize"));
            bae_ui::widgets::checkbox(ui, &mut off, tr("Crop"));
        });
        if self.format.is_quicktime() {
            dialog::row(ui, LW, tr("Audio Output"), |ui| {
                let mut i = usize::from(!self.pcm_audio);
                if bae_ui::widgets::dropdown(
                    ui,
                    "om-audio-codec",
                    &mut i,
                    &[tr("24-bit PCM"), "AAC"],
                    170.0,
                )
                .changed()
                {
                    self.pcm_audio = i == 0;
                }
            });
        } else {
            fixed(ui, "Audio Output", "Audio Output Auto");
        }
        ui.add_space(6.0);
        dialog::buttons(ui, true)
    }
}

/// English strings of these dialogs (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "Render Settings",
    "Quality:",
    "Best",
    "Resolution:",
    "Full",
    "Size:",
    "Disk Cache:",
    "Read Only",
    "Proxy Use:",
    "Use No Proxies",
    "Effects:",
    "Current Settings",
    "Solo Switches:",
    "Guide Layers:",
    "All Off",
    "Color Depth:",
    "Time Sampling",
    "Frame Blending:",
    "On for Checked Layers",
    "Field Render:",
    "Off",
    "Motion Blur:",
    "Time Span:",
    "Length of Comp",
    "Work Area Only",
    "Custom...",
    "Start:",
    "End:",
    "Duration:",
    "Use comp's frame rate",
    "Use this frame rate:",
    "Options",
    "Skip existing files (allows multi-machine rendering)",
    "Output Module Settings",
    "Format:",
    "Post-Render Action:",
    "None",
    "Video Output",
    "Channels:",
    "Depth:",
    "Millions of Colors",
    "Color:",
    "Premultiplied (Matted)",
    "Bitrate:",
    "Resize",
    "Crop",
    "Audio Output",
    "Audio Output Auto",
];
