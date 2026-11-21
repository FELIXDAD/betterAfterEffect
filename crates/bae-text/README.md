# bae-text · 文字

工作流 05。AE 文字图层的排版与 GPU 绘制：parley 负责字形整形、字体回退和换行（UAX #14，中文在汉字之间断行、标点不出现在行首），本 crate 负责 AE 的定位规则。行为以 `docs/spec/ae/text.md`（AE-TXT-nnn）为准。

默认不带 GPU 依赖（字体来自系统，经 fontique 读取，不内置字体）；打开 `render` feature 才会引入 Vello、`bae-render`、`bae-gpu`。

## 数据流

```text
TextDocument（bae-model，某一时刻的源文本）
    ──► layout_text（parley 整形、回退、换行 + AE 定位）──► TextLayout
    ──► render::encode（Vello 字形轮廓）──► render::TextSource（bae-render 的 LayerSource）
```

## 接口

| 类型 / 函数 | 作用 |
|---|---|
| `FontSystem` | 系统字体库 + 排版缓存。`families()` 列出字体系列（字符面板），`faces(family)` 列出字体样式和 PostScript 名，`resolve(name)` 把工程里存的 PostScript 名（`ArialMT`、`PingFangSC-Regular`）或系列名匹配到已安装字体（AE-TXT-082），找不到返回 `FontMatch::Missing`（用默认字体代替，原名保留，AE-TXT-083）。`register_font_data` 加入工程自带或测试用字体。`set_locale` 改回退语言，默认 `zh-Hans` |
| `layout_text(&mut FontSystem, &TextDocument) -> TextLayout` | 排版入口 |
| `TextLayout` | **结果类型**：`runs`（同一字体的字形，图层空间坐标）、`lines`（基线、起点、宽度、上升/下降、字符范围）、`chars`（每个"字符"= 整形簇：对应源文本的字符下标、所在行、所在词、基线原点、进距，给文字动画器用）、`paint`（填充、描边、描边在填充之上）、`box_rect`、`font`（字体匹配结果）、`overset`（段落文本溢出）；`bounds(with_stroke)` 对应 `sourceRectAtTime`（AE-TXT-007），`content_hash()` 用作缓存键 |
| `render::encode`（`render`） | 把 `TextLayout` 写进 `vello::Scene`，按字符交替画填充和描边（AE-TXT-017），仿粗体用轮廓扩张，仿斜体用斜切 |
| `render::TextSource` / `TextProvider`（`render`） | bae-render 的 `LayerSource` / `SourceProvider`，provider ID 是 `TEXT_PROVIDER = "bae.text"`；数据可以是 `Arc<EvaluatedLayer>`（03 的求值结果）、`Arc<TextDocument>` 或 `Arc<TextLayout>`。绘制用 bae-vector 的共享 `VelloRenderer` |

### 已实现的 AE 行为

- 点文本（AE-TXT-003）：图层原点在第一行基线起点；左对齐从 x = 0 开始，居中以 x = 0 为中心，右对齐在 x = 0 结束；只在回车（`\r`、`\n`、`\r\n`）和软回车（U+0003）处换行。
- 段落文本（AE-TXT-004/042）：按文本框宽度换行；第一行基线 = 框顶 + 第一行上升；放不下的行溢出（不画、不计入字符），`overset = true`；左/中/右对齐在框内，两端对齐交给 parley（最后一行按所选对齐）。
- 行距（AE-TXT-020/041）：自动行距 = 字号 × 120%，或固定值；基线逐行下移一个行距。
- 字符间距（AE-TXT-018）：每个字符后加 `tracking/1000 × 字号`，测量行宽时不计最后一个；基线偏移（正值上移）；全部大写（只改显示，字符下标仍对应原文）；小型大写字母（字体的 `smcp`）；仿粗体（每边 0.02 em，进距加 0.04 em）、仿斜体（12°）——两者的数值是规范里的 BAE 暂定值，等和 AE 实测对比后再定。
- 中文：拉丁字体里的汉字按 `zh-Hans` 回退到系统中文字体（macOS 苹方、Windows 微软雅黑），不会出现豆腐块；段落文本中文换行遵守避头尾。

### 还没有

逐字符样式（`TextDocument` 目前整段一种样式，见 bae-model 的说明）、段落缩进和段前段后距、文本框内边距和垂直对齐（AE 24.6+）、比例间距、上标下标、竖排、路径文字、文字动画器（下一阶段）、从文字创建形状。

## 已知问题

- 没有显卡的 Windows（WARP）上文字和形状一样改用 `vello_cpu` 栅格化（字形轮廓来自 skrifa），见 `crates/bae-vector/README.md` 的"兼容性"一节。

- debug 构建里，排含中文的段落时 ICU4X 会往 stderr 打一行 `No segmentation model for complex script: Chinese/Japanese`（parley 没开 `complex-scripts`，中文词边界退化为按字符；换行不受影响）。release 构建里这行日志被编译掉。开 `complex-scripts` 要多带约 2 MB 的词典数据，暂不开。
- 和形状一样，文字在图层空间栅格化，放大图层会糊；连续栅格化需要 04 在 `SourceRequest` 里带上变换（已提请求）。

## 测试

```sh
cargo test -p bae-text   # 单元测试 + 系统字体排版测试 + GPU 测试
```

排版测试用机器上的系统字体（macOS / Windows 都有拉丁和中文字体；缺字体时跳过并打印原因）：PostScript 名匹配、点文本对齐和行距、字符间距、拉丁字体里的中文回退、中文段落避头尾、段落溢出、全部大写。GPU 测试画中文、描边和仿粗体并检查像素；还有一个测试用模型命令搭文字图层，经 03 求值后交给 provider。
