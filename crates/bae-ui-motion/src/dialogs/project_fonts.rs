//! Fonts used in the newly opened document, including mixed styles and Source Text keys.
use super::{Dialog, SharedDialogs};
use crate::ae_panels::character;
use bae_model::{Stage, Value};
use bae_ui::tr;
use std::collections::BTreeSet;

pub(crate) fn recent(resources: &bae_app::Resources, count: usize) -> Vec<String> {
    let shared = super::shared(resources);
    let mut d = shared.lock();
    super::remember(resources, &mut d);
    d.recent_fonts.iter().take(count).cloned().collect()
}

pub(crate) fn selected(resources: &bae_app::Resources, family: &str) {
    let shared = super::shared(resources);
    let mut d = shared.lock();
    super::remember(resources, &mut d);
    if d.recent_fonts.first().is_some_and(|f| f == family) {
        return;
    }
    d.recent_fonts.retain(|f| f != family);
    d.recent_fonts.insert(0, family.to_owned());
    d.recent_fonts.truncate(15);
    super::memory::save(&d);
}

impl SharedDialogs {
    /// Start a fresh font check for this document. The native font catalog is
    /// prepared independently at application startup.
    pub fn check_project_fonts(&self, stage: &Stage, warn: bool) {
        let mut d = self.lock();
        d.pending_fonts.clear();
        if matches!(d.open, Some(Dialog::MissingFonts(_))) {
            d.open = None;
        }
        if !warn {
            return;
        }
        let mut names = BTreeSet::new();
        for path in stage.traverse() {
            let Some(source) = stage
                .prim(&path)
                .and_then(|p| p.attr(bae_motion::text::SOURCE))
            else {
                continue;
            };
            for value in source
                .value
                .iter()
                .chain(source.keyframes.values().map(|key| &key.value))
            {
                if let Value::TextDocument(doc) = value {
                    for name in std::iter::once(&doc.style.font)
                        .chain(doc.runs.iter().map(|run| &run.style.font))
                    {
                        if !name.is_empty() {
                            names.insert(name.clone());
                        }
                    }
                }
            }
        }
        d.pending_fonts = names.into_iter().collect();
    }
}

pub(super) fn warning(pending: &mut Vec<String>) -> Option<Dialog> {
    if pending.is_empty() {
        return None;
    }
    let catalog = character::font_menu::CATALOG.get()?;
    if !catalog.indexed() {
        return None;
    }
    let missing: Vec<_> = std::mem::take(pending)
        .into_iter()
        .filter(|name| catalog.lookup(name).is_none())
        .collect();
    (!missing.is_empty()).then(|| {
        Dialog::MissingFonts(format!(
            "{}\n{}",
            tr("These fonts are missing from this computer:"),
            missing.join("\n")
        ))
    })
}
