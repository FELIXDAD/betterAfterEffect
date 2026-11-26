//! Original BAE animation presets, organized using the AE preset folders.

use bae_model::{Keyframe, Time, Value, effect::param_attr};
use bae_motion::effect::PresetEffect;

pub(crate) struct Recipe {
    pub folder: &'static str,
    pub name: &'static str,
    effect: &'static str,
    values: &'static [(u32, f64)],
    /// Parameter, start and end values, duration in seconds.
    ramp: Option<(u32, f64, f64, f64)>,
}

impl Recipe {
    pub fn build(&self) -> PresetEffect {
        let mut effect = PresetEffect {
            match_name: self.effect.into(),
            values: self
                .values
                .iter()
                .map(|&(id, value)| (param_attr(id), Value::Float(value)))
                .collect(),
            ..Default::default()
        };
        if let Some((id, start, end, duration)) = self.ramp {
            effect.keys.insert(
                param_attr(id),
                vec![
                    (Time::ZERO, Keyframe::linear(Value::Float(start))),
                    (
                        Time::from_seconds(duration),
                        Keyframe::linear(Value::Float(end)),
                    ),
                ],
            );
        }
        effect
    }
}

// Sorted by folder so the panel can visit each branch without allocating a tree.
#[rustfmt::skip]
pub(crate) const LIBRARY: &[Recipe] = &[
    Recipe { folder: "Adobe Express/In", name: "Soft Focus Entrance", effect: "ADBE Gaussian Blur 2", values: &[], ramp: Some((1, 20.0, 0.0, 0.4)) },
    Recipe { folder: "Adobe Express/Looping", name: "Light Drift", effect: "ADBE Fractal Noise", values: &[(4, 76.0)], ramp: Some((23, 0.0, 360.0, 4.0)) },
    Recipe { folder: "Adobe Express/Out", name: "Soft Focus Exit", effect: "ADBE Gaussian Blur 2", values: &[], ramp: Some((1, 0.0, 20.0, 0.4)) },
    Recipe { folder: "Backgrounds", name: "Slow Cloud Field", effect: "ADBE Fractal Noise", values: &[(4, 84.0), (10, 165.0)], ramp: Some((23, 0.0, 110.0, 5.0)) },
    Recipe { folder: "Behaviors", name: "Exposure Lift", effect: "ADBE Exposure2", values: &[], ramp: Some((3, -1.0, 0.0, 0.75)) },
    Recipe { folder: "Image - Creative", name: "Faded Silver", effect: "ADBE Tint", values: &[(3, 78.0)], ramp: None },
    Recipe { folder: "Image - Special Effects", name: "Fine Grain", effect: "ADBE Noise2", values: &[(1, 8.0)], ramp: None },
    Recipe { folder: "Image - Utilities", name: "Midtone Lift", effect: "ADBE Exposure2", values: &[(5, 1.16)], ramp: None },
    Recipe { folder: "Legacy", name: "Soft Monochrome", effect: "ADBE Tint", values: &[(3, 94.0)], ramp: None },
    Recipe { folder: "Shapes/Backgrounds", name: "Paper Texture", effect: "ADBE Fractal Noise", values: &[(4, 27.0), (10, 18.0)], ramp: None },
    Recipe { folder: "Shapes/Elements", name: "Soft Edge", effect: "ADBE Gaussian Blur 2", values: &[(1, 6.0)], ramp: None },
    Recipe { folder: "Shapes/Sprites - Animated", name: "Iris Reveal", effect: "ADBE Radial Wipe", values: &[(5, 12.0)], ramp: Some((1, 100.0, 0.0, 0.65)) },
    Recipe { folder: "Shapes/Sprites - Still", name: "Bright Accent", effect: "ADBE Brightness & Contrast 2", values: &[(1, 14.0), (2, 18.0)], ramp: None },
    Recipe { folder: "Sound Effects", name: "Room Slap", effect: "ADBE Aud Delay", values: &[(1, 94.0), (2, 22.0), (3, 9.0), (5, 28.0)], ramp: None },
    Recipe { folder: "Synthetics", name: "Fine Smoke", effect: "ADBE Fractal Noise", values: &[(4, 136.0), (10, 51.0)], ramp: Some((23, 0.0, 210.0, 4.0)) },
    Recipe { folder: "Text/Animate In", name: "Type in Focus", effect: "ADBE Gaussian Blur 2", values: &[], ramp: Some((1, 18.0, 0.0, 0.55)) },
    Recipe { folder: "Text/Animate Out", name: "Soft Type Exit", effect: "ADBE Linear Wipe", values: &[(3, 64.0)], ramp: Some((1, 0.0, 100.0, 0.55)) },
    Recipe { folder: "Text/Blurs", name: "Ink Bloom", effect: "ADBE Gaussian Blur 2", values: &[], ramp: Some((1, 0.0, 8.0, 1.0)) },
    Recipe { folder: "Text/Fill and Stroke", name: "Silver Lettering", effect: "ADBE Tint", values: &[(3, 87.0)], ramp: None },
    Recipe { folder: "Transitions - Dissolves", name: "Tile Dissolve", effect: "ADBE Block Dissolve", values: &[(2, 9.0), (3, 9.0), (4, 2.0)], ramp: Some((1, 0.0, 100.0, 0.8)) },
    Recipe { folder: "Transitions - Movement", name: "Clock Sweep", effect: "ADBE Radial Wipe", values: &[(5, 16.0)], ramp: Some((1, 0.0, 100.0, 0.8)) },
    Recipe { folder: "Transitions - Wipes", name: "Feathered Diagonal", effect: "ADBE Linear Wipe", values: &[(2, 37.0), (3, 72.0)], ramp: Some((1, 0.0, 100.0, 0.7)) },
];

pub(super) const ZH: &[(&str, &str)] = &[
    ("In", "进入"),
    ("Looping", "循环播放"),
    ("Out", "退出"),
    ("Backgrounds", "背景"),
    ("Behaviors", "行为"),
    ("Image - Creative", "图像 - 创意"),
    ("Image - Special Effects", "图像 - 特殊效果"),
    ("Image - Utilities", "图像 - 实用工具"),
    ("Legacy", "旧版"),
    ("Shapes", "形状"),
    ("Elements", "元素"),
    ("Sprites - Animated", "精灵 - 动画"),
    ("Sprites - Still", "精灵 - 静态"),
    ("Sound Effects", "声音效果"),
    ("Synthetics", "合成纹理"),
    ("Animate In", "进入动画"),
    ("Animate Out", "退出动画"),
    ("Blurs", "模糊"),
    ("Fill and Stroke", "填充和描边"),
    ("Transitions - Dissolves", "过渡 - 溶解"),
    ("Transitions - Movement", "过渡 - 移动"),
    ("Transitions - Wipes", "过渡 - 擦除"),
    ("User Presets", "用户预设"),
    ("Soft Focus Entrance", "柔焦入场"),
    ("Light Drift", "光影渐变"),
    ("Soft Focus Exit", "柔焦退场"),
    ("Slow Cloud Field", "缓动云层"),
    ("Exposure Lift", "曝光渐亮"),
    ("Faded Silver", "褪色银灰"),
    ("Fine Grain", "细颗粒"),
    ("Midtone Lift", "提亮中间调"),
    ("Soft Monochrome", "柔和单色"),
    ("Paper Texture", "纸张纹理"),
    ("Soft Edge", "柔化轮廓"),
    ("Iris Reveal", "径向显现"),
    ("Bright Accent", "亮色点缀"),
    ("Room Slap", "房间短延迟"),
    ("Fine Smoke", "细烟"),
    ("Type in Focus", "文字渐清"),
    ("Soft Type Exit", "文字渐隐"),
    ("Ink Bloom", "墨迹晕开"),
    ("Silver Lettering", "银灰字色"),
    ("Tile Dissolve", "方格溶解"),
    ("Clock Sweep", "钟面扫过"),
    ("Feathered Diagonal", "斜向柔边擦除"),
];
