//! Comp and project-panel commands: `comp.create`, `comp.set_settings`,
//! `comp.set_work_area`, `comp.trim_to_work_area`, `folder.create`.

use bae_model::{
    AttrSpec, Command, CommandError, FrameRate, PrimPath, PrimType, Time, Transaction, Value,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::{self, COMPS_FOLDER};
use crate::util::{child, create, ensure_folder, fail, set, write_attr};

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
    /// The rest of AE's Composition Settings (see `comp.set_settings`).
    #[serde(default)]
    pub drop_frame: Option<bool>,
    #[serde(default)]
    pub resolution: Option<[u32; 2]>,
    #[serde(default)]
    pub start_timecode: Option<Time>,
    #[serde(default)]
    pub shutter_angle: Option<f64>,
    #[serde(default)]
    pub shutter_phase: Option<f64>,
    #[serde(default)]
    pub samples_per_frame: Option<i64>,
    #[serde(default)]
    pub adaptive_sample_limit: Option<i64>,
    #[serde(default)]
    pub preserve_frame_rate: Option<bool>,
    #[serde(default)]
    pub preserve_resolution: Option<bool>,
    #[serde(default)]
    pub renderer: Option<String>,
    /// Label color index (the UI passes the Preferences > Labels default).
    #[serde(default)]
    pub label: Option<i64>,
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
        if let Some(l) = p.label.filter(|l| (0..=16).contains(l)) {
            set(tx, &path, "label", Value::Int(l))?;
        }
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
        let rest = CompSettingsParams {
            comp: path.clone(),
            name: None,
            width: None,
            height: None,
            anchor: None,
            pixel_aspect: None,
            fps: None,
            drop_frame: p.drop_frame,
            resolution: p.resolution,
            start_timecode: p.start_timecode,
            duration: None,
            bg_color: None,
            shutter_angle: p.shutter_angle,
            shutter_phase: p.shutter_phase,
            samples_per_frame: p.samples_per_frame,
            adaptive_sample_limit: p.adaptive_sample_limit,
            preserve_frame_rate: p.preserve_frame_rate,
            preserve_resolution: p.preserve_resolution,
            renderer: p.renderer,
        };
        SetCompSettings.run(tx, rest)?;
        Ok(CompOutput { path, layers })
    }
}

fn next_comp_name(tx: &Transaction<'_>) -> String {
    // AE numbers comps project-wide.
    let stage = tx.stage();
    let base = crate::names::localized("Comp");
    let max = query::comps(stage)
        .iter()
        .filter_map(|c| {
            query::display_name_of(stage, c)
                .strip_prefix(base)?
                .trim_start()
                .parse::<u32>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    format!("{base} {}", max + 1)
}

// ---------------------------------------------------------------------------

/// Where layers stay when a comp is resized (AE Composition Settings > Advanced
/// > Anchor): the point of the frame that keeps its place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResizeAnchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl ResizeAnchor {
    /// Fraction of the size change layers move by, per axis.
    pub fn factor(self) -> [f64; 2] {
        use ResizeAnchor::*;
        let x = match self {
            TopLeft | Left | BottomLeft => 0.0,
            Top | Center | Bottom => 0.5,
            TopRight | Right | BottomRight => 1.0,
        };
        let y = match self {
            TopLeft | Top | TopRight => 0.0,
            Left | Center | Right => 0.5,
            BottomLeft | Bottom | BottomRight => 1.0,
        };
        [x, y]
    }
}

pub struct SetCompSettings;

#[derive(Deserialize, JsonSchema)]
pub struct CompSettingsParams {
    pub comp: PrimPath,
    /// Display name (the comp keeps its path).
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    /// Point that keeps its place when the size changes (default center):
    /// unparented layers move with it, keyframes included.
    #[serde(default)]
    pub anchor: Option<ResizeAnchor>,
    #[serde(default)]
    pub pixel_aspect: Option<f64>,
    /// Frames per second (29.97 → 30000/1001). Keyframes keep their times.
    #[serde(default)]
    pub fps: Option<f64>,
    /// Drop-frame timecode (29.97 / 59.94 only).
    #[serde(default)]
    pub drop_frame: Option<bool>,
    /// Resolution as [horizontal, vertical] pixel step: [1,1] Full, [2,2] Half …
    #[serde(default)]
    pub resolution: Option<[u32; 2]>,
    /// Timecode of comp time 0 (flicks).
    #[serde(default)]
    pub start_timecode: Option<Time>,
    /// Duration (flicks). The work area is clamped to it.
    #[serde(default)]
    pub duration: Option<Time>,
    /// Straight RGBA 0–1.
    #[serde(default)]
    pub bg_color: Option<[f64; 4]>,
    /// Motion blur shutter angle, 0–720 degrees.
    #[serde(default)]
    pub shutter_angle: Option<f64>,
    /// Motion blur shutter phase, -360–360 degrees.
    #[serde(default)]
    pub shutter_phase: Option<f64>,
    /// Motion blur samples per frame (2D layers), 2–64.
    #[serde(default)]
    pub samples_per_frame: Option<i64>,
    /// Motion blur adaptive sample limit, 64–256.
    #[serde(default)]
    pub adaptive_sample_limit: Option<i64>,
    #[serde(default)]
    pub preserve_frame_rate: Option<bool>,
    #[serde(default)]
    pub preserve_resolution: Option<bool>,
    /// classic_3d, cinema_4d or advanced_3d.
    #[serde(default)]
    pub renderer: Option<String>,
}

/// Offset the x / y of a position value.
fn offset_point(v: &Value, d: [f64; 2]) -> Value {
    match v {
        Value::Vec2([x, y]) => Value::Vec2([x + d[0], y + d[1]]),
        Value::Vec3([x, y, z]) => Value::Vec3([x + d[0], y + d[1], *z]),
        Value::Float(f) => Value::Float(f + d[0]),
        other => other.clone(),
    }
}

/// Move an attribute (static value and every keyframe) by `d`; tangents are
/// relative and stay. `axis` limits it to one separated dimension.
fn offset_attr(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    attr: &str,
    d: [f64; 2],
) -> Result<(), CommandError> {
    let Some(spec) = query::attr_spec(tx.stage(), path, attr).cloned() else {
        return Ok(());
    };
    let mut spec: AttrSpec = spec;
    if let Some(v) = &mut spec.value {
        *v = offset_point(v, d);
    }
    for k in spec.keyframes.values_mut() {
        k.value = offset_point(&k.value, d);
    }
    write_attr(tx, path, attr, spec)
}

/// Move every top-level (unparented) layer of `comp` by `d` pixels, keyframes
/// included. AE moves locked ones too: the frame changes, not the layers'
/// place in it.
fn move_top_layers(
    tx: &mut Transaction<'_>,
    comp: &PrimPath,
    d: [f64; 2],
) -> Result<(), CommandError> {
    if d == [0.0, 0.0] {
        return Ok(());
    }
    tx.allow_locked(|tx| -> Result<(), CommandError> {
        for layer in query::comp_layers(tx.stage(), comp) {
            let view = tx.stage().prim(&layer);
            let parented = view
                .as_ref()
                .and_then(|v| query::static_value(v, "parent"))
                .is_some_and(|v| matches!(v, Value::PrimRef(_)));
            if parented {
                continue;
            }
            let separate = view
                .as_ref()
                .and_then(|v| query::static_value(v, "transform:separate_dimensions"))
                .and_then(|v| v.as_bool())
                == Some(true);
            if separate {
                offset_attr(tx, &layer, "transform:position_x", [d[0], 0.0])?;
                offset_attr(tx, &layer, "transform:position_y", [d[1], 0.0])?;
            } else {
                offset_attr(tx, &layer, "transform:position", d)?;
            }
            offset_attr(tx, &layer, "transform:point_of_interest", d)?;
        }
        Ok(())
    })
}

impl Command for SetCompSettings {
    type Params = CompSettingsParams;
    type Output = ();
    const ID: &'static str = "comp.set_settings";
    const TITLE: &'static str = "Composition Settings";
    const DESCRIPTION: &'static str = "Change a comp's settings (AE Composition Settings, Cmd/Ctrl+K): name, size, pixel aspect ratio, frame rate, resolution, start timecode, duration, background color, motion blur shutter and samples, preserve options, 3D renderer. Resizing moves unparented layers with the anchor (default center) so they keep their place in the frame.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let info = query::comp_info(tx.stage(), &p.comp)
            .ok_or_else(|| fail(format!("{} is not a comp", p.comp)))?;
        if let Some(name) = p.name.as_ref().map(|n| n.trim()) {
            if name.is_empty() {
                return Err(fail("the comp name cannot be empty"));
            }
            set(tx, &p.comp, "name", Value::String(name.to_owned()))?;
        }
        let width = p.width.unwrap_or(info.width);
        let height = p.height.unwrap_or(info.height);
        if !(1..=30_000).contains(&width) || !(1..=30_000).contains(&height) {
            return Err(fail("comp size must be between 1 and 30000 pixels"));
        }
        if (width, height) != (info.width, info.height) {
            set(tx, &p.comp, "width", Value::Int(i64::from(width)))?;
            set(tx, &p.comp, "height", Value::Int(i64::from(height)))?;
            let f = p.anchor.unwrap_or_default().factor();
            let d = [
                (f64::from(width) - f64::from(info.width)) * f[0],
                (f64::from(height) - f64::from(info.height)) * f[1],
            ];
            move_top_layers(tx, &p.comp, d)?;
        }
        if let Some(a) = p.pixel_aspect {
            if !(a.is_finite() && (0.01..=100.0).contains(&a)) {
                return Err(fail("pixel aspect ratio must be between 0.01 and 100"));
            }
            set(tx, &p.comp, "pixel_aspect", Value::Float(a))?;
        }
        if let Some(fps) = p.fps {
            if !(fps.is_finite() && (1.0..=999.0).contains(&fps)) {
                return Err(fail("fps must be between 1 and 999"));
            }
            let rate = FrameRate::from_fps(fps);
            if rate != info.frame_rate {
                set(tx, &p.comp, "frame_rate", Value::FrameRate(rate))?;
            }
        }
        if let Some(df) = p.drop_frame {
            set(tx, &p.comp, "drop_frame", Value::Bool(df))?;
        }
        if let Some([h, v]) = p.resolution {
            if h == 0 || v == 0 || h > 99 || v > 99 {
                return Err(fail("resolution steps must be between 1 and 99"));
            }
            set(
                tx,
                &p.comp,
                "resolution",
                Value::Vec2([f64::from(h), f64::from(v)]),
            )?;
        }
        if let Some(t) = p.start_timecode {
            set(tx, &p.comp, "display_start_time", Value::Time(t))?;
        }
        if let Some(d) = p.duration
            && d != info.duration
        {
            if d.0 <= 0 {
                return Err(fail("duration must be positive"));
            }
            set(tx, &p.comp, "duration", Value::Time(d))?;
            // Keep the work area inside the comp; a full-length one (no
            // stored duration) follows the new end, like AE.
            let ws = info.work_area.start;
            let stored = tx
                .stage()
                .prim(&p.comp)
                .and_then(|v| v.value("work_area_duration"))
                .and_then(|v| v.as_time());
            if ws >= d {
                set(tx, &p.comp, "work_area_start", Value::Time(Time::ZERO))?;
                set(tx, &p.comp, "work_area_duration", Value::Time(d))?;
            } else if let Some(wd) = stored
                && ws + wd > d
            {
                set(tx, &p.comp, "work_area_duration", Value::Time(d - ws))?;
            } else if stored.is_some() && info.work_area.end == info.duration && ws == Time::ZERO {
                // A work area covering the whole comp keeps covering it.
                set(tx, &p.comp, "work_area_duration", Value::Time(d))?;
            }
        }
        if let Some(c) = p.bg_color {
            set(tx, &p.comp, "bg_color", Value::Color(c))?;
        }
        if let Some(a) = p.shutter_angle {
            set(
                tx,
                &p.comp,
                "shutter_angle",
                Value::Float(a.clamp(0.0, 720.0)),
            )?;
        }
        if let Some(a) = p.shutter_phase {
            set(
                tx,
                &p.comp,
                "shutter_phase",
                Value::Float(a.clamp(-360.0, 360.0)),
            )?;
        }
        if let Some(n) = p.samples_per_frame {
            set(
                tx,
                &p.comp,
                "motion_blur_samples",
                Value::Int(n.clamp(2, 64)),
            )?;
        }
        if let Some(n) = p.adaptive_sample_limit {
            set(
                tx,
                &p.comp,
                "motion_blur_adaptive_limit",
                Value::Int(n.clamp(16, 256)),
            )?;
        }
        if let Some(b) = p.preserve_frame_rate {
            set(tx, &p.comp, "preserve_frame_rate", Value::Bool(b))?;
        }
        if let Some(b) = p.preserve_resolution {
            set(tx, &p.comp, "preserve_resolution", Value::Bool(b))?;
        }
        if let Some(r) = p.renderer {
            set(tx, &p.comp, "renderer", Value::Enum(r))?;
        }
        Ok(())
    }
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

pub struct CropComp;

#[derive(Deserialize, JsonSchema)]
pub struct CropCompParams {
    pub comp: PrimPath,
    /// The part of the frame to keep, comp pixels: [left, top, right, bottom].
    /// Rounded outward to whole pixels.
    pub rect: [f64; 4],
}

impl Command for CropComp {
    type Params = CropCompParams;
    type Output = ();
    const ID: &'static str = "comp.crop";
    const TITLE: &'static str = "Crop Comp";
    const DESCRIPTION: &'static str = "Composition > Crop Comp to Region of Interest / to Selected Layer(s) Bounds: the comp becomes the size of `rect` and the top-level layers move so that what was inside it stays in place on screen.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        query::comp_info(tx.stage(), &p.comp)
            .ok_or_else(|| fail(format!("{} is not a comp", p.comp)))?;
        let [l, t, r, b] = p.rect;
        let (l, t, r, b) = (l.floor(), t.floor(), r.ceil(), b.ceil());
        let (w, h) = (r - l, b - t);
        if !(1.0..=30_000.0).contains(&w) || !(1.0..=30_000.0).contains(&h) {
            return Err(fail("comp size must be between 1 and 30000 pixels"));
        }
        set(tx, &p.comp, "width", Value::Int(w as i64))?;
        set(tx, &p.comp, "height", Value::Int(h as i64))?;
        move_top_layers(tx, &p.comp, [-l, -t])
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
        // AE shifts every layer, locked ones too: the comp's range moves,
        // not what the layers show.
        if shift != Time::ZERO {
            tx.allow_locked(|tx| -> Result<(), CommandError> {
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
                Ok(())
            })?;
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
    /// Project panel label, 0 to 16.
    #[serde(default)]
    pub label: Option<i64>,
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
        let name = p.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
            query::next_numbered_name(tx.stage(), &parent, crate::names::localized("Untitled"))
        });
        let path = create_project_folder(tx, &parent, name, p.label)?;
        Ok(PathOutput { path })
    }
}

pub(crate) fn create_project_folder(
    tx: &mut Transaction<'_>,
    parent: &PrimPath,
    name: String,
    label: Option<i64>,
) -> Result<PrimPath, CommandError> {
    let path = child(parent, &query::unique_child_name(tx.stage(), parent, &name))?;
    create(tx, &path, PrimType::FOLDER, None)?;
    set(tx, &path, "name", Value::String(name))?;
    if let Some(label) = label.filter(|n| (0..=16).contains(n)) {
        set(tx, &path, "label", Value::Int(label))?;
    }
    Ok(path)
}
