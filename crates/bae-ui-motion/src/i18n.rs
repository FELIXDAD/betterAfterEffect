//! Chinese strings shared by the motion workspace's modules (keys are the
//! English text, see `bae_ui::i18n`). Terms follow the Adobe After Effects
//! Simplified Chinese UI and `docs/glossary.md`. Each module registers its own
//! table (`<module>::ZH` / `<module>/zh.rs`); only common words stay here.

pub(crate) const ZH: &[(&str, &str)] = &[
    // Titles of bae-motion's commands: Edit > Undo / Redo, History, Keyboard Shortcuts
    // (#136). The command crate stays UI-free; its titles are translated here.
    ("Waveform Audio (WAV)", "波形音频（WAV）"),
    ("Stereo, 48 kHz, 24-bit PCM", "立体声，48 kHz，24 位 PCM"),
    ("Convert Audio to Keyframes", "将音频转换为关键帧"),
    ("Audio Amplitude", "音频振幅"),
    ("Left Channel", "左声道"),
    ("Right Channel", "右声道"),
    ("Both Channels", "两个声道"),
    ("Analyzing audio…", "正在分析音频…"),
    ("Audio analysis stopped.", "音频分析已停止。"),
    (
        "The composition has no enabled audio layers.",
        "合成中没有开启声音的图层。",
    ),
    ("Play/Stop (Numpad 0)", "播放/停止（小键盘 0）"),
    ("Apply Motion Tracker", "应用动态跟踪器"),
    ("Edit Motion Tracker", "编辑动态跟踪器"),
    ("Analyze Motion", "分析运动"),
    ("Reset Tracker", "重置跟踪器"),
    ("Change Value", "更改值"),
    ("Change Values", "更改多个值"),
    ("Set Work Area", "设置工作区域"),
    ("Delete Keyframes", "删除关键帧"),
    ("Move Keyframes", "移动关键帧"),
    ("Paste Keyframes", "粘贴关键帧"),
    ("New Layer", "新建图层"),
    ("Delete Layers", "删除图层"),
    ("Reorder Layers", "排列图层"),
    ("Set Parent", "设置父级"),
    ("Move Layer in Time", "在时间上移动图层"),
    ("Trim Layer", "修剪图层"),
    ("Change Layers", "更改图层"),
    // Composition viewer: an effect or the text renderer still warming up.
    ("Preparing…", "正在准备…"),
    ("Time Stretch", "时间伸缩"),
    // Panel-scoped zoom shortcuts (timeline: = / -, viewer: . / , / ?)
    ("Zoom In Time", "放大时间"),
    ("Zoom Out Time", "缩小时间"),
    // Panels and tabs
    ("Composition", "合成"),
    ("Timeline", "时间轴"),
    ("Effect Controls", "效果控件"),
    ("Properties", "属性"),
    ("Preview", "预览"),
    ("Project", "项目"),
    ("(none)", "（无）"),
    // Column names of the Project panel and the Timeline (AE 26 zh_CN).
    ("#", "#"),
    ("A/V Features", "A/V 功能"),
    ("Comment", "注释"),
    ("Date", "日期"),
    ("File Path", "文件路径"),
    ("In", "入"),
    ("In Point", "入点"),
    ("Keys", "键"),
    ("Media Duration", "媒体持续时间"),
    ("Media End", "媒体结束"),
    ("Media Start", "媒体开始"),
    ("Modes", "模式"),
    ("Out", "出"),
    ("Out Point", "出点"),
    ("Render Time", "渲染时间"),
    ("ms", "毫秒"),
    ("Stretch", "伸缩"),
    ("Tape Name", "磁带名称"),
    ("Video Info", "视频信息"),
    ("Easy Ease In", "缓入"),
    ("Easy Ease Out", "缓出"),
    ("Time-Reverse Keyframes", "时间反向关键帧"),
    ("Toggle Hold Keyframe", "切换定格关键帧"),
    ("Linear", "线性"),
    ("Bezier", "贝塞尔曲线"),
    ("Hold", "定格"),
    ("Keyframe Interpolation", "关键帧插值"),
    // Pixel unit after numbers (AE zh_CN unitsText, #197).
    ("px", "像素"),
];

/// Registers the common strings (`MotionUiModule::build`).
pub(crate) fn register(_app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
}

/// Every module's table, common words first (what was one table): tests check
/// coverage against it.
#[cfg(test)]
pub(crate) fn all() -> Vec<(&'static str, &'static str)> {
    [
        ZH,
        crate::effects::ZH,
        crate::actions::ZH,
        crate::timeline::ZH,
        crate::project::ZH,
        crate::menus::ZH,
        crate::render_queue::ZH,
        crate::context_menu::ZH,
        crate::panels::ZH,
        crate::viewer::ZH,
        crate::graph::ZH,
        crate::dialogs::ZH,
        crate::import::ZH,
        crate::assist::ZH,
    ]
    .concat()
}
