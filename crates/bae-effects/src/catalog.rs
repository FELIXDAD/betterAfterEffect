//! Catalog metadata for the "Effects & Presets" panel (12), the AE importer
//! (15) and agents: names, categories and search keywords in English and
//! Simplified Chinese, the AE matchNames each effect stands in for, and how it
//! is implemented.

use bae_model::{EffectDef, EffectParamDef, ParamFlags, ParamKind, Value};

/// How a built-in effect is implemented (plugin spec §19).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// An embedded declarative WGSL package, pixel style (fusable).
    DeclarativePixel,
    /// An embedded declarative WGSL package, compute style (multi-pass).
    DeclarativeCompute,
    /// A Rust `bae_render::Effect` with its own WGSL kernels.
    Rust,
}

/// One built-in effect as listed in the Effects & Presets panel.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectInfo {
    /// Document identity (`Effect` prim `match_name`): the AE matchName for AE
    /// effects, so AE projects map 1:1 (`ADBE Gaussian Blur 2`).
    pub match_name: String,
    /// Contribution id inside the built-in plugin (`bae.effects/<id>`).
    pub full_id: String,
    /// English display name (AE's).
    pub name: String,
    /// Simplified Chinese display name (Adobe's official translation).
    pub name_zh: String,
    /// Category key (AE's English category, e.g. `Blur & Sharpen`).
    pub category: String,
    /// Search keywords, English and Chinese.
    pub keywords: Vec<String>,
    /// Implementation form.
    pub form: Form,
    /// Obsolete AE versions kept for old projects (listed under Obsolete).
    pub obsolete: bool,
    /// Chinese parameter names by parameter id.
    pub param_names_zh: Vec<(u32, String)>,
    /// Chinese popup options by parameter id (in option order).
    pub param_options_zh: Vec<(u32, Vec<String>)>,
}

impl EffectInfo {
    /// The Chinese parameter name of `id`, if translated.
    pub fn param_name_zh(&self, id: u32) -> Option<&str> {
        self.param_names_zh
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, n)| n.as_str())
    }

    /// True if `query` (case-insensitive) matches a name or keyword.
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        self.name.to_lowercase().contains(&q)
            || self.name_zh.contains(&q)
            || self.match_name.to_lowercase().contains(&q)
            || self.keywords.iter().any(|k| k.to_lowercase().contains(&q))
    }
}

/// Chinese name of an AE effect category (Adobe's official translations).
pub fn category_zh(category: &str) -> &'static str {
    match category {
        "3D Channel" => "3D 通道",
        "Audio" => "音频",
        "Blur & Sharpen" => "模糊和锐化",
        "Channel" => "通道",
        "Color Correction" => "颜色校正",
        "Distort" => "扭曲",
        "Expression Controls" => "表达式控制",
        "Generate" => "生成",
        "Keying" => "抠像",
        "Matte" => "遮罩",
        "Noise & Grain" => "杂色和颗粒",
        "Obsolete" => "过时",
        "Perspective" => "透视",
        "Simulation" => "模拟",
        "Stylize" => "风格化",
        "Text" => "文本",
        "Time" => "时间",
        "Transition" => "过渡",
        "Utility" => "实用工具",
        _ => "其他",
    }
}

/// Builds the `EffectDef` of a Rust built-in effect. Parameter ids are AE
/// disk ids and get the AE parameter matchName `<matchName>-NNNN`.
pub(crate) struct DefBuilder {
    def: EffectDef,
    zh: Vec<(u32, String)>,
}

impl DefBuilder {
    pub fn new(match_name: &str, name: &str, category: &str) -> Self {
        Self {
            def: EffectDef {
                match_name: match_name.to_owned(),
                name: name.to_owned(),
                category: category.to_owned(),
                plugin: None,
                params: Vec::new(),
                thread_safe: true,
                gpu: true,
            },
            zh: Vec::new(),
        }
    }

    pub(crate) fn push(
        &mut self,
        id: u32,
        name: &str,
        zh: &str,
        kind: ParamKind,
        default: Option<Value>,
    ) {
        self.def.params.push(EffectParamDef {
            id,
            name: name.to_owned(),
            match_name: Some(format!("{}-{id:04}", self.def.match_name)),
            kind,
            default,
            flags: ParamFlags::default(),
        });
        self.zh.push((id, zh.to_owned()));
    }

    /// Float slider: `valid` range, `slider` range, decimals, display token
    /// (`none`, `percent`, `pixels`, `degrees`).
    #[allow(clippy::too_many_arguments)]
    pub fn slider(
        mut self,
        id: u32,
        name: &str,
        zh: &str,
        default: f64,
        valid: [f64; 2],
        slider: [f64; 2],
        precision: u8,
        display: &str,
    ) -> Self {
        self.push(
            id,
            name,
            zh,
            ParamKind::Slider {
                valid_min: valid[0],
                valid_max: valid[1],
                slider_min: slider[0],
                slider_max: slider[1],
                precision,
                integer: false,
                display: display.to_owned(),
            },
            Some(Value::Float(default)),
        );
        self
    }

    /// Integer slider.
    pub fn int(mut self, id: u32, name: &str, zh: &str, default: i64, valid: [i64; 2]) -> Self {
        self.push(
            id,
            name,
            zh,
            ParamKind::Slider {
                valid_min: valid[0] as f64,
                valid_max: valid[1] as f64,
                slider_min: valid[0] as f64,
                slider_max: valid[1] as f64,
                precision: 0,
                integer: true,
                display: "none".to_owned(),
            },
            Some(Value::Int(default)),
        );
        self
    }

    pub fn checkbox(mut self, id: u32, name: &str, zh: &str, default: bool) -> Self {
        self.push(
            id,
            name,
            zh,
            ParamKind::Checkbox,
            Some(Value::Bool(default)),
        );
        self
    }

    /// Popup (1-based default).
    pub fn popup(mut self, id: u32, name: &str, zh: &str, default: i64, options: &[&str]) -> Self {
        self.push(
            id,
            name,
            zh,
            ParamKind::Popup {
                options: options.iter().map(|s| (*s).to_owned()).collect(),
            },
            Some(Value::Int(default)),
        );
        self
    }

    /// A same-comp layer reference. An unwritten default resolves to self.
    pub fn layer(mut self, id: u32, name: &str, zh: &str) -> Self {
        self.push(id, name, zh, ParamKind::Layer, None);
        let flags = &mut self.def.params.last_mut().unwrap().flags;
        flags.cannot_animate = true;
        flags.cannot_time_vary = true;
        self
    }

    /// The host's second menu for an image layer parameter. It has no AE disk ID.
    pub(crate) fn layer_stage(mut self, layer_id: u32) -> Self {
        self = self.popup(
            layer_stage_id(layer_id),
            "Input Stage",
            "输入阶段",
            1,
            &["Source", "Masks", "Effects & Masks"],
        );
        let stage = self.def.params.last_mut().unwrap();
        stage.match_name = None;
        stage.flags.cannot_animate = true;
        stage.flags.cannot_time_vary = true;
        self
    }

    pub fn build(self) -> (EffectDef, Vec<(u32, String)>) {
        (self.def, self.zh)
    }
}

pub(crate) const fn layer_stage_id(layer_id: u32) -> u32 {
    0x10000 + layer_id
}

/// Chinese popup labels for Rust effects in the controls and channel group,
/// in the same one-based order as the corresponding parameter definition.
pub fn options_zh(match_name: &str, param_id: u32) -> Option<&'static [&'static str]> {
    Some(match (match_name, param_id) {
        (_, 0x10001..=0x1ffff) => &["源", "蒙版", "效果和蒙版"],
        ("ADBE Echo", 5) => &[
            "相加",
            "最大值",
            "最小值",
            "滤色",
            "从后至前组合",
            "从前至后组合",
            "混合",
        ],
        ("ADBE Difference", 5) => &[
            "原始",
            "目标",
            "混合",
            "最大值",
            "完全打开",
            "(-",
            "结果亮度",
            "结果最大值",
            "(-",
            "Alpha 差值",
            "仅 Alpha 差值",
        ],
        ("ADBE Timewarp", 1) => &["全帧", "帧混合", "像素运动"],
        ("ADBE Timewarp", 2) => &["速度", "源帧"],
        ("ADBE Timewarp", 14) => &["正常", "极端"],
        ("ADBE Timewarp", 25) => &["手动", "自动"],
        ("ADBE Timewarp", 30) => &["明亮度", "反转明亮度", "Alpha", "反转 Alpha"],
        ("ADBE Timewarp", 32) => &["正常", "遮罩", "前景", "背景"],
        ("ADBE Gradient Wipe", 4) => &["拼贴渐变", "中心渐变", "伸缩渐变以适合"],
        ("ADBE Displacement Map", 2 | 4) => &[
            "红色",
            "绿色",
            "蓝色",
            "Alpha",
            "明亮度",
            "色相",
            "亮度",
            "饱和度",
            "完全",
            "一半",
            "关闭",
        ],
        ("ADBE Displacement Map", 6) => &["中心图", "伸缩对应图以适合", "拼贴图"],
        ("ADBE Texturize", 4) => &["拼贴纹理", "居中纹理", "拉伸纹理以适合"],
        ("ADBE Dropdown Control", 1) => &["项目 1", "项目 2", "项目 3"],
        (
            "ADBE Camera Lens Blur"
            | "ADBE Smart Blur"
            | "ADBE Channel Blur"
            | "CC Radial Blur"
            | "CC Radial Fast Blur"
            | "CC Vector Blur"
            | "CS CrossBlur",
            id,
        ) => return crate::effects::spatial::optics::definitions::options(match_name, id),
        ("ADBE Minimax", 1) => &["最小值", "最大值", "先最小值再最大值", "先最大值再最小值"],
        ("ADBE Minimax", 3) => &["颜色", "Alpha 和颜色", "红色", "绿色", "蓝色", "Alpha"],
        ("ADBE Minimax", 4) => &["水平和垂直", "仅水平", "仅垂直"],
        ("ADBE Simple Choker", 1) => &["最终输出", "遮罩"],
        ("ADBE Set Matte3", 2) => &[
            "红色通道",
            "绿色通道",
            "蓝色通道",
            "Alpha 通道",
            "明亮度",
            "色相",
            "亮度",
            "饱和度",
            "完全",
            "关闭",
        ],
        _ => return crate::effects::grain::definitions::options(match_name, param_id),
    })
}
