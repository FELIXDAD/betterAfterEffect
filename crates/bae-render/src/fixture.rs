//! Scene fixtures: builds test and benchmark projects through `bae-model`
//! commands (`prim.create`, `attr.set`, `key.set`), so they pass schema
//! validation exactly like edits from the UI or an agent.

use std::path::Path;

use bae_model::{
    AssetFile, AuthorId, CommandRegistry, FrameRate, Keyframe, PrimPath, Stage, Time, Value,
};
use serde_json::json;

use crate::error::RenderError;

/// Builds a stage with comps, layers and assets.
pub struct SceneBuilder {
    stage: Stage,
    registry: CommandRegistry,
    author: AuthorId,
}

impl Default for SceneBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SceneBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SceneBuilder").finish_non_exhaustive()
    }
}

fn path(s: &str) -> Result<PrimPath, RenderError> {
    PrimPath::new(s).map_err(|e| RenderError::Other(e.to_string()))
}

impl SceneBuilder {
    /// An empty stage with `/comps` and `/assets` folders.
    pub fn new() -> Self {
        let mut b = Self {
            stage: Stage::new(),
            registry: CommandRegistry::with_builtins(),
            author: AuthorId::system(),
        };
        for folder in ["/comps", "/assets"] {
            b.create(folder, "Folder").expect("fresh stage");
        }
        b
    }

    /// The stage built so far.
    pub fn stage(&self) -> &Stage {
        &self.stage
    }

    /// Finishes and returns the stage.
    pub fn into_stage(self) -> Stage {
        self.stage
    }

    fn run(&mut self, id: &str, params: serde_json::Value) -> Result<(), RenderError> {
        self.registry
            .execute(&mut self.stage, &self.author, id, params)
            .map(|_| ())
            .map_err(|e| RenderError::Other(format!("{id}: {e}")))
    }

    fn create(&mut self, p: &str, prim_type: &str) -> Result<PrimPath, RenderError> {
        self.run("prim.create", json!({"path": p, "prim_type": prim_type}))?;
        path(p)
    }

    /// Sets a static value.
    pub fn set(&mut self, prim: &PrimPath, attr: &str, value: Value) -> Result<(), RenderError> {
        let value = serde_json::to_value(&value).map_err(|e| RenderError::Other(e.to_string()))?;
        self.run(
            "attr.set",
            json!({"path": prim.as_str(), "attr": attr, "value": value}),
        )
    }

    /// Sets a linear keyframe at `time` (layer time for layer attributes).
    pub fn key(
        &mut self,
        prim: &PrimPath,
        attr: &str,
        time: Time,
        value: Value,
    ) -> Result<(), RenderError> {
        self.key_with(prim, attr, time, Keyframe::linear(value))
    }

    /// Sets `keyframe` at `time` (layer time for layer attributes).
    pub fn key_with(
        &mut self,
        prim: &PrimPath,
        attr: &str,
        time: Time,
        keyframe: Keyframe,
    ) -> Result<(), RenderError> {
        let keyframe =
            serde_json::to_value(keyframe).map_err(|e| RenderError::Other(e.to_string()))?;
        self.run(
            "key.set",
            json!({"path": prim.as_str(), "attr": attr, "time": time.flicks(), "keyframe": keyframe}),
        )
    }

    /// Creates `/comps/<name>` with its `layers` folder.
    pub fn comp(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        frame_rate: FrameRate,
        duration: Time,
    ) -> Result<PrimPath, RenderError> {
        let comp = self.create(&format!("/comps/{name}"), "Comp")?;
        self.create(&format!("/comps/{name}/layers"), "Folder")?;
        self.set(&comp, "width", Value::Int(i64::from(width)))?;
        self.set(&comp, "height", Value::Int(i64::from(height)))?;
        self.set(&comp, "frame_rate", Value::FrameRate(frame_rate))?;
        self.set(&comp, "duration", Value::Time(duration))?;
        Ok(comp)
    }

    /// Creates `/assets/<name>`: a solid of `color` (straight RGBA) and size.
    pub fn solid_asset(
        &mut self,
        name: &str,
        color: [f64; 4],
        width: u32,
        height: u32,
    ) -> Result<PrimPath, RenderError> {
        let asset = self.create(&format!("/assets/{name}"), "Asset")?;
        self.set(&asset, "kind", Value::Enum("solid".into()))?;
        self.set(&asset, "solid_color", Value::Color(color))?;
        self.set(&asset, "width", Value::Int(i64::from(width)))?;
        self.set(&asset, "height", Value::Int(i64::from(height)))?;
        Ok(asset)
    }

    /// Creates `/assets/<name>` referencing an existing image file.
    pub fn footage_asset(
        &mut self,
        name: &str,
        file: &Path,
        base_dir: Option<&Path>,
        size: Option<[u32; 2]>,
    ) -> Result<PrimPath, RenderError> {
        let asset = self.create(&format!("/assets/{name}"), "Asset")?;
        self.set(&asset, "kind", Value::Enum("footage".into()))?;
        let reference = AssetFile::from_path(file, base_dir, false)?;
        let ops = reference.to_ops(&asset);
        for op in ops {
            if let bae_model::Op::SetValue {
                attr,
                value: Some(value),
                ..
            } = op
            {
                self.set(&asset, &attr, value)?;
            }
        }
        if let Some([w, h]) = size {
            self.set(&asset, "width", Value::Int(i64::from(w)))?;
            self.set(&asset, "height", Value::Int(i64::from(h)))?;
        }
        Ok(asset)
    }

    /// Adds a layer **below** the existing layers of `comp` (so scenes read top
    /// to bottom), with `kind` and optional `source`.
    pub fn layer(
        &mut self,
        comp: &PrimPath,
        name: &str,
        kind: &str,
        source: Option<&PrimPath>,
    ) -> Result<PrimPath, RenderError> {
        let layer = self.create(&format!("{}/layers/{name}", comp.as_str()), "Layer")?;
        self.set(&layer, "kind", Value::Enum(kind.into()))?;
        if let Some(source) = source {
            self.set(&layer, "source", Value::PrimRef(source.clone()))?;
        }
        Ok(layer)
    }

    /// Adds a solid layer of `size` and `color` positioned with its top-left
    /// corner at `top_left` (anchor at the centre, like AE).
    pub fn solid_layer(
        &mut self,
        comp: &PrimPath,
        name: &str,
        color: [f64; 4],
        size: [u32; 2],
        top_left: [f64; 2],
    ) -> Result<PrimPath, RenderError> {
        let asset = self.solid_asset(&format!("{name}_solid"), color, size[0], size[1])?;
        let layer = self.layer(comp, name, "solid", Some(&asset))?;
        let half = [f64::from(size[0]) / 2.0, f64::from(size[1]) / 2.0];
        self.set(
            &layer,
            "transform:anchor_point",
            Value::Vec3([half[0], half[1], 0.0]),
        )?;
        self.set(
            &layer,
            "transform:position",
            Value::Vec3([top_left[0] + half[0], top_left[1] + half[1], 0.0]),
        )?;
        Ok(layer)
    }
}

impl SceneBuilder {
    /// Adds mask `name` to `layer` (`<layer>/masks/<name>`, below the existing
    /// masks) with `shape` (layer pixels) and `mode` (`add`, `subtract` …).
    pub fn mask(
        &mut self,
        layer: &PrimPath,
        name: &str,
        shape: bae_model::BezierPath,
        mode: &str,
    ) -> Result<PrimPath, RenderError> {
        let folder = format!("{}/masks", layer.as_str());
        if self.stage.prim(&path(&folder)?).is_none() {
            self.create(&folder, "Folder")?;
        }
        let mask = self.create(&format!("{folder}/{name}"), "Mask")?;
        self.set(&mask, "path", Value::Bezier(shape))?;
        self.set(&mask, "mode", Value::Enum(mode.into()))?;
        Ok(mask)
    }
}

/// A closed rectangle path `[x0, x1] × [y0, y1]` (clockwise from top-left, like
/// AE's rectangle mask tool).
pub fn rect_path(x0: f64, y0: f64, x1: f64, y1: f64) -> bae_model::BezierPath {
    bae_model::BezierPath {
        vertices: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
        in_tangents: vec![[0.0, 0.0]; 4],
        out_tangents: vec![[0.0, 0.0]; 4],
        closed: true,
        feather: Vec::new(),
    }
}

/// A closed ellipse path inscribed in `[x0, x1] × [y0, y1]` (four cubic
/// segments, AE's ellipse tool: starts at the top, clockwise).
pub fn ellipse_path(x0: f64, y0: f64, x1: f64, y1: f64) -> bae_model::BezierPath {
    // AE's ellipse handle length factor (4/3 · tan(π/8)).
    const K: f64 = 0.552_284_749_830_793_6;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    bae_model::BezierPath {
        vertices: vec![[cx, y0], [x1, cy], [cx, y1], [x0, cy]],
        in_tangents: vec![[-rx * K, 0.0], [0.0, -ry * K], [rx * K, 0.0], [0.0, ry * K]],
        out_tangents: vec![[rx * K, 0.0], [0.0, ry * K], [-rx * K, 0.0], [0.0, -ry * K]],
        closed: true,
        feather: Vec::new(),
    }
}

/// Deterministic pseudo-random numbers for fixtures (64-bit LCG).
#[derive(Clone, Debug)]
pub struct Lcg(u64);

impl Lcg {
    /// A generator from a seed.
    pub fn new(seed: u64) -> Self {
        Self(
            seed.wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407),
        )
    }

    /// Uniform in `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
}

/// Writes a `w × h` RGBA test texture (a soft-edged rounded "label" with a
/// gradient, straight alpha), standing in for a text layer until 05 lands.
pub fn write_label_png(path: &Path, w: u32, h: u32, hue: f64) -> Result<(), RenderError> {
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    let r = f64::from(h) * 0.3;
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            // Signed distance to a rounded rectangle inset by 2 px.
            let qx = (fx - f64::from(w) / 2.0).abs() - (f64::from(w) / 2.0 - 2.0 - r);
            let qy = (fy - f64::from(h) / 2.0).abs() - (f64::from(h) / 2.0 - 2.0 - r);
            let outside =
                (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
            let alpha = (0.5 - outside).clamp(0.0, 1.0);
            let t = fx / f64::from(w);
            let c = |phase: f64| {
                (0.5 + 0.5 * ((hue + t * 0.3 + phase) * std::f64::consts::TAU).cos()) * 255.0
            };
            data.extend_from_slice(&[
                c(0.0) as u8,
                c(1.0 / 3.0) as u8,
                c(2.0 / 3.0) as u8,
                (alpha * 255.0).round() as u8,
            ]);
        }
    }
    crate::png_out::write_rgba8(
        path,
        &bae_gpu::transfer::ImageData {
            width: w,
            height: h,
            format: bae_gpu::wgpu::TextureFormat::Rgba8Unorm,
            bytes_per_row: w * 4,
            data,
        },
    )
}

/// Benchmark B1 "motion graphics" (ARCHITECTURE §11): 1080p30, 10 s, 50 shape
/// layers + 10 text layers. Until 05 lands, shapes are animated solids and text
/// layers are animated RGBA textures (written into `assets_dir`); blur and glow
/// wait for 07. Layers move, rotate, scale and fade; a fifth use Add, Screen,
/// Multiply or Overlay. Returns the builder and the comp.
pub fn b1(assets_dir: &Path) -> Result<(SceneBuilder, PrimPath), RenderError> {
    const SECONDS: i64 = 10;
    let mut b = SceneBuilder::new();
    let comp = b.comp(
        "b1",
        1920,
        1080,
        FrameRate::FPS_30,
        Time::from_flicks(SECONDS * Time::FLICKS_PER_SECOND),
    )?;
    let mut rng = Lcg::new(0xB1);
    let modes = ["add", "screen", "multiply", "overlay"];
    let end = Time::from_flicks(SECONDS * Time::FLICKS_PER_SECOND);
    let mid = Time::from_flicks(SECONDS * Time::FLICKS_PER_SECOND / 2);

    std::fs::create_dir_all(assets_dir)?;
    for i in 0..10 {
        let file = assets_dir.join(format!("label_{i:02}.png"));
        let (w, h) = (
            rng.range(360.0, 720.0) as u32,
            rng.range(80.0, 140.0) as u32,
        );
        write_label_png(&file, w, h, rng.next_f64())?;
        let asset = b.footage_asset(&format!("label_{i:02}"), &file, None, Some([w, h]))?;
        let layer = b.layer(&comp, &format!("text_{i:02}"), "footage", Some(&asset))?;
        b.set(
            &layer,
            "transform:anchor_point",
            Value::Vec3([f64::from(w) / 2.0, f64::from(h) / 2.0, 0.0]),
        )?;
        let y = 120.0 + 84.0 * f64::from(i);
        b.key(
            &layer,
            "transform:position",
            Time::ZERO,
            Value::Vec3([-400.0, y, 0.0]),
        )?;
        b.key(
            &layer,
            "transform:position",
            mid,
            Value::Vec3([960.0, y, 0.0]),
        )?;
        b.key(
            &layer,
            "transform:position",
            end,
            Value::Vec3([2300.0, y, 0.0]),
        )?;
        b.key(&layer, "transform:opacity", Time::ZERO, Value::Float(0.0))?;
        b.key(
            &layer,
            "transform:opacity",
            Time::ONE_SECOND,
            Value::Float(100.0),
        )?;
    }
    for i in 0..50 {
        let size = [rng.range(60.0, 420.0) as u32, rng.range(60.0, 420.0) as u32];
        let color = [rng.next_f64(), rng.next_f64(), rng.next_f64(), 1.0];
        let layer = b.solid_layer(&comp, &format!("shape_{i:02}"), color, size, [0.0, 0.0])?;
        let p0 = [rng.range(0.0, 1920.0), rng.range(0.0, 1080.0), 0.0];
        let p1 = [rng.range(0.0, 1920.0), rng.range(0.0, 1080.0), 0.0];
        let p2 = [rng.range(0.0, 1920.0), rng.range(0.0, 1080.0), 0.0];
        b.key(&layer, "transform:position", Time::ZERO, Value::Vec3(p0))?;
        b.key(&layer, "transform:position", mid, Value::Vec3(p1))?;
        b.key(&layer, "transform:position", end, Value::Vec3(p2))?;
        b.key(&layer, "transform:rotation", Time::ZERO, Value::Float(0.0))?;
        b.key(
            &layer,
            "transform:rotation",
            end,
            Value::Float(rng.range(-360.0, 360.0)),
        )?;
        let s = rng.range(50.0, 150.0);
        b.key(
            &layer,
            "transform:scale",
            Time::ZERO,
            Value::Vec3([s, s, 100.0]),
        )?;
        b.key(
            &layer,
            "transform:scale",
            end,
            Value::Vec3([150.0 - s + 50.0, s, 100.0]),
        )?;
        b.key(
            &layer,
            "transform:opacity",
            Time::ZERO,
            Value::Float(rng.range(40.0, 100.0)),
        )?;
        b.key(
            &layer,
            "transform:opacity",
            end,
            Value::Float(rng.range(40.0, 100.0)),
        )?;
        if i % 5 == 4 {
            b.set(
                &layer,
                "blend_mode",
                Value::Enum(modes[(i / 5) % modes.len()].into()),
            )?;
        }
    }
    // A full-frame background at the bottom.
    b.solid_layer(
        &comp,
        "background",
        [0.08, 0.09, 0.12, 1.0],
        [1920, 1080],
        [0.0, 0.0],
    )?;
    Ok((b, comp))
}
