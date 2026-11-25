//! AE's small panels of the Motion workspace (Window menu): Info, and next
//! Align, Character and Paragraph. Their Window menu items and shortcuts
//! (Cmd/Ctrl+2 …) work once the panels are registered (`late_bindings`).

pub mod info;

use bae_ui::PanelKind;

/// The panels, by id, replacing the shell's placeholders.
pub(crate) fn panels() -> Vec<(&'static str, PanelKind)> {
    vec![(
        "info",
        PanelKind::new("Info", |spawn| {
            Box::new(info::InfoPanel::from_state(spawn.state))
        }),
    )]
}

/// Chinese for these panels (AE's zh_CN dictionary).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Auto Color Display", "自动颜色显示"),
    ("Percent", "百分比"),
    ("Web", "Web"),
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
];
