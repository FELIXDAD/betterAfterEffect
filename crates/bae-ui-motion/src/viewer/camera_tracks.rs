use super::{Tool, View};
use crate::session::SharedSession;
use bae_anim::{EvaluatedComp, glam::DVec3};
use bae_model::effect::camera_tracks::{DISPLAY_POINTS, TrackPoint};
use bae_model::{PrimPath, Time, Value};
use bae_motion::camera_track::MATCH_NAME;
use bae_ui::{PanelContext, tr};
use egui::{Color32, Painter, Pos2, Rect, Response, Stroke, Ui, vec2};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Selection {
    effect: Option<PrimPath>,
    ids: BTreeSet<u32>,
    start: Option<Pos2>,
    /// The selected points shown at `time` (what Delete removes).
    visible: BTreeSet<u32>,
    time: Time,
}
const ACTIONS: [(&str, &str); 6] = [
    ("Set Ground Plane and Origin", "ground"),
    ("Create Text and Camera", "text"),
    ("Create Solid and Camera", "solid"),
    ("Create Null and Camera", "null"),
    ("Create Shadow Catcher, Camera and Light", "shadow"),
    ("Delete Selected Points", "delete"),
];
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Set Ground Plane and Origin", "设置地平面和原点"),
    ("Create Text and Camera", "创建文本和摄像机"),
    ("Create Solid and Camera", "创建实底和摄像机"),
    ("Create Null and Camera", "创建空白和摄像机"),
    (
        "Create Shadow Catcher, Camera and Light",
        "创建阴影捕手、摄像机和光",
    ),
    ("Create Text", "创建文本"),
    ("Create Solid", "创建实底"),
    ("Create Null", "创建空白"),
    ("Create Shadow Catcher", "创建阴影捕手"),
    ("Delete Selected Points", "删除所选点"),
];
struct Mark {
    id: u32,
    pos: Pos2,
    solved: bool,
}
impl Selection {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn frame(
        &mut self,
        ui: &Ui,
        resp: &Response,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        comp: &EvaluatedComp,
        view: &View,
        time: Time,
        painter: &Painter,
        tool: Tool,
    ) -> bool {
        let (selected, layers) = {
            let s = session.lock();
            (s.selected_effects.clone(), s.selected_layers.clone())
        };
        // Only a selected layer's selected tracker owns the Selection tool.
        let Some((layer, effect)) = comp
            .layers
            .iter()
            .filter(|layer| layers.contains(&layer.path))
            .find_map(|layer| {
                layer
                    .effects
                    .iter()
                    .find(|e| e.enabled && e.match_name == MATCH_NAME && selected.contains(&e.path))
                    .map(|e| (layer, e))
            })
        else {
            *self = Self::default();
            return false;
        };
        if self.effect.as_ref() != Some(&effect.path) {
            *self = Self {
                effect: Some(effect.path.clone()),
                ..Default::default()
            };
        }
        let Some(Value::Bytes(rows)) = effect.param(DISPLAY_POINTS) else {
            return false;
        };
        let value = |id, default| effect.param(id).and_then(Value::as_f64).unwrap_or(default);
        let source = value(405, 2.) == 1.;
        let radius = (value(407, 100.) as f32 * 0.035).max(0.);
        let mut marks = Vec::new();
        for point in TrackPoint::read(rows) {
            let Some([x, y]) = point.position(source) else {
                continue;
            };
            let q = layer
                .world_matrix
                .transform_point3(DVec3::new(x as f64, y as f64, 0.));
            let pos = view.to_screen(q.truncate());
            let id = point.id;
            let error = point.error;
            let e = (error / 4.).clamp(0., 1.);
            let r = radius * (1. + 0.5 * e);
            let selected = self.ids.contains(&id);
            let tint = if selected {
                Color32::YELLOW
            } else {
                Color32::from_rgb((32. + 220. * e) as u8, (230. - 190. * e) as u8, 64)
            };
            if r > 0. {
                let stroke = Stroke::new(if selected { 2. } else { 1.5 }, tint);
                painter.line_segment([pos - vec2(r, r), pos + vec2(r, r)], stroke);
                painter.line_segment([pos + vec2(-r, r), pos + vec2(r, -r)], stroke);
            }
            marks.push(Mark {
                id,
                pos,
                solved: error >= 0.,
            });
        }
        if marks.is_empty() || tool != Tool::Selection {
            self.visible.clear();
            return false;
        }
        let nearest = |p: Pos2| {
            marks
                .iter()
                .filter(|m| m.pos.distance(p) <= radius.max(5.) + 3.)
                .min_by(|a, b| a.pos.distance_sq(p).total_cmp(&b.pos.distance_sq(p)))
        };
        let modifiers = ui.input(|i| i.modifiers);
        if resp.drag_started_by(egui::PointerButton::Primary) {
            self.start = ui.input(|i| i.pointer.press_origin());
        }
        if let Some(start) = self.start
            && let Some(end) = resp.interact_pointer_pos()
        {
            let rect = Rect::from_two_pos(start, end);
            painter.rect_stroke(
                rect,
                0.,
                Stroke::new(1., Color32::WHITE),
                egui::StrokeKind::Inside,
            );
            if resp.drag_stopped() {
                if !modifiers.shift && !modifiers.alt {
                    self.ids.clear();
                }
                for mark in &marks {
                    if rect.contains(mark.pos) {
                        if modifiers.alt {
                            self.ids.remove(&mark.id);
                        } else {
                            self.ids.insert(mark.id);
                        }
                    }
                }
                self.start = None;
            }
        }
        if resp.clicked_by(egui::PointerButton::Primary)
            && let Some(pos) = resp.interact_pointer_pos()
        {
            if !modifiers.shift && !modifiers.alt {
                self.ids.clear();
            }
            if let Some(mark) = nearest(pos) {
                if modifiers.alt {
                    self.ids.remove(&mark.id);
                } else {
                    self.ids.insert(mark.id);
                }
            }
        }
        if resp.secondary_clicked()
            && let Some(pos) = resp.interact_pointer_pos()
            && let Some(mark) = nearest(pos)
            && !self.ids.contains(&mark.id)
        {
            self.ids = BTreeSet::from([mark.id]);
        }
        let visible: BTreeSet<_> = marks
            .iter()
            .filter(|m| self.ids.contains(&m.id))
            .map(|m| m.id)
            .collect();
        let solved: Vec<_> = marks
            .iter()
            .filter(|m| m.solved && visible.contains(&m.id))
            .collect();
        if !solved.is_empty() {
            let center = Pos2::ZERO
                + solved
                    .iter()
                    .fold(egui::Vec2::ZERO, |v, m| v + m.pos.to_vec2())
                    / solved.len() as f32;
            let target = value(409, 100.) as f32 * 0.21;
            for r in [target, target * 0.6, target * 0.18] {
                painter.circle_stroke(center, r, Stroke::new(1.6, Color32::LIGHT_RED));
            }
        }
        let mut action = None;
        if !visible.is_empty() {
            let existing = cx
                .stage
                .prim(&effect.path)
                .and_then(|v| {
                    v.value("tracker:camera")
                        .and_then(Value::as_prim_ref)
                        .cloned()
                })
                .is_some_and(|r| !r.is_root() && cx.stage.has_prim(&r));
            resp.context_menu(|ui| {
                for (label, kind) in ACTIONS {
                    let label = if existing {
                        match kind {
                            "text" => "Create Text",
                            "solid" => "Create Solid",
                            "null" => "Create Null",
                            "shadow" => "Create Shadow Catcher",
                            _ => label,
                        }
                    } else {
                        label
                    };
                    if ui
                        .add_enabled(
                            kind == "delete" || !solved.is_empty(),
                            egui::Button::new(tr(label)),
                        )
                        .clicked()
                    {
                        action = Some(kind);
                        ui.close();
                    }
                }
            });
        }
        self.time = time;
        self.visible = visible;
        if let Some(action) = action {
            if action == "delete" {
                self.delete_selected(cx);
            } else {
                cx.invoke_with("camera_tracker.attach",serde_json::json!({"effect":effect.path,"time":time,"points":self.visible,"action":action}));
            }
        }
        true
    }

    /// Selected track points are shown: Delete removes them, not the layer.
    pub(super) fn has_selection(&self) -> bool {
        self.effect.is_some() && !self.visible.is_empty()
    }

    /// Deletes the selected points and solves the camera again without them.
    pub(super) fn delete_selected(&mut self, cx: &mut PanelContext<'_>) -> bool {
        let Some(effect) = self.effect.clone() else {
            return false;
        };
        if self.visible.is_empty() {
            return false;
        }
        let points = std::mem::take(&mut self.visible);
        cx.invoke_with(
            "camera_tracker.attach",
            serde_json::json!({"effect":effect,"time":self.time,"points":points,"action":"delete"}),
        );
        crate::tracker::camera::request_solve(cx.resources, &effect, cx.stage.revision());
        self.ids.clear();
        true
    }
}
