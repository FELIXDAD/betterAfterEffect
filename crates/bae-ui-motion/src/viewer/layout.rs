//! View layouts (View ▸ Switch View Layout, the viewer's Select View Layout
//! popup): 1, 2 or 4 views of the comp, each with its own 3D view. The active
//! view is the full viewer (tools, overlays); the others show their 3D view
//! and the selected layers' boxes, and a click makes one active.

use bae_anim::EvaluatedComp;
use bae_anim::glam::DVec2;
use bae_motion::query::CompInfo;
use bae_ui::theme::{self, color};
use bae_ui::{PanelContext, tr};
use egui::{Align2, Color32, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};

use super::view3d::{self, View3d};
use super::{CompositionViewer, GpuPreview};
use crate::render::{self, View};
use crate::session::SharedSession;

/// The views' rectangles in `area`: one; two side by side; four in a grid.
pub(super) fn rects(area: Rect, n: usize) -> Vec<Rect> {
    let gap = 2.0;
    let (cols, rows) = match n {
        2 => (2, 1),
        4 => (2, 2),
        _ => return vec![area],
    };
    let w = (area.width() - gap * (cols - 1) as f32) / cols as f32;
    let h = (area.height() - gap * (rows - 1) as f32) / rows as f32;
    (0..rows)
        .flat_map(|r| {
            (0..cols).map(move |c| {
                let min = area.min + vec2(c as f32 * (w + gap), r as f32 * (h + gap));
                Rect::from_min_size(min, vec2(w, h))
            })
        })
        .collect()
}

impl CompositionViewer {
    /// The views other than the active one, and the active view's corners.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn other_views(
        &mut self,
        ui: &Ui,
        cx: &PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        comp: &EvaluatedComp,
        rects: &[Rect],
        panes: &[View3d],
        active: usize,
    ) {
        if rects.len() < 2 {
            return;
        }
        let size = DVec2::new(f64::from(info.width), f64::from(info.height));
        let views = session
            .lock()
            .views3d
            .get(&info.path)
            .cloned()
            .unwrap_or_default();
        let selected = session.lock().selected_layers.clone();
        let ppp = ui.ctx().pixels_per_point();
        self.extra_gpu.resize_with(rects.len(), || None);
        for (i, (&rect, &pane)) in rects.iter().zip(panes).enumerate() {
            if i == active {
                continue;
            }
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, Color32::from_gray(35));
            let (w, h) = (info.width as f32, info.height as f32);
            let scale = ((rect.width() - 24.0) / w)
                .min((rect.height() - 24.0) / h)
                .clamp(0.01, 1.0 / ppp);
            let camera = views.camera_of(pane, size);
            let view = View {
                origin: rect.center() - vec2(w, h) * scale / 2.0,
                scale,
                point: self.prefs.point_size,
                camera: Some((
                    camera.unwrap_or_else(|| view3d::active_camera(comp, size)),
                    size,
                )),
            };
            let frame = render::comp_rect(&view, info.width, info.height);
            let c = info.bg_color;
            let to = |v: f64| (v * 255.0) as u8;
            painter.rect_filled(frame, 0.0, Color32::from_rgb(to(c[0]), to(c[1]), to(c[2])));
            if self.gpu.is_some() && self.extra_gpu[i].is_none() {
                self.extra_gpu[i] = GpuPreview::new(cx.resources);
            }
            if let Some(img) = self.extra_gpu[i].as_mut().and_then(|g| {
                g.frame(
                    cx.resources,
                    cx.stage,
                    comp,
                    self.render_factor,
                    camera,
                    false,
                )
            }) {
                let r = img.rect;
                let at = |x: i32, y: i32| view.to_screen(DVec2::new(f64::from(x), f64::from(y)));
                painter.with_clip_rect(frame.intersect(rect)).image(
                    egui::TextureId::User(img.handle),
                    Rect::from_min_max(at(r.x0, r.y0), at(r.x1, r.y1)),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            let outer = render::Affine::new(bae_anim::glam::DMat4::IDENTITY);
            for l in comp.layers.iter().filter(|l| selected.contains(&l.path)) {
                if let Some(q) = render::bounds_quad(l, &view, &outer) {
                    painter.add(Shape::closed_line(
                        q.to_vec(),
                        Stroke::new(1.0, theme::label_color(8)),
                    ));
                }
            }
            painter.text(
                rect.left_top() + vec2(8.0, 6.0),
                Align2::LEFT_TOP,
                tr(pane.title()),
                theme::small(),
                color::TEXT_DIM,
            );
            if ui
                .interact(rect, ui.id().with(("view", i)), Sense::click())
                .clicked()
            {
                session
                    .lock()
                    .views3d
                    .entry(info.path.clone())
                    .or_default()
                    .activate(i);
            }
        }
        // AE marks the active view's corners.
        let r = rects[active.min(rects.len() - 1)];
        let mark = Color32::from_rgb(0xe8, 0xc8, 0x3c);
        let s = 8.0;
        for (corner, dx, dy) in [
            (r.left_top(), s, s),
            (r.right_top(), -s, s),
            (r.left_bottom(), s, -s),
            (r.right_bottom(), -s, -s),
        ] {
            ui.painter().add(Shape::convex_polygon(
                vec![corner, corner + vec2(dx, 0.0), corner + vec2(0.0, dy)],
                mark,
                Stroke::NONE,
            ));
        }
    }
}
