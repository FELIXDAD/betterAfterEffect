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
//! here they are effect parameters.
//!
//! The mesh covers where the layer is opaque (AE's auto-traced shapes): the
//! first render without a traced outline reads the layer's alpha back once,
//! asynchronously and at mesh precision ([`take_trace`]); the viewer stores it
//! in Auto-traced Shapes (`puppet.set_mesh`, AE's Create Puppet Mesh), and
//! from then on the mesh is built from it, the same in every render.
//! Separate parts get separate meshes; a part with no pin stays still. Until
//! the outline is stored the mesh is a grid over the layer's bounds.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use bae_gpu::wgpu::{self, util::DeviceExt};
use bae_model::{EffectDef, ParamKind, PrimPath, Value};
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
/// Parameter: Auto-traced Shapes, the layer's traced alpha ([`Mask::to_value`]).
pub const OUTLINES: u32 = 6;

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
    let mut def = def;
    def.push(
        OUTLINES,
        "Auto-traced Shapes",
        "自动追踪形状",
        ParamKind::CurvePoints,
        Some(Value::FloatList(Vec::new())),
    );
    let mut def = def.ae_match_name("ADBE FreePin3 Outlines").static_param();
    def.hide_last();
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

/// Where a layer is opaque, on a grid of cells (layer pixels): its alpha
/// traced at mesh precision, grown by the mesh's Expansion.
#[derive(Clone, Debug, PartialEq)]
pub struct Mask {
    /// Layer-pixel corner of cell (0, 0).
    pub origin: DVec2,
    /// Cell side (layer pixels).
    pub step: f64,
    /// Cells per row.
    pub cols: usize,
    /// Rows of cells.
    pub rows: usize,
    /// Per cell, row by row: inside the shape.
    pub on: Vec<bool>,
}

impl Mask {
    /// Cells whose most opaque pixel shows (alpha above 1/32), grown by
    /// `grow` pixels.
    pub fn from_alpha(
        origin: DVec2,
        step: f64,
        cols: usize,
        rows: usize,
        alpha: &[f32],
        grow: f64,
    ) -> Mask {
        let solid: Vec<bool> = (0..cols * rows)
            .map(|i| alpha.get(i).is_some_and(|a| *a > 1.0 / 32.0))
            .collect();
        let r = (grow / step).ceil().max(0.0) as isize;
        let mut on = solid.clone();
        if r > 0 {
            for y in 0..rows as isize {
                for x in 0..cols as isize {
                    if solid[(y * cols as isize + x) as usize] {
                        continue;
                    }
                    let hit = (-r..=r).any(|dy| {
                        (-r..=r).any(|dx| {
                            let (u, v) = (x + dx, y + dy);
                            dx * dx + dy * dy <= r * r
                                && u >= 0
                                && v >= 0
                                && u < cols as isize
                                && v < rows as isize
                                && solid[(v * cols as isize + u) as usize]
                        })
                    });
                    on[(y * cols as isize + x) as usize] = hit;
                }
            }
        }
        Mask {
            origin,
            step,
            cols,
            rows,
            on,
        }
    }

    /// Whether `p` (layer pixels) is inside; outside the grid counts as out
    /// unless the expansion reaches past the layer (the nearest cell).
    pub fn at(&self, p: DVec2) -> bool {
        let c = ((p - self.origin) / self.step).floor();
        let x = (c.x as isize).clamp(0, self.cols as isize - 1) as usize;
        let y = (c.y as isize).clamp(0, self.rows as isize - 1) as usize;
        self.on.get(y * self.cols + x).copied().unwrap_or(false)
    }

    /// Area inside (square pixels).
    pub fn area(&self) -> f64 {
        self.on.iter().filter(|b| **b).count() as f64 * self.step * self.step
    }

    /// Auto-traced Shapes: cols, rows, step, origin, then the cells as run
    /// lengths, alternately out and in (starting out).
    pub fn to_value(&self) -> Value {
        let mut f = vec![
            self.cols as f64,
            self.rows as f64,
            self.step,
            self.origin.x,
            self.origin.y,
        ];
        let (mut current, mut run) = (false, 0.0);
        for &b in &self.on {
            if b == current {
                run += 1.0;
            } else {
                f.push(run);
                current = b;
                run = 1.0;
            }
        }
        f.push(run);
        Value::FloatList(f)
    }

    /// The mask a value from [`Mask::to_value`] holds (`None` when empty).
    pub fn from_value(v: &Value) -> Option<Mask> {
        let Value::FloatList(f) = v else {
            return None;
        };
        let (cols, rows) = (*f.first()? as usize, *f.get(1)? as usize);
        let (step, origin) = (*f.get(2)?, DVec2::new(*f.get(3)?, *f.get(4)?));
        if cols == 0 || rows == 0 || cols * rows > 1 << 20 || step <= 0.0 {
            return None;
        }
        let mut on = Vec::with_capacity(cols * rows);
        let mut inside = false;
        for run in f.get(5..)? {
            on.extend(std::iter::repeat_n(inside, *run as usize));
            inside = !inside;
        }
        on.resize(cols * rows, false);
        Some(Mask {
            origin,
            step,
            cols,
            rows,
            on,
        })
    }

    /// The mask grown by `grow` pixels.
    pub fn grown(&self, grow: f64) -> Mask {
        let alpha: Vec<f32> = self.on.iter().map(|b| f32::from(u8::from(*b))).collect();
        Mask::from_alpha(self.origin, self.step, self.cols, self.rows, &alpha, grow)
    }
}

/// The Puppet mesh of a layer whose content spans `bounds` (layer pixels):
/// over the stored outline (`outlines`, grown by `expansion`) when there is
/// one, else a grid over the bounds grown by `expansion`. The renderer and the
/// viewer's Show Mesh both build it here.
pub fn mesh_of(outlines: Option<&Value>, bounds: IRect, expansion: f64, density: f64) -> Mesh {
    let grow = expansion.max(0.0);
    let min = DVec2::new(f64::from(bounds.x0), f64::from(bounds.y0)) - grow;
    let max = DVec2::new(f64::from(bounds.x1), f64::from(bounds.y1)) + grow;
    match outlines.and_then(Mask::from_value) {
        Some(mask) => Mesh::traced(min, max, density, &mask.grown(grow)),
        None => Mesh::grid(min, max, density),
    }
}

/// A layer's alpha being read back for its outline.
enum Trace {
    /// Copied to `buffer` (cells of `step` pixels from `origin`) at `since`;
    /// `state` follows its mapping.
    Pending {
        bounds: IRect,
        device: wgpu::Device,
        buffer: wgpu::Buffer,
        origin: DVec2,
        step: f64,
        cols: usize,
        rows: usize,
        since: Instant,
        state: Arc<AtomicU8>,
    },
    /// Read back, waiting for [`take_trace`].
    Ready { bounds: IRect, outlines: Value },
    /// Taken: not read again for these bounds.
    Taken { bounds: IRect },
}

const COPIED: u8 = 0;
const MAPPING: u8 = 1;
const MAPPED: u8 = 2;
const FAILED: u8 = 3;

/// Outlines being traced, per Puppet effect.
static TRACES: LazyLock<Mutex<HashMap<PrimPath, Trace>>> = LazyLock::new(Mutex::default);

/// Moves a pending read-back along without waiting: maps it once its frame
/// is surely submitted, polls, reads it once mapped. `true` when it must
/// start again (failed or lost).
fn advance(trace: &mut Trace) -> bool {
    let Trace::Pending {
        bounds,
        device,
        buffer,
        origin,
        step,
        cols,
        rows,
        since,
        state,
    } = trace
    else {
        return false;
    };
    if state.load(Ordering::Acquire) == COPIED {
        if since.elapsed() < TRACE_SETTLE {
            return false;
        }
        state.store(MAPPING, Ordering::Release);
        let done = state.clone();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            done.store(if r.is_ok() { MAPPED } else { FAILED }, Ordering::Release);
        });
    }
    if state.load(Ordering::Acquire) == MAPPING {
        let _ = device.poll(wgpu::PollType::Poll);
    }
    match state.load(Ordering::Acquire) {
        MAPPING => since.elapsed() > Duration::from_secs(10),
        MAPPED => {
            let alpha: Option<Vec<f32>> = buffer.slice(..).get_mapped_range().ok().map(|d| {
                d.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| f32::from_le_bytes(*c))
                    .collect()
            });
            buffer.unmap();
            let Some(alpha) = alpha else {
                return true;
            };
            let mask = Mask::from_alpha(*origin, *step, *cols, *rows, &alpha, 0.0);
            *trace = Trace::Ready {
                bounds: *bounds,
                outlines: mask.to_value(),
            };
            false
        }
        FAILED => true,
        _ => false,
    }
}

/// Whether an outline is still being read back (the viewer keeps drawing
/// until it lands).
pub fn tracing() -> bool {
    TRACES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .any(|t| matches!(t, Trace::Pending { .. }))
}

/// The outline traced for the Puppet effect `effect` since it was last
/// taken: the value for Auto-traced Shapes (the viewer stores it, asking
/// every frame, which also moves the read-back along).
pub fn take_trace(effect: &PrimPath) -> Option<Value> {
    let mut traces = TRACES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(t) = traces.get_mut(effect)
        && advance(t)
    {
        traces.remove(effect);
    }
    let bounds = match traces.get(effect) {
        Some(Trace::Ready { bounds, .. }) => *bounds,
        _ => return None,
    };
    match traces.insert(effect.clone(), Trace::Taken { bounds }) {
        Some(Trace::Ready { outlines, .. }) => Some(outlines),
        _ => None,
    }
}

impl Mesh {
    /// A grid of about `35 × density` triangles over `min .. max`.
    pub fn grid(min: DVec2, max: DVec2, density: f64) -> Mesh {
        let size = (max - min).max(DVec2::ONE);
        Mesh::build(min, max, cell_for(size.x * size.y, density), |_| true)
    }

    /// The mesh over the shape `mask` traces (AE's auto-traced outline):
    /// about `35 × density` triangles over its area, triangles kept where
    /// the shape is, so separate parts get separate meshes. A layer with
    /// nothing opaque gets the whole grid.
    pub fn traced(min: DVec2, max: DVec2, density: f64, mask: &Mask) -> Mesh {
        let area = mask.area();
        if area <= 0.0 {
            return Mesh::grid(min, max, density);
        }
        let mut mesh = Mesh::build(min, max, cell_for(area, density), |t| {
            let centre = (t[0] + t[1] + t[2]) / 3.0;
            mask.at(centre) || t.iter().any(|v| mask.at(*v))
        });
        mesh.hug(mask);
        mesh
    }

    /// Pulls the outline vertices that lie off `mask` onto its edge, so the
    /// mesh follows the traced shape instead of the grid's staircase.
    fn hug(&mut self, mask: &Mask) {
        let mut uses: HashMap<(u32, u32), u8> = HashMap::new();
        for t in &self.tris {
            for k in 0..3 {
                let (u, v) = (t[k], t[(k + 1) % 3]);
                *uses.entry((u.min(v), u.max(v))).or_default() += 1;
            }
        }
        let mut outline = vec![false; self.rest.len()];
        for ((u, v), n) in uses {
            if n == 1 {
                outline[u as usize] = true;
                outline[v as usize] = true;
            }
        }
        let rest = self.rest.clone();
        for (i, on) in outline.into_iter().enumerate() {
            let p = rest[i];
            if !on || mask.at(p) {
                continue;
            }
            // Towards the middle of its neighbours inside the shape.
            let inside: Vec<DVec2> = self.neighbours[i]
                .iter()
                .map(|n| rest[*n as usize])
                .filter(|q| mask.at(*q))
                .collect();
            if inside.is_empty() {
                continue;
            }
            let target = inside.iter().copied().sum::<DVec2>() / inside.len() as f64;
            if !mask.at(target) {
                continue;
            }
            let (mut out, mut ins) = (p, target);
            for _ in 0..10 {
                let m = (out + ins) / 2.0;
                if mask.at(m) {
                    ins = m;
                } else {
                    out = m;
                }
            }
            self.rest[i] = ins;
        }
    }

    /// Grid triangles of side `cell` over `min .. max` that `keep` accepts,
    /// with only the vertices they use.
    fn build(min: DVec2, max: DVec2, cell: f64, keep: impl Fn([DVec2; 3]) -> bool) -> Mesh {
        let size = (max - min).max(DVec2::ONE);
        let cols = ((size.x / cell).ceil() as usize).clamp(1, 200);
        let rows = ((size.y / cell).ceil() as usize).clamp(1, 200);
        let step = size / DVec2::new(cols as f64, rows as f64);
        let grid: Vec<DVec2> = (0..=rows)
            .flat_map(|j| (0..=cols).map(move |i| (i, j)))
            .map(|(i, j)| min + DVec2::new(i as f64, j as f64) * step)
            .collect();
        let at = |i: usize, j: usize| j * (cols + 1) + i;
        let mut index = vec![u32::MAX; grid.len()];
        let mut rest = Vec::new();
        let mut tris = Vec::with_capacity(cols * rows * 2);
        for j in 0..rows {
            for i in 0..cols {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                // Alternate the diagonal so the mesh has no grain.
                let pair = if (i + j) % 2 == 0 {
                    [[a, b, c], [a, c, d]]
                } else {
                    [[a, b, d], [b, c, d]]
                };
                for t in pair {
                    if !keep(t.map(|v| grid[v])) {
                        continue;
                    }
                    tris.push(t.map(|v| {
                        if index[v] == u32::MAX {
                            index[v] = rest.len() as u32;
                            rest.push(grid[v]);
                        }
                        index[v]
                    }));
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

/// The cell side giving about `35 × density` triangles over `area`.
fn cell_for(area: f64, density: f64) -> f64 {
    let triangles = (35.0 * density.max(1.0)).min(20_000.0);
    (2.0 * area.max(1.0) / triangles).sqrt().max(2.0)
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
    // Bend and Advanced pins: their Rotation and Scale turn the mesh around
    // them (the vertex and its neighbours), as a scaled rotation.
    let mut turns: Vec<Option<(f64, f64)>> = vec![None; n];
    for pin in pins
        .iter()
        .filter(|p| matches!(p.kind, PinKind::Bend | PinKind::Advanced))
        .filter(|p| p.rotation != 0.0 || p.scale != 100.0)
    {
        let k = mesh.nearest(pin.rest);
        let (sin, cos) = pin.rotation.to_radians().sin_cos();
        let scale = (pin.scale / 100.0).max(0.01);
        for v in std::iter::once(k as u32).chain(mesh.neighbours[k].iter().copied()) {
            turns[v as usize] = Some((scale * sin, scale * cos));
        }
    }
    // Parts of the mesh no pin reaches stay where they rest (AE: each traced
    // shape is its own mesh).
    let pulled: Vec<usize> = (0..n).filter(|&i| fixed[i].is_some()).collect();
    let mut reached = vec![false; n];
    let mut stack = pulled.clone();
    for &i in &stack {
        reached[i] = true;
    }
    while let Some(i) = stack.pop() {
        for &j in &mesh.neighbours[i] {
            if !reached[j as usize] {
                reached[j as usize] = true;
                stack.push(j as usize);
            }
        }
    }
    // One pin: its part moves with it, unrotated.
    if let [k] = pulled[..]
        && turns.iter().all(Option::is_none)
    {
        let d = x[k] - mesh.rest[k];
        return (0..n)
            .map(|i| mesh.rest[i] + if reached[i] { d } else { DVec2::ZERO })
            .collect();
    }
    let free: Vec<usize> = (0..n)
        .filter(|&i| fixed[i].is_none() && reached[i])
        .collect();
    let mut index = vec![usize::MAX; n];
    for (u, &i) in free.iter().enumerate() {
        index[i] = u;
    }
    let p = &mesh.rest;
    // The first round starts from the turns spread over the mesh (inverse
    // square distance; the other pulling pins hold theirs at none), so a
    // large turn does not have to crawl out vertex by vertex.
    let sources: Vec<(DVec2, f64, f64)> = pins
        .iter()
        .filter(|p| p.kind.pulls() || matches!(p.kind, PinKind::Bend | PinKind::Advanced))
        .map(|p| {
            let turned = matches!(p.kind, PinKind::Bend | PinKind::Advanced);
            let (angle, scale) = if turned {
                (p.rotation.to_radians(), (p.scale / 100.0).max(0.01))
            } else {
                (0.0, 1.0)
            };
            (p.rest, angle, scale)
        })
        .collect();
    let any_turn = turns.iter().any(Option::is_some);
    let mut rot: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            if let Some(t) = turns[i] {
                return t;
            }
            if !any_turn {
                return (0.0, 1.0);
            }
            let (mut w_sum, mut angle, mut scale) = (0.0, 0.0, 0.0);
            for (at, a, s) in &sources {
                let w = 1.0 / mesh.rest[i].distance_squared(*at).max(1e-6);
                w_sum += w;
                angle += w * a;
                scale += w * s;
            }
            let (sin, cos) = (angle / w_sum).sin_cos();
            let scale = scale / w_sum;
            (scale * sin, scale * cos)
        })
        .collect();
    // Each vertex keeps its scale; the rounds only find its rotation.
    let scales: Vec<f64> = rot.iter().map(|(s, c)| s.hypot(*c)).collect();
    for round in 0..rounds.max(1) {
        // Local: each vertex's best rotation (none in the first round; a
        // Bend or Advanced pin's around it).
        if round > 0 {
            for i in (0..n).filter(|&i| turns[i].is_none()) {
                let (mut dot, mut cross) = (0.0, 0.0);
                for (k, &j) in mesh.neighbours[i].iter().enumerate() {
                    let j = j as usize;
                    let (ep, ex) = (p[i] - p[j], x[i] - x[j]);
                    dot += weights[i][k] * ep.dot(ex);
                    cross += weights[i][k] * (ep.x * ex.y - ep.y * ex.x);
                }
                let (sin, cos) = cross.atan2(dot).sin_cos();
                rot[i] = (scales[i] * sin, scales[i] * cos);
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
        let outlines = v
            .value(OUTLINES)
            .filter(|o| !matches!(o, Value::FloatList(f) if f.is_empty()));
        let mut mesh_key = mesh_key;
        if let Some(Value::FloatList(f)) = outlines {
            mesh_key.extend(f.iter().map(|v| v.to_bits()));
        }
        let rounds = v.f(REFINEMENT).round().clamp(1.0, 20.0) as usize;
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let mesh = match &cache.mesh {
            Some((key, mesh)) if *key == mesh_key => mesh.clone(),
            _ => {
                let mesh = Arc::new(mesh_of(outlines, b, grow, density));
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
        let stored = matches!(
            Params::new(&self.def, p).value(OUTLINES),
            Some(Value::FloatList(f)) if !f.is_empty()
        );
        if !stored {
            trace(target, p, input)?;
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

/// Cells on the long side of a traced outline.
const TRACE_CELLS: f64 = 256.0;
/// Time between recording the copy and mapping it (the frame is submitted).
const TRACE_SETTLE: Duration = Duration::from_millis(50);

/// Moves the outline trace of `p`'s effect along, never waiting on the GPU:
/// records the alpha read-back (a pass summing the layer's alpha into cells,
/// copied to a mappable buffer), maps it a frame later, reads it once mapped.
fn trace(
    target: &mut dyn EffectTarget,
    p: &EffectParams<'_>,
    input: &GpuImage,
) -> Result<(), RenderError> {
    let effect = &p.effect.path;
    let bounds = p.layer_bounds;
    let rt = gpu::runtime(target.gpu());
    let mut traces = TRACES.lock().unwrap_or_else(|e| e.into_inner());
    let same = |t: &Trace| match t {
        Trace::Pending { bounds: b, .. }
        | Trace::Ready { bounds: b, .. }
        | Trace::Taken { bounds: b } => *b == bounds,
    };
    if let Some(t) = traces.get_mut(effect)
        && same(t)
        && (!matches!(t, Trace::Pending { .. }) || !advance(t))
    {
        return Ok(());
    }
    // Start: the layer's alpha in cells of `step` layer pixels.
    let size = DVec2::new(f64::from(bounds.width()), f64::from(bounds.height()));
    let step = (size.max_element() / TRACE_CELLS).max(1.0);
    let (cols, rows) = (
        (size.x / step).ceil().max(1.0) as usize,
        (size.y / step).ceil().max(1.0) as usize,
    );
    let origin = DVec2::new(f64::from(bounds.x0), f64::from(bounds.y0));
    let grid = image_grid::grid_scale(p, input);
    let bytes = (cols * rows * 4) as u64;
    let cells = rt.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(TRACE_LABEL),
        size: bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let buffer = rt.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(TRACE_LABEL),
        size: bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let pipeline = trace_kernel(&rt, target.format())?;
    let mut u = UniformWriter::with_size(TRACE_UNIFORM_SIZE as usize);
    image_grid::image_coords(&mut u, 0, input, grid);
    u.f32(32, (origin.x / grid) as f32)
        .f32(36, (origin.y / grid) as f32)
        .f32(40, (step / grid) as f32)
        .u32(44, ((step / grid).ceil() as u32).clamp(1, 16))
        .u32(48, cols as u32)
        .u32(52, rows as u32);
    let offset = gpu::push_uniform(&rt, &u);
    let group = rt.bind_group(
        &pipeline,
        &[
            Resource::Uniform,
            Resource::View(&input.view),
            Resource::Buffer(&cells),
        ],
    );
    {
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(TRACE_LABEL),
                timestamp_writes: None,
            });
        gpu::dispatch(
            &mut pass,
            &pipeline,
            &group,
            &[offset],
            [cols as u32, rows as u32],
        );
    }
    target
        .encoder()
        .copy_buffer_to_buffer(&cells, 0, &buffer, 0, bytes);
    traces.insert(
        effect.clone(),
        Trace::Pending {
            bounds,
            device: rt.device.clone(),
            buffer,
            origin,
            step,
            cols,
            rows,
            since: Instant::now(),
            state: Arc::new(AtomicU8::new(COPIED)),
        },
    );
    Ok(())
}

const TRACE_LABEL: &str = "bae.effects.puppet.trace";
const TRACE_UNIFORM_SIZE: u32 = 64;

fn trace_kernel(
    rt: &gpu::Runtime,
    format: wgpu::TextureFormat,
) -> Result<Arc<Kernel>, RenderError> {
    rt.kernel(
        &KernelKey {
            owner: TRACE_LABEL.into(),
            entry: "main".into(),
            variant: gpu::format_key(format),
        },
        || {
            Ok(KernelSource {
                label: TRACE_LABEL,
                wgsl: [
                    image_grid::CHANNEL_MATH,
                    include_str!("../shaders/image_grid.wgsl"),
                    include_str!("puppet_trace.wgsl"),
                ]
                .concat(),
                entry: "main",
                bindings: vec![
                    (0, Binding::Uniform(TRACE_UNIFORM_SIZE)),
                    (1, Binding::Texture),
                    (2, Binding::ReadWriteBuffer),
                ],
                workgroup: [8, 8, 1],
            })
        },
    )
}

/// Compile the kernels ahead of first use (the effects' background warm-up).
pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    kernel(rt, format)?;
    trace_kernel(rt, format).map(|_| ())
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
    fn traced_meshes_follow_the_outline() {
        // A disc of radius 30 in an 80 × 80 layer, cells of 1 px: the grid's
        // corners outside it are pulled onto its edge.
        let n = 80;
        let alpha: Vec<f32> = (0..n * n)
            .map(|i| {
                let d = DVec2::new((i % n) as f64 + 0.5, (i / n) as f64 + 0.5) - 40.0;
                if d.length() <= 30.0 { 1.0 } else { 0.0 }
            })
            .collect();
        let mask = Mask::from_alpha(DVec2::ZERO, 1.0, n, n, &alpha, 0.0);
        let mesh = Mesh::traced(DVec2::ZERO, DVec2::splat(80.0), 10.0, &mask);
        assert!(
            mesh.rest.iter().all(|p| mask.at(*p)),
            "every vertex on the shape"
        );
        let far = mesh
            .rest
            .iter()
            .map(|p| (*p - 40.0).length())
            .fold(0.0, f64::max);
        assert!(far > 28.0 && far <= 31.0, "out to the edge: {far}");
    }

    #[test]
    fn traced_outlines_split_the_mesh_and_round_trip() {
        // Two 20 × 20 blobs 40 px apart in an 80 × 20 layer, cells of 2 px.
        let (cols, rows) = (40, 10);
        let alpha: Vec<f32> = (0..cols * rows)
            .map(|i| {
                if i % cols < 10 || i % cols >= 30 {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();
        let mask = Mask::from_alpha(DVec2::ZERO, 2.0, cols, rows, &alpha, 0.0);
        assert_eq!(Mask::from_value(&mask.to_value()), Some(mask.clone()));
        let mesh = Mesh::traced(DVec2::ZERO, DVec2::new(80.0, 20.0), 10.0, &mask);
        assert!(
            mesh.rest.iter().all(|p| p.x <= 24.0 || p.x >= 56.0),
            "no vertices in the gap"
        );
        // A pin on the left blob moves it; the right one stays.
        let left = mesh.rest[mesh.nearest(DVec2::new(10.0, 10.0))];
        let pins = [pin(PinKind::Position, left, left + DVec2::new(5.0, 0.0))];
        let moved = deform(&mesh, &pins, 5);
        let l = mesh.nearest(DVec2::new(0.0, 0.0));
        let r = mesh.nearest(DVec2::new(80.0, 0.0));
        assert!((moved[l].x - mesh.rest[l].x - 5.0).abs() < 1e-6);
        assert_eq!(moved[r], mesh.rest[r]);
        // Expansion grows it: the gap narrows.
        let grown = mesh_of(Some(&mask.to_value()), IRect::new(0, 0, 80, 20), 6.0, 10.0);
        assert!(grown.rest.iter().any(|p| p.x > 21.0 && p.x < 30.0));
    }

    #[test]
    fn a_bend_pin_turns_the_mesh_around_it() {
        let mesh = Mesh::grid(DVec2::ZERO, DVec2::new(100.0, 20.0), 10.0);
        let centre = mesh.rest[mesh.nearest(DVec2::new(50.0, 10.0))];
        let mut bend = pin(PinKind::Bend, centre, centre);
        bend.rotation = 90.0;
        let turned = deform(&mesh, &[bend], 20);
        // Clockwise on screen: the right end swings down below the pin.
        let end = turned[mesh.nearest(DVec2::new(100.0, 10.0))];
        let want = centre + DVec2::new(0.0, 100.0 - centre.x);
        assert!(end.distance(want) < 12.0, "{end} vs {want}");
        // Scale 50 %: the ends come in.
        let mut small = pin(PinKind::Advanced, centre, centre);
        small.scale = 50.0;
        let shrunk = deform(&mesh, &[small], 20);
        let end = shrunk[mesh.nearest(DVec2::new(100.0, 10.0))];
        assert!(end.x < 90.0 && end.x > centre.x, "{end}");
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
