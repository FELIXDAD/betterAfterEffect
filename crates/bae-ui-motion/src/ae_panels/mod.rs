//! AE's small panels of the Motion workspace (Window menu): Info, and next
//! Align, Character and Paragraph. Their Window menu items and shortcuts
//! (Cmd/Ctrl+2 …) work once the panels are registered (`late_bindings`).

pub mod align;
pub mod info;

use bae_ui::PanelKind;

/// The panels, by id, replacing the shell's placeholders.
/// Registers the panels (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    for (id, kind) in panels() {
        crate::register_panel(app, id, kind);
    }
}

pub(crate) fn panels() -> Vec<(&'static str, PanelKind)> {
    vec![
        (
            "info",
            PanelKind::new("Info", |spawn| {
                Box::new(info::InfoPanel::from_state(spawn.state))
            }),
        ),
        (
            "align",
            PanelKind::new("Align", |spawn| {
                Box::new(align::AlignPanel::from_state(spawn.state))
            }),
        ),
    ]
}

/// Chinese for these panels (AE's zh_CN dictionary).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Auto Color Display", "自动颜色显示"),
    ("Percent (0-100)", "百分比 (0-100)"),
    ("Web (00-FF)", "Web (00-FF)"),
    ("HSB", "HSB"),
    ("8-bpc (0-255)", "8-bpc (0-255)"),
    ("10-bpc (0-1023)", "10-bpc (0-1023)"),
    ("16-bpc (0-32768)", "16-bpc (0-32768)"),
    ("Decimal (0.0-1.0)", "小数 (0.0-1.0)"),
    (", ", "、"),
    ("Position:", "位置:"),
    ("\u{394}:", "\u{25b3}:"),
    ("Duration: {d}", "持续时间: {d}"),
    ("In: {in}, Out: {out}", "入: {in}，出: {out}"),
    // Align ($$$/AE/Plugin/Align/*).
    ("Align Layers to:", "将图层对齐到:"),
    ("Selection", "选区"),
    ("Composition", "合成"),
    ("Align Options", "对齐选项"),
    ("Distribute Layers:", "分布图层:"),
    ("Align Left", "左对齐"),
    ("Align Horizontally", "水平对齐"),
    ("Align Right", "右对齐"),
    ("Align Top", "顶对齐"),
    ("Align Vertically", "垂直对齐"),
    ("Align Bottom", "底对齐"),
    ("Distribute Top", "按顶分布"),
    ("Distribute Vertically", "垂直均匀分布"),
    ("Distribute Bottom", "按底分布"),
    ("Distribute Left", "按左分布"),
    ("Distribute Horizontally", "水平均匀分布"),
    ("Distribute Right", "按右分布"),
];
