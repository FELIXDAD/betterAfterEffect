use crate::catalog::DefBuilder;
use crate::{
    Builtin,
    categories::color::ocio::Operation,
    effects::lut::ocio_effect,
    effects::sampled::{Sampled, Shader, unchanged},
    params::Params,
    rust_builtin,
};
use bae_render::IRect;
use std::sync::Arc;

pub(super) static GROW_BOUNDS: Shader = Shader {
    label: "bae.effects/grow_bounds",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("grow_bounds.wgsl")
    ),
};

/// AE grows the layer's buffer by Pixels on every side (at the render scale),
/// so effects after it can draw outside the original bounds.
fn grown(v: Params<'_>, input: IRect) -> IRect {
    input.inflate((v.f(1).max(0.) * v.p.scale).ceil() as i32)
}

fn cineon_converter() -> Builtin {
    let mut b = DefBuilder::new("ADBE Cineon Converter2", "Cineon Converter", "Utility");
    b = b.popup(
        1,
        "Conversion Type",
        "转换类型",
        2,
        &["Linear to Log", "Log to Linear", "Log to Log"],
    );
    b = b.slider(
        2,
        "10 Bit Black Point",
        "10 位黑场",
        95.0,
        [0.0, 1023.0],
        [0.0, 1023.0],
        2,
        "none",
    );
    b = b.slider(
        8,
        "Internal Black Point",
        "内部黑场",
        0.,
        [0., 1.],
        [0., 1.],
        3,
        "none",
    );
    b = b.slider(
        4,
        "10 Bit White Point",
        "10 位白场",
        685.0,
        [0.0, 1023.0],
        [0.0, 1023.0],
        2,
        "none",
    );
    b = b.slider(
        9,
        "Internal White Point",
        "内部白场",
        1.,
        [0., 1.],
        [0., 1.],
        3,
        "none",
    );
    b = b.slider(
        6,
        "Gamma",
        "灰度系数",
        1.699997,
        [0.009995, 5.0],
        [0.009995, 5.0],
        2,
        "none",
    );
    b = b.slider(
        7,
        "Highlight Rolloff",
        "高光滤除",
        20.0,
        [0.0, 150.0],
        [0.0, 150.0],
        2,
        "none",
    );
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (ocio_effect(def, Operation::Cineon), labels),
        "cineon-converter",
        "Cineon 转换器",
        &["胶片", "对数"],
    );
    effect.info.param_options_zh.push((
        1,
        ["线性到对数", "对数到线性", "对数到对数"]
            .map(str::to_owned)
            .to_vec(),
    ));
    effect
}

fn color_profile_converter() -> Builtin {
    let mut b = DefBuilder::new(
        "ADBE ProfileToProfile",
        "Color Profile Converter",
        "Utility",
    );
    b = b.popup(1, "Input Profile", "输入配置文件", 1, INPUT_PROFILES);
    b = b.checkbox(2, "Linearize Input Profile", "线性化输入配置文件", false);
    b = b.popup(3, "Output Profile", "输出配置文件", 1, OUTPUT_PROFILES);
    b = b.checkbox(4, "Linearize Output Profile", "线性化输出配置文件", false);
    b = b.popup(
        5,
        "Intent",
        "意图",
        3,
        &[
            "Perceptual",
            "Saturation",
            "Relative Colorimetric",
            "Absolute Colorimetric",
        ],
    );
    b = b.checkbox(6, "Use Black Point Compensation", "使用黑场补偿", true);
    b = b.popup(
        7,
        "Scene-referred Profile Compensation",
        "场景参考配置文件补偿",
        1,
        &["Use Project Settings", "On", "Off"],
    );
    let (def, labels) = b.build();
    let mut effect = rust_builtin(
        (ocio_effect(def, Operation::Profile), labels),
        "color-profile-converter",
        "颜色配置文件转换器",
        &["色彩管理"],
    );
    effect.info.param_options_zh.push((
        1,
        INPUT_PROFILES
            .iter()
            .map(|v| {
                match *v {
                    "Project Working Space" => "项目工作空间",
                    "Embedded" => "嵌入",
                    other => other,
                }
                .to_owned()
            })
            .collect(),
    ));
    effect.info.param_options_zh.push((
        3,
        OUTPUT_PROFILES
            .iter()
            .map(|v| {
                match *v {
                    "Project Working Space" => "项目工作空间",
                    "Embedded" => "嵌入",
                    other => other,
                }
                .to_owned()
            })
            .collect(),
    ));
    effect.info.param_options_zh.push((
        5,
        ["感性", "饱和度", "相对色度分析", "绝对色度分析"]
            .map(str::to_owned)
            .to_vec(),
    ));
    effect.info.param_options_zh.push((
        7,
        ["使用项目设置", "打开", "关闭"].map(str::to_owned).to_vec(),
    ));
    effect
}

fn grow_bounds() -> Builtin {
    let mut b = DefBuilder::new("ADBE GROW BOUNDS", "Grow Bounds", "Utility");
    b = b.slider(
        1,
        "Pixels",
        "像素",
        10.0,
        [0.0, 10000.0],
        [0.0, 10000.0],
        2,
        "none",
    );
    let (def, labels) = b.build();
    rust_builtin(
        (
            Arc::new(Sampled {
                def,
                shader: &GROW_BOUNDS,
                mode: 0,
                inputs: &[],
                read: unchanged,
                bounds: grown,
            }),
            labels,
        ),
        "grow-bounds",
        "范围扩散",
        &["范围扩散"],
    )
}

pub(crate) fn entries() -> Vec<Builtin> {
    vec![cineon_converter(), color_profile_converter(), grow_bounds()]
}

const INPUT_PROFILES: &[&str] = &[
    "Project Working Space",
    "(-",
    "Adobe RGB (1998)",
    "Apple RGB",
    "ColorMatch RGB",
    "image P3",
    "ProPhoto RGB",
    "sRGB IEC61966-2.1",
    "(-",
    "Fujifilm ETERNA 250 Printing Density (by Adobe)",
    "Fujifilm ETERNA 250D Printing Density (by Adobe)",
    "Fujifilm ETERNA 400 Printing Density (by Adobe)",
    "Fujifilm ETERNA 500 Printing Density (by Adobe)",
    "Fujifilm ETERNA Vivid 160 Printing Density (by Adobe)",
    "Fujifilm F-125 Printing Density (by Adobe)",
    "Fujifilm F-64D Printing Density (by Adobe)",
    "Fujifilm REALA 500D Printing Density (by Adobe)",
    "Kodak 5205/7205 Printing Density (by Adobe)",
    "Kodak 5218/7218 Printing Density (by Adobe)",
    "Kodak 5229/7229 Printing Density (by Adobe)",
    "Universal Camera Film Printing Density",
    "ACEScct",
    "ARRI LogC3 Wide Color Gamut - EI 1000",
    "ARRI LogC3 Wide Color Gamut - EI 1280",
    "ARRI LogC3 Wide Color Gamut - EI 160",
    "ARRI LogC3 Wide Color Gamut - EI 1600",
    "ARRI LogC3 Wide Color Gamut - EI 200",
    "ARRI LogC3 Wide Color Gamut - EI 250",
    "ARRI LogC3 Wide Color Gamut - EI 320",
    "ARRI LogC3 Wide Color Gamut - EI 400",
    "ARRI LogC3 Wide Color Gamut - EI 500",
    "ARRI LogC3 Wide Color Gamut - EI 640",
    "ARRI LogC3 Wide Color Gamut - EI 800",
    "ARRI WCG Preview LUT for P3",
    "ARRI WCG Preview LUT for P3D65",
    "ARRI WCG Preview LUT for Rec. 709",
    "ARRIFLEX D-20 Daylight Log (by Adobe)",
    "ARRIFLEX D-20 Tungsten Log (by Adobe)",
    "Canon Cinema CLog2",
    "Canon Cinema CLog3",
    "Panasonic V-Gamut V-Log",
    "Panavision Genesis Tungsten Log (by Adobe)",
    "Rec.709 V-Log",
    "Sony S-Gamut3.Cine/S-Log3",
    "Sony S-Gamut3/S-Log3",
    "Viper FilmStream Daylight Log (by Adobe)",
    "Viper FilmStream Tungsten Log (by Adobe)",
    "Fujifilm 3510 (RDI) Theater Preview (by Adobe)",
    "Fujifilm 3513DI (RDI) Theater Preview (by Adobe)",
    "Fujifilm 3521XD (RDI) Theater Preview (by Adobe)",
    "Kodak 2383 Theater Preview 2 (by Adobe)",
    "Kodak 2393 Theater Preview 2 (by Adobe)",
    "Kodak 2395 Theater Preview 2 (by Adobe)",
    "(-",
    "ACES Academy Color Encoding Specification SMPTE ST 2065-1",
    "ACEScct",
    "ACEScg ACES Working Space AMPAS S-2014-004",
    "ARRI LogC3 Wide Color Gamut - EI 1000",
    "ARRI LogC3 Wide Color Gamut - EI 1280",
    "ARRI LogC3 Wide Color Gamut - EI 160",
    "ARRI LogC3 Wide Color Gamut - EI 1600",
    "ARRI LogC3 Wide Color Gamut - EI 200",
    "ARRI LogC3 Wide Color Gamut - EI 250",
    "ARRI LogC3 Wide Color Gamut - EI 320",
    "ARRI LogC3 Wide Color Gamut - EI 400",
    "ARRI LogC3 Wide Color Gamut - EI 500",
    "ARRI LogC3 Wide Color Gamut - EI 640",
    "ARRI LogC3 Wide Color Gamut - EI 800",
    "ARRI WCG Preview LUT for P3",
    "ARRI WCG Preview LUT for P3D65",
    "ARRI WCG Preview LUT for Rec. 709",
    "ARRIFLEX D-20 Daylight Log (by Adobe)",
    "ARRIFLEX D-20 Tungsten Log (by Adobe)",
    "Canon Cinema CLog2",
    "Canon Cinema CLog3",
    "CIE RGB",
    "DCDM X'Y'Z' D55 Gamma 2.6",
    "DCDM X'Y'Z' D60 Gamma 2.6",
    "DCDM X'Y'Z' D65 Gamma 2.6",
    "DCDM X'Y'Z' DCI Gamma 2.6",
    "e-sRGB",
    "Fujifilm 3510 (RDI) Theater Preview (by Adobe)",
    "Fujifilm 3513DI (RDI) Theater Preview (by Adobe)",
    "Fujifilm 3521XD (RDI) Theater Preview (by Adobe)",
    "Fujifilm ETERNA 250 Printing Density (by Adobe)",
    "Fujifilm ETERNA 250D Printing Density (by Adobe)",
    "Fujifilm ETERNA 400 Printing Density (by Adobe)",
    "Fujifilm ETERNA 500 Printing Density (by Adobe)",
    "Fujifilm ETERNA Vivid 160 Printing Density (by Adobe)",
    "Fujifilm F-125 Printing Density (by Adobe)",
    "Fujifilm F-64D Printing Density (by Adobe)",
    "Fujifilm REALA 500D Printing Density (by Adobe)",
    "HDTV (Rec. 709) 16-235",
    "Kodak 2383 Theater Preview 2 (by Adobe)",
    "Kodak 2393 Theater Preview 2 (by Adobe)",
    "Kodak 2395 Theater Preview 2 (by Adobe)",
    "Kodak 5205/7205 Printing Density (by Adobe)",
    "Kodak 5218/7218 Printing Density (by Adobe)",
    "Kodak 5229/7229 Printing Density (by Adobe)",
    "P3 D55 Gamma 2.6",
    "P3 D60 Gamma 2.6",
    "P3 D65 Gamma 2.4",
    "P3 D65 Gamma 2.6",
    "P3 D65 PQ",
    "P3 D65 PQ W100",
    "P3 D65 PQ W203",
    "P3 D65 PQ W300",
    "P3 DCI Gamma 2.6",
    "P3 DCI PQ",
    "PAL/SECAM",
    "Panasonic V-Gamut V-Log",
    "Panavision Genesis Tungsten Log (by Adobe)",
    "Rec.2020 Gamma 2.4",
    "Rec.2100 HLG Scene W100",
    "Rec.2100 HLG Scene W63",
    "Rec.2100 HLG Scene W75",
    "Rec.2100 HLG Scene W81",
    "Rec.2100 HLG W100",
    "Rec.2100 HLG W1000",
    "Rec.2100 HLG W203",
    "Rec.2100 HLG W300",
    "Rec.2100 PQ",
    "Rec.2100 PQ W100",
    "Rec.2100 PQ W10000",
    "Rec.2100 PQ W203",
    "Rec.2100 PQ W300",
    "Rec.601 NTSC Gamma 2.4",
    "Rec.601 PAL Gamma 2.4",
    "Rec.709 Gamma 2.4",
    "Rec.709 HLG Scene W100",
    "Rec.709 HLG Scene W63",
    "Rec.709 HLG Scene W75",
    "Rec.709 HLG Scene W81",
    "Rec.709 PQ",
    "Rec.709 PQ W100",
    "Rec.709 PQ W203",
    "Rec.709 PQ W300",
    "Rec.709 V-Log",
    "ROMM-RGB",
    "SDTV NTSC 16-235",
    "SDTV PAL 16-235",
    "SMPTE-C",
    "Sony S-Gamut3.Cine/S-Log3",
    "Sony S-Gamut3/S-Log3",
    "Universal Camera Film Printing Density",
    "Viper FilmStream Daylight Log (by Adobe)",
    "Viper FilmStream Tungsten Log (by Adobe)",
    "Wide Gamut RGB",
    "* wscRGB",
    "* wsRGB",
    "(-",
    "Embedded",
];

const OUTPUT_PROFILES: &[&str] = &[
    "Project Working Space",
    "(-",
    "Adobe RGB (1998)",
    "Apple RGB",
    "ColorMatch RGB",
    "image P3",
    "ProPhoto RGB",
    "sRGB IEC61966-2.1",
    "(-",
    "Fujifilm ETERNA 250 Printing Density (by Adobe)",
    "Fujifilm ETERNA 250D Printing Density (by Adobe)",
    "Fujifilm ETERNA 400 Printing Density (by Adobe)",
    "Fujifilm ETERNA 500 Printing Density (by Adobe)",
    "Fujifilm ETERNA Vivid 160 Printing Density (by Adobe)",
    "Fujifilm F-125 Printing Density (by Adobe)",
    "Fujifilm F-64D Printing Density (by Adobe)",
    "Fujifilm REALA 500D Printing Density (by Adobe)",
    "Kodak 5205/7205 Printing Density (by Adobe)",
    "Kodak 5218/7218 Printing Density (by Adobe)",
    "Kodak 5229/7229 Printing Density (by Adobe)",
    "Universal Camera Film Printing Density",
    "ACEScct",
    "ARRI LogC3 Wide Color Gamut - EI 1000",
    "ARRI LogC3 Wide Color Gamut - EI 1280",
    "ARRI LogC3 Wide Color Gamut - EI 160",
    "ARRI LogC3 Wide Color Gamut - EI 1600",
    "ARRI LogC3 Wide Color Gamut - EI 200",
    "ARRI LogC3 Wide Color Gamut - EI 250",
    "ARRI LogC3 Wide Color Gamut - EI 320",
    "ARRI LogC3 Wide Color Gamut - EI 400",
    "ARRI LogC3 Wide Color Gamut - EI 500",
    "ARRI LogC3 Wide Color Gamut - EI 640",
    "ARRI LogC3 Wide Color Gamut - EI 800",
    "ARRI WCG Preview LUT for P3",
    "ARRI WCG Preview LUT for P3D65",
    "ARRI WCG Preview LUT for Rec. 709",
    "ARRIFLEX D-20 Daylight Log (by Adobe)",
    "ARRIFLEX D-20 Tungsten Log (by Adobe)",
    "Canon Cinema CLog2",
    "Canon Cinema CLog3",
    "Panasonic V-Gamut V-Log",
    "Panavision Genesis Tungsten Log (by Adobe)",
    "Rec.709 V-Log",
    "Sony S-Gamut3.Cine/S-Log3",
    "Sony S-Gamut3/S-Log3",
    "Viper FilmStream Daylight Log (by Adobe)",
    "Viper FilmStream Tungsten Log (by Adobe)",
    "(-",
    "ACES Academy Color Encoding Specification SMPTE ST 2065-1",
    "ACEScct",
    "ACEScg ACES Working Space AMPAS S-2014-004",
    "ARRI LogC3 Wide Color Gamut - EI 1000",
    "ARRI LogC3 Wide Color Gamut - EI 1280",
    "ARRI LogC3 Wide Color Gamut - EI 160",
    "ARRI LogC3 Wide Color Gamut - EI 1600",
    "ARRI LogC3 Wide Color Gamut - EI 200",
    "ARRI LogC3 Wide Color Gamut - EI 250",
    "ARRI LogC3 Wide Color Gamut - EI 320",
    "ARRI LogC3 Wide Color Gamut - EI 400",
    "ARRI LogC3 Wide Color Gamut - EI 500",
    "ARRI LogC3 Wide Color Gamut - EI 640",
    "ARRI LogC3 Wide Color Gamut - EI 800",
    "ARRI WCG Preview LUT for P3",
    "ARRI WCG Preview LUT for P3D65",
    "ARRI WCG Preview LUT for Rec. 709",
    "ARRIFLEX D-20 Daylight Log (by Adobe)",
    "ARRIFLEX D-20 Tungsten Log (by Adobe)",
    "Canon Cinema CLog2",
    "Canon Cinema CLog3",
    "CIE RGB",
    "DCDM X'Y'Z' D55 Gamma 2.6",
    "DCDM X'Y'Z' D60 Gamma 2.6",
    "DCDM X'Y'Z' D65 Gamma 2.6",
    "DCDM X'Y'Z' DCI Gamma 2.6",
    "e-sRGB",
    "Fujifilm 3510 (RDI) Theater Preview (by Adobe)",
    "Fujifilm 3513DI (RDI) Theater Preview (by Adobe)",
    "Fujifilm 3521XD (RDI) Theater Preview (by Adobe)",
    "Fujifilm ETERNA 250 Printing Density (by Adobe)",
    "Fujifilm ETERNA 250D Printing Density (by Adobe)",
    "Fujifilm ETERNA 400 Printing Density (by Adobe)",
    "Fujifilm ETERNA 500 Printing Density (by Adobe)",
    "Fujifilm ETERNA Vivid 160 Printing Density (by Adobe)",
    "Fujifilm F-125 Printing Density (by Adobe)",
    "Fujifilm F-64D Printing Density (by Adobe)",
    "Fujifilm REALA 500D Printing Density (by Adobe)",
    "HDTV (Rec. 709) 16-235",
    "Kodak 2383 Theater Preview 2 (by Adobe)",
    "Kodak 2393 Theater Preview 2 (by Adobe)",
    "Kodak 2395 Theater Preview 2 (by Adobe)",
    "Kodak 5205/7205 Printing Density (by Adobe)",
    "Kodak 5218/7218 Printing Density (by Adobe)",
    "Kodak 5229/7229 Printing Density (by Adobe)",
    "P3 D55 Gamma 2.6",
    "P3 D60 Gamma 2.6",
    "P3 D65 Gamma 2.4",
    "P3 D65 Gamma 2.6",
    "P3 D65 PQ",
    "P3 D65 PQ W100",
    "P3 D65 PQ W203",
    "P3 D65 PQ W300",
    "P3 DCI Gamma 2.6",
    "P3 DCI PQ",
    "PAL/SECAM",
    "Panasonic V-Gamut V-Log",
    "Panavision Genesis Tungsten Log (by Adobe)",
    "Rec.2020 Gamma 2.4",
    "Rec.2100 HLG Scene W100",
    "Rec.2100 HLG Scene W63",
    "Rec.2100 HLG Scene W75",
    "Rec.2100 HLG Scene W81",
    "Rec.2100 HLG W100",
    "Rec.2100 HLG W1000",
    "Rec.2100 HLG W203",
    "Rec.2100 HLG W300",
    "Rec.2100 PQ",
    "Rec.2100 PQ W100",
    "Rec.2100 PQ W10000",
    "Rec.2100 PQ W203",
    "Rec.2100 PQ W300",
    "Rec.601 NTSC Gamma 2.4",
    "Rec.601 PAL Gamma 2.4",
    "Rec.709 Gamma 2.4",
    "Rec.709 HLG Scene W100",
    "Rec.709 HLG Scene W63",
    "Rec.709 HLG Scene W75",
    "Rec.709 HLG Scene W81",
    "Rec.709 PQ",
    "Rec.709 PQ W100",
    "Rec.709 PQ W203",
    "Rec.709 PQ W300",
    "Rec.709 V-Log",
    "ROMM-RGB",
    "SDTV NTSC 16-235",
    "SDTV PAL 16-235",
    "SMPTE-C",
    "Sony S-Gamut3.Cine/S-Log3",
    "Sony S-Gamut3/S-Log3",
    "Universal Camera Film Printing Density",
    "Viper FilmStream Daylight Log (by Adobe)",
    "Viper FilmStream Tungsten Log (by Adobe)",
    "Wide Gamut RGB",
    "* wscRGB",
    "* wsRGB",
    "(-",
    "Embedded",
];

// Cineon controls describe an OCIO LogAffineTransform. Code values are in
// 10-bit film density units; the black and white points anchor the linear side.
pub(crate) fn cineon_processor(
    s: [f64; 9],
) -> Result<bae_color::ColorTransform, bae_color::ColorError> {
    use bae_color::{ColorOp, ColorTransform, LogOp};
    let make = |black: f64, white: f64, lo: f64, hi: f64, gamma: f64, inverse: bool| {
        let slope = 300. / gamma.max(0.01);
        let toe = 10f64.powf((black - white) / slope);
        let lin = (1. - toe) / (hi - lo).max(0.00001);
        ColorTransform::from_ops([ColorOp::Log(LogOp {
            base: 10.,
            log_slope: [slope / 1023.; 3],
            log_offset: [white / 1023.; 3],
            lin_slope: [lin; 3],
            lin_offset: [toe - lo * lin; 3],
            lin_break: None,
            linear_slope: None,
            inverse,
        })])
    };
    let grade = make(s[1], s[3], s[7], s[8], s[5], s[0] as i32 != 1);
    Ok(if s[0] as i32 == 3 {
        grade.then(&make(95., 685., 0., 1., 1.7, false))
    } else {
        grade
    })
}
pub(crate) fn profile_processor(
    s: [i64; 9],
) -> Result<bae_color::ColorTransform, bae_color::ColorError> {
    use bae_color::{ColorError, ColorSpace, builtin as b};
    let choose = |id: i64, table: &[&str], linear: bool| -> Result<ColorSpace, ColorError> {
        let label = table
            .get(id.saturating_sub(1) as usize)
            .copied()
            .unwrap_or("");
        let space = if label == "Project Working Space" || label == "Embedded" {
            if s[8] == 1 { b::SRGB_LINEAR } else { b::SRGB }
        } else if label == "Adobe RGB (1998)" {
            b::ADOBE_RGB
        } else if label == "image P3" {
            b::DISPLAY_P3
        } else if label.starts_with("sRGB") || label == "* wsRGB" {
            b::SRGB
        } else if label == "* wscRGB" {
            b::SRGB_LINEAR
        } else if label == "ACEScct" {
            b::ACESCCT
        } else if label.starts_with("ACEScg") {
            b::ACESCG
        } else if label.starts_with("ACES Academy") {
            b::ACES2065_1
        } else if label == "ARRI LogC3 Wide Color Gamut - EI 800" {
            b::LOGC3_AWG3
        } else if label == "Canon Cinema CLog3" {
            b::CLOG3_CINEMA_GAMUT
        } else if label == "Sony S-Gamut3.Cine/S-Log3" {
            b::SLOG3_SGAMUT3_CINE
        } else if label == "Sony S-Gamut3/S-Log3" {
            b::SLOG3_SGAMUT3
        } else if label == "Panasonic V-Gamut V-Log" {
            b::VLOG_VGAMUT
        } else if label == "Rec.709 Gamma 2.4" {
            b::REC709_GAMMA24
        } else if label == "Rec.2020 Gamma 2.4" {
            b::REC2020_GAMMA24
        } else if label == "Rec.2100 PQ" {
            b::REC2100_PQ
        } else if label == "Rec.2100 HLG W1000" {
            b::REC2100_HLG
        } else {
            return Err(ColorError::Unsupported(format!("ICC profile: {label}")));
        };
        Ok(if linear {
            space.linearized().unwrap_or(space)
        } else {
            space
        })
    };
    let input = choose(s[0], INPUT_PROFILES, s[1] != 0)?;
    let output = choose(s[2], OUTPUT_PROFILES, s[3] != 0)?;
    input.transform_to(&output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cineon_code_points_and_project_profile() {
        let t = cineon_processor([2., 95., 0., 685., 0., 1.7, 20., 0., 1.]).unwrap();
        let rgb = t.apply_f64([95. / 1023., 685. / 1023., 685. / 1023.]);
        assert!(rgb[0].abs() < 1e-6 && (rgb[1] - 1.).abs() < 1e-6);
        let rgb = [0.2, 0.5, 0.8];
        assert_eq!(
            profile_processor([1, 0, 1, 0, 3, 1, 1, 0, 0])
                .unwrap()
                .apply_f64(rgb),
            rgb
        );
        let fx = grow_bounds().effect();
        assert!(fx.renders_pixels());
        let instance = bae_render::RenderEffect {
            path: bae_model::PrimPath::new("/fx").unwrap(),
            effect: "ADBE GROW BOUNDS".into(),
            enabled: true,
            params: vec![(1, bae_model::Value::Float(10.))],
        };
        let p = bae_render::EffectParams {
            scale: 0.5,
            ..bae_render::EffectParams::new(&instance)
        };
        assert_eq!(
            fx.output_bounds(&p, IRect::new(0, 0, 20, 20)),
            IRect::new(-5, -5, 25, 25)
        );
    }
}
