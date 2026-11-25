//! How the viewer shows the comp, display only (AE's viewer bar): Show
//! Channel (RGB, Red, Green, Blue, Alpha, RGB Straight; Colorize) with
//! Alt/Option+1…4 and Alt/Option+Shift+1…4, Exposure with its reset, and the
//! Region of Interest (only that part of the comp is rendered; drawn by
//! dragging in the viewer while the button is on).

use bae_render::IRect;

/// The show-channel shortcuts: `(params, English name)` (AE's names).
pub(crate) const CHANNEL_SHORTCUTS: &[(&str, &str)] = &[
    (
        r#"{"channel":"red","colorize":false}"#,
        "Show/Hide Red Channel (Grayscale)",
    ),
    (
        r#"{"channel":"green","colorize":false}"#,
        "Show/Hide Green Channel (Grayscale)",
    ),
    (
        r#"{"channel":"blue","colorize":false}"#,
        "Show/Hide Blue Channel (Grayscale)",
    ),
    (
        r#"{"channel":"alpha","colorize":false}"#,
        "Show/Hide Alpha Channel (Grayscale)",
    ),
    (
        r#"{"channel":"red","colorize":true}"#,
        "Show/Hide Red Channel (Colorized)",
    ),
    (
        r#"{"channel":"green","colorize":true}"#,
        "Show/Hide Green Channel (Colorized)",
    ),
    (
        r#"{"channel":"blue","colorize":true}"#,
        "Show/Hide Blue Channel (Colorized)",
    ),
    // AE: Alt+Shift+4 is RGB Straight.
    (
        r#"{"channel":"alpha","colorize":true}"#,
        "Show/Hide RGB Straight",
    ),
    // Roto Brush views (not shown yet).
    (
        r#"{"channel":"alpha_boundary","colorize":false}"#,
        "Toggle Alpha Boundary",
    ),
    (
        r#"{"channel":"alpha_overlay","colorize":false}"#,
        "Toggle Alpha Overlay",
    ),
];

pub(crate) const SHOW_CHANNEL: &str = "motion.viewer.show_channel";
/// `{"mode": "off" | "adaptive_resolution" | "wireframe" | "draft_3d"}`.
pub(crate) const FAST_PREVIEWS: &str = "motion.viewer.fast_previews";

/// Fast Previews (AE's viewer bar): what the viewer does while something is
/// dragged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Fast {
    /// Off (Final Quality).
    #[default]
    Off,
    /// A lower resolution while dragging (at most ¼).
    Adaptive,
    /// Layer outlines while dragging.
    Wireframe,
}

impl Fast {
    pub const ALL: [(Fast, &'static str); 3] = [
        (Fast::Off, "Off (Final Quality)"),
        (Fast::Adaptive, "Adaptive Resolution"),
        (Fast::Wireframe, "Wireframe"),
    ];

    pub fn parse(mode: &str) -> Option<Fast> {
        Some(match mode {
            "off" => Fast::Off,
            "adaptive_resolution" => Fast::Adaptive,
            "wireframe" => Fast::Wireframe,
            _ => return None,
        })
    }
}

/// The Fast Previews shortcuts' names (AE's).
pub(crate) const FAST_SHORTCUTS: &[(&str, &str)] = &[
    (
        r#"{"mode":"off"}"#,
        "Switch Fast Previews to Off (Final Quality)",
    ),
    (
        r#"{"mode":"adaptive_resolution"}"#,
        "Switch Fast Previews to Adaptive Resolution",
    ),
    (r#"{"mode":"draft_3d"}"#, "Enable Draft 3D"),
    (
        r#"{"mode":"wireframe"}"#,
        "Switch Fast Previews to Wireframe",
    ),
];
pub(crate) const TAKE_SNAPSHOT: &str = "motion.viewer.take_snapshot";
pub(crate) const SHOW_SNAPSHOT: &str = "motion.viewer.show_snapshot";
pub(crate) const PURGE_SNAPSHOT: &str = "motion.viewer.purge_snapshot";

/// The snapshot shortcuts' names (AE's), slots 1–4.
pub(crate) fn snapshot_names() -> Vec<(&'static str, serde_json::Value, String)> {
    let mut out = Vec::new();
    for slot in 1..=4 {
        for (id, name) in [
            (TAKE_SNAPSHOT, "Take Snapshot"),
            (SHOW_SNAPSHOT, "Show Snapshot"),
            (PURGE_SNAPSHOT, "Purge Snapshot"),
        ] {
            out.push((
                id,
                serde_json::json!({ "slot": slot }),
                format!("{name} {slot}"),
            ));
        }
    }
    out
}

/// Chinese of the viewer's display controls and shortcuts (AE's zh_CN
/// dictionary: ItemButtonBar, CompItemButtonBar, KB/S/CompositionPanel-Views).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Select", "选择"),
    ("Show Channel", "显示通道"),
    (
        "Show Channel and Color Management Settings",
        "显示通道和色彩管理设置",
    ),
    ("RGB", "RGB"),
    ("Red", "红色"),
    ("Green", "绿色"),
    ("Blue", "蓝色"),
    ("Alpha", "Alpha"),
    ("RGB Straight", "RGB 直接"),
    ("Colorize", "彩色化"),
    ("Adjust Exposure", "调整曝光度"),
    ("Reset Exposure", "重置曝光度"),
    ("Region of Interest", "目标区域"),
    (
        "Toggle Mask and Shape Path Visibility",
        "切换蒙版和形状路径可见性",
    ),
    ("Choose grid and guide options", "选择网格和参考线选项"),
    ("Proportional Grid", "对称网格"),
    ("Rulers", "标尺"),
    ("Guides", "参考线"),
    ("Take Snapshot", "拍摄快照"),
    ("Show Snapshot", "显示快照"),
    (
        "Show/Hide Red Channel (Grayscale)",
        "显示/隐藏红色通道（灰度）",
    ),
    (
        "Show/Hide Green Channel (Grayscale)",
        "显示/隐藏绿色通道（灰度）",
    ),
    (
        "Show/Hide Blue Channel (Grayscale)",
        "显示/隐藏蓝色通道（灰度）",
    ),
    (
        "Show/Hide Alpha Channel (Grayscale)",
        "显示/隐藏 Alpha 通道（灰度）",
    ),
    (
        "Show/Hide Red Channel (Colorized)",
        "显示/隐藏红色通道（彩色）",
    ),
    (
        "Show/Hide Green Channel (Colorized)",
        "显示/隐藏绿色通道（彩色）",
    ),
    (
        "Show/Hide Blue Channel (Colorized)",
        "显示/隐藏蓝色通道（彩色）",
    ),
    ("Show/Hide RGB Straight", "显示/隐藏 RGB 连续颜色"),
    ("Take Snapshot 1", "拍快照 1"),
    ("Take Snapshot 2", "拍快照 2"),
    ("Take Snapshot 3", "拍快照 3"),
    ("Take Snapshot 4", "拍快照 4"),
    ("Show Snapshot 1", "显示快照 1"),
    ("Show Snapshot 2", "显示快照 2"),
    ("Show Snapshot 3", "显示快照 3"),
    ("Show Snapshot 4", "显示快照 4"),
    ("Purge Snapshot 1", "清理快照 1"),
    ("Purge Snapshot 2", "清理快照 2"),
    ("Purge Snapshot 3", "清理快照 3"),
    ("Purge Snapshot 4", "清理快照 4"),
    ("Purge Snapshot", "清理快照"),
    ("Fast Previews", "快速预览"),
    ("Off (Final Quality)", "关（最终品质）"),
    ("Adaptive Resolution", "自适应分辨率"),
    ("Wireframe", "线框"),
    (
        "Switch Fast Previews to Off (Final Quality)",
        "关闭快速预览（最终质量）",
    ),
    (
        "Switch Fast Previews to Adaptive Resolution",
        "将快速预览切换为自适应分辨率",
    ),
    ("Switch Fast Previews to Wireframe", "将快速预览切换为线框"),
    ("Enable Draft 3D", "启用草图 3D"),
    ("Show Layer Controls", "显示图层控件"),
    ("Toggle Alpha Boundary", "切换 Alpha 边界"),
    ("Toggle Alpha Overlay", "切换 Alpha 叠加"),
];

/// The Show Channel popup's entries, in AE's order.
pub(super) const CHANNELS: [&str; 6] = ["RGB", "Red", "Green", "Blue", "Alpha", "RGB Straight"];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Display {
    pub fast: Fast,
    /// Index into [`CHANNELS`].
    pub channel: u32,
    pub colorize: bool,
    /// Stops.
    pub exposure: f32,
    pub roi_on: bool,
    /// The region, comp pixels.
    pub roi: Option<IRect>,
}

impl Display {
    /// A show-channel shortcut: that channel, or back to RGB when it shows.
    pub fn shortcut(&mut self, params: &serde_json::Value) {
        let colorize = params.get("colorize").and_then(|v| v.as_bool()) == Some(true);
        let (channel, colorize) = match params.get("channel").and_then(|v| v.as_str()) {
            Some("red") => (1, colorize),
            Some("green") => (2, colorize),
            Some("blue") => (3, colorize),
            Some("alpha") if colorize => (5, false),
            Some("alpha") => (4, false),
            // Alpha Boundary / Overlay are Roto Brush views.
            _ => return,
        };
        if (self.channel, self.colorize) == (channel, colorize) {
            (self.channel, self.colorize) = (0, false);
        } else {
            (self.channel, self.colorize) = (channel, colorize);
        }
    }

    /// What the renderer's output pass does with it.
    pub fn render(&self) -> bae_render::Display {
        bae_render::Display {
            channel: self.channel,
            colorize: self.colorize && (1..=3).contains(&self.channel),
            exposure: self.exposure,
        }
    }

    /// The region to render, when on.
    pub fn roi(&self) -> Option<IRect> {
        self.roi.filter(|_| self.roi_on)
    }
}
