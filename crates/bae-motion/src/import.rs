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
use bae_model::{Command, CommandError, PrimPath, PrimType, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
}

/// Output of `item.import`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ImportOutput {
    /// The new footage items, in the order of `files`.
    pub items: Vec<PrimPath>,
}

/// The Asset attributes an importer may set.
const ATTRS: &[&str] = &[
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
];

/// The folder `rel` (`a/b`) under `parent`, created as needed (matched by
/// display name, like the Project panel shows it).
fn folder_path(
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
            items.push(item);
        }
        Ok(ImportOutput { items })
    }
}
