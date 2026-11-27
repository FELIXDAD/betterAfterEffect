//! AE's small panels of the Motion workspace (Window menu): Info, and next
//! Align, Character and Paragraph. Their Window menu items and shortcuts
//! (Cmd/Ctrl+2 …) work once the panels are registered (`late_bindings`).

pub mod align;
pub mod character;
pub mod info;
pub mod paragraph;
mod text_menu;

use bae_ui::PanelKind;

/// The panels, by id, replacing the shell's placeholders.
/// Registers the panels (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    for (id, kind) in panels() {
        crate::register_panel(app, id, kind);
    }
}

pub(crate) fn panels() -> Vec<(&'static str, PanelKind)> {
    vec![
        (
            "info",
            PanelKind::new("Info", |spawn| {
                Box::new(info::InfoPanel::from_state(spawn.state))
            }),
        ),
        (
            "align",
            PanelKind::new("Align", |spawn| {
                Box::new(align::AlignPanel::from_state(spawn.state))
            }),
        ),
        (
            "character",
            PanelKind::new("Character", |_| {
                Box::new(character::CharacterPanel::default())
            }),
        ),
        (
            "paragraph",
            PanelKind::new("Paragraph", |_| Box::new(paragraph::ParagraphPanel)),
        ),
    ]
}

/// Chinese for these panels (AE's zh_CN dictionary).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Auto Color Display", "自动颜色显示"),
    ("Percent", "百分比"),
    ("Web", "Web"),
    ("HSB", "HSB"),
    ("8-bpc (0-255)", "8-bpc (0-255)"),
    ("10-bpc (0-1023)", "10-bpc (0-1023)"),
    ("16-bpc (0-32768)", "16-bpc (0-32768)"),
    ("Decimal (0.0-1.0)", "小数 (0.0-1.0)"),
    (", ", "、"),
    ("Position:", "位置:"),
    ("\u{394}:", "\u{25b3}:"),
    ("Duration: {d}", "持续时间: {d}"),
    ("In: {in}, Out: {out}", "入: {in}，出: {out}"),
    // Align ($$$/AE/Plugin/Align/*).
    ("Align Layers to:", "将图层对齐到:"),
    ("Selection", "选区"),
    ("Composition", "合成"),
    ("Align Options", "对齐选项"),
    ("Distribute Layers:", "分布图层:"),
    ("Align Left", "左对齐"),
    ("Align Horizontally", "水平对齐"),
    ("Align Right", "右对齐"),
    ("Align Top", "顶对齐"),
    ("Align Vertically", "垂直对齐"),
    ("Align Bottom", "底对齐"),
    ("Distribute Top", "按顶分布"),
    ("Distribute Vertically", "垂直均匀分布"),
    ("Distribute Bottom", "按底分布"),
    ("Distribute Left", "按左分布"),
    ("Distribute Horizontally", "水平均匀分布"),
    ("Distribute Right", "按右分布"),
    // Character ($$$/AE/Character_Palette/*, $$$/AE/Plugin/CharPal/*).
    ("Set the font family", "设置字体系列"),
    ("Recent Fonts", "最近使用的字体"),
    ("Resolve Fonts", "解决字体问题"),
    (
        "These fonts are missing from this computer:",
        "此电脑上缺少以下字体：",
    ),
    ("Set the font style", "设置字体样式"),
    ("Set the font size", "设置字体大小"),
    ("Set the leading", "设置行距"),
    (
        "Set the kerning between two characters",
        "设置两个字符间的字偶间距",
    ),
    (
        "Set the tracking for the selected characters",
        "设置所选字符的字符间距",
    ),
    ("Set the stroke width", "设置描边宽度"),
    ("Vertical Scale", "垂直缩放"),
    ("Horizontal Scale", "水平缩放"),
    ("Set the baseline shift", "设置基线偏移"),
    (
        "Set the tsume for the selected characters",
        "设置所选字符的比例间距",
    ),
    ("Metrics", "度量标准"),
    ("Auto", "自动"),
    ("px", "像素"),
    ("Fill Over Stroke", "在描边上填充"),
    ("Stroke Over Fill", "在填充上描边"),
    ("All Fills Over All Strokes", "全部填充在全部描边之上"),
    ("All Strokes Over All Fills", "全部描边在全部填充之上"),
    ("Faux Bold", "仿粗体"),
    ("Faux Italic", "仿斜体"),
    ("All Caps", "全部大写字母"),
    ("Small Caps", "小型大写字母"),
    ("Superscript", "上标"),
    ("Subscript", "下标"),
    ("Ligatures", "连字"),
    ("Hindi Digits", "印地语数字"),
    ("Set to Black", "设置为黑色"),
    ("Set to White", "设置为白色"),
    ("Fill Color (Click to change)", "填充颜色（单击以更改）"),
    ("Stroke Color (Click to change)", "描边颜色（单击以更改）"),
    ("Swap Fill and Stroke", "交换填充和描边"),
    ("Text Color", "文本颜色"),
    ("Standard Vertical Roman Alignment", "标准垂直罗马对齐方式"),
    ("Tate-Chuu-Yoko", "直排内横排"),
    ("Use Smart Quotes", "使用弯引号"),
    ("Show Font Names in English", "显示英文字体名称"),
    ("No Break", "无间断"),
    ("Reset Character", "重置字符"),
    ("Line Join", "线段连接"),
    ("Miter Join", "尖角"),
    ("Round Join", "圆角"),
    ("Bevel Join", "斜角"),
    // Paragraph ($$$/AE/Paragraph_Palette/*).
    ("Left align text", "左对齐文本"),
    ("Center text", "居中对齐文本"),
    ("Right align text", "右对齐文本"),
    ("Justify last line left", "最后一行左对齐"),
    ("Justify last line center", "最后一行居中对齐"),
    ("Justify last line right", "最后一行右对齐"),
    ("Justify all lines", "两端对齐"),
    ("Indent left margin", "缩进左边距"),
    ("Indent right margin", "缩进右边距"),
    ("Indent first line", "首行缩进"),
    ("Add space before paragraph", "段前添加空格"),
    ("Add space after paragraph", "段后添加空格"),
    ("Left to right text direction", "从左到右的文本方向"),
    ("Right to left text direction", "从右到左的文本方向"),
    ("Roman Hanging Punctuation", "罗马悬挂式标点"),
    ("Top-to-Top Leading", "顶到顶行距"),
    ("Bottom-to-Bottom Leading", "底到底行距"),
    ("Adobe Single-line Composer", "Adobe 单行书写器"),
    ("Adobe Every-line Composer", "Adobe 逐行书写器"),
    ("Reset Paragraph", "重置段落"),
    ("Change Text Style", "更改文本样式"),
];
