//! Bridge to the merged 02 effect model: AE descriptors → `bae_model::EffectDef`.
//!
//! The compat host registers every AE effect it can run in
//! `bae_model::EffectRegistry` like any other effect (19 spec §8.4, "params"),
//! so the UI, agents, AE import (15) and placeholders see one effect model.
//! This module performs that conversion against the *real* 02 types, which
//! turns the gap review of `docs/spec/aex-compat.md` §4 into compiled code:
//! whatever the 02 model cannot express yet is reported as a [`ModelGap`],
//! and every gap names the reservation request that closes it.
//!
//! Conventions (19 spec §9.4, and `bae_model::effect`):
//!
//! - `EffectParamDef::id` is the AE *disk id* (the number at the end of the
//!   param match name); definition order equals the AE param index minus one
//!   (`params[0]`, the input layer, has no definition), groups occupy a slot.
//! - Popup values are the 1-based index; separators (`(-`) stay in the option
//!   list because they occupy an index in AE.
//! - `EffectDef::match_name` is the AE match name, byte-exact.

use bae_model::{
    EffectDef, EffectParamDef, ParamFlags as ModelFlags, ParamKind as ModelKind, PluginRef, Value,
};

use crate::caps::{Concurrency, EffectCaps};
use crate::error::Result;
use crate::params::{
    DiskId, ParamDescriptor, ParamKind, PopupChoice, ValueDisplay, validate_param_list,
};
use crate::pipl::{EffectVersion, PluginDescriptor, VersionStage};

/// Plug-in id under which the compat host registers AE effects
/// (`EffectDef::plugin`, the `plugin:id` attribute of an `Effect` Prim).
/// `bae.` ids are reserved for first-party packages (bae-plugin `id.rs`).
pub const COMPAT_PLUGIN_ID: &str = "bae.aex-compat";

/// Something an AE param definition carries that the merged 02 model cannot
/// represent yet. The conversion still succeeds with the closest
/// representation; the gap is what is lost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelGap {
    /// `NO_DATA` rows (custom-UI-only rows without a value) have no
    /// `ParamKind`; represented as a custom-UI `Button` to keep the index slot.
    NoDataKind,
    /// `CANNOT_INTERP` (keyframes always hold) has no flag; numeric params
    /// would interpolate in BAE.
    HoldOnlyFlag,
    /// Point defaults are a fraction of the layer size; left empty and
    /// resolved against the layer bounds by the compat host.
    RelativePointDefault,
    /// A layer param defaulting to "this layer" cannot be told apart from
    /// "none" in the definition.
    LayerDefaultSelf,
    /// A mask param with a non-zero default mask index.
    MaskDefaultIndex,
    /// `USE_VALUE_FOR_OLD_PROJECTS`: projects saved before the param existed
    /// load a different value than new instances get.
    OldProjectValue,
    /// `STD_CONTROL_ONLY`: a standard control without a keyframe stream whose
    /// value the plug-in keeps in sequence data.
    UiOnlyControl,
    /// Custom-UI height/width of the topic or control area.
    CustomUiSize,
    /// Checkbox/button label text next to the control.
    ControlLabel,
    /// Cosmetic UI flags (separator, radio buttons, no Effect Controls row
    /// but a timeline row, don't-erase, skip-reveal, runtime twirl changes).
    CosmeticUi,
}

/// How urgently a gap must be closed (spec §4 classification).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GapClass {
    /// Must be reserved in the 02 schema now: changing it later would change
    /// persisted data.
    ReserveNow,
    /// Can be added when the compat host is built; nothing persisted depends
    /// on it today.
    Later,
    /// Not planned: cosmetic or host-specific.
    NotPlanned,
}

impl ModelGap {
    /// Classification and the request (in `docs/spec/aex-compat.md` §5, or
    /// 19's spec §23 when 19 already asked for it) that closes the gap.
    pub fn triage(self) -> (GapClass, &'static str) {
        match self {
            ModelGap::NoDataKind => (GapClass::ReserveNow, "R02-01"),
            ModelGap::HoldOnlyFlag => (GapClass::ReserveNow, "R02-01"),
            ModelGap::RelativePointDefault => (GapClass::ReserveNow, "19:R02-9"),
            ModelGap::LayerDefaultSelf => (GapClass::ReserveNow, "R02-07"),
            ModelGap::MaskDefaultIndex => (GapClass::Later, "R02-07"),
            ModelGap::OldProjectValue => (GapClass::ReserveNow, "R02-01"),
            ModelGap::UiOnlyControl => (GapClass::Later, "R02-05"),
            ModelGap::CustomUiSize => (GapClass::Later, "R12-01"),
            ModelGap::ControlLabel => (GapClass::Later, "R02-05"),
            ModelGap::CosmeticUi => (GapClass::NotPlanned, "-"),
        }
    }
}

/// One converted parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelParam {
    /// The 02 definition.
    pub def: EffectParamDef,
    /// What was lost (empty = exact).
    pub gaps: Vec<ModelGap>,
}

fn slider_display(d: ValueDisplay) -> String {
    if d.percent {
        "percent".to_owned()
    } else if d.pixel {
        "pixels".to_owned()
    } else {
        "none".to_owned()
    }
}

/// Convert one AE param definition.
pub fn param_def(p: &ParamDescriptor) -> ModelParam {
    let mut gaps = Vec::new();
    let (kind, default) = match &p.kind {
        ParamKind::Layer { default_self } => {
            if *default_self {
                gaps.push(ModelGap::LayerDefaultSelf);
            }
            (ModelKind::Layer, None)
        }
        ParamKind::IntSlider {
            valid,
            slider,
            default,
        } => (
            ModelKind::Slider {
                valid_min: valid.0.into(),
                valid_max: valid.1.into(),
                slider_min: slider.0.into(),
                slider_max: slider.1.into(),
                precision: 0,
                integer: true,
                display: "none".to_owned(),
            },
            Some(Value::Int((*default).into())),
        ),
        ParamKind::FixedSlider {
            valid,
            slider,
            default,
            precision,
            display,
        } => (
            ModelKind::Slider {
                valid_min: valid.0.to_f64(),
                valid_max: valid.1.to_f64(),
                slider_min: slider.0.to_f64(),
                slider_max: slider.1.to_f64(),
                precision: (*precision).min(u8::MAX.into()) as u8,
                integer: false,
                display: slider_display(*display),
            },
            Some(Value::Float(default.to_f64())),
        ),
        ParamKind::FloatSlider {
            valid,
            slider,
            default,
            precision,
            display,
            ..
        } => (
            ModelKind::Slider {
                valid_min: valid.0,
                valid_max: valid.1,
                slider_min: slider.0,
                slider_max: slider.1,
                precision: (*precision).min(u8::MAX.into()) as u8,
                integer: false,
                display: slider_display(*display),
            },
            Some(Value::Float(*default)),
        ),
        ParamKind::Angle { default } => (ModelKind::Angle, Some(Value::Float(default.to_f64()))),
        ParamKind::Checkbox { default, label } => {
            if !label.is_empty() {
                gaps.push(ModelGap::ControlLabel);
            }
            (ModelKind::Checkbox, Some(Value::Bool(*default)))
        }
        ParamKind::Color { default_rgb8 } => {
            // 8-bit defaults are display-referred code values; 06 decides how
            // they enter the working space (request R06-01). Stored /255 here.
            let c = default_rgb8.map(|v| f64::from(v) / 255.0);
            (
                ModelKind::Color,
                Some(Value::Color([c[0], c[1], c[2], 1.0])),
            )
        }
        ParamKind::Point { .. } => {
            gaps.push(ModelGap::RelativePointDefault);
            (ModelKind::Point, None)
        }
        ParamKind::Point3D { .. } => {
            gaps.push(ModelGap::RelativePointDefault);
            (ModelKind::Point3d, None)
        }
        ParamKind::Popup { choices, default } => (
            ModelKind::Popup {
                options: choices
                    .iter()
                    .map(|c| match c {
                        PopupChoice::Item(s) => s.clone(),
                        PopupChoice::Separator => "(-".to_owned(),
                    })
                    .collect(),
            },
            Some(Value::Int((*default).into())),
        ),
        ParamKind::ArbitraryData { default_flat, .. } => (
            ModelKind::ArbitraryData,
            Some(Value::Bytes(default_flat.clone())),
        ),
        ParamKind::Path { default_mask } => {
            if *default_mask != 0 {
                gaps.push(ModelGap::MaskDefaultIndex);
            }
            (ModelKind::Path, None)
        }
        ParamKind::GroupStart => (ModelKind::GroupStart, None),
        ParamKind::GroupEnd => (ModelKind::GroupEnd, None),
        ParamKind::Button { label } => {
            if !label.is_empty() {
                gaps.push(ModelGap::ControlLabel);
            }
            (ModelKind::Button, None)
        }
        ParamKind::NoData => {
            gaps.push(ModelGap::NoDataKind);
            (ModelKind::Button, None)
        }
    };

    let f = &p.flags;
    let ui = &p.ui;
    let custom_ui = ui.topic || ui.control || matches!(p.kind, ParamKind::NoData);
    let flags = ModelFlags {
        reference_point_default: false,
        hidden: ui.invisible,
        disabled: ui.disabled,
        supervise: f.supervise || matches!(p.kind, ParamKind::Button { .. }),
        cannot_animate: f.cannot_time_vary || ui.std_control_only,
        cannot_time_vary: f.cannot_time_vary,
        collapsed: f.start_collapsed,
        custom_ui,
        relative_default: false,
    };
    // Checkboxes, popups and references hold in BAE anyway; only numeric
    // params lose the hold-only behaviour.
    let holds_anyway = matches!(
        p.kind,
        ParamKind::Checkbox { .. }
            | ParamKind::Popup { .. }
            | ParamKind::Layer { .. }
            | ParamKind::Path { .. }
            | ParamKind::ArbitraryData { .. }
    );
    if f.cannot_interp && !f.cannot_time_vary && !holds_anyway {
        gaps.push(ModelGap::HoldOnlyFlag);
    }
    if f.use_value_for_old_projects {
        gaps.push(ModelGap::OldProjectValue);
    }
    if ui.std_control_only {
        gaps.push(ModelGap::UiOnlyControl);
    }
    if p.custom_ui_size.is_some() {
        gaps.push(ModelGap::CustomUiSize);
    }
    if ui.no_ecw_ui
        || ui.ecw_separator
        || ui.radio_button
        || ui.dont_erase_topic
        || ui.dont_erase_control
        || f.skip_reveal_when_unhidden
        || f.collapse_twirly
    {
        gaps.push(ModelGap::CosmeticUi);
    }

    ModelParam {
        def: EffectParamDef {
            id: p.disk_id.0,
            name: p.name.as_str().to_owned(),
            // The param match name is derived from the effect match name by
            // the importer (15); the definition does not need it.
            match_name: None,
            kind,
            default,
            flags,
        },
        gaps,
    }
}

/// `major.minor.bug[-stage].build` as a semver-like string for `PluginRef`.
pub fn version_string(v: EffectVersion) -> String {
    let stage = match v.stage {
        VersionStage::Develop => "-dev",
        VersionStage::Alpha => "-alpha",
        VersionStage::Beta => "-beta",
        VersionStage::Release => "",
    };
    format!("{}.{}.{}{stage}+{}", v.major, v.minor, v.bug, v.build)
}

/// Result of converting a whole effect.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelEffect {
    /// The definition to register in `bae_model::EffectRegistry`.
    pub def: EffectDef,
    /// Gaps per param (disk id), in definition order.
    pub gaps: Vec<(DiskId, ModelGap)>,
}

/// Convert a described and parameterised AE effect. The param list is
/// validated first (dense indices, unique disk ids, balanced groups).
pub fn effect_def(
    desc: &PluginDescriptor,
    caps: &EffectCaps,
    params: &[ParamDescriptor],
) -> Result<ModelEffect> {
    validate_param_list(params)?;
    let mut defs = Vec::with_capacity(params.len());
    let mut gaps = Vec::new();
    for p in params {
        let m = param_def(p);
        gaps.extend(m.gaps.into_iter().map(|g| (p.disk_id, g)));
        defs.push(m.def);
    }
    let req = caps.requirements();
    Ok(ModelEffect {
        def: EffectDef {
            match_name: desc.match_name.as_str().to_owned(),
            name: desc.name.clone(),
            category: desc.category.clone(),
            plugin: Some(PluginRef {
                id: COMPAT_PLUGIN_ID.to_owned(),
                version: version_string(desc.version),
            }),
            params: defs,
            thread_safe: req.concurrency != Concurrency::SerialPerModule,
            gpu: req.gpu,
        },
        gaps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Fixed;
    use crate::params::{ParamFlags, ParamName, ParamUiFlags};

    fn p(index: u32, kind: ParamKind) -> ParamDescriptor {
        ParamDescriptor {
            index,
            disk_id: DiskId(index + 100),
            name: ParamName::new(format!("p{index}")).unwrap(),
            kind,
            flags: ParamFlags::default(),
            ui: ParamUiFlags::default(),
            custom_ui_size: None,
        }
    }

    fn sample_descriptor() -> PluginDescriptor {
        use crate::pipl::{
            CodeArch, DescriptorSource, EntryPoint, MatchName, PluginKind, RawOutFlags, SpecVersion,
        };
        PluginDescriptor {
            kind: PluginKind::Effect,
            name: "Fancy Glow".into(),
            category: "Vendor".into(),
            match_name: MatchName::new("VENDOR Fancy Glow").unwrap(),
            version: EffectVersion {
                major: 1,
                minor: 2,
                bug: 0,
                stage: VersionStage::Release,
                build: 1,
            },
            spec_version: SpecVersion {
                major: 13,
                minor: 28,
            },
            entry_points: vec![EntryPoint {
                arch: CodeArch::MacArm64,
                symbol: "EffectMain".into(),
            }],
            declared_flags: RawOutFlags::default(),
            support_url: None,
            search_keywords_json: None,
            description: None,
            source: DescriptorSource::Pipl,
        }
    }

    fn all_kinds() -> Vec<ParamKind> {
        vec![
            ParamKind::Layer {
                default_self: false,
            },
            ParamKind::IntSlider {
                valid: (0, 100),
                slider: (0, 10),
                default: 3,
            },
            ParamKind::FixedSlider {
                valid: (Fixed(0), Fixed::ONE),
                slider: (Fixed(0), Fixed::ONE),
                default: Fixed(1 << 15),
                precision: 2,
                display: ValueDisplay::default(),
            },
            ParamKind::FloatSlider {
                valid: (0.0, 1000.0),
                slider: (0.0, 100.0),
                default: 10.0,
                precision: 1,
                display: ValueDisplay {
                    percent: true,
                    pixel: false,
                },
                want_phase: false,
                curve_tolerance: 0.0,
            },
            ParamKind::Angle {
                default: Fixed(45 << 16),
            },
            ParamKind::Checkbox {
                default: true,
                label: String::new(),
            },
            ParamKind::Color {
                default_rgb8: [255, 0, 51],
            },
            ParamKind::Popup {
                choices: vec![
                    PopupChoice::Item("A".into()),
                    PopupChoice::Separator,
                    PopupChoice::Item("B".into()),
                ],
                default: 3,
            },
            ParamKind::GroupStart,
            ParamKind::ArbitraryData {
                arb_type: 1,
                default_flat: vec![1, 2, 3],
            },
            ParamKind::Path { default_mask: 0 },
            ParamKind::GroupEnd,
            ParamKind::Button {
                label: String::new(),
            },
        ]
    }

    #[test]
    fn exact_kinds_convert_without_gaps() {
        for (i, k) in all_kinds().into_iter().enumerate() {
            let d = p(i as u32 + 1, k);
            let m = param_def(&d);
            assert!(m.gaps.is_empty(), "{:?}: {:?}", d.kind, m.gaps);
            assert_eq!(m.def.id, d.disk_id.0);
            assert_eq!(
                d.bae_mapping().shape.value_type(),
                m.def.kind.value_type(),
                "{:?}",
                d.kind
            );
            // The stored default must have the value type 02 expects.
            if let (Some(want), Some(v)) = (m.def.kind.value_type(), &m.def.default) {
                assert_eq!(v.value_type(), want, "{:?}", d.kind);
            }
        }
    }

    #[test]
    fn whole_effect_registers_and_checks_values() {
        let params: Vec<_> = all_kinds()
            .into_iter()
            .enumerate()
            .map(|(i, k)| p(i as u32 + 1, k))
            .collect();
        let desc = sample_descriptor();
        let caps = EffectCaps {
            smart_render: true,
            threaded_rendering: true,
            ..EffectCaps::default()
        };
        let m = effect_def(&desc, &caps, &params).unwrap();
        assert!(m.gaps.is_empty());
        assert!(m.def.thread_safe);
        assert_eq!(m.def.plugin.as_ref().unwrap().id, COMPAT_PLUGIN_ID);
        let mut reg = bae_model::EffectRegistry::new();
        let name = m.def.match_name.clone();
        reg.register(m.def.clone());
        // Popup: separators count as indices, as in AE (3 = "B").
        let popup = m.def.params.iter().find(|d| d.id == 108).unwrap();
        assert_eq!(popup.default, Some(Value::Int(3)));
        reg.check_param(&name, &bae_model::effect::param_attr(108), &Value::Int(3))
            .unwrap();
        assert!(
            reg.check_param(&name, &bae_model::effect::param_attr(108), &Value::Int(4))
                .is_err()
        );
        // Definition order is the AE param order (index - 1), groups included.
        let ids: Vec<u32> = m.def.params.iter().map(|d| d.id).collect();
        assert_eq!(ids, (101..=113).collect::<Vec<_>>());
    }

    #[test]
    fn gaps_are_reported_with_requests() {
        let mut d = p(
            1,
            ParamKind::Point {
                default_percent: (50.0, 50.0),
                restrict_bounds: false,
            },
        );
        d.flags.use_value_for_old_projects = true;
        let m = param_def(&d);
        assert_eq!(
            m.gaps,
            vec![ModelGap::RelativePointDefault, ModelGap::OldProjectValue]
        );
        let mut s = p(
            2,
            ParamKind::FloatSlider {
                valid: (0.0, 1.0),
                slider: (0.0, 1.0),
                default: 0.0,
                precision: 0,
                display: ValueDisplay::default(),
                want_phase: false,
                curve_tolerance: 0.0,
            },
        );
        s.flags.cannot_interp = true;
        assert_eq!(param_def(&s).gaps, vec![ModelGap::HoldOnlyFlag]);
        let n = param_def(&p(3, ParamKind::NoData));
        assert_eq!(n.gaps, vec![ModelGap::NoDataKind]);
        assert!(n.def.flags.custom_ui);
        for g in [
            ModelGap::NoDataKind,
            ModelGap::HoldOnlyFlag,
            ModelGap::RelativePointDefault,
            ModelGap::LayerDefaultSelf,
            ModelGap::MaskDefaultIndex,
            ModelGap::OldProjectValue,
            ModelGap::UiOnlyControl,
            ModelGap::CustomUiSize,
            ModelGap::ControlLabel,
            ModelGap::CosmeticUi,
        ] {
            let (class, req) = g.triage();
            assert_eq!(class == GapClass::NotPlanned, req == "-", "{g:?}");
        }
    }

    #[test]
    fn versions_format() {
        let v = EffectVersion {
            major: 2,
            minor: 1,
            bug: 0,
            stage: VersionStage::Beta,
            build: 7,
        };
        assert_eq!(version_string(v), "2.1.0-beta+7");
    }
}
