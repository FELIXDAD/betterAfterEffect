//! UI language (English / Simplified Chinese).
//!
//! Strings are keyed by their English text, gettext style: `tr("Project")`
//! returns "项目" when the UI language is Chinese. Chinese terms follow the
//! official Adobe After Effects Simplified Chinese UI; see docs/glossary.md.
//! Feature crates add their own tables with [`register`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{OnceLock, RwLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Lang {
    #[default]
    En,
    Zh,
}

impl Lang {
    /// Pick the language from a locale string such as `zh_CN.UTF-8` or `zh-Hans`.
    pub fn from_locale(locale: &str) -> Lang {
        if locale.to_ascii_lowercase().starts_with("zh") {
            Lang::Zh
        } else {
            Lang::En
        }
    }

    /// Name of the language in that language, for a language picker.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Zh => "简体中文",
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

type Table = HashMap<&'static str, &'static str>;

fn tables() -> &'static RwLock<Table> {
    static TABLE: OnceLock<RwLock<Table>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let shortcuts = crate::workbench::SHORTCUTS_ZH;
        RwLock::new(ZH.iter().chain(shortcuts).copied().collect())
    })
}

pub fn set_lang(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::Zh,
        _ => Lang::En,
    }
}

/// Add (English, Chinese) pairs. Later registrations override earlier ones.
pub fn register(pairs: &[(&'static str, &'static str)]) {
    let mut t = tables().write().unwrap_or_else(|e| e.into_inner());
    t.extend(pairs.iter().copied());
}

/// Translate `en` into the current UI language. Unknown strings are returned unchanged.
pub fn tr(en: &str) -> &str {
    if lang() == Lang::En {
        return en;
    }
    let t = tables().read().unwrap_or_else(|e| e.into_inner());
    t.get(en).copied().unwrap_or(en)
}

/// Whether `en` has a Chinese translation in the registered tables, whatever the
/// current language (for tests that check coverage without switching the global
/// language under other tests).
pub fn has_translation(en: &str) -> bool {
    let t = tables().read().unwrap_or_else(|e| e.into_inner());
    t.contains_key(en)
}

/// The Chinese for `en`, whatever the current language (`None` when there is none).
/// For tests and tools that check translations without switching the global language.
pub fn translation(en: &str) -> Option<&'static str> {
    let t = tables().read().unwrap_or_else(|e| e.into_inner());
    t.get(en).copied()
}

/// Every registered (English, Chinese) pair, sorted by the English key.
pub fn all_pairs() -> Vec<(&'static str, &'static str)> {
    let t = tables().read().unwrap_or_else(|e| e.into_inner());
    let mut pairs: Vec<_> = t.iter().map(|(en, zh)| (*en, *zh)).collect();
    pairs.sort_unstable();
    pairs
}

/// Shorthand for [`tr`].
#[macro_export]
macro_rules! tr {
    ($en:expr) => {
        $crate::i18n::tr($en)
    };
}

/// Shell strings. Keep in sync with docs/glossary.md.
const ZH: &[(&str, &str)] = &[
    // Numpad keys in shortcuts (AE: 数字小键盘).
    ("Num *", "数字小键盘 *"),
    ("Num /", "数字小键盘 /"),
    ("Num +", "数字小键盘 +"),
    ("Num -", "数字小键盘 -"),
    ("Num .", "数字小键盘 ."),
    ("Num 0", "数字小键盘 0"),
    ("Num 1", "数字小键盘 1"),
    ("Num 2", "数字小键盘 2"),
    ("Num 3", "数字小键盘 3"),
    ("Num 4", "数字小键盘 4"),
    ("Num 5", "数字小键盘 5"),
    ("Num 6", "数字小键盘 6"),
    ("Num 7", "数字小键盘 7"),
    ("Num 8", "数字小键盘 8"),
    ("Num 9", "数字小键盘 9"),
    // Dock and panel menu
    ("Close Panel", "关闭面板"),
    ("Undock Panel", "浮动面板"),
    ("Close Other Panels in Group", "关闭组中的其他面板"),
    ("Panel Group Settings", "面板组设置"),
    ("Close Panel Group", "关闭面板组"),
    ("Undock Panel Group", "浮动面板组"),
    ("Maximize Panel Group", "最大化面板组"),
    ("Restore Panel Group Size", "恢复面板组大小"),
    ("Stacked Panel Group", "堆叠的面板组"),
    ("Solo Panels in Stack", "堆栈中的单独面板"),
    ("Small Tabs", "小选项卡"),
    // Workspaces
    ("Workspace", "工作区"),
    ("Default", "默认"),
    ("Review", "审阅"),
    ("Learn", "学习"),
    ("Small Screen", "小屏幕"),
    ("Standard", "标准"),
    ("Motion", "动效"),
    ("Agent", "Agent"),
    ("Reset to Saved Layout", "重置为已保存的布局"),
    ("Save Changes to this Workspace", "保存对此工作区所做的更改"),
    ("Edit Workspaces...", "编辑工作区..."),
    ("Edit Workspaces", "编辑工作区"),
    ("Save as New Workspace...", "另存为新工作区..."),
    ("New Workspace", "新建工作区"),
    ("Show in Bar", "显示在工作区栏"),
    ("Delete", "删除"),
    ("Name", "名称"),
    (
        "Saves the current layout and shortcuts as a new workspace.",
        "把当前布局和快捷键存成一个新工作区。",
    ),
    ("Welcome to BAE", "欢迎使用 BAE"),
    ("Close", "关闭"),
    ("Close Window", "关闭窗口"),
    (
        "What do you do? BAE opens the matching workspace; you can switch any time in the workspace bar.",
        "你主要做什么？BAE 会打开对应的工作区，之后随时可以在工作区栏切换。",
    ),
    ("Storyboard", "分镜"),
    ("Editing", "剪辑"),
    ("coming later", "以后提供"),
    (
        "Compositions, layers, keyframes, expressions and effects, laid out like After Effects.",
        "合成、图层、关键帧、表达式和效果，界面与 After Effects 一致。",
    ),
    (
        "Play back, annotate and compare versions.",
        "播放、批注、对比版本。",
    ),
    (
        "Watch what agents do and review their proposals.",
        "查看 Agent 的操作并审核它们的提案。",
    ),
    // Shell actions and menus (AE 2026 zh_CN dictionary, MenuID 0002/0003/0152)
    ("Undo", "撤消"),
    ("Redo", "重做"),
    ("Quit", "退出"),
    ("About BAE", "关于 BAE"),
    ("Keyboard Shortcuts", "键盘快捷键"),
    ("New Project", "新建项目"),
    ("Open Project...", "打开项目..."),
    ("Close Project", "关闭项目"),
    ("Save", "保存"),
    ("Save As...", "另存为..."),
    ("Switch Workspace", "切换工作区"),
    ("Open Panel", "打开面板"),
    ("BAE Help", "BAE 帮助"),
    ("History", "历史记录"),
    ("Not implemented yet.", "尚未实现。"),
    // Agent panels
    ("No agent is connected.", "没有连接 Agent。"),
    ("No agent activity yet.", "还没有 Agent 活动。"),
    ("Only Errors", "只显示错误"),
    ("changes", "处更改"),
    ("No proposals.", "没有提案。"),
    ("prims", "个节点"),
    ("Accept All", "全部接受"),
    ("Reject All", "全部拒绝"),
    ("Accept", "接受"),
    // Proposal commands (bae-agent), shown in Undo, History and Keyboard Shortcuts.
    ("Accept Proposal", "接受提案"),
    ("Reject Proposal", "拒绝提案"),
    ("Reject", "拒绝"),
    ("New", "新建"),
    ("Changed", "已修改"),
    // History and shortcut editor
    ("Collaborator", "协作者"),
    ("You", "你"),
    ("No history.", "没有历史记录。"),
    ("Only My Changes", "只显示我的更改"),
    ("Remote change", "远程更改"),
    ("Layer stack change", "工程层更改"),
    ("Change", "更改"),
    ("Replace", "替换"),
    ("Remove", "移除"),
    ("After Effects", "After Effects"),
    // bae-model built-in command titles
    ("Create Prim", "创建节点"),
    ("Create Text Animator", "创建文字动画器"),
    ("Delete Prim", "删除节点"),
    ("Move Prim", "移动节点"),
    ("Rename Prim", "重命名节点"),
    ("Set Value", "设置值"),
    ("Clear Attribute", "清除属性"),
    ("Set Keyframe", "设置关键帧"),
    ("Remove Keyframe", "移除关键帧"),
    ("Set Expression", "设置表达式"),
    ("Set References", "设置引用"),
    ("New Sublayer", "新建工程层"),
    ("Set Edit Target", "设置编辑目标"),
    ("Push Down Overrides", "下推覆盖"),
    ("Merge Sublayer", "合并工程层"),
    ("Relink Asset", "重新链接素材"),
    ("This panel is not available.", "此面板不可用。"),
    ("Untitled Project", "未命名项目"),
    ("Agent Activity", "Agent 活动"),
    ("Proposals", "提案"),
    // Panels (docs/glossary.md §2)
    ("Project", "项目"),
    ("Composition", "合成"),
    ("Timeline", "时间轴"),
    ("Effect Controls", "效果控件"),
    ("Effects & Presets", "效果和预设"),
    ("Preview", "预览"),
    ("Properties", "属性"),
    ("Align", "对齐"),
    ("Audio", "音频"),
    ("Info", "信息"),
    ("Character", "字符"),
    ("Paragraph", "段落"),
    ("Render Queue", "渲染队列"),
    ("Tools", "工具"),
    // Accessibility labels (a11y): toolbar, title bar, dock, controls
    ("Home", "主页"),
    ("Selection Tool", "选取工具"),
    ("Hand Tool", "手形工具"),
    ("Zoom Tool", "缩放工具"),
    ("Orbit Around Cursor Tool", "绕光标旋转工具"),
    ("Pan Under Cursor Tool", "在光标下移动工具"),
    ("Dolly Towards Cursor Tool", "向光标方向推拉镜头工具"),
    ("Rotation Tool", "旋转工具"),
    ("Pan Behind (Anchor Point) Tool", "向后平移（锚点）工具"),
    ("Rectangle Tool", "矩形工具"),
    ("Cube Tool", "立方体工具"),
    ("Pen Tool", "钢笔工具"),
    ("Horizontal Type Tool", "横排文字工具"),
    ("Brush Tool", "画笔工具"),
    ("Clone Stamp Tool", "仿制图章工具"),
    ("Eraser Tool", "橡皮擦工具"),
    ("Roto Brush Tool", "Roto 笔刷工具"),
    ("Puppet Position Pin Tool", "人偶位置控点工具"),
    ("Minimize", "最小化"),
    ("Zoom", "缩放"),
    ("Resize Panels", "调整面板大小"),
    ("Panel Menu", "面板菜单"),
    ("More Panels", "更多面板"),
    ("Lock", "锁定"),
    ("Workspace Menu", "工作区菜单"),
    ("Time-Vary Stopwatch", "时间变化秒表"),
    ("Go to Previous Keyframe", "转到上一个关键帧"),
    (
        "Add or Remove Keyframe at Current Time",
        "在当前时间添加或移除关键帧",
    ),
    ("Go to Next Keyframe", "转到下一个关键帧"),
    ("Expand", "展开"),
    ("Collapse", "折叠"),
    ("Color", "颜色"),
    ("Layer “{name}”", "图层“{name}”"),
    ("Layer {n} “{name}”", "图层 {n}“{name}”"),
    // Common controls
    ("Search", "搜索"),
    ("Reset", "重置"),
    ("None", "无"),
    ("OK", "确定"),
    ("Cancel", "取消"),
    ("Language", "语言"),
    // Color Picker (`color_picker`); AE's picker labels H/S/B/R/G/B by letter,
    // the full names are for screen readers.
    ("Color Field", "颜色场"),
    ("Color Slider", "颜色滑块"),
    ("Original Color", "原始颜色"),
    ("Hex Color", "十六进制颜色"),
    ("Eyedropper", "吸管"),
    ("Only Web Colors", "仅限 Web 颜色"),
    ("Hue", "色相"),
    ("Saturation", "饱和度"),
    ("Brightness", "亮度"),
    ("Red", "红色"),
    ("Green", "绿色"),
    ("Blue", "蓝色"),
    ("Value", "值"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_english() {
        set_lang(Lang::Zh);
        assert_eq!(tr("Close Panel"), "关闭面板");
        assert_eq!(tr("No such string"), "No such string");
        register(&[("Project", "项目")]);
        assert_eq!(tr("Project"), "项目");
        assert!(has_translation("Close Panel") && !has_translation("No such string"));
        set_lang(Lang::En);
        assert_eq!(tr("Project"), "Project");
    }

    #[test]
    fn locale_detection() {
        assert_eq!(Lang::from_locale("zh_CN.UTF-8"), Lang::Zh);
        assert_eq!(Lang::from_locale("zh-Hans"), Lang::Zh);
        assert_eq!(Lang::from_locale("en_US"), Lang::En);
    }
}
