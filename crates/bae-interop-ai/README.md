# bae-interop-ai · Ai 互转

工作流 16。把 Illustrator 的内容变成 BAE 里可编辑的矢量：画板变成合成，Ai 图层变成形状图层，编组、复合路径、填充、描边、渐变都能逐点编辑。不依赖 GPU、窗口和平台 API，Agent、CI、Hub 都能直接用。

## 数据流

```text
.ai（PDF 兼容）/ .pdf ──┐
bridges/ai 导出的 JSON ─┼─► AiDocument ──plan_import──► CommandBatch ──execute──► Stage
剪贴板（PDF / SVG）─────┘   中间表示      规划（只读）     命令批次       一个撤销步骤
```

## 接口

| 类型 / 函数 | 作用 |
|---|---|
| `AiDocument` / `Artboard` / `AiLayer` / `AiItem`（`AiGroup`、`AiPath`、`AiText`） | 所有读取器的统一输出。坐标是画板空间（pt = px，原点在画板左上角，y 向下），顺序与 Ai 图层面板一致（下标 0 在最上面）。填充、描边、渐变直接用 bae-vector 的 `Paint` / `Brush` / `Gradient` |
| `AiLayer::to_shape_group()` | 图层内容对应的 bae-vector 形状树（画板空间），用于预览和测试 |
| `plan_import(&doc, &stage, &options) -> ImportPlan` | 规划导入：只读 Stage（避开重名），返回命令批次和导入报告 |
| `CommandBatch` / `BatchStep` | 命令批次，只用 `prim.create`、`attr.set` 两个内置命令。JSON 形状与 Agent 工具 `command.batch` 的参数一致，Agent 可以先看再提交 |
| `CommandBatch::execute(&registry, &mut stage, &author)` | 作为**一个撤销步骤**执行，任何一步失败都整体回滚，错误里写明是第几步 |
| `import_ai(path, &stage, &options)` | 读文件并规划（任务书里的 `import_ai(path) -> 命令批次`） |
| `read_document(path)` / `read_bytes(name, bytes)` / `sniff(bytes)` | 读取与格式识别。PDF 兼容的 .ai 和 PDF 由内置的 PDF 读取器读（`src/pdf/`，不引入 lopdf 等外部库）；桥接 JSON 由 Illustrator 一侧的 `bridge.rs` 读；不带 PDF 兼容的旧 .ai 会提示重新保存 |
| `ai.import`（`ImportAi`，`register_commands`） | 同一件事做成命令，菜单「文件 > 导入」和 Agent（MCP）直接调用，参数是 `file` 加 `ImportOptions` 的各项，返回 `ImportReport` |
| `ImportOptions` | 合成放在哪个文件夹（默认 `/comps`）、导入哪些画板、帧率、时长、是否同时建 `Artboard` Prim、文字怎么导、隐藏内容是否跳过 |
| `ImportReport` | 新建的合成和画板 Prim、形状图层 / 文字图层 / 路径数量、警告（哪些东西做了近似或没导入） |

## 导入结果（对齐 AE「导入为合成」+「从矢量图层创建形状」）

- 每个画板 → 一个合成，尺寸按画板向上取整（1 pt = 1 px），默认 30 fps、10 秒。多画板时合成名是「文件名 画板名」。默认同时在 `/artboards` 下建 `Artboard` Prim（名称、尺寸、在文档里的位置），合成的 `ai:artboard` 属性指向它。
- 每个 Ai 图层 → 一个形状图层，锚点 (0, 0)，位置在合成中心，内容画在图层原点周围（与 AE 新建形状图层、bae-vector 的约定一致）。隐藏图层导入后关闭视频开关，锁定图层导入后锁定。
- 编组 → 形状组「组 1」（有名字时用 Ai 里的名字），不透明度、混合模式放在组上。
- 路径 / 复合路径 → 一个形状组，里面是「路径 1…n」，后面跟外观里的绘画：「描边 1」「填充 1」「渐变填充 1」……，顺序与 Ai 外观面板一致（默认描边在填充之上）。奇偶填充规则、端点、连接、尖角限制、虚线都保留。
- 渐变 → 渐变填充 / 渐变描边：线性、径向（含高光长度和角度），颜色和不透明度色标按 AE 的 `colors` 布局写入。
- 点文字 → 文字图层（文字、字体、字号、颜色、对齐、基线位置、旋转、缩放），一段文字一种样式。转成轮廓等 bae-text（05 PR B）合并后再做。编组里的文字会单独提到该组所在形状图层的上方，并给出警告。
- 顶层剪切组 → 单独的形状图层，剪切路径变成图层蒙版（相加）。嵌套的剪切组暂时不裁剪，给出警告。

逐项支持程度见 [`docs/interop/ai-mapping.md`](../../docs/interop/ai-mapping.md)。

## 进度

- [x] 公共接口：中间表示、命令批次、规划器、`ai.import` 命令
- [x] .ai / PDF 读取
- [ ] Illustrator 导出脚本 `bridges/ai/` 与 JSON 读取（Illustrator 一侧，`interop/ai-bridge`）
- [ ] 剪贴板粘贴、导出 PDF / SVG / .jsx

## .ai / PDF 读取（`src/pdf/`）

Illustrator 保存 .ai 时默认「创建 PDF 兼容文件」，里面是一份完整的 PDF。读取器按 Illustrator 25–30 的写法解析：

| Illustrator 写法 | 读成 |
|---|---|
| 每个画板一页，CropBox = 画板 | `Artboard`（名称按「Artboard N」，PDF 里没有画板名；位置按顺序横排，间隔 20 pt） |
| 顶层图层 = 可选内容组（OCG），`/D /Order` 是图层顺序，`/OFF` 是隐藏 | `AiLayer`（名称、顺序、可见性）；嵌套的可选内容组读成以子图层命名的编组 |
| 路径 `m l c v y h re` + `S f f* B B*…` | `AiPath`，几何换到画板空间；填充时省略 `h` 的闭合路径会补上闭合 |
| 同一路径的填充和描边分两次写 | 合并回一个路径（描边在上） |
| 渐变 = 剪切路径 + `sh`；渐变描边 = 图案描边 | 线性 / 径向渐变，色标来自函数的分段点（类型 0/2/3/4），焦点偏移换成高光长度和角度 |
| 编组的不透明度 / 混合模式 = 透明组 Form XObject | `AiGroup`（只包一个路径时并到路径上） |
| 剪切组 = `W n` 剪切，q/Q 与图层的 BDC/EMC 互相交错 | 按每个对象自己的剪切栈重建剪切组；与画板一样大或包住对象的矩形剪切直接去掉 |
| 文字：一个 `BT` 里可能有多个文字框，路径文字每个字一个 `Tm` | 按落点拆分成 `AiText`（同一行续写、下一行换行、其余另起一项）；字符用 ToUnicode、简单字体编码，或 Adobe 字符集的 ASCII 段解码 |
| 颜色：灰度、RGB、CMYK、ICC、索引、专色（Separation / DeviceN）、Lab | 直通 RGB（不做色彩管理，CMYK 用简单公式） |
| 文件结构：xref 表 / xref 流、对象流、增量更新、Flate / ASCII85 / ASCIIHex / RunLength / LZW、PNG 预测器 | 全部支持；xref 损坏时扫描 `n g obj` 重建；畸形输入只报错不崩溃 |

PDF 里看不到的东西（普通编组、对象和画板的名字、锁定、符号、不透明蒙版、外观的结构）需要 Illustrator 桥接导出。

## 测试

```sh
cargo test -p bae-interop-ai
cargo run -p bae-interop-ai --example ai_dump -- some.ai   # 看读出来的结构
```

`tests/pdf.rs`：在测试里按 Illustrator 的写法生成 PDF（两种交叉引用方式各一份：普通 xref 表，以及 Flate + 对象流 + xref 流），检查图层顺序和可见性、复合路径和虚线描边、线性 / 径向渐变、透明组、剪切组、CID 文字、多画板，再走一遍导入、求值和显示列表；截断和随机改字节的文件不会崩溃，startxref 错误能扫描修复。真机数据（Illustrator 30 导出的 `tests/data/illustrator/*.ai`，由 Illustrator 一侧提供）存在时会逐项检查，不存在时跳过；`BAE_AI_TEST_DATA` 可以指向别的目录。

`tests/import.rs`：在代码里搭一个 Ai 文档（复合路径 + 奇偶填充 + 虚线描边、线性和径向渐变、半透明正片叠底、编组里的文字、剪切组、隐藏锁定图层），生成命令批次、执行、用 03 求值、用 05 生成显示列表，再与文档自己的形状树逐项比对几何、绘画、渐变和不透明度；另外检查撤销、重名、失败回滚、批次 JSON 和命令注册。
