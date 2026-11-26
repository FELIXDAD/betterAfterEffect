use super::*;
use bae_model::ParamKind;

const RANGE: [f64; 2] = [-10_000_000., 10_000_000.];

pub(super) fn entries() -> [Builtin; 4] {
    let def = |id, name| DefBuilder::new(id, name, "3D Channel");
    let mut extract = entry(
        def("ADBE AUX CHANNEL EXTRACT", "3D Channel Extract")
            .popup(
                1,
                "3D Channel",
                "3D 通道",
                1,
                &[
                    "Z Depth",
                    "Object ID",
                    "Texture UV",
                    "Surface Normals",
                    "Coverage",
                    "Background RGB",
                    "Unclamped RGB",
                    "Material ID",
                ],
            )
            .static_param()
            .slider(
                2,
                "Black Point",
                "黑场",
                5000.,
                RANGE,
                [0., 5000.],
                1,
                "none",
            )
            .slider(3, "White Point", "白场", 0., RANGE, [0., 5000.], 1, "none")
            .checkbox(4, "Antialias", "消除锯齿", false)
            .checkbox(5, "Clamp Output", "固定输出", true)
            .checkbox(6, "Invert Depth Map", "反转景深映射", false),
        Method::Extract,
        "3d-channel-extract",
        "3D 通道提取",
    );
    extract.info.param_options_zh.push((
        1,
        [
            "Z 深度",
            "对象 ID",
            "纹理 UV",
            "曲面法线",
            "覆盖范围",
            "背景 RGB",
            "非固定 RGB",
            "材质 ID",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    let focus = entry(
        def("ADBE DEPTH FIELD", "Depth of Field")
            .slider(
                1,
                "Focal Plane",
                "焦平面",
                0.,
                RANGE,
                [0., 5000.],
                1,
                "none",
            )
            .slider(
                2,
                "Maximum Radius",
                "最大半径",
                0.,
                [0., 1000.],
                [0., 100.],
                1,
                "pixels",
            )
            .slider(
                3,
                "Focal Plane Thickness",
                "焦平面厚度",
                0.,
                [0., 10000.],
                [0., 1000.],
                1,
                "none",
            )
            .slider(
                4,
                "Focal Bias",
                "焦点偏移",
                50.,
                [0., 100.],
                [0., 100.],
                1,
                "percent",
            ),
        Method::Focus,
        "depth-of-field",
        "场深度",
    );
    let matte = entry(
        def("ADBE DEPTH MATTE", "Depth Matte")
            .slider(1, "Depth", "深度", 0., RANGE, [0., 5000.], 1, "none")
            .slider(
                2,
                "Feather",
                "羽化",
                0.,
                [0., 10000000.],
                [0., 100.],
                1,
                "none",
            )
            .checkbox(3, "Invert", "反转", false),
        Method::Matte,
        "depth-matte",
        "深度遮罩",
    );
    let mut fog = def("ADBE FOG_3D", "Fog 3D");
    fog.push(
        1,
        "Fog Color",
        "雾颜色",
        ParamKind::Color,
        Some(Value::Color([1., 1., 1., 0.015686])),
    );
    let fog = entry(
        fog.slider(
            2,
            "Fog Start Depth",
            "雾开始深度",
            0.,
            RANGE,
            [0., 5000.],
            1,
            "none",
        )
        .slider(
            3,
            "Fog End Depth",
            "雾结束深度",
            0.,
            RANGE,
            [0., 5000.],
            1,
            "none",
        )
        .slider(
            4,
            "Fog Opacity",
            "雾不透明度",
            100.,
            [0., 100.],
            [0., 100.],
            1,
            "percent",
        )
        .slider(
            5,
            "Scattering Density",
            "散布浓度",
            50.,
            [0., 100.],
            [0., 100.],
            1,
            "percent",
        )
        .checkbox(6, "Foggy Background", "多雾背景", true)
        .layer(7, "Gradient Layer", "渐变图层")
        .layer_stage(7)
        .slider(
            8,
            "Layer Contribution",
            "图层贡献",
            0.,
            [0., 100.],
            [0., 100.],
            1,
            "percent",
        ),
        Method::Fog,
        "fog-3d",
        "雾 3D",
    );
    [extract, focus, matte, fog]
}

pub(super) const OBJECT: &[&str] = &[
    "ObjectID",
    "Object.ID",
    "Object_ID",
    "IndexOB.X",
    "IndexOB",
    "id",
];
pub(super) const MATERIAL: &[&str] = &[
    "MaterialID",
    "Material.ID",
    "Material_ID",
    "IndexMA.X",
    "IndexMA",
];
const Z: &[&str] = &["Depth.Z", "Z", "Depth", "ZBack"];

pub(super) fn planes(c: &FootageChannels, method: Method, p: Params<'_>) -> [i32; 4] {
    let mut ids = [-1; 4];
    let names: &[&[&str]] = match if method == Method::Extract {
        p.popup(1)
    } else {
        1
    } {
        2 => &[OBJECT],
        3 => &[&["UV.U", "UV.X", "U"], &["UV.V", "UV.Y", "V"]],
        4 => &[
            &["Normal.X", "N.X", "NX"],
            &["Normal.Y", "N.Y", "NY"],
            &["Normal.Z", "N.Z", "NZ"],
        ],
        5 => &[&["Coverage", "Cov"]],
        6 => &[
            &["Background.R", "BG.R"],
            &["Background.G", "BG.G"],
            &["Background.B", "BG.B"],
        ],
        7 => &[
            &["Unclamped.R", "Beauty.R", "Combined.R", "R"],
            &["Unclamped.G", "Beauty.G", "Combined.G", "G"],
            &["Unclamped.B", "Beauty.B", "Combined.B", "B"],
        ],
        8 => &[MATERIAL],
        _ => &[Z],
    };
    for (slot, aliases) in ids.iter_mut().zip(names) {
        *slot = plane(c, aliases);
    }
    ids
}
