//! Chinese strings of the effects module (keys are the English text,
//! see `bae_ui::i18n`; AE's Simplified Chinese UI terms).

pub(crate) const ZH: &[(&str, &str)] = &[
    // Effect Controls / Effects & Presets.
    ("Swap Colors", "交换颜色"),
    ("Missing effect", "缺失效果"),
    ("BAE built-in effect", "BAE 内置效果"),
    ("Not available in BAE yet", "BAE 暂不支持"),
    ("Effect", "效果"),
    ("Effects & Presets", "效果和预设"),
    ("Search", "搜索"),
    // Effect commands (bae-motion `effect.*`).
    ("Apply Effect", "应用效果"),
    ("Paint Stroke", "绘画描边"),
    ("Add Puppet Pin", "添加操控点"),
    ("Remove Selected Puppet Parameters", "移除所选操控参数"),
    ("Create Puppet Mesh", "创建操控网格"),
    ("Remove Effect", "移除效果"),
    ("Reorder Effects", "排列效果"),
    ("Effect On/Off", "效果开关"),
    ("Cut", "剪切"),
    ("Add Shape", "添加形状"),
    ("New Mask", "新建蒙版"),
    ("Edit Path", "编辑路径"),
    ("Add Vertex", "添加顶点"),
    ("Delete Vertex", "删除顶点"),
    ("New Text", "新建文本"),
    ("Edit Text", "编辑文本"),
    ("Change Path", "更改路径"),
];
