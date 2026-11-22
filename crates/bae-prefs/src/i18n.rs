//! Chinese text of the Preferences dialog, keyed by the English text (gettext
//! style, registered into `bae_ui::i18n` by the dialog). The table is generated
//! with the pages from R1's reading of AE 26's zh_CN UI ([`crate::pages::ZH`]):
//! page names, group titles, labels, options, units and the dialog buttons.

pub use crate::pages::ZH;

/// Default values shown as text that are names BAE gives in the UI language:
/// the solids folder of a new project is "纯色" in Chinese, the name the
/// folder gets when a solid is made (issue #206).
pub const DEFAULT_VALUES_ZH: &[(&str, &str)] = &[("Solids", "纯色")];

/// The Chinese text of `en`, if the tables have it.
pub fn zh(en: &str) -> Option<&'static str> {
    ZH.iter()
        .chain(DEFAULT_VALUES_ZH)
        .find(|(e, _)| *e == en)
        .map(|(_, z)| *z)
}

#[cfg(test)]
mod tests {
    #[test]
    fn text_defaults_have_chinese() {
        // Every free-text default that is a word (not empty, not a path or
        // timecode) is translated, so the Chinese UI never shows it in English.
        for p in crate::schema::all() {
            if let (crate::Kind::Text, crate::DefaultValue::Str(v)) = (p.kind, p.default)
                && !v.is_empty()
                && !p.key.starts_with("labels.label_name_")
            {
                assert!(super::zh(v).is_some(), "{}: no Chinese for {v:?}", p.key);
            }
        }
        assert_eq!(super::zh("Solids"), Some("纯色"));
    }
}
