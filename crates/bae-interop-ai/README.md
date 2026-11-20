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
| `read_document(path)` / `read_bytes(name, bytes)` / `sniff(bytes)` | 读取与格式识别（PDF 兼容的 .ai、PDF、桥接 JSON；不带 PDF 兼容的旧 .ai 会提示重新保存） |
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
- [ ] .ai / PDF 读取（下一个 PR）
- [ ] Illustrator 导出脚本 `bridges/ai/` 与 JSON 读取
- [ ] 剪贴板粘贴、导出 PDF / SVG / .jsx

## 测试

```sh
cargo test -p bae-interop-ai
```

`tests/import.rs`：在代码里搭一个 Ai 文档（复合路径 + 奇偶填充 + 虚线描边、线性和径向渐变、半透明正片叠底、编组里的文字、剪切组、隐藏锁定图层），生成命令批次、执行、用 03 求值、用 05 生成显示列表，再与文档自己的形状树逐项比对几何、绘画、渐变和不透明度；另外检查撤销、重名、失败回滚、批次 JSON 和命令注册。
