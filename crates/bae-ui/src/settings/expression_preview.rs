//! Live sample in the expression editor preferences.
use bae_prefs::Preferences;
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, TextFormat, Ui, pos2, vec2};

use crate::{Accessible as _, i18n::tr, icons};

fn color(prefs: &Preferences, token: &str) -> Color32 {
    let [r,g,b] = prefs.color(&format!("scripting_expressions.expr_color_{token}"));
    Color32::from_rgb(r,g,b)
}

pub(super) fn show(ui: &mut Ui, prefs: &Preferences) {
    let font = FontId::monospace(prefs.int(bae_prefs::keys::EXPR_FONT_SIZE) as f32);
    let style = prefs.str("scripting_expressions.expr_folding");
    let numbers = prefs.bool("scripting_expressions.expr_show_line_numbers");
    let guides = prefs.bool("scripting_expressions.expr_show_indent_guides");
    let syntax = prefs.bool("scripting_expressions.expr_syntax_highlighting");
    let wrap = prefs.str("scripting_expressions.expr_word_wrap");
    let tab = prefs.int("scripting_expressions.expr_tab_width").clamp(1,16) as u8;
    let id = ui.make_persistent_id("expression-preview-fold");
    let collapsed = style != "off" && ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let number_width = if numbers { font.size + 6.0 } else { 0.0 };
    let fold_width = if style == "off" { 0.0 } else { 14.0 };
    let width = ui.available_width().max(1.0);
    let gutter = number_width + fold_width + 4.0;

    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = if wrap == "none" { f32::INFINITY } else { (width-gutter-6.0).max(1.0) };
    job.wrap.break_anywhere = wrap == "character";
    job.tab_stop_spaces = std::num::NonZeroU8::new(tab);
    let mut add = |source: &str, token| {
        job.append(source, 0.0, TextFormat::simple(font.clone(), color(prefs, if syntax { token } else { "default" })));
    };
    add("function ", "keywords");
    add("offset", "identifiers");
    add("() {", "operators");
    if collapsed {
        add(" … }", "default");
    } else {
        add("\n\t// wiggle the position of this layer over time", "comments");
        add("\n\treturn ", "keywords");
        add("thisLayer.name", "identifiers");
        add(" === ", "operators");
        add("\"Camera\"", "strings");
        add(" ? value : ", "default");
        add("wiggle", "identifiers");
        add("(", "operators");
        add("2", "numbers");
        add(", ", "default");
        add("20", "numbers");
        add(");\n}", "operators");
    }
    add("\noffset();", "default");
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y + 8.0), Sense::hover());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 1.0, color(prefs, "background"));
    let origin = rect.min + vec2(gutter,4.0);
    let first = Rect::from_min_size(rect.min + vec2(number_width,4.0),vec2(fold_width,galley.rows[0].size.y));
    if style != "off" {
        let toggle = ui.interact(first, id, Sense::click()).labeled_toggle(
            egui::WidgetType::CollapsingHeader, tr(if collapsed { "Expand" } else { "Collapse" }), !collapsed);
        if toggle.clicked() {
            ui.data_mut(|d| d.insert_temp(id, !collapsed));
            ui.ctx().request_repaint();
        }
        let mark = Rect::from_center_size(first.center(),vec2(9.0,9.0));
        let ink = color(prefs,"line_numbers");
        let stroke = Stroke::new(1.0,ink);
        if style == "arrows" {
            icons::paint(&painter,mark,if collapsed { icons::Icon::ChevronRight } else { icons::Icon::ChevronDown },ink);
        } else {
            match style {
                "circle_tree" => { painter.circle_stroke(mark.center(),4.5,stroke); },
                "box_tree" => { painter.rect_stroke(mark,0.0,stroke,StrokeKind::Inside); },
                _ => {}
            }
            painter.hline(mark.left()+2.0..=mark.right()-2.0,mark.center().y,stroke);
            if collapsed { painter.vline(mark.center().x,mark.top()+2.0..=mark.bottom()-2.0,stroke); }
        }
    }
    let mut line = 1;
    let mut start = true;
    for row in &galley.rows {
        let y = origin.y + row.pos.y;
        if !collapsed && (2..=4).contains(&line) && matches!(style, "circle_tree" | "box_tree") {
            let x = first.center().x;
            let end = if line == 4 { y + row.size.y*0.5 } else { y+row.size.y };
            let stroke = Stroke::new(1.0,color(prefs,"line_numbers"));
            painter.vline(x,y..=end,stroke);
            if line == 4 { painter.hline(x..=x+3.0,end,stroke); }
        }
        if numbers && start {
            painter.text(pos2(first.left()-2.0,y),Align2::RIGHT_TOP,line.to_string(),font.clone(),color(prefs,"line_numbers"));
        }
        if guides && start && let Some(glyph) = row.glyphs.first().filter(|g| g.chr == '\t') {
            painter.vline(origin.x+glyph.pos.x+glyph.advance_width, y..=y+row.size.y,Stroke::new(1.0,color(prefs,"indent_guides")));
        }
        start = row.ends_with_newline;
        if start { line += if collapsed && line == 1 { 4 } else { 1 }; }
    }
    painter.galley(origin,galley,color(prefs,"default"));
}
