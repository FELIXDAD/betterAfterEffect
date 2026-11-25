//! Camera and light layers drawn in the 3D views (AE draws them as
//! wireframes in Top, Custom View …): a camera's frustum to the corners of its
//! image at the zoom distance and the line to its point of interest; a light's
//! position, a spot light's cone, a parallel light's direction. Selected ones
//! in their label color, the others grey. Looking through a camera, that
//! camera is not drawn.

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec3};
use bae_model::{LayerKind, PrimPath, Stage};
use bae_motion::query;
use egui::{Color32, Painter, Stroke};

use crate::render::View;

fn label(stage: &Stage, l: &PrimPath) -> Color32 {
    let i = stage
        .prim(l)
        .and_then(|v| query::static_value(&v, "label"))
        .and_then(|v| v.as_f64())
        .map_or(8, |v| v as usize);
    bae_ui::theme::label_color(i)
}

fn line(painter: &Painter, view: &View, a: DVec3, b: DVec3, stroke: Stroke) {
    if let (Some(p), Some(q)) = (view.project(a, true), view.project(b, true)) {
        painter.line_segment([p, q], stroke);
    }
}

/// `looking_through`: the camera layer the view shows the comp through, if any.
pub(super) fn paint(
    painter: &Painter,
    stage: &Stage,
    comp: &EvaluatedComp,
    view: &View,
    selected: &[PrimPath],
    looking_through: Option<&PrimPath>,
) {
    let (w, h) = (f64::from(comp.width) / 2.0, f64::from(comp.height) / 2.0);
    for l in &comp.layers {
        if !matches!(l.kind, LayerKind::Camera | LayerKind::Light) || !l.visible {
            continue;
        }
        if looking_through == Some(&l.path) {
            continue;
        }
        let color = if selected.contains(&l.path) {
            label(stage, &l.path)
        } else {
            Color32::from_gray(160)
        };
        let stroke = Stroke::new(1.0, color);
        let m: DMat4 = l.camera.as_ref().map_or(l.world_matrix, |c| c.world_matrix);
        let eye = m.w_axis.truncate();
        if let Some(c) = &l.camera {
            let z = c.zoom;
            let corners = [(-w, -h), (w, -h), (w, h), (-w, h)]
                .map(|(x, y)| m.transform_point3(DVec3::new(x, y, z)));
            for (i, k) in corners.iter().enumerate() {
                line(painter, view, eye, *k, stroke);
                line(painter, view, *k, corners[(i + 1) % 4], stroke);
            }
            if c.two_node {
                let parent = l
                    .parent
                    .map_or(DMat4::IDENTITY, |p| comp.layers[p].world_matrix);
                line(
                    painter,
                    view,
                    eye,
                    parent.transform_point3(c.point_of_interest),
                    stroke,
                );
            }
        }
        if let Some(light) = &l.light {
            use bae_anim::LightType;
            let dir = m.z_axis.truncate().normalize_or(DVec3::Z);
            let reach = 200.0;
            match light.light_type {
                LightType::Spot => {
                    let (u, v) = (
                        m.x_axis.truncate().normalize_or(DVec3::X),
                        m.y_axis.truncate().normalize_or(DVec3::Y),
                    );
                    let r = (light.cone_angle.to_radians() / 2.0).tan() * reach;
                    for (a, b) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                        line(
                            painter,
                            view,
                            eye,
                            eye + dir * reach + (u * a + v * b) * r,
                            stroke,
                        );
                    }
                }
                LightType::Parallel => line(painter, view, eye, eye + dir * reach, stroke),
                _ => {}
            }
            if let Some(p) = view.project(eye, true) {
                painter.circle_stroke(p, 6.0, stroke);
                for d in [egui::vec2(9.0, 0.0), egui::vec2(0.0, 9.0)] {
                    painter.line_segment([p - d, p + d], stroke);
                }
            }
        } else if let Some(p) = view.project(eye, true) {
            painter.circle_filled(p, 2.5, color);
        }
    }
}
