//! OCIO effect parameters select processors from the active config or BAE's built-in spaces.
use super::lut::ocio_effect;
use crate::{Builtin, catalog::DefBuilder, params::Params, rust_builtin};
use bae_color::{CdlOp, ColorError, ColorOp, ColorTransform, builtin, ocio::Config};
use bae_model::{EffectDef, ParamKind};
use bae_render::{EffectParams, RenderError};
use std::sync::OnceLock;

static SPACES: [bae_color::ColorSpace; 26] = builtin::ALL;
fn config() -> Result<Option<&'static Config>, RenderError> {
    static CONFIG: OnceLock<Result<Option<Config>, String>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            std::env::var_os("OCIO")
                .filter(|p| !p.is_empty())
                .map(|p| Config::load(p.as_ref()))
                .transpose()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map(Option::as_ref)
        .map_err(|e| RenderError::Other(e.clone()))
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Operation {
    Cdl,
    Space,
    Display,
    File,
    Look,
    Cineon,
    Profile,
}
impl Operation {
    pub(super) fn data_id(self) -> u32 {
        match self {
            Self::File => 5,
            Self::Cdl => 6,
            _ => 0,
        }
    }
    pub(super) fn selection(self, def: &EffectDef, p: &EffectParams<'_>) -> [i64; 9] {
        let params = Params::new(def, p);
        std::array::from_fn(|i| {
            if self == Self::Profile && i == 8 {
                return i64::from(p.linear);
            }
            let v = params.f(i as u32 + 1);
            if self == Self::Cineon {
                v.to_bits() as i64
            } else {
                v as i64
            }
        })
    }

    pub(super) fn transform(
        self,
        bytes: &[u8],
        s: [i64; 9],
    ) -> Result<ColorTransform, RenderError> {
        let grade = (|| -> Result<ColorTransform, ColorError> {
            if self == Self::Cineon {
                return super::utility_aliases::cineon_processor(
                    s.map(|v| f64::from_bits(v as u64)),
                );
            }
            if self == Self::Profile {
                return super::utility_aliases::profile_processor(s);
            }
            if self == Self::Cdl {
                return cdl(bytes, s[1], s[2] != 0);
            }
            if self == Self::File {
                let t = bae_color::lutfile::from_embedded(bytes)?;
                return if s[1] != 0 { t.inverse() } else { Ok(t) };
            }
            let cfg = config().map_err(|e| ColorError::Io(e.to_string()))?;
            let fallback = |n: i64, len: usize| {
                usize::try_from(n.saturating_sub(1))
                    .ok()
                    .filter(|i| *i < len)
                    .unwrap_or(0)
            };
            let t = if let Some(c) = cfg {
                let space = |n: i64| {
                    c.colorspaces
                        .get(fallback(n, c.colorspaces.len()))
                        .map(|v| v.name.as_str())
                        .ok_or_else(|| ColorError::Unsupported("no OCIO color spaces".into()))
                };
                match self {
                    Self::Display => {
                        let display = c
                            .displays
                            .get(fallback(s[1], c.displays.len()))
                            .ok_or_else(|| ColorError::Unsupported("no OCIO displays".into()))?;
                        let names = view_names(c);
                        let name = names
                            .get(fallback(s[2], names.len()))
                            .copied()
                            .filter(|n| display.views.iter().any(|v| v.name == *n))
                            .or_else(|| display.views.first().map(|v| v.name.as_str()))
                            .ok_or_else(|| ColorError::Unsupported("no OCIO views".into()))?;
                        c.display_view(space(s[0])?, &display.name, name)?
                    }
                    Self::Look if s[1] > 1 => {
                        let name = c
                            .look_names()
                            .nth((s[1] - 2) as usize)
                            .ok_or_else(|| ColorError::Unsupported("unknown OCIO look".into()))?;
                        c.look_transform(space(s[0])?, name, space(s[2])?)?
                    }
                    _ => c.processor(
                        space(s[0])?,
                        space(if self == Self::Look { s[2] } else { s[1] })?,
                    )?,
                }
            } else {
                let space = |n: i64| &SPACES[fallback(n, builtin::ALL.len())];
                let dst = match self {
                    Self::Look => space(s[2]),
                    Self::Display => match s[1] {
                        2 => &builtin::DISPLAY_P3,
                        3 => &builtin::REC709_GAMMA24,
                        _ => &builtin::SRGB,
                    },
                    _ => space(s[1]),
                };
                space(s[0]).transform_to(dst)?
            };
            if matches!(self, Self::Display | Self::Look) && s[3] != 0 {
                t.inverse()
            } else {
                Ok(t)
            }
        })();
        grade.map_err(|e| RenderError::Other(e.to_string()))
    }
}
fn view_names(config: &Config) -> Vec<&str> {
    let mut views = Vec::new();
    for v in config.displays.iter().flat_map(|d| &d.views) {
        if !views.contains(&v.name.as_str()) {
            views.push(v.name.as_str());
        }
    }
    views
}
fn cdl(bytes: &[u8], index: i64, inverse: bool) -> Result<ColorTransform, ColorError> {
    let error = |s: &str| ColorError::Parse {
        what: "ASC CDL".into(),
        message: s.into(),
    };
    let data = bytes.splitn(2, |b| *b == 0).nth(1).unwrap_or(bytes);
    let text = std::str::from_utf8(data).map_err(|_| error("expected UTF-8 XML"))?;
    let doc = roxmltree::Document::parse(text).map_err(|e| error(&e.to_string()))?;
    let correction = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "ColorCorrection")
        .nth(index.saturating_sub(1).max(0) as usize)
        .ok_or_else(|| error("missing ColorCorrection"))?;
    let values = |tag: &str| -> Result<Option<Vec<f64>>, ColorError> {
        correction
            .descendants()
            .find(|n| n.tag_name().name() == tag)
            .map(|n| {
                n.text()
                    .unwrap_or("")
                    .split_whitespace()
                    .map(|x| {
                        x.parse::<f64>()
                            .ok()
                            .filter(|v| v.is_finite())
                            .ok_or_else(|| error(tag))
                    })
                    .collect()
            })
            .transpose()
    };
    let triple = |name: &str, default: [f64; 3]| -> Result<[f64; 3], ColorError> {
        values(name)?
            .map(|v| v.try_into().map_err(|_| error(name)))
            .unwrap_or(Ok(default))
    };
    let sat = values("Saturation")?.unwrap_or_else(|| vec![1.]);
    let [saturation] = sat.as_slice() else {
        return Err(error("Saturation"));
    };
    Ok(ColorTransform::from_ops([ColorOp::Cdl(CdlOp {
        slope: triple("Slope", [1.; 3])?,
        offset: triple("Offset", [0.; 3])?,
        power: triple("Power", [1.; 3])?,
        saturation: *saturation,
        clamp: true,
        inverse,
    })]))
}
pub(crate) fn entries() -> Vec<Builtin> {
    let cfg = config().ok().flatten();
    let spaces = cfg
        .map(|c| {
            c.colorspaces
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| SPACES.iter().map(|s| s.name.as_ref()).collect());
    let displays = cfg
        .map(|c| {
            c.displays
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec!["sRGB", "Display P3", "Rec.709"]);
    let views = cfg.map(view_names).unwrap_or_else(|| vec!["Standard"]);
    let mut looks = vec!["None"];
    if let Some(c) = cfg {
        looks.extend(c.look_names());
    }
    [
        (
            Operation::Cdl,
            "OCIO CDL Transform",
            "OCIO CDL 变换",
            "ADBE OCIO CDL Transform",
            "ocio-cdl-transform",
        ),
        (
            Operation::Space,
            "OCIO Color Space Transform",
            "OCIO 颜色空间变换",
            "ADBE OCIO Color Space Transform",
            "ocio-color-space-transform",
        ),
        (
            Operation::Display,
            "OCIO Display Transform",
            "OCIO 显示变换",
            "ADBE OCIO Display Transform",
            "ocio-display-transform",
        ),
        (
            Operation::File,
            "OCIO File Transform",
            "OCIO 文件转换",
            "ADBE OCIO FILE Transform",
            "ocio-file-transform",
        ),
        (
            Operation::Look,
            "OCIO Look Transform",
            "OCIO 外观变换",
            "ADBE OCIO Look Transform",
            "ocio-look-transform",
        ),
    ]
    .into_iter()
    .map(|(op, en, zh, mn, id)| {
        let mut b = DefBuilder::new(mn, en, "Color Correction");
        let info = match op {
            Operation::Cdl | Operation::File => {
                b.push(1, "File", "文件", ParamKind::Button, None);
                let inv = if op == Operation::Cdl {
                    b = b.int(2, "CCCID", "CCCID", 1, [1, 1]);
                    3
                } else {
                    2
                };
                b = b
                    .checkbox(inv, "Invert Transform", "逆向变换", false)
                    .checkbox(inv + 1, " ", " ", false);
                inv + 2
            }
            Operation::Cineon | Operation::Profile => unreachable!("registered as utility aliases"),
            Operation::Space | Operation::Look | Operation::Display => {
                b = b.popup(1, "Input Color Space", "输入色彩空间", 1, &spaces);
                match op {
                    Operation::Space => {
                        b = b.popup(2, "Output Color Space", "输出色彩空间：", 1, &spaces)
                    }
                    Operation::Look => {
                        b = b
                            .popup(2, "Look", "Look", 1, &looks)
                            .popup(3, "Output Color Space", "输出色彩空间：", 1, &spaces)
                            .checkbox(4, "Invert Transform", "逆向变换", false)
                    }
                    _ => {
                        b = b
                            .popup(2, "Display Device", "显示设备", 1, &displays)
                            .popup(3, "View Transform", "视图变换", 1, &views)
                            .checkbox(4, "Invert Transform", "逆向变换", false)
                    }
                }
                let reset = if op == Operation::Space { 3 } else { 5 };
                b = b.checkbox(reset, "Reset Controls", "重置控制", false);
                reset + 1
            }
        };
        b.push(info, "Information", "信息", ParamKind::Button, None);
        if op.data_id() != 0 {
            b.push(op.data_id(), " ", " ", ParamKind::ArbitraryData, None);
        }
        let (mut def, labels) = b.build();
        // AE records opaque menu tokens for its config-dependent defaults.
        // An unresolved token selects the config's first space.
        let token = match op {
            Operation::Space => Some(954204852),
            Operation::Display => Some(522),
            Operation::Look => Some(527),
            _ => None,
        };
        if let Some(token) = token {
            def.params[0].default = Some(bae_model::Value::Int(token));
        }
        if matches!(op, Operation::Space | Operation::Look) {
            let id = if op == Operation::Space { 2 } else { 3 };
            if let Some(p) = def.params.iter_mut().find(|p| p.id == id) {
                p.default = Some(bae_model::Value::Int(0));
            }
        }
        for p in &mut def.params {
            p.flags.hidden = p.name.trim().is_empty();
        }
        rust_builtin(
            (ocio_effect(def, op), labels),
            id,
            zh,
            &["OpenColorIO", "OCIO", zh],
        )
    })
    .collect()
}

/// Validate and embed a selected CDL or LUT file for the OCIO effect controls.
pub fn embed_color_file(path: &std::path::Path, is_cdl: bool) -> Result<Vec<u8>, ColorError> {
    if !is_cdl {
        return bae_color::lutfile::embed(path);
    }
    let text = std::fs::read_to_string(path).map_err(|e| ColorError::Io(e.to_string()))?;
    let mut data = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .as_bytes()
        .to_vec();
    data.push(0);
    data.extend_from_slice(text.as_bytes());
    cdl(&data, 1, false)?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_cdl_grade_round_trips() {
        let data=b"<ColorCorrectionCollection><ColorCorrection/><ColorCorrection><SOPNode><Slope>2 1 1</Slope></SOPNode></ColorCorrection></ColorCorrectionCollection>";
        let input = [0.2, 0.3, 0.4];
        let result = cdl(data, 2, false).unwrap().apply_f64(input);
        assert_eq!(result, [0.4, 0.3, 0.4]);
        assert_eq!(cdl(data, 2, true).unwrap().apply_f64(result), input);
    }
}
