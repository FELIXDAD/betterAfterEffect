use crate::{Builtin, catalog::DefBuilder, rust_builtin};
use bae_model::{ParamKind, Value};
use std::sync::Arc;
pub(super) fn entry() -> Builtin {
    let mut b = DefBuilder::new("ADBE 3D Tracker", "3D Camera Tracker", "Perspective");
    for id in [1, 11] {
        b.push(id, "", "", ParamKind::Display, None);
    }
    b = b
        .popup(
            403,
            "Shot Type",
            "拍摄类型",
            1,
            &[
                "Fixed Angle of View",
                "Variable Zoom",
                "(-",
                "Specify Angle of View",
            ],
        )
        .static_param()
        .slider(
            414,
            "Horizontal Angle of View",
            "水平视角",
            0.,
            [0., 180.],
            [0., 180.],
            2,
            "degrees",
        )
        .static_param()
        .popup(
            405,
            "Show Track Points",
            "显示轨迹点",
            2,
            &["2D Source", "3D Solved"],
        )
        .static_param()
        .checkbox(406, "Render Track Points", "渲染跟踪点", false)
        .static_param()
        .slider(
            407,
            "Track Point Size",
            "跟踪点大小",
            100.,
            [0., 10000.],
            [0., 200.],
            1,
            "percent",
        )
        .slider(
            409,
            "Target Size",
            "目标大小",
            100.,
            [0., 10000.],
            [0., 200.],
            1,
            "percent",
        );
    b.push(400, "", "", ParamKind::Display, None);
    b.push(24, "Advanced", "高级", ParamKind::GroupStart, None);
    b = b
        .popup(
            410,
            "Solve Method",
            "解决方法",
            1,
            &[
                "Auto Detect",
                "(-",
                "Typical",
                "Mostly Flat Scene",
                "Tripod Pan",
            ],
        )
        .static_param();
    b.push(
        313,
        "Average Error: -",
        "平均误差: -",
        ParamKind::Display,
        None,
    );
    b.push(314, "", "", ParamKind::Display, None);
    b.push(
        311,
        "Average Error: %.2f pixels",
        "平均误差: %.2f 像素",
        ParamKind::Display,
        None,
    );
    b.push(312, "", "", ParamKind::Display, None);
    b = b
        .checkbox(36, "Detailed Analysis", "详细分析", false)
        .static_param()
        .checkbox(
            306,
            "Auto-delete Points Across Time",
            "跨时间自动删除点",
            true,
        )
        .static_param()
        .checkbox(26, "Hide Warning Banner", "隐藏警告横幅", false)
        .static_param();
    b.push(25, "", "", ParamKind::GroupEnd, None);
    b.push(
        305,
        "AnalysisDeletedTrackPoints",
        "AnalysisDeletedTrackPoints",
        ParamKind::ArbitraryData,
        Some(Value::Bytes(Vec::new())),
    );
    let (mut def, labels) = b.build();
    for p in &mut def.params {
        p.flags.hidden = matches!(p.id, 1 | 11 | 400 | 314 | 311 | 312 | 305);
    }
    let mut entry = rust_builtin(
        (Arc::new(super::track_points::TrackPoints(def)), labels),
        "camera-tracker",
        "3D 摄像机跟踪器",
        &["camera", "tracking", "跟踪"],
    );
    entry.info.param_options_zh = vec![
        (403, vec!["视图的固定角度", "变量缩放", "(-", "指定视角"]),
        (405, vec!["2D 源", "3D 已解析"]),
        (
            410,
            vec!["自动检测", "(-", "典型", "最平场景", "三脚架全景"],
        ),
    ]
    .into_iter()
    .map(|(k, s)| (k, s.into_iter().map(String::from).collect()))
    .collect();
    entry
}
