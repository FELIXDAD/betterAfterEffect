//! AE's Liquify (`ADBE LIQUIFY`, Distort): brushes painted in the Layer panel
//! push, twirl, pucker and bloat the layer. The result is the Distortion Mesh
//! parameter, a grid of sample offsets that keyframes like any value: each
//! output pixel reads the layer at its position plus the mesh's offset there,
//! scaled by Distortion Percentage and shifted by Distortion Mesh Offset.

use std::sync::Arc;

use bae_gpu::wgpu;
use bae_model::{EffectDef, ParamKind, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use glam::DVec2;

mod brush;
pub use brush::{Brush, BrushStroke, unfrozen_weight};

use super::image_grid;
use crate::gpu::{self, Binding, Kernel, KernelKey, KernelSource, Resource, UniformWriter};
use crate::{Builtin, EffectTarget, TargetEffect, catalog::DefBuilder, params::Params};

/// The effect's matchName.
pub const MATCH_NAME: &str = "ADBE LIQUIFY";
/// Parameter: the brush tool (1-based index into [`TOOLS`]).
pub const TOOL: u32 = 1;
/// Layer-view grid density and color. Applied only to the preview copy.
pub const VIEW_GRID: u32 = u32::MAX;
/// Parameter: brush size in pixels.
pub const BRUSH_SIZE: u32 = 3;
/// Parameter: brush pressure in percent.
pub const BRUSH_PRESSURE: u32 = 4;
/// Parameter: the Distortion Mesh ([`Mesh::to_value`]).
pub const MESH: u32 = 14;
/// Parameter: Distortion Mesh Offset.
pub const MESH_OFFSET: u32 = 15;
/// Parameter: Distortion Percentage.
pub const PERCENT: u32 = 16;

const LABEL: &str = "bae.effects.liquify";
const UNIFORM_SIZE: u32 = 80;
/// Mesh cell size in layer pixels.
pub const CELL: f64 = 8.0;

/// The brush tools, in AE's order (the Tool parameter, 1-based).
pub const TOOLS: &[&str] = &[
    "Warp",
    "Turbulence",
    "Twirl Clockwise",
    "Twirl Counterclockwise",
    "Pucker",
    "Bloat",
    "Shift Pixels",
    "Reflection",
    "Clone",
    "Reconstruction",
];

/// A grid of sample offsets over the layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    /// Layer-pixel position of vertex (0, 0).
    pub origin: DVec2,
    /// Distance between vertices (layer pixels).
    pub cell: f64,
    /// Vertices per row.
    pub cols: usize,
    /// Rows of vertices.
    pub rows: usize,
    /// Per vertex, row by row: where the output pixel reads, relative to it.
    pub offsets: Vec<DVec2>,
}

impl Mesh {
    /// An undistorted mesh covering `min .. min + size`.
    pub fn new(min: DVec2, size: DVec2) -> Mesh {
        let cols = (size.x / CELL).ceil() as usize + 1;
        let rows = (size.y / CELL).ceil() as usize + 1;
        Mesh {
            origin: min,
            cell: CELL,
            cols,
            rows,
            offsets: vec![DVec2::ZERO; cols * rows],
        }
    }

    /// The mesh a Distortion Mesh value holds (`None` when empty or broken).
    pub fn from_value(v: &Value) -> Option<Mesh> {
        let Value::FloatList(f) = v else {
            return None;
        };
        let (cols, rows) = (*f.first()? as usize, *f.get(1)? as usize);
        let body = f.get(5..)?;
        if cols == 0 || rows == 0 || body.len() != cols.checked_mul(rows)?.checked_mul(2)? {
            return None;
        }
        Some(Mesh {
            origin: DVec2::new(f[3], f[4]),
            cell: f[2].max(1.0),
            cols,
            rows,
            offsets: body
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| DVec2::new(c[0], c[1]))
                .collect(),
        })
    }

    /// The Distortion Mesh value: cols, rows, cell, origin, then the offsets.
    pub fn to_value(&self) -> Value {
        let mut f = vec![
            self.cols as f64,
            self.rows as f64,
            self.cell,
            self.origin.x,
            self.origin.y,
        ];
        f.extend(self.offsets.iter().flat_map(|o| [o.x, o.y]));
        Value::FloatList(f)
    }

    /// The largest offset (how far the effect reads beyond a pixel).
    pub fn reach(&self) -> f64 {
        self.offsets.iter().map(|o| o.length()).fold(0.0, f64::max)
    }
}

pub(crate) fn liquify() -> Builtin {
    let mut def = DefBuilder::new(MATCH_NAME, "Liquify", "Distort");
    def.push(TOOL, "Tools", "工具", ParamKind::Button, None);
    def.push(2, "Tool Options", "工具选项", ParamKind::GroupStart, None);
    let mut def = def
        .slider(
            BRUSH_SIZE,
            "Brush Size",
            "画笔大小",
            64.0,
            [1.0, 600.0],
            [1.0, 600.0],
            0,
            "pixels",
        )
        .slider(
            BRUSH_PRESSURE,
            "Brush Pressure",
            "画笔压力",
            50.0,
            [1.0, 100.0],
            [1.0, 100.0],
            0,
            "percent",
        );
    def.push(7, "Freeze Area Mask", "冻结区域蒙版", ParamKind::Path, None);
    let mut def = def
        .slider(
            5,
            "Turbulent Jitter",
            "湍流抖动",
            70.0,
            [1.0, 100.0],
            [1.0, 100.0],
            0,
            "percent",
        )
        .checkbox(17, "Clone Offset", "仿制位移", false)
        .popup(
            6,
            "Reconstruction Mode",
            "重建模式",
            1,
            &["Revert", "Displace", "Amplitwist", "Affine"],
        );
    def.push(8, "Tool Options", "工具选项", ParamKind::GroupEnd, None);
    def.push(9, "View Options", "视图选项", ParamKind::GroupStart, None);
    let mut def = def
        .checkbox(10, "View Mesh", "视图网格", false)
        .popup(
            11,
            "Mesh Size",
            "网格大小",
            2,
            &["Small", "Medium", "Large"],
        )
        .popup(
            12,
            "Mesh Color",
            "网格颜色",
            7,
            &["Red", "Yellow", "Green", "Cyan", "Blue", "Magenta", "Gray"],
        );
    def.push(13, "View Options", "视图选项", ParamKind::GroupEnd, None);
    def.push(
        MESH,
        "Distortion Mesh",
        "扭曲网格",
        ParamKind::CurvePoints,
        Some(Value::FloatList(Vec::new())),
    );
    def.push(
        MESH_OFFSET,
        "Distortion Mesh Offset",
        "扭曲网格位移",
        ParamKind::Point,
        Some(Value::Vec2([0.0, 0.0])),
    );
    let def = def.slider(
        PERCENT,
        "Distortion Percentage",
        "扭曲百分比",
        100.0,
        [0.0, 200.0],
        [0.0, 200.0],
        0,
        "percent",
    );
    let (mut def, names) = def.build();
    for p in &mut def.params {
        if !matches!(p.id, MESH | MESH_OFFSET | PERCENT) {
            p.flags.cannot_animate = true;
            p.flags.cannot_time_vary = true;
        }
    }
    crate::rust_builtin(
        (Arc::new(Liquify { def }), names),
        "liquify",
        "液化",
        &["liquify", "液化"],
    )
}

pub(crate) fn options_zh(id: u32) -> Option<&'static [&'static str]> {
    match id {
        6 => Some(&["恢复", "置换", "放大扭转", "仿射"]),
        11 => Some(&["小", "中", "大"]),
        12 => Some(&["红色", "黄色", "绿色", "青色", "蓝色", "洋红色", "灰色"]),
        _ => None,
    }
}

struct Liquify {
    def: EffectDef,
}

impl Liquify {
    fn mesh(&self, p: &EffectParams<'_>) -> Option<Mesh> {
        Params::new(&self.def, p)
            .value(MESH)
            .and_then(Mesh::from_value)
    }
}

impl Effect for Liquify {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        let v = Params::new(&self.def, p);
        let handles = brush::ticket(v.value(MESH));
        if handles.is_some() {
            let b = p.layer_bounds;
            let x = |n| f64::from(n) * p.scale;
            return roi.union(&IRect::new(
                x(b.x0).floor() as i32,
                x(b.y0).floor() as i32,
                x(b.x1).ceil() as i32,
                x(b.y1).ceil() as i32,
            ));
        }
        let Some(mesh) = self.mesh(p) else {
            return roi;
        };
        let pad = (mesh.reach() * v.f(PERCENT).max(0.0) / 100.0 * p.scale).ceil() as i32 + 2;
        IRect::new(roi.x0 - pad, roi.y0 - pad, roi.x1 + pad, roi.y1 + pad)
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, p, input, roi)
    }
}

impl TargetEffect for Liquify {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        let percent = v.f(PERCENT) / 100.0;
        if roi.is_empty() {
            return Ok(input.clone());
        };

        let grid = image_grid::grid_scale(p, input);
        let offset = match v.value(MESH_OFFSET) {
            Some(Value::Vec2(o)) => DVec2::from_array(*o),
            _ => DVec2::ZERO,
        };
        let rt = gpu::runtime(target.gpu());
        let pipeline = kernel(&rt, target.format())?;
        let ticket = brush::ticket(v.value(MESH));
        // Keep the upload texture leased through the dispatch on 32-bpc targets.
        let grid_image;
        let has_live_buffer = ticket.is_some();
        let data = if let Some(id) = ticket {
            brush::preview(&rt, id)
        } else if let Some(mesh) = self.mesh(p).filter(|m| m.reach() > 0.0 && percent != 0.) {
            let (w, h) = (mesh.cols as u32, mesh.rows as u32);
            grid_image = target.allocate_format(
                IRect::new(0, 0, w as i32, h as i32),
                1.,
                wgpu::TextureFormat::Rgba32Float,
            )?;
            let row_bytes = (w * 16).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
            {
                let mut belt = upload_belt(&rt).lock();
                let size = wgpu::BufferSize::new(u64::from(row_bytes) * u64::from(h)).unwrap();
                let slice = belt.allocate(
                    size,
                    wgpu::BufferSize::new(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.into()).unwrap(),
                );
                {
                    let mut data = slice
                        .get_mapped_range_mut()
                        .map_err(|err| RenderError::Other(err.to_string()))?;
                    let mut row = vec![0; mesh.cols * 16];
                    for (y, offsets) in mesh.offsets.chunks_exact(mesh.cols).enumerate() {
                        for (pixel, v) in row.as_chunks_mut::<16>().0.iter_mut().zip(offsets) {
                            pixel[..4].copy_from_slice(&(v.x as f32).to_le_bytes());
                            pixel[4..8].copy_from_slice(&(v.y as f32).to_le_bytes());
                        }
                        let at = y * row_bytes as usize;
                        data.slice(at..at + row.len()).copy_from_slice(&row);
                    }
                }
                target.encoder().copy_buffer_to_texture(
                    wgpu::TexelCopyBufferInfo {
                        buffer: slice.buffer(),
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: slice.offset(),
                            bytes_per_row: Some(row_bytes),
                            rows_per_image: None,
                        },
                    },
                    grid_image.texture.as_image_copy(),
                    wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                );
                belt.finish_and_recall_on_submit(target.encoder());
            }
            let mesh_view = grid_image.view.clone();
            Some((mesh_view, mesh.origin, mesh.cell, [w, h]))
        } else {
            None
        };
        let display = match v.value(VIEW_GRID) {
            Some(Value::Int(v)) => (*v).clamp(0, 70) as u32,
            _ => 0,
        };
        let percent = if has_live_buffer || data.is_some() {
            percent
        } else {
            0.
        };
        let (mesh_view, origin, cell, size) = match data {
            Some(grid) => grid,
            None if display >= 8 => {
                let b = p.layer_bounds;
                let size = [
                    ((b.x1 - b.x0).max(0) as u32).div_ceil(CELL as u32) + 1,
                    ((b.y1 - b.y0).max(0) as u32).div_ceil(CELL as u32) + 1,
                ];
                (
                    input.view.clone(),
                    DVec2::new(f64::from(b.x0), f64::from(b.y0)),
                    CELL,
                    size,
                )
            }
            None => return Ok(input.clone()),
        };
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let mut u = UniformWriter::with_size(UNIFORM_SIZE as usize);
        image_grid::image_coords(&mut u, 0, input, grid);
        // The shader works in render-grid pixels (`grid` layer pixels each).
        let origin = (origin + offset) / grid;
        u.i32s(32, &image_grid::rect4(roi))
            .f32(48, origin.x as f32)
            .f32(52, origin.y as f32)
            .f32(56, (cell / grid) as f32)
            .f32(60, (percent / grid) as f32)
            .u32(64, size[0])
            .u32(68, size[1])
            .u32(72, display / 8)
            .u32(76, display % 8);
        let at = gpu::push_uniform(&rt, &u);
        let group = rt.bind_group(
            &pipeline,
            &[
                Resource::Uniform,
                Resource::View(&input.view),
                Resource::View(&out.view),
                Resource::View(&mesh_view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(LABEL),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &pipeline, &group, &[at], roi.size());
        Ok(out)
    }
}

/// The transfer for saved meshes lives on the rendering device. Uploaded rows
/// and their consumer share an encoder. GPU completion recycles the upload pages.
fn upload_belt(rt: &gpu::Runtime) -> &parking_lot::Mutex<wgpu::util::StagingBelt> {
    rt.mesh_upload.get_or_init(|| {
        let size = 1 << 20;
        let mut belt = wgpu::util::StagingBelt::new(rt.device.clone(), size);
        belt.allocate(
            wgpu::BufferSize::new(size).unwrap(),
            wgpu::BufferSize::new(256).unwrap(),
        );
        let warmup = rt.device.create_command_encoder(&Default::default());
        belt.finish_and_recall_on_submit(&warmup);
        rt.queue.submit([warmup.finish()]);
        parking_lot::Mutex::new(belt)
    })
}

/// Compile the kernel ahead of first use (the effects' background warm-up).
pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    upload_belt(rt);
    brush::kernel(rt)?;
    kernel(rt, format).map(|_| ())
}

fn kernel(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<Arc<Kernel>, RenderError> {
    rt.kernel(
        &KernelKey {
            owner: LABEL.into(),
            entry: "main".into(),
            variant: gpu::format_key(format),
        },
        || {
            Ok(KernelSource {
                label: LABEL,
                wgsl: [
                    image_grid::CHANNEL_MATH,
                    include_str!("../shaders/image_grid.wgsl"),
                    include_str!("liquify.wgsl"),
                ]
                .concat()
                .replace("WORKING_FORMAT", crate::blur::format_name(format)?),
                entry: "main",
                bindings: vec![
                    (0, Binding::Uniform(UNIFORM_SIZE)),
                    (1, Binding::Texture),
                    (2, Binding::Storage(format)),
                    (3, Binding::Texture),
                ],
                workgroup: [8, 8, 1],
            })
        },
    )
}
