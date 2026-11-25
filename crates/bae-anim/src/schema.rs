//! Names of the attributes the evaluator reads, and evaluation-specific schema facts
//! (spatial interpolation, clamping).
//!
//! Attribute names, value types and defaults are defined by 02's prim schema
//! (`bae_model::schema`, `docs/spec/data-model.md` §10); the constants here are the
//! same strings, and a test checks that every one of them exists in that schema.
//! Units are AE's: pixels, degrees, percent (scale, opacity), flicks for times.

use bae_model::{AttrSpec, PrimType, Value};

use crate::curve::CurveKind;

/// `Comp` prim attributes.
pub mod comp {
    pub const WIDTH: &str = "width";
    pub const HEIGHT: &str = "height";
    pub const PIXEL_ASPECT: &str = "pixel_aspect";
    pub const FRAME_RATE: &str = "frame_rate";
    pub const DURATION: &str = "duration";
    pub const BG_COLOR: &str = "bg_color";
    pub const MOTION_BLUR: &str = "motion_blur";
    pub const SHUTTER_ANGLE: &str = "shutter_angle";
    pub const SHUTTER_PHASE: &str = "shutter_phase";
    pub const MOTION_BLUR_SAMPLES: &str = "motion_blur_samples";
    /// Adaptive Sample Limit: the most samples a 2D layer gets (16–256).
    pub const MOTION_BLUR_ADAPTIVE_LIMIT: &str = "motion_blur_adaptive_limit";
    pub const PRESERVE_FRAME_RATE: &str = "preserve_frame_rate";
    /// Child folder holding the layers; first child = AE layer 1 (top).
    pub const LAYERS: &str = "layers";
}

/// `Layer` prim attributes.
pub mod layer {
    pub const KIND: &str = "kind";
    pub const NAME: &str = "name";
    pub const SOURCE: &str = "source";
    pub const PARENT: &str = "parent";
    pub const IN_POINT: &str = "in_point";
    pub const OUT_POINT: &str = "out_point";
    pub const START_TIME: &str = "start_time";
    pub const STRETCH: &str = "stretch";
    pub const ENABLED: &str = "enabled";
    pub const EFFECTS_ENABLED: &str = "effects_enabled";
    pub const SOLO: &str = "solo";
    pub const SHY: &str = "shy";
    pub const GUIDE: &str = "guide";
    pub const ADJUSTMENT_LAYER: &str = "adjustment_layer";
    pub const THREE_D: &str = "three_d";
    pub const AUTO_ORIENT: &str = "auto_orient";
    pub const BLEND_MODE: &str = "blend_mode";
    pub const TRACK_MATTE: &str = "track_matte";
    pub const TRACK_MATTE_LAYER: &str = "track_matte_layer";
    pub const MOTION_BLUR: &str = "motion_blur";
    pub const COLLAPSE_TRANSFORMATION: &str = "collapse_transformation";
    pub const QUALITY: &str = "quality";
    pub const TIME_REMAP_ENABLED: &str = "time_remap_enabled";
    pub const TIME_REMAP: &str = "time_remap";
    pub const PRESERVE_TRANSPARENCY: &str = "preserve_transparency";
    pub const TEXT_SOURCE: &str = "text:source";
    pub const TEXT_PATH: &str = "text:path";
    pub const TEXT_REVERSE_PATH: &str = "text:reverse_path";
    pub const TEXT_PERPENDICULAR_TO_PATH: &str = "text:perpendicular_to_path";
    pub const TEXT_FORCE_ALIGNMENT: &str = "text:force_alignment";
    pub const TEXT_FIRST_MARGIN: &str = "text:first_margin";
    pub const TEXT_LAST_MARGIN: &str = "text:last_margin";
    /// Child folders.
    pub const MASKS: &str = "masks";
    pub const EFFECTS: &str = "effects";
    pub const CONTENTS: &str = "contents";
    pub const ANIMATORS: &str = "animators";
}

/// `Asset` prim attributes.
pub mod asset {
    pub const KIND: &str = "kind";
    pub const WIDTH: &str = "width";
    pub const HEIGHT: &str = "height";
    pub const PIXEL_ASPECT: &str = "pixel_aspect";
    pub const FRAME_RATE: &str = "frame_rate";
    pub const DURATION: &str = "duration";
    pub const SOLID_COLOR: &str = "solid_color";
}

/// Layer transform group.
pub mod transform {
    pub const ANCHOR_POINT: &str = "transform:anchor_point";
    pub const POSITION: &str = "transform:position";
    pub const SEPARATE_DIMENSIONS: &str = "transform:separate_dimensions";
    pub const POSITION_X: &str = "transform:position_x";
    pub const POSITION_Y: &str = "transform:position_y";
    pub const POSITION_Z: &str = "transform:position_z";
    pub const SCALE: &str = "transform:scale";
    pub const ORIENTATION: &str = "transform:orientation";
    pub const ROTATION_X: &str = "transform:rotation_x";
    pub const ROTATION_Y: &str = "transform:rotation_y";
    pub const ROTATION: &str = "transform:rotation";
    pub const OPACITY: &str = "transform:opacity";
    pub const POINT_OF_INTEREST: &str = "transform:point_of_interest";
}

/// Camera layer options.
pub mod camera {
    pub const TYPE: &str = "camera:type";
    pub const ZOOM: &str = "camera:zoom";
    pub const DEPTH_OF_FIELD: &str = "camera:depth_of_field";
    pub const FOCUS_DISTANCE: &str = "camera:focus_distance";
    pub const APERTURE: &str = "camera:aperture";
    pub const BLUR_LEVEL: &str = "camera:blur_level";
}

/// Light layer options.
pub mod light {
    pub const TYPE: &str = "light:type";
    pub const INTENSITY: &str = "light:intensity";
    pub const COLOR: &str = "light:color";
    pub const CONE_ANGLE: &str = "light:cone_angle";
    pub const CONE_FEATHER: &str = "light:cone_feather";
    pub const FALLOFF: &str = "light:falloff";
    pub const RADIUS: &str = "light:radius";
    pub const FALLOFF_DISTANCE: &str = "light:falloff_distance";
    pub const CASTS_SHADOWS: &str = "light:casts_shadows";
    pub const SHADOW_DARKNESS: &str = "light:shadow_darkness";
    pub const SHADOW_DIFFUSION: &str = "light:shadow_diffusion";
}

/// `Mask` prim attributes.
pub mod mask {
    pub const PATH: &str = "path";
    pub const MODE: &str = "mode";
    pub const INVERTED: &str = "inverted";
    pub const FEATHER: &str = "feather";
    pub const OPACITY: &str = "opacity";
    pub const EXPANSION: &str = "expansion";
}

/// `Effect` prim attributes; parameters are `param:NNNN`.
pub mod effect {
    pub const MATCH_NAME: &str = "match_name";
    pub const NAME: &str = "name";
    pub const ENABLED: &str = "enabled";
    pub const EFFECTS_ENABLED: &str = "effects_enabled";
    pub const PARAM_PREFIX: &str = "param:";
}

/// Schema default of `attr` on a prim of type `prim_type`.
pub fn default_value(prim_type: &PrimType, attr: &str) -> Option<Value> {
    bae_model::schema::attr_def(prim_type, attr)?
        .default
        .to_value()
}

/// Whether an attribute's keyframes interpolate along a spatial path (one ease for
/// the distance travelled, AE `TwoD_SPATIAL` / `ThreeD_SPATIAL`).
///
/// Layer Anchor Point, Position and Point of Interest are spatial; so is any other
/// `vec2`/`vec3` attribute whose keyframes carry spatial tangents (AE exports tangents
/// only for spatial properties, e.g. point effect parameters, shape positions).
pub fn curve_kind(attr: &str, spec: &AttrSpec) -> CurveKind {
    let known = matches!(
        attr,
        transform::ANCHOR_POINT | transform::POSITION | transform::POINT_OF_INTEREST
    );
    let tangents = spec
        .keyframes
        .values()
        .any(|k| k.spatial_in.is_some() || k.spatial_out.is_some());
    if known || tangents {
        CurveKind::Spatial
    } else {
        CurveKind::PerDimension
    }
}

/// Range an attribute's value is clamped to after interpolation (AE clamps these
/// even when a Bezier ease overshoots).
pub fn clamp_range(prim_type: Option<&PrimType>, attr: &str) -> Option<(f64, f64)> {
    match (prim_type, attr) {
        (_, transform::OPACITY) => Some((0.0, 100.0)),
        (Some(t), mask::OPACITY) if *t == PrimType::MASK => Some((0.0, 100.0)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::schema::attr_def;

    #[test]
    fn names_exist_in_the_model_schema() {
        let check = |t: &PrimType, names: &[&str]| {
            for n in names {
                assert!(
                    attr_def(t, n).is_some(),
                    "{t}: {n} missing from bae_model::schema"
                );
            }
        };
        use comp as c;
        check(
            &PrimType::COMP,
            &[
                c::WIDTH,
                c::HEIGHT,
                c::PIXEL_ASPECT,
                c::FRAME_RATE,
                c::DURATION,
                c::BG_COLOR,
                c::MOTION_BLUR,
                c::SHUTTER_ANGLE,
                c::SHUTTER_PHASE,
                c::MOTION_BLUR_SAMPLES,
                c::MOTION_BLUR_ADAPTIVE_LIMIT,
                c::PRESERVE_FRAME_RATE,
            ],
        );
        use layer as l;
        use transform as tr;
        check(
            &PrimType::LAYER,
            &[
                l::KIND,
                l::NAME,
                l::SOURCE,
                l::PARENT,
                l::IN_POINT,
                l::OUT_POINT,
                l::START_TIME,
                l::STRETCH,
                l::ENABLED,
                l::SOLO,
                l::SHY,
                l::GUIDE,
                l::ADJUSTMENT_LAYER,
                l::THREE_D,
                l::AUTO_ORIENT,
                l::BLEND_MODE,
                l::TRACK_MATTE,
                l::TRACK_MATTE_LAYER,
                l::MOTION_BLUR,
                l::COLLAPSE_TRANSFORMATION,
                l::QUALITY,
                l::TIME_REMAP_ENABLED,
                l::TIME_REMAP,
                l::PRESERVE_TRANSPARENCY,
                l::TEXT_SOURCE,
                l::TEXT_PATH,
                tr::ANCHOR_POINT,
                tr::POSITION,
                tr::SEPARATE_DIMENSIONS,
                tr::POSITION_X,
                tr::POSITION_Y,
                tr::POSITION_Z,
                tr::SCALE,
                tr::ORIENTATION,
                tr::ROTATION_X,
                tr::ROTATION_Y,
                tr::ROTATION,
                tr::OPACITY,
                tr::POINT_OF_INTEREST,
                camera::TYPE,
                camera::ZOOM,
                camera::DEPTH_OF_FIELD,
                camera::FOCUS_DISTANCE,
                camera::APERTURE,
                camera::BLUR_LEVEL,
                light::TYPE,
                light::INTENSITY,
                light::COLOR,
                light::CONE_ANGLE,
                light::CONE_FEATHER,
                light::FALLOFF,
                light::RADIUS,
                light::FALLOFF_DISTANCE,
                light::CASTS_SHADOWS,
                light::SHADOW_DARKNESS,
                light::SHADOW_DIFFUSION,
            ],
        );
        check(
            &PrimType::MASK,
            &[
                mask::PATH,
                mask::MODE,
                mask::INVERTED,
                mask::FEATHER,
                mask::OPACITY,
                mask::EXPANSION,
            ],
        );
        check(
            &PrimType::ASSET,
            &[
                asset::KIND,
                asset::WIDTH,
                asset::HEIGHT,
                asset::PIXEL_ASPECT,
                asset::FRAME_RATE,
                asset::DURATION,
                asset::SOLID_COLOR,
            ],
        );
        check(
            &PrimType::EFFECT,
            &[effect::MATCH_NAME, effect::NAME, effect::ENABLED],
        );
    }
}
