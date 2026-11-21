//! Chinese text of the Preferences dialog, keyed by the English text (gettext
//! style, registered into `bae_ui::i18n` by the dialog). The table is generated
//! with the pages from R1's reading of AE 26's zh_CN UI ([`crate::pages::ZH`]):
//! page names, group titles, labels, options, units and the dialog buttons.

pub use crate::pages::ZH;

/// The Chinese text of `en`, if the table has it.
pub fn zh(en: &str) -> Option<&'static str> {
    ZH.iter().find(|(e, _)| *e == en).map(|(_, z)| *z)
}
