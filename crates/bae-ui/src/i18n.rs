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
    TABLE.get_or_init(|| RwLock::new(ZH.iter().copied().collect()))
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

/// Shorthand for [`tr`].
#[macro_export]
macro_rules! tr {
    ($en:expr) => {
        $crate::i18n::tr($en)
    };
}

/// Shell strings. Keep in sync with docs/glossary.md.
const ZH: &[(&str, &str)] = &[
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
    ("Reset to Saved Layout", "重置为已保存的布局"),
    ("Save Changes to this Workspace", "保存对此工作区所做的更改"),
    ("Edit Workspaces...", "编辑工作区..."),
    // Common controls
    ("Search", "搜索"),
    ("Reset", "重置"),
    ("None", "无"),
    ("OK", "确定"),
    ("Cancel", "取消"),
    ("Language", "语言"),
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
