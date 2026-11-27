//! Window ▸ Footage: AE's Footage panel, a viewer of one footage item at its
//! own size and time (double-click footage in the Project panel). The frame is
//! drawn by the compositor like a layer would be (same decoders and caches):
//! a one-layer comp the size of the footage, published as a GPU texture. Movies
//! and sequences get AE's time ruler and current time below the image.
//! Spacebar with the panel focused plays the footage here, with its own sound
//! (AE: the preview plays in the focused viewer).

use std::sync::Arc;

use bae_gpu::GpuContext;
use bae_gpu::viewport::{ViewportTextureId, ViewportTextureRegistry};
use bae_gpu::wgpu;
use bae_model::{FrameRate, PrimPath, Stage, Time, Value};
use bae_render::{
    AssetFiles, FootageRef, LayerContent, OutputAlpha, RenderComp, RenderLayer, RenderPurpose,
    RenderRequest, Renderer, RendererConfig,
};
use bae_ui::Accessible as _;
use bae_ui::theme::{self, color};
use bae_ui::{Invocation, Panel, PanelContext, PanelKind, TabInfo, tr};
use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::common;
use crate::session::SharedSession;

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
    marks: (Time, Time),
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
        if !matches!(kind.as_str(), "footage" | "sequence" | "solid") {
            return None;
        }
        let dim = |attr: &str| get(attr).and_then(|v| v.as_f64()).unwrap_or(0.0).max(0.0) as u32;
        let duration = get("duration").and_then(|v| v.as_time());
        Some(Footage {
            asset: asset.clone(),
            name: bae_motion::query::display_name_of(stage, asset),
            size: [dim("width"), dim("height")],
            frame_rate: match get("frame_rate") {
                Some(Value::FrameRate(r)) => Some(r),
                Some(v) => v.as_f64().filter(|f| *f > 0.0).map(FrameRate::from_fps),
                None => None,
            },
            duration,
            marks: bae_motion::query::footage_range(stage, asset, duration.unwrap_or(Time::ZERO)),
            start_frame: match get("timecode:start_frame") {
                Some(Value::Int(n)) => n.max(0) as u64,
                _ => 0,
            },
            drop_frame: get("timecode:drop_frame")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            kind,
        })
    }

    /// A solid's color (AE shows solids in the Footage panel too).
    fn solid_color(&self, stage: &Stage) -> Option<[f32; 4]> {
        (self.kind == "solid")
            .then(|| bae_motion::solid::solid_info(stage, &self.asset))
            .flatten()
            .map(|s| s.color.map(|v| v as f32))
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
    last: Option<(PrimPath, Time, u64)>,
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
        let key = (footage.asset.clone(), time, stage.revision());
        if self.last.as_ref() != Some(&key) {
            let view = stage.prim(&footage.asset)?;
            let file = self.assets.file(&view).ok().flatten();
            let [w, h] = footage.size;
            let mut comp = RenderComp::new(footage.asset.clone(), w.max(1), h.max(1));
            let content = match footage.solid_color(stage) {
                Some(color) => LayerContent::Solid {
                    color,
                    width: w.max(1),
                    height: h.max(1),
                },
                None => LayerContent::Footage(FootageRef {
                    asset: footage.asset.clone(),
                    file_revision: bae_model::asset::file_revision(&view),
                    psd_layer: bae_model::psd::LayerImage::from_asset(&view),
                    file,
                    size: (w > 0 && h > 0).then_some([w, h]),
                    source_time: time,
                    alphabetical: view
                        .value("sequence:pattern")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s.starts_with("*.")),
                    sequence: (footage.kind == "sequence").then(|| footage.rate()),
                    // The footage panel shows source frames as they are.
                    blend: None,
                }),
            };
            let mut layer = RenderLayer::new(footage.asset.clone(), content);
            layer.resolve_photoshop_mask(stage);
            if let Ok(path) = footage.asset.child(bae_model::layer_styles::PRIM)
                && let Some(style) = stage.prim(&path)
            {
                let params = style
                    .attr_names()
                    .into_iter()
                    .filter_map(|key| {
                        Some((bae_model::effect::param_id(key)?, style.value(key)?.clone()))
                    })
                    .collect();
                layer.effects.push(bae_render::RenderEffect {
                    path: path.clone(),
                    effect: bae_model::layer_styles::RENDER_ID.into(),
                    enabled: true,
                    params,
                });
            }
            comp.layers.push(layer);
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
    /// Playing here (Spacebar with the panel focused).
    playing: Option<Playing>,
}

struct Playing {
    started: std::time::Instant,
    from: Time,
    repeat: bool,
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
        let gpu = self.gpu.as_mut()?;
        if let Some(crate::PreviewEffects(registry)) = cx.resources.get::<crate::PreviewEffects>()
            && gpu.renderer.sync_effects(registry)
        {
            gpu.last = None;
        }
        Some(gpu)
    }

    /// Spacebar: play the footage from the current time (from the start when
    /// at its end), with its sound; the composition stops.
    fn play(&mut self, cx: &PanelContext<'_>, f: &Footage) {
        let session = common::session(cx.resources);
        let mut s = session.lock();
        s.stop();
        let duration = f.duration.unwrap_or(Time::ZERO);
        let from = if self.time + f.rate().frame_to_time(1) >= duration {
            Time::ZERO
        } else {
            self.time
        };
        let repeat = s.preview.profiles[crate::session::PreviewShortcut::Space as usize].repeat;
        s.sound_footage(cx, &f.asset, from, duration, repeat);
        self.time = from;
        self.playing = Some(Playing {
            started: std::time::Instant::now(),
            from,
            repeat,
        });
    }

    fn stop(&mut self, session: &SharedSession) {
        if self.playing.take().is_some() {
            session.lock().stop_footage_sound();
        }
    }

    /// Advance the playing time; stops at the end unless the preview loops,
    /// and when the composition starts playing.
    fn advance(&mut self, ui: &Ui, session: &SharedSession, f: &Footage) {
        let Some(play) = &self.playing else {
            return;
        };
        let duration = f.duration.unwrap_or(Time::ZERO);
        if session.lock().is_playing() || duration <= Time::ZERO {
            self.stop(session);
            return;
        }
        let mut t = play.from + Time::from_seconds(play.started.elapsed().as_secs_f64());
        if t >= duration {
            if play.repeat {
                t = Time(t.0 % duration.0);
            } else {
                self.time = duration - f.rate().frame_to_time(1);
                self.stop(session);
                return;
            }
        }
        let rate = f.rate();
        self.time = rate.frame_to_time(rate.time_to_frame(t));
        ui.ctx().request_repaint();
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

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        if action.id != crate::actions::ids::PLAY {
            return false;
        }
        // A still has nothing to play (AE).
        if let Some(f) = Self::footage(cx).filter(Footage::timed) {
            if self.playing.is_some() {
                self.stop(&common::session(cx.resources));
            } else {
                self.play(cx, &f);
            }
        }
        true
    }

    fn on_close(&mut self, cx: &mut PanelContext<'_>) {
        self.stop(&common::session(cx.resources));
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.display).ok()
    }

    fn tab(&self, cx: &PanelContext<'_>) -> TabInfo {
        // AE: "Footage <name>", "Footage (none)" when empty.
        match Self::footage(cx) {
            Some(f) => common::related_tab(cx, "Footage", &f.asset, &f.name),
            None => TabInfo::new(format!("{} {}", tr("Footage"), tr("(none)"))),
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let full = ui.max_rect();
        ui.allocate_rect(full, Sense::hover());
        let footage = Self::footage(cx);
        let session = common::session(cx.resources);
        if footage.as_ref().map(|f| &f.asset) != self.shown.as_ref() {
            self.stop(&session);
            self.shown = footage.as_ref().map(|f| f.asset.clone());
            self.time = Time::ZERO;
        }
        if let Some(f) = &footage {
            self.advance(ui, &session, f);
        }
        let timed = footage.as_ref().is_some_and(Footage::timed);
        let bottom = BAR + if timed { RULER } else { 0.0 };
        let view = Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - bottom));
        if timed && let Some(f) = &footage {
            let ruler =
                Rect::from_min_size(pos2(full.min.x, view.max.y), vec2(full.width(), RULER));
            if self.ruler(ui, ruler, f) {
                self.stop(&session);
            }
        }
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
                    crate::viewer::checkerboard(&p, r.intersect(view), cx.resources);
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
        let bar = Rect::from_min_max(pos2(full.min.x, full.max.y - BAR), full.max);
        self.bar(ui, cx, bar, footage.as_ref(), zoom);
    }
}

impl FootagePanel {
    /// AE's time ruler: the footage's duration with the current-time indicator,
    /// click or drag to move it.
    /// Returns whether the pointer moved the time (that stops playback).
    fn ruler(&mut self, ui: &Ui, r: Rect, f: &Footage) -> bool {
        let resp = ui.interact(r, ui.id().with("footage-ruler"), Sense::click_and_drag());
        let p = ui.painter_at(r);
        p.rect_filled(r, 0.0, color::PANEL);
        let track = r.shrink2(vec2(10.0, 4.0));
        let rate = f.rate();
        let duration = f.duration.unwrap_or(Time::ZERO);
        let last = rate.time_to_frame(duration).max(1) - 1;
        let xpos = |t: Time| {
            track.left()
                + track.width() * (t.as_seconds() / duration.as_seconds()).clamp(0.0, 1.0) as f32
        };
        p.rect_filled(
            Rect::from_min_max(
                pos2(xpos(f.marks.0), track.bottom() - 3.0),
                pos2(xpos(f.marks.1), track.bottom()),
            ),
            0.0,
            color::ACCENT_DIM,
        );
        let moved = if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let frame = (t * last as f32).round() as i64;
            self.time = rate.frame_to_time(frame);
            true
        } else {
            false
        };
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
        moved
    }

    /// The bottom bar: magnification and, for movies, the current time.
    fn bar(
        &self,
        ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        r: Rect,
        f: Option<&Footage>,
        zoom: Option<f32>,
    ) {
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
            // As AE and the rest of BAE write it: 0:00:00:00, drop frame 0;00;00;00.
            let code = crate::dialogs::fields::format_timecode(
                i64::try_from(frame).unwrap_or(i64::MAX),
                rate.as_f64(),
                f.drop_frame,
            );
            p.text(
                pos2(r.left() + 70.0, y),
                Align2::LEFT_CENTER,
                code,
                font.clone(),
                color::ACCENT,
            );
            let mut controls = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_max(
                        pos2((r.left() + 148.0).min(r.right()), r.top()),
                        r.max,
                    ))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            let step = rate.frame_to_time(1);
            let (first, end) = f.marks;
            for (button, tip, attr, value, enabled, shown) in [
                (
                    "{",
                    "Set In Point to Current Time",
                    "footage:in",
                    self.time,
                    self.time < end,
                    first,
                ),
                (
                    "}",
                    "Set Out Point to Current Time",
                    "footage:out",
                    (self.time + step).min(f.duration.unwrap_or(end)),
                    self.time >= first,
                    (end - step).max(first),
                ),
            ] {
                if controls
                    .add_enabled(enabled, egui::Button::new(button).small())
                    .labeled(egui::WidgetType::Button, tr(tip))
                    .on_hover_text(tr(tip))
                    .clicked()
                {
                    cx.invoke_with("attr.set", serde_json::json!({"path": f.asset, "attr": attr, "value": Value::Time(value)}));
                }
                controls.small(bae_ui::widgets::timecode(
                    rate.time_to_frame(shown),
                    rate.as_f64(),
                ));
            }
        }
        if let Some(f) = f.filter(|f| !f.timed() || r.width() >= 460.0) {
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
