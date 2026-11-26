//! Shared presentation of the Timeline and Graph Editor expression fields.
use bae_app::Resources;
use egui::text::LayoutJob;
use egui::text_edit::TextEditOutput;
use egui::{Align2, Color32, FontId, Galley, Id, Rect, Stroke, TextBuffer, TextEdit, Ui, pos2};
use std::sync::Arc;

mod syntax;

fn display_preferences(resources: &Resources) -> Arc<bae_prefs::Preferences> {
    resources
        .get::<Arc<bae_prefs::PrefsStore>>()
        .map(|s| s.display())
        .unwrap_or_default()
}

fn gutter_width(ui: &Ui, source: &str, font: &FontId, width: f32, numbers: bool) -> f32 {
    if !numbers {
        return 0.0;
    }
    let count = source.bytes().filter(|b| *b == b'\n').count() + 1;
    let label = ui
        .painter()
        .layout_no_wrap(count.to_string(), font.clone(), Color32::WHITE);
    (label.size().x + 8.0).min(width * 0.25)
}

fn layout(
    ui: &Ui,
    source: &str,
    font: &FontId,
    text: Color32,
    prefs: &bae_prefs::Preferences,
    width: f32,
    caret: Option<usize>,
) -> Arc<Galley> {
    let wrap = prefs.str("scripting_expressions.expr_word_wrap");
    let mut job = LayoutJob::simple(
        source.to_owned(),
        font.clone(),
        text,
        if wrap == "none" {
            f32::INFINITY
        } else {
            width.max(1.0)
        },
    );
    if prefs.bool("scripting_expressions.expr_syntax_highlighting") {
        syntax::colorize(ui, &mut job, prefs, caret);
    }
    job.wrap.break_anywhere = wrap == "character";
    job.tab_stop_spaces = std::num::NonZeroU8::new(
        prefs
            .int("scripting_expressions.expr_tab_width")
            .clamp(1, 16) as u8,
    );
    ui.fonts_mut(|f| f.layout_job(job))
}

pub(crate) fn height(ui: &Ui, resources: &Resources, source: &str, width: f32) -> f32 {
    let prefs = display_preferences(resources);
    let (font, text, _) = crate::common::expression_style(resources);
    let gutter = gutter_width(
        ui,
        source,
        &font,
        width,
        prefs.bool("scripting_expressions.expr_show_line_numbers"),
    );
    layout(ui, source, &font, text, &prefs, width - gutter - 8.0, None)
        .size()
        .y
}

pub(super) fn show(
    ui: &mut Ui,
    resources: &Resources,
    rect: Rect,
    id: Id,
    buffer: &mut String,
    rows: Option<usize>,
) -> TextEditOutput {
    let prefs = display_preferences(resources);
    let color = |key| {
        let [r, g, b] = prefs.color(key);
        Color32::from_rgb(r, g, b)
    };
    let (font, text, bg) = crate::common::expression_style(resources);
    let numbers = rows.is_some() && prefs.bool("scripting_expressions.expr_show_line_numbers");
    let guides = rows.is_some() && prefs.bool("scripting_expressions.expr_show_indent_guides");
    let wrap = prefs.str("scripting_expressions.expr_word_wrap");
    let mut field = rect;
    if numbers {
        field.min.x += gutter_width(ui, buffer, &font, rect.width(), numbers);
        ui.painter().rect_filled(
            Rect::from_min_max(rect.min, pos2(field.left(), rect.bottom())),
            0.0,
            bg,
        );
    }
    let shown = ui
        .scope_builder(egui::UiBuilder::new().max_rect(field), |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(field));
            let [r, g, b] = prefs.color("scripting_expressions.expr_color_selection");
            let alpha =
                (prefs.float("scripting_expressions.expr_selection_alpha") * 255.0).round() as u8;
            ui.visuals_mut().selection.bg_fill = Color32::from_rgba_unmultiplied(r, g, b, alpha);
            let vertical = rows.is_some()
                && layout(ui, buffer, &font, text, &prefs, field.width() - 8.0, None)
                    .size()
                    .y
                    > field.height() + 0.5;
            let mut edit = |ui: &mut Ui| {
                let mut input = super::typing::ExpressionBuffer::new(ui, buffer, &prefs);
                let indented = rows.is_some() && input.indent_selection(ui, id);
                let caret = if ui.memory(|m| m.has_focus(id))
                    && prefs.bool("scripting_expressions.expr_highlight_matching_braces")
                {
                    TextEdit::load_state(ui.ctx(), id)
                        .and_then(|s| s.cursor.char_range())
                        .filter(|r| r.is_empty())
                        .map(|r| usize::from(r.primary.index))
                } else {
                    None
                };
                let mut layouter = |ui: &Ui, buffer: &dyn TextBuffer, width: f32| {
                    let width = if rows.is_some() { width } else { f32::INFINITY };
                    layout(ui, buffer.as_str(), &font, text, &prefs, width, caret)
                };
                let widget = match rows {
                    Some(_) => TextEdit::multiline(&mut input)
                        .lock_focus(true)
                        .margin(egui::Margin::symmetric(4, 0))
                        .desired_rows(1),
                    None => TextEdit::singleline(&mut input),
                };
                let mut shown = widget
                    .layouter(&mut layouter)
                    .id(id)
                    .font(font.clone())
                    .text_color(text)
                    .background_color(bg)
                    .min_size(field.size())
                    .desired_width(field.width())
                    .show(ui);
                if indented {
                    shown.response.response.mark_changed();
                }
                shown
            };
            if rows.is_some() && (wrap == "none" || vertical) {
                egui::ScrollArea::new([wrap == "none", vertical])
                    .id_salt(id.with("scroll"))
                    .auto_shrink([false, false])
                    .show(ui, edit)
                    .inner
            } else {
                edit(ui)
            }
        })
        .inner;

    if numbers || guides {
        let painter = ui.painter_at(rect);
        let line_color = color("scripting_expressions.expr_color_line_numbers");
        let guide = Stroke::new(1.0, color("scripting_expressions.expr_color_indent_guides"));
        let tab = prefs.int("scripting_expressions.expr_tab_width").max(1) as usize;
        let mut number = 1;
        let mut hard_start = true;
        for row in &shown.galley.rows {
            let origin = shown.galley_pos + row.pos.to_vec2();
            if numbers && hard_start {
                painter.text(
                    pos2(field.left() - 4.0, origin.y),
                    Align2::RIGHT_TOP,
                    number.to_string(),
                    font.clone(),
                    line_color,
                );
            }
            if guides && hard_start {
                let mut columns = 0;
                for glyph in &row.glyphs {
                    match glyph.chr {
                        ' ' => columns += 1,
                        '\t' => columns = (columns / tab + 1) * tab,
                        _ => break,
                    }
                    if columns % tab == 0 {
                        let x = origin.x + glyph.pos.x + glyph.advance_width;
                        if field.x_range().contains(x) {
                            painter.vline(x, origin.y..=origin.y + row.size.y, guide);
                        }
                    }
                }
            }
            hard_start = row.ends_with_newline;
            if hard_start {
                number += 1;
            }
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_stops_align_with_spaces_at_the_configured_width() {
        let ctx = bae_ui::testing::ctx_with_fonts();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let mut prefs = bae_prefs::Preferences::default();
            for width in [2, 7] {
                prefs.set("scripting_expressions.expr_tab_width", width);
                let source = format!(" \tX\n{}X", " ".repeat(width as usize));
                let galley = layout(
                    ui,
                    &source,
                    &FontId::monospace(12.0),
                    Color32::WHITE,
                    &prefs,
                    400.0,
                    None,
                );
                let x = |line: usize| galley.rows[line].glyphs.last().unwrap().pos.x;
                assert!(
                    (x(0) - x(1)).abs() <= 1.0,
                    "tab width {width}: {} vs {}",
                    x(0),
                    x(1)
                );
            }
        });
        output.textures_delta.clear();
    }

    #[test]
    fn wrapping_changes_visual_rows_without_changing_the_source() {
        let src = "short words fit here then wrap here";
        for mode in ["none", "word", "character"] {
            let ctx = bae_ui::testing::ctx_with_fonts();
            let mut resources = Resources::new();
            let mut prefs = bae_prefs::Preferences::default();
            prefs.set("scripting_expressions.expr_word_wrap", mode);
            resources.insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(prefs)));
            let mut source = src.to_owned();
            let mut output = ctx.run_ui(Default::default(), |ui| {
                let rect = Rect::from_min_size(pos2(10.0, 10.0), egui::vec2(112.0, 100.0));
                let output = show(
                    ui,
                    &resources,
                    rect,
                    Id::new("editor"),
                    &mut source,
                    Some(4),
                );
                assert_eq!(output.galley.rows.len() > 1, mode != "none", "{mode}");
                assert_eq!(output.galley.job.wrap.break_anywhere, mode == "character");
                assert_eq!(output.galley.text(), src);
            });
            output.textures_delta.clear();
            assert_eq!(source, src);
        }
    }
}
