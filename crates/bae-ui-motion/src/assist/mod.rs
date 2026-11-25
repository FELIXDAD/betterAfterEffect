//! Keyframe assistant panels of the Window menu.

pub(crate) mod mask_interpolation;

/// Chinese strings of the keyframe assistants (AE 26's terms).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Mask Interpolation", "蒙版插值"),
    ("Keyframe Rate:", "关键帧速率:"),
    ("per second", "每秒"),
    ("Keyframe Fields (doubles rate)", "“关键帧”字段（双重比率）"),
    ("Use “Linear” Vertex Paths", "使用“线性”顶点路径"),
    ("Bending Resistance:", "抗弯强度:"),
    ("Quality:", "品质:"),
    ("Add Mask Path Vertices", "添加蒙版路径顶点"),
    ("Pixels Between Vertices", "顶点之间的像素"),
    ("Total Vertices", "总顶点数"),
    ("Percentage of Outline", "轮廓的百分比"),
    ("Matching Method:", "配合法:"),
    ("Curve", "曲线"),
    ("Polyline", "多角线"),
    ("Use 1:1 Vertex Matches", "使用 1:1 顶点匹配"),
    ("First Vertices Match", "第一顶点匹配"),
    ("Apply", "应用"),
    (
        "Please select at least 2 consecutive mask path keyframes to interpolate between.",
        "请选择至少 2 个连续蒙版路径关键帧以在其间插值。",
    ),
];

/// Registers the assistants' Chinese strings and panels (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    crate::register_panel(
        app,
        "mask_interpolation",
        bae_ui::PanelKind::new("Mask Interpolation", |_| {
            Box::new(mask_interpolation::MaskInterpolationPanel::default())
        }),
    );
}
