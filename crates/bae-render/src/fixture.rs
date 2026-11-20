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
        let keyframe = serde_json::to_value(Keyframe::linear(value))
            .map_err(|e| RenderError::Other(e.to_string()))?;
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
