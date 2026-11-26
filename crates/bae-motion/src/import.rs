//! `item.import`: AE's File > Import > File (Cmd/Ctrl+I), Import Multiple
//! Files and files dragged from Finder / Explorer onto the Project panel.
//!
//! The importer (the UI, a script or an agent) probes the files with
//! `bae-media` (`MediaInfo::asset_attributes`) and passes the attributes; this
//! command creates the footage items (one `Asset` prim each, with the file's
//! path and fingerprint) in one undo step. It reads only file metadata (size,
//! modification time), never pixels, so it stays free of media and GPU code.

use std::collections::BTreeMap;
use std::path::Path;

use bae_model::asset::AssetFile;
use bae_model::{Command, CommandError, LayerKind, PrimPath, PrimType, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::layer::{CreateLayer, CreateLayerParams};
use crate::query::{self, ASSETS_FOLDER};
use crate::util::{child, create, ensure_folder, fail, set};

pub struct ImportItems;

/// One file to import.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ImportFile {
    /// The file (for an image sequence: one of its frames).
    pub path: String,
    /// Display name (default: the file name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Asset attributes from probing the file: `kind` (footage, sequence),
    /// `width`, `height`, `frame_rate`, `duration`, `pixel_aspect`,
    /// `alpha_mode`, `has_video`, `has_audio`, `sequence:first_frame`,
    /// `sequence:pattern` (bae-media `MediaInfo::asset_attributes`).
    #[serde(default)]
    pub attrs: BTreeMap<String, Value>,
    /// Folder inside `parent` to put the item in, `/`-separated (a dropped
    /// folder keeps its structure); created when missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ImportParams {
    pub files: Vec<ImportFile>,
    /// Project folder to import into (default the assets folder).
    #[serde(default)]
    pub parent: Option<PrimPath>,
    /// Also add the files (not those inside a dropped folder) to this comp as
    /// layers, the first on top: files dropped on the Composition or
    /// Timeline panel (AE), in the same undo step.
    #[serde(default)]
    pub comp: Option<PrimPath>,
}

/// Output of `item.import`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ImportOutput {
    /// The new footage items, in the order of `files`.
    pub items: Vec<PrimPath>,
    /// The layers added to `comp`, top first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<PrimPath>,
}

/// The Asset attributes an importer may set.
const ATTRS: &[&str] = &[
    bae_model::psd::LayerImage::KEY,
    bae_model::psd::LayerImage::CANVAS,
    "psd:ignore_styles",
    "psd:layer_name",
    "kind",
    "width",
    "height",
    "frame_rate",
    "duration",
    "pixel_aspect",
    "alpha_mode",
    "has_video",
    "has_audio",
    "sequence:first_frame",
    "sequence:pattern",
    "loop",
    "label",
    "color_space",
    "timecode:start_frame",
    "timecode:drop_frame",
];

/// The folder `rel` (`a/b`) under `parent`, created as needed (matched by
/// display name, like the Project panel shows it).
pub(crate) fn folder_path(
    tx: &mut Transaction<'_>,
    parent: &PrimPath,
    rel: &str,
) -> Result<PrimPath, CommandError> {
    let mut cur = parent.clone();
    for part in rel.split('/').map(str::trim).filter(|p| !p.is_empty()) {
        let existing = tx.stage().children(&cur).into_iter().find(|c| {
            tx.stage()
                .prim(c)
                .is_some_and(|v| v.is_a(&PrimType::FOLDER))
                && query::display_name_of(tx.stage(), c) == part
        });
        cur = match existing {
            Some(c) => c,
            None => {
                let path = child(&cur, &query::unique_child_name(tx.stage(), &cur, part))?;
                create(tx, &path, PrimType::FOLDER, None)?;
                set(tx, &path, "name", Value::String(part.to_owned()))?;
                path
            }
        };
    }
    Ok(cur)
}

impl Command for ImportItems {
    type Params = ImportParams;
    type Output = ImportOutput;
    const ID: &'static str = "item.import";
    const FILESYSTEM: bool = true;
    const TITLE: &'static str = "Import";
    const DESCRIPTION: &'static str = "Import files as footage items (AE File > Import > File, Cmd/Ctrl+I; files dropped on the Project panel). Pass each file's path and the asset attributes from probing it (kind, width, height, frame_rate, duration, pixel_aspect, alpha_mode, has_video, has_audio, sequence:first_frame, sequence:pattern); `folder` keeps a dropped folder's structure. One undo step.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<ImportOutput, CommandError> {
        if p.files.is_empty() {
            return Err(fail("no files given"));
        }
        let parent = match p.parent {
            Some(parent) => {
                tx.stage().require_prim(&parent)?;
                parent
            }
            None => {
                let assets = PrimPath::new(ASSETS_FOLDER).expect("valid");
                ensure_folder(tx, &assets)?;
                assets
            }
        };
        let mut items = Vec::with_capacity(p.files.len());
        let mut add = Vec::new();
        for f in p.files {
            let path = Path::new(&f.path);
            let file = AssetFile::from_path(path, None, false)
                .map_err(|e| fail(format!("cannot import {}: {e}", f.path)))?;
            if let Some(bad) = f.attrs.keys().find(|k| !ATTRS.contains(&k.as_str())) {
                return Err(fail(format!("{bad} is not a footage attribute")));
            }
            let folder = match f.folder.as_deref() {
                Some(rel) => folder_path(tx, &parent, rel)?,
                None => parent.clone(),
            };
            let name = f
                .name
                .filter(|n| !n.trim().is_empty())
                .or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "Footage".to_owned());
            let flag = |k: &str| f.attrs.get(k).and_then(Value::as_bool);
            let audio_only = flag("has_audio") == Some(true) && flag("has_video") == Some(false);
            let item = child(
                &folder,
                &query::unique_child_name(tx.stage(), &folder, &name),
            )?;
            create(tx, &item, PrimType::ASSET, None)?;
            set(tx, &item, "name", Value::String(name))?;
            if !f.attrs.contains_key("kind") {
                set(tx, &item, "kind", Value::Enum("footage".into()))?;
            }
            for (attr, value) in f.attrs {
                set(tx, &item, &attr, value)?;
            }
            for op in file.to_ops(&item) {
                tx.apply(op)?;
            }
            if f.folder.is_none() {
                add.push((item.clone(), audio_only));
            }
            items.push(item);
        }
        let mut layers = Vec::new();
        if let Some(comp) = p.comp {
            // Added bottom-up at the top of the stack: the first file ends on top.
            for (item, audio_only) in add.into_iter().rev() {
                let kind = if audio_only {
                    LayerKind::Audio
                } else {
                    LayerKind::Footage
                };
                layers.insert(0, add_imported_layer(tx, &comp, kind, &item)?);
            }
        }
        Ok(ImportOutput { items, layers })
    }
}

pub(crate) fn add_imported_layer(
    tx: &mut Transaction<'_>,
    comp: &PrimPath,
    kind: LayerKind,
    item: &PrimPath,
) -> Result<PrimPath, CommandError> {
    let mut params = CreateLayerParams::with_source(comp, kind, item, Time::ZERO);
    params.name = Some(query::display_name_of(tx.stage(), item));
    params.index = Some(0);
    Ok(CreateLayer.run(tx, params)?.path)
}

/// File ▸ Replace Footage ▸ File… (Cmd/Ctrl+H): a footage or placeholder item
/// takes another file; its layers keep their place and now show the new file.
pub struct ReplaceFile;

#[derive(Deserialize, JsonSchema)]
pub struct ReplaceFileParams {
    pub item: PrimPath,
    /// The new file and the asset attributes from probing it (as for
    /// `item.import`; `folder` is ignored).
    pub file: ImportFile,
    /// Keep the item's name (File > Reload Footage reads the same file again).
    #[serde(default)]
    pub keep_name: bool,
    /// Reapply the current interpretation to the replacement file.
    #[serde(default)]
    pub keep_interpretation: bool,
}

impl Command for ReplaceFile {
    type Params = ReplaceFileParams;
    type Output = ();
    const ID: &'static str = "item.replace_file";
    const TITLE: &'static str = "Replace Footage";
    const DESCRIPTION: &'static str = "Replace a footage or placeholder item's file (AE File > Replace Footage > File..., Cmd/Ctrl+H): the item takes the new file, its name and probed attributes; layers using it keep their timing and transform. One undo step.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let view = tx.stage().require_prim(&p.item)?;
        let kind = query::static_value(&view, "kind")
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        if !view.is_a(&PrimType::ASSET) || kind == "solid" {
            return Err(fail(format!("{} is not a footage item", p.item)));
        }
        let path = Path::new(&p.file.path);
        let file = AssetFile::from_path(path, None, false)
            .map_err(|e| fail(format!("cannot use {}: {e}", p.file.path)))?;
        if let Some(bad) = p.file.attrs.keys().find(|k| !ATTRS.contains(&k.as_str())) {
            return Err(fail(format!("{bad} is not a footage attribute")));
        }
        let mut attrs = p.file.attrs;
        // An item's label belongs to the project. Imported labels only initialize new items.
        attrs.remove("label");
        let original_rate = attrs.get("frame_rate").cloned();
        let mut stored_file_rate = None;
        if p.keep_interpretation {
            for key in ["alpha_mode", "pixel_aspect", "loop", "color_space"] {
                if let Some(value) = view.value(key) {
                    attrs.insert(key.to_owned(), value.clone());
                }
            }
            if view.value(crate::interpret::FILE_FRAME_RATE).is_some()
                && let (Some(rate), Some(assumed)) = (original_rate, view.value("frame_rate"))
            {
                attrs.insert("frame_rate".into(), assumed.clone());
                stored_file_rate = Some(rate);
            }
        }
        let name = p
            .file
            .name
            .filter(|n| !n.trim().is_empty())
            .or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "Footage".to_owned());
        if bae_model::psd::LayerImage::from_asset(&view).is_some() {
            let styles = child(&p.item, bae_model::layer_styles::PRIM)?;
            crate::effect::delete_all(tx, &[styles, child(&p.item, "masks")?])?;
        }
        if !p.keep_name {
            set(tx, &p.item, "name", Value::String(name))?;
        }
        // What the old file had and the new one does not say goes back to its
        // default (a sequence pattern, a duration of a movie …); the label stays.
        for attr in ATTRS.iter().filter(|a| **a != "label" && **a != "kind") {
            if !attrs.contains_key(*attr) {
                tx.apply(bae_model::Op::SetValue {
                    path: p.item.clone(),
                    attr: (*attr).to_owned(),
                    value: None,
                })?;
            }
        }
        tx.apply(bae_model::Op::SetValue {
            path: p.item.clone(),
            attr: crate::interpret::FILE_FRAME_RATE.into(),
            value: stored_file_rate,
        })?;
        if !p.keep_interpretation {
            tx.apply(bae_model::Op::SetValue {
                path: p.item.clone(),
                attr: "premultiply_color".into(),
                value: None,
            })?;
        }
        if !attrs.contains_key("kind") {
            set(tx, &p.item, "kind", Value::Enum("footage".into()))?;
        }
        for (attr, value) in attrs {
            set(tx, &p.item, &attr, value)?;
        }
        set(
            tx,
            &p.item,
            bae_model::asset::FILE_REVISION,
            Value::Int(tx.stage().revision() as i64 + 1),
        )?;
        for op in file.to_ops(&p.item) {
            tx.apply(op)?;
        }
        Ok(())
    }
}
