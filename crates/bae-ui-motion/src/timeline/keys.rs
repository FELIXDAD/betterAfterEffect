//! Keyframes in the layer-bar time graph: glyphs, selection, Cmd/Ctrl-click
//! interpolation toggles and drags. A drag moves the selected keyframes by whole
//! frames; Shift snaps the grabbed one to the CTI, other keyframes, In / Out points,
//! the work area and the comp's ends (AE-ANI-070); Alt-dragging the first or last
//! selected keyframe scales the selection's timing about the other end (AE-ANI-071).

use std::collections::HashSet;

use bae_model::AttrSpec;
use bae_motion::KeyEdit;

use super::*;

/// Snapping distance (points).
const SNAP_PX: f32 = 8.0;
/// Snap indicator.
pub(super) const SNAP: Color32 = Color32::from_rgb(0xff, 0x8c, 0x00);

/// AE keyframe glyph: each half shows its side's interpolation.
pub(super) fn paint_key(p: &egui::Painter, c: Pos2, k: &Keyframe, selected: bool) {
    let fill = if selected {
        color::ACCENT
    } else {
        color::KEYFRAME
    };
    let stroke = Stroke::new(1.0, Color32::from_gray(20));
    let r = 5.0;
    let half = |left: bool, interp: Interpolation, auto: bool| -> Vec<Pos2> {
        let s = if left { -1.0 } else { 1.0 };
        match interp {
            Interpolation::Hold => vec![
                c + vec2(0.0, -r),
                c + vec2(s * r * 0.8, -r),
                c + vec2(s * r * 0.8, r),
                c + vec2(0.0, r),
            ],
            Interpolation::Bezier if auto => (0..=8)
                .map(|i| {
                    let a = std::f32::consts::PI * (i as f32 / 8.0) - std::f32::consts::FRAC_PI_2;
                    c + vec2(s * a.cos() * r, a.sin() * r)
                })
                .collect(),
            Interpolation::Bezier => vec![
                c + vec2(0.0, -r),
                c + vec2(s * r * 0.75, -r),
                c + vec2(s * r * 0.2, 0.0),
                c + vec2(s * r * 0.75, r),
                c + vec2(0.0, r),
            ],
            Interpolation::Linear => {
                vec![c + vec2(0.0, -r), c + vec2(s * r, 0.0), c + vec2(0.0, r)]
            }
        }
    };
    let auto = k.flags.temporal_auto_bezier;
    for (left, interp) in [(true, k.interp_in), (false, k.interp_out)] {
        let pts = half(left, interp, auto);
        p.add(Shape::convex_polygon(pts.clone(), fill, Stroke::NONE));
        p.add(Shape::line(pts, stroke));
    }
}

/// Cmd/Ctrl-click on a keyframe (AE-ANI-049): Linear → Auto Bezier, any Bezier →
/// Linear; with Alt (`hold`): Hold ⇄ Linear.
pub(crate) fn click_conversion(k: &Keyframe, key: KeyRef, hold: bool) -> KeyEdit {
    let mut e = KeyEdit::new(key);
    let both = |i: Interpolation| k.interp_in == i && k.interp_out == i;
    let to = if hold {
        if both(Interpolation::Hold) {
            Interpolation::Linear
        } else {
            Interpolation::Hold
        }
    } else if both(Interpolation::Linear) {
        e.auto_bezier = Some(true);
        return e;
    } else {
        Interpolation::Linear
    };
    e.interp_in = Some(to);
    e.interp_out = Some(to);
    e.auto_bezier = Some(false);
    e.continuous = Some(false);
    e
}

/// Where the dragged keyframes are this frame (computed before the rows draw, so
/// the preview and the release use the same numbers).
#[derive(Clone, Debug)]
pub(super) struct KeyPlan {
    pub(super) keys: Vec<KeyRef>,
    /// Comp-time offset of the grabbed keyframe: whole frames, or onto the snap target.
    pub(super) delta: Time,
    /// Alt-drag: every keyframe goes to `anchor + (t - anchor) * factor` (comp time).
    pub(super) scale: Option<(Time, f64)>,
    /// x of the snap target (indicator line).
    pub(super) snap_x: Option<f32>,
}

impl KeyPlan {
    /// New comp time of a dragged keyframe now at comp time `ct`.
    pub(super) fn map(&self, ct: Time) -> Time {
        match self.scale {
            Some((a, f)) => a + Time(((ct - a).0 as f64 * f).round() as i64),
            None => ct + self.delta,
        }
    }

    /// Keyframe time (layer time) of a dragged keyframe after the drag.
    fn key_time(&self, stage: &Stage, k: &KeyRef) -> Time {
        let ct = query::to_comp_time(stage, &k.path, k.time);
        query::to_key_time(stage, &k.path, self.map(ct))
    }
}

impl TimelinePanel {
    /// The keyframes of one property row in layer-bar mode.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn key_track(
        &mut self,
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        axis: &TimeAxis,
        rect: Rect,
        path: &PrimPath,
        attr: &str,
        spec: &AttrSpec,
        prop_name: &str,
        key_rects: &mut Vec<(Rect, KeyRef)>,
    ) {
        let stage = cx.stage;
        let cy = rect.center().y;
        let graph = Rect::from_min_max(pos2(axis.left, rect.top()), rect.max);
        let p = ui.painter().with_clip_rect(graph);
        let selected_keys = session.lock().selected_keys.clone();
        // Keyframes of a locked layer cannot be selected or moved (AE).
        let locked = query::locked_layer_of(stage, path).is_some();
        for (t, k) in &spec.keyframes {
            let kr = KeyRef::new(path.clone(), attr, *t);
            let selected = selected_keys.contains(&kr);
            let mut ct = query::to_comp_time(stage, path, *t);
            if let Some(plan) = self.key_plan.as_ref().filter(|p| p.keys.contains(&kr)) {
                ct = plan.map(ct);
            }
            let c = pos2(axis.x(ct), cy);
            if !graph.expand(6.0).contains(c) {
                continue;
            }
            paint_key(&p, c, k, selected);
            #[cfg(test)]
            self.last_keys.push((kr.clone(), c));
            let hit = Rect::from_center_size(c, vec2(11.0, 13.0));
            key_rects.push((hit, kr.clone()));
            let sense = if locked {
                Sense::hover()
            } else {
                Sense::click_and_drag()
            };
            let resp = ui
                .interact(hit, ui.id().with(("key", path.as_str(), attr, t.0)), sense)
                .labeled_toggle(
                    egui::WidgetType::Button,
                    &bae_ui::a11y::context(&[
                        prop_name,
                        tr("Keyframe"),
                        &common::timecode(info, query::to_comp_time(stage, path, *t)),
                    ]),
                    selected,
                );
            if resp.clicked() {
                let mods = ui.input(|i| i.modifiers);
                if mods.command {
                    // Cmd/Ctrl-click converts the interpolation (AE-ANI-049).
                    let e = click_conversion(k, kr.clone(), mods.alt);
                    cx.invoke_with("key.edit", json!({ "edits": [e] }));
                } else {
                    let mut s = session.lock();
                    if mods.shift {
                        if let Some(i) = s.selected_keys.iter().position(|x| *x == kr) {
                            s.selected_keys.remove(i);
                        } else {
                            s.selected_keys.push(kr.clone());
                        }
                    } else {
                        s.selected_keys = vec![kr.clone()];
                    }
                }
            }
            if resp.double_clicked() {
                session.lock().set_time(info, ct);
            }
            // The drag starts where the button went down (egui reports it a few
            // points later), so the keyframes stay under the pointer.
            if resp.drag_started()
                && let Some(pos) = ui
                    .input(|i| i.pointer.press_origin())
                    .or(resp.interact_pointer_pos())
            {
                let alt = ui.input(|i| i.modifiers.alt);
                let mut s = session.lock();
                if !s.selected_keys.contains(&kr) {
                    s.selected_keys = vec![kr.clone()];
                }
                self.drag = Some(Drag::Keys {
                    origin: pos.x,
                    keys: s.selected_keys.clone(),
                    grabbed: kr.clone(),
                    scale: alt,
                });
            }
            crate::common::context_menu(&resp, &mut |ui| {
                if !session.lock().selected_keys.contains(&kr) {
                    session.lock().selected_keys = vec![kr.clone()];
                }
                menus::key_menu(ui, cx, session, &kr);
            });
        }
    }

    /// Where the keyframe drag puts the keyframes with the pointer at `pointer_x`
    /// (Shift held: snapping). `rows` are the shown rows: their keyframes and layers
    /// are the snap targets.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn plan_key_drag(
        &self,
        stage: &Stage,
        info: &CompInfo,
        axis: &TimeAxis,
        rows: &[Row],
        now: Time,
        pointer_x: Option<f32>,
        shift: bool,
    ) -> Option<KeyPlan> {
        let Some(Drag::Keys {
            origin,
            keys,
            grabbed,
            scale,
        }) = &self.drag
        else {
            return None;
        };
        let frame = info.frame_duration();
        let g0 = query::to_comp_time(stage, &grabbed.path, grabbed.time);
        let x = pointer_x.unwrap_or(*origin);
        let raw = Time((f64::from(x - origin) * axis.per_point()) as i64);
        let mut delta = Time((raw.0 as f64 / frame.0 as f64).round() as i64 * frame.0);
        let mut snap_x = None;
        if shift {
            let dragged: HashSet<&KeyRef> = keys.iter().collect();
            let mut targets = vec![
                now,
                Time::ZERO,
                info.duration,
                info.work_area.start,
                info.work_area.end,
            ];
            let mut layers: Vec<&PrimPath> = Vec::new();
            for row in rows {
                if !layers.contains(&&row.layer) {
                    layers.push(&row.layer);
                    let t = query::layer_timing(stage, &row.layer);
                    targets.push(t.in_point);
                    targets.push(t.out_point);
                }
                if let RowKind::Prop { path, attr, .. } = &row.kind
                    && let Some(spec) = query::attr_spec(stage, path, attr)
                {
                    for t in spec.keyframes.keys() {
                        if !dragged.contains(&KeyRef::new(path.clone(), attr.as_str(), *t)) {
                            targets.push(query::to_comp_time(stage, path, *t));
                        }
                    }
                }
            }
            let tol = Time((f64::from(SNAP_PX) * axis.per_point()) as i64);
            if let Some(t) = crate::graph::snap_to(g0 + raw, &targets, tol) {
                delta = t - g0;
                snap_x = Some(axis.x(t));
            }
        }
        let mut factor = None;
        if *scale {
            let times: Vec<Time> = keys
                .iter()
                .map(|k| query::to_comp_time(stage, &k.path, k.time))
                .collect();
            let lo = times.iter().copied().min().unwrap_or(g0);
            let hi = times.iter().copied().max().unwrap_or(g0);
            // Only the first or last keyframe scales; the other end stays.
            if hi > lo && (g0 == lo || g0 == hi) {
                let anchor = if g0 == hi { lo } else { hi };
                let mut to = g0 + delta;
                to = if g0 > anchor {
                    to.max(anchor + frame)
                } else {
                    to.min(anchor - frame)
                };
                delta = to - g0;
                factor = Some((anchor, (to - anchor).0 as f64 / (g0 - anchor).0 as f64));
            }
        }
        Some(KeyPlan {
            keys: keys.clone(),
            delta,
            scale: factor,
            snap_x,
        })
    }

    /// Release of a keyframe drag: one `key.move` (or one `key.edit` for Alt
    /// scaling), and the selection follows the keyframes.
    pub(super) fn commit_key_drag(
        &self,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        plan: &KeyPlan,
    ) {
        let stage = cx.stage;
        let Some(first) = plan.keys.first() else {
            return;
        };
        let moved: Vec<KeyRef> = if plan.scale.is_some() {
            let edits: Vec<KeyEdit> = plan
                .keys
                .iter()
                .filter_map(|k| {
                    let nt = plan.key_time(stage, k);
                    (nt != k.time).then(|| {
                        let mut e = KeyEdit::new(k.clone());
                        e.time = Some(nt);
                        e
                    })
                })
                .collect();
            if edits.is_empty() {
                return;
            }
            cx.invoke_with("key.edit", json!({ "edits": edits }));
            plan.keys
                .iter()
                .map(|k| KeyRef::new(k.path.clone(), k.attr.clone(), plan.key_time(stage, k)))
                .collect()
        } else {
            if plan.delta == Time::ZERO {
                return;
            }
            // Keyframe time offset (stretch-aware) from the first key.
            let kd = plan.key_time(stage, first) - first.time;
            if kd == Time::ZERO {
                return;
            }
            cx.invoke_with("key.move", json!({"keys": plan.keys, "delta": kd}));
            plan.keys
                .iter()
                .map(|k| KeyRef::new(k.path.clone(), k.attr.clone(), k.time + kd))
                .collect()
        };
        session.lock().selected_keys = moved;
    }
}
