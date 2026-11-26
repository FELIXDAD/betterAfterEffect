//! Window ▸ Footage: AE's Footage panel, a viewer of one footage item at its
//! own size and time (double-click footage in the Project panel). The frame is
//! drawn by the compositor like a layer would be (same decoders and caches):
//! a one-layer comp the size of the footage, published as a GPU texture. Movies
//! and sequences get AE's time ruler and current time below the image.

use std::sync::Arc;

use bae_gpu::GpuContext;
use bae_gpu::viewport::{ViewportTextureId, ViewportTextureRegistry};
use bae_gpu::wgpu;
use bae_model::{FrameRate, PrimPath, Stage, Time, Value};
use bae_render::{
    AssetFiles, FootageRef, LayerContent, OutputAlpha, RenderComp, RenderLayer, RenderPurpose,
    RenderRequest, Renderer, RendererConfig,
};
use bae_ui::theme::{self, color};
use bae_ui::{Panel, PanelContext, PanelKind, TabInfo, tr};
use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::common;

/// Height of the time ruler and of the bottom bar.
const RULER: f32 = 22.0;
const BAR: f32 = 24.0;

/// What the panel knows about the footage it shows.
#[derive(Clone, Debug, PartialEq)]
struct Footage {
    asset: PrimPath,
    name: String,
    kind: String,
    size: [u32; 2],
    frame_rate: Option<FrameRate>,
    duration: Option<Time>,
    start_frame: u64,
    drop_frame: bool,
}

impl Footage {
    fn of(stage: &Stage, asset: &PrimPath) -> Option<Footage> {
        let view = stage.prim(asset)?;
        let get = |attr: &str| bae_motion::query::static_value(&view, attr);
        let kind = match get("kind") {
            Some(Value::String(s) | Value::Enum(s)) => s,
            _ => "footage".into(),
        };
        if !matches!(kind.as_str(), "footage" | "sequence") {
            return None;
        }
        let dim = |attr: &str| get(attr).and_then(|v| v.as_f64()).unwrap_or(0.0).max(0.0) as u32;
        Some(Footage {
            asset: asset.clone(),
            name: bae_motion::query::display_name_of(stage, asset),
            size: [dim("width"), dim("height")],
            frame_rate: match get("frame_rate") {
                Some(Value::FrameRate(r)) => Some(r),
                Some(v) => v.as_f64().filter(|f| *f > 0.0).map(FrameRate::from_fps),
                None => None,
            },
            start_frame: match get("timecode:start_frame") {
                Some(Value::Int(n)) => n.max(0) as u64,
                _ => 0,
            },
            drop_frame: get("timecode:drop_frame")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            duration: match get("duration") {
                Some(Value::Time(t)) => Some(t),
                _ => None,
            },
            kind,
        })
    }

    /// A movie or sequence (has a time ruler).
    fn timed(&self) -> bool {
        self.duration.is_some_and(|d| d > Time::ZERO)
    }

    fn rate(&self) -> FrameRate {
        self.frame_rate.unwrap_or(FrameRate::FPS_30)
    }
}

/// The renderer and the published frame.
struct Gpu {
    gpu: Arc<GpuContext>,
    registry: Arc<ViewportTextureRegistry>,
    id: ViewportTextureId,
    renderer: Renderer,
    assets: AssetFiles,
    last: Option<(PrimPath, Time)>,
}

impl Gpu {
    fn new(gpu: Arc<GpuContext>, registry: Arc<ViewportTextureRegistry>) -> Self {
        let id = registry.create("footage viewer");
        let renderer = Renderer::with_config(
            gpu.clone(),
            RendererConfig {
                frame_cache_bytes: 128 << 20,
                pool_idle_bytes: 32 << 20,
            },
        );
        Gpu {
            gpu,
            registry,
            id,
            renderer,
            assets: AssetFiles::new(None),
            last: None,
        }
    }

    /// The frame of `footage` at `time` (rendered when it changed); its egui handle.
    fn frame(&mut self, stage: &Stage, footage: &Footage, time: Time) -> Option<u64> {
        let key = (footage.asset.clone(), time);
        if self.last.as_ref() != Some(&key) {
            let view = stage.prim(&footage.asset)?;
            let file = self.assets.file(&view).ok().flatten();
            let [w, h] = footage.size;
            let mut comp = RenderComp::new(footage.asset.clone(), w.max(1), h.max(1));
            comp.layers.push(RenderLayer::new(
                footage.asset.clone(),
                LayerContent::Footage(FootageRef {
                    asset: footage.asset.clone(),
                    file,
                    size: (w > 0 && h > 0).then_some([w, h]),
                    source_time: time,
                    sequence: (footage.kind == "sequence").then(|| footage.rate()),
                    // The footage panel shows source frames as they are.
                    blend: None,
                }),
            ));
            let request = RenderRequest {
                purpose: RenderPurpose::Preview,
                ..RenderRequest::full()
            };
            let frame = self
                .renderer
                .render_frame_with(&comp, &request, None)
                .ok()?;
            let target = self
                .renderer
                .output_texture(&frame, OutputAlpha::Premultiplied, None)
                .ok()?;
            self.registry
                .publish(self.id, target.view().texture(), wgpu::FilterMode::Linear)
                .ok()?;
            self.last = Some(key);
        }
        self.registry.display_handle(self.id)
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        self.registry.remove(self.id);
    }
}

/// Registers the panel (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    crate::register_panel(
        app,
        "footage",
        PanelKind::new("Footage", |spawn| {
            Box::new(FootagePanel {
                display: spawn
                    .state
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default(),
                ..Default::default()
            })
        }),
    );
}

/// The Footage panel.
#[derive(Default)]
pub struct FootagePanel {
    gpu: Option<Gpu>,
    display: crate::source_view_menu::SourceView,
    /// Time inside the footage.
    time: Time,
    /// The footage the time belongs to.
    shown: Option<PrimPath>,
}

impl FootagePanel {
    fn gpu(&mut self, cx: &PanelContext<'_>) -> Option<&mut Gpu> {
        let context = cx.resources.get::<Arc<GpuContext>>()?.clone();
        if self
            .gpu
            .as_ref()
            .is_some_and(|g| !Arc::ptr_eq(&g.gpu, &context) || context.is_lost())
        {
            self.gpu = None;
        }
        if self.gpu.is_none() && !context.is_lost() {
            let registry = cx.resources.get::<Arc<ViewportTextureRegistry>>()?.clone();
            self.gpu = Some(Gpu::new(context, registry));
        }
        self.gpu.as_mut()
    }

    fn footage(cx: &PanelContext<'_>) -> Option<Footage> {
        let asset = common::session(cx.resources).lock().footage_view.clone()?;
        Footage::of(cx.stage, &asset)
    }
}

impl Panel for FootagePanel {
    fn menu(&mut self, ui: &mut Ui, _: &mut PanelContext<'_>) {
        self.display.menu(ui);
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.display).ok()
    }

    fn tab(&self, cx: &PanelContext<'_>) -> TabInfo {
        // AE: "Footage <name>", "Footage (none)" when empty.
        let name = Self::footage(cx).map_or_else(|| tr("(none)").to_owned(), |f| f.name);
        TabInfo::new(format!("{} {name}", tr("Footage")))
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let full = ui.max_rect();
        ui.allocate_rect(full, Sense::hover());
        let footage = Self::footage(cx);
        if footage.as_ref().map(|f| &f.asset) != self.shown.as_ref() {
            self.shown = footage.as_ref().map(|f| f.asset.clone());
            self.time = Time::ZERO;
        }
        let timed = footage.as_ref().is_some_and(Footage::timed);
        let bottom = BAR + if timed { RULER } else { 0.0 };
        let view = Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - bottom));
        let p = ui.painter_at(full);
        p.rect_filled(view, 0.0, self.display.background(ui));
        let mut zoom = None;
        if let Some(f) = &footage {
            let stage = cx.stage;
            let time = self.time;
            if let Some(handle) = self.gpu(cx).and_then(|g| g.frame(stage, f, time)) {
                let [w, h] = f.size.map(|v| v.max(1) as f32);
                let w = w * self.display.aspect(stage, Some(&f.asset));
                // Fit (AE's default magnification for the Footage panel), never
                // above 100 %.
                let scale = ((view.width() - 20.0) / w)
                    .min((view.height() - 20.0) / h)
                    .clamp(0.01, 1.0);
                let r = Rect::from_center_size(view.center(), vec2(w, h) * scale);
                if self.display.grid {
                    crate::viewer::checkerboard(&p, r.intersect(view));
                }
                p.image(
                    egui::TextureId::User(handle),
                    r,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                zoom = Some(scale);
            }
        }
        if timed && let Some(f) = &footage {
            let ruler =
                Rect::from_min_size(pos2(full.min.x, view.max.y), vec2(full.width(), RULER));
            self.ruler(ui, ruler, f);
        }
        let bar = Rect::from_min_max(pos2(full.min.x, full.max.y - BAR), full.max);
        self.bar(ui, bar, footage.as_ref(), zoom);
    }
}

impl FootagePanel {
    /// AE's time ruler: the footage's duration with the current-time indicator,
    /// click or drag to move it.
    fn ruler(&mut self, ui: &Ui, r: Rect, f: &Footage) {
        let resp = ui.interact(r, ui.id().with("footage-ruler"), Sense::click_and_drag());
        let p = ui.painter_at(r);
        p.rect_filled(r, 0.0, color::PANEL);
        let track = r.shrink2(vec2(10.0, 4.0));
        let rate = f.rate();
        let duration = f.duration.unwrap_or(Time::ZERO);
        let last = rate.time_to_frame(duration).max(1) - 1;
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let frame = (t * last as f32).round() as i64;
            self.time = rate.frame_to_time(frame);
        }
        // Ticks: one per second.
        let seconds = duration.as_seconds().max(0.0);
        let step = (seconds / 20.0).ceil().max(1.0);
        let mut s = 0.0;
        while s <= seconds {
            let x = track.left() + (s / seconds.max(1e-9)) as f32 * track.width();
            p.line_segment(
                [pos2(x, track.bottom() - 5.0), pos2(x, track.bottom())],
                Stroke::new(1.0, color::TEXT_DIM),
            );
            s += step;
        }
        let frame = rate.time_to_frame(self.time).clamp(0, last);
        let x = track.left() + frame as f32 / last.max(1) as f32 * track.width();
        p.line_segment(
            [pos2(x, r.top()), pos2(x, r.bottom())],
            Stroke::new(1.0, color::ACCENT),
        );
        p.add(egui::Shape::convex_polygon(
            vec![
                pos2(x - 5.0, r.top()),
                pos2(x + 5.0, r.top()),
                pos2(x, r.top() + 6.0),
            ],
            color::ACCENT,
            Stroke::NONE,
        ));
    }

    /// The bottom bar: magnification and, for movies, the current time.
    fn bar(&self, ui: &Ui, r: Rect, f: Option<&Footage>, zoom: Option<f32>) {
        let p = ui.painter_at(r);
        p.rect_filled(r, 0.0, color::PANEL);
        let font = theme::body();
        let y = r.center().y;
        let zoom = zoom.map_or_else(
            || "100%".to_owned(),
            |z| format!("{}%", (z * 100.0 * 10.0).round() / 10.0),
        );
        p.text(
            pos2(r.left() + 8.0, y),
            Align2::LEFT_CENTER,
            zoom,
            font.clone(),
            color::TEXT,
        );
        if let Some(f) = f.filter(|f| f.timed()) {
            let rate = f.rate();
            let frame = f
                .start_frame
                .saturating_add(rate.time_to_frame(self.time).max(0) as u64);
            let code =
                bae_media::time::Timecode::from_frame_number(frame, rate, f.drop_frame).to_string();
            p.text(
                pos2(r.left() + 70.0, y),
                Align2::LEFT_CENTER,
                code,
                font.clone(),
                color::ACCENT,
            );
        }
        if let Some(f) = f {
            let info = format!("{} × {}", f.size[0], f.size[1]);
            p.text(
                pos2(r.right() - 8.0, y),
                Align2::RIGHT_CENTER,
                info,
                font,
                color::TEXT_DIM,
            );
        }
    }
}
