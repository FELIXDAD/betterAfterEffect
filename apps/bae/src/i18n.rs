//! UI language for the shell's own strings (menus, About, status bar).
//!
//! Translation lives in `bae_ui::i18n`; this module adds the shell's strings to its
//! table and picks the language from the environment. Keys are the English text;
//! Chinese follows the AE 2026 Simplified Chinese UI (`docs/glossary.md`).

pub use bae_ui::i18n::tr;
use bae_ui::i18n::{self, Lang};

/// Environment variable forcing the UI language: `zh` or `en`.
pub const LANG_ENV: &str = "BAE_LANG";

const ZH: &[(&str, &str)] = &[
    (
        "Composition viewer: render output appears here.",
        "合成查看器：渲染结果显示在这里。",
    ),
    ("Edit", "编辑"),
    ("Exit", "退出"),
    ("File", "文件"),
    ("Help", "帮助"),
    ("Quit BAE", "退出 BAE"),
    ("Window", "窗口"),
    ("Workspace", "工作区"),
    ("Minimize", "最小化"),
    ("Full Screen", "全屏"),
    ("frame", "帧"),
    ("English", "English"),
    // File commands and their dialogs
    ("Open Project", "打开项目"),
    ("Save As", "另存为"),
    ("BAE Project", "BAE 项目"),
    ("BAE Package", "BAE 打包项目"),
    (
        "BAE Project (.baez, project.json)",
        "BAE 项目（.baez、project.json）",
    ),
    ("Format:", "格式："),
    (
        "Save changes to “{}” before closing?",
        "关闭前是否保存对“{}”的更改？",
    ),
    (
        "If you don't save, your changes will be lost.",
        "如果不保存，你的更改将会丢失。",
    ),
    ("Don't Save", "不保存"),
    ("Could not open the project.", "无法打开项目。"),
    ("Could not save the project.", "无法保存项目。"),
    (
        "GPU-first motion graphics and compositing.",
        "以 GPU 为基础的动态图形与合成应用。",
    ),
];

/// Registers the shell strings and picks the language from `BAE_LANG`, then
/// `LC_ALL` / `LC_MESSAGES` / `LANG`.
pub fn init_from_env() {
    i18n::register(ZH);
    let locale = std::env::var(LANG_ENV)
        .ok()
        .or_else(|| std::env::var("LC_ALL").ok())
        .or_else(|| std::env::var("LC_MESSAGES").ok())
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_default();
    i18n::set_lang(Lang::from_locale(&locale));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_menu_and_dialog_strings_have_chinese() {
        i18n::register(ZH);
        let mut strings: Vec<String> = crate::shell::file_menu_titles();
        strings.extend(
            [
                "File",
                "Exit",
                "Open Project",
                "Save As",
                "BAE Project",
                "BAE Package",
                "BAE Project (.baez, project.json)",
                "Format:",
                "Save changes to “{}” before closing?",
                "If you don't save, your changes will be lost.",
                "Save",
                "Don't Save",
                "Cancel",
                "Could not open the project.",
                "Could not save the project.",
                "Untitled Project",
            ]
            .map(String::from),
        );
        let missing: Vec<_> = strings
            .iter()
            .filter(|s| !i18n::has_translation(s))
            .collect();
        assert!(missing.is_empty(), "no Chinese for {missing:?}");
    }

    #[test]
    fn table_has_unique_keys() {
        let mut keys: Vec<_> = ZH.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len());
    }
}
