//! AE's Liquify (`ADBE Liquify`, Distort): brushes painted in the Layer panel
//! push, twirl, pucker and bloat the layer. The result is the Distortion Mesh
//! parameter, a grid of sample offsets that keyframes like any value: each
//! output pixel reads the layer at its position plus the mesh's offset there,
//! scaled by Distortion Percentage and shifted by Distortion Mesh Offset.

use std::sync::Arc;

use bae_gpu::wgpu::{self, util::DeviceExt};
use bae_model::{EffectDef, ParamKind, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use glam::DVec2;

use super::image_grid;
use crate::gpu::{self, Binding, Kernel, KernelKey, KernelSource, Resource, UniformWriter};
use crate::{Builtin, EffectTarget, TargetEffect, catalog::DefBuilder, params::Params};

/// The effect's matchName.
pub const MATCH_NAME: &str = "ADBE Liquify";
/// Parameter: the brush tool (1-based index into [`TOOLS`]).
pub const TOOL: u32 = 1;
/// Parameter: brush size in pixels.
pub const BRUSH_SIZE: u32 = 2;
/// Parameter: brush pressure in percent.
pub const BRUSH_PRESSURE: u32 = 3;
/// Parameter: the Distortion Mesh ([`Mesh::to_value`]).
pub const MESH: u32 = 10;
/// Parameter: Distortion Mesh Offset.
pub const MESH_OFFSET: u32 = 11;
/// Parameter: Distortion Percentage.
pub const PERCENT: u32 = 12;

const LABEL: &str = "bae.effects.liquify";
const UNIFORM_SIZE: u32 = 80;
/// Mesh cell size in layer pixels.
pub const CELL: f64 = 16.0;

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
        if cols == 0 || rows == 0 || body.len() != cols * rows * 2 {
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

    fn vertex(&self, i: usize, j: usize) -> DVec2 {
        self.origin + DVec2::new(i as f64, j as f64) * self.cell
    }

    /// The largest offset (how far the effect reads beyond a pixel).
    pub fn reach(&self) -> f64 {
        self.offsets.iter().map(|o| o.length()).fold(0.0, f64::max)
    }

    /// One application of `tool` (index into [`TOOLS`]) with a brush of
    /// `radius` at `center`; `pressure` 0–1; `drag` the pointer's move since
    /// the last application (Warp, Shift Pixels).
    pub fn brush(&mut self, tool: usize, center: DVec2, radius: f64, pressure: f64, drag: DVec2) {
        let r = radius.max(1.0);
        for j in 0..self.rows {
            for i in 0..self.cols {
                let q = self.vertex(i, j);
                let d = q.distance(center) / r;
                if d >= 1.0 {
                    continue;
                }
                let f = (1.0 - d * d).powi(2) * pressure;
                let o = &mut self.offsets[j * self.cols + i];
                let from_center = q - center;
                match TOOLS.get(tool).copied().unwrap_or("Warp") {
                    "Warp" => *o -= drag * f,
                    "Shift Pixels" => *o -= drag.perp() * f,
                    "Twirl Clockwise" | "Twirl Counterclockwise" => {
                        let a = if tool == 2 { -0.08 } else { 0.08 } * f;
                        *o += DVec2::from_angle(a).rotate(from_center) - from_center;
                    }
                    "Pucker" => *o += from_center * 0.06 * f,
                    "Bloat" => *o -= from_center * 0.06 * f,
                    "Reconstruction" => *o *= 1.0 - 0.2 * f,
                    "Turbulence" => {
                        let h = ((i * 73_856_093) ^ (j * 19_349_663)) as f64;
                        *o += DVec2::new((h * 0.001).sin(), (h * 0.0013).cos()) * r * 0.02 * f;
                    }
                    _ => {}
                }
            }
        }
    }
}

pub(crate) fn liquify() -> Builtin {
    let mut def = DefBuilder::new(MATCH_NAME, "Liquify", "Distort")
        .popup(TOOL, "Tool", "工具", 1, TOOLS)
        .slider(
            BRUSH_SIZE,
            "Brush Size",
            "画笔大小",
            64.0,
            [1.0, 1500.0],
            [1.0, 1500.0],
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
        )
        .slider(
            4,
            "Turbulent Jitter",
            "湍流抖动",
            70.0,
            [1.0, 100.0],
            [1.0, 100.0],
            0,
            "percent",
        )
        .popup(
            5,
            "Reconstruction Mode",
            "重建模式",
            1,
            &["Revert", "Displace", "Amplitwist", "Affine"],
        )
        .checkbox(6, "View Mesh", "视图网格", false)
        .popup(7, "Mesh Size", "网格大小", 2, &["Small", "Medium", "Large"])
        .popup(
            8,
            "Mesh Color",
            "网格颜色",
            1,
            &["Red", "Yellow", "Green", "Cyan", "Blue", "Magenta", "Gray"],
        );
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
    let (def, names) = def.build();
    crate::rust_builtin(
        (Arc::new(Liquify { def }), names),
        "liquify",
        "液化",
        &["liquify", "液化"],
    )
}

pub(crate) fn options_zh(id: u32) -> Option<&'static [&'static str]> {
    match id {
        TOOL => Some(&[
            "变形",
            "湍流",
            "顺时针旋转扭曲",
            "逆时针旋转扭曲",
            "凹陷",
            "膨胀",
            "转移像素",
            "反射",
            "仿制",
            "重建",
        ]),
        5 => Some(&["恢复", "置换", "放大扭转", "仿射"]),
        7 => Some(&["小", "中", "大"]),
        8 => Some(&["红色", "黄色", "绿色", "青色", "蓝色", "洋红色", "灰色"]),
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
        let Some(mesh) = self.mesh(p).filter(|m| m.reach() > 0.0 && percent != 0.0) else {
            return Ok(input.clone());
        };
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let grid = image_grid::grid_scale(p, input);
        let offset = match v.value(MESH_OFFSET) {
            Some(Value::Vec2(o)) => DVec2::from_array(*o),
            _ => DVec2::ZERO,
        };
        let rt = gpu::runtime(target.gpu());
        let pipeline = kernel(&rt, target.format())?;
        // The offsets as a cols × rows float texture.
        let limit = rt.device.limits().max_texture_dimension_2d;
        let (w, h) = (mesh.cols as u32, mesh.rows as u32);
        if w.max(h) > limit {
            return Err(RenderError::TooLarge {
                width: w,
                height: h,
                limit,
            });
        }
        let data: Vec<u8> = mesh
            .offsets
            .iter()
            .flat_map(|o| [o.x as f32, o.y as f32, 0.0, 0.0])
            .flat_map(f32::to_le_bytes)
            .collect();
        let texture = rt.device.create_texture_with_data(
            &rt.queue,
            &wgpu::TextureDescriptor {
                label: Some(LABEL),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &data,
        );
        let mesh_view = texture.create_view(&Default::default());
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let mut u = UniformWriter::with_size(UNIFORM_SIZE as usize);
        image_grid::image_coords(&mut u, 0, input, grid);
        // The shader works in render-grid pixels (`grid` layer pixels each).
        let origin = (mesh.origin + offset) / grid;
        u.i32s(32, &image_grid::rect4(roi))
            .f32(48, origin.x as f32)
            .f32(52, origin.y as f32)
            .f32(56, (mesh.cell / grid) as f32)
            .f32(60, (percent / grid) as f32)
            .u32(64, mesh.cols as u32)
            .u32(68, mesh.rows as u32);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meshes_round_trip_and_brushes_move_them() {
        let mut m = Mesh::new(DVec2::ZERO, DVec2::new(64.0, 32.0));
        assert_eq!((m.cols, m.rows), (5, 3));
        // Warp to the right: pixels there read from the left.
        m.brush(0, DVec2::new(32.0, 16.0), 20.0, 1.0, DVec2::new(5.0, 0.0));
        assert!(m.offsets[m.cols + 2].x < -4.0);
        assert_eq!(m.offsets[0], DVec2::ZERO, "outside the brush");
        assert_eq!(Mesh::from_value(&m.to_value()), Some(m.clone()));
        // Reconstruction brings it back.
        for _ in 0..60 {
            m.brush(9, DVec2::new(32.0, 16.0), 40.0, 1.0, DVec2::ZERO);
        }
        assert!(m.reach() < 0.1);
    }
}
