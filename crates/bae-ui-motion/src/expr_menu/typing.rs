//! Expression typing rules. The underlying TextEdit keeps its own undo, selection and IME.
use bae_prefs::Preferences;
use egui::epaint::text::CharIndex;
use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{Event, Id, Key, Modifiers, TextBuffer, Ui};
use std::any::TypeId;
use std::ops::Range;

pub(super) struct ExpressionBuffer<'a> {
    source: &'a mut String,
    prefs: &'a Preferences,
    literal: bool,
}

impl<'a> ExpressionBuffer<'a> {
    pub(super) fn new(ui: &Ui, source: &'a mut String, prefs: &'a Preferences) -> Self {
        let literal = ui.input(|i| {
            i.events
                .iter()
                .any(|event| matches!(event, Event::Paste(_) | Event::Ime(_)))
        });
        Self {
            source,
            prefs,
            literal,
        }
    }

    fn width(&self) -> usize {
        self.prefs
            .int("scripting_expressions.expr_tab_width")
            .clamp(1, 16) as usize
    }

    fn unit(&self, column: usize) -> String {
        if self.prefs.bool("scripting_expressions.expr_use_tabs") {
            "\t".into()
        } else {
            " ".repeat(self.width() - column % self.width())
        }
    }

    fn line_before(&self, cursor: CCursor) -> &str {
        let byte = self.source.byte_index_from_char_index(cursor.index);
        self.source[..usize::from(byte)]
            .rsplit('\n')
            .next()
            .unwrap_or("")
    }

    fn columns(&self, text: &str) -> usize {
        let width = self.width();
        text.chars().fold(0, |col, ch| {
            if ch == '\t' {
                col + width - col % width
            } else {
                col + 1
            }
        })
    }

    /// TextEdit's default Tab replaces a selection. In code, indent the selected lines together.
    pub(super) fn indent_selection(&mut self, ui: &Ui, id: Id) -> bool {
        if !ui.memory(|m| m.has_focus(id)) {
            return false;
        }
        let Some(mut state) = TextEditState::load(ui.ctx(), id) else {
            return false;
        };
        let Some(range) = state.cursor.char_range().filter(|r| !r.is_empty()) else {
            return false;
        };
        let unindent = ui.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Tab));
        if !unindent && !ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
            return false;
        }
        let [lo, hi] = range.sorted_cursors();
        let lo = usize::from(self.source.byte_index_from_char_index(lo.index));
        let hi = usize::from(self.source.byte_index_from_char_index(hi.index));
        let start = self.source[..lo].rfind('\n').map_or(0, |i| i + 1);
        let end = if self.source[..hi].ends_with('\n') {
            hi
        } else {
            self.source[hi..]
                .find('\n')
                .map_or(self.source.len(), |i| hi + i)
        };
        let unit = self.unit(0);
        let mut block = String::new();
        for line in self.source[start..end].split_inclusive('\n') {
            if unindent {
                block.push_str(&line[self.indent_bytes(line)..]);
            } else {
                block.push_str(&unit);
                block.push_str(line);
            }
        }
        let first = self.source[..start].chars().count();
        let last = first + block.chars().count();
        self.source.replace_range(start..end, &block);
        let mut selection = CCursorRange::two(CCursor::new(first), CCursor::new(last));
        if range.primary.index < range.secondary.index {
            std::mem::swap(&mut selection.primary, &mut selection.secondary);
        }
        state.cursor.set_char_range(Some(selection));
        state.store(ui.ctx(), id);
        true
    }

    fn indent_bytes(&self, line: &str) -> usize {
        if line.starts_with('\t') {
            1
        } else {
            line.bytes()
                .take(self.width())
                .take_while(|ch| *ch == b' ')
                .count()
        }
    }
}

impl TextBuffer for ExpressionBuffer<'_> {
    fn type_id(&self) -> TypeId {
        TypeId::of::<ExpressionBuffer<'static>>()
    }
    fn is_mutable(&self) -> bool {
        true
    }
    fn as_str(&self) -> &str {
        self.source
    }
    fn insert_text(&mut self, text: &str, at: CharIndex) -> usize {
        self.source.insert_text(text, at)
    }
    fn delete_char_range(&mut self, range: Range<CharIndex>) {
        self.source.delete_char_range(range)
    }

    fn insert_text_at(&mut self, cursor: &mut CCursor, text: &str, limit: usize) {
        if self.literal {
            self.source.insert_text_at(cursor, text, limit);
            return;
        }
        let before = self.line_before(*cursor);
        let byte = usize::from(self.source.byte_index_from_char_index(cursor.index));
        let next = self.source[byte..].chars().next();
        let auto_pair = self
            .prefs
            .bool("scripting_expressions.expr_auto_insert_braces");
        let closing = match text {
            "(" => ")",
            "[" => "]",
            "{" => "}",
            _ => "",
        };
        if auto_pair && !closing.is_empty() {
            self.source.insert_text_at(cursor, text, limit);
            self.source.insert_text(closing, cursor.index);
        } else if auto_pair && matches!(text, ")" | "]" | "}") && text.chars().next() == next {
            cursor.index += 1;
        } else if text == "\t" {
            let indent = self.unit(self.columns(before));
            self.source.insert_text_at(cursor, &indent, limit);
        } else if text == "\n"
            && self
                .prefs
                .bool("scripting_expressions.expr_auto_indent_braces")
        {
            let indent = &before[..before.len() - before.trim_start_matches([' ', '\t']).len()];
            let opener = before.trim_end().chars().last();
            let closer = match opener {
                Some('(') => Some(')'),
                Some('[') => Some(']'),
                Some('{') => Some('}'),
                _ => None,
            };
            let mut line = format!("\n{indent}");
            let closing_line = if closer.is_some() {
                line += &self.unit(self.columns(indent));
                (closer == next).then(|| format!("\n{indent}"))
            } else {
                None
            };
            self.source.insert_text_at(cursor, &line, limit);
            if let Some(closing_line) = closing_line {
                self.source.insert_text(&closing_line, cursor.index);
            }
        } else {
            self.source.insert_text_at(cursor, text, limit);
        }
    }

    fn delete_previous_char(&mut self, mut cursor: CCursor) -> CCursor {
        let before = self.line_before(cursor);
        if self
            .prefs
            .bool("scripting_expressions.expr_backspace_unindents")
            && !before.is_empty()
            && before.ends_with(' ')
            && before.trim_matches([' ', '\t']).is_empty()
        {
            let to_stop = (self.columns(before) - 1) % self.width() + 1;
            let count = before.len() - before.trim_end_matches(' ').len();
            let start = cursor.index - to_stop.min(count);
            self.source.delete_char_range(start..cursor.index);
            cursor.index = start;
            cursor
        } else {
            self.source.delete_previous_char(cursor)
        }
    }

    fn decrease_indentation(&mut self, cursor: &mut CCursor) {
        let start = cursor.index - self.line_before(*cursor).chars().count();
        let from = usize::from(self.source.byte_index_from_char_index(start));
        let line = &self.source[from..];
        let count = self.indent_bytes(line);
        self.source.delete_char_range(start..start + count);
        cursor.index -= count.min(cursor.index.0 - start.0);
    }
}
