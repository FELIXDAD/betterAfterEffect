//! Coverage of the text the UI can show, on every platform (Windows CI too):
//!
//! - #136: every registered command, action, panel, workspace and menu entry has a
//!   Chinese name, and the Keyboard Shortcuts window never falls back to a raw id.
//! - #135: every character of that text, of the translation tables and of the string
//!   literals in the UI crates is in a font BAE always has: egui's built-in fonts (the
//!   only fonts bundled), or the system CJK font for CJK text. A `▸` (U+25B8) in the
//!   render queue hint showed as a missing-glyph box on Windows.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use bae_app::App;
use bae_ui::i18n;
use bae_ui::{ActionInfo, Keymap, MenuItemSpec, PanelKind, WorkbenchState, WorkspacePreset};
use egui::epaint::text::{FontDefinitions, FontFamily};
use skrifa::MetadataProvider as _;

use crate::document::Document;
use crate::ui::Workspace;

struct Assembled {
    app: App,
    doc: Document,
    workspace: Workspace,
}

/// The app as `main` builds it, with the shell's and every module's tables.
fn assemble() -> Assembled {
    crate::i18n::register_tables();
    let mut app = crate::app_builder().build();
    let mut doc = Document::new(app.resource::<Arc<bae_ui::history::HistoryLog>>().cloned());
    doc.add_module_commands(&app);
    let mut workspace = Workspace::new(&mut app, WorkbenchState::default());
    workspace.set_extra_commands(doc.command_titles());
    Assembled {
        app,
        doc,
        workspace,
    }
}

/// Every user-facing English name, keyed by where it comes from (for messages).
fn names(a: &Assembled) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (id, title) in a.doc.command_titles() {
        out.insert(format!("command {id}"), title);
    }
    if let Some(r) = a.app.get_registry::<ActionInfo>() {
        for e in r.snapshot().iter() {
            out.insert(format!("action {}", e.id()), e.item().title.clone());
        }
    }
    if let Some(r) = a.app.get_registry::<PanelKind>() {
        for e in r.snapshot().iter() {
            out.insert(format!("panel {}", e.id()), e.item().title.clone());
        }
    }
    if let Some(r) = a.app.get_registry::<WorkspacePreset>() {
        for e in r.snapshot().iter() {
            out.insert(format!("workspace {}", e.id()), e.item().title.clone());
        }
    }
    if let Some(r) = a.app.get_registry::<Keymap>() {
        for e in r.snapshot().iter() {
            out.insert(format!("keymap {}", e.id()), e.item().title.clone());
        }
    }
    if let Some(r) = a.app.get_registry::<MenuItemSpec>() {
        for e in r.snapshot().iter() {
            let spec = e.item();
            out.insert(format!("menu item {}", e.id()), spec.title.clone());
            for (i, part) in spec.path.iter().enumerate() {
                out.insert(format!("menu {} path {i}", e.id()), part.clone());
            }
        }
    }
    for row in a.workspace.shortcut_rows() {
        let key = format!("shortcut {} {}", row.id, row.params);
        if let Some(arg) = &row.arg {
            out.insert(format!("{key} argument"), arg.clone());
        }
        out.insert(key, row.title);
    }
    out
}

/// Names that read the same in Chinese (product names; "Agent" per docs/glossary.md).
/// They still need a table entry, so a new English name never slips through as "the
/// same".
pub(crate) const SAME_IN_CHINESE: &[&str] = &["BAE", "After Effects", "Agent"];

#[test]
fn every_command_panel_and_action_has_a_chinese_name() {
    let a = assemble();
    let names = names(&a);
    assert!(names.len() > 100, "only {} names collected", names.len());
    let mut missing = Vec::new();
    let mut untranslated = Vec::new();
    for (source, en) in &names {
        match i18n::translation(en) {
            None => missing.push(format!("{source}: {en:?}")),
            Some(zh) if zh == en && !SAME_IN_CHINESE.contains(&en.as_str()) => {
                untranslated.push(format!("{source}: {en:?}"));
            }
            Some(_) => {}
        }
    }
    // A shortcut row never shows a raw panel id ("Open Panel: character").
    let raw_ids: Vec<String> = a
        .workspace
        .shortcut_rows()
        .into_iter()
        .filter_map(|row| {
            let id = row.params.get("id")?.as_str()?;
            (row.arg.as_deref() == Some(id))
                .then(|| format!("{} {}: {:?}", row.id, row.params, row.shown))
        })
        .collect();
    assert!(
        raw_ids.is_empty(),
        "shortcut rows show raw ids: {raw_ids:#?}"
    );
    assert!(
        missing.is_empty(),
        "no Chinese name (add it to the owning crate's i18n table):\n{}",
        missing.join("\n")
    );
    assert!(
        untranslated.is_empty(),
        "Chinese name equals the English:\n{}",
        untranslated.join("\n")
    );
}

/// CJK text is drawn with the system CJK font (PingFang, Microsoft YaHei), which BAE
/// requires anyway: egui's built-in fonts have no CJK.
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{2E80}'..='\u{9FFF}'     // radicals, CJK punctuation, kana, ideographs
        | '\u{F900}'..='\u{FAFF}'   // compatibility ideographs
        | '\u{FE30}'..='\u{FE4F}'   // vertical forms
        | '\u{FF00}'..='\u{FFEF}') // full-width forms
}

/// The modifier glyphs of macOS shortcuts (`⌃⌥⇧`; `⌘` is in egui's fonts). Shown only
/// on macOS, where bae-ui draws text with the system font (SF) first
/// (`theme::install_fonts`); `macos_system_font_has_the_modifier_glyphs` checks SF.
pub(crate) const MACOS_MODIFIERS: [char; 3] = ['⌃', '⌥', '⇧'];

/// Whether the font's character map has `c`. (Not `Fonts::has_glyph`: it reports
/// characters of the face that also draws the missing-glyph box as missing.)
fn font_has(font: &[u8], index: u32, c: char) -> bool {
    skrifa::FontRef::from_index(font, index).is_ok_and(|f| f.charmap().map(c).is_some())
}

/// Characters egui's built-in proportional fonts (Ubuntu-Light, Noto Emoji,
/// emoji-icon-font: the fonts BAE bundles) cannot draw, outside ASCII and CJK.
pub(crate) fn missing_from_builtin_fonts(text: impl Iterator<Item = char>) -> Vec<char> {
    let defs = FontDefinitions::default();
    let fonts: Vec<_> = defs.families[&FontFamily::Proportional]
        .iter()
        .map(|name| &defs.font_data[name])
        .collect();
    let mut missing: Vec<char> = text
        .filter(|c| !c.is_ascii() && !is_cjk(*c))
        .filter(|c| !fonts.iter().any(|f| font_has(&f.font, f.index, *c)))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    missing
}

/// String literals of the UI crates' non-test code (rough: full-line comments and the
/// trailing `mod tests` are skipped), with their location.
fn ui_source_literals() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dirs = [
        "crates/bae-ui/src",
        "crates/bae-ui-motion/src",
        "apps/bae/src",
    ];
    let mut out = Vec::new();
    let mut stack: Vec<_> = dirs.iter().map(|d| root.join(d)).collect();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n != "acceptance") {
                    stack.push(path);
                }
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if !name.ends_with(".rs")
                || ["tests.rs", "ui_text.rs", "save_roundtrip.rs"].contains(&name.as_str())
            {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (n, line) in lines.iter().enumerate() {
                let next = lines.get(n + 1).map_or("", |l| l.trim_start());
                if line.trim() == "#[cfg(test)]" && next.starts_with("mod tests") {
                    break;
                }
                if line.trim_start().starts_with("//") {
                    continue;
                }
                let mut rest = *line;
                while let Some(start) = rest.find('"') {
                    let after = &rest[start + 1..];
                    let Some(end) = after.find('"') else { break };
                    // `/` on every platform: the keymap allowance below matches on it.
                    let rel = path.strip_prefix(&root).unwrap_or(&path);
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    out.push((format!("{rel}:{}", n + 1), after[..end].to_owned()));
                    rest = &after[end + 1..];
                }
            }
        }
    }
    out
}

#[test]
fn ui_text_glyphs_are_in_fonts_bae_always_has() {
    let a = assemble();
    let mut texts: Vec<(String, String)> = Vec::new();
    for (en, zh) in i18n::all_pairs() {
        texts.push((format!("i18n {en:?}"), en.to_owned()));
        texts.push((format!("i18n {en:?} (zh)"), zh.to_owned()));
    }
    for (source, en) in names(&a) {
        texts.push((source, en));
    }
    // Shortcut labels as this platform shows them (menus, shortcut editor).
    if let Some(r) = a.app.get_registry::<Keymap>() {
        for e in r.snapshot().iter() {
            for b in &e.item().bindings {
                texts.push((format!("keymap {} {}", e.id(), b.chord), b.chord.display()));
            }
        }
    }
    texts.extend(ui_source_literals());

    let mut problems = Vec::new();
    for (source, text) in &texts {
        let missing: Vec<char> = missing_from_builtin_fonts(text.chars())
            .into_iter()
            .filter(|c| {
                // macOS modifier glyphs: only in macOS shortcut labels and the chord
                // parser's aliases.
                !(MACOS_MODIFIERS.contains(c)
                    && (source.starts_with("keymap ")
                        || source.starts_with("crates/bae-ui/src/keymap.rs")))
            })
            .collect();
        if !missing.is_empty() {
            let codes: Vec<String> = missing
                .iter()
                .map(|c| format!("{c} U+{:04X}", u32::from(*c)))
                .collect();
            problems.push(format!("{source}: {} in {text:?}", codes.join(", ")));
        }
    }
    assert!(
        problems.is_empty(),
        "text with characters egui's built-in fonts cannot draw (shows as a box on \
         Windows); use a glyph they have, a painted shape or words:\n{}",
        problems.join("\n")
    );
}

/// On macOS the modifier glyphs allowed above really are in the system font.
#[cfg(target_os = "macos")]
#[test]
fn macos_system_font_has_the_modifier_glyphs() {
    let sf = std::fs::read("/System/Library/Fonts/SFNS.ttf").expect("SF system font");
    for c in MACOS_MODIFIERS {
        assert!(font_has(&sf, 0, c), "SF has no {c}");
    }
}
