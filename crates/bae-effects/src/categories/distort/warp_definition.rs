use crate::catalog::DefBuilder;
use bae_model::ParamKind;

pub(super) fn definition() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE SubspaceStabilizer", "Warp Stabilizer", "Distort");
    b.push(1, "", "", ParamKind::Display, None);
    b.push(11, "", "", ParamKind::Display, None);
    b.push(12, "Stabilization", "稳定", ParamKind::GroupStart, None);
    b = b
        .popup(19, "Result", "结果", 1, &["Smooth Motion", "No Motion"])
        .static_param();
    b = b
        .slider(
            20,
            "Smoothness",
            "平滑度",
            50.0,
            [0.0, 1000.0],
            [0.0, 100.0],
            1,
            "percent",
        )
        .static_param();
    b = b
        .popup(
            28,
            "Method",
            "方法",
            4,
            &[
                "Position",
                "Position, Scale, Rotation",
                "Perspective",
                "Subspace Warp",
            ],
        )
        .static_param();
    b = b
        .checkbox(44, "Preserve Scale", "保持缩放", false)
        .static_param();
    b.push(13, "", "", ParamKind::GroupEnd, None);
    b.push(8, "Borders", "边界", ParamKind::GroupStart, None);
    b = b
        .popup(
            22,
            "Framing",
            "取景",
            3,
            &[
                "Stabilize Only",
                "Stabilize, Crop",
                "Stabilize, Crop, Auto-scale",
                "Stabilize, Synthesize Edges",
            ],
        )
        .static_param();
    b.push(17, "Auto-scale", "自动缩放", ParamKind::GroupStart, None);
    b = b
        .slider(
            4,
            "Maximum Scale",
            "最大缩放",
            150.0,
            [100.0, 10000.0],
            [100.0, 150.0],
            1,
            "percent",
        )
        .static_param();
    b = b
        .slider(
            3,
            "Action-safe Margin",
            "动作安全边距",
            0.0,
            [0.0, 100.0],
            [0.0, 100.0],
            1,
            "percent",
        )
        .static_param();
    b.push(18, "", "", ParamKind::GroupEnd, None);
    b = b.slider(
        16,
        "Additional Scale",
        "其他缩放",
        100.0,
        [0.0, 10000.0],
        [0.0, 100.0],
        1,
        "percent",
    );
    b.push(9, "", "", ParamKind::GroupEnd, None);
    b.push(24, "Advanced", "高级", ParamKind::GroupStart, None);
    b = b
        .checkbox(36, "Detailed Analysis", "详细分析", false)
        .static_param();
    b = b
        .checkbox(308, "Fast Analysis", "快速分析", true)
        .static_param();
    b = b
        .popup(
            37,
            "Rolling Shutter Ripple",
            "果冻效应波纹",
            1,
            &["Automatic Reduction", "Enhanced Reduction"],
        )
        .static_param();
    b = b
        .slider(
            23,
            "Crop Less <-> Smooth More",
            "更少裁剪 <-> 更平滑",
            50.0,
            [0.0, 100.0],
            [0.0, 100.0],
            1,
            "percent",
        )
        .static_param();
    b = b
        .slider(
            38,
            "Synthesis Input Range (seconds)",
            "合成输入范围（秒）",
            0.5,
            [0.0, 999.0],
            [0.0, 100.0],
            1,
            "none",
        )
        .static_param();
    b = b.slider(
        35,
        "Synthesis Edge Feather",
        "合成边缘羽化",
        10.0,
        [0.0, 999.0],
        [0.0, 100.0],
        1,
        "none",
    );
    b.push(
        29,
        "Synthesis Edge Cropping",
        "合成边缘裁剪",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        31,
        "Left",
        "左侧",
        0.0,
        [0.0, 999.0],
        [0.0, 100.0],
        1,
        "none",
    );
    b = b.slider(
        32,
        "Top",
        "顶部",
        0.0,
        [0.0, 999.0],
        [0.0, 100.0],
        1,
        "none",
    );
    b = b.slider(
        33,
        "Right",
        "右侧",
        0.0,
        [0.0, 999.0],
        [0.0, 100.0],
        1,
        "none",
    );
    b = b.slider(
        34,
        "Bottom",
        "底部",
        0.0,
        [0.0, 999.0],
        [0.0, 100.0],
        1,
        "none",
    );
    b.push(30, "", "", ParamKind::GroupEnd, None);
    b = b
        .popup(
            47,
            "Objective",
            "目标",
            1,
            &[
                "Stabilize",
                "(-",
                "Reversible Stabilization",
                "Reverse Stabilization",
                "Apply Motion to Target",
                "Apply Motion to Target Over Original",
            ],
        )
        .static_param();
    b = b.layer(46, "Target Layer", "目标图层").static_param();
    b = b
        .checkbox(280, "Show Track Points", "显示跟踪点", false)
        .static_param();
    b = b
        .slider(
            301,
            "Track Point Size",
            "跟踪点大小",
            100.0,
            [0.0, 10000.0],
            [0.0, 100.0],
            1,
            "percent",
        )
        .static_param();
    b = b
        .checkbox(
            306,
            "Auto-delete Points Across Time",
            "跨时间自动删除点",
            true,
        )
        .static_param();
    b = b
        .checkbox(26, "Hide Warning Banner", "隐藏警告横幅", false)
        .static_param();
    b.push(25, "", "", ParamKind::GroupEnd, None);
    b.push(
        305,
        "AnalysisDeletedTrackPoints",
        "AnalysisDeletedTrackPoints",
        ParamKind::ArbitraryData,
        None,
    );
    b
}

pub(super) fn options() -> Vec<(u32, Vec<String>)> {
    vec![
        (19, ["平滑运动", "无运动"].map(str::to_owned).to_vec()),
        (
            28,
            ["位置", "位置、缩放、旋转", "透视", "子空间变形"]
                .map(str::to_owned)
                .to_vec(),
        ),
        (
            22,
            [
                "仅稳定",
                "稳定、裁剪",
                "稳定、裁剪、自动缩放",
                "稳定、人工合成边缘",
            ]
            .map(str::to_owned)
            .to_vec(),
        ),
        (37, ["自动减小", "增强减小"].map(str::to_owned).to_vec()),
        (
            47,
            [
                "稳定",
                "(-",
                "可逆稳定",
                "反向稳定",
                "向目标应用运动",
                "在原始图层上向目标应用运动",
            ]
            .map(str::to_owned)
            .to_vec(),
        ),
    ]
}
