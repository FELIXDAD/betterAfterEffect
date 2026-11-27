//! File ▸ Dependencies: Remove Unused Footage, Consolidate All Footage and
//! Reduce Project. Each is one undo step and reports how many project items it
//! removed (AE says so in an alert).

use std::collections::{HashMap, HashSet};

use bae_model::{Command, CommandError, PrimPath, PrimType, Stage, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::layer::{DeleteItems, DeleteItemsParams};
use crate::query;
use crate::util::{fail, set};

/// How many project items a clean-up removed.
#[derive(Serialize, JsonSchema)]
pub struct CleanUpOutput {
    pub removed: usize,
}

/// Project items (comps, footage, solids, folders): not the prims inside comps.
fn items(stage: &Stage) -> Vec<PrimPath> {
    let mut comps = Vec::new();
    let mut out = Vec::new();
    for p in stage.traverse() {
        if comps.iter().any(|c: &PrimPath| c.is_ancestor_of(&p)) {
            continue;
        }
        let Some(v) = stage.prim(&p) else { continue };
        if v.is_a(&PrimType::COMP) {
            comps.push(p.clone());
            out.push(p);
        } else if v.is_a(&PrimType::ASSET) || v.is_a(&PrimType::FOLDER) {
            out.push(p);
        }
    }
    out
}

fn is_asset(stage: &Stage, p: &PrimPath) -> bool {
    stage.prim(p).is_some_and(|v| v.is_a(&PrimType::ASSET))
}

/// The item a layer shows (footage, solid, pre-comp).
fn source(stage: &Stage, layer: &PrimPath) -> Option<PrimPath> {
    match stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, "source"))
    {
        Some(Value::PrimRef(p)) => Some(p),
        _ => None,
    }
}

/// Every layer of every comp with its source item.
fn layer_sources(stage: &Stage) -> Vec<(PrimPath, PrimPath)> {
    query::comps(stage)
        .into_iter()
        .flat_map(|c| query::comp_layers(stage, &c))
        .filter_map(|l| source(stage, &l).map(|s| (l, s)))
        .collect()
}

/// Delete `paths` (never one inside another that goes too) in one step.
fn delete(tx: &mut Transaction<'_>, mut paths: Vec<PrimPath>) -> Result<usize, CommandError> {
    paths.sort_by_key(|p| p.as_str().len());
    let mut top: Vec<PrimPath> = Vec::new();
    for p in paths {
        if !top.iter().any(|t| t.is_prefix_of(&p)) {
            top.push(p);
        }
    }
    let n = top.len();
    if n > 0 {
        DeleteItems.run(tx, DeleteItemsParams { paths: top })?;
    }
    Ok(n)
}

// ---------------------------------------------------------------------------

/// The footage and solid items no layer uses (Remove Unused Footage).
pub fn unused_footage(stage: &Stage) -> Vec<PrimPath> {
    let used: HashSet<PrimPath> = layer_sources(stage).into_iter().map(|(_, s)| s).collect();
    items(stage)
        .into_iter()
        .filter(|p| is_asset(stage, p) && !used.contains(p))
        .collect()
}

pub struct RemoveUnusedFootage;

#[derive(Deserialize, JsonSchema)]
pub struct NoParams {}

impl Command for RemoveUnusedFootage {
    type Params = NoParams;
    type Output = CleanUpOutput;
    const ID: &'static str = "item.remove_unused";
    const TITLE: &'static str = "Remove Unused Footage";
    const DESCRIPTION: &'static str = "File > Dependencies > Remove Unused Footage: delete the footage and solid items no layer uses.";

    fn run(&self, tx: &mut Transaction<'_>, _: NoParams) -> Result<CleanUpOutput, CommandError> {
        let unused = unused_footage(tx.stage());
        Ok(CleanUpOutput {
            removed: delete(tx, unused)?,
        })
    }
}

// ---------------------------------------------------------------------------

pub struct ConsolidateFootage;

/// What makes two footage items the same: the file and how it is interpreted,
/// or a solid's name, size and color.
fn identity(stage: &Stage, item: &PrimPath) -> Option<String> {
    let v = stage.prim(item)?;
    let get = |a: &str| format!("{:?}", query::static_value(&v, a));
    let kind = get("kind");
    let file = bae_model::asset::AssetFile::from_prim(&v).map(|f| f.original);
    let solid = matches!(query::static_value(&v, "kind"), Some(Value::Enum(k)) if k == "solid");
    if !solid && file.is_none() {
        return None;
    }
    let mut key = format!("{kind}|{file:?}");
    let attrs: &[&str] = if solid {
        &["name", "width", "height", "pixel_aspect", "solid_color"]
    } else {
        &[
            "alpha_mode",
            "premultiply_color",
            "frame_rate",
            "pixel_aspect",
            "loop",
        ]
    };
    for a in attrs {
        key.push('|');
        key.push_str(&get(a));
    }
    Some(key)
}

/// Each duplicate footage item and the first item it duplicates (Consolidate
/// All Footage).
pub fn duplicate_footage(stage: &Stage) -> HashMap<PrimPath, PrimPath> {
    let mut first: HashMap<String, PrimPath> = HashMap::new();
    let mut keeper: HashMap<PrimPath, PrimPath> = HashMap::new();
    for item in items(stage).into_iter().filter(|p| is_asset(stage, p)) {
        if let Some(key) = identity(stage, &item) {
            match first.get(&key) {
                Some(k) => {
                    keeper.insert(item, k.clone());
                }
                None => {
                    first.insert(key, item);
                }
            }
        }
    }
    keeper
}

impl Command for ConsolidateFootage {
    type Params = NoParams;
    type Output = CleanUpOutput;
    const ID: &'static str = "item.consolidate";
    const TITLE: &'static str = "Consolidate All Footage";
    const DESCRIPTION: &'static str = "File > Dependencies > Consolidate All Footage: footage items of the same file with the same interpretation (and identical solids) become one; their layers use the first.";

    fn run(&self, tx: &mut Transaction<'_>, _: NoParams) -> Result<CleanUpOutput, CommandError> {
        let stage = tx.stage();
        let keeper = duplicate_footage(stage);
        let moves: Vec<(PrimPath, PrimPath)> = layer_sources(stage)
            .into_iter()
            .filter_map(|(layer, s)| keeper.get(&s).map(|k| (layer, k.clone())))
            .collect();
        tx.allow_locked(|tx| -> Result<(), CommandError> {
            for (layer, k) in moves {
                set(tx, &layer, "source", Value::PrimRef(k))?;
            }
            Ok(())
        })?;
        Ok(CleanUpOutput {
            removed: delete(tx, keeper.into_keys().collect())?,
        })
    }
}

// ---------------------------------------------------------------------------

/// The comps and footage `keep_items` do not use, directly or through nested
/// comps (Reduce Project; folders stay).
pub fn unused_by(stage: &Stage, keep_items: &[PrimPath]) -> Vec<PrimPath> {
    let mut keep: HashSet<PrimPath> = HashSet::new();
    let mut todo = keep_items.to_vec();
    while let Some(item) = todo.pop() {
        if !keep.insert(item.clone()) {
            continue;
        }
        if query::comp_info(stage, &item).is_some() {
            todo.extend(
                query::comp_layers(stage, &item)
                    .iter()
                    .filter_map(|l| source(stage, l)),
            );
        }
        // A kept folder keeps what is in it.
        if stage.prim(&item).is_some_and(|v| v.is_a(&PrimType::FOLDER)) {
            todo.extend(
                items(stage)
                    .into_iter()
                    .filter(|c| c.parent().as_ref() == Some(&item)),
            );
        }
    }
    // Comps and footage go; folders stay (AE).
    items(stage)
        .into_iter()
        .filter(|p| !keep.contains(p))
        .filter(|p| !stage.prim(p).is_some_and(|v| v.is_a(&PrimType::FOLDER)))
        .collect()
}

pub struct ReduceProject;

#[derive(Deserialize, JsonSchema)]
pub struct ReduceProjectParams {
    /// The items to keep (with everything they use).
    pub items: Vec<PrimPath>,
}

impl Command for ReduceProject {
    type Params = ReduceProjectParams;
    type Output = CleanUpOutput;
    const ID: &'static str = "item.reduce";
    const TITLE: &'static str = "Reduce Project";
    const DESCRIPTION: &'static str = "File > Dependencies > Reduce Project: delete every comp and footage item the given items do not use, directly or through nested comps (folders stay).";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: ReduceProjectParams,
    ) -> Result<CleanUpOutput, CommandError> {
        if p.items.is_empty() {
            return Err(fail("select the items to keep"));
        }
        let gone = unused_by(tx.stage(), &p.items);
        Ok(CleanUpOutput {
            removed: delete(tx, gone)?,
        })
    }
}
