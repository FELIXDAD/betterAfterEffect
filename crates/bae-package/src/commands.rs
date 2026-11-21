//! Packaging commands (ARCHITECTURE §2: everything the UI can do, agents can do).
//!
//! | ID | What |
//! |---|---|
//! | `package.scan` | Report what packing would do (assets used/unused/missing, fonts, sizes) |
//! | `package.pack` | Write a self-contained `.baez` |
//! | `package.collect` | Collect the project and its files into a `.bae` folder |
//! | `package.unpack` | Unpack a `.baez` into a `.bae` folder |
//! | `asset.relink` | Relink a missing asset and find the other missing ones the same way |
//! | `asset.set_files` | Set the file references of several assets in one step |
//!
//! Commands only see the stage, so the project's location (for relative paths
//! and embedded assets) and its path mappings come in as parameters. The packing
//! commands do not modify the open project.

use std::path::PathBuf;

use bae_model::{
    AssetFile, Command, CommandError, CommandRegistry, ModelError, Op, PathMapping, PrimPath,
    PrimType, SublayerId, Transaction,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::pack::{self, PackOptions, PackSource, UnpackReport, WriteOptions};
use crate::report::PackageReport;
use crate::resolve::{AssetLocator, Relink, RelinkPlan, plan_relink};

/// Register the packaging commands.
pub fn register(r: &mut CommandRegistry) {
    let results = [
        r.register(Scan),
        r.register(Pack),
        r.register(Collect),
        r.register(Unpack),
        r.register(RelinkAsset),
        r.register(SetAssetFiles),
    ];
    for res in results {
        res.expect("package command IDs are unique");
    }
}

/// Write a file reference onto an asset prim.
///
/// Each attribute is written where its winning opinion lives, so the composed
/// result is the new reference without leaving overrides in unrelated
/// sublayers; attributes nobody has an opinion about go where `file:original`
/// lives (or the edit target). `None` fields are cleared in every sublayer.
pub fn write_asset_file(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    file: &AssetFile,
) -> Result<(), CommandError> {
    let view = tx.stage().require_prim(path)?;
    if !view.is_a(&PrimType::ASSET) {
        return Err(CommandError::Failed(format!("{path} is not an Asset prim")));
    }
    if path.is_root() {
        return Err(ModelError::RootNotEditable.into());
    }
    let local = |attr: &str| -> Vec<SublayerId> {
        view.opinions()
            .iter()
            .filter(|o| !o.is_referenced(path) && o.spec.attrs.contains_key(attr))
            .map(|o| o.sublayer().clone())
            .collect()
    };
    let home = local(bae_model::asset::attrs::ORIGINAL)
        .into_iter()
        .next()
        .unwrap_or_else(|| tx.target().clone());
    let mut writes: Vec<(SublayerId, Op)> = Vec::new();
    for op in file.to_ops(path) {
        let Op::SetValue { attr, value, .. } = &op else {
            continue;
        };
        let holders = local(attr);
        match value {
            Some(_) => {
                let dest = holders.first().cloned().unwrap_or_else(|| home.clone());
                writes.push((dest, op));
            }
            None => {
                for h in holders {
                    writes.push((h, op.clone()));
                }
            }
        }
    }
    for (sublayer, op) in writes {
        tx.ensure_spec_in(&sublayer, path)?;
        tx.apply_to(&sublayer, op)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------

/// `asset.set_files`.
pub struct SetAssetFiles;

#[derive(Deserialize, JsonSchema)]
pub struct SetAssetFilesParams {
    /// Asset prims and their new file references.
    pub assets: Vec<Relink>,
}

impl Command for SetAssetFiles {
    type Params = SetAssetFilesParams;
    type Output = ();
    const ID: &'static str = "asset.set_files";
    const TITLE: &'static str = "Relink Assets";
    const DESCRIPTION: &'static str = "Set the file references (relative path, original path, fingerprint) of several Asset prims as one undo step. Each value is written in the sublayer whose opinion currently wins.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for r in &p.assets {
            write_asset_file(tx, &r.path, &r.file)?;
        }
        Ok(())
    }
}

/// `asset.relink`.
pub struct RelinkAsset;

#[derive(Deserialize, JsonSchema)]
pub struct RelinkAssetParams {
    /// The Asset prim to relink.
    pub path: PrimPath,
    /// The file the user found for it (absolute path).
    pub file: PathBuf,
    /// The open project's location (.bae folder or .baez file), for relative
    /// paths. Omit for an unsaved project.
    #[serde(default)]
    pub project: Option<PathBuf>,
    /// Also relink the other missing assets: through the same directory change
    /// (e.g. Z:/ → /Volumes/NAS/) or found in the same folder. Default true.
    #[serde(default = "yes")]
    pub find_others: bool,
    /// The project's path mappings.
    #[serde(default)]
    pub path_mappings: Vec<PathMapping>,
}

fn yes() -> bool {
    true
}

impl Command for RelinkAsset {
    type Params = RelinkAssetParams;
    type Output = RelinkPlan;
    const ID: &'static str = "asset.relink";
    const TITLE: &'static str = "Relink Asset";
    const DESCRIPTION: &'static str = "Relink an asset to a file on disk; with find_others, also relink the other missing assets found through the same directory change or in the same folder. Returns what was relinked, the implied path mapping and what is still missing.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<RelinkPlan, CommandError> {
        let locator = AssetLocator::for_location(p.project.as_deref(), p.path_mappings)?;
        let plan = plan_relink(tx.stage(), &locator, &p.path, &p.file, p.find_others)?;
        for r in &plan.relinks {
            write_asset_file(tx, &r.path, &r.file)?;
        }
        Ok(plan)
    }
}

/// Parameters shared by the packing commands.
#[derive(Deserialize, JsonSchema)]
pub struct PackParams {
    /// The open project's location (.bae folder or .baez file): relative asset
    /// paths resolve against it and assets embedded in a .baez are read from
    /// it. Omit for an unsaved project.
    #[serde(default)]
    pub project: Option<PathBuf>,
    #[serde(flatten)]
    pub options: PackOptions,
}

/// `package.scan`.
pub struct Scan;

impl Command for Scan {
    type Params = PackParams;
    type Output = PackageReport;
    const ID: &'static str = "package.scan";
    const TITLE: &'static str = "Check Project Assets";
    const DESCRIPTION: &'static str = "Report what packing would do without writing anything: every asset (used or unused, found or missing, where it was found, size), fonts used, referenced projects and totals.";

    fn run(&self, tx: &mut Transaction<'_>, p: PackParams) -> Result<PackageReport, CommandError> {
        let manifest = p.project.as_deref().and_then(pack::read_manifest);
        let source = PackSource {
            manifest: manifest.as_ref(),
            ..PackSource::stage(tx.stage(), p.project.as_deref())
        };
        Ok(pack::scan(&source, &p.options)?)
    }
}

/// `package.pack`.
pub struct Pack;

#[derive(Deserialize, JsonSchema)]
pub struct PackOutputParams {
    /// The .baez file to write.
    pub output: PathBuf,
    /// Replace an existing file at `output`.
    #[serde(default)]
    pub overwrite: bool,
    #[serde(flatten)]
    pub pack: PackParams,
}

impl Command for Pack {
    type Params = PackOutputParams;
    type Output = PackageReport;
    const ID: &'static str = "package.pack";
    const TITLE: &'static str = "Package Project";
    const DESCRIPTION: &'static str = "Write the project and every asset file it uses into one self-contained .baez file (media stored uncompressed, project data compressed). The open project and the original files are not changed. Returns a report with missing assets and sizes.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: PackOutputParams,
    ) -> Result<PackageReport, CommandError> {
        let manifest = p.pack.project.as_deref().and_then(pack::read_manifest);
        let source = PackSource {
            manifest: manifest.as_ref(),
            ..PackSource::stage(tx.stage(), p.pack.project.as_deref())
        };
        Ok(pack::pack(
            &source,
            &p.output,
            &p.pack.options,
            WriteOptions {
                overwrite: p.overwrite,
            },
        )?)
    }
}

/// `package.collect`.
pub struct Collect;

#[derive(Deserialize, JsonSchema)]
pub struct CollectParams {
    /// The folder to write (a .bae project folder); must not exist or be empty.
    pub output: PathBuf,
    #[serde(flatten)]
    pub pack: PackParams,
}

impl Command for Collect {
    type Params = CollectParams;
    type Output = PackageReport;
    const ID: &'static str = "package.collect";
    const TITLE: &'static str = "Collect Files";
    const DESCRIPTION: &'static str = "Copy the project and every asset file it uses into a new .bae folder (assets under assets/), like AE's Collect Files. The open project and the original files are not changed.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: CollectParams,
    ) -> Result<PackageReport, CommandError> {
        let manifest = p.pack.project.as_deref().and_then(pack::read_manifest);
        let source = PackSource {
            manifest: manifest.as_ref(),
            ..PackSource::stage(tx.stage(), p.pack.project.as_deref())
        };
        Ok(pack::collect_files(&source, &p.output, &p.pack.options)?)
    }
}

/// `package.unpack`.
pub struct Unpack;

#[derive(Deserialize, JsonSchema)]
pub struct UnpackParams {
    /// The .baez file.
    pub package: PathBuf,
    /// The .bae folder to create; must not exist or be empty.
    pub output: PathBuf,
}

impl Command for Unpack {
    type Params = UnpackParams;
    type Output = UnpackReport;
    const ID: &'static str = "package.unpack";
    const TITLE: &'static str = "Unpack Project";
    const DESCRIPTION: &'static str = "Unpack a .baez into a normal .bae project folder and check that every asset resolves. Does not change the open project.";

    fn run(
        &self,
        _tx: &mut Transaction<'_>,
        p: UnpackParams,
    ) -> Result<UnpackReport, CommandError> {
        Ok(pack::unpack(&p.package, &p.output)?)
    }
}
