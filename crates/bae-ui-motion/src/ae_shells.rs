//! Outlines of AE 26's Window-menu panels BAE does not implement yet: each
//! panel opens from Window (and its AE shortcut) and shows AE's controls in
//! AE's arrangement, greyed, so the workspaces look like AE's. A module that
//! implements a panel registers its id first and replaces the outline.

use bae_ui::{Panel, PanelContext, PanelKind, tr};
use egui::{Align2, Sense, Ui, vec2};

use bae_ui::theme::{self, color};

/// One line of an outline.
#[derive(Clone, Copy)]
enum Row {
    /// A label and a control on one line.
    Pair(&'static str, Control),
    /// Controls side by side.
    Line(&'static [Control]),
    /// Section title.
    Title(&'static str),
    /// Dim text.
    Note(&'static str),
    /// Search field with a hint.
    Search(&'static str),
    Sep,
    /// The rest of the panel: an empty viewer / list with a dim caption.
    Area(&'static str),
}

#[derive(Clone, Copy)]
enum Control {
    Dropdown(&'static str),
    Button(&'static str),
    Check(&'static str, bool),
    /// Blue hot text (a value).
    Value(&'static str),
    Text(&'static str),
}

use Control::*;
use Row::*;

/// `(id, English title, rows)` of every outline, in Window-menu order.
const SHELLS: &[(&str, &str, &[Row])] = &[
    (
        "lumetri_scopes",
        "Lumetri Scopes",
        &[
            Area(""),
            Line(&[Dropdown("Presets"), Dropdown("8 Bit"), Dropdown("Rec. 709")]),
        ],
    ),
    (
        "metadata",
        "Metadata",
        &[
            Search("Search"),
            Title("File Properties"),
            Title("Dynamic Media"),
            Title("Basic"),
            Area(""),
        ],
    ),
    (
        "content_aware_fill",
        "Content-Aware Fill",
        &[
            Title("Fill Target"),
            Area(""),
            Pair("Alpha Expansion", Value("0")),
            Pair("Fill Method", Dropdown("Object")),
            Pair("Lighting Correction", Dropdown("None")),
            Pair("Range", Dropdown("Work Area")),
            Line(&[Button("Create Reference Frame")]),
            Line(&[Button("Generate Fill Layer")]),
        ],
    ),
    (
        "motion_sketch",
        "Motion Sketch",
        &[
            Pair("Capture speed at:", Value("100 %")),
            Pair("Smoothing:", Value("1")),
            Note("Show:"),
            Line(&[Check("Wireframe", false), Check("Background", false)]),
            Pair("Start:", Text("0:00:00:00")),
            Pair("Duration:", Text("0:00:00:00")),
            Line(&[Button("Start Capture")]),
        ],
    ),
    (
        "essential_graphics",
        "Essential Graphics",
        &[
            Pair("Primary:", Dropdown("Select a Composition")),
            Line(&[Button("Solo Supported Properties")]),
            Area("Drag properties from the Timeline panel here."),
            Line(&[Button("Export Motion Graphics Template...")]),
        ],
    ),
    (
        "media_browser",
        "Media Browser",
        &[Search("Search"), Area("")],
    ),
    ("learn", "Learn", &[Area("")]),
    (
        "smoother",
        "Smoother",
        &[
            Pair("Apply To:", Dropdown("Temporal Graph")),
            Pair("Tolerance:", Value("1")),
            Line(&[Button("Apply")]),
        ],
    ),
    ("libraries", "Libraries", &[Search("Search"), Area("")]),
    // Review with Frame.io needs an Adobe account: its outline, greyed.
    ("frame_io", "Review with Frame.io", &[Area("")]),
    (
        "wiggler",
        "Wiggler",
        &[
            Pair("Apply To:", Dropdown("Spatial Path")),
            Pair("Noise Type:", Dropdown("Smooth")),
            Pair("Dimensions:", Dropdown("One Dimension")),
            Pair("Frequency:", Value("5.0 per second")),
            Pair("Magnitude:", Value("1.0")),
            Line(&[Button("Apply")]),
        ],
    ),
    (
        "brushes",
        "Brushes",
        &[
            Area(""),
            Pair("Diameter:", Value("21 px")),
            Pair("Angle:", Value("0°")),
            Pair("Roundness:", Value("100%")),
            Pair("Hardness:", Value("0%")),
            Pair("Spacing:", Value("25%")),
        ],
    ),
    (
        "paint",
        "Paint",
        &[
            Pair("Opacity:", Value("100 %")),
            Pair("Flow:", Value("100 %")),
            Pair("Mode:", Dropdown("Normal")),
            Pair("Channels:", Dropdown("RGBA")),
            Pair("Duration:", Dropdown("Constant")),
            Sep,
            Pair("Erase:", Dropdown("Layer Source & Paint")),
            Sep,
            Title("Clone Options"),
            Pair("Preset:", Dropdown("1")),
            Pair("Source:", Dropdown("Current Layer")),
            Line(&[Check("Aligned", true), Check("Lock Source Time", false)]),
        ],
    ),
    (
        "mask_interpolation",
        "Mask Interpolation",
        &[
            Line(&[Text("Keyframe Rate:"), Dropdown("Auto"), Text("per second")]),
            Line(&[Check("Keyframe Fields (doubles rate)", false)]),
            Line(&[Check("Use \"Linear\" Vertex Paths", false)]),
            Sep,
            Pair("Bending Resistance:", Value("50")),
            Pair("Quality:", Value("50")),
            Sep,
            Line(&[Check("Add Mask Path Vertices", true)]),
            Line(&[Value("5"), Dropdown("Pixels Between Vertices")]),
            Sep,
            Pair("Matching Method:", Dropdown("Auto")),
            Line(&[Check("Use 1:1 Vertex Matches", false)]),
            Line(&[Check("First Vertices Match", true)]),
            Sep,
            Line(&[Button("Apply")]),
        ],
    ),
    ("progress", "Progress", &[Area("")]),
    (
        "layer",
        "Layer",
        &[
            Area(""),
            Line(&[Text("View:"), Dropdown("Masks"), Check("Render", true)]),
        ],
    ),
    ("flowchart", "Flowchart", &[Area("")]),
    ("footage", "Footage", &[Area("")]),
];

/// Registers the outline of every panel no module registered (call last in
/// `build`, after the real panels).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    let owner = app.current_owner();
    let kinds = app.registry::<PanelKind>();
    kinds.batch(|tx| {
        for (id, title, rows) in SHELLS {
            if tx.get(id).is_some_and(|k| !k.is_placeholder) {
                continue;
            }
            let kind = PanelKind::new(*title, move |_| Box::new(Shell { rows }));
            let _ = tx.replace(owner.clone(), *id, std::sync::Arc::new(kind));
        }
    });
}

struct Shell {
    rows: &'static [Row],
}

impl Panel for Shell {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut PanelContext<'_>) {
        ui.add_space(6.0);
        ui.add_enabled_ui(false, |ui| {
            for row in self.rows {
                draw_row(ui, row);
            }
        });
    }
}

fn draw_row(ui: &mut Ui, row: &Row) {
    match *row {
        Pair(label, c) => {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(tr(label));
                control(ui, c);
            });
        }
        Line(cs) => {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                for c in cs {
                    control(ui, *c);
                }
            });
        }
        Title(t) => {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(tr(t)).color(color::TEXT_STRONG));
            });
        }
        Note(t) => {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(tr(t)).color(color::TEXT_DIM));
            });
        }
        Search(hint) => {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let mut text = String::new();
                ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .hint_text(tr(hint))
                        .desired_width(ui.available_width() - 8.0),
                );
            });
        }
        Sep => {
            ui.separator();
        }
        Area(caption) => {
            let h = (ui.available_height() - 60.0).max(80.0);
            let w = ui.available_width() - 16.0;
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
                let p = ui.painter();
                p.rect_filled(rect, 2.0, color::FIELD);
                if !caption.is_empty() {
                    p.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        tr(caption),
                        theme::body(),
                        color::TEXT_DIM,
                    );
                }
            });
        }
    }
}

fn control(ui: &mut Ui, c: Control) {
    match c {
        Dropdown(v) => {
            let mut i = 0;
            bae_ui::widgets::dropdown(ui, v, &mut i, &[tr(v)], 130.0);
        }
        Button(t) => {
            let _ = ui.button(tr(t));
        }
        Check(t, on) => {
            let mut on = on;
            bae_ui::widgets::checkbox(ui, &mut on, tr(t));
        }
        Value(v) => {
            ui.label(egui::RichText::new(tr(v)).color(color::ACCENT_DIM));
        }
        Text(t) => {
            ui.label(tr(t));
        }
    }
}

/// Window ▸ Tools: AE's toolbar as a panel (the tools and their options; the
/// current tool is the toolbar's).
pub(crate) struct ToolsPanel {
    toolbar: crate::tools::toolbar::AeToolbar,
    keymap: bae_ui::Keymap,
}

impl ToolsPanel {
    pub(crate) fn new(tools: crate::tools::SharedTools) -> Self {
        ToolsPanel {
            toolbar: crate::tools::toolbar::AeToolbar::new(tools),
            keymap: bae_ui::presets::ae_keymap(),
        }
    }
}

impl Panel for ToolsPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        use bae_ui::Toolbar as _;
        ui.horizontal_wrapped(|ui| self.toolbar.ui(ui, cx, &self.keymap));
    }
}

/// Chinese for the outlines (AE 26's zh_CN dictionary).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Presets", "预设"),
    ("8 Bit", "8 位"),
    ("File Properties", "文件属性"),
    ("Dynamic Media", "动态媒体"),
    ("Basic", "基本"),
    ("Fill Target", "填充目标"),
    ("Alpha Expansion", "Alpha 扩展"),
    ("Fill Method", "填充方法"),
    ("Object", "对象"),
    ("Lighting Correction", "光照校正"),
    ("Range", "范围"),
    ("Work Area", "工作区域"),
    ("Create Reference Frame", "创建参照帧"),
    ("Generate Fill Layer", "生成填充图层"),
    ("Capture speed at:", "捕捉速度为:"),
    ("Smoothing:", "平滑:"),
    ("Show:", "显示:"),
    ("Wireframe", "线框"),
    ("Background", "背景"),
    ("Start:", "开始:"),
    ("Duration:", "持续时间:"),
    ("Start Capture", "开始捕捉"),
    ("Primary:", "主要内容："),
    ("Select a Composition", "选择合成"),
    ("Solo Supported Properties", "单独支持的属性"),
    (
        "Drag properties from the Timeline panel here.",
        "将属性从时间轴面板拖到此处。",
    ),
    ("Export Motion Graphics Template...", "导出动态图形模板..."),
    ("Apply To:", "应用到:"),
    ("Temporal Graph", "时间图表"),
    ("Spatial Path", "空间路径"),
    ("Tolerance:", "容差:"),
    ("Apply", "应用"),
    ("Noise Type:", "杂色类型:"),
    ("Smooth", "平滑"),
    ("Dimensions:", "尺寸:"),
    ("One Dimension", "一维"),
    ("Frequency:", "频率:"),
    ("5.0 per second", "5.0 每秒"),
    ("Magnitude:", "数量级:"),
    ("Diameter:", "直径:"),
    ("Angle:", "角度:"),
    ("Roundness:", "圆度:"),
    ("Hardness:", "硬度:"),
    ("Spacing:", "间距:"),
    ("21 px", "21 像素"),
    ("Opacity:", "不透明:"),
    ("Flow:", "流量:"),
    ("Mode:", "模式:"),
    ("Channels:", "通道:"),
    ("Constant", "固定"),
    ("Erase:", "抹除:"),
    ("Layer Source & Paint", "图层源和绘画"),
    ("Clone Options", "仿制选项"),
    ("Preset:", "预设:"),
    ("Source:", "源:"),
    ("Current Layer", "当前图层"),
    ("Aligned", "已对齐"),
    ("Lock Source Time", "锁定源时间"),
    ("Keyframe Rate:", "关键帧速率:"),
    ("per second", "每秒"),
    ("Keyframe Fields (doubles rate)", "“关键帧”字段（双重比率）"),
    ("Use \"Linear\" Vertex Paths", "使用“线性”顶点路径"),
    ("Bending Resistance:", "抗弯强度:"),
    ("Quality:", "品质:"),
    ("Add Mask Path Vertices", "添加蒙版路径顶点"),
    ("Pixels Between Vertices", "顶点之间的像素"),
    ("Matching Method:", "配合法:"),
    ("Use 1:1 Vertex Matches", "使用 1:1 顶点匹配"),
    ("First Vertices Match", "第一顶点匹配"),
    ("Track Camera", "跟踪摄像机"),
    ("Warp Stabilizer", "变形稳定器"),
    ("Track Motion", "跟踪运动"),
    ("Stabilize Motion", "稳定运动"),
    ("Motion Source:", "运动源:"),
    ("Current Track:", "当前跟踪:"),
    ("Track Type:", "跟踪类型:"),
    ("Motion Target:", "运动目标:"),
    ("Edit Target...", "编辑目标..."),
    ("Options...", "选项..."),
    ("Analyze:", "分析:"),
    ("View:", "视图:"),
    ("Masks", "蒙版"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_window_menu_panel_has_a_panel() {
        let ids: Vec<&str> = SHELLS.iter().map(|(id, _, _)| *id).collect();
        for (_, item) in crate::catalog::menu_items() {
            if item.action != "window.panel" {
                continue;
            }
            let params: serde_json::Value = serde_json::from_str(item.params).unwrap();
            let id = params["id"].as_str().unwrap();
            let elsewhere = matches!(
                id,
                "info"
                    | "audio"
                    | "character"
                    | "align"
                    | "tracker"
                    | "create_nulls_from_paths"
                    | "vr_comp_editor"
                    | "paragraph"
                    | "properties"
                    | "effects"
                    | "preview"
                    | "tools"
                    | "composition"
                    | "effect_controls"
                    | "timeline"
                    | "render_queue"
                    | "project"
                    | "frame_io"
            );
            assert!(elsewhere || ids.contains(&id), "no panel for {id}");
        }
    }
}
