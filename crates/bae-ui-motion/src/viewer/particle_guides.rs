//! CC Particle World guides in the composition viewer.
use bae_anim::glam::{DMat3, DMat4, DVec2, DVec3, DVec4};
use bae_anim::{Curve, CurveKind, EvaluatedComp, EvaluatedEffect, EvaluatedLayer, Evaluator};
use bae_model::{PrimPath, Stage, Value};
use bae_render::{anim::affine_2d, camera::Camera, effect::EffectScene};
use egui::{Color32, Painter, Pos2, Stroke, vec2};

use crate::render::{self, View};

const GUIDE: Stroke = Stroke {
    width: 1.,
    color: Color32::from_rgb(89, 170, 228),
};
const GRID: Stroke = Stroke {
    width: 1.,
    color: Color32::from_rgb(72, 111, 156),
};
const AXES: [DVec3; 3] = [DVec3::X, DVec3::Y, DVec3::Z];

struct Timeline<'a> {
    stage: &'a Stage,
    eval: &'a mut Evaluator,
    comp: &'a EvaluatedComp,
}

pub(super) fn paint(
    painter: &Painter,
    stage: &Stage,
    eval: &mut Evaluator,
    comp: &EvaluatedComp,
    view: &View,
    selected: &[PrimPath],
    view_camera: Option<Camera>,
) {
    let mut timeline = Timeline { stage, eval, comp };
    for path in selected {
        let Some(layer) = comp.layer(path) else {
            continue;
        };
        if !layer.visible || !layer.switches.effects {
            continue;
        }
        for fx in layer
            .effects
            .iter()
            .filter(|fx| fx.enabled && fx.match_name == "CC Particle World")
        {
            paint_effect(painter, &mut timeline, layer, fx, view, view_camera);
        }
    }
}

struct Guides<'a> {
    painter: &'a Painter,
    view: &'a View,
    // Maps normalized, centered particle coordinates to homogeneous comp pixels.
    projection: DMat4,
    layer_to_comp: DMat3,
}

impl Guides<'_> {
    fn project(&self, p: DVec4) -> DVec3 {
        let h = self.projection * p;
        self.layer_to_comp * DVec3::new(h.x, h.y, h.w)
    }

    fn screen(&self, h: DVec3) -> Pos2 {
        self.view.to_screen(h.truncate() / h.z)
    }

    fn segment(&self, a: DVec3, b: DVec3, stroke: Stroke) {
        let mut a = self.project(a.extend(1.));
        let mut b = self.project(b.extend(1.));
        const NEAR: f64 = 0.001;
        if a.z < NEAR && b.z < NEAR {
            return;
        }
        if a.z < NEAR {
            a = a.lerp(b, (NEAR - a.z) / (b.z - a.z));
        }
        if b.z < NEAR {
            b = b.lerp(a, (NEAR - b.z) / (a.z - b.z));
        }
        self.painter
            .line_segment([self.screen(a), self.screen(b)], stroke);
    }

    // The vanishing line of the world XZ plane, intersected with the viewer.
    fn horizon(&self) {
        let h = self.project(DVec4::X).cross(self.project(DVec4::Z));
        let rect = self.painter.clip_rect();
        let lo = self.view.to_comp(rect.min);
        let hi = self.view.to_comp(rect.max);
        let (a, b) = if h.y.abs() >= h.x.abs() && h.y != 0. {
            let y = |x: f64| -(h.x * x + h.z) / h.y;
            (DVec2::new(lo.x, y(lo.x)), DVec2::new(hi.x, y(hi.x)))
        } else if h.x != 0. {
            let x = |y: f64| -(h.y * y + h.z) / h.x;
            (DVec2::new(x(lo.y), lo.y), DVec2::new(x(hi.y), hi.y))
        } else {
            return;
        };
        self.painter
            .line_segment([self.view.to_screen(a), self.view.to_screen(b)], GUIDE);
    }

    fn axis_box(&self) {
        let center = self.painter.clip_rect().left_top() + vec2(32., 32.);
        let origin = self.project(DVec4::W);
        // Project directions at the origin without translating the reference box.
        let axes = AXES.map(|a| {
            let v = self.project(a.extend(0.));
            v.truncate() * origin.z - origin.truncate() * v.z
        });
        let scale = axes.iter().map(|a| a.length()).fold(0., f64::max);
        if scale == 0. {
            return;
        }
        let axes = axes.map(|a| vec2(a.x as f32, a.y as f32) * (13. / scale) as f32);
        let corner = center - axes[0] - axes[1] - axes[2];
        let colors = [
            Color32::LIGHT_RED,
            Color32::LIGHT_GREEN,
            Color32::LIGHT_BLUE,
        ];
        for i in 0..3 {
            for u in [0., 2.] {
                for v in [0., 2.] {
                    let from = corner + axes[(i + 1) % 3] * u + axes[(i + 2) % 3] * v;
                    let stroke = if u == 0. && v == 0. {
                        Stroke::new(1.5, colors[i])
                    } else {
                        GRID
                    };
                    self.painter
                        .line_segment([from, from + axes[i] * 2.], stroke);
                }
            }
        }
    }
}

fn paint_effect(
    painter: &Painter,
    timeline: &mut Timeline<'_>,
    layer: &EvaluatedLayer,
    fx: &EvaluatedEffect,
    view: &View,
    view_camera: Option<Camera>,
) {
    let def = bae_effects::get("CC Particle World").unwrap().def();
    let param = |id| match fx.param(id).or_else(|| def.param(id)?.default.as_ref()) {
        Some(Value::Bool(b)) => f64::from(*b),
        Some(v) => v.as_f64().unwrap_or_default(),
        None => 0.,
    };
    let comp = timeline.comp;
    let comp_size = DVec2::new(f64::from(comp.width), f64::from(comp.height));
    let (origin, end) = render::layer_bounds(layer).unwrap_or((DVec2::ZERO, comp_size));
    let size = end - origin;
    if size.min_element() <= 0. {
        return;
    }
    let camera = view_camera.or_else(|| {
        let c = comp.layers.get(comp.camera?)?.camera?;
        Some(Camera::new(c.world_matrix, c.zoom))
    });
    let placement = affine_2d(&layer.world_matrix);
    if placement.matrix2.determinant() == 0. {
        return;
    }
    let scene = EffectScene {
        camera,
        light: None,
        size: [comp.width, comp.height],
        placement,
    };
    let projection = bae_effects::particle_world_projection(size, origin, Some(&scene), param)
        * DMat4::from_translation((size * 0.5).extend(0.))
        * DMat4::from_scale(DVec3::splat(size.x));
    let layer_to_comp = match view.camera {
        Some((camera, viewport)) if layer.switches.three_d => {
            camera.plane_homography(&layer.world_matrix, viewport)
        }
        _ => DMat3::from(placement),
    } * DMat3::from_translation(origin);
    let guides = Guides {
        painter,
        view,
        projection,
        layer_to_comp,
    };
    let producer = DVec3::new(param(7), param(8), param(9));
    let radius = DVec3::new(param(10), param(11), param(12));
    let (grid_origin, normal) = match param(56) as u32 {
        1 => (DVec3::Y * param(3), 1),
        position => (
            if position == 2 { producer } else { DVec3::ZERO },
            (param(57) as usize).clamp(1, 3) - 1,
        ),
    };
    if param(55) != 0. {
        let extent = param(59) * 0.5;
        let divisions = (param(58) as usize).clamp(1, 200);
        let u = AXES[(normal + 1) % 3] * extent;
        let v = AXES[(normal + 2) % 3] * extent;
        for i in 0..=divisions {
            let d = 2. * i as f64 / divisions as f64 - 1.;
            guides.segment(grid_origin + u * d - v, grid_origin + u * d + v, GRID);
            guides.segment(grid_origin + v * d - u, grid_origin + v * d + u, GRID);
        }
    }
    if param(60) != 0. {
        guides.horizon();
    }
    if param(61) != 0. {
        guides.axis_box();
    }
    if param(51) != 0. {
        let mut foot = producer;
        foot[normal] = grid_origin[normal];
        guides.segment(producer, foot, GUIDE);
        let h = guides.project(producer.extend(1.));
        if h.z > 0.001 {
            let p = guides.screen(h);
            for d in [vec2(4., 4.), vec2(-4., 4.)] {
                painter.line_segment([p - d, p + d], GUIDE);
            }
        }
    }
    if param(52) != 0. {
        for axis in 0..3 {
            let u = radius * AXES[axis];
            let v = radius * AXES[(axis + 1) % 3];
            let mut prev = producer + u;
            for i in 1..=32 {
                let (s, c) = (std::f64::consts::TAU * f64::from(i) / 32.).sin_cos();
                let next = producer + u * c + v * s;
                guides.segment(prev, next, GUIDE);
                prev = next;
            }
        }
    }
    if param(53) != 0. {
        let radius_frames = (param(54) as i64).clamp(0, 1000);
        motion_path(&guides, timeline, layer, fx, producer, radius_frames);
    }
}

fn motion_path(
    guides: &Guides<'_>,
    timeline: &mut Timeline<'_>,
    layer: &EvaluatedLayer,
    fx: &EvaluatedEffect,
    producer: DVec3,
    radius_frames: i64,
) {
    let comp = timeline.comp;
    let stage = timeline.stage;
    let Some(prim) = stage.prim(&fx.path) else {
        return;
    };
    let attrs = ["param:0007", "param:0008", "param:0009"];
    let specs = attrs.map(|name| prim.attr(name));
    if !specs
        .iter()
        .flatten()
        .any(|s| s.is_animated() || s.expression.as_ref().is_some_and(|e| e.enabled))
    {
        return;
    }
    let curves = specs.map(|s| s.and_then(|s| Curve::new(&s.keyframes, CurveKind::PerDimension)));
    let mut prev = None;
    for offset in -radius_frames..=radius_frames {
        let time = comp.time + comp.frame_rate.frame_to_time(offset);
        if time < bae_model::Time::ZERO || time >= comp.duration || !layer.timing.is_active(time) {
            continue;
        }
        let mut pos = producer;
        for i in 0..3 {
            let v = if specs[i]
                .and_then(|s| s.expression.as_ref())
                .is_some_and(|e| e.enabled)
            {
                timeline
                    .eval
                    .evaluate_attribute(stage, &fx.path, attrs[i], time)
                    .ok()
            } else {
                curves[i]
                    .as_ref()
                    .map(|c| c.sample(layer.timing.layer_time(time)))
            };
            if let Some(v) = v.and_then(|v| v.as_f64()) {
                pos[i] = v;
            }
        }
        if let Some(p) = prev
            && p != pos
        {
            guides.segment(p, pos, GUIDE);
        }
        prev = Some(pos);
    }
}
