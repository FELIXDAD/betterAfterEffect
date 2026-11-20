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
    ("Open Project…", "打开项目…"),
    ("Quit BAE", "退出 BAE"),
    ("Window", "窗口"),
    ("Workspace", "工作区"),
    ("Minimize", "最小化"),
    ("Full Screen", "全屏"),
    ("frame", "帧"),
    ("English", "English"),
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
    fn table_has_unique_keys() {
        let mut keys: Vec<_> = ZH.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len());
    }
}
