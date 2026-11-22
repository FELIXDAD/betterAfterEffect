//! Effect parameters: AE parameter definitions → BAE effect-param model.
//!
//! AE effects declare parameters during `PF_Cmd_PARAM_SETUP`, one definition
//! per `PF_ADD_PARAM` call; `params[0]` is always the implicit input layer
//! (SDK guide, "Effect Basics › Parameters", "› PF_ParamDef"). Each definition
//! has a type, a name (≤ 31 bytes + NUL), behaviour flags, UI flags, an
//! optional custom-UI size and a stable *disk id* that survives re-ordering
//! across plug-in versions.
//!
//! This module mirrors those definitions ([`ParamDescriptor`]) and decides how
//! each one is stored and animated ([`BaeParamMapping`]). The conversion to
//! the merged 02 types (`bae_model::EffectParamDef`) is in [`crate::model`].

use crate::error::{HostError, Result};
use crate::geometry::Fixed;

/// Position of a parameter in the `params[]` array (0 = input layer).
pub type ParamIndex = u32;

/// Stable parameter identity persisted with projects (`PF_ParamDef::uu.id`).
/// BAE stores effect param Attributes keyed by this id, never by index, so
/// a plug-in update that re-orders params keeps old projects working.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DiskId(pub u32);

/// Parameter display name, limited to 31 bytes (not characters) plus NUL.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ParamName(String);

impl ParamName {
    /// Maximum length in bytes, excluding the terminator.
    pub const MAX_BYTES: usize = 31;

    /// Validate a UTF-8 name (after the host converted it from the plug-in's
    /// encoding: locale multibyte, or UTF-8 when the plug-in declares
    /// `STRINGS_ARE_UTF8`).
    pub fn new(s: impl Into<String>) -> Result<ParamName> {
        let s = s.into();
        if s.len() > Self::MAX_BYTES || s.contains('\0') {
            return Err(HostError::OutOfRange { what: "param name" });
        }
        Ok(ParamName(s))
    }

    /// Name as `&str`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Behaviour flags (`PF_ParamFlag_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ParamFlags {
    /// No keyframes (`CANNOT_TIME_VARY`).
    pub cannot_time_vary: bool,
    /// Hold interpolation only (`CANNOT_INTERP`; forced for checkboxes).
    pub cannot_interp: bool,
    /// Twirl state change requested at runtime (`COLLAPSE_TWIRLY`).
    pub collapse_twirly: bool,
    /// Send `USER_CHANGED_PARAM` when the user edits it (`SUPERVISE`).
    pub supervise: bool,
    /// Group starts collapsed (`START_COLLAPSED`; needs the module flag).
    pub start_collapsed: bool,
    /// Initialise from `value` rather than the default when loading projects
    /// saved before the param existed (`USE_VALUE_FOR_OLD_PROJECTS`).
    pub use_value_for_old_projects: bool,
    /// Premiere-only track-matte hint (`LAYER_PARAM_IS_TRACKMATTE`).
    pub layer_param_is_trackmatte: bool,
    /// Ignore in have-inputs-changed queries (`EXCLUDE_FROM_HAVE_INPUTS_CHANGED`).
    pub exclude_from_have_inputs_changed: bool,
    /// Do not reveal the param when it becomes visible
    /// (`SKIP_REVEAL_WHEN_UNHIDDEN`).
    pub skip_reveal_when_unhidden: bool,
}

/// UI flags (`PF_PUI_*`). Some may change at runtime via the param-UI update
/// callback; BAE stores the runtime UI state in the *session* sublayer (not
/// saved, not synced) except where AE persists it (see spec R02-05).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ParamUiFlags {
    /// Plug-in draws the topic (title row) itself (`TOPIC`).
    pub topic: bool,
    /// Plug-in draws the control area itself (`CONTROL`).
    pub control: bool,
    /// Standard control without a data stream / keyframes
    /// (`STD_CONTROL_ONLY`; requires `supervise`).
    pub std_control_only: bool,
    /// No effect-controls UI (`NO_ECW_UI`).
    pub no_ecw_ui: bool,
    /// Separator line above (`ECW_SEPARATOR`, Premiere).
    pub ecw_separator: bool,
    /// Greyed out (`DISABLED`).
    pub disabled: bool,
    /// Host must not erase the topic before custom drawing.
    pub dont_erase_topic: bool,
    /// Host must not erase the control before custom drawing.
    pub dont_erase_control: bool,
    /// Popup shown as radio buttons (`RADIO_BUTTON`, Premiere).
    pub radio_button: bool,
    /// Hidden in effect controls and timeline (`INVISIBLE`).
    pub invisible: bool,
}

/// Slider value display (`PF_ValueDisplayFlag_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ValueDisplay {
    /// Show as percentage.
    pub percent: bool,
    /// Show in the user's pixel-value units (0–1, 0–255, 0–32768...).
    pub pixel: bool,
}

/// One entry of a popup menu.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PopupChoice {
    /// A selectable item.
    Item(String),
    /// A separator (written as `(-` in the choice string); not selectable but
    /// it *does* occupy a 1-based index slot.
    Separator,
}

/// Parse an AE popup choice string (`"One|Two|(-|Three"`).
pub fn parse_popup_choices(s: &str) -> Vec<PopupChoice> {
    s.split('|')
        .map(|c| {
            if c == "(-" {
                PopupChoice::Separator
            } else {
                PopupChoice::Item(c.to_string())
            }
        })
        .collect()
}

/// Parameter type with its definition-time data (`PF_Param_*`).
#[derive(Clone, Debug, PartialEq)]
pub enum ParamKind {
    /// Layer reference (`LAYER`); `default_self` = defaults to the layer the
    /// effect is on, otherwise "None".
    Layer {
        /// Default to own layer.
        default_self: bool,
    },
    /// Deprecated integer slider (`SLIDER`).
    IntSlider {
        /// Valid range.
        valid: (i32, i32),
        /// Slider range.
        slider: (i32, i32),
        /// Default.
        default: i32,
    },
    /// Deprecated 16.16 slider (`FIX_SLIDER`).
    FixedSlider {
        /// Valid range.
        valid: (Fixed, Fixed),
        /// Slider range.
        slider: (Fixed, Fixed),
        /// Default.
        default: Fixed,
        /// Displayed decimal places.
        precision: u16,
        /// Display mode.
        display: ValueDisplay,
    },
    /// Float slider (`FLOAT_SLIDER`).
    FloatSlider {
        /// Valid range.
        valid: (f64, f64),
        /// Slider range.
        slider: (f64, f64),
        /// Default.
        default: f64,
        /// Displayed decimal places.
        precision: u16,
        /// Display mode.
        display: ValueDisplay,
        /// Audio: phase wanted (`PF_FSliderFlag_WANT_PHASE`).
        want_phase: bool,
        /// Audio: curve tolerance.
        curve_tolerance: f32,
    },
    /// Angle in 16.16 degrees, multiple revolutions allowed (`ANGLE`).
    Angle {
        /// Default (degrees).
        default: Fixed,
    },
    /// Checkbox (`CHECKBOX`); always hold-interpolated.
    Checkbox {
        /// Default.
        default: bool,
        /// Label shown next to the box.
        label: String,
    },
    /// RGB colour; 8-bit in the legacy field, float through the colour-param
    /// suite (`COLOR`).
    Color {
        /// Default (8-bit RGB).
        default_rgb8: [u8; 3],
    },
    /// 2D point in layer pixels; the default is specified in percent of the
    /// layer size by spec ≥ 12.1 plug-ins (`POINT`).
    Point {
        /// Default, percent of layer size.
        default_percent: (f64, f64),
        /// Restrict to layer bounds.
        restrict_bounds: bool,
    },
    /// 3D point (`POINT_3D`).
    Point3D {
        /// Default (percent of layer size for x/y, pixels for z).
        default: [f64; 3],
    },
    /// Popup menu with 1-based selection (`POPUP`).
    Popup {
        /// Choices including separators.
        choices: Vec<PopupChoice>,
        /// Default (1-based).
        default: u32,
    },
    /// Plug-in-defined value (`ARBITRARY_DATA`): opaque bytes whose copy,
    /// flatten, interpolation and comparison are implemented by the plug-in.
    ArbitraryData {
        /// Plug-in's own type id (several arb types per effect allowed).
        arb_type: u16,
        /// Default value in flattened form.
        default_flat: Vec<u8>,
    },
    /// Reference to a mask on the same layer (`PATH`); 0 = none.
    Path {
        /// Default mask index (1-based; 0 = none).
        default_mask: u32,
    },
    /// Start of a twirl-down group (`GROUP_START`); groups nest.
    GroupStart,
    /// End of a group (`GROUP_END`).
    GroupEnd,
    /// Push button (`BUTTON`), no value; pressing it is a supervised change.
    Button {
        /// Button label.
        label: String,
    },
    /// Placeholder without data (`NO_DATA`), e.g. a custom-UI-only row.
    NoData,
}

/// One parameter definition as received during `PARAM_SETUP`.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamDescriptor {
    /// Index in `params[]` (≥ 1; index 0 is the implicit input layer).
    pub index: ParamIndex,
    /// Stable id (falls back to the index when the plug-in leaves it 0).
    pub disk_id: DiskId,
    /// Display name.
    pub name: ParamName,
    /// Type and type data.
    pub kind: ParamKind,
    /// Behaviour flags.
    pub flags: ParamFlags,
    /// UI flags.
    pub ui: ParamUiFlags,
    /// Custom UI area size for `topic`/`control` custom drawing.
    pub custom_ui_size: Option<(u16, u16)>,
}

/// How BAE interpolates the stored value between keyframes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    /// BAE's own keyframe interpolation (linear/bezier/hold per keyframe).
    Native,
    /// Hold only.
    HoldOnly,
    /// The plug-in interpolates (`PF_Arbitrary_INTERP_FUNC`): BAE stores flat
    /// bytes per keyframe and asks the plug-in for in-between values.
    PluginCallback,
    /// Not animatable at all.
    Static,
}

/// The `bae_model::Value` shape an AE param maps onto (see
/// [`BaeValueShape::value_type`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaeValueShape {
    /// `Value::Float` (f64 in the model; sliders, angles in degrees).
    Float,
    /// `Value::Int`.
    Int,
    /// `Value::Bool`.
    Bool,
    /// `Value::Color` (float RGBA; alpha ignored).
    Color,
    /// `Value::Vec2` in layer pixels.
    Point2,
    /// `Value::Vec3`.
    Point3,
    /// `Value::Int` holding the 1-based choice index (separators counted).
    Enum,
    /// `Value::PrimRef` to a layer Prim (or none).
    LayerRef,
    /// `Value::PrimRef` to a mask Prim of the same layer (or none).
    MaskRef,
    /// `Value::Bytes` (opaque bytes, plug-in owned semantics).
    Blob,
    /// No stored value (groups, buttons, no-data rows).
    None,
}

impl BaeValueShape {
    /// The 02 value type of the `param:NNNN` attribute.
    pub fn value_type(self) -> Option<bae_model::ValueType> {
        use bae_model::ValueType as V;
        Some(match self {
            BaeValueShape::Float => V::Float,
            BaeValueShape::Int | BaeValueShape::Enum => V::Int,
            BaeValueShape::Bool => V::Bool,
            BaeValueShape::Color => V::Color,
            BaeValueShape::Point2 => V::Vec2,
            BaeValueShape::Point3 => V::Vec3,
            BaeValueShape::LayerRef | BaeValueShape::MaskRef => V::PrimRef,
            BaeValueShape::Blob => V::Bytes,
            BaeValueShape::None => return None,
        })
    }
}

/// Decision of how one AE param is stored and animated in BAE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaeParamMapping {
    /// Stored value shape.
    pub shape: BaeValueShape,
    /// Interpolation regime.
    pub interpolation: Interpolation,
    /// Whether an Attribute with keyframes exists at all (`false` for
    /// `STD_CONTROL_ONLY`, which is UI-only and lives in sequence data).
    pub has_stream: bool,
}

impl ParamDescriptor {
    /// Decide the BAE representation.
    pub fn bae_mapping(&self) -> BaeParamMapping {
        use BaeValueShape as S;
        let shape = match &self.kind {
            ParamKind::Layer { .. } => S::LayerRef,
            ParamKind::IntSlider { .. } => S::Int,
            ParamKind::FixedSlider { .. }
            | ParamKind::FloatSlider { .. }
            | ParamKind::Angle { .. } => S::Float,
            ParamKind::Checkbox { .. } => S::Bool,
            ParamKind::Color { .. } => S::Color,
            ParamKind::Point { .. } => S::Point2,
            ParamKind::Point3D { .. } => S::Point3,
            ParamKind::Popup { .. } => S::Enum,
            ParamKind::ArbitraryData { .. } => S::Blob,
            ParamKind::Path { .. } => S::MaskRef,
            ParamKind::GroupStart
            | ParamKind::GroupEnd
            | ParamKind::Button { .. }
            | ParamKind::NoData => S::None,
        };
        let has_stream = shape != S::None && !self.ui.std_control_only;
        let interpolation = if !has_stream || self.flags.cannot_time_vary {
            Interpolation::Static
        } else {
            match &self.kind {
                ParamKind::ArbitraryData { .. } if !self.flags.cannot_interp => {
                    Interpolation::PluginCallback
                }
                ParamKind::Checkbox { .. }
                | ParamKind::Popup { .. }
                | ParamKind::Layer { .. }
                | ParamKind::Path { .. }
                | ParamKind::ArbitraryData { .. } => Interpolation::HoldOnly,
                _ if self.flags.cannot_interp => Interpolation::HoldOnly,
                _ => Interpolation::Native,
            }
        };
        BaeParamMapping {
            shape,
            interpolation,
            has_stream,
        }
    }
}

/// Validate a full parameter list as a host must after `PARAM_SETUP`:
/// indices are dense from 1, disk ids are unique, groups balance, and flag
/// combinations the SDK forbids are rejected.
pub fn validate_param_list(params: &[ParamDescriptor]) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    let mut depth: i32 = 0;
    for (i, p) in params.iter().enumerate() {
        if p.index as usize != i + 1 {
            return Err(HostError::OutOfRange {
                what: "param indices must be dense from 1",
            });
        }
        if !ids.insert(p.disk_id) {
            return Err(HostError::OutOfRange {
                what: "duplicate param disk id",
            });
        }
        match p.kind {
            ParamKind::GroupStart => depth += 1,
            ParamKind::GroupEnd => {
                depth -= 1;
                if depth < 0 {
                    return Err(HostError::OutOfRange {
                        what: "GROUP_END without GROUP_START",
                    });
                }
            }
            _ => {}
        }
        if p.ui.std_control_only {
            let forbidden = matches!(
                p.kind,
                ParamKind::NoData
                    | ParamKind::Layer { .. }
                    | ParamKind::ArbitraryData { .. }
                    | ParamKind::Path { .. }
            );
            if forbidden || !p.flags.supervise {
                return Err(HostError::OutOfRange {
                    what: "STD_CONTROL_ONLY needs SUPERVISE and a standard type",
                });
            }
        }
    }
    if depth != 0 {
        return Err(HostError::OutOfRange {
            what: "unbalanced param groups",
        });
    }
    Ok(())
}

/// Change flags the plug-in sets on a param during supervision
/// (`PF_ChangeFlag_*`): the host must write the new value back through a
/// Command so it is undoable and synced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChangeFlags {
    /// The plug-in changed the value (`CHANGED_VALUE`).
    pub changed_value: bool,
}

/// Arbitrary-data operations the host invokes through
/// `PF_Cmd_ARBITRARY_CALLBACK` (`PF_Arbitrary_*_FUNC`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArbOp {
    /// Create a default-valued instance.
    New,
    /// Free an instance.
    Dispose,
    /// Deep copy.
    Copy,
    /// Size of the flattened form.
    FlatSize,
    /// Serialize (byte-order independent).
    Flatten,
    /// Deserialize.
    Unflatten,
    /// Interpolate between two keyframe values at `t` in 0..=1 (called on
    /// render threads while evaluating animation).
    Interpolate {
        /// Normalised position between the keyframes.
        t: f64,
    },
    /// Compare two values.
    Compare,
    /// Size of the text form.
    PrintSize,
    /// Text form (copy/paste, keyframe text).
    Print,
    /// Parse the text form.
    Scan,
}

/// Result of [`ArbOp::Compare`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArbCompare {
    /// Equal.
    Equal,
    /// First is less.
    Less,
    /// First is more.
    More,
    /// Not equal, unordered.
    NotEqual,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(index: u32, kind: ParamKind) -> ParamDescriptor {
        ParamDescriptor {
            index,
            disk_id: DiskId(index),
            name: ParamName::new(format!("p{index}")).unwrap(),
            kind,
            flags: ParamFlags::default(),
            ui: ParamUiFlags::default(),
            custom_ui_size: None,
        }
    }

    #[test]
    fn names_are_byte_limited() {
        assert!(ParamName::new("a".repeat(31)).is_ok());
        assert!(ParamName::new("a".repeat(32)).is_err());
        // 11 CJK characters = 33 bytes: too long even though only 11 chars.
        assert!(ParamName::new("模糊".repeat(5) + "度").is_err());
        assert!(
            ParamName::new(
                "模糊半径强度参数一二三"
                    .chars()
                    .take(10)
                    .collect::<String>()
            )
            .is_ok()
        );
    }

    #[test]
    fn popup_parsing() {
        assert_eq!(
            parse_popup_choices("Add|Multiply|(-|Screen"),
            vec![
                PopupChoice::Item("Add".into()),
                PopupChoice::Item("Multiply".into()),
                PopupChoice::Separator,
                PopupChoice::Item("Screen".into()),
            ]
        );
    }

    #[test]
    fn mapping_rules() {
        let slider = p(
            1,
            ParamKind::FloatSlider {
                valid: (0.0, 100.0),
                slider: (0.0, 10.0),
                default: 1.0,
                precision: 1,
                display: ValueDisplay::default(),
                want_phase: false,
                curve_tolerance: 0.0,
            },
        );
        assert_eq!(slider.bae_mapping().interpolation, Interpolation::Native);
        let arb = p(
            2,
            ParamKind::ArbitraryData {
                arb_type: 1,
                default_flat: vec![],
            },
        );
        assert_eq!(
            arb.bae_mapping().interpolation,
            Interpolation::PluginCallback
        );
        let mut chk = p(
            3,
            ParamKind::Checkbox {
                default: false,
                label: String::new(),
            },
        );
        assert_eq!(chk.bae_mapping().interpolation, Interpolation::HoldOnly);
        chk.ui.std_control_only = true;
        chk.flags.supervise = true;
        assert_eq!(
            chk.bae_mapping(),
            BaeParamMapping {
                shape: BaeValueShape::Bool,
                interpolation: Interpolation::Static,
                has_stream: false
            }
        );
        assert_eq!(
            p(4, ParamKind::GroupStart).bae_mapping().shape,
            BaeValueShape::None
        );
    }

    #[test]
    fn list_validation() {
        let ok = vec![
            p(1, ParamKind::GroupStart),
            p(2, ParamKind::Angle { default: Fixed(0) }),
            p(3, ParamKind::GroupEnd),
        ];
        assert!(validate_param_list(&ok).is_ok());
        let unbalanced = vec![p(1, ParamKind::GroupStart)];
        assert!(validate_param_list(&unbalanced).is_err());
        let mut dup = ok.clone();
        dup[2].disk_id = DiskId(1);
        assert!(validate_param_list(&dup).is_err());
        let mut bad = vec![p(
            1,
            ParamKind::Layer {
                default_self: false,
            },
        )];
        bad[0].ui.std_control_only = true;
        bad[0].flags.supervise = true;
        assert!(validate_param_list(&bad).is_err());
        let gap = vec![p(2, ParamKind::NoData)];
        assert!(validate_param_list(&gap).is_err());
    }
}
