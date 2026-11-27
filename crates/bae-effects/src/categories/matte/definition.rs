use crate::catalog::DefBuilder;
use bae_model::ParamKind;

pub(super) fn definition() -> DefBuilder {
    let mut b = DefBuilder::new(bae_vision::ROTO, "Roto Brush & Refine Edge", "Matte");
    b = b
        .popup(1, "Version", "版本", 3, &["1.0 (Classic)", "2.0", "3.0"])
        .ae_match_name("/propagation_method")
        .static_param();
    b = b
        .popup(2, "Quality", "品质", 1, &["Standard", "Best"])
        .ae_match_name("/fm_quality")
        .static_param();
    b.push(3, "Propagation", "传播", ParamKind::GroupStart, None);
    b = b.ae_match_name("/roto brush propagation");
    b = b
        .slider(
            4,
            "Search Radius",
            "搜索半径",
            30.,
            [1., 100.],
            [1., 100.],
            1,
            "none",
        )
        .ae_match_name("/winsize")
        .static_param();
    b = b
        .checkbox(5, "View Search Region", "查看搜索区域", false)
        .ae_match_name("/viewsearchregion")
        .static_param();
    b = b
        .checkbox(6, "Enable Classic Controls", "  启用经典控件", false)
        .ae_match_name("/use_adobecut_too")
        .static_param();
    b = b
        .slider(
            7,
            "Motion Threshold",
            "运动阈值",
            50.,
            [0., 100.],
            [0., 100.],
            1,
            "none",
        )
        .ae_match_name("/staticdamping");
    b = b
        .slider(
            8,
            "Motion Damping",
            "运动阻尼",
            50.,
            [0., 100.],
            [0., 100.],
            1,
            "none",
        )
        .ae_match_name("/dynamicdamping");
    b = b
        .popup(
            9,
            "Edge Detection",
            "边缘检测",
            2,
            &["Favor Predicted Edges", "Balanced", "Favor Current Edges"],
        )
        .ae_match_name("/edgedetection");
    b = b
        .checkbox(
            10,
            "Use Alternate Color Estimation",
            "使用备选颜色估计",
            false,
        )
        .ae_match_name("/enfromulate");
    b.push(11, "", "", ParamKind::GroupEnd, None);
    b = b.ae_match_name("/10");
    b = b
        .checkbox(12, "Invert Foreground/Background", "反转前台/后台", false)
        .ae_match_name("/invertforegroundbackground")
        .static_param();
    b = b
        .checkbox(13, "Fine-tune Roto Brush Matte", "微调 Roto 笔刷遮罩", true)
        .ae_match_name("/rmrefinematteenable")
        .static_param();
    b.push(
        14,
        "Roto Brush Matte",
        "Roto 笔刷遮罩",
        ParamKind::GroupStart,
        None,
    );
    b = b.ae_match_name("/refine_rb_groupstart");
    b = b
        .slider(
            15,
            "Feather",
            "羽化",
            5.,
            [0., 1000.],
            [0., 1000.],
            1,
            "none",
        )
        .ae_match_name("/spatialsmoothing2");
    b = b
        .slider(
            16,
            "Contrast",
            "对比度",
            80.,
            [0., 100.],
            [0., 100.],
            1,
            "none",
        )
        .ae_match_name("/spatialsoftness2");
    b = b
        .slider(
            17,
            "Shift Edge",
            "移动边缘",
            0.,
            [-100., 100.],
            [-100., 100.],
            1,
            "none",
        )
        .ae_match_name("/rmshiftedges");
    b = b
        .slider(
            18,
            "Reduce Chatter",
            "减少震颤",
            0.,
            [0., 100.],
            [0., 100.],
            1,
            "none",
        )
        .ae_match_name("/rmreducechatter");
    b.push(19, "", "", ParamKind::GroupEnd, None);
    b = b.ae_match_name("/18");
    b = b
        .checkbox(20, "Render Refine Edge", "渲染调整边缘", true)
        .ae_match_name("/renderHB")
        .static_param();
    b = b
        .slider(
            21,
            "Base Refine Edge Radius",
            "基准调整边缘半径",
            0.,
            [0., 250.],
            [0., 250.],
            1,
            "none",
        )
        .ae_match_name("/baserefineedgeradius");
    b = b
        .checkbox(22, "Fine-tune Refine Edge Matte", "微调调整边缘遮罩", true)
        .ae_match_name("/rmrefinematte2enable")
        .static_param();
    b.push(
        23,
        "Refine Edge Matte",
        "调整边缘遮罩",
        ParamKind::GroupStart,
        None,
    );
    b = b.ae_match_name("/refine_hb_groupstart");
    b = b
        .slider(24, "Smooth", "平滑", 0., [0., 50.], [0., 50.], 1, "none")
        .ae_match_name("/HBspatialsmoothing");
    b = b
        .slider(
            25,
            "Feather",
            "羽化",
            0.,
            [0., 1000.],
            [0., 1000.],
            1,
            "none",
        )
        .ae_match_name("/HBspatialsoftness2");
    b = b
        .slider(
            26,
            "Contrast",
            "对比度",
            0.,
            [0., 100.],
            [0., 100.],
            1,
            "none",
        )
        .ae_match_name("/HBspatialcontrast");
    b = b
        .slider(
            27,
            "Shift Edge",
            "移动边缘",
            0.,
            [-100., 100.],
            [-100., 100.],
            1,
            "none",
        )
        .ae_match_name("/HBspatialchoke");
    b = b
        .popup(
            28,
            "Chatter Reduction",
            "震颤减少",
            1,
            &["Off", "(-", "More Detailed", "Smoother (Slower)"],
        )
        .ae_match_name("/chatter_reduction2");
    b = b
        .slider(
            29,
            "Reduce Chatter",
            "减少震颤",
            50.,
            [0., 400.],
            [0., 400.],
            1,
            "none",
        )
        .ae_match_name("/rm2reducechatter");
    b.push(30, "", "", ParamKind::GroupEnd, None);
    b = b.ae_match_name("/29");
    b = b
        .checkbox(31, "Use Motion Blur", "使用运动模糊", false)
        .ae_match_name("/rmusemotionblur")
        .static_param();
    b.push(32, "Motion Blur", "运动模糊", ParamKind::GroupStart, None);
    b = b.ae_match_name("/mblur_groupstart");
    b = b
        .slider(
            33,
            "Samples Per Frame",
            "每帧样本",
            8.,
            [2., 128.],
            [2., 128.],
            1,
            "none",
        )
        .ae_match_name("/rmsamplesperframe");
    b = b
        .slider(
            34,
            "Shutter Angle",
            "快门角度",
            180.,
            [1., 720.],
            [1., 720.],
            1,
            "none",
        )
        .ae_match_name("/rmshutterangle");
    b = b
        .checkbox(35, "Higher Quality", "较高品质", false)
        .ae_match_name("/rmhigherquality");
    b.push(36, "", "", ParamKind::GroupEnd, None);
    b = b.ae_match_name("/35");
    b = b
        .checkbox(37, "Decontaminate Edge Colors", "净化边缘颜色", false)
        .ae_match_name("/rmcleanedgecolors")
        .static_param();
    b.push(38, "Decontamination", "净化", ParamKind::GroupStart, None);
    b = b.ae_match_name("/decontam_groupstart");
    b = b
        .slider(
            39,
            "Amount",
            "数量",
            100.,
            [0., 100.],
            [0., 100.],
            1,
            "none",
        )
        .ae_match_name("/rmcleaningamount");
    b = b
        .checkbox(40, "Extend Where Smoothed", "扩展平滑的地方", false)
        .ae_match_name("/rmextendedcleaning");
    b = b
        .slider(
            41,
            "Increase Radius",
            "增大半径",
            0.,
            [0., 80.],
            [0., 80.],
            1,
            "none",
        )
        .ae_match_name("/rmadditionalcleaningradius");
    b = b
        .checkbox(42, "View Map", "查看地图", false)
        .ae_match_name("/rmviewedgecleaningmap");
    b.push(43, "", "", ParamKind::GroupEnd, None);
    b = b.ae_match_name("/42");
    // AE exposes Strokes as an indexed property group with no scalar type.
    // The brush edits it on the layer canvas. The inspector shows its label.
    b.push(44, "Strokes", "描边", ParamKind::Display, None);
    b = b.ae_match_name("ADBE Samurai Strokes Group");
    b
}
