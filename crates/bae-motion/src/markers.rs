//! Marker commands: `marker.add`, `marker.edit`, `marker.move`, `marker.delete`.
//!
//! Markers are `Marker` prims in the `markers` folder of a comp (comp markers, comp
//! time) or of a layer (layer markers, layer time), as in AE.

use bae_model::{Command, CommandError, PrimPath, PrimType, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::{self, MARKERS};
use crate::util::{child, create, ensure_folder, ensure_unlocked, fail, set};

/// Output of `marker.add`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MarkerOutput {
    /// The new marker.
    pub path: PrimPath,
}

/// The comp or layer a marker belongs to.
fn owner_of(path: &PrimPath) -> Option<PrimPath> {
    let folder = path.parent()?;
    (folder.name() == MARKERS)
        .then(|| folder.parent())
        .flatten()
}

fn check_owner(tx: &Transaction<'_>, owner: &PrimPath) -> Result<(), CommandError> {
    let ty = tx.stage().require_prim(owner)?.prim_type().cloned();
    if ty != Some(PrimType::COMP) && ty != Some(PrimType::LAYER) {
        return Err(fail(format!("{owner} is not a comp or a layer")));
    }
    Ok(())
}

fn check_marker(tx: &Transaction<'_>, path: &PrimPath) -> Result<(), CommandError> {
    let prim = tx.stage().require_prim(path)?;
    if prim.prim_type() != Some(&PrimType::MARKER) {
        return Err(fail(format!("{path} is not a marker")));
    }
    Ok(())
}

/// Marker fields that `marker.add` and `marker.edit` set (omitted = unchanged).
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct MarkerFields {
    /// Duration in flicks (0 = a single-time marker).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<Time>,
    /// Comment shown next to the marker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cue_point_name: Option<String>,
    /// Label colour index (0 = none, 1–16).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_region: Option<bool>,
}

impl MarkerFields {
    fn write(&self, tx: &mut Transaction<'_>, path: &PrimPath) -> Result<(), CommandError> {
        if let Some(d) = self.duration {
            set(tx, path, "duration", Value::Time(d.max(Time::ZERO)))?;
        }
        for (attr, v) in [
            ("comment", &self.comment),
            ("chapter", &self.chapter),
            ("url", &self.url),
            ("frame_target", &self.frame_target),
            ("cue_point_name", &self.cue_point_name),
        ] {
            if let Some(v) = v {
                set(tx, path, attr, Value::String(v.clone()))?;
            }
        }
        if let Some(l) = self.label {
            set(tx, path, "label", Value::Int(l.clamp(0, 16)))?;
        }
        if let Some(p) = self.protected_region {
            set(tx, path, "protected_region", Value::Bool(p))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct AddMarker;

#[derive(Deserialize, JsonSchema)]
pub struct AddMarkerParams {
    /// The comp (comp marker) or layer (layer marker).
    pub owner: PrimPath,
    /// Marker time in flicks: comp time for comp markers, layer time for layer
    /// markers.
    pub time: Time,
    #[serde(flatten)]
    pub fields: MarkerFields,
}

impl Command for AddMarker {
    type Params = AddMarkerParams;
    type Output = MarkerOutput;
    const ID: &'static str = "marker.add";
    const TITLE: &'static str = "Add Marker";
    const DESCRIPTION: &'static str = "Add a comp marker (owner = comp, comp time) or a layer marker (owner = layer, layer time), optionally with a duration, comment, chapter, URL, cue point and label.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<MarkerOutput, CommandError> {
        check_owner(tx, &p.owner)?;
        ensure_unlocked(tx.stage(), [&p.owner])?;
        let folder = child(&p.owner, MARKERS)?;
        ensure_folder(tx, &folder)?;
        let name = query::unique_child_name(tx.stage(), &folder, "Marker");
        let path = child(&folder, &name)?;
        create(tx, &path, PrimType::MARKER, None)?;
        set(tx, &path, "time", Value::Time(p.time))?;
        p.fields.write(tx, &path)?;
        Ok(MarkerOutput { path })
    }
}

// ---------------------------------------------------------------------------

pub struct EditMarker;

#[derive(Deserialize, JsonSchema)]
pub struct EditMarkerParams {
    pub path: PrimPath,
    /// New time in flicks (same time base as `marker.add`).
    #[serde(default)]
    pub time: Option<Time>,
    #[serde(flatten)]
    pub fields: MarkerFields,
}

impl Command for EditMarker {
    type Params = EditMarkerParams;
    type Output = ();
    const ID: &'static str = "marker.edit";
    const TITLE: &'static str = "Edit Marker";
    const DESCRIPTION: &'static str = "Change a marker's time, duration, comment, chapter, URL, frame target, cue point name, label or protected region (the Marker dialog).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        check_marker(tx, &p.path)?;
        ensure_unlocked(tx.stage(), [&p.path])?;
        if let Some(t) = p.time {
            set(tx, &p.path, "time", Value::Time(t))?;
        }
        p.fields.write(tx, &p.path)
    }
}

// ---------------------------------------------------------------------------

pub struct MoveMarkers;

#[derive(Deserialize, JsonSchema)]
pub struct MoveMarkersParams {
    pub markers: Vec<PrimPath>,
    /// Offset in flicks added to every marker time (may be negative).
    pub delta: Time,
}

impl Command for MoveMarkers {
    type Params = MoveMarkersParams;
    type Output = ();
    const ID: &'static str = "marker.move";
    const TITLE: &'static str = "Move Markers";
    const DESCRIPTION: &'static str =
        "Move markers in time by an offset (dragging markers in the timeline).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for m in &p.markers {
            check_marker(tx, m)?;
        }
        ensure_unlocked(tx.stage(), p.markers.iter())?;
        for m in &p.markers {
            let t = marker_time(tx.stage(), m);
            set(tx, m, "time", Value::Time(t.saturating_add(p.delta)))?;
        }
        Ok(())
    }
}

/// A marker's time (its own time base).
pub fn marker_time(stage: &bae_model::Stage, path: &PrimPath) -> Time {
    stage
        .prim(path)
        .and_then(|v| query::static_value(&v, "time"))
        .and_then(|v| v.as_time())
        .unwrap_or(Time::ZERO)
}

// ---------------------------------------------------------------------------

pub struct DeleteMarkers;

#[derive(Deserialize, JsonSchema)]
pub struct DeleteMarkersParams {
    pub markers: Vec<PrimPath>,
}

impl Command for DeleteMarkers {
    type Params = DeleteMarkersParams;
    type Output = ();
    const ID: &'static str = "marker.delete";
    const TITLE: &'static str = "Delete Markers";
    const DESCRIPTION: &'static str =
        "Delete comp or layer markers (Delete This Marker, Delete All Markers).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for m in &p.markers {
            check_marker(tx, m)?;
            if owner_of(m).is_none() {
                return Err(fail(format!("{m} is not in a markers folder")));
            }
        }
        ensure_unlocked(tx.stage(), p.markers.iter())?;
        crate::layer::delete_prims(tx, &p.markers, Time::ZERO)
    }
}

/// The markers of a comp or layer, sorted by time.
pub fn markers_of(stage: &bae_model::Stage, owner: &PrimPath) -> Vec<(PrimPath, Time)> {
    let Ok(folder) = owner.child(MARKERS) else {
        return Vec::new();
    };
    let mut out: Vec<(PrimPath, Time)> = stage
        .children(&folder)
        .into_iter()
        .filter(|p| {
            stage
                .prim(p)
                .is_some_and(|v| v.prim_type() == Some(&PrimType::MARKER))
        })
        .map(|p| {
            let t = marker_time(stage, &p);
            (p, t)
        })
        .collect();
    out.sort_by_key(|(_, t)| *t);
    out
}
