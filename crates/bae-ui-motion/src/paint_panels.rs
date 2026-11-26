//! The Paint and Brushes panels (AE-UI-104): settings of the Brush, Clone
//! Stamp and Eraser tools for the next stroke, kept in the tool options
//! ([`crate::tools::PaintOptions`]). Also AE's Paint workspace.

use bae_model::PrimPath;
use bae_motion::query;
use bae_ui::theme::{self, color};
use bae_ui::{Panel, PanelContext, TabInfo, tr, widgets};
use egui::{Color32, Rect, Sense, Stroke, Ui, vec2};

use crate::common;
use crate::tools::{self, PaintOptions, Tool};

/// Blend modes of a stroke, in AE's Paint panel order (AE lists them all).
const MODES: &[(&str, &str)] = &[
    ("normal", "Normal"),
    ("darken", "Darken"),
    ("multiply", "Multiply"),
    ("color_burn", "Color Burn"),
    ("linear_burn", "Linear Burn"),
    ("add", "Add"),
    ("lighten", "Lighten"),
    ("screen", "Screen"),
    ("color_dodge", "Color Dodge"),
    ("linear_dodge", "Linear Dodge"),
    ("overlay", "Overlay"),
    ("soft_light", "Soft Light"),
    ("hard_light", "Hard Light"),
    ("difference", "Difference"),
    ("exclusion", "Exclusion"),
    ("hue", "Hue"),
    ("saturation", "Saturation"),
    ("color", "Color"),
    ("luminosity", "Luminosity"),
];
const CHANNELS: &[(&str, &str)] = &[("rgba", "RGBA"), ("rgb", "RGB"), ("alpha", "Alpha")];
const DURATIONS: &[(&str, &str)] = &[
    ("constant", "Constant"),
    ("write_on", "Write On"),
    ("single_frame", "Single Frame"),
    ("custom", "Custom"),
];
const ERASE: &[(&str, &str)] = &[
    ("layer_and_paint", "Layer Source & Paint"),
    ("paint", "Paint Only"),
    ("last_stroke", "Last Stroke Only"),
];
const DYNAMICS: [&str; 4] = ["Off", "Pen Pressure", "Pen Tilt", "Stylus Wheel"];
/// AE's default brush tips: diameter, hard.
const TIPS: &[(f64, bool)] = &[
    (1.0, true),
    (3.0, true),
    (5.0, true),
    (9.0, true),
    (13.0, true),
    (19.0, true),
    (5.0, false),
    (9.0, false),
    (13.0, false),
    (17.0, false),
    (21.0, false),
    (27.0, false),
    (35.0, false),
    (45.0, false),
    (65.0, false),
    (100.0, false),
    (200.0, false),
    (300.0, false),
];

/// A dropdown over `(token, English label)` pairs editing `value`.
fn choice(ui: &mut Ui, id: &str, value: &mut String, items: &[(&str, &str)], width: f32) {
    let labels: Vec<&str> = items.iter().map(|(_, l)| tr(l)).collect();
    let mut i = items.iter().position(|(t, _)| t == value).unwrap_or(0);
    if widgets::dropdown(ui, id, &mut i, &labels, width).changed() {
        *value = items[i].0.to_owned();
    }
}

fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [92.0, 18.0],
            egui::Label::new(
                egui::RichText::new(tr(label))
                    .font(theme::small())
                    .color(color::TEXT),
            ),
        );
        add(ui);
    });
}

fn percent(ui: &mut Ui, v: &mut f64) {
    ui.add(egui::DragValue::new(v).range(0.0..=100.0).suffix("%"));
}

fn color32(c: [f64; 3]) -> Color32 {
    let to = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(to(c[0]), to(c[1]), to(c[2]))
}

/// A color swatch that opens a picker.
fn swatch(ui: &Ui, id: &str, c: &mut [f64; 3], tip: &str, at: Rect) {
    let resp = ui
        .interact(at, ui.id().with(id), Sense::click())
        .on_hover_text(tr(tip));
    ui.painter().rect_filled(at, 0.0, color32(*c));
    ui.painter().rect_stroke(
        at,
        0.0,
        Stroke::new(1.0, Color32::from_gray(160)),
        egui::StrokeKind::Inside,
    );
    let picker = egui::Popup::from_response(&resp)
        .id(egui::Id::new(("paint_swatch", id)))
        .open_memory(resp.clicked().then_some(egui::SetOpenCommand::Toggle))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    picker.show(|ui| {
        let mut v = color32(*c);
        if egui::color_picker::color_picker_color32(ui, &mut v, egui::color_picker::Alpha::Opaque) {
            *c = [v.r(), v.g(), v.b()].map(|x| f64::from(x) / 255.0);
        }
    });
}

/// Edit the tool options' paint settings with `f`; changes are kept.
fn with_paint(cx: &PanelContext<'_>, f: impl FnOnce(&mut PaintOptions, Tool)) {
    let shared = tools::shared(cx.resources);
    let (mut paint, tool) = {
        let t = shared.lock();
        (t.options.paint.clone(), t.current())
    };
    let before = paint.clone();
    f(&mut paint, tool);
    if paint != before {
        shared.lock().options.paint = paint;
    }
}

#[derive(Default)]
pub struct PaintPanel;

impl Panel for PaintPanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        TabInfo::new(tr("Paint"))
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        let layers: Vec<(PrimPath, String)> = common::active_comp(cx.stage, &session)
            .map(|info| {
                query::comp_layers(cx.stage, &info.path)
                    .into_iter()
                    .map(|l| {
                        let name = query::display_name_of(cx.stage, &l);
                        (l, name)
                    })
                    .collect()
            })
            .unwrap_or_default();
        with_paint(cx, |p, tool| {
            ui.add_space(4.0);
            // Foreground / background colors, swap.
            let (rect, _) = ui.allocate_exact_size(vec2(56.0, 34.0), Sense::hover());
            let bg = Rect::from_min_size(rect.min + vec2(14.0, 12.0), vec2(20.0, 20.0));
            let fg = Rect::from_min_size(rect.min, vec2(20.0, 20.0));
            swatch(
                ui,
                "background",
                &mut p.background,
                "Set background color",
                bg,
            );
            swatch(
                ui,
                "foreground",
                &mut p.foreground,
                "Set foreground color",
                fg,
            );
            let flip = Rect::from_min_size(rect.min + vec2(38.0, 0.0), vec2(14.0, 12.0));
            if ui
                .interact(flip, ui.id().with("flip"), Sense::click())
                .on_hover_text(tr("Swap foreground and background colors"))
                .clicked()
            {
                std::mem::swap(&mut p.foreground, &mut p.background);
            }
            ui.painter().text(
                flip.center(),
                egui::Align2::CENTER_CENTER,
                "⇄",
                theme::small(),
                color::TEXT,
            );
            row(ui, "Opacity:", |ui| percent(ui, &mut p.opacity));
            row(ui, "Flow:", |ui| percent(ui, &mut p.flow));
            row(ui, "Mode:", |ui| {
                choice(ui, "paint_mode", &mut p.mode, MODES, 120.0)
            });
            row(ui, "Channels:", |ui| {
                choice(ui, "paint_channels", &mut p.channels, CHANNELS, 120.0);
            });
            row(ui, "Duration:", |ui| {
                choice(ui, "paint_duration", &mut p.duration, DURATIONS, 100.0);
                if p.duration == "custom" {
                    ui.add(
                        egui::DragValue::new(&mut p.frames)
                            .range(1..=99999)
                            .suffix(" f"),
                    );
                }
            });
            if tool == Tool::Eraser {
                row(ui, "Erase:", |ui| {
                    choice(ui, "paint_erase", &mut p.erase, ERASE, 140.0)
                });
            }
            if tool == Tool::CloneStamp {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(tr("Clone Options"))
                        .font(theme::small())
                        .color(color::TEXT_STRONG),
                );
                row(ui, "Source:", |ui| {
                    let mut names = vec![tr("Current Layer").to_owned()];
                    names.extend(layers.iter().map(|(_, n)| n.clone()));
                    let items: Vec<&str> = names.iter().map(String::as_str).collect();
                    let mut i = p
                        .clone_source
                        .as_ref()
                        .and_then(|s| layers.iter().position(|(l, _)| l == s))
                        .map_or(0, |i| i + 1);
                    if widgets::dropdown(ui, "clone_source", &mut i, &items, 140.0).changed() {
                        p.clone_source = (i > 0).then(|| layers[i - 1].0.clone());
                    }
                });
                widgets::checkbox(ui, &mut p.aligned, tr("Aligned"));
                widgets::checkbox(ui, &mut p.lock_source_time, tr("Lock Source Time"));
                row(ui, "Offset:", |ui| {
                    ui.add(egui::DragValue::new(&mut p.clone_offset[0]).speed(1.0));
                    ui.add(egui::DragValue::new(&mut p.clone_offset[1]).speed(1.0));
                });
                // AE: Source Time when locked, else Source Time Shift.
                if p.lock_source_time {
                    row(ui, "Source Time:", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut p.clone_source_time)
                                .speed(0.04)
                                .range(0.0..=f64::MAX)
                                .suffix(" s"),
                        );
                    });
                } else {
                    row(ui, "Source Time Shift:", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut p.clone_time_shift)
                                .speed(0.04)
                                .suffix(" s"),
                        );
                    });
                }
            }
        });
    }
}

mod brushes;
pub(crate) use brushes::{BrushLibrary, BrushesPanel};

fn brush_controls(ui: &mut Ui, p: &mut PaintOptions) {
    row(ui, "Diameter:", |ui| {
        ui.add(
            egui::DragValue::new(&mut p.diameter)
                .range(1.0..=2500.0)
                .suffix(" px"),
        );
    });
    row(ui, "Angle:", |ui| {
        ui.add(
            egui::DragValue::new(&mut p.angle)
                .range(-180.0..=180.0)
                .suffix("°"),
        );
    });
    row(ui, "Roundness:", |ui| percent(ui, &mut p.roundness));
    row(ui, "Hardness:", |ui| percent(ui, &mut p.hardness));
    row(ui, "Spacing:", |ui| {
        ui.add(
            egui::DragValue::new(&mut p.spacing)
                .range(1.0..=1000.0)
                .suffix("%"),
        );
    });
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(tr("Brush Dynamics"))
            .font(theme::small())
            .color(color::TEXT_STRONG),
    );
    let labels: Vec<&str> = DYNAMICS.iter().map(|l| tr(l)).collect();
    for (k, name) in ["Size:", "Angle:", "Roundness:", "Opacity:", "Flow:"]
        .into_iter()
        .enumerate()
    {
        row(ui, name, |ui| {
            let mut i = usize::from(p.dynamics[k]).min(3);
            if widgets::dropdown(ui, format!("dynamics{k}"), &mut i, &labels, 110.0).changed() {
                p.dynamics[k] = i as u8;
            }
        });
        if k == 0 {
            row(ui, "Minimum Size:", |ui| percent(ui, &mut p.minimum_size));
        }
    }
}

/// The Paint and Brushes panels and their Chinese (AE's Paint workspace, in
/// `workspaces`, lays them out).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    crate::register_panel(
        app,
        "paint",
        bae_ui::PanelKind::new("Paint", |_| Box::new(PaintPanel)),
    );
    crate::register_panel(
        app,
        "brushes",
        bae_ui::PanelKind::new("Brushes", |_| Box::new(BrushesPanel::default())),
    );
}

/// Chinese of the Paint and Brushes panels (AE's zh_CN dictionary,
/// Paint_Palette / BrushPal), registered by the module.
pub(crate) const ZH: &[(&str, &str)] = &[
    ("New Brush...", "新建画笔..."),
    ("Rename Brush...", "重命名画笔..."),
    ("Delete Brush", "删除画笔"),
    ("Text Only", "仅文本"),
    ("Small Thumbnail", "小缩览图"),
    ("Large Thumbnail", "大缩览图"),
    ("Small List", "小列表"),
    ("Large List", "大列表"),
    ("Reset Brush Tips...", "重置画笔笔尖..."),
    (
        "Replace current brush tips with the default brush tips?",
        "是否将当前画笔笔尖替换为默认画笔笔尖？",
    ),
    ("New Brush Tip Name:", "新建画笔笔尖名称:"),
    ("Rename Brush Tip:", "重命名画笔笔尖:"),
    ("Delete the brush?", "是否删除画笔？"),
    ("Append", "追加"),
    ("Opacity:", "不透明度:"),
    ("Flow:", "流量:"),
    ("Mode:", "模式:"),
    ("Channels:", "通道:"),
    ("Erase:", "抹除:"),
    ("Clone Options", "仿制选项"),
    ("Source:", "源:"),
    ("Current Layer", "当前图层"),
    ("Aligned", "已对齐"),
    ("Lock Source Time", "锁定源时间"),
    ("Offset:", "偏移:"),
    ("Source Time Shift:", "源时间转移:"),
    ("Source Time:", "源时间:"),
    ("Set background color", "设定背景颜色"),
    ("Set foreground color", "设置前景颜色"),
    (
        "Swap foreground and background colors",
        "切换前景颜色和背景颜色",
    ),
    ("RGBA", "RGBA"),
    ("Alpha", "Alpha"),
    ("Constant", "固定"),
    ("Write On", "写入"),
    ("Single Frame", "单帧"),
    ("Layer Source & Paint", "图层源和绘画"),
    ("Paint Only", "仅绘画"),
    ("Last Stroke Only", "仅最后描边"),
    ("Pen Pressure", "笔头压力"),
    ("Pen Tilt", "笔倾斜"),
    ("Stylus Wheel", "笔尖转动"),
    ("Hard Elliptical", "实边椭圆"),
    ("Soft Elliptical", "柔边椭圆"),
    ("Hard Round", "尖角"),
    ("Soft Round", "柔角"),
    ("Diameter:", "直径:"),
    ("Angle:", "角度:"),
    ("Roundness:", "圆度:"),
    ("Hardness:", "硬度:"),
    ("Spacing:", "间距:"),
    ("Brush Dynamics", "画笔动态"),
    ("Size:", "大小:"),
    ("Minimum Size:", "最小大小:"),
    (
        "Paint on layers in the Layer panel with the Brush, Clone Stamp and Eraser.",
        "在图层面板里用画笔、仿制图章和橡皮擦在图层上绘画。",
    ),
];
