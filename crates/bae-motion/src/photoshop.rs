//! Commit the import dialog's choice in one transaction, including groups and source styles.
use crate::{
    comp::{CreateComp, CreateFolder, CreateFolderParams},
    import::{ImportFile, ImportItems, ImportParams, ReplaceFile, ReplaceFileParams},
    layer::{CreateLayer, CreateLayerParams},
    layer_styles::{EditStyles, Operation},
    query,
    shape::CreateMask,
    util::{child, create, ensure_folder, fail, set},
};
use bae_interop_psd::import::{Document, Layer};
use bae_model::{
    Command, CommandError, LayerKind, PrimPath, PrimType, Time, Transaction, Value,
    layer_styles as styles, psd::LayerImage,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    #[default]
    Footage,
    Composition,
    RetainSize,
}

#[derive(Deserialize, JsonSchema)]
pub struct Params {
    pub file: String,
    pub document: Document,
    pub kind: ImportKind,
    pub selection: Option<LayerImage>,
    pub parent: Option<PrimPath>,
    /// Composition or Timeline drop destination.
    pub comp: Option<PrimPath>,
    pub folder: Option<String>,
    pub replace: Option<PrimPath>,
    #[serde(default)]
    pub keep_name: bool,
    /// In compositions copy the source style onto each instance; otherwise let the
    /// instance inherit the source style (AE's merged-styles footage).
    pub editable: bool,
    /// Single-layer footage can discard the recorded style.
    pub ignore_styles: bool,
    pub fps: f64,
    pub duration: Option<Time>,
    pub label: Option<i64>,
    pub folder_label: Option<i64>,
}
pub struct ImportPhotoshop;
impl Command for ImportPhotoshop {
    type Params = Params;
    type Output = PrimPath;
    const ID: &'static str = "item.import_photoshop";
    const TITLE: &'static str = "Import Photoshop File";
    const DESCRIPTION: &'static str = "Import a PSD/PSB as footage or a layered composition. The file-reading worker supplies the layer metadata.";
    const FILESYSTEM: bool = true;
    fn run(&self, tx: &mut Transaction<'_>, p: Params) -> Result<PrimPath, CommandError> {
        let stem = std::path::Path::new(&p.file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Photoshop");
        let mut parent = if let Some(parent) = &p.parent {
            tx.stage().require_prim(parent)?;
            parent.clone()
        } else {
            let assets = PrimPath::new(query::ASSETS_FOLDER).expect("valid assets path");
            ensure_folder(tx, &assets)?;
            assets
        };
        if let Some(rel) = &p.folder {
            parent = crate::import::folder_path(tx, &parent, rel)?;
        }
        let (item, kind) = if p.kind == ImportKind::Footage {
            let layer = if let Some(sel) = p.selection {
                Some(
                    p.document
                        .find(sel.key)
                        .ok_or_else(|| fail("selected Photoshop layer is missing"))?,
                )
            } else {
                None
            };
            if layer.is_none() && !p.document.merged {
                return Err(fail("document has no saved composite"));
            }
            (
                footage(tx, &p, &parent, layer, p.selection)?,
                LayerKind::Footage,
            )
        } else {
            let root = comp(tx, &p, &parent, stem, p.document.size)?;
            let folder = CreateFolder
                .run(
                    tx,
                    CreateFolderParams {
                        name: Some(format!("{stem} {}", crate::names::localized("Layers"))),
                        parent: Some(parent),
                        label: p.folder_label,
                    },
                )?
                .path;
            stack(tx, &p, &root, &folder, &p.document.layers, [0, 0])?;
            (root, LayerKind::PreComp)
        };
        if p.folder.is_none()
            && p.replace.is_none()
            && let Some(target) = &p.comp
        {
            crate::import::add_imported_layer(tx, target, kind, &item)?;
        }
        Ok(item)
    }
}
fn comp(
    tx: &mut Transaction<'_>,
    p: &Params,
    parent: &PrimPath,
    name: &str,
    [w, h]: [u32; 2],
) -> Result<PrimPath, CommandError> {
    let args = serde_json::from_value(
        json!({"name":name,"width":w,"height":h,"parent":parent,"fps":p.fps,"duration":p.duration}),
    )
    .map_err(|e| fail(e.to_string()))?;
    Ok(CreateComp.run(tx, args)?.path)
}
fn footage(
    tx: &mut Transaction<'_>,
    p: &Params,
    parent: &PrimPath,
    layer: Option<&Layer>,
    selection: Option<LayerImage>,
) -> Result<PrimPath, CommandError> {
    let [w, h] = if selection.is_some_and(|s| !s.canvas) {
        layer.map_or(p.document.size, |l| l.size)
    } else {
        p.document.size
    };
    let mut attrs = BTreeMap::from([
        ("width".into(), Value::Int(w.into())),
        ("height".into(), Value::Int(h.into())),
        ("has_video".into(), Value::Bool(true)),
        ("has_audio".into(), Value::Bool(false)),
        ("alpha_mode".into(), Value::Enum("straight".into())),
    ]);
    if let Some(label) = p.label {
        attrs.insert("label".into(), Value::Int(label));
    }
    if let Some(duration) = p.duration {
        attrs.insert("duration".into(), Value::Time(duration));
    }
    if let Some(l) = layer {
        attrs.insert("psd:layer_name".into(), Value::String(l.name.clone()));
    }
    if let Some(s) = selection {
        attrs.extend(s.attributes().into_iter().map(|(k, v)| (k.into(), v)));
    }
    let file = ImportFile {
        path: p.file.clone(),
        name: layer.map(|l| l.name.clone()),
        attrs,
        folder: None,
    };
    let item = if let Some(item) = &p.replace {
        ReplaceFile.run(
            tx,
            ReplaceFileParams {
                item: item.clone(),
                file,
                keep_name: p.keep_name,
            },
        )?;
        item.clone()
    } else {
        ImportItems
            .run(
                tx,
                ImportParams {
                    precompose: false,
                    parent: Some(parent.clone()),
                    comp: None,
                    files: vec![file],
                },
            )?
            .items
            .remove(0)
    };
    if let (Some(layer), Some(sel)) = (layer, selection) {
        let parent = child(&item, "masks")?;
        let origin = if sel.canvas { [0, 0] } else { layer.origin };
        for (i, mask) in layer.masks.iter().enumerate() {
            ensure_folder(tx, &parent)?;
            let path = child(&parent, &format!("VectorMask{}", i + 1))?;
            create(tx, &path, PrimType::MASK, None)?;
            let mut shape = mask.shape.clone();
            for v in &mut shape.vertices {
                v[0] -= f64::from(origin[0]);
                v[1] -= f64::from(origin[1]);
            }
            set(tx, &path, "path", Value::Bezier(shape))?;
            set(tx, &path, "mode", Value::Enum(mask.mode.clone()))?;
            set(tx, &path, "inverted", Value::Bool(mask.inverted))?;
        }
    }
    if selection.is_some() {
        set(tx, &item, "psd:ignore_styles", Value::Bool(p.ignore_styles))?;
    }
    if let Some(l) = layer
        && !p.ignore_styles
    {
        write_styles(tx, &item, &l.styles)?;
    }
    Ok(item)
}
fn write_styles(
    tx: &mut Transaction<'_>,
    target: &PrimPath,
    params: &BTreeMap<u32, Value>,
) -> Result<(), CommandError> {
    if params.is_empty() {
        return Ok(());
    }
    let path = child(target, styles::PRIM)?;
    create(tx, &path, PrimType::EFFECT, None)?;
    set(
        tx,
        &path,
        "match_name",
        Value::String(styles::RENDER_ID.into()),
    )?;
    set(tx, &path, "name", Value::String("Layer Styles".into()))?;
    for (&id, v) in params {
        set(tx, &path, &bae_model::effect::param_attr(id), v.clone())?;
    }
    Ok(())
}
fn stack(
    tx: &mut Transaction<'_>,
    p: &Params,
    comp_path: &PrimPath,
    folder: &PrimPath,
    layers: &[Layer],
    parent_origin: [i32; 2],
) -> Result<(), CommandError> {
    let mut base: Option<PrimPath> = None;
    for l in layers {
        let (source, kind, origin, size) = if let Some(children) = &l.children {
            let (origin, size) = if p.kind == ImportKind::RetainSize {
                (l.origin, l.size)
            } else {
                ([0, 0], p.document.size)
            };
            let nested = comp(tx, p, folder, &l.name, size)?;
            stack(tx, p, &nested, folder, children, origin)?;
            (nested, LayerKind::PreComp, origin, size)
        } else {
            let canvas = p.kind == ImportKind::Composition;
            let source = footage(
                tx,
                p,
                folder,
                Some(l),
                Some(LayerImage {
                    key: l.key,
                    canvas,
                    mask: Default::default(),
                }),
            )?;
            (
                source,
                if l.text.is_some() {
                    LayerKind::Text
                } else {
                    LayerKind::Footage
                },
                if canvas { [0, 0] } else { l.origin },
                if canvas { p.document.size } else { l.size },
            )
        };
        let mut args = CreateLayerParams::with_source(comp_path, kind, &source, Time::ZERO);
        args.name = Some(l.name.clone());
        args.position = Some([
            f64::from(origin[0]) - f64::from(parent_origin[0]) + f64::from(size[0]) / 2.,
            f64::from(origin[1]) - f64::from(parent_origin[1]) + f64::from(size[1]) / 2.,
        ]);
        if let Some((doc, pos)) = &l.text {
            args.source = None;
            args.text = Some(doc.text.clone());
            args.position = Some([
                pos[0] - f64::from(parent_origin[0]),
                pos[1] - f64::from(parent_origin[1]),
            ]);
        }
        let path = CreateLayer.run(tx, args)?.path;
        if let Some((doc, _)) = &l.text {
            set(
                tx,
                &path,
                "text:source",
                Value::TextDocument(Box::new(doc.clone())),
            )?;
            set(tx, &path, "source", Value::PrimRef(source))?;
        }
        set(tx, &path, "enabled", Value::Bool(l.visible))?;
        set(tx, &path, "blend_mode", Value::Enum(l.blend.clone()))?;
        set(tx, &path, "transform:opacity", Value::Float(l.opacity))?;
        if l.clipped {
            if let Some(base) = &base {
                set(tx, &path, "psd:clip_base", Value::PrimRef(base.clone()))?;
                set(tx, &path, "track_matte", Value::Enum("alpha".into()))?;
                set(tx, &path, "track_matte_layer", Value::PrimRef(base.clone()))?;
            }
        } else {
            base = Some(path.clone());
        }
        if let Some((enabled, inverted, opacity)) = l.pixel_mask {
            let masks = child(&path, "masks")?;
            ensure_folder(tx, &masks)?;
            let pixel = child(&masks, "PhotoshopPixelMask")?;
            create(tx, &pixel, PrimType::MASK, None)?;
            set(
                tx,
                &pixel,
                "name",
                Value::String(format!("{} {}", l.name, crate::names::localized("Mask"))),
            )?;
            set(tx, &pixel, "psd:channel_mask", Value::Bool(true))?;
            if l.children.is_some() {
                let file = bae_model::asset::AssetFile::from_path(
                    std::path::Path::new(&p.file),
                    None,
                    false,
                )
                .map_err(|e| fail(e.to_string()))?;
                for op in file.to_ops(&pixel) {
                    tx.apply(op)?;
                }
                for (k, v) in (LayerImage {
                    key: l.key,
                    canvas: false,
                    mask: Default::default(),
                })
                .attributes()
                {
                    set(tx, &pixel, k, v)?;
                }
                set(tx, &pixel, "psd:origin", Value::Vec2(origin.map(f64::from)))?;
            }
            set(
                tx,
                &pixel,
                "mode",
                Value::Enum(if enabled { "add" } else { "none" }.into()),
            )?;
            set(tx, &pixel, "inverted", Value::Bool(inverted))?;
            set(tx, &pixel, "opacity", Value::Float(opacity))?;
            set(tx, &pixel, "path", Value::Bezier(Default::default()))?;
            set(tx, &path, "psd:separate_mask", Value::Bool(true))?;
        }
        if !l.masks.is_empty() {
            set(tx, &path, "psd:separate_vectors", Value::Bool(true))?;
        }
        for m in &l.masks {
            let mut shape = m.shape.clone();
            for v in &mut shape.vertices {
                v[0] -= f64::from(origin[0]);
                v[1] -= f64::from(origin[1]);
            }
            let args = serde_json::from_value(json!({
                "layer": path, "kind": "path", "path": shape,
                "mode": m.mode, "inverted": m.inverted
            }))
            .map_err(|e| fail(e.to_string()))?;
            CreateMask.run(tx, args)?;
        }
        if !l.styles.is_empty() {
            if l.children.is_some() {
                write_styles(tx, &path, &l.styles)?;
            } else if p.editable {
                EditStyles.run(
                    tx,
                    crate::layer_styles::Params {
                        layers: vec![path],
                        operation: Operation::Convert,
                    },
                )?;
            }
        }
    }
    Ok(())
}
