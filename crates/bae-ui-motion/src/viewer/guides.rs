//! The viewer's rulers, guides, grids and safe areas (AE's View menu and the
//! Choose Grid and Guide Options popup): Show Rulers (Cmd/Ctrl+R), Show / Snap
//! to / Lock / Clear Guides (Cmd/Ctrl+; and variants), Show Grid / Snap to Grid
//! (Cmd/Ctrl+'), Proportional Grid (Alt+'), Title/Action Safe (').
//!
//! Guides belong to the comp (saved with it, `guides`): drag one out of a
//! ruler, drag it to move it, drag it back onto a ruler to remove it; each is
//! one undo step. The toggles are view options, shared by the viewers.

use bae_model::{PrimPath, Stage, Value};
use bae_motion::query::{self, CompInfo};
use bae_ui::theme::{self, color};
use bae_ui::{Invocation, PanelContext};
use egui::{Align2, Color32, CursorIcon, Painter, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use crate::common;
use crate::render::View;

/// Ruler thickness (points).
pub(super) const RULER: f32 = 16.0;
/// How close to a guide a press grabs it (points).
const GRAB: f32 = 4.0;

/// The View menu's toggles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ViewOptions {
    pub rulers: bool,
    pub guides: bool,
    pub snap_guides: bool,
    pub lock_guides: bool,
    pub grid: bool,
    pub snap_grid: bool,
    pub proportional: bool,
    pub safe: bool,
}

impl Default for ViewOptions {
    fn default() -> Self {
        ViewOptions {
            rulers: false,
            guides: true,
            snap_guides: false,
            lock_guides: false,
            grid: false,
            snap_grid: false,
            proportional: false,
            safe: false,
        }
    }
}

/// `(action id, English title)`; catalog ids, handled here.
pub(crate) const ACTIONS: &[(&str, &str)] = &[
    ("motion.view.show_rulers", "Show Rulers"),
    ("motion.view.show_guides", "Show Guides"),
    ("motion.view.snap_to_guides", "Snap to Guides"),
    ("motion.view.lock_guides", "Lock Guides"),
    ("motion.view.clear_guides", "Clear Guides"),
    ("motion.view.show_grid", "Show Grid"),
    ("motion.view.snap_to_grid", "Snap to Grid"),
    ("motion.view.proportional_grid", "Proportional Grid"),
    ("motion.view.title_action_safe", "Title/Action Safe"),
];

/// Toggles the option of `id`; `None` for an action that is not a toggle.
fn option<'a>(o: &'a mut ViewOptions, id: &str) -> Option<&'a mut bool> {
    Some(match id {
        "motion.view.show_rulers" => &mut o.rulers,
        "motion.view.show_guides" => &mut o.guides,
        "motion.view.snap_to_guides" => &mut o.snap_guides,
        "motion.view.lock_guides" => &mut o.lock_guides,
        "motion.view.show_grid" => &mut o.grid,
        "motion.view.snap_to_grid" => &mut o.snap_grid,
        "motion.view.proportional_grid" => &mut o.proportional,
        "motion.view.title_action_safe" => &mut o.safe,
        _ => return None,
    })
}

/// Whether the toggle of `id` is on (menu check marks).
pub(crate) fn checked(resources: &bae_app::Resources, id: &str) -> bool {
    let session = common::session(resources);
    let mut o = session.lock().view_options;
    option(&mut o, id).is_some_and(|v| *v)
}

pub(crate) fn handle(inv: &Invocation, cx: &mut PanelContext<'_>) -> bool {
    if !ACTIONS.iter().any(|(id, _)| *id == inv.id) {
        return false;
    }
    let session = common::session(cx.resources);
    if inv.id == "motion.view.clear_guides" {
        if let Some(info) = common::active_comp(cx.stage, &session) {
            set_guides(cx, &info.path, &[]);
        }
        return true;
    }
    if let Some(v) = option(&mut session.lock().view_options, &inv.id) {
        *v = !*v;
    }
    true
}

/// A guide: horizontal (a y) or vertical (an x), comp pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Guide {
    pub vertical: bool,
    pub at: f64,
}

pub(crate) fn guides_of(stage: &Stage, comp: &PrimPath) -> Vec<Guide> {
    match stage
        .prim(comp)
        .and_then(|v| query::static_value(&v, "guides"))
    {
        Some(Value::FloatList(f)) => f
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| Guide {
                vertical: c[0] != 0.0,
                at: c[1],
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn set_guides(cx: &mut PanelContext<'_>, comp: &PrimPath, guides: &[Guide]) {
    let list: Vec<f64> = guides
        .iter()
        .flat_map(|g| [f64::from(u8::from(g.vertical)), g.at])
        .collect();
    cx.invoke_with(
        "attr.set",
        json!({"path": comp, "attr": "guides", "value": {"type": "float_list", "value": list}}),
    );
}

/// Lines dragged layers snap to: `(vertical, at)` in comp pixels.
pub(crate) fn snap_lines(
    stage: &Stage,
    info: &CompInfo,
    o: &ViewOptions,
    grid: &bae_ui::settings::GridGuides,
) -> Vec<(bool, f64)> {
    let mut out = Vec::new();
    if o.snap_guides {
        out.extend(
            guides_of(stage, &info.path)
                .iter()
                .map(|g| (g.vertical, g.at)),
        );
    }
    if o.snap_grid && grid.spacing > 0.0 {
        let step = f64::from(grid.spacing);
        for (vertical, len) in [
            (true, f64::from(info.width)),
            (false, f64::from(info.height)),
        ] {
            let mut at = 0.0;
            while at <= len {
                out.push((vertical, at));
                at += step;
            }
        }
    }
    out
}

/// A guide being dragged: which (`None`: a new one from a ruler) and where.
pub(super) struct GuideDrag {
    index: Option<usize>,
    vertical: bool,
    at: f64,
}

/// The rulers' strips in `full` (top, left) and the viewer area left.
pub(super) fn layout(full: Rect, rulers: bool) -> (Option<(Rect, Rect)>, Rect) {
    if !rulers {
        return (None, full);
    }
    let top = Rect::from_min_max(
        pos2(full.left() + RULER, full.top()),
        pos2(full.right(), full.top() + RULER),
    );
    let left = Rect::from_min_max(
        pos2(full.left(), full.top() + RULER),
        pos2(full.left() + RULER, full.bottom()),
    );
    let area = Rect::from_min_max(full.min + vec2(RULER, RULER), full.max);
    (Some((top, left)), area)
}

/// Ruler steps: AE-like 1 / 2 / 5 × 10ⁿ comp pixels, about 80 points apart.
fn step(scale: f32) -> f64 {
    let want = 80.0 / f64::from(scale.max(1e-6));
    let p = 10f64.powf(want.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * p)
        .find(|s| *s >= want)
        .unwrap_or(10.0 * p)
}

fn paint_ruler(painter: &Painter, strip: Rect, view: &View, horizontal: bool) {
    painter.rect_filled(strip, 0.0, color::PANEL);
    let s = step(view.scale);
    let (lo, hi) = if horizontal {
        (
            view.to_comp(strip.left_top()).x,
            view.to_comp(strip.right_top()).x,
        )
    } else {
        (
            view.to_comp(strip.left_top()).y,
            view.to_comp(strip.left_bottom()).y,
        )
    };
    let tick = Stroke::new(1.0, color::TEXT_DIM);
    let minor = s / 10.0;
    let mut v = (lo / minor).floor() * minor;
    while v <= hi {
        let major = (v / s).round() * s == v || ((v / s) - (v / s).round()).abs() < 1e-6;
        let len = if major { RULER } else { RULER * 0.3 };
        if horizontal {
            let x = view.to_screen(bae_anim::glam::DVec2::new(v, 0.0)).x;
            painter.line_segment(
                [pos2(x, strip.bottom() - len), pos2(x, strip.bottom())],
                tick,
            );
            if major {
                painter.text(
                    pos2(x + 2.0, strip.top()),
                    Align2::LEFT_TOP,
                    format!("{v:.0}"),
                    theme::small(),
                    color::TEXT_DIM,
                );
            }
        } else {
            let y = view.to_screen(bae_anim::glam::DVec2::new(0.0, v)).y;
            painter.line_segment([pos2(strip.right() - len, y), pos2(strip.right(), y)], tick);
            if major {
                painter.text(
                    pos2(strip.left() + 1.0, y + 2.0),
                    Align2::LEFT_TOP,
                    format!("{v:.0}"),
                    theme::small(),
                    color::TEXT_DIM,
                );
            }
        }
        v += minor;
    }
}

/// Rulers, guides and guide drags. Returns true while a guide is dragged (the
/// tools leave the pointer alone).
#[allow(clippy::too_many_arguments)]
pub(super) fn frame(
    drag: &mut Option<GuideDrag>,
    ui: &Ui,
    cx: &mut PanelContext<'_>,
    info: &CompInfo,
    view: &View,
    area: Rect,
    rulers: Option<(Rect, Rect)>,
    o: &ViewOptions,
    grab: bool,
) -> bool {
    let painter = ui.painter();
    let guides = guides_of(cx.stage, &info.path);
    let style = bae_ui::settings::GridGuides::current(cx.resources);
    if let Some((top, left)) = rulers {
        paint_ruler(painter, top, view, true);
        paint_ruler(painter, left, view, false);
        painter.rect_filled(
            Rect::from_min_size(left.left_top() - vec2(0.0, RULER), vec2(RULER, RULER)),
            0.0,
            color::PANEL,
        );
        // Drag a new guide out of a ruler.
        for (strip, vertical) in [(top, false), (left, true)] {
            let resp = ui.interact(strip, ui.id().with(("ruler", vertical)), Sense::drag());
            if resp.drag_started() && !o.lock_guides {
                *drag = Some(GuideDrag {
                    index: None,
                    vertical,
                    at: 0.0,
                });
            }
        }
    }
    let pointer = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));
    // Grab an existing guide (Selection tool, not locked).
    let hit = |pos: Pos2| {
        guides.iter().position(|g| {
            let s = view.to_screen(bae_anim::glam::DVec2::new(g.at, g.at));
            if g.vertical {
                (s.x - pos.x).abs() <= GRAB
            } else {
                (s.y - pos.y).abs() <= GRAB
            }
        })
    };
    if drag.is_none()
        && grab
        && o.guides
        && !o.lock_guides
        && let Some(pos) = pointer.filter(|p| area.contains(*p))
        && let Some(i) = hit(pos)
    {
        ui.ctx().set_cursor_icon(if guides[i].vertical {
            CursorIcon::ResizeHorizontal
        } else {
            CursorIcon::ResizeVertical
        });
        if ui.input(|i| i.pointer.primary_pressed()) {
            *drag = Some(GuideDrag {
                index: Some(i),
                vertical: guides[i].vertical,
                at: guides[i].at,
            });
        }
    }
    if let Some(d) = drag.as_mut()
        && let Some(pos) = pointer
    {
        let c = view.to_comp(pos);
        d.at = if d.vertical { c.x } else { c.y }.round();
    }
    // Guides, and the one being dragged.
    if o.guides || drag.is_some() {
        let stroke = Stroke::new(1.0, style.guide_color);
        let clip = painter.with_clip_rect(area);
        let shown = guides
            .iter()
            .enumerate()
            .filter(|(i, _)| drag.as_ref().is_none_or(|d| d.index != Some(*i)));
        let line = |g: &Guide| {
            let s = view.to_screen(bae_anim::glam::DVec2::new(g.at, g.at));
            if g.vertical {
                [pos2(s.x, area.top()), pos2(s.x, area.bottom())]
            } else {
                [pos2(area.left(), s.y), pos2(area.right(), s.y)]
            }
        };
        for (_, g) in shown {
            clip.line_segment(line(g), stroke);
        }
        if let Some(d) = drag.as_ref() {
            clip.line_segment(
                line(&Guide {
                    vertical: d.vertical,
                    at: d.at,
                }),
                stroke,
            );
        }
    }
    let Some(d) = drag.as_ref() else {
        return false;
    };
    if ui.input(|i| i.pointer.primary_down()) {
        return true;
    }
    // Released: dropped on the viewer keeps it, on a ruler (or outside) removes it.
    let inside = pointer.is_some_and(|p| area.contains(p));
    let mut list = guides.clone();
    match (d.index, inside) {
        (Some(i), true) => list[i].at = d.at,
        (Some(i), false) => {
            list.remove(i);
        }
        (None, true) => list.push(Guide {
            vertical: d.vertical,
            at: d.at,
        }),
        (None, false) => {}
    }
    if list != guides {
        set_guides(cx, &info.path, &list);
    }
    *drag = None;
    true
}

/// The proportional grid (Preferences > Grids & Guides: columns × rows).
pub(super) fn paint_proportional(painter: &Painter, comp: Rect, cells: [u32; 2], color: Color32) {
    let stroke = Stroke::new(1.0, color);
    let [h, v] = cells.map(|n| n.max(1));
    for i in 1..h {
        let x = comp.left() + comp.width() * i as f32 / h as f32;
        painter.line_segment([pos2(x, comp.top()), pos2(x, comp.bottom())], stroke);
    }
    for j in 1..v {
        let y = comp.top() + comp.height() * j as f32 / v as f32;
        painter.line_segment([pos2(comp.left(), y), pos2(comp.right(), y)], stroke);
    }
}
