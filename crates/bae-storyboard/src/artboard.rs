//! Artboard commands (Illustrator's Artboards panel and Artboard tool):
//! `artboard.create`, `artboard.update`, `artboard.delete`, `artboard.rearrange`.
//!
//! An artboard's artwork lives in its `layers` folder in artboard coordinates
//! (origin at the artboard's top-left corner), so moving or rearranging artboards
//! moves their artwork with them (Illustrator's "Move Artwork with Artboard").

use bae_model::{Command, CommandError, PrimPath, PrimType, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::{self, ARTBOARDS_FOLDER, attr};
use crate::util::{apply_order, child, create, delete, ensure_folder, fail, path, set};

/// Illustrator's default spacing between artboards, in points (= pixels).
pub const DEFAULT_SPACING: f64 = 20.0;

/// Output of `artboard.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ArtboardOutput {
    /// The new artboard.
    pub path: PrimPath,
    /// Its artwork folder (parent of its layers).
    pub artwork: PrimPath,
}

fn check_size(size: [f64; 2]) -> Result<(), CommandError> {
    if size
        .iter()
        .all(|v| v.is_finite() && *v >= 1.0 && *v <= 30_000.0)
    {
        Ok(())
    } else {
        Err(fail("artboard size must be between 1 and 30000 pixels"))
    }
}

fn check_origin(origin: [f64; 2]) -> Result<(), CommandError> {
    if origin.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(fail("artboard origin must be finite"))
    }
}

fn require_artboard(tx: &Transaction<'_>, path: &PrimPath) -> Result<(), CommandError> {
    if query::artboard_info(tx.stage(), path).is_some() {
        Ok(())
    } else {
        Err(fail(format!("{path} is not an artboard")))
    }
}

// ---------------------------------------------------------------------------

pub struct CreateArtboard;

#[derive(Deserialize, JsonSchema)]
pub struct CreateArtboardParams {
    /// Display name (default the next free "Artboard N", like Illustrator).
    #[serde(default)]
    pub name: Option<String>,
    /// Width and height in pixels (default 1920 × 1080).
    #[serde(default)]
    pub size: Option<[f64; 2]>,
    /// Top-left corner on the canvas in pixels, y down (default: 20 px to the right
    /// of the rightmost artboard, top-aligned with it).
    #[serde(default)]
    pub origin: Option<[f64; 2]>,
    /// Position in the Artboards panel, 0 = first (default last).
    #[serde(default)]
    pub index: Option<usize>,
}

impl Command for CreateArtboard {
    type Params = CreateArtboardParams;
    type Output = ArtboardOutput;
    const ID: &'static str = "artboard.create";
    const TITLE: &'static str = "New Artboard";
    const DESCRIPTION: &'static str = "Create an Illustrator-style artboard (name, size, position on the canvas) with an empty artwork folder `layers` for its shape and text layers.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<ArtboardOutput, CommandError> {
        let size = p.size.unwrap_or([1920.0, 1080.0]);
        check_size(size)?;
        let folder = path(ARTBOARDS_FOLDER);
        ensure_folder(tx, &folder)?;
        let stage = tx.stage();
        let origin = match p.origin {
            Some(o) => o,
            None => query::artboards(stage)
                .iter()
                .filter_map(|a| query::artboard_info(stage, a))
                .max_by(|a, b| a.rect()[2].total_cmp(&b.rect()[2]))
                .map_or([0.0, 0.0], |r| [r.rect()[2] + DEFAULT_SPACING, r.origin[1]]),
        };
        check_origin(origin)?;
        let name = p
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| bae_motion::query::next_numbered_name(stage, &folder, "Artboard"));
        let artboard = child(
            &folder,
            &bae_motion::query::unique_child_name(stage, &folder, &name),
        )?;
        create(tx, &artboard, PrimType::ARTBOARD, p.index)?;
        let artwork = query::artwork_folder(&artboard);
        create(tx, &artwork, PrimType::FOLDER, None)?;
        set(tx, &artboard, attr::NAME, Value::String(name))?;
        set(tx, &artboard, attr::SIZE, Value::Vec2(size))?;
        set(tx, &artboard, attr::ORIGIN, Value::Vec2(origin))?;
        Ok(ArtboardOutput {
            path: artboard,
            artwork,
        })
    }
}

// ---------------------------------------------------------------------------

pub struct UpdateArtboard;

#[derive(Deserialize, JsonSchema)]
pub struct UpdateArtboardParams {
    pub artboard: PrimPath,
    /// New display name.
    #[serde(default)]
    pub name: Option<String>,
    /// New width and height in pixels. The top-left corner stays put.
    #[serde(default)]
    pub size: Option<[f64; 2]>,
    /// New top-left corner on the canvas (the artwork moves with the artboard).
    #[serde(default)]
    pub origin: Option<[f64; 2]>,
}

impl Command for UpdateArtboard {
    type Params = UpdateArtboardParams;
    type Output = ();
    const ID: &'static str = "artboard.update";
    const TITLE: &'static str = "Artboard Options";
    const DESCRIPTION: &'static str =
        "Rename, resize or move an artboard. Omitted fields are left unchanged.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_artboard(tx, &p.artboard)?;
        if let Some(name) = p.name {
            if name.trim().is_empty() {
                return Err(fail("artboard name must not be empty"));
            }
            set(tx, &p.artboard, attr::NAME, Value::String(name))?;
        }
        if let Some(size) = p.size {
            check_size(size)?;
            set(tx, &p.artboard, attr::SIZE, Value::Vec2(size))?;
        }
        if let Some(origin) = p.origin {
            check_origin(origin)?;
            set(tx, &p.artboard, attr::ORIGIN, Value::Vec2(origin))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct DeleteArtboards;

#[derive(Deserialize, JsonSchema)]
pub struct DeleteArtboardsParams {
    /// Artboards to delete, with their artwork.
    pub artboards: Vec<PrimPath>,
}

impl Command for DeleteArtboards {
    type Params = DeleteArtboardsParams;
    type Output = ();
    const ID: &'static str = "artboard.delete";
    const TITLE: &'static str = "Delete Artboard";
    const DESCRIPTION: &'static str = "Delete artboards and the artwork on them. Panels that used them as their picture show a missing picture.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for a in &p.artboards {
            require_artboard(tx, a)?;
        }
        for a in &p.artboards {
            if tx.stage().has_prim(a) {
                delete(tx, a)?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

/// Layouts of Illustrator's Rearrange All Artboards dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtboardLayout {
    /// Rows of `columns` artboards, filled left to right then top to bottom.
    #[default]
    GridByRow,
    /// Columns of `rows` artboards, filled top to bottom then left to right.
    GridByColumn,
    /// One row.
    Row,
    /// One column.
    Column,
}

pub struct RearrangeArtboards;

#[derive(Deserialize, JsonSchema)]
pub struct RearrangeArtboardsParams {
    /// Layout (default grid_by_row).
    #[serde(default)]
    pub layout: ArtboardLayout,
    /// Artboards per row (grid_by_row) or per column (grid_by_column). Default: about
    /// the square root of the artboard count.
    #[serde(default)]
    pub columns: Option<u32>,
    /// Gap between artboards in pixels (default 20, Illustrator's default).
    #[serde(default)]
    pub spacing: Option<f64>,
    /// Lay rows out right to left (Illustrator's "Change to Right-to-Left Layout").
    #[serde(default)]
    pub right_to_left: bool,
    /// New Artboards panel order (default the current order). Artboards not listed
    /// follow in their current order.
    #[serde(default)]
    pub order: Option<Vec<PrimPath>>,
}

impl Command for RearrangeArtboards {
    type Params = RearrangeArtboardsParams;
    type Output = ();
    const ID: &'static str = "artboard.rearrange";
    const TITLE: &'static str = "Rearrange All Artboards";
    const DESCRIPTION: &'static str = "Lay all artboards out in a grid, row or column with even spacing (Illustrator: Object > Artboards > Rearrange All). Artwork moves with its artboard. Optionally reorders the Artboards panel first.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let spacing = p.spacing.unwrap_or(DEFAULT_SPACING);
        if !(spacing.is_finite() && spacing >= 0.0) {
            return Err(fail("spacing must be zero or more"));
        }
        let folder = path(ARTBOARDS_FOLDER);
        if let Some(order) = &p.order {
            for a in order {
                require_artboard(tx, a)?;
            }
            apply_order(tx, &folder, order)?;
        }
        let stage = tx.stage();
        let boards: Vec<_> = query::artboards(stage)
            .iter()
            .filter_map(|a| query::artboard_info(stage, a))
            .collect();
        let n = boards.len();
        if n == 0 {
            return Ok(());
        }
        let per_line = match p.layout {
            ArtboardLayout::Row => n,
            ArtboardLayout::Column => 1,
            ArtboardLayout::GridByRow | ArtboardLayout::GridByColumn => {
                let auto = (n as f64).sqrt().ceil() as usize;
                p.columns.map_or(auto, |c| c as usize).clamp(1, n)
            }
        };
        // (column, row) of every artboard.
        let cell = |i: usize| match p.layout {
            ArtboardLayout::GridByColumn => (i / per_line, i % per_line),
            ArtboardLayout::Column => (0, i),
            _ => (i % per_line, i / per_line),
        };
        let cols = (0..n).map(|i| cell(i).0).max().unwrap_or(0) + 1;
        let rows = (0..n).map(|i| cell(i).1).max().unwrap_or(0) + 1;
        let mut widths = vec![0.0_f64; cols];
        let mut heights = vec![0.0_f64; rows];
        for (i, b) in boards.iter().enumerate() {
            let (c, r) = cell(i);
            widths[c] = widths[c].max(b.size[0]);
            heights[r] = heights[r].max(b.size[1]);
        }
        // The layout starts at the top-left corner of all artboards' bounds.
        let anchor = boards.iter().fold([f64::MAX, f64::MAX], |a, b| {
            [a[0].min(b.origin[0]), a[1].min(b.origin[1])]
        });
        let mut xs = vec![0.0; cols];
        for c in 1..cols {
            xs[c] = xs[c - 1] + widths[c - 1] + spacing;
        }
        let total_width = xs[cols - 1] + widths[cols - 1];
        let mut ys = vec![0.0; rows];
        for r in 1..rows {
            ys[r] = ys[r - 1] + heights[r - 1] + spacing;
        }
        let targets: Vec<(PrimPath, [f64; 2])> = boards
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let (c, r) = cell(i);
                let x = if p.right_to_left {
                    // Mirror the columns, right-aligning each artboard in its column.
                    total_width - xs[c] - b.size[0]
                } else {
                    xs[c]
                };
                (b.path.clone(), [anchor[0] + x, anchor[1] + ys[r]])
            })
            .collect();
        for (a, origin) in targets {
            let current = query::artboard_info(tx.stage(), &a).map(|i| i.origin);
            if current != Some(origin) {
                set(tx, &a, attr::ORIGIN, Value::Vec2(origin))?;
            }
        }
        Ok(())
    }
}
