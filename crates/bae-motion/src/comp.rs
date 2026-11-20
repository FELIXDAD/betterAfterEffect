//! Comp and project-panel commands: `comp.create`, `comp.set_work_area`,
//! `comp.trim_to_work_area`, `folder.create`.

use bae_model::{Command, CommandError, FrameRate, PrimPath, PrimType, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::{self, COMPS_FOLDER};
use crate::util::{child, create, ensure_folder, fail, set};

/// Output of commands that create a comp.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CompOutput {
    /// The new comp.
    pub path: PrimPath,
    /// Its `layers` folder (parent of the comp's layers).
    pub layers: PrimPath,
}

// ---------------------------------------------------------------------------

pub struct CreateComp;

fn default_width() -> u32 {
    1920
}
fn default_height() -> u32 {
    1080
}
fn default_fps() -> f64 {
    30.0
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateCompParams {
    /// Display name, e.g. "Comp 1" (default: the next free "Comp N").
    #[serde(default)]
    pub name: Option<String>,
    /// Folder to create the comp in (default /comps, created if missing).
    #[serde(default)]
    pub parent: Option<PrimPath>,
    /// Width in pixels (default 1920).
    #[serde(default = "default_width")]
    pub width: u32,
    /// Height in pixels (default 1080).
    #[serde(default = "default_height")]
    pub height: u32,
    /// Frames per second: 23.976, 24, 25, 29.97, 30, 50, 59.94, 60 … (default 30).
    #[serde(default = "default_fps")]
    pub fps: f64,
    /// Duration in flicks (705600000 per second); default 10 seconds.
    #[serde(default)]
    pub duration: Option<Time>,
    /// Background color, straight RGBA 0–1 (default black).
    #[serde(default)]
    pub bg_color: Option<[f64; 4]>,
    /// Pixel aspect ratio (default 1).
    #[serde(default)]
    pub pixel_aspect: Option<f64>,
}

impl Command for CreateComp {
    type Params = CreateCompParams;
    type Output = CompOutput;
    const ID: &'static str = "comp.create";
    const TITLE: &'static str = "New Composition";
    const DESCRIPTION: &'static str = "Create a composition (size, frame rate, duration, background color) with an empty layers folder.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<CompOutput, CommandError> {
        if p.width == 0 || p.height == 0 || p.width > 30_000 || p.height > 30_000 {
            return Err(fail("comp size must be between 1 and 30000 pixels"));
        }
        if !(p.fps.is_finite() && p.fps > 0.0 && p.fps <= 999.0) {
            return Err(fail("fps must be between 0 and 999"));
        }
        let parent = match p.parent {
            Some(parent) => {
                tx.stage().require_prim(&parent)?;
                parent
            }
            None => {
                let folder = PrimPath::new(COMPS_FOLDER).expect("valid");
                ensure_folder(tx, &folder)?;
                folder
            }
        };
        let name = p
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| next_comp_name(tx));
        let path = child(
            &parent,
            &query::unique_child_name(tx.stage(), &parent, &name),
        )?;
        create(tx, &path, PrimType::COMP, None)?;
        let layers = child(&path, query::LAYERS)?;
        create(tx, &layers, PrimType::FOLDER, None)?;
        set(tx, &path, "name", Value::String(name))?;
        set(tx, &path, "width", Value::Int(i64::from(p.width)))?;
        set(tx, &path, "height", Value::Int(i64::from(p.height)))?;
        set(
            tx,
            &path,
            "frame_rate",
            Value::FrameRate(FrameRate::from_fps(p.fps)),
        )?;
        let duration = p.duration.unwrap_or(Time::from_seconds(10.0));
        if duration.0 <= 0 {
            return Err(fail("duration must be positive"));
        }
        set(tx, &path, "duration", Value::Time(duration))?;
        if let Some(c) = p.bg_color {
            set(tx, &path, "bg_color", Value::Color(c))?;
        }
        if let Some(a) = p.pixel_aspect.filter(|a| *a > 0.0) {
            set(tx, &path, "pixel_aspect", Value::Float(a))?;
        }
        Ok(CompOutput { path, layers })
    }
}

fn next_comp_name(tx: &Transaction<'_>) -> String {
    // AE numbers comps project-wide.
    let stage = tx.stage();
    let max = query::comps(stage)
        .iter()
        .filter_map(|c| {
            query::display_name_of(stage, c)
                .strip_prefix("Comp ")?
                .parse::<u32>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    format!("Comp {}", max + 1)
}

// ---------------------------------------------------------------------------

pub struct SetWorkArea;

#[derive(Deserialize, JsonSchema)]
pub struct SetWorkAreaParams {
    pub comp: PrimPath,
    /// New start of the work area (comp time, flicks); omitted keeps the current start.
    #[serde(default)]
    pub start: Option<Time>,
    /// New end of the work area (comp time, flicks, exclusive); omitted keeps the current end.
    #[serde(default)]
    pub end: Option<Time>,
}

impl Command for SetWorkArea {
    type Params = SetWorkAreaParams;
    type Output = ();
    const ID: &'static str = "comp.set_work_area";
    const TITLE: &'static str = "Set Work Area";
    const DESCRIPTION: &'static str = "Set the start and/or end of a comp's work area (AE: B / N keys). Times are snapped to frames and kept inside the comp, at least one frame long.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let info = query::comp_info(tx.stage(), &p.comp)
            .ok_or_else(|| fail(format!("{} is not a comp", p.comp)))?;
        let rate = info.frame_rate;
        let frame = rate.frame_duration();
        let snap = |t: Time| rate.frame_to_time(rate.time_to_frame(t + Time(frame.0 / 2)));
        let mut start = p.start.map_or(info.work_area.start, snap);
        let mut end = p.end.map_or(info.work_area.end, snap);
        start = start.clamp(Time::ZERO, info.duration);
        end = end.clamp(Time::ZERO, info.duration);
        // AE: moving the start past the end drags the end along (and vice versa).
        if end <= start {
            if p.start.is_some() && p.end.is_none() {
                end = (start + frame).min(info.duration);
                start = start.min(end - frame);
            } else {
                start = (end - frame).max(Time::ZERO);
                end = end.max(start + frame);
            }
        }
        set(tx, &p.comp, "work_area_start", Value::Time(start))?;
        set(tx, &p.comp, "work_area_duration", Value::Time(end - start))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct TrimCompToWorkArea;

#[derive(Deserialize, JsonSchema)]
pub struct TrimCompParams {
    pub comp: PrimPath,
}

impl Command for TrimCompToWorkArea {
    type Params = TrimCompParams;
    type Output = ();
    const ID: &'static str = "comp.trim_to_work_area";
    const TITLE: &'static str = "Trim Comp to Work Area";
    const DESCRIPTION: &'static str = "Make the work area the whole comp: the duration becomes the work area's and every layer moves earlier by the work area start.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let info = query::comp_info(tx.stage(), &p.comp)
            .ok_or_else(|| fail(format!("{} is not a comp", p.comp)))?;
        let shift = info.work_area.start;
        let duration = info.work_area.duration();
        if shift != Time::ZERO {
            for layer in query::comp_layers(tx.stage(), &p.comp) {
                let timing = query::layer_timing(tx.stage(), &layer);
                set(
                    tx,
                    &layer,
                    "start_time",
                    Value::Time(timing.start_time - shift),
                )?;
                set(tx, &layer, "in_point", Value::Time(timing.in_point - shift))?;
                let has_out = tx
                    .stage()
                    .prim(&layer)
                    .is_some_and(|v| v.value("out_point").is_some());
                if has_out {
                    set(
                        tx,
                        &layer,
                        "out_point",
                        Value::Time(timing.out_point - shift),
                    )?;
                }
            }
        }
        set(tx, &p.comp, "duration", Value::Time(duration))?;
        set(tx, &p.comp, "work_area_start", Value::Time(Time::ZERO))?;
        set(tx, &p.comp, "work_area_duration", Value::Time(duration))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct CreateFolder;

#[derive(Deserialize, JsonSchema)]
pub struct CreateFolderParams {
    /// Display name (default "Untitled N").
    #[serde(default)]
    pub name: Option<String>,
    /// Parent folder (default the project root).
    #[serde(default)]
    pub parent: Option<PrimPath>,
}

/// Output of commands that create one prim.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PathOutput {
    pub path: PrimPath,
}

impl Command for CreateFolder {
    type Params = CreateFolderParams;
    type Output = PathOutput;
    const ID: &'static str = "folder.create";
    const TITLE: &'static str = "New Folder";
    const DESCRIPTION: &'static str = "Create a project panel folder.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<PathOutput, CommandError> {
        let parent = p.parent.unwrap_or_else(PrimPath::root);
        tx.stage().require_prim(&parent)?;
        let name = p
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| query::next_numbered_name(tx.stage(), &parent, "Untitled"));
        let path = child(
            &parent,
            &query::unique_child_name(tx.stage(), &parent, &name),
        )?;
        create(tx, &path, PrimType::FOLDER, None)?;
        set(tx, &path, "name", Value::String(name))?;
        Ok(PathOutput { path })
    }
}
