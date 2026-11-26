//! Expression completion and call signatures shared by both editors.
use super::{Group, MENU};
use bae_app::Resources;
use bae_ui::i18n::tr;
use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui::text_edit::{TextEditOutput, TextEditState};
use egui::{Id, Key, Modifiers, Rect, TextEdit, TextFormat, Ui};
use std::ops::Range;
use std::sync::Arc;

fn signatures() -> impl Iterator<Item = &'static str> {
    MENU.iter()
        .flat_map(|(_, group)| {
            let (top, subs) = match group {
                Group::Items(top) => (*top, &[][..]),
                Group::Sub(subs) => (&[][..], *subs),
            };
            top.iter()
                .chain(subs.iter().flat_map(|(_, list)| list.iter()))
        })
        .copied()
        .filter(|s| s.contains('('))
}

fn function_name(signature: &str) -> &str {
    signature
        .split_once('(')
        .unwrap()
        .0
        .rsplit('.')
        .next()
        .unwrap()
}

fn byte_at(text: &str, character: usize) -> usize {
    text.char_indices()
        .nth(character)
        .map_or(text.len(), |(byte, _)| byte)
}

fn ident(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$')
}

/// Identify the word to replace and the innermost call at a character cursor.
/// Commas in literals, comments, nested calls and array defaults do not advance the outer call.
#[derive(Debug)]
struct Context<'a> {
    word: Range<usize>,
    prefix: &'a str,
    call: Option<(&'a str, usize)>,
}

fn context(text: &str, caret: usize) -> Option<Context<'_>> {
    let cursor = byte_at(text, caret);
    let before = &text[..cursor];
    let mut tokens = before.char_indices().peekable();
    let mut stack = Vec::new();
    let mut quote = None;
    let mut line_comment = false;
    let mut block_comment = false;
    while let Some((i, c)) = tokens.next() {
        if line_comment {
            line_comment = c != '\n';
            continue;
        }
        if block_comment {
            if c == '*' && tokens.next_if(|(_, n)| *n == '/').is_some() {
                block_comment = false;
            }
            continue;
        }
        if let Some(q) = quote {
            if c == '\\' {
                tokens.next();
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '/' if tokens.next_if(|(_, n)| *n == '/').is_some() => line_comment = true,
            '/' if tokens.next_if(|(_, n)| *n == '*').is_some() => block_comment = true,
            '\'' | '"' | '\x60' => quote = Some(c),
            '(' | '[' | '{' => {
                let name = before[..i]
                    .trim_end()
                    .rsplit(|ch| !ident(ch))
                    .next()
                    .unwrap_or("");
                stack.push((if c == '(' { name } else { "" }, 0));
            }
            ')' | ']' | '}' => {
                stack.pop();
            }
            ',' => {
                if let Some((_, arg)) = stack.last_mut() {
                    *arg += 1;
                }
            }
            _ => {}
        }
    }
    if quote.is_some() || block_comment || line_comment {
        return None;
    }
    let prefix = before.rsplit(|c| !ident(c)).next()?;
    let suffix = text[cursor..].split(|c| !ident(c)).next()?;
    Some(Context {
        word: cursor - prefix.len()..cursor + suffix.len(),
        prefix,
        call: stack.into_iter().rev().find(|(name, _)| !name.is_empty()),
    })
}

fn candidates(cx: &Context<'_>, manual: bool) -> Vec<&'static str> {
    if cx.prefix.is_empty() && !manual {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for signature in signatures() {
        let name = function_name(signature);
        if name.starts_with(cx.prefix) && !hits.iter().any(|&s| function_name(s) == name) {
            hits.push(signature);
        }
    }
    hits
}

fn insert(buffer: &mut String, word: Range<usize>, signature: &str) -> usize {
    let name = function_name(signature);
    // Qualify Math methods at a bare cursor and keep any receiver already typed.
    let name = if (signature.starts_with("Math.") || signature.starts_with("style."))
        && !buffer[..word.start].ends_with('.')
    {
        signature.split_once('(').unwrap().0
    } else {
        name
    };
    let has_parens = buffer[word.end..].starts_with('(');
    let caret = buffer[..word.start].chars().count() + name.chars().count() + 1;
    buffer.replace_range(
        word,
        &format!("{name}{}", if has_parens { "" } else { "()" }),
    );
    caret
}

#[derive(Clone, Default)]
struct State {
    selected: usize,
    manual: bool,
    dismissed: Option<(String, usize)>,
    popup: Option<Rect>,
}

fn set_caret(ui: &Ui, id: Id, caret: usize) -> CCursorRange {
    let range = CCursorRange::one(CCursor::new(caret));
    let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
    state.cursor.set_char_range(Some(range));
    state.store(ui.ctx(), id);
    range
}

fn signature_label(ui: &mut Ui, signature: &str, arg: usize) {
    let Some(open) = signature.find('(') else {
        return;
    };
    let mut start = open + 1;
    let mut depth = 0;
    let mut current = 0;
    let mut active = 0..0;
    for (i, c) in signature.char_indices().skip(open + 1) {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' | ')' if depth == 0 => {
                if current == arg {
                    active = start..i;
                    break;
                }
                start = i + 1;
                current += 1;
            }
            _ => {}
        }
    }
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let mut job = LayoutJob::default();
    for (part, color) in [
        (&signature[..active.start], ui.visuals().weak_text_color()),
        (&signature[active.clone()], ui.visuals().strong_text_color()),
        (&signature[active.end..], ui.visuals().weak_text_color()),
    ] {
        job.append(
            part,
            0.0,
            TextFormat {
                font_id: font.clone(),
                color,
                ..Default::default()
            },
        );
    }
    ui.label(job);
}

pub(crate) fn show(
    ui: &mut Ui,
    resources: &Resources,
    rect: Rect,
    id: Id,
    buffer: &mut String,
    rows: Option<usize>,
) -> TextEditOutput {
    let enabled = resources
        .get::<Arc<bae_prefs::PrefsStore>>()
        .is_none_or(|p| p.current().bool("scripting_expressions.expr_auto_complete"));
    let state_id = id.with("expression hints");
    let mut state = ui
        .data(|d| d.get_temp::<State>(state_id))
        .unwrap_or_default();
    let in_popup = state.popup.is_some_and(|r| {
        ui.input(|i| {
            i.pointer.interact_pos().is_some_and(|pos| r.contains(pos))
                && (i.pointer.primary_down() || i.pointer.any_released())
        })
    });
    if in_popup {
        ui.memory_mut(|m| m.request_focus(id));
    }
    let previous_selection = state.selected;
    let has_focus = ui.memory(|m| m.has_focus(id));
    if enabled
        && has_focus
        && let Some(range) = TextEditState::load(ui.ctx(), id).and_then(|s| s.cursor.char_range())
        && range.is_empty()
    {
        let caret = range.primary.index.into();
        if let Some(cx) = context(buffer, caret) {
            if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Space)) {
                state.manual = true;
                state.dismissed = None;
            }
            let hits = candidates(&cx, state.manual);
            let visible = !state
                .dismissed
                .as_ref()
                .is_some_and(|(s, c)| s == buffer && *c == caret);
            if visible
                && (!hits.is_empty() || cx.call.is_some())
                && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
            {
                state.dismissed = Some((buffer.clone(), caret));
                state.manual = false;
            } else if visible && !hits.is_empty() {
                state.selected = state.selected.min(hits.len() - 1);
                ui.input_mut(|i| {
                    if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                        state.selected = (state.selected + 1) % hits.len();
                    }
                    if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                        state.selected = (state.selected + hits.len() - 1) % hits.len();
                    }
                });
                if ui.input_mut(|i| {
                    i.consume_key(Modifiers::NONE, Key::Tab)
                        || (state.manual && i.consume_key(Modifiers::NONE, Key::Enter))
                }) {
                    let at = insert(buffer, cx.word, hits[state.selected]);
                    set_caret(ui, id, at);
                    state.manual = false;
                    state.selected = 0;
                }
            }
        }
    }
    let (font, text, bg) = crate::common::expression_style(resources);
    let mut shown = ui
        .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            let edit = if let Some(n) = rows {
                TextEdit::multiline(buffer).desired_rows(n)
            } else {
                TextEdit::singleline(buffer)
            };
            edit.id(id)
                .font(font)
                .text_color(text)
                .background_color(bg)
                .min_size(rect.size())
                .desired_width(rect.width())
                .show(ui)
        })
        .inner;
    let response = &shown.response.response;
    state.popup = None;
    if enabled
        && (response.has_focus() || in_popup)
        && let Some(range) = shown
            .cursor_range
            .or_else(|| shown.state.cursor.char_range())
        && range.is_empty()
    {
        let caret = range.primary.index.into();
        if response.changed() {
            state.selected = 0;
            state.manual = false;
            state.dismissed = None;
        }
        if let Some(cx) = context(buffer, caret)
            && !state
                .dismissed
                .as_ref()
                .is_some_and(|(s, c)| s == buffer && *c == caret)
        {
            let hits = candidates(&cx, state.manual);
            let overloads: Vec<_> = cx
                .call
                .map(|(name, _)| signatures().filter(|s| function_name(s) == name).collect())
                .unwrap_or_default();
            let mut choice = None;
            if !hits.is_empty() || !overloads.is_empty() {
                let anchor = shown
                    .galley
                    .pos_from_cursor(range.primary)
                    .translate(shown.galley_pos.to_vec2());
                let popup = egui::Popup::from_response(response)
                    .id(state_id.with("popup"))
                    .anchor(anchor)
                    .open(true)
                    .show(|ui| {
                        ui.set_max_width(460.0_f32.min(ui.ctx().content_rect().width()));
                        if !hits.is_empty() {
                            state.selected = state.selected.min(hits.len() - 1);
                            egui::ScrollArea::vertical()
                                .max_height(150.0)
                                .show(ui, |ui| {
                                    for (i, &s) in hits.iter().enumerate() {
                                        let option = ui.selectable_label(
                                            i == state.selected,
                                            function_name(s),
                                        );
                                        if i == state.selected && previous_selection != i {
                                            option.scroll_to_me(None);
                                        }
                                        if option.clicked() {
                                            choice = Some(s);
                                        }
                                    }
                                });
                            let s = hits[state.selected];
                            ui.monospace(s);
                            ui.label(tr(summary(function_name(s))));
                        } else if let Some((name, arg)) = cx.call {
                            for s in overloads {
                                signature_label(ui, s, arg);
                            }
                            ui.label(tr(summary(name)));
                        }
                    });
                state.popup = popup.map(|p| p.response.rect);
                if in_popup {
                    response.request_focus();
                }
            }
            if let Some(s) = choice {
                let at = insert(buffer, cx.word, s);
                shown.cursor_range = Some(set_caret(ui, id, at));
                shown.response.response.request_focus();
                shown.response.response.mark_changed();
                state.selected = 0;
                state.manual = false;
            }
        }
    } else if !has_focus && !in_popup {
        state = State::default();
    }
    if enabled {
        ui.data_mut(|d| d.insert_temp(state_id, state));
    }
    shown
}
// Parameter names follow Adobe's expression language reference.
macro_rules! function_help {
    ($(($name:literal, $en:literal, $zh:literal)),+ $(,)?) => {
        fn summary(name: &str) -> &'static str { match name { $($name => $en,)+ _ => "" } }
        pub(crate) const ZH: &[(&str, &str)] = &[$(($en, $zh)),+];
    };
}

function_help![
    (
        "comp",
        "Find a composition by its project name.",
        "按工程中的名称查找合成。"
    ),
    (
        "footage",
        "Find a footage item by its project name.",
        "按工程中的名称查找素材。"
    ),
    (
        "posterizeTime",
        "Set the sampling rate for the rest of this expression. Zero holds time at the first frame.",
        "设置后续表达式的采样帧率。设为零时停在首帧。"
    ),
    (
        "framesToTime",
        "Convert a frame count at the given frame rate to seconds.",
        "按给定帧率将帧数换算为秒。"
    ),
    (
        "timeToFrames",
        "Convert seconds to frames. Durations round away from zero.",
        "将秒换算为帧数。时长向远离零的方向取整。"
    ),
    (
        "timeToTimecode",
        "Format seconds as timecode at the given integer frame rate.",
        "按给定的整数帧率将秒数显示为时间码。"
    ),
    (
        "timeToFeetAndFrames",
        "Format time as film feet and the remaining frames.",
        "将时间换算为胶片英尺数和剩余帧数。"
    ),
    (
        "timeToNTSCTimecode",
        "Format time at 29.97 fps, optionally using drop-frame labels.",
        "按每秒 29.97 帧显示时间码，可使用丢帧编号。"
    ),
    (
        "timeToCurrentFormat",
        "Format time using the project display settings.",
        "按工程的时间显示设置格式化时间。"
    ),
    (
        "add",
        "Add corresponding components, padding missing dimensions with zero.",
        "对应分量相加，缺少的维度补零。"
    ),
    (
        "sub",
        "Subtract corresponding components, padding missing dimensions with zero.",
        "对应分量相减，缺少的维度补零。"
    ),
    (
        "mul",
        "Multiply each vector component by a number.",
        "将向量的每个分量乘以同一个数。"
    ),
    (
        "div",
        "Divide each vector component by a number.",
        "将向量的每个分量除以同一个数。"
    ),
    (
        "clamp",
        "Limit a number or each vector component to the supplied bounds.",
        "将数值或向量各分量限制在给定范围内。"
    ),
    (
        "dot",
        "Return the scalar dot product of two vectors.",
        "返回两个向量的点积。"
    ),
    (
        "cross",
        "Return a vector perpendicular to the two input vectors.",
        "返回与两个输入向量都垂直的向量。"
    ),
    (
        "normalize",
        "Return a unit vector in the input direction.",
        "返回与输入方向相同的单位向量。"
    ),
    (
        "length",
        "Measure a vector's magnitude or the distance between two points.",
        "计算向量的长度或两点之间的距离。"
    ),
    (
        "lookAt",
        "Return 3D orientation angles that point from one position to another.",
        "返回从起点指向目标点的三维方向角。"
    ),
    (
        "seedRandom",
        "Restart the random sequence with an offset. Use timeless to keep it fixed over time.",
        "按偏移量重置随机序列。启用 timeless 可让序列不随时间变化。"
    ),
    (
        "random",
        "Draw a uniformly distributed number or vector within the supplied range.",
        "在给定范围内均匀抽取一个数或向量。"
    ),
    (
        "gaussRandom",
        "Draw a bell-shaped random value. About 90% fall inside the specified bounds.",
        "按高斯分布抽取随机值。约九成结果落在指定范围内。"
    ),
    (
        "noise",
        "Sample continuous Perlin noise at the supplied coordinates.",
        "在给定坐标采样连续的柏林噪声。"
    ),
    (
        "linear",
        "Interpolate between two values at a constant rate.",
        "在两个值之间匀速插值。"
    ),
    (
        "ease",
        "Interpolate with zero speed at both ends.",
        "在两个值之间插值，两端速度均为零。"
    ),
    (
        "easeIn",
        "Interpolate with zero speed at the start.",
        "从零速度开始插值。"
    ),
    (
        "easeOut",
        "Interpolate with zero speed at the end.",
        "在终点减速至零。"
    ),
    (
        "rgbToHsl",
        "Convert RGBA to hue, saturation, lightness and alpha.",
        "将 RGBA 转为色相、饱和度、亮度和透明度。"
    ),
    (
        "hslToRgb",
        "Convert HSLA to red, green, blue and alpha.",
        "将 HSLA 转为红、绿、蓝和透明度。"
    ),
    (
        "hexToRgb",
        "Convert a hexadecimal color string to normalized RGBA components.",
        "将十六进制颜色字符串转为 0 到 1 的 RGBA 分量。"
    ),
    (
        "degreesToRadians",
        "Convert degrees to radians.",
        "将角度转换为弧度。"
    ),
    (
        "radiansToDegrees",
        "Convert radians to degrees.",
        "将弧度转换为角度。"
    ),
    (
        "valueAtTime",
        "Read the property value at a composition time in seconds.",
        "读取属性在指定合成时间的值，时间以秒计。"
    ),
    (
        "velocityAtTime",
        "Read the property's rate of change at the specified time.",
        "读取属性在指定时间的变化率。"
    ),
    (
        "speedAtTime",
        "Measure the speed of a spatial property at the specified time.",
        "读取空间属性在指定时间的速度大小。"
    ),
    (
        "wiggle",
        "Vary the property around its value. Frequency is in changes per second.",
        "在属性值附近产生扰动。频率以每秒变化次数计。"
    ),
    (
        "temporalWiggle",
        "Vary the sampling time of an animated property. Amplitude is in seconds.",
        "扰动动画属性的采样时间。幅度以秒计。"
    ),
    (
        "smooth",
        "Average samples across a time window centered on the supplied time.",
        "对指定时间前后窗口内的属性样本取平均值。"
    ),
    (
        "loopIn",
        "Repeat a segment before the first keyframe. Count the segment in keyframe intervals.",
        "在首个关键帧前重复一段动画，按关键帧间隔指定长度。"
    ),
    (
        "loopOut",
        "Repeat a segment after the last keyframe. Count the segment in keyframe intervals.",
        "在末个关键帧后重复一段动画，按关键帧间隔指定长度。"
    ),
    (
        "loopInDuration",
        "Repeat a segment before the first keyframe. Specify its length in seconds.",
        "在首个关键帧前重复一段动画，长度以秒计。"
    ),
    (
        "loopOutDuration",
        "Repeat a segment after the last keyframe. Specify its length in seconds.",
        "在末个关键帧后重复一段动画，长度以秒计。"
    ),
    (
        "key",
        "Read a keyframe by its one-based index, or a marker by its comment.",
        "按从 1 开始的序号读取关键帧，也可按注释查找标记。"
    ),
    (
        "nearestKey",
        "Find the keyframe or marker closest to a composition time.",
        "查找离指定合成时间最近的关键帧或标记。"
    ),
    (
        "propertyGroup",
        "Read the enclosing group the specified number of levels up.",
        "按指定层数向上查找属性组。"
    ),
    (
        "sourceRectAtTime",
        "Read the layer's local top, left, width and height at the supplied time.",
        "读取图层在指定时间的局部上边距、左边距、宽和高。"
    ),
    (
        "sampleImage",
        "Average the layer's RGBA values around a point, before or after effects.",
        "对图层上指定点附近的像素取 RGBA 平均值，可选效果前或效果后。"
    ),
    (
        "toComp",
        "Transform a point from layer space to composition space.",
        "将点从图层坐标转换到合成坐标。"
    ),
    (
        "fromComp",
        "Transform a point from composition space to layer space.",
        "将点从合成坐标转换到图层坐标。"
    ),
    (
        "toWorld",
        "Transform a point from layer space to world space.",
        "将点从图层坐标转换到世界坐标。"
    ),
    (
        "fromWorld",
        "Transform a point from world space to layer space.",
        "将点从世界坐标转换到图层坐标。"
    ),
    (
        "toCompVec",
        "Transform a direction from layer space to composition space.",
        "将方向向量从图层坐标转换到合成坐标。"
    ),
    (
        "fromCompVec",
        "Transform a direction from composition space to layer space.",
        "将方向向量从合成坐标转换到图层坐标。"
    ),
    (
        "toWorldVec",
        "Transform a direction from layer space to world space.",
        "将方向向量从图层坐标转换到世界坐标。"
    ),
    (
        "fromWorldVec",
        "Transform a direction from world space to layer space.",
        "将方向向量从世界坐标转换到图层坐标。"
    ),
    (
        "fromCompToSurface",
        "Project a composition point onto the layer's surface.",
        "将合成坐标中的点投影到图层表面。"
    ),
    (
        "getStyleAt",
        "Read a character's text style at a composition time.",
        "读取指定字符在给定合成时间的样式。"
    ),
    (
        "createStyle",
        "Create a text style to use on Source Text.",
        "创建用于源文本的样式。"
    ),
    (
        "setText",
        "Set the text returned by this style.",
        "设置这个样式返回的文字。"
    ),
    (
        "setJustification",
        "Set the paragraph alignment.",
        "设置段落对齐方式。"
    ),
    (
        "setFont",
        "Set the font by its PostScript name.",
        "用 PostScript 名称设置字体。"
    ),
    (
        "setFontSize",
        "Set the font size in pixels.",
        "设置以像素计的字号。"
    ),
    (
        "setFillColor",
        "Set the fill color from normalized RGB components.",
        "用 0 到 1 的 RGB 分量设置填充颜色。"
    ),
    (
        "setStrokeColor",
        "Set the stroke color from normalized RGB components.",
        "用 0 到 1 的 RGB 分量设置描边颜色。"
    ),
    (
        "setStrokeWidth",
        "Set the width of the text stroke.",
        "设置文字的描边宽度。"
    ),
    (
        "setTracking",
        "Set character spacing in thousandths of an em.",
        "设置字距，单位为千分之一 em。"
    ),
    (
        "setLeading",
        "Set the vertical distance between text baselines.",
        "设置相邻文本基线之间的距离。"
    ),
    (
        "setBaselineShift",
        "Move the characters relative to their baseline.",
        "移动字符相对于基线的位置。"
    ),
    (
        "setFauxBold",
        "Toggle simulated bold for the characters.",
        "切换字符的仿粗体。"
    ),
    (
        "setFauxItalic",
        "Toggle simulated italic for the characters.",
        "切换字符的仿斜体。"
    ),
    (
        "setAllCaps",
        "Toggle uppercase display for the characters.",
        "切换字符的全大写显示。"
    ),
    (
        "setSmallCaps",
        "Toggle small-cap display for the characters.",
        "切换字符的小型大写显示。"
    ),
    (
        "setBaselineOption",
        "Choose normal, superscript or subscript position.",
        "将基线位置设为常规、上标或下标。"
    ),
    (
        "setApplyFill",
        "Enable or disable the text fill.",
        "启用或关闭文字填充。"
    ),
    (
        "setApplyStroke",
        "Enable or disable the text stroke.",
        "启用或关闭文字描边。"
    ),
    (
        "setAutoLeading",
        "Enable or disable automatic line spacing.",
        "启用或关闭自动行距。"
    ),
    (
        "layer",
        "Find a layer by name, index or offset from another layer.",
        "按名称、序号或相对另一层的偏移量查找图层。"
    ),
    (
        "effect",
        "Find a layer effect by name or one-based index.",
        "按名称或从 1 开始的序号查找效果。"
    ),
    (
        "mask",
        "Find a layer mask by name or one-based index.",
        "按名称或从 1 开始的序号查找蒙版。"
    ),
    (
        "param",
        "Read an effect parameter by name or index.",
        "按名称或序号读取效果参数。"
    ),
    (
        "points",
        "Read the path vertices in local coordinates.",
        "读取路径顶点的局部坐标。"
    ),
    (
        "inTangents",
        "Read incoming tangent offsets relative to the vertices.",
        "读取各顶点入切线的相对偏移。"
    ),
    (
        "outTangents",
        "Read outgoing tangent offsets relative to the vertices.",
        "读取各顶点出切线的相对偏移。"
    ),
    (
        "isClosed",
        "Check whether the path joins its last vertex to its first.",
        "检查路径首尾是否相连。"
    ),
    (
        "pointOnPath",
        "Read a point at a fraction of the path length.",
        "按路径长度比例取点。"
    ),
    (
        "tangentOnPath",
        "Read the unit tangent at a fraction of the path length.",
        "按路径长度比例读取单位切向量。"
    ),
    (
        "normalOnPath",
        "Read the unit normal at a fraction of the path length.",
        "按路径长度比例读取单位法向量。"
    ),
    (
        "createPath",
        "Construct a path from vertices, tangent offsets and a closed flag.",
        "用顶点、切线偏移和闭合标记创建路径。"
    ),
    (
        "dataValue",
        "Read a value at the specified path in a data source.",
        "读取数据源中指定路径的值。"
    ),
    (
        "dataKeyCount",
        "Count data samples at the specified path.",
        "统计指定数据路径的样本数。"
    ),
    (
        "dataKeyTimes",
        "Read sample times for the specified data path and interval.",
        "读取指定数据路径和区间内的采样时间。"
    ),
    (
        "dataKeyValues",
        "Read sample values for the specified data path and interval.",
        "读取指定数据路径和区间内的样本值。"
    ),
    ("abs", "Return the absolute value.", "返回绝对值。"),
    (
        "acos",
        "Return the inverse cosine in radians.",
        "返回以弧度计的反余弦值。"
    ),
    (
        "atan",
        "Return the inverse tangent in radians.",
        "返回以弧度计的反正切值。"
    ),
    (
        "atan2",
        "Return the angle of the vector from its x and y components.",
        "根据向量的 x 和 y 分量计算方向角。"
    ),
    ("ceil", "Round up to an integer.", "向上取整。"),
    (
        "cos",
        "Return the cosine of an angle in radians.",
        "返回弧度角的余弦。"
    ),
    ("exp", "Raise e to the given power.", "计算 e 的指定次幂。"),
    ("floor", "Round down to an integer.", "向下取整。"),
    ("log", "Return the natural logarithm.", "返回自然对数。"),
    (
        "max",
        "Return the largest argument.",
        "返回参数中的最大值。"
    ),
    (
        "min",
        "Return the smallest argument.",
        "返回参数中的最小值。"
    ),
    (
        "pow",
        "Raise a base to an exponent.",
        "计算底数的指定次幂。"
    ),
    ("round", "Round to the nearest integer.", "四舍五入为整数。"),
    (
        "sin",
        "Return the sine of an angle in radians.",
        "返回弧度角的正弦。"
    ),
    (
        "sqrt",
        "Return the nonnegative square root.",
        "返回非负平方根。"
    ),
    (
        "tan",
        "Return the tangent of an angle in radians.",
        "返回弧度角的正切。"
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_calls_strings_and_unicode_cursors() {
        let src = "/* 猫 */ linear(time, [1,2], \"a,b\", wig";
        let c = context(src, src.chars().count()).unwrap();
        assert_eq!(c.call, Some(("linear", 3)));
        assert_eq!(c.prefix, "wig");
        assert_eq!(function_name(candidates(&c, false)[0]), "wiggle");
        assert!(context("a // wig", 8).is_none());
        let mut src = "猫.style.setFontSxyz(42)".to_owned();
        let word = context(&src, "猫.style.setFontS".chars().count())
            .unwrap()
            .word;
        let caret = insert(
            &mut src,
            word,
            "style.setFontSize(value, startIndex, numOfCharacters)",
        );
        assert_eq!(src, "猫.style.setFontSize(42)");
        assert_eq!(caret, "猫.style.setFontSize(".chars().count());
        for sig in signatures() {
            assert!(!summary(function_name(sig)).is_empty(), "{sig}");
        }
    }
}
