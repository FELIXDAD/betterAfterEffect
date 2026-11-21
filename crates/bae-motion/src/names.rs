//! Default names of new items: project folders, comps, layers, shape groups
//! and their properties. AE gives them in the UI language at the moment they
//! are created; they are project data, so switching the language later renames
//! nothing. The app sets the language at startup ([`set_chinese`]); anything
//! else (tests, tools without a locale) gets English.

use std::sync::atomic::{AtomicBool, Ordering};

static CHINESE: AtomicBool = AtomicBool::new(false);

/// Name new items in Simplified Chinese from now on (`false`: English).
pub fn set_chinese(on: bool) {
    CHINESE.store(on, Ordering::Relaxed);
}

/// New items are named in Simplified Chinese.
pub fn chinese() -> bool {
    CHINESE.load(Ordering::Relaxed)
}

/// English default names and their Simplified Chinese, as AE zh_CN names them
/// (`docs/reference/ae/layers/*.json`, `docs/glossary.md`).
const ZH: &[(&str, &str)] = &[
    // Project folders.
    ("comps", "合成"),
    ("assets", "素材"),
    ("solids", "纯色"),
    ("Untitled", "未命名"),
    ("Comp", "合成"),
    // Layers.
    ("Shape Layer", "形状图层"),
    ("Text", "文本"),
    ("Solid", "纯色"),
    ("Null", "空"),
    ("Camera", "摄像机"),
    ("Light", "灯光"),
    ("Adjustment Layer", "调整图层"),
    ("Pre-comp", "预合成"),
    ("Footage", "素材"),
    ("Audio", "音频"),
    // Shape contents.
    ("Rectangle", "矩形"),
    ("Rectangle Path", "矩形路径"),
    ("Ellipse", "椭圆"),
    ("Ellipse Path", "椭圆路径"),
    ("Polystar", "多边星形"),
    ("Polystar Path", "多边星形路径"),
    ("Fill", "填充"),
    ("Stroke", "描边"),
    // Pen-drawn shapes and masks.
    ("Shape", "形状"),
    ("Path", "路径"),
    ("Mask", "蒙版"),
];

/// The default name `en` in the language new items are named in.
pub fn localized(en: &'static str) -> &'static str {
    if !chinese() {
        return en;
    }
    ZH.iter().find(|(e, _)| *e == en).map_or(en, |(_, zh)| zh)
}

/// Display name of a default project folder (`/comps`, `/assets`,
/// `/assets/solids`) when it is created, if it needs one: the path keeps the
/// English segment so paths stay the same in every language.
pub(crate) fn folder_name(segment: &str) -> Option<&'static str> {
    let (en, _) = ZH[..3].iter().find(|(e, _)| *e == segment)?;
    chinese().then(|| localized(en))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_name_has_chinese_and_english_is_the_default() {
        assert!(ZH.iter().all(|(en, zh)| !en.is_empty() && !zh.is_ascii()));
        let mut keys: Vec<_> = ZH.iter().map(|(en, _)| *en).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), ZH.len(), "duplicate keys");
        assert_eq!(
            &ZH[..3].iter().map(|(e, _)| *e).collect::<Vec<_>>(),
            &["comps", "assets", "solids"]
        );
    }
}
