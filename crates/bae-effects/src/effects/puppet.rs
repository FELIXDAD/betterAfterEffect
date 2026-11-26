//! AE's Puppet (`ADBE FreePin3`), the effect the Puppet tools create: a
//! triangle mesh over the layer, pulled by its pins and kept as rigid as
//! possible (as-rigid-as-possible deformation, solved on the CPU), each output
//! pixel reading the layer where its triangle rested. Pixels outside the mesh
//! are transparent, as in AE.
//!
//! The pins are the effect's `PuppetPin` prims, packed into parameter 0 by the
//! comp conversion ([`bae_render::puppet`]). Position (and Bend / Advanced)
//! pins hold their point; Starch pins stiffen the mesh around them; Overlap
//! pins draw the mesh near them in front. Mesh Rotation Refinement is the
//! number of solver rounds. AE keeps Density and Expansion per mesh (Mesh 1);
//! here the effect has one mesh over the layer's bounds, a grid of Density's
//! triangle count grown by Expansion (AE traces the layer's outline: an
//! accepted difference for now).

use std::sync::{Arc, Mutex};

use bae_gpu::wgpu::{self, util::DeviceExt};
use bae_model::EffectDef;
use bae_render::puppet::{self, Pin, PinKind};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use glam::DVec2;

use super::image_grid;
use crate::gpu::{self, Binding, Kernel, KernelKey, KernelSource, Resource, UniformWriter};
use crate::{Builtin, EffectTarget, TargetEffect, catalog::DefBuilder, params::Params};

/// The effect's matchName.
pub const MATCH_NAME: &str = puppet::PUPPET_EFFECT;
/// Parameter: Puppet Engine (1 Legacy, 2 Advanced).
pub const ENGINE: u32 = 1;
/// Parameter: Mesh Rotation Refinement (1–20): solver rounds.
pub const REFINEMENT: u32 = 2;
/// Parameter: On Transparent.
pub const ON_TRANSPARENT: u32 = 3;
/// Parameter: the mesh's Expansion (pixels past the layer's bounds).
pub const EXPANSION: u32 = 4;
/// Parameter: the mesh's Density (35 triangles per unit).
pub const DENSITY: u32 = 5;

const LABEL: &str = "bae.effects.puppet";
const UNIFORM_SIZE: u32 = 64;
/// Tile side for binning triangles (render-grid pixels).
const TILE: u32 = 16;

pub(crate) fn puppet_effect() -> Builtin {
    let def = DefBuilder::new(MATCH_NAME, "Puppet", "Distort")
        .popup(
            ENGINE,
            "Puppet Engine",
            "人偶引擎",
            2,
            &["Legacy", "Advanced"],
        )
        .ae_match_name("ADBE FreePin3 Puppet Engine")
        .static_param()
        .slider(
            REFINEMENT,
            "Mesh Rotation Refinement",
            "网格旋转调整",
            20.0,
            [1.0, 20.0],
            [1.0, 20.0],
            0,
            "",
        )
        .ae_match_name("ADBE FreePin3 Auto Rotate Pins")
        .checkbox(ON_TRANSPARENT, "On Transparent", "在透明背景上", false)
        .ae_match_name("ADBE FreePin3 On Transparent")
        .static_param()
        .slider(
            EXPANSION,
            "Expansion",
            "扩展",
            3.0,
            [0.0, 100.0],
            [0.0, 100.0],
            0,
            "pixels",
        )
        .static_param()
        .slider(
            DENSITY,
            "Density",
            "密度",
            10.0,
            [1.0, 100.0],
            [1.0, 100.0],
            0,
            "",
        )
        .static_param();
    let (def, names) = def.build();
    crate::rust_builtin(
        (
            Arc::new(Puppet {
                def,
                cache: Mutex::default(),
            }),
            names,
        ),
        "puppet",
        "操控",
        &["puppet", "操控", "人偶"],
    )
}

pub(crate) fn options_zh(id: u32) -> Option<&'static [&'static str]> {
    match id {
        ENGINE => Some(&["旧版", "高级"]),
        _ => None,
    }
}

/// A triangle mesh over a layer (layer pixels).
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    /// Vertices where they rest.
    pub rest: Vec<DVec2>,
    /// Triangles as vertex indices.
    pub tris: Vec<[u32; 3]>,
    /// Each vertex's neighbours along mesh edges.
    neighbours: Vec<Vec<u32>>,
}

impl Mesh {
    /// A grid of about `35 × density` triangles over `min .. max`.
    pub fn grid(min: DVec2, max: DVec2, density: f64) -> Mesh {
        let size = (max - min).max(DVec2::ONE);
        let triangles = (35.0 * density.max(1.0)).min(20_000.0);
        let cell = (2.0 * size.x * size.y / triangles).sqrt().max(2.0);
        let cols = ((size.x / cell).ceil() as usize).clamp(1, 200);
        let rows = ((size.y / cell).ceil() as usize).clamp(1, 200);
        let step = size / DVec2::new(cols as f64, rows as f64);
        let mut rest = Vec::with_capacity((cols + 1) * (rows + 1));
        for j in 0..=rows {
            for i in 0..=cols {
                rest.push(min + DVec2::new(i as f64, j as f64) * step);
            }
        }
        let at = |i: usize, j: usize| (j * (cols + 1) + i) as u32;
        let mut tris = Vec::with_capacity(cols * rows * 2);
        for j in 0..rows {
            for i in 0..cols {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                // Alternate the diagonal so the mesh has no grain.
                if (i + j) % 2 == 0 {
                    tris.push([a, b, c]);
                    tris.push([a, c, d]);
                } else {
                    tris.push([a, b, d]);
                    tris.push([b, c, d]);
                }
            }
        }
        let mut neighbours = vec![Vec::new(); rest.len()];
        for t in &tris {
            for k in 0..3 {
                let (u, v) = (t[k], t[(k + 1) % 3]);
                if !neighbours[u as usize].contains(&v) {
                    neighbours[u as usize].push(v);
                    neighbours[v as usize].push(u);
                }
            }
        }
        Mesh {
            rest,
            tris,
            neighbours,
        }
    }

    /// The vertex closest to `p`.
    pub fn nearest(&self, p: DVec2) -> usize {
        self.rest
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.distance_squared(p).total_cmp(&b.1.distance_squared(p)))
            .map_or(0, |(i, _)| i)
    }
}

fn rotate(v: DVec2, (s, c): (f64, f64)) -> DVec2 {
    DVec2::new(c * v.x - s * v.y, s * v.x + c * v.y)
}

/// Where the mesh's vertices go with `pins` (layer pixels): each pulling pin
/// holds its nearest vertex at its Position, the rest of the mesh keeps its
/// shape as well as it can (`rounds` local / global rounds of
/// as-rigid-as-possible deformation); Starch pins make the mesh stiffer
/// within their Extent.
pub fn deform(mesh: &Mesh, pins: &[Pin], rounds: usize) -> Vec<DVec2> {
    let n = mesh.rest.len();
    let mut fixed: Vec<Option<(DVec2, f64)>> = vec![None; n];
    for pin in pins.iter().filter(|p| p.kind.pulls()) {
        let k = mesh.nearest(pin.rest);
        let target = pin.position + (mesh.rest[k] - pin.rest);
        let (sum, count) = fixed[k].unwrap_or((DVec2::ZERO, 0.0));
        fixed[k] = Some((sum + target, count + 1.0));
    }
    let fixed: Vec<Option<DVec2>> = fixed
        .into_iter()
        .map(|f| f.map(|(sum, count)| sum / count))
        .collect();
    if fixed.iter().all(Option::is_none) {
        return mesh.rest.clone();
    }
    // Edge weights: 1, stiffer near Starch pins.
    let stiffness = |a: DVec2, b: DVec2| {
        let mid = (a + b) / 2.0;
        pins.iter()
            .filter(|p| p.kind == PinKind::Starch && p.extent > 0.0)
            .filter(|p| p.rest.distance(mid) <= p.extent)
            .fold(1.0, |w, p| w * (1.0 + 9.0 * p.amount.max(0.0)))
    };
    let weights: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            mesh.neighbours[i]
                .iter()
                .map(|&j| stiffness(mesh.rest[i], mesh.rest[j as usize]))
                .collect()
        })
        .collect();
    let mut x: Vec<DVec2> = (0..n).map(|i| fixed[i].unwrap_or(mesh.rest[i])).collect();
    // One pin: the mesh moves with it, unrotated.
    let pulled: Vec<usize> = (0..n).filter(|&i| fixed[i].is_some()).collect();
    if let [k] = pulled[..] {
        let d = x[k] - mesh.rest[k];
        return mesh.rest.iter().map(|p| *p + d).collect();
    }
    let free: Vec<usize> = (0..n).filter(|&i| fixed[i].is_none()).collect();
    let mut index = vec![usize::MAX; n];
    for (u, &i) in free.iter().enumerate() {
        index[i] = u;
    }
    let p = &mesh.rest;
    let mut rot = vec![(0.0, 1.0); n];
    for round in 0..rounds.max(1) {
        // Local: each vertex's best rotation (none in the first round).
        if round > 0 {
            for i in 0..n {
                let (mut dot, mut cross) = (0.0, 0.0);
                for (k, &j) in mesh.neighbours[i].iter().enumerate() {
                    let j = j as usize;
                    let (ep, ex) = (p[i] - p[j], x[i] - x[j]);
                    dot += weights[i][k] * ep.dot(ex);
                    cross += weights[i][k] * (ep.x * ex.y - ep.y * ex.x);
                }
                let a = cross.atan2(dot);
                rot[i] = a.sin_cos();
            }
        }
        // Global: the positions that best follow those rotations.
        let mut b = vec![DVec2::ZERO; free.len()];
        for (u, &i) in free.iter().enumerate() {
            for (k, &j) in mesh.neighbours[i].iter().enumerate() {
                let j = j as usize;
                let w = weights[i][k];
                let e = p[i] - p[j];
                b[u] += w / 2.0 * (rotate(e, rot[i]) + rotate(e, rot[j]));
                if let Some(q) = fixed[j] {
                    b[u] += w * q;
                }
            }
        }
        let apply = |y: &[DVec2], out: &mut [DVec2]| {
            for (u, &i) in free.iter().enumerate() {
                let mut acc = DVec2::ZERO;
                for (k, &j) in mesh.neighbours[i].iter().enumerate() {
                    let w = weights[i][k];
                    acc += w * y[u];
                    let v = index[j as usize];
                    if v != usize::MAX {
                        acc -= w * y[v];
                    }
                }
                out[u] = acc;
            }
        };
        let mut y: Vec<DVec2> = free.iter().map(|&i| x[i]).collect();
        conjugate_gradient(apply, &b, &mut y, 400);
        for (u, &i) in free.iter().enumerate() {
            x[i] = y[u];
        }
    }
    x
}

/// Solves `A y = b` for a symmetric positive definite `A` (given as its
/// product), both coordinates at once, starting from `y`.
fn conjugate_gradient(
    apply: impl Fn(&[DVec2], &mut [DVec2]),
    b: &[DVec2],
    y: &mut [DVec2],
    max: usize,
) {
    let n = b.len();
    let dot = |a: &[DVec2], c: &[DVec2]| a.iter().zip(c).fold(DVec2::ZERO, |s, (a, c)| s + *a * *c);
    let mut ay = vec![DVec2::ZERO; n];
    apply(y, &mut ay);
    let mut r: Vec<DVec2> = b.iter().zip(&ay).map(|(b, a)| *b - *a).collect();
    let mut d = r.clone();
    let mut rs = dot(&r, &r);
    let limit = (dot(b, b).max_element() * 1e-14).max(1e-18);
    let mut ad = vec![DVec2::ZERO; n];
    for _ in 0..max {
        if rs.max_element() <= limit {
            break;
        }
        apply(&d, &mut ad);
        let dad = dot(&d, &ad);
        let alpha = DVec2::new(
            if dad.x.abs() > 1e-300 {
                rs.x / dad.x
            } else {
                0.0
            },
            if dad.y.abs() > 1e-300 {
                rs.y / dad.y
            } else {
                0.0
            },
        );
        for i in 0..n {
            y[i] += alpha * d[i];
            r[i] -= alpha * ad[i];
        }
        let next = dot(&r, &r);
        let beta = DVec2::new(
            if rs.x > 0.0 { next.x / rs.x } else { 0.0 },
            if rs.y > 0.0 { next.y / rs.y } else { 0.0 },
        );
        for i in 0..n {
            d[i] = r[i] + beta * d[i];
        }
        rs = next;
    }
}

/// The last mesh and its deformation, reused while nothing changes.
#[derive(Default)]
struct Cache {
    mesh: Option<(Vec<u64>, Arc<Mesh>)>,
    deformed: Option<(Vec<u64>, Arc<[DVec2]>)>,
}

struct Puppet {
    def: EffectDef,
    cache: Mutex<Cache>,
}

impl Puppet {
    /// The mesh and where its vertices go at this frame; `None` without pins.
    fn solve(&self, p: &EffectParams<'_>) -> Option<(Arc<Mesh>, Arc<[DVec2]>)> {
        let pins = p.effect.param(puppet::PINS_PARAM).map(puppet::unpack)?;
        if pins.is_empty() || p.layer_bounds.is_empty() {
            return None;
        }
        let v = Params::new(&self.def, p);
        let b = p.layer_bounds;
        let grow = v.f(EXPANSION).max(0.0);
        let density = v.f(DENSITY);
        let mesh_key = vec![
            b.x0 as u64,
            b.y0 as u64,
            b.x1 as u64,
            b.y1 as u64,
            grow.to_bits(),
            density.to_bits(),
        ];
        let rounds = v.f(REFINEMENT).round().clamp(1.0, 20.0) as usize;
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let mesh = match &cache.mesh {
            Some((key, mesh)) if *key == mesh_key => mesh.clone(),
            _ => {
                let min = DVec2::new(f64::from(b.x0), f64::from(b.y0)) - grow;
                let max = DVec2::new(f64::from(b.x1), f64::from(b.y1)) + grow;
                let mesh = Arc::new(Mesh::grid(min, max, density));
                cache.mesh = Some((mesh_key.clone(), mesh.clone()));
                mesh
            }
        };
        let mut key = mesh_key;
        key.push(rounds as u64);
        if let bae_model::Value::FloatList(f) = puppet::pack(&pins) {
            key.extend(f.iter().map(|v| v.to_bits()));
        }
        let deformed = match &cache.deformed {
            Some((k, d)) if *k == key => d.clone(),
            _ => {
                let d: Arc<[DVec2]> = deform(&mesh, &pins, rounds).into();
                cache.deformed = Some((key, d.clone()));
                d
            }
        };
        Some((mesh, deformed))
    }
}

/// `points`' bounds at `scale`, rounded out.
fn bounds(points: &[DVec2], scale: f64) -> IRect {
    let (lo, hi) = points.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    if !lo.is_finite() {
        return IRect::default();
    }
    let (lo, hi) = (lo * scale, hi * scale);
    IRect::new(
        lo.x.floor() as i32,
        lo.y.floor() as i32,
        hi.x.ceil() as i32 + 1,
        hi.y.ceil() as i32 + 1,
    )
}

impl Effect for Puppet {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn output_bounds(&self, p: &EffectParams<'_>, input: IRect) -> IRect {
        match self.solve(p) {
            Some((_, deformed)) => bounds(&deformed, p.scale),
            None => input,
        }
    }

    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        match self.solve(p) {
            Some((mesh, _)) => bounds(&mesh.rest, p.scale),
            None => roi,
        }
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

impl TargetEffect for Puppet {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let Some((mesh, deformed)) = self.solve(p) else {
            return Ok(input.clone());
        };
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let pins = p
            .effect
            .param(puppet::PINS_PARAM)
            .map(puppet::unpack)
            .unwrap_or_default();
        let grid = image_grid::grid_scale(p, input);
        let [columns, rows] = roi.size().map(|v| v.div_ceil(TILE));
        let mut tiles = vec![Vec::new(); (columns * rows) as usize];
        let origin = DVec2::new(f64::from(roi.x0), f64::from(roi.y0));
        let mut data: Vec<u8> = Vec::with_capacity(mesh.tris.len() * 64);
        for (index, t) in mesh.tris.iter().enumerate() {
            let at = t.map(|v| deformed[v as usize] / grid);
            let from = t.map(|v| mesh.rest[v as usize] / grid);
            // Overlap pins: the mesh near them draws in front.
            let centre = t.iter().map(|&v| mesh.rest[v as usize]).sum::<DVec2>() / 3.0;
            let front = pins
                .iter()
                .filter(|p| p.kind == PinKind::Overlap && p.rest.distance(centre) <= p.extent)
                .map(|p| p.in_front)
                .fold(0.0f64, f64::max);
            for k in 0..3 {
                data.extend(
                    [at[k].x, at[k].y, from[k].x, from[k].y]
                        .map(|v| v as f32)
                        .into_iter()
                        .flat_map(f32::to_le_bytes),
                );
            }
            data.extend(
                [front as f32, 0.0, 0.0, 0.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes),
            );
            let (lo, hi) = at.iter().fold(
                (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
                |(lo, hi), q| (lo.min(*q), hi.max(*q)),
            );
            let tile = f64::from(TILE);
            let lo = ((lo - origin - 1.0) / tile)
                .floor()
                .as_ivec2()
                .max(glam::IVec2::ZERO);
            let hi = ((hi - origin + 1.0) / tile)
                .floor()
                .as_ivec2()
                .min(glam::IVec2::new(columns as i32 - 1, rows as i32 - 1));
            for y in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    tiles[(y as u32 * columns + x as u32) as usize].push(index as u32);
                }
            }
        }
        let mut bins = vec![0u32; tiles.len() * 2];
        for (i, tile) in tiles.into_iter().enumerate() {
            bins[2 * i] = bins.len() as u32;
            bins[2 * i + 1] = tile.len() as u32;
            bins.extend(tile);
        }
        let bins: Vec<u8> = bins.into_iter().flat_map(u32::to_le_bytes).collect();
        let rt = gpu::runtime(target.gpu());
        let pipeline = kernel(&rt, target.format())?;
        let buffer = |data: &[u8]| -> Result<_, RenderError> {
            if data.len() as u64 > rt.device.limits().max_storage_buffer_binding_size {
                return Err(RenderError::Other(
                    "puppet mesh exceeds the GPU buffer limit".into(),
                ));
            }
            Ok(rt
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(LABEL),
                    contents: data,
                    usage: wgpu::BufferUsages::STORAGE,
                }))
        };
        let tris = buffer(&data)?;
        let bins = buffer(&bins)?;
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let mut u = UniformWriter::with_size(UNIFORM_SIZE as usize);
        image_grid::image_coords(&mut u, 0, input, grid);
        u.i32s(32, &image_grid::rect4(roi))
            .u32(48, columns)
            .u32(52, mesh.tris.len() as u32);
        let offset = gpu::push_uniform(&rt, &u);
        let group = rt.bind_group(
            &pipeline,
            &[
                Resource::Uniform,
                Resource::View(&input.view),
                Resource::View(&out.view),
                Resource::Buffer(&tris),
                Resource::Buffer(&bins),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(LABEL),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &pipeline, &group, &[offset], roi.size());
        Ok(out)
    }
}

/// Compile the kernel ahead of first use (the effects' background warm-up).
pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
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
                    include_str!("puppet.wgsl"),
                ]
                .concat()
                .replace("WORKING_FORMAT", crate::blur::format_name(format)?),
                entry: "main",
                bindings: vec![
                    (0, Binding::Uniform(UNIFORM_SIZE)),
                    (1, Binding::Texture),
                    (2, Binding::Storage(format)),
                    (3, Binding::ReadBuffer),
                    (4, Binding::ReadBuffer),
                ],
                workgroup: [8, 8, 1],
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(kind: PinKind, rest: DVec2, position: DVec2) -> Pin {
        Pin {
            kind,
            rest,
            position,
            scale: 100.0,
            rotation: 0.0,
            amount: 0.3,
            extent: 15.0,
            in_front: 0.0,
        }
    }

    #[test]
    fn one_pin_moves_the_mesh_and_two_pins_stretch_it_rigidly() {
        let mesh = Mesh::grid(DVec2::ZERO, DVec2::new(100.0, 40.0), 10.0);
        assert!(mesh.tris.len() >= 300, "{}", mesh.tris.len());
        // Nothing pulled: unchanged.
        let still = deform(&mesh, &[pin(PinKind::Starch, DVec2::ZERO, DVec2::ONE)], 5);
        assert_eq!(still, mesh.rest);
        // One pin: a translation.
        let one = [pin(
            PinKind::Position,
            DVec2::new(50.0, 20.0),
            DVec2::new(60.0, 25.0),
        )];
        let moved = deform(&mesh, &one, 5);
        let d = moved[0] - mesh.rest[0];
        assert!((d - DVec2::new(10.0, 5.0)).length() < 1.0, "{d}");
        // Two pins turned a quarter round about the left one: the far end
        // follows and the middle stays between them (rigid, not squashed).
        let left = DVec2::new(mesh.rest[mesh.nearest(DVec2::new(0.0, 20.0))].x, 20.0);
        let right = DVec2::new(100.0, 20.0);
        let pins = [
            pin(PinKind::Position, left, left),
            pin(
                PinKind::Position,
                right,
                left + DVec2::new(0.0, 100.0 - left.x),
            ),
        ];
        let turned = deform(&mesh, &pins, 20);
        let mid = mesh.nearest(DVec2::new(50.0, 20.0));
        let m = turned[mid];
        assert!(
            (m.x - left.x).abs() < 8.0,
            "middle near the turned axis: {m}"
        );
        assert!(m.y > 35.0 && m.y < 85.0, "middle between the pins: {m}");
    }
}
