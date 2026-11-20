//! Minimal English / Chinese strings for the shell's own placeholder UI and menus.
//!
//! Workstream 11 (`bae-ui::i18n`) owns the real translation system; when it lands
//! the shell switches to it and this table goes away. Keys are the English text;
//! Chinese terms follow Adobe's official Simplified Chinese AE translations
//! (`docs/glossary.md`).

use std::sync::atomic::{AtomicBool, Ordering};

static CHINESE: AtomicBool = AtomicBool::new(false);

/// Environment variable forcing the UI language: `zh` or `en`.
pub const LANG_ENV: &str = "BAE_LANG";

const ZH: &[(&str, &str)] = &[
    ("About BAE", "关于 BAE"),
    ("BAE Help", "BAE 帮助"),
    ("Composition", "合成"),
    (
        "Composition viewer: render output appears here.",
        "合成查看器：渲染结果显示在这里。",
    ),
    ("Edit", "编辑"),
    ("Effect Controls", "效果控件"),
    ("Exit", "退出"),
    ("File", "文件"),
    ("Help", "帮助"),
    ("Info", "信息"),
    ("New Project", "新建项目"),
    ("Open Project…", "打开项目…"),
    ("Project", "项目"),
    ("Quit BAE", "退出 BAE"),
    ("Redo", "重做"),
    ("Reset Workspace", "重置工作区"),
    ("Save", "保存"),
    ("Timeline", "时间轴"),
    ("Undo", "撤消"),
    ("Window", "窗口"),
    ("Minimize", "最小化"),
    ("Full Screen", "全屏"),
    ("Not implemented yet.", "尚未实现。"),
    ("frame", "帧"),
    (
        "GPU-first motion graphics and compositing.",
        "以 GPU 为基础的动态图形与合成应用。",
    ),
];

/// Picks the language from `BAE_LANG`, then `LC_ALL` / `LC_MESSAGES` / `LANG`.
pub fn init_from_env() {
    let chinese = std::env::var(LANG_ENV)
        .ok()
        .or_else(|| std::env::var("LC_ALL").ok())
        .or_else(|| std::env::var("LC_MESSAGES").ok())
        .or_else(|| std::env::var("LANG").ok())
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("zh"));
    set_chinese(chinese);
}

/// Switches between Chinese and English.
pub fn set_chinese(chinese: bool) {
    CHINESE.store(chinese, Ordering::Relaxed);
}

/// Translates `en` (returned unchanged when there is no translation).
pub fn tr(en: &'static str) -> &'static str {
    if !CHINESE.load(Ordering::Relaxed) {
        return en;
    }
    ZH.iter()
        .find(|(key, _)| *key == en)
        .map_or(en, |(_, zh)| zh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_unique_keys() {
        let mut keys: Vec<_> = ZH.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len());
    }
}
