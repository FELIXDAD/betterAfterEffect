//! Camera and light settings: `camera.set_settings` (AE Camera Settings) and
//! `light.set_settings` (AE Light Settings), the settings `layer.create` takes
//! for new cameras and lights, and AE's lens math ([`lens`]).
//!
//! Editing writes each property the way the timeline does: a keyframed
//! property gets a keyframe at the given time, others a new static value
//! (AE: "OK on an existing camera writes the values at the current time").

use bae_model::{Command as _, CommandError, PrimPath, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::keys::{SetAt, SetAtParams};
use crate::query;
use crate::util::{fail, set};

/// AE's lens math (docs/spec/ae AE-CMP-418/419). Only Zoom, Focus Distance
/// and Aperture are stored (pixels); the rest is derived for display.
pub mod lens {
    /// AE's camera presets: name and focal length in mm (36 mm film).
    pub const PRESETS: [(&str, f64); 9] = [
        ("15mm", 15.0),
        ("20mm", 20.0),
        ("24mm", 24.0),
        ("28mm", 28.0),
        ("35mm", 35.0),
        ("50mm", 50.0),
        ("80mm", 80.0),
        ("135mm", 135.0),
        ("200mm", 200.0),
    ];
    /// Film size of the presets, in mm.
    pub const FILM_SIZE: f64 = 36.0;
    /// AE's default camera aperture in pixels (Ref Camera, AE 26).
    pub const DEFAULT_APERTURE: f64 = 25.309336;

    /// Which comp dimension the film size measures.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum Measure {
        #[default]
        Horizontally,
        Vertically,
        Diagonally,
    }

    /// The comp size in pixels along `measure`.
    pub fn dim(width: u32, height: u32, measure: Measure) -> f64 {
        let (w, h) = (f64::from(width), f64::from(height));
        match measure {
            Measure::Horizontally => w,
            Measure::Vertically => h,
            Measure::Diagonally => w.hypot(h),
        }
    }

    /// Zoom (px) of a focal length on a film (same units).
    pub fn zoom(focal: f64, film: f64, dim: f64) -> f64 {
        focal * dim / film
    }

    /// Focal length (film units) of a zoom.
    pub fn focal(zoom: f64, film: f64, dim: f64) -> f64 {
        zoom * film / dim
    }

    /// Angle of view in degrees of a zoom.
    pub fn angle_of_view(zoom: f64, dim: f64) -> f64 {
        2.0 * (dim / (2.0 * zoom)).atan().to_degrees()
    }

    /// Zoom of an angle of view in degrees (0–180).
    pub fn zoom_from_angle(degrees: f64, dim: f64) -> f64 {
        dim / (2.0 * (degrees.to_radians() / 2.0).tan())
    }

    /// Pixels to film units (mm when `film` is in mm).
    pub fn to_film(px: f64, film: f64, dim: f64) -> f64 {
        px * film / dim
    }

    /// Film units to pixels.
    pub fn from_film(v: f64, film: f64, dim: f64) -> f64 {
        v * dim / film
    }

    /// F-stop of an aperture (px) at a focal length (mm).
    pub fn f_stop(focal_mm: f64, aperture_px: f64, film: f64, dim: f64) -> f64 {
        focal_mm / to_film(aperture_px, film, dim)
    }

    /// Aperture (px) of an f-stop at a focal length (mm).
    pub fn aperture(focal_mm: f64, f_stop: f64, film: f64, dim: f64) -> f64 {
        from_film(focal_mm / f_stop, film, dim)
    }
}

/// Camera Settings values (all optional: only the given ones change).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct CameraSettings {
    /// one_node or two_node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_type: Option<String>,
    /// Zoom in pixels (≥ 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth_of_field: Option<bool>,
    /// Focus distance in pixels (> 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_distance: Option<f64>,
    /// Aperture in pixels (≥ 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aperture: Option<f64>,
    /// Blur level in percent (≥ 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur_level: Option<f64>,
}

fn finite(v: f64, what: &str) -> Result<f64, CommandError> {
    v.is_finite()
        .then_some(v)
        .ok_or_else(|| fail(format!("{what} must be a number")))
}

fn at_least(v: f64, min: f64, what: &str) -> Result<f64, CommandError> {
    if finite(v, what)? < min {
        return Err(fail(format!("{what} must be at least {min}")));
    }
    Ok(v)
}

fn within(v: f64, min: f64, max: f64, what: &str) -> Result<f64, CommandError> {
    if !(min..=max).contains(&finite(v, what)?) {
        return Err(fail(format!("{what} must be between {min} and {max}")));
    }
    Ok(v)
}

fn one_of(v: &str, options: &[&str], what: &str) -> Result<Value, CommandError> {
    if options.contains(&v) {
        Ok(Value::Enum(v.to_owned()))
    } else {
        Err(fail(format!(
            "{what} must be one of {}",
            options.join(", ")
        )))
    }
}

impl CameraSettings {
    /// The camera attributes and their values, validated.
    pub fn attrs(&self) -> Result<Vec<(&'static str, Value)>, CommandError> {
        let mut out = Vec::new();
        if let Some(t) = &self.camera_type {
            out.push((
                "camera:type",
                one_of(t, &["one_node", "two_node"], "camera_type")?,
            ));
        }
        if let Some(v) = self.zoom {
            out.push(("camera:zoom", Value::Float(at_least(v, 1.0, "zoom")?)));
        }
        if let Some(v) = self.depth_of_field {
            out.push(("camera:depth_of_field", Value::Bool(v)));
        }
        if let Some(v) = self.focus_distance {
            let v = at_least(v, 0.0, "focus_distance")?;
            out.push(("camera:focus_distance", Value::Float(v)));
        }
        if let Some(v) = self.aperture {
            out.push((
                "camera:aperture",
                Value::Float(at_least(v, 0.0, "aperture")?),
            ));
        }
        if let Some(v) = self.blur_level {
            out.push((
                "camera:blur_level",
                Value::Float(at_least(v, 0.0, "blur_level")?),
            ));
        }
        Ok(out)
    }
}

/// Light Settings values (all optional: only the given ones change).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct LightSettings {
    /// parallel, spot, point, ambient or environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light_type: Option<String>,
    /// Straight RGBA 0–1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f64; 4]>,
    /// Percent; negative lights subtract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intensity: Option<f64>,
    /// Full cone angle in degrees, 0–180 (spot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cone_angle: Option<f64>,
    /// Percent, 0–100 (spot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cone_feather: Option<f64>,
    /// none, smooth or inverse_square_clamped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub falloff: Option<String>,
    /// Pixels (≥ 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
    /// Pixels (≥ 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub falloff_distance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub casts_shadows: Option<bool>,
    /// Percent, 0–100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_darkness: Option<f64>,
    /// Pixels (≥ 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_diffusion: Option<f64>,
}

impl LightSettings {
    /// The light attributes and their values, validated.
    pub fn attrs(&self) -> Result<Vec<(&'static str, Value)>, CommandError> {
        let mut out = Vec::new();
        if let Some(t) = &self.light_type {
            let types = ["parallel", "spot", "point", "ambient", "environment"];
            out.push(("light:type", one_of(t, &types, "light_type")?));
        }
        if let Some(c) = self.color {
            for v in c {
                finite(v, "color")?;
            }
            out.push(("light:color", Value::Color(c)));
        }
        if let Some(v) = self.intensity {
            out.push(("light:intensity", Value::Float(finite(v, "intensity")?)));
        }
        if let Some(v) = self.cone_angle {
            let v = within(v, 0.0, 180.0, "cone_angle")?;
            out.push(("light:cone_angle", Value::Float(v)));
        }
        if let Some(v) = self.cone_feather {
            let v = within(v, 0.0, 100.0, "cone_feather")?;
            out.push(("light:cone_feather", Value::Float(v)));
        }
        if let Some(f) = &self.falloff {
            let options = ["none", "smooth", "inverse_square_clamped"];
            out.push(("light:falloff", one_of(f, &options, "falloff")?));
        }
        if let Some(v) = self.radius {
            out.push(("light:radius", Value::Float(at_least(v, 0.0, "radius")?)));
        }
        if let Some(v) = self.falloff_distance {
            let v = at_least(v, 0.0, "falloff_distance")?;
            out.push(("light:falloff_distance", Value::Float(v)));
        }
        if let Some(v) = self.casts_shadows {
            out.push(("light:casts_shadows", Value::Bool(v)));
        }
        if let Some(v) = self.shadow_darkness {
            let v = within(v, 0.0, 100.0, "shadow_darkness")?;
            out.push(("light:shadow_darkness", Value::Float(v)));
        }
        if let Some(v) = self.shadow_diffusion {
            let v = at_least(v, 0.0, "shadow_diffusion")?;
            out.push(("light:shadow_diffusion", Value::Float(v)));
        }
        Ok(out)
    }
}

/// Write `attrs` on `layer` like timeline edits at comp time `time`.
fn write_at(
    tx: &mut Transaction<'_>,
    layer: &PrimPath,
    attrs: Vec<(&'static str, Value)>,
    time: Time,
) -> Result<(), CommandError> {
    let key_time = query::to_key_time(tx.stage(), layer, time);
    for (attr, value) in attrs {
        SetAt.run(
            tx,
            SetAtParams {
                path: layer.clone(),
                attr: attr.to_owned(),
                time: key_time,
                value,
            },
        )?;
    }
    Ok(())
}

fn rename(
    tx: &mut Transaction<'_>,
    layer: &PrimPath,
    name: Option<&str>,
) -> Result<(), CommandError> {
    match name.map(str::trim) {
        Some("") => Err(fail("the layer name cannot be empty")),
        Some(n) => set(tx, layer, "name", Value::String(n.to_owned())),
        None => Ok(()),
    }
}

fn require_kind(tx: &Transaction<'_>, layer: &PrimPath, kind: &str) -> Result<(), CommandError> {
    if query::layer_kind(tx.stage(), layer).as_deref() == Some(kind) {
        Ok(())
    } else {
        Err(fail(format!("{layer} is not a {kind} layer")))
    }
}

pub struct SetCameraSettings;

#[derive(Deserialize, JsonSchema)]
pub struct CameraSettingsParams {
    /// The camera layer.
    pub layer: PrimPath,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub settings: CameraSettings,
    /// Comp time (flicks) where keyframed properties get the new values (default 0).
    #[serde(default)]
    pub time: Option<Time>,
}

impl bae_model::Command for SetCameraSettings {
    type Params = CameraSettingsParams;
    type Output = ();
    const ID: &'static str = "camera.set_settings";
    const TITLE: &'static str = "Camera Settings";
    const DESCRIPTION: &'static str = "Change a camera layer's settings (AE Camera Settings): name, type (one_node / two_node; also switches auto-orient towards the point of interest), zoom, depth of field, focus distance, aperture (pixels) and blur level. Keyframed properties get a keyframe at `time`.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_kind(tx, &p.layer, "camera")?;
        let attrs = p.settings.attrs()?;
        rename(tx, &p.layer, p.name.as_deref())?;
        if let Some(t) = &p.settings.camera_type {
            // AE: the camera type is the auto-orient towards the point of interest.
            let orient = if t == "two_node" {
                "towards_point_of_interest"
            } else {
                "off"
            };
            set(tx, &p.layer, "auto_orient", Value::Enum(orient.into()))?;
        }
        write_at(tx, &p.layer, attrs, p.time.unwrap_or(Time::ZERO))
    }
}

pub struct SetLightSettings;

#[derive(Deserialize, JsonSchema)]
pub struct LightSettingsParams {
    /// The light layer.
    pub layer: PrimPath,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub settings: LightSettings,
    /// Comp time (flicks) where keyframed properties get the new values (default 0).
    #[serde(default)]
    pub time: Option<Time>,
}

impl bae_model::Command for SetLightSettings {
    type Params = LightSettingsParams;
    type Output = ();
    const ID: &'static str = "light.set_settings";
    const TITLE: &'static str = "Light Settings";
    const DESCRIPTION: &'static str = "Change a light layer's settings (AE Light Settings): name, light type, color, intensity, cone angle and feather, falloff, radius, falloff distance, casts shadows, shadow darkness and diffusion. Keyframed properties get a keyframe at `time`.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_kind(tx, &p.layer, "light")?;
        let attrs = p.settings.attrs()?;
        rename(tx, &p.layer, p.name.as_deref())?;
        write_at(tx, &p.layer, attrs, p.time.unwrap_or(Time::ZERO))
    }
}

#[cfg(test)]
mod tests {
    use super::lens::*;

    #[test]
    fn presets_match_ae_at_1920x1080() {
        let w = dim(1920, 1080, Measure::Horizontally);
        let v = dim(1920, 1080, Measure::Vertically);
        let d = dim(1920, 1080, Measure::Diagonally);
        let close = |a: f64, b: f64, eps: f64| (a - b).abs() < eps;
        // (focal, zoom, AOV h, v, diagonal) from docs/spec/ae AE-CMP-418.
        for (mm, z, h_aov, v_aov, d_aov) in [
            (15.0, 800.00, 100.39, 68.04, 108.02),
            (50.0, 2666.67, 39.60, 22.90, 44.89),
            (200.0, 10666.67, 10.29, 5.80, 11.79),
        ] {
            let zoom = zoom(mm, FILM_SIZE, w);
            assert!(close(zoom, z, 0.01), "{mm}: {zoom}");
            assert!(close(angle_of_view(zoom, w), h_aov, 0.01));
            assert!(close(angle_of_view(zoom, v), v_aov, 0.01));
            assert!(close(angle_of_view(zoom, d), d_aov, 0.01));
            assert!(close(
                zoom_from_angle(angle_of_view(zoom, w), w),
                zoom,
                1e-6
            ));
            assert!(close(focal(zoom, FILM_SIZE, w), mm, 1e-9));
        }
        // F-stop and aperture are inverses.
        let a = aperture(50.0, 2.8, FILM_SIZE, w);
        assert!(close(f_stop(50.0, a, FILM_SIZE, w), 2.8, 1e-9));
    }
}
