use crate::catalog::DefBuilder;
use bae_model::ParamKind;

pub(super) fn definition() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Rolling Shutter", "Rolling Shutter Repair", "Distort");
    b = b.slider(
        1,
        "Rolling Shutter Rate",
        "果冻效应率",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        1,
        "percent",
    );
    b = b
        .popup(
            2,
            "Scan Direction",
            "扫描方向",
            1,
            &[
                "Top -> Bottom",
                "Bottom -> Top",
                "Left -> Right",
                "Right -> Left",
            ],
        )
        .static_param();
    b.push(3, "Advanced", "高级", ParamKind::GroupStart, None);
    b = b
        .popup(4, "Method", "方法", 1, &["Warp", "Pixel Motion"])
        .static_param();
    b = b
        .checkbox(5, "Detailed Analysis", "详细分析", false)
        .static_param();
    b = b.slider(
        6,
        "Pixel Motion Detail",
        "像素运动细节",
        20.0,
        [5.0, 100.0],
        [5.0, 100.0],
        1,
        "percent",
    );
    b.push(7, "Advanced", "高级", ParamKind::GroupEnd, None);
    b
}

pub(super) fn options() -> Vec<(u32, Vec<String>)> {
    vec![
        (
            2,
            [
                "顶层 -> 底部",
                "底部 -> 顶层",
                "左边 -> 右边",
                "右边 -> 左边",
            ]
            .map(str::to_owned)
            .to_vec(),
        ),
        (4, ["变形", "像素运动"].map(str::to_owned).to_vec()),
    ]
}
