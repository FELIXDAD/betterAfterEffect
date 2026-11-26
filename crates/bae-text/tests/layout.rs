//! Layout with the machine's system fonts (macOS / Windows always have Latin and
//! Chinese fonts; tests that need a font the machine lacks skip with a note).

use bae_model::TextDocument;
use bae_text::{CharKind, FontMatch, FontSystem, layout_text};

const LATIN: &[&str] = &[
    "ArialMT",
    "Helvetica",
    "SegoeUI",
    "DejaVuSans",
    "Roboto-Regular",
];
const CJK_FAMILIES: &[&str] = &[
    "PingFang SC",
    "Microsoft YaHei",
    "Noto Sans CJK SC",
    "Source Han Sans SC",
    "Hiragino Sans GB",
    "Heiti SC",
];

fn doc(text: &str, font: &str) -> TextDocument {
    TextDocument {
        text: text.to_owned(),
        font: font.to_owned(),
        font_size: 50.0,
        fill_color: [1.0, 1.0, 1.0, 1.0],
        stroke_color: None,
        stroke_width: 0.0,
        stroke_over_fill: false,
        justification: "left".to_owned(),
        tracking: 0.0,
        leading: None,
        baseline_shift: 0.0,
        faux_bold: false,
        faux_italic: false,
        all_caps: false,
        small_caps: false,
        box_size: None,
        box_position: None,
    }
}

/// A Latin font installed on this machine (PostScript name).
fn latin(fonts: &mut FontSystem) -> Option<String> {
    for name in LATIN {
        if matches!(fonts.resolve(name), FontMatch::Found(_)) {
            return Some((*name).to_owned());
        }
    }
    eprintln!("skipping: no known Latin font installed");
    None
}

fn has_cjk(fonts: &mut FontSystem) -> bool {
    let families = fonts.families();
    let found = CJK_FAMILIES.iter().any(|f| families.iter().any(|g| g == f));
    if !found {
        eprintln!("skipping: no Chinese system font installed");
    }
    found
}

#[test]
fn system_fonts_are_enumerated_and_matched_by_postscript_name() {
    let mut fonts = FontSystem::new();
    let families = fonts.families();
    if families.is_empty() {
        eprintln!("skipping: no system fonts");
        return;
    }
    // Every face of some family resolves back to itself by PostScript name.
    let family = families
        .iter()
        .find(|f| !fonts.faces(f).is_empty())
        .cloned()
        .expect("a family with readable faces");
    for face in fonts.faces(&family) {
        match fonts.resolve(&face.postscript_name) {
            FontMatch::Found(found) => assert_eq!(found.postscript_name, face.postscript_name),
            FontMatch::Missing => panic!("{} did not resolve", face.postscript_name),
        }
    }
    // A family name resolves to its default face.
    assert!(matches!(fonts.resolve(&family), FontMatch::Found(f) if f.family == family));
    assert_eq!(fonts.resolve("NoSuchFont-Regular"), FontMatch::Missing);
}

#[test]
fn point_text_alignment_and_lines() {
    let mut fonts = FontSystem::new();
    let Some(font) = latin(&mut fonts) else {
        return;
    };
    let left = layout_text(&mut fonts, &doc("Hello\rWorld", &font));
    assert_eq!(left.lines.len(), 2);
    assert_eq!(left.lines[0].baseline, 0.0, "first baseline at the origin");
    assert!(
        (left.lines[1].baseline - 60.0).abs() < 1e-9,
        "auto leading 120 %"
    );
    assert_eq!(left.lines[0].x, 0.0);
    let first = &left.chars[0];
    assert_eq!(first.source, 0..1);
    assert!(first.origin.x.abs() < 1e-3, "{:?}", first.origin);
    // Glyphs sit on the baseline: ink above it (negative y) and not far below.
    let b = left.bounds(false).unwrap();
    assert!(b.y0 < -25.0 && b.y1 < 15.0 + 60.0, "{b:?}");
    // The break is a character of its own and the second line starts at char 6.
    assert_eq!(left.chars[5].kind, CharKind::LineBreak);
    let second_line_first = &left.chars[left.lines[1].chars.start];
    assert_eq!(second_line_first.source, 6..7);
    assert_eq!(second_line_first.word, 1);

    let mut d = doc("Hello", &font);
    d.justification = "center".to_owned();
    let center = layout_text(&mut fonts, &d);
    let w = center.lines[0].width;
    assert!(w > 50.0);
    assert!((center.lines[0].x + w / 2.0).abs() < 1e-6);
    d.justification = "right".to_owned();
    let right = layout_text(&mut fonts, &d);
    assert!((right.lines[0].x + w).abs() < 1e-6);
    let last = right.runs[0].glyphs.last().unwrap();
    assert!(f64::from(last.x) < 0.0 && f64::from(last.x) > -w);

    // Explicit leading and baseline shift.
    let mut d = doc("a\rb", &font);
    d.leading = Some(80.0);
    d.baseline_shift = 10.0;
    let l = layout_text(&mut fonts, &d);
    assert_eq!(l.lines[1].baseline, 80.0);
    assert!((f64::from(l.runs[0].glyphs[0].y) + 10.0).abs() < 1e-3);
}

#[test]
fn tracking_widens_lines_but_not_their_trailing_edge() {
    let mut fonts = FontSystem::new();
    let Some(font) = latin(&mut fonts) else {
        return;
    };
    let plain = layout_text(&mut fonts, &doc("ABCD", &font));
    let mut d = doc("ABCD", &font);
    d.tracking = 100.0; // 0.1 em = 5 px after each character
    let tracked = layout_text(&mut fonts, &d);
    let delta = tracked.lines[0].width - plain.lines[0].width;
    assert!((delta - 15.0).abs() < 0.01, "3 gaps × 5 px, got {delta}");
}

#[test]
fn chinese_falls_back_to_a_system_cjk_font() {
    let mut fonts = FontSystem::new();
    if !has_cjk(&mut fonts) {
        return;
    }
    let Some(font) = latin(&mut fonts) else {
        return;
    };
    // Latin font with Chinese text: no tofu (glyph 0) for any character.
    let l = layout_text(&mut fonts, &doc("BAE 你好，世界", &font));
    let glyphs: Vec<_> = l.runs.iter().flat_map(|r| r.glyphs.iter()).collect();
    assert_eq!(
        glyphs.len(),
        8,
        "8 visible characters (the space has none): {glyphs:?}"
    );
    assert!(glyphs.iter().all(|g| g.id != 0), "tofu in {glyphs:?}");
    assert!(l.runs.len() >= 2, "Latin and CJK runs use different fonts");
    // A missing font still renders through the default font and fallback.
    let missing = layout_text(&mut fonts, &doc("缺失字体 Missing", "NoSuchFont-Regular"));
    assert_eq!(missing.font, FontMatch::Missing);
    assert!(
        missing
            .runs
            .iter()
            .flat_map(|r| &r.glyphs)
            .all(|g| g.id != 0)
    );
}

#[test]
fn chinese_paragraph_text_wraps_with_kinsoku() {
    let mut fonts = FontSystem::new();
    if !has_cjk(&mut fonts) {
        return;
    }
    let text = "这是一个用于检查中文段落换行的测试，标点不能出现在行首。第二句也一样！";
    let mut d = doc(text, "PingFangSC-Regular");
    d.font_size = 40.0;
    d.box_size = Some([250.0, 1000.0]);
    d.box_position = Some([-125.0, -40.0]);
    let l = layout_text(&mut fonts, &d);
    assert!(l.lines.len() >= 5, "{} lines", l.lines.len());
    let chars: Vec<char> = text.chars().collect();
    for line in &l.lines {
        let first = &l.chars[line.chars.start];
        let c = chars[first.source.start];
        assert!(!"，。！、".contains(c), "line starts with {c:?}");
        // Lines start at the box's left edge and stay inside the box.
        assert!((line.x + 125.0).abs() < 1e-6);
        assert!(line.width <= 250.0 + 0.5, "{}", line.width);
    }
    // First baseline: box top + ascent.
    let first = &l.lines[0];
    assert!((first.baseline - (-40.0 + first.ascent)).abs() < 1e-6);
    assert!(!l.overset);

    // A short box oversets: later lines are not composed.
    d.box_size = Some([250.0, 100.0]);
    let short = layout_text(&mut fonts, &d);
    assert!(short.overset);
    assert!(short.lines.len() < l.lines.len());
    assert!(short.chars.len() < l.chars.len());
}

#[test]
fn centred_paragraph_lines_centre_in_the_box() {
    let mut fonts = FontSystem::new();
    let Some(font) = latin(&mut fonts) else {
        return;
    };
    let mut d = doc("one two three four five six", &font);
    d.justification = "center".to_owned();
    d.box_size = Some([300.0, 500.0]);
    d.box_position = Some([0.0, 0.0]);
    let l = layout_text(&mut fonts, &d);
    assert!(l.lines.len() >= 2);
    for line in &l.lines {
        let centre = line.x + line.width / 2.0;
        assert!((centre - 150.0).abs() < 1e-6, "{line:?}");
    }
}

#[test]
fn all_caps_keeps_character_indices() {
    let mut fonts = FontSystem::new();
    let Some(font) = latin(&mut fonts) else {
        return;
    };
    let mut d = doc("abc", &font);
    d.all_caps = true;
    let caps = layout_text(&mut fonts, &d);
    let upper = layout_text(&mut fonts, &doc("ABC", &font));
    let ids = |l: &bae_text::TextLayout| -> Vec<u32> {
        l.runs
            .iter()
            .flat_map(|r| r.glyphs.iter().map(|g| g.id))
            .collect()
    };
    assert_eq!(ids(&caps), ids(&upper));
    assert_eq!(caps.chars[2].source, 2..3);
}

/// `sourceRectAtTime` measures text with the provider's fonts and shapes from
/// their geometry.
#[cfg(feature = "render")]
#[test]
fn provider_measures_text_and_shapes_for_expressions() {
    use bae_anim::{EvaluatedPrim, SourceBounds};
    use bae_model::{PrimPath, PrimType, Value};
    let provider = bae_text::render::TextProvider::new();
    let doc = bae_model::TextDocument::new("Hello", "ArialMT", 50.0);
    let [_, _, w, h] = provider.text(&doc, false).expect("text bounds");
    assert!(w > 50.0 && h > 20.0, "{w} × {h}");
    let prim = |path: &str, ty: PrimType, attrs: Vec<(&str, Value)>| EvaluatedPrim {
        path: PrimPath::new(path).unwrap(),
        prim_type: Some(ty),
        attrs: std::sync::Arc::new(attrs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()),
        children: Vec::new().into(),
    };
    let rect = prim(
        "/r",
        PrimType::RECT,
        vec![("size", Value::Vec2([100.0, 50.0]))],
    );
    let fill = prim("/f", PrimType::FILL, vec![]);
    let r = provider.shapes(&[rect, fill], false).expect("shape bounds");
    assert_eq!(r, [-50.0, -25.0, 100.0, 50.0]);
}
