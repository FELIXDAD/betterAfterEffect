//! Colour runs in the original editable source, including brace pairs at the caret.
use bae_anim::{ExpressionToken, expression_tokens};
use egui::cache::{ComputerMut, FrameCache};
use egui::text::{LayoutJob, LayoutSection};
use egui::{Color32, Ui};
use std::{ops::Range, sync::Arc};

#[derive(Default)]
struct Scanner;

struct Syntax {
    tokens: Vec<(Range<usize>, ExpressionToken)>,
    // Source byte offset and the matching offset, when the pair is balanced.
    braces: Vec<(usize, Option<usize>)>,
}

impl ComputerMut<&str, Arc<Syntax>> for Scanner {
    fn compute(&mut self, source: &str) -> Arc<Syntax> {
        let tokens = expression_tokens(source);
        let mut braces = Vec::new();
        let mut open = Vec::new();
        for (range, kind) in &tokens {
            if *kind != ExpressionToken::Operator { continue }
            let key = &source[range.clone()];
            let at = range.start;
            match key {
                "(" | "[" | "{" => {
                    open.push((key, braces.len()));
                    braces.push((at, None));
                }
                ")" | "]" | "}" => {
                    let mut mate = None;
                    if let Some((key_open, index)) = open.pop()
                        && matches!((key_open, key), ("(", ")") | ("[", "]") | ("{", "}"))
                    {
                        let entry: &mut (usize, Option<usize>) = &mut braces[index];
                        entry.1 = Some(at);
                        mate = Some(entry.0);
                    }
                    braces.push((at, mate));
                }
                _ => {}
            }
        }
        Arc::new(Syntax { tokens, braces })
    }
}

impl Syntax {
    fn at_caret(&self, source: &str, caret: usize) -> Option<(usize, Option<usize>)> {
        let byte = source.char_indices().nth(caret).map_or(source.len(), |(i, _)| i);
        let find = |i| self.braces.binary_search_by_key(&i, |e| e.0).ok().map(|n| self.braces[n]);
        byte.checked_sub(1).and_then(find).or_else(|| find(byte))
    }
}

pub(super) fn colorize(ui: &Ui, job: &mut LayoutJob, prefs: &bae_prefs::Preferences, caret: Option<usize>) {
    let syntax = ui.memory_mut(|m| m.caches.cache::<FrameCache<Arc<Syntax>, Scanner>>().get(job.text.as_str()).clone());
    if syntax.tokens.is_empty() { return }
    let rgb = |key| {
        let [r, g, b] = prefs.color(key);
        Color32::from_rgb(r,g,b)
    };
    let colors = [
        rgb("scripting_expressions.expr_color_keywords"),
        rgb("scripting_expressions.expr_color_identifiers"),
        rgb("scripting_expressions.expr_color_numbers"),
        rgb("scripting_expressions.expr_color_strings"),
        rgb("scripting_expressions.expr_color_operators"),
        rgb("scripting_expressions.expr_color_comments"),
    ];
    let highlight = caret.and_then(|c| syntax.at_caret(&job.text, c));
    let brace_color = highlight.map(|(_, mate)| rgb(if mate.is_some() {
        "scripting_expressions.expr_color_correct_braces"
    } else {
        "scripting_expressions.expr_color_incorrect_braces"
    }));
    let plain = job.sections[0].format.clone();
    job.sections.clear();
    let mut append = |range: Range<usize>, color| {
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: range,
            format: egui::TextFormat { color, ..plain.clone() },
        });
    };
    let mut end = 0;
    for (span, kind) in &syntax.tokens {
        if end < span.start { append(end..span.start, plain.color); }
        let color = match (highlight, brace_color) {
            (Some((at, mate)), Some(c)) if span.start == at || Some(span.start) == mate => c,
            _ => colors[*kind as usize],
        };
        append(span.clone(), color);
        end = span.end;
    }
    if end < job.text.len() { append(end..job.text.len(), plain.color); }
}
