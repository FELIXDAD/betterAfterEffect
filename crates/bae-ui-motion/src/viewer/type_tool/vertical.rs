//! Native text input uses source indices. Project the cursor, selection and IME
//! rectangle onto the shaped columns that the viewer actually displays.
use super::*;
use egui::text::{CCursor, CCursorRange};

impl Editing {
    pub(super) fn update_preview(&mut self, template: Option<&TextDocument>, painter: &Painter) {
        let mut doc = template.cloned().unwrap_or_else(|| {
            TextDocument::new("", bae_motion::text::DEFAULT_FONT, self.font_size)
        });
        if doc.text == self.original {
            for edit in &self.edits {
                doc.replace_range(edit.range.clone(), &edit.text);
            }
        }
        if doc.text != self.buffer {
            doc.set_text(&self.buffer);
        }
        doc.line_orientation = self.orientation;
        if let Some((pos, size)) = self.text_box {
            doc.box_position = Some(pos);
            doc.box_size = Some(size);
        }
        self.preview = Preview::get(painter, text_id().with("glyphs"), &doc);
    }

    fn screen_edge(&self, view: &View, points: [[f64; 2]; 2]) -> [Pos2; 2] {
        points.map(|p| self.screen(view, p))
    }

    pub(super) fn vertical_field(&self, view: &View) -> Rect {
        let mut r = Rect::NOTHING;
        let mut include = |x0, y0, x1, y1| {
            for p in [[x0, y0], [x1, y0], [x0, y1], [x1, y1]] {
                r.extend_with(self.screen(view, p));
            }
        };
        if let Some((p, s)) = self.text_box {
            include(p[0], p[1], p[0] + s[0], p[1] + s[1]);
        } else {
            let em = self.font_size;
            include(-em / 2., 0., em / 2., em);
            if let Some(p) = &self.preview {
                for l in &p.layout.lines {
                    let b = p.layout.line_rect(l);
                    include(b.x0, b.y0, b.x1, b.y1);
                }
            }
        }
        r.expand(2.)
    }

    fn caret(&self, index: usize) -> ([[f64; 2]; 2], usize) {
        if let Some(preview) = &self.preview {
            let layout = &preview.layout;
            let cluster = layout.chars.iter().find(|c| c.source.contains(&index));
            if let Some(c) = cluster.or_else(|| layout.chars.last()) {
                if cluster.is_none()
                    && c.kind == bae_text::CharKind::LineBreak
                    && let Some(line) = layout.lines.get(c.line + 1)
                {
                    return (
                        [
                            [line.baseline - line.descent, line.x],
                            [line.baseline + line.ascent, line.x],
                        ],
                        c.line + 1,
                    );
                }
                let end = (index >= c.source.end) != c.right_to_left;
                return (layout.caret_edge(c, end).map(|p| [p.x, p.y]), c.line);
            }
        }
        let em = self.font_size;
        let (x, y) = self
            .text_box
            .map_or((0., 0.), |(p, s)| (p[0] + s[0] - em / 2., p[1]));
        ([[x - em / 2., y], [x + em / 2., y]], 0)
    }

    pub(super) fn vertical_navigation(&self, ui: &Ui) {
        if !ui.memory(|m| m.has_focus(text_id())) {
            return;
        }
        let Some(preview) = &self.preview else { return };
        let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), text_id()) else {
            return;
        };
        let Some(mut cursor) = state.cursor.char_range() else {
            return;
        };
        ui.input_mut(|input| {
            input.events.retain_mut(|event| {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                else {
                    return true;
                };
                match key {
                    Key::ArrowUp => *key = Key::ArrowLeft,
                    Key::ArrowDown => *key = Key::ArrowRight,
                    Key::ArrowLeft | Key::ArrowRight => {
                        let (edge, col) = self.caret(cursor.primary.index.0);
                        let next = if *key == Key::ArrowLeft {
                            col.checked_add(1)
                        } else {
                            col.checked_sub(1)
                        };
                        if let Some(line) = next.and_then(|n| preview.layout.lines.get(n))
                            && let Some(index) = preview
                                .layout
                                .cursor_at(line.baseline, (edge[0][1] + edge[1][1]) / 2.)
                        {
                            cursor.primary = CCursor::new(index);
                            if !modifiers.shift {
                                cursor.secondary = cursor.primary;
                            }
                        }
                        return false;
                    }
                    _ => {}
                }
                true
            })
        });
        state.cursor.set_char_range(Some(cursor));
        state.store(ui.ctx(), text_id());
    }

    pub(super) fn vertical_drag(&self, ui: &Ui, view: &View, rect: Rect) -> Option<CCursorRange> {
        let preview = self.preview.as_ref()?;
        if !ui.input(|i| i.pointer.primary_down() || i.pointer.primary_released()) {
            return None;
        }
        let (origin, end) = ui.input(|i| (i.pointer.press_origin(), i.pointer.latest_pos()));
        let start = origin.filter(|p| rect.contains(*p))?;
        let at = |p| {
            let local = self
                .to_comp
                .inverse()
                .transform_point3(view.to_comp(p).extend(0.));
            preview.layout.cursor_at(local.x, local.y).map(CCursor::new)
        };
        Some(CCursorRange::two(at(start)?, at(end?)?))
    }

    pub(super) fn paint_vertical(
        &self,
        ui: &Ui,
        painter: &Painter,
        view: &View,
        cursor: Option<CCursorRange>,
    ) {
        if let Some(p) = &self.preview {
            if let Some(range) = &self.selection {
                for c in &p.layout.chars {
                    if c.source.start >= range.end || c.source.end <= range.start {
                        continue;
                    }
                    let [a, b] = p.layout.caret_edge(c, false);
                    let [d, e] = p.layout.caret_edge(c, true);
                    let corners = [a, b, e, d].map(|p| self.screen(view, [p.x, p.y]));
                    painter.add(egui::Shape::convex_polygon(
                        corners.to_vec(),
                        ui.visuals().selection.bg_fill,
                        Stroke::NONE,
                    ));
                }
            }
            painter.add(p.shape(|p| self.screen(view, p), 1.));
        }
        if let Some(c) = cursor {
            let edge = self.screen_edge(view, self.caret(c.primary.index.0).0);
            painter.line_segment(edge, Stroke::new(1.5, Color32::WHITE));
            ui.ctx().output_mut(|out| {
                if let Some(ime) = &mut out.ime {
                    ime.cursor_rect = Rect::from_two_pos(edge[0], edge[1]).expand(1.);
                }
            });
        }
    }
}
