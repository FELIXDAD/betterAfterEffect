//! Puppet pins as the renderer sees them (the Puppet effect, `ADBE FreePin3`).
//! The comp conversion packs the effect's `PuppetPin` prims, evaluated at the
//! frame, into the effect's parameter [`PINS_PARAM`]; the effect unpacks
//! them. Packing keeps them in the effect's parameters, so the frame cache
//! sees every change.

use bae_anim::EvaluatedPrim;
use bae_model::{PrimType, Value};
use glam::DVec2;

/// The Puppet effect's matchName.
pub const PUPPET_EFFECT: &str = "ADBE FreePin3";
/// The effect parameter holding the packed pins (AE parameters start at 1).
pub const PINS_PARAM: u32 = 0;

/// AE's pin kinds (the Pin Type popup, plus Overlap pins).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PinKind {
    /// Pulls its point to its Position.
    #[default]
    Position,
    /// Stiffens the mesh around it (Amount, Extent).
    Starch,
    /// Rotates and scales the mesh around it without moving it.
    Bend,
    /// Position, rotation and scale.
    Advanced,
    /// Draws the mesh around it in front (In Front, Extent).
    Overlap,
}

impl PinKind {
    pub fn token(self) -> &'static str {
        match self {
            PinKind::Position => "position",
            PinKind::Starch => "starch",
            PinKind::Bend => "bend",
            PinKind::Advanced => "advanced",
            PinKind::Overlap => "overlap",
        }
    }

    pub fn parse(token: &str) -> PinKind {
        match token {
            "starch" => PinKind::Starch,
            "bend" => PinKind::Bend,
            "advanced" => PinKind::Advanced,
            "overlap" => PinKind::Overlap,
            _ => PinKind::Position,
        }
    }

    fn code(self) -> f64 {
        f64::from(self as u8)
    }

    fn from_code(c: f64) -> PinKind {
        match c.round() as i64 {
            1 => PinKind::Starch,
            2 => PinKind::Bend,
            3 => PinKind::Advanced,
            4 => PinKind::Overlap,
            _ => PinKind::Position,
        }
    }

    /// Whether the pin holds its point where its Position says.
    pub fn pulls(self) -> bool {
        matches!(self, PinKind::Position | PinKind::Bend | PinKind::Advanced)
    }
}

/// One pin at the frame (layer pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pin {
    pub kind: PinKind,
    /// Where it was placed on the undeformed mesh.
    pub rest: DVec2,
    /// Where it pulls that point.
    pub position: DVec2,
    /// Percent.
    pub scale: f64,
    /// Degrees.
    pub rotation: f64,
    pub amount: f64,
    pub extent: f64,
    pub in_front: f64,
}

/// Floats per packed pin.
const STRIDE: usize = 10;

fn vec2(p: &EvaluatedPrim, attr: &str) -> DVec2 {
    match p.get(attr) {
        Some(Value::Vec2(v)) => DVec2::from_array(*v),
        Some(Value::Vec3(v)) => DVec2::new(v[0], v[1]),
        _ => DVec2::ZERO,
    }
}

/// The pins under a Puppet effect, in prim order.
pub fn pins(children: &[EvaluatedPrim]) -> Vec<Pin> {
    children
        .iter()
        .filter(|c| c.prim_type.as_ref() == Some(&PrimType::PUPPET_PIN))
        .map(|c| Pin {
            kind: match c.get("pin_type") {
                Some(Value::Enum(s) | Value::String(s)) => PinKind::parse(s),
                _ => PinKind::Position,
            },
            rest: vec2(c, "rest"),
            position: vec2(c, "position"),
            scale: c.f64("scale").unwrap_or(100.0),
            rotation: c.f64("rotation").unwrap_or(0.0),
            amount: c.f64("amount").unwrap_or(0.3),
            extent: c.f64("extent").unwrap_or(15.0),
            in_front: c.f64("in_front").unwrap_or(0.0),
        })
        .collect()
}

/// The pins as one parameter value.
pub fn pack(pins: &[Pin]) -> Value {
    let mut f = Vec::with_capacity(pins.len() * STRIDE);
    for p in pins {
        f.extend([
            p.kind.code(),
            p.rest.x,
            p.rest.y,
            p.position.x,
            p.position.y,
            p.scale,
            p.rotation,
            p.amount,
            p.extent,
            p.in_front,
        ]);
    }
    Value::FloatList(f)
}

/// The pins [`pack`] packed.
pub fn unpack(v: &Value) -> Vec<Pin> {
    let Value::FloatList(f) = v else {
        return Vec::new();
    };
    f.as_chunks::<STRIDE>()
        .0
        .iter()
        .map(|c| Pin {
            kind: PinKind::from_code(c[0]),
            rest: DVec2::new(c[1], c[2]),
            position: DVec2::new(c[3], c[4]),
            scale: c[5],
            rotation: c[6],
            amount: c[7],
            extent: c[8],
            in_front: c[9],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_round_trip() {
        let pins = [
            Pin {
                kind: PinKind::Starch,
                rest: DVec2::new(1.0, 2.0),
                position: DVec2::new(3.0, 4.0),
                scale: 100.0,
                rotation: 0.0,
                amount: 0.3,
                extent: 15.0,
                in_front: 0.0,
            },
            Pin {
                kind: PinKind::Overlap,
                rest: DVec2::new(5.0, 6.0),
                position: DVec2::new(5.0, 6.0),
                scale: 100.0,
                rotation: 0.0,
                amount: 0.3,
                extent: 20.0,
                in_front: 50.0,
            },
        ];
        assert_eq!(unpack(&pack(&pins)), pins);
    }
}
