//! Window ▸ Lumetri Scopes: AE's video scopes of the frame the Composition
//! viewer shows — Vectorscope YUV, Histogram, Parade (RGB) and Waveform (Luma),
//! chosen from the panel menu like AE's. Computed on the GPU from the viewer's
//! published frame (no read-back): one pass bins the pixels, one draws the
//! scope image the panel shows; graticules and labels are painted on top.

use std::sync::Arc;

use bae_gpu::GpuContext;
use bae_gpu::viewport::{ViewportTextureId, ViewportTextureRegistry};
use bae_gpu::wgpu;
use bae_ui::{Panel, PanelContext, tr};
use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

use crate::common;

/// Bins: luma waveform, RGB parade (3×), vectorscope (256 × 256 each),
/// histogram (R, G, B, luma × 256) and the histogram peak.
const BINS: u64 = 65536 * 8 + 1024 + 1;
/// The scope image: four 256 × 256 tiles.
const IMAGE: [u32; 2] = [1024, 256];
/// At most this many pixels are sampled (a 640 × 360 grid).
const SAMPLES: [u32; 2] = [640, 360];

/// The scopes AE's panel menu lists, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Scope {
    Vectorscope,
    Histogram,
    Parade,
    Waveform,
}

impl Scope {
    const ALL: [Scope; 4] = [
        Scope::Vectorscope,
        Scope::Histogram,
        Scope::Parade,
        Scope::Waveform,
    ];

    fn title(self) -> &'static str {
        match self {
            Scope::Vectorscope => "Vectorscope YUV",
            Scope::Histogram => "Histogram",
            Scope::Parade => "Parade (RGB)",
            Scope::Waveform => "Waveform",
        }
    }

    /// Tile of the scope image.
    fn tile(self) -> u32 {
        match self {
            Scope::Vectorscope => 0,
            Scope::Histogram => 1,
            Scope::Parade => 2,
            Scope::Waveform => 3,
        }
    }
}

/// Waveform Type in AE: RGB overlay, YC or luma alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Wave {
    Rgb,
    Yc,
    YcNoChroma,
    #[default]
    Luma,
}

impl Wave {
    fn title(self) -> &'static str {
        match self {
            Wave::Rgb => "RGB",
            Wave::Yc => "YC",
            Wave::Luma => "Luma",
            Wave::YcNoChroma => "YC No Chroma",
        }
    }
}

/// The channel set and trace colors in the parade.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Parade {
    #[default]
    Rgb,
    RgbWhite,
    Yuv,
    YuvWhite,
}
impl Parade {
    const ALL: [(Self, &'static str); 4] = [
        (Self::Rgb, "RGB"),
        (Self::RgbWhite, "RGB-White"),
        (Self::Yuv, "YUV"),
        (Self::YuvWhite, "YUV-White"),
    ];

    fn title(self) -> &'static str {
        Self::ALL[self as usize].1
    }
}

mod settings;
use settings::{PRESETS, Space};

/// Brightness of the traces (AE: Bright, Normal, Dimmed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Brightness {
    Bright,
    #[default]
    Normal,
    Dimmed,
}

impl Brightness {
    fn title(self) -> &'static str {
        match self {
            Brightness::Bright => "Bright",
            Brightness::Normal => "Normal",
            Brightness::Dimmed => "Dimmed",
        }
    }

    /// Divides the brightness unit of the shader.
    fn gain(self) -> f32 {
        match self {
            Brightness::Bright => 2.5,
            Brightness::Normal => 1.0,
            Brightness::Dimmed => 0.4,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct State {
    shown: Vec<Scope>,
    wave: Wave,
    parade: Parade,
    brightness: Brightness,
    space: Space,
}

impl Default for State {
    fn default() -> Self {
        State {
            shown: Scope::ALL.to_vec(),
            wave: Wave::default(),
            parade: Parade::default(),
            brightness: Brightness::default(),
            space: Space::Automatic,
        }
    }
}

impl State {
    /// What the scope image depends on besides the frame.
    fn key(&self) -> u64 {
        (self.space as u64) << 24
            | (self.parade as u64) << 16
            | (self.wave as u64) << 8
            | self.brightness as u64
    }
}

/// The GPU side: pipelines, bins and the published scope image.
struct Gpu {
    gpu: Arc<GpuContext>,
    registry: Arc<ViewportTextureRegistry>,
    id: ViewportTextureId,
    layout: wgpu::BindGroupLayout,
    accumulate: wgpu::ComputePipeline,
    peak: wgpu::ComputePipeline,
    draw: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    bins: wgpu::Buffer,
    image: wgpu::Texture,
    /// Key of the frame the image shows.
    shown: Option<(u64, u64)>,
}

impl Gpu {
    fn new(gpu: Arc<GpuContext>, registry: Arc<ViewportTextureRegistry>) -> Self {
        let device = gpu.device();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae.scopes"),
            source: wgpu::ShaderSource::Wgsl(settings::shader_source().into()),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae.scopes"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    2,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    3,
                    wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                ),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bae.scopes"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("bae.scopes"),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let accumulate = pipeline("accumulate");
        let peak = pipeline("peak");
        let draw = pipeline("draw");
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bae.scopes.params"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bins = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bae.scopes.bins"),
            size: BINS * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let image = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bae.scopes.image"),
            size: wgpu::Extent3d {
                width: IMAGE[0],
                height: IMAGE[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let id = registry.create("lumetri scopes");
        Gpu {
            gpu,
            registry,
            id,
            layout,
            accumulate,
            peak,
            draw,
            params,
            bins,
            image,
            shown: None,
        }
    }

    /// Recomputes the scopes of `frame` when it changed; the published image's
    /// egui handle (none until the shell registered it).
    fn update(&mut self, frame: &wgpu::Texture, key: u64, state: &State) -> Option<u64> {
        let key = (key, state.key());
        if self.shown != Some(key) {
            self.compute(frame, state);
            if self
                .registry
                .publish(self.id, &self.image, wgpu::FilterMode::Linear)
                .is_ok()
            {
                self.shown = Some(key);
            }
        }
        self.registry.display_handle(self.id)
    }

    fn compute(&self, frame: &wgpu::Texture, state: &State) {
        let (w, h) = (frame.width().max(1), frame.height().max(1));
        let step = w.div_ceil(SAMPLES[0]).max(h.div_ceil(SAMPLES[1])).max(1);
        let (sx, sy) = (w.div_ceil(step), h.div_ceil(step));
        let unit = (sx * sy) as f32 / 65536.0 / state.brightness.gain();
        let mut params = [0u8; 32];
        params[0..4].copy_from_slice(&w.to_le_bytes());
        params[4..8].copy_from_slice(&h.to_le_bytes());
        params[8..12].copy_from_slice(&step.to_le_bytes());
        params[12..16].copy_from_slice(&unit.max(1e-3).to_le_bytes());
        let wave: u32 = match state.wave {
            Wave::Luma => 0,
            Wave::Rgb => 1,
            Wave::Yc => 2,
            Wave::YcNoChroma => 3,
        };
        params[16..20].copy_from_slice(&wave.to_le_bytes());
        params[20..24].copy_from_slice(&(state.parade as u32).to_le_bytes());
        params[24..28].copy_from_slice(&(state.space as u32).to_le_bytes());
        let queue = self.gpu.queue();
        queue.write_buffer(&self.params, 0, &params);
        let device = self.gpu.device();
        let src = frame.create_view(&wgpu::TextureViewDescriptor::default());
        let dst = self
            .image
            .create_view(&wgpu::TextureViewDescriptor::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae.scopes"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&src),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.bins.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&dst),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bae.scopes"),
        });
        encoder.clear_buffer(&self.bins, 0, None);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("bae.scopes"),
                timestamp_writes: None,
            });
            pass.set_bind_group(0, &group, &[]);
            pass.set_pipeline(&self.accumulate);
            pass.dispatch_workgroups(sx.div_ceil(16), sy.div_ceil(16), 1);
            pass.set_pipeline(&self.peak);
            pass.dispatch_workgroups(1, 1, 1);
            pass.set_pipeline(&self.draw);
            pass.dispatch_workgroups(IMAGE[0] / 16, IMAGE[1] / 16, 1);
        }
        queue.submit([encoder.finish()]);
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        self.registry.remove(self.id);
    }
}

/// Registers the panel and its Chinese strings (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    if let Some(registry) = app.resource::<Arc<ViewportTextureRegistry>>().cloned() {
        let scopes = Arc::new(ScopesWarmUp {
            registry,
            gpu: std::sync::Mutex::default(),
        });
        app.insert_resource(scopes.clone());
        let _ = app.register_arc::<dyn bae_gpu::WarmUp>("bae.ui-motion/lumetri-scopes", scopes);
    }
    crate::register_panel(
        app,
        "lumetri_scopes",
        bae_ui::PanelKind::new("Lumetri Scopes", |spawn| {
            Box::new(ScopesPanel::from_state(spawn.state))
        }),
    );
}

struct ScopesWarmUp {
    registry: Arc<ViewportTextureRegistry>,
    gpu: std::sync::Mutex<Option<Gpu>>,
}
impl bae_gpu::WarmUp for ScopesWarmUp {
    fn name(&self) -> &str {
        "Lumetri Scopes"
    }
    fn run(&self, gpu: &Arc<GpuContext>) -> Result<(), String> {
        let ready = Gpu::new(gpu.clone(), self.registry.clone());
        *self.gpu.lock().unwrap_or_else(|e| e.into_inner()) = Some(ready);
        Ok(())
    }
}

/// The Lumetri Scopes panel.
#[derive(Default)]
pub struct ScopesPanel {
    state: State,
}

impl ScopesPanel {
    pub fn from_state(state: Option<&serde_json::Value>) -> Self {
        ScopesPanel {
            state: state
                .and_then(|s| serde_json::from_value(s.clone()).ok())
                .unwrap_or_default(),
        }
    }
}

impl Panel for ScopesPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        let frame = session.lock().scope_frame.clone();
        let full = ui.max_rect();
        let resp = ui.allocate_rect(full, Sense::click());
        resp.context_menu(|ui| self.menu_items(ui));
        let p = ui.painter_at(full);
        p.rect_filled(full, 0.0, Color32::from_gray(18));
        let space = self.state.space.resolve(cx.stage);
        let handle = frame.and_then(|(texture, key)| {
            let context = cx.resources.get::<Arc<GpuContext>>()?;
            let warm = cx.resources.get::<Arc<ScopesWarmUp>>()?;
            let mut lock = warm.gpu.try_lock().ok()?;
            let gpu = lock
                .as_mut()
                .filter(|g| Arc::ptr_eq(&g.gpu, context) && !context.is_lost())?;
            let mut state = self.state.clone();
            state.space = space;
            gpu.update(&texture, key, &state)
        });
        let shown = self.state.shown.clone();
        if shown.is_empty() {
            return;
        }
        // AE lays the chosen scopes out in a grid, in menu order.
        let cols = if shown.len() > 1 { 2 } else { 1 };
        let rows = shown.len().div_ceil(cols);
        let cell = vec2(full.width() / cols as f32, full.height() / rows as f32);
        for (i, scope) in shown.iter().enumerate() {
            let min = full.min + vec2((i % cols) as f32 * cell.x, (i / cols) as f32 * cell.y);
            let r = Rect::from_min_size(min, cell).shrink(10.0);
            draw_scope(&p, *scope, r, handle, self.state.parade, space);
        }
    }

    fn menu(&mut self, ui: &mut Ui, _cx: &mut PanelContext<'_>) {
        self.menu_items(ui);
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.state).ok()
    }
}

impl ScopesPanel {
    /// The scope check items of AE's panel menu (the wrench / right-click menu).
    fn menu_items(&mut self, ui: &mut Ui) {
        bae_ui::widgets::menu_button(ui, tr("Presets"), |ui| {
            for preset in PRESETS {
                if ui
                    .radio(preset.matches(&self.state), tr(preset.title))
                    .clicked()
                {
                    preset.apply(&mut self.state);
                    ui.close();
                }
            }
        });
        ui.separator();
        for scope in Scope::ALL {
            let mut on = self.state.shown.contains(&scope);
            let title = match scope {
                Scope::Parade => {
                    tr("Parade ({type})").replace("{type}", tr(self.state.parade.title()))
                }
                Scope::Waveform => {
                    tr("Waveform ({type})").replace("{type}", tr(self.state.wave.title()))
                }
                _ => tr(scope.title()).to_owned(),
            };
            if ui.checkbox(&mut on, title).changed() {
                if on {
                    self.state.shown.push(scope);
                    self.state
                        .shown
                        .sort_by_key(|s| Scope::ALL.iter().position(|a| a == s));
                } else {
                    self.state.shown.retain(|s| *s != scope);
                }
            }
        }
        ui.separator();
        bae_ui::widgets::menu_button(ui, tr("Parade Type"), |ui| {
            for (mode, name) in Parade::ALL {
                if ui
                    .radio_value(&mut self.state.parade, mode, tr(name))
                    .clicked()
                {
                    ui.close();
                }
            }
        });
        bae_ui::widgets::menu_button(ui, tr("Waveform Type"), |ui| {
            for wave in [Wave::Rgb, Wave::Luma, Wave::Yc, Wave::YcNoChroma] {
                ui.radio_value(&mut self.state.wave, wave, tr(wave.title()));
            }
        });
        bae_ui::widgets::menu_button(ui, tr("Color Space"), |ui| {
            for (space, title) in Space::ALL {
                if ui
                    .radio_value(&mut self.state.space, space, tr(title))
                    .clicked()
                {
                    ui.close();
                }
            }
        });
        bae_ui::widgets::menu_button(ui, tr("Brightness"), |ui| {
            for b in [Brightness::Bright, Brightness::Normal, Brightness::Dimmed] {
                ui.radio_value(&mut self.state.brightness, b, tr(b.title()));
            }
        });
    }
}

/// One scope: its tile of the scope image (a square for the vectorscope) and
/// AE's graticule.
fn draw_scope(
    p: &egui::Painter,
    scope: Scope,
    area: Rect,
    handle: Option<u64>,
    parade: Parade,
    space: Space,
) {
    let r = match scope {
        Scope::Vectorscope => {
            let s = area.width().min(area.height());
            Rect::from_center_size(area.center(), vec2(s, s))
        }
        _ => Rect::from_min_max(area.min + vec2(28.0, 0.0), area.max),
    };
    if r.width() < 8.0 || r.height() < 8.0 {
        return;
    }
    if let Some(h) = handle {
        let t = scope.tile() as f32 / 4.0;
        p.image(
            egui::TextureId::User(h),
            r,
            Rect::from_min_max(pos2(t, 0.0), pos2(t + 0.25, 1.0)),
            Color32::WHITE,
        );
    }
    let line = Stroke::new(1.0, Color32::from_rgba_unmultiplied(160, 160, 160, 70));
    let font = bae_ui::theme::small();
    let label = Color32::from_gray(150);
    match scope {
        Scope::Vectorscope => {
            let c = r.center();
            for f in [0.25, 0.5] {
                p.circle_stroke(c, r.width() * f * 0.9, line);
            }
            p.line_segment([pos2(r.left(), c.y), pos2(r.right(), c.y)], line);
            p.line_segment([pos2(c.x, r.top()), pos2(c.x, r.bottom())], line);
            // 75 % color targets (Rec. 709).
            for (name, rgb) in [
                ("R", [0.75, 0.0, 0.0]),
                ("Mg", [0.75, 0.0, 0.75]),
                ("B", [0.0, 0.0, 0.75]),
                ("Cy", [0.0, 0.75, 0.75]),
                ("G", [0.0, 0.75, 0.0]),
                ("Yl", [0.75, 0.75, 0.0]),
            ] {
                let [cb, cr] = cb_cr(rgb, space);
                let at = pos2(
                    r.left() + (cb + 0.5) * r.width(),
                    r.top() + (0.5 - cr) * r.height(),
                );
                p.rect_stroke(
                    Rect::from_center_size(at, vec2(8.0, 8.0)),
                    0.0,
                    line,
                    egui::StrokeKind::Middle,
                );
                p.text(
                    at + vec2(7.0, -7.0),
                    Align2::LEFT_BOTTOM,
                    name,
                    font.clone(),
                    label,
                );
            }
        }
        _ => {
            for (i, v) in [100, 75, 50, 25, 0].into_iter().enumerate() {
                let y = r.top() + r.height() * i as f32 / 4.0;
                p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], line);
                p.text(
                    pos2(r.left() - 4.0, y),
                    Align2::RIGHT_CENTER,
                    v.to_string(),
                    font.clone(),
                    label,
                );
            }
            if scope == Scope::Parade {
                let channels = match parade {
                    Parade::Rgb | Parade::RgbWhite => ["R", "G", "B"],
                    Parade::Yuv | Parade::YuvWhite => ["Y", "U", "V"],
                };
                for (k, name) in channels.into_iter().enumerate() {
                    let x = r.left() + r.width() * (k as f32 + 0.5) / 3.0;
                    p.text(
                        pos2(x, r.bottom() + 2.0),
                        Align2::CENTER_TOP,
                        name,
                        font.clone(),
                        label,
                    );
                }
            }
        }
    }
}

/// Rec. 709 Cb, Cr of display RGB (as the shader bins them).
fn cb_cr(rgb: [f32; 3], space: Space) -> [f32; 2] {
    let [r, b] = space.weights();
    let y = r * rgb[0] + (1.0 - r - b) * rgb[1] + b * rgb[2];
    [
        (rgb[2] - y) / (2.0 * (1.0 - b)),
        (rgb[0] - y) / (2.0 * (1.0 - r)),
    ]
}

/// Chinese for the panel (AE 26's zh_CN dictionary).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Vectorscope YUV", "矢量示波器 YUV"),
    ("Histogram", "直方图"),
    ("Parade (RGB)", "分量 (RGB)"),
    ("Parade ({type})", "分量 ({type})"),
    ("Luma", "明度"),
    ("Color Space", "色彩空间"),
    ("Presets", "预设"),
    ("All Scopes RGB", "所有示波器 RGB"),
    ("All Scopes YUV/YC", "所有示波器 YUV/YC"),
    ("Parade RGB", "分量 RGB"),
    ("Parade RGB/Waveform YC", "分量 RGB/波形 YC"),
    ("Parade/Waveform RGB", "分量/波形 RGB"),
    (
        "Vectorscope YUV/Parade RGB/Waveform YC",
        "矢量示波器 YUV/分量 RGB/波形 YC",
    ),
    (
        "Vectorscope YUV/Parade YUV/Waveform YC",
        "矢量示波器 YUV/分量 YUV/波形 YC",
    ),
    ("Waveform RGB", "波形 RGB"),
    ("Parade Type", "分量类型"),
    ("RGB-White", "RGB-白色"),
    ("YUV-White", "YUV-白色"),
    ("Waveform ({type})", "波形 ({type})"),
    ("YC No Chroma", "YC 无色度"),
    ("Waveform Type", "波形类型"),
    ("Brightness", "亮度"),
    ("Bright", "明亮"),
    ("Dimmed", "昏暗"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parade_menu_modes_select_distinct_images_and_localized_titles() {
        bae_ui::i18n::register(ZH);
        let mut state = State::default();
        let mut keys = std::collections::BTreeSet::new();
        let choices = Parade::ALL.map(|(mode, en)| {
            state.parade = mode;
            assert!(keys.insert(state.key()));
            (
                bae_ui::i18n::translation(en).unwrap_or(en),
                mode == Parade::default(),
            )
        });
        assert_eq!(
            choices,
            [
                ("RGB", true),
                ("RGB-白色", false),
                ("YUV", false),
                ("YUV-白色", false)
            ]
        );
    }

    #[test]
    fn color_targets_sit_where_ae_draws_them() {
        // Red up-left of center, blue to the right, green down-left.
        let [cb, cr] = cb_cr([0.75, 0.0, 0.0], Space::Rec709);
        assert!(cb < 0.0 && cr > 0.0);
        let [cb, _] = cb_cr([0.0, 0.0, 0.75], Space::Rec709);
        assert!(cb > 0.0);
        let [cb, cr] = cb_cr([0.0, 0.75, 0.0], Space::Rec709);
        assert!(cb < 0.0 && cr < 0.0);
        // Gray has no color.
        let [cb, cr] = cb_cr([0.5, 0.5, 0.5], Space::Rec709);
        assert!(cb.abs() < 1e-6 && cr.abs() < 1e-6);
    }

    #[test]
    fn scopes_compute_on_the_gpu() {
        let Some(gpu) = bae_gpu::testing::shared_context() else {
            return;
        };
        let device = gpu.device();
        let frame = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test frame"),
            size: wgpu::Extent3d {
                width: 1920,
                height: 1080,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let registry = Arc::new(ViewportTextureRegistry::new());
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut scopes = Gpu::new(gpu.clone(), registry);
        scopes.update(&frame, 1, &State::default());
        let error = pollster_block(scope.pop());
        assert!(error.is_none(), "{error:?}");
        assert_eq!(scopes.shown, Some((1, State::default().key())));
    }

    fn pollster_block<F: std::future::Future>(f: F) -> F::Output {
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        let mut f = std::pin::pin!(f);
        loop {
            if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }
}
