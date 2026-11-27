//! Coverage of the text the UI can show, on every platform (Windows CI too):
//!
//! - #136: every registered command, action, panel, workspace and menu entry has a
//!   Chinese name, and the Keyboard Shortcuts window never falls back to a raw id.
//! - #135: every character of that text, of the translation tables and of the string
//!   literals in the UI crates is in the UI's fonts: the system fonts bae-ui loads
//!   (`theme::system_ui_fonts`: system UI font, Simplified Chinese font, symbol font;
//!   nothing is bundled). A `▸` (U+25B8) in the render queue hint showed as a
//!   missing-glyph box on Windows. Run on macOS and Windows CI, this checks each
//!   platform's real fonts.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use bae_app::App;
use bae_ui::i18n;
use bae_ui::{
    ActionInfo, CommandVariant, Keymap, MenuItemSpec, PanelKind, WorkbenchState, WorkspacePreset,
};
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
    // Variant names: shortcut rows, and undo / History labels of model commands.
    if let Some(r) = a.app.get_registry::<CommandVariant>() {
        for e in r.snapshot().iter() {
            out.insert(format!("variant {}", e.id()), e.item().title.clone());
        }
    }
    if let Some(r) = a.app.get_registry::<MenuItemSpec>() {
        for e in r.snapshot().iter() {
            let spec = e.item();
            if spec.is_placeholder {
                // Never shown (it only makes its submenu exist).
                continue;
            }
            out.insert(format!("menu item {}", e.id()), spec.title.clone());
            for (i, part) in spec.path.iter().enumerate() {
                if part == bae_ui::menu::APP_MENU {
                    // The macOS application menu: shown as the application's name.
                    continue;
                }
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
pub(crate) const SAME_IN_CHINESE: &[&str] = &[
    "BAE",
    "After Effects",
    "Photoshop",
    "Agent",
    "3D",
    "CPU",
    // AE's Chinese UI keeps these: RotoBezier, Roto 笔刷工具.
    "RotoBezier",
    "Roto",
    // AE's 3D view shortcuts: 切换到三维视图 A / B / C.
    "视图 A",
    "视图 B",
    "视图 C",
    // AE: 显示/隐藏 Alpha 通道, RGB 直接.
    "Alpha",
    "RGB",
    // AE: 3D 位置 Gizmo, 三维旋转 Gizmo …
    "Gizmo",
    // AE: 预览（Shift+数字小键盘 0）.
    "Shift",
];

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
            Some(zh)
                if zh == en
                    && !SAME_IN_CHINESE.contains(&en.as_str())
                    && !bae_ui_motion::catalog::SAME_IN_CHINESE.contains(&en.as_str())
                    && !bae_ui_motion::effects::same_in_chinese(en) =>
            {
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

/// A shortcut row's name in English or (from the tables, without switching the
/// global language) Chinese, as the Keyboard Shortcuts window shows it.
fn row_name(row: &crate::ui::ShortcutRow, zh: bool) -> String {
    let t = |s: &str| {
        if zh {
            i18n::translation(s).unwrap_or(s).to_owned()
        } else {
            s.to_owned()
        }
    };
    match &row.arg {
        Some(arg) => format!("{}: {}", t(&row.title), t(arg)),
        None => t(&row.title),
    }
}

/// #140: every row of the Keyboard Shortcuts window (every bindable command and
/// variant) has its own name, in English and in Chinese: no four "Next Frame" rows
/// for PageUp / PageDown / Shift+PageUp / Shift+PageDown.
#[test]
fn every_shortcut_row_has_a_unique_name() {
    let a = assemble();
    let rows = a.workspace.shortcut_rows();
    assert!(rows.len() > 50, "only {} rows", rows.len());
    for zh in [false, true] {
        let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for row in &rows {
            by_name
                .entry(row_name(row, zh))
                .or_default()
                .push(format!("{} {}", row.id, row.params));
        }
        let duplicates: Vec<String> = by_name
            .into_iter()
            .filter(|(_, rows)| rows.len() > 1)
            .map(|(name, rows)| format!("{name:?}: {}", rows.join(", ")))
            .collect();
        let lang = if zh { "Chinese" } else { "English" };
        assert!(
            duplicates.is_empty(),
            "{lang} shortcut rows share a name (register a CommandVariant):\n{}",
            duplicates.join("\n")
        );
    }
}

/// The modifier glyphs of macOS shortcuts (`⌃⌥⇧`). Shown only on macOS (in
/// `Chord::display`, and as the chord parser's aliases), so on Windows their literals in
/// keymap.rs need no font; on macOS the system font must have them like everything else.
pub(crate) const MACOS_MODIFIERS: [char; 3] = ['⌃', '⌥', '⇧'];

/// Glyphs egui's own widgets draw as text in the UI BAE shows: elided text (`…`),
/// negative numbers in drag fields (`−`, U+2212) and BAE's submenu arrow (`›`,
/// `widgets::SUBMENU_ARROW`; egui's own `⏵` is avoided).
const EGUI_GLYPHS: &str = "…−›";

/// Japanese: the modern kana (ぁ–ん, ァ–ヶ, ー, ・) and kanji in their Japanese forms;
/// the Chinese UI font draws them. (Archaic kana such as ゕ or ヷ are not in PingFang SC.)
fn japanese_sample() -> String {
    let kana = ('\u{3041}'..='\u{3093}')
        .chain('\u{30A1}'..='\u{30F6}')
        .chain(['ー', '・']);
    kana.chain("日本語円駅気楽図読書様画説変戻込拡縮選択設定編集表示保存開閉".chars())
        .collect()
}

/// Whether the font's character map has `c`. (Not `Fonts::has_glyph`: it reports
/// characters of the face that also draws the missing-glyph box as missing.)
fn font_has(font: &[u8], index: u32, c: char) -> bool {
    skrifa::FontRef::from_index(font, index).is_ok_and(|f| f.charmap().map(c).is_some())
}

/// Format characters (joiners, variation selectors) need no glyph.
fn needs_glyph(c: char) -> bool {
    !c.is_control()
        && !matches!(c, '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{206F}' | '\u{FE00}'..='\u{FE0F}')
}

/// Characters none of the UI's fonts (`theme::system_ui_fonts`) can draw.
pub(crate) fn missing_from_ui_fonts(text: impl Iterator<Item = char>) -> Vec<char> {
    let fonts = bae_ui::theme::system_ui_fonts();
    let mut missing: Vec<char> = text
        .filter(|c| needs_glyph(*c) && *c != ' ')
        .filter(|c| !fonts.iter().any(|f| font_has(f.data, f.index, *c)))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    missing
}

/// The UI crates' non-test code lines, with their location (`/`-separated path and
/// line number). Rough: full-line comments and the trailing `mod tests` are skipped.
fn ui_source_lines() -> Vec<(String, String)> {
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
            // `/` on every platform: the keymap allowance matches on it.
            let rel = path.strip_prefix(&root).unwrap_or(&path);
            let rel = rel.to_string_lossy().replace('\\', "/");
            let text = std::fs::read_to_string(&path).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (n, line) in lines.iter().enumerate() {
                let next = lines.get(n + 1).map_or("", |l| l.trim_start());
                if line.trim() == "#[cfg(test)]" && next.starts_with("mod tests") {
                    break;
                }
                if !line.trim_start().starts_with("//") {
                    out.push((format!("{rel}:{}", n + 1), (*line).to_owned()));
                }
            }
        }
    }
    out
}

/// String literals of the UI crates' non-test code, with their location.
fn ui_source_literals() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (at, line) in ui_source_lines() {
        let mut rest = line.as_str();
        while let Some(start) = rest.find('"') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('"') else { break };
            out.push((at.clone(), after[..end].to_owned()));
            rest = &after[end + 1..];
        }
    }
    out
}

#[test]
fn ui_text_glyphs_are_in_the_ui_fonts() {
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
    texts.push(("egui's own glyphs".into(), EGUI_GLYPHS.into()));
    texts.push(("Japanese".into(), japanese_sample()));
    // Korean: every Hangul syllable (the Korean system font has them all).
    texts.push(("Korean".into(), ('\u{AC00}'..='\u{D7A3}').collect()));

    let fonts = bae_ui::theme::system_ui_fonts();
    let names: Vec<String> = fonts
        .iter()
        .map(|f| format!("{} = {}", f.name, f.family))
        .collect();
    for name in ["ui-text", "ui-cjk", "ui-symbols"] {
        assert!(
            fonts.iter().any(|f| f.name == name),
            "no system font for {name}: {names:?}"
        );
    }
    let mut problems = Vec::new();
    for (source, text) in &texts {
        let missing: Vec<char> = missing_from_ui_fonts(text.chars())
            .into_iter()
            .filter(|c| {
                // macOS modifier glyphs in keymap.rs: only shown on macOS.
                cfg!(target_os = "macos")
                    || !(MACOS_MODIFIERS.contains(c)
                        && source.starts_with("crates/bae-ui/src/keymap.rs"))
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
        "text with characters the UI's system fonts ({names:?}) cannot draw (a box); use \
         a glyph they have, a painted shape or words:\n{}",
        problems.join("\n")
    );
}

/// egui's `ui.menu_button` draws its submenu arrow `⏵`, which no system font has; UI
/// code uses `bae_ui::widgets::menu_button` (arrow `›`) instead.
#[test]
fn ui_code_uses_the_menu_button_with_a_system_glyph() {
    let calls: Vec<String> = ui_source_lines()
        .into_iter()
        .filter(|(_, line)| line.contains(".menu_button("))
        .map(|(at, line)| format!("{at}: {}", line.trim()))
        .collect();
    assert!(
        calls.is_empty(),
        "use bae_ui::widgets::menu_button:\n{}",
        calls.join("\n")
    );
}

/// Default-ignorable characters (variation selectors, joiners, soft hyphen, tags) are
/// drawn as nothing, not as the missing-glyph box (vendored epaint's `invisible_char`,
/// vendor/epaint/BAE-PATCH.md).
#[test]
fn invisible_characters_draw_nothing() {
    let mut measure = text_measure(bae_ui::theme::ui_font_definitions());
    let letters = measure("AB");
    let alone = measure("x");
    for c in INVISIBLE_SAMPLE {
        let code = u32::from(c);
        // Between letters (joins a cluster) and on its own at the start: a selector
        // there found no font and drew the box with the system fonts.
        assert_eq!(
            measure(&format!("A{c}B")),
            letters,
            "U+{code:04X} between letters"
        );
        assert_eq!(
            measure(&format!("{c}x")),
            alone,
            "U+{code:04X} at the start"
        );
    }
    // A character no font has still shows the missing-glyph box.
    let (drawn, width) = measure("\u{10FFFD}");
    assert!(drawn >= 1 && width > 0.0, "the missing glyph box");
}

/// Windows CI drew a lone U+FE0F because a font there maps it to a visible glyph
/// that the shaper keeps. macOS's LastResort font maps every code point to a
/// visible glyph, so it reproduces that on any Mac.
#[cfg(target_os = "macos")]
#[test]
fn invisible_characters_draw_nothing_when_a_font_maps_them() {
    let Ok(data) = std::fs::read("/System/Library/Fonts/LastResort.otf") else {
        panic!("macOS ships /System/Library/Fonts/LastResort.otf");
    };
    let mut definitions = bae_ui::theme::ui_font_definitions();
    definitions.font_data.insert(
        "last-resort".into(),
        std::sync::Arc::new(egui::FontData::from_owned(data)),
    );
    for family in definitions.families.values_mut() {
        family.insert(0, "last-resort".into());
    }
    let mut measure = text_measure(definitions);
    let alone = measure("x");
    for c in INVISIBLE_SAMPLE {
        let code = u32::from(c);
        assert_eq!(
            measure(&format!("{c}x")),
            alone,
            "U+{code:04X} at the start"
        );
    }
    assert_eq!(
        measure("\u{200D}\u{FE0F}x"),
        alone,
        "ZWJ and U+FE0F at the start"
    );
}

/// Default-ignorable characters that must not draw.
const INVISIBLE_SAMPLE: [char; 6] = [
    '\u{FE0F}',
    '\u{FE0E}',
    '\u{200D}',
    '\u{00AD}',
    '\u{E0020}',
    '\u{2060}',
];

/// (glyphs that draw something, total advance) of a text on one line.
fn text_measure(definitions: egui::FontDefinitions) -> impl FnMut(&str) -> (usize, f32) {
    use egui::epaint::text::{Fonts, LayoutJob, TextOptions};
    let mut fonts = Fonts::new(TextOptions::default(), definitions);
    move |text| {
        let job = LayoutJob::simple_singleline(
            text.to_owned(),
            egui::FontId::proportional(13.0),
            egui::Color32::WHITE,
        );
        let galley = fonts.with_pixels_per_point(2.0).layout_job(job);
        let glyphs = galley.rows.iter().flat_map(|r| r.row.glyphs.iter());
        glyphs.fold((0usize, 0.0f32), |(n, w), g| {
            (
                n + usize::from(!g.uv_rect.is_nothing()),
                w + g.advance_width,
            )
        })
    }
}
