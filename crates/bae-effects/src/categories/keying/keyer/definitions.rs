use crate::catalog::DefBuilder;
use bae_model::{ParamKind, Value};

pub(super) fn keylight() -> DefBuilder {
    let mut b = DefBuilder::new("Keylight 906", "Keylight (1.2)", "Keying");
    b.push(1, "About", "About", ParamKind::Button, None);
    b = b.popup(
        2,
        "View",
        "View",
        11,
        &[
            "Source",
            "Source Alpha",
            "Corrected Source",
            "Colour Correction Edges",
            "Screen Matte",
            "Inside Mask",
            "Outside Mask",
            "Combined Matte",
            "Status",
            "Intermediate Result",
            "Final Result",
        ],
    );
    b = b.checkbox(3, "Unpremultiply Result", "Unpremultiply Result", true);
    b.push(
        4,
        "Screen Colour",
        "Screen Colour",
        ParamKind::Color,
        Some(Value::Color([0.0, 0.0, 0.0, 1.0])),
    );
    b = b.slider(
        5,
        "Screen Gain",
        "Screen Gain",
        100.0,
        [0.0, 5000.0],
        [0.0, 5000.0],
        2,
        "none",
    );
    b = b.slider(
        6,
        "Screen Balance",
        "Screen Balance",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(
        7,
        "Despill Bias",
        "Despill Bias",
        ParamKind::Color,
        Some(Value::Color([0.498039, 0.498039, 0.498039, 1.0])),
    );
    b.push(
        8,
        "Alpha Bias",
        "Alpha Bias",
        ParamKind::Color,
        Some(Value::Color([0.498039, 0.498039, 0.498039, 1.0])),
    );
    b = b.checkbox(9, "Lock Biases Together", "Lock Biases Together", true);
    b = b.slider(
        10,
        "Screen Pre-blur",
        "Screen Pre-blur",
        0.0,
        [0.0, 5000.0],
        [0.0, 5000.0],
        2,
        "none",
    );
    b.push(
        11,
        "Screen Matte",
        "Screen Matte",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        12,
        "Clip Black",
        "Clip Black",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        13,
        "Clip White",
        "Clip White",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        14,
        "Clip Rollback",
        "Clip Rollback",
        0.0,
        [0.0, 5000.0],
        [0.0, 5000.0],
        2,
        "none",
    );
    b = b.slider(
        15,
        "Screen Shrink/Grow",
        "Screen Shrink/Grow",
        0.0,
        [-5000.0, 5000.0],
        [-5000.0, 5000.0],
        2,
        "none",
    );
    b = b.slider(
        16,
        "Screen Softness",
        "Screen Softness",
        0.0,
        [0.0, 5000.0],
        [0.0, 5000.0],
        2,
        "none",
    );
    b = b.slider(
        17,
        "Screen Despot Black",
        "Screen Despot Black",
        0.0,
        [0.0, 5000.0],
        [0.0, 5000.0],
        2,
        "none",
    );
    b = b.slider(
        18,
        "Screen Despot White",
        "Screen Despot White",
        0.0,
        [0.0, 5000.0],
        [0.0, 5000.0],
        2,
        "none",
    );
    b = b.popup(
        19,
        "Replace Method",
        "Replace Method",
        4,
        &["None", "Source", "Hard Colour", "Soft Colour"],
    );
    b.push(
        20,
        "Replace Colour",
        "Replace Colour",
        ParamKind::Color,
        Some(Value::Color([0.498039, 0.498039, 0.498039, 1.0])),
    );
    b.push(
        21,
        "Screen Matte",
        "Screen Matte",
        ParamKind::GroupEnd,
        None,
    );
    b.push(
        22,
        "Inside Mask",
        "Inside Mask",
        ParamKind::GroupStart,
        None,
    );
    b.push(23, "Inside Mask", "Inside Mask", ParamKind::Path, None);
    b = b.slider(
        24,
        "Inside Mask Softness",
        "Inside Mask Softness",
        0.0,
        [0.0, 5000.0],
        [0.0, 5000.0],
        2,
        "none",
    );
    b = b.checkbox(25, "Invert", "Invert", false);
    b = b.popup(
        26,
        "Replace Method",
        "Replace Method",
        2,
        &["None", "Source", "Hard Colour", "Soft Colour"],
    );
    b.push(
        27,
        "Replace Colour",
        "Replace Colour",
        ParamKind::Color,
        Some(Value::Color([0.498039, 0.498039, 0.498039, 1.0])),
    );
    b = b.popup(
        28,
        "Source Alpha",
        "Source Alpha",
        3,
        &["Ignore", "Add To Inside Mask", "Normal"],
    );
    b.push(29, "Inside Mask", "Inside Mask", ParamKind::GroupEnd, None);
    b.push(
        30,
        "Outside Mask",
        "Outside Mask",
        ParamKind::GroupStart,
        None,
    );
    b.push(31, "Outside Mask", "Outside Mask", ParamKind::Path, None);
    b = b.slider(
        32,
        "Outside Mask Softness",
        "Outside Mask Softness",
        0.0,
        [0.0, 5000.0],
        [0.0, 5000.0],
        2,
        "none",
    );
    b = b.checkbox(33, "Invert", "Invert", false);
    b.push(
        34,
        "Outside Mask",
        "Outside Mask",
        ParamKind::GroupEnd,
        None,
    );
    b.push(
        35,
        "Foreground Colour Correction",
        "Foreground Colour Correction",
        ParamKind::GroupStart,
        None,
    );
    b = b.checkbox(
        36,
        "Enable Colour Correction",
        "Enable Colour Correction",
        false,
    );
    b = b.slider(
        37,
        "Saturation",
        "Saturation",
        100.0,
        [-5000.0, 5000.0],
        [-5000.0, 5000.0],
        2,
        "none",
    );
    b = b.slider(
        38,
        "Contrast",
        "Contrast",
        0.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        39,
        "Brightness",
        "Brightness",
        0.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "none",
    );
    b.push(
        40,
        "Colour Suppression",
        "Colour Suppression",
        ParamKind::GroupStart,
        None,
    );
    b = b.popup(
        41,
        "Suppress",
        "Suppress",
        1,
        &["None", "Red", "Green", "Blue", "Cyan", "Magenta", "Yellow"],
    );
    b = b.slider(
        42,
        "Suppression Balance",
        "Suppression Balance",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        43,
        "Suppression Amount",
        "Suppression Amount",
        100.0,
        [0.0, 5000.0],
        [0.0, 5000.0],
        2,
        "none",
    );
    b.push(
        44,
        "Colour Suppression",
        "Colour Suppression",
        ParamKind::GroupEnd,
        None,
    );
    b.push(
        45,
        "Colour Balancing",
        "Colour Balancing",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        46,
        "Hue",
        "Hue",
        0.0,
        [-5000.0, 5000.0],
        [-5000.0, 5000.0],
        2,
        "none",
    );
    b = b.slider(
        47,
        "Sat",
        "Sat",
        0.0,
        [-5000.0, 5000.0],
        [-5000.0, 5000.0],
        2,
        "none",
    );
    b.push(
        48,
        "Colour Balance Wheel",
        "Colour Balance Wheel",
        ParamKind::Button,
        None,
    );
    b.push(
        49,
        "Colour Balancing",
        "Colour Balancing",
        ParamKind::GroupEnd,
        None,
    );
    b.push(
        50,
        "Foreground Colour Correction",
        "Foreground Colour Correction",
        ParamKind::GroupEnd,
        None,
    );
    b.push(
        51,
        "Edge Colour Correction",
        "Edge Colour Correction",
        ParamKind::GroupStart,
        None,
    );
    b = b.checkbox(
        52,
        "Enable Edge Colour Correction",
        "Enable Edge Colour Correction",
        false,
    );
    b = b.slider(
        53,
        "Edge Hardness",
        "Edge Hardness",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        54,
        "Edge Softness",
        "Edge Softness",
        0.0,
        [0.0, 5000.0],
        [0.0, 5000.0],
        2,
        "none",
    );
    b = b.slider(
        55,
        "Edge Grow",
        "Edge Grow",
        0.0,
        [-5000.0, 5000.0],
        [-5000.0, 5000.0],
        2,
        "none",
    );
    b = b.slider(
        56,
        "Saturation",
        "Saturation",
        100.0,
        [-5000.0, 5000.0],
        [-5000.0, 5000.0],
        2,
        "none",
    );
    b = b.slider(
        57,
        "Contrast",
        "Contrast",
        0.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        58,
        "Brightness",
        "Brightness",
        0.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "none",
    );
    b.push(
        59,
        "Edge Colour Suppression",
        "Edge Colour Suppression",
        ParamKind::GroupStart,
        None,
    );
    b = b.popup(
        60,
        "Suppress",
        "Suppress",
        1,
        &["None", "Red", "Green", "Blue", "Cyan", "Magenta", "Yellow"],
    );
    b = b.slider(
        61,
        "Suppression Balance",
        "Suppression Balance",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        62,
        "Suppression Amount",
        "Suppression Amount",
        100.0,
        [0.0, 5000.0],
        [0.0, 5000.0],
        2,
        "none",
    );
    b.push(
        63,
        "Edge Colour Suppression",
        "Edge Colour Suppression",
        ParamKind::GroupEnd,
        None,
    );
    b.push(
        64,
        "Colour Balancing",
        "Colour Balancing",
        ParamKind::GroupStart,
        None,
    );
    b = b.slider(
        65,
        "Hue",
        "Hue",
        0.0,
        [-5000.0, 5000.0],
        [-5000.0, 5000.0],
        2,
        "none",
    );
    b = b.slider(
        66,
        "Sat",
        "Sat",
        0.0,
        [-5000.0, 5000.0],
        [-5000.0, 5000.0],
        2,
        "none",
    );
    b.push(
        67,
        "Colour Balance Wheel",
        "Colour Balance Wheel",
        ParamKind::Button,
        None,
    );
    b.push(
        68,
        "Colour Balancing",
        "Colour Balancing",
        ParamKind::GroupEnd,
        None,
    );
    b.push(
        69,
        "Edge Colour Correction",
        "Edge Colour Correction",
        ParamKind::GroupEnd,
        None,
    );
    b.push(
        70,
        "Source Crops",
        "Source Crops",
        ParamKind::GroupStart,
        None,
    );
    b = b.popup(
        71,
        "X Method",
        "X Method",
        1,
        &["Colour", "Repeat", "Reflect", "Wrap"],
    );
    b = b.popup(
        72,
        "Y Method",
        "Y Method",
        1,
        &["Colour", "Repeat", "Reflect", "Wrap"],
    );
    b.push(
        73,
        "Edge Colour",
        "Edge Colour",
        ParamKind::Color,
        Some(Value::Color([0.0, 0.0, 0.0, 1.0])),
    );
    b = b.slider(
        74,
        "Edge Colour Alpha",
        "Edge Colour Alpha",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        75,
        "Left",
        "Left",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        76,
        "Right",
        "Right",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(77, "Top", "Top", 0.0, [0.0, 100.0], [0.0, 100.0], 2, "none");
    b = b.slider(
        78,
        "Bottom",
        "Bottom",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b.push(
        79,
        "Source Crops",
        "Source Crops",
        ParamKind::GroupEnd,
        None,
    );
    b
}

pub(super) fn cleaner() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE KeyCleaner", "Key Cleaner", "Keying");
    b = b.slider(
        6,
        "Additional Edge Radius",
        "其他边缘半径",
        10.0,
        [0.0, 250.0],
        [0.0, 250.0],
        2,
        "none",
    );
    b = b.checkbox(2, "Reduce Chatter", "减少震颤", false);
    b = b.slider(
        16,
        "Alpha Contrast",
        "Alpha 对比度",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        1,
        "Strength",
        "强度",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b
}

pub(super) fn soft() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE RefineMatte2", "Refine Soft Matte", "Matte");
    b = b.checkbox(24, "Calculate Edge Details", "计算边缘细节", true);
    b = b.slider(
        25,
        "Additional Edge Radius",
        "其他边缘半径",
        10.0,
        [0.0, 250.0],
        [0.0, 250.0],
        2,
        "none",
    );
    b = b.checkbox(26, "View Edge Region", "查看边缘区域", false);
    b = b.slider(
        27,
        "Smooth",
        "平滑",
        0.0,
        [0.0, 50.0],
        [0.0, 50.0],
        2,
        "none",
    );
    b = b.slider(
        36,
        "Feather",
        "羽化",
        0.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b = b.slider(
        30,
        "Contrast",
        "对比度",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        33,
        "Shift Edge",
        "移动边缘",
        0.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "none",
    );
    b = b.popup(
        35,
        "Chatter Reduction",
        "震颤减少",
        1,
        &["Off", "(-", "More Detailed", "Smoother (Slower)"],
    );
    b = b.slider(
        29,
        "Reduce Chatter",
        "减少震颤",
        50.0,
        [0.0, 400.0],
        [0.0, 400.0],
        2,
        "none",
    );
    b = b.checkbox(8, "More Motion Blur", "更多运动模糊", false);
    b.push(17, "Motion Blur", "运动模糊", ParamKind::GroupStart, None);
    b = b.slider(
        9,
        "Samples Per Frame",
        "每帧样本",
        8.0,
        [2.0, 128.0],
        [2.0, 128.0],
        2,
        "none",
    );
    b = b.slider(
        10,
        "Shutter Angle",
        "快门角度",
        180.0,
        [1.0, 720.0],
        [1.0, 720.0],
        2,
        "none",
    );
    b = b.checkbox(14, "Higher Quality", "较高品质", false);
    b.push(18, "Higher Quality", "较高品质", ParamKind::GroupEnd, None);
    b = b.checkbox(11, "Decontaminate Edge Colors", "净化边缘颜色", true);
    b.push(19, "Decontamination", "净化", ParamKind::GroupStart, None);
    b = b.slider(
        22,
        "Amount",
        "数量",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.checkbox(12, "Extend Where Smoothed", "扩展平滑的地方", false);
    b = b.slider(
        2,
        "Increase Radius",
        "增大半径",
        0.0,
        [0.0, 80.0],
        [0.0, 80.0],
        2,
        "none",
    );
    b = b.checkbox(20, "View Map", "查看地图", false);
    b.push(21, "View Map", "查看地图", ParamKind::GroupEnd, None);
    b
}

pub(super) fn hard() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE RefineRBMatte", "Refine Hard Matte", "Matte");
    b = b.slider(
        24,
        "Feather",
        "羽化",
        2.0,
        [0.0, 1000.0],
        [0.0, 1000.0],
        2,
        "none",
    );
    b = b.slider(
        6,
        "Contrast",
        "对比度",
        80.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        23,
        "Shift Edge",
        "移动边缘",
        0.0,
        [-100.0, 100.0],
        [-100.0, 100.0],
        2,
        "none",
    );
    b = b.slider(
        16,
        "Reduce Chatter",
        "减少震颤",
        50.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.checkbox(8, "Use Motion Blur", "使用运动模糊", true);
    b.push(17, "Motion Blur", "运动模糊", ParamKind::GroupStart, None);
    b = b.slider(
        9,
        "Samples Per Frame",
        "每帧样本",
        8.0,
        [2.0, 128.0],
        [2.0, 128.0],
        2,
        "none",
    );
    b = b.slider(
        10,
        "Shutter Angle",
        "快门角度",
        180.0,
        [1.0, 720.0],
        [1.0, 720.0],
        2,
        "none",
    );
    b = b.checkbox(14, "Higher Quality", "较高品质", false);
    b.push(18, "Higher Quality", "较高品质", ParamKind::GroupEnd, None);
    b = b.checkbox(11, "Decontaminate Edge Colors", "净化边缘颜色", true);
    b.push(19, "Decontamination", "净化", ParamKind::GroupStart, None);
    b = b.slider(
        22,
        "Amount",
        "数量",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    b = b.checkbox(12, "Extend Where Smoothed", "扩展平滑的地方", false);
    b = b.slider(
        2,
        "Increase Radius",
        "增大半径",
        0.0,
        [0.0, 80.0],
        [0.0, 80.0],
        2,
        "none",
    );
    b = b.checkbox(20, "View Map", "查看地图", false);
    b.push(21, "View Map", "查看地图", ParamKind::GroupEnd, None);
    b
}

pub(crate) fn options(m: &str, id: u32) -> Option<&'static [&'static str]> {
    Some(match (m, id) {
        ("Keylight 906", 2) => &[
            "Source",
            "Source Alpha",
            "Corrected Source",
            "Colour Correction Edges",
            "Screen Matte",
            "Inside Mask",
            "Outside Mask",
            "Combined Matte",
            "Status",
            "Intermediate Result",
            "Final Result",
        ],
        ("Keylight 906", 19) => &["None", "Source", "Hard Colour", "Soft Colour"],
        ("Keylight 906", 26) => &["None", "Source", "Hard Colour", "Soft Colour"],
        ("Keylight 906", 28) => &["Ignore", "Add To Inside Mask", "Normal"],
        ("Keylight 906", 41) => &["None", "Red", "Green", "Blue", "Cyan", "Magenta", "Yellow"],
        ("Keylight 906", 60) => &["None", "Red", "Green", "Blue", "Cyan", "Magenta", "Yellow"],
        ("Keylight 906", 71) => &["Colour", "Repeat", "Reflect", "Wrap"],
        ("Keylight 906", 72) => &["Colour", "Repeat", "Reflect", "Wrap"],
        ("ADBE RefineMatte2", 35) => &["关闭", "(-", "更详细", "更平滑（更慢）"],
        _ => return None,
    })
}
