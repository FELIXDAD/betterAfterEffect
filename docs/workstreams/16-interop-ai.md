# 16 · Ai 互转

| 项 | 内容 |
|---|---|
| 分支 | `interop/ai` |
| Orca 工作区 | `bae-16-interop-ai` |
| 阶段 | P1 |
| 依赖 | 02、05（矢量模型与几何） |
| 负责目录 | `crates/bae-interop-ai/`、`bridges/ai/`、`docs/interop/ai-*` |

## 目标

Illustrator 的文件和剪贴板内容能变成 BAE 里可编辑的矢量，也能导回 Ai。这是"分镜/设计和动效在同一平台"的入口。

## 要做

- **.ai / PDF 解析**：用纯 Rust PDF 库（如 lopdf）读取内容流，得到路径、填充、描边、渐变、剪切路径、文字（先转成轮廓，可选保留为文字）、画板（PDF 页面以及 Ai 私有数据里的画板信息）；保留图层结构（PDF 可选内容组）。
- **SVG**：导入（usvg 解析）和导出。
- **Ai 脚本（`bridges/ai/`，ExtendScript）**：从 Ai 导出更完整的信息（图层名、编组、画板名、外观、符号）；以及在 Ai 里重建 BAE 画板的导入脚本。
- **剪贴板**：读取 Ai 复制时放进剪贴板的 PDF 数据（macOS NSPasteboard、Windows 剪贴板），粘贴成可编辑矢量。13 负责界面上的粘贴入口。
- **导出**：PDF（Ai 能以可编辑矢量打开）、SVG、导回 Ai 的 .jsx 重建脚本。
- **与 AE 的对应**：实现等同于 AE"从矢量图层创建形状"的转换，把 Ai 内容变成动效里的形状图层。
- **对照表**：`docs/interop/ai-mapping.md`，列出 Ai 功能的支持程度。

## 对外接口

- `import_ai(path) -> 命令批次`、`paste_from_clipboard() -> 命令批次`、`export_pdf/svg/jsx`。

## 验收

- 一组自制的 Ai 测试文件（路径、复合路径、渐变、剪切蒙版、多画板、文字）导入后与 Ai 中显示一致。
- 在 Ai 里复制一组图形，粘贴到 BAE 后可以逐点编辑。
- 导出的 PDF 在 Ai 里打开后仍是可编辑矢量。

## 不做

- Ai 的实时效果（如 3D、变形效果）：只保留其展开后的外观。

## 进度

- [x] 公共接口（PR A）：`crates/bae-interop-ai` 的中间表示 `AiDocument`、命令批次 `CommandBatch`（一个撤销步骤，JSON 与 Agent 的 `command.batch` 一致）、规划器 `plan_import`（画板 → 合成，Ai 图层 → 形状图层，编组、复合路径、填充描边、渐变、点文字占位、顶层剪切组 → 蒙版）、`ai.import` 命令；对照表初稿 `docs/interop/ai-mapping.md`
- [x] PDF/.ai 解析（PR B #69）：`src/pdf/` 自写的 PDF 读取器（词法、xref 表 / xref 流 / 对象流 / 增量更新、Flate 等过滤器、损坏时扫描重建；颜色空间和函数；内容流解释器；字体解码），按 Illustrator 的写法组装成 `AiDocument`：画板、OCG 图层（顺序、可见性）、路径和复合路径、填充描边（虚线、端点、连接）、线性 / 径向渐变（剪切 + `sh`、图案）、透明组编组、剪切组、文字占位（多文字框拆分、路径文字、CID 文字）、专色 / CMYK。新依赖 flate2 与 bae-model 的 zip 是同一份 zlib-rs 构建。真机验证：Illustrator 30 导出的 5 个测试文件（Illustrator 一侧 `interop/ai-bridge` 提供，本分支不提交）逐项通过；CI 上用测试里生成的仿 Ai PDF。
- **状态（2026-11-21，按协调人通知暂停）**：#69 代码已完成并推送，本地 fmt / clippy / 测试全过，真机数据本地通过；CI 刚开始跑、还没出结果，所以仍是 Draft。继续时：看 CI 结果（Windows 上若有问题先修），全绿后 `gh pr ready 69`；`interop/ai-bridge` 合并后 rebase，让真机数据测试正式启用。之后是剪贴板粘贴、导出 PDF / SVG / .jsx、SVG 导入。
- [ ] SVG 导入导出
- [ ] 剪贴板粘贴
- [ ] Ai 脚本：导出脚本已完成（见下方「本机：Illustrator 一侧」）；在 Ai 里重建 BAE 画板的导入脚本还没做
- [ ] 导出 PDF / .jsx
- [x] 对照表（`docs/interop/ai-mapping.md`，随功能更新）

### 本机：Illustrator 一侧

分支 `interop/ai-bridge`，在装了 Illustrator 30.4.0 的本机上做；`crates/bae-interop-ai` 的其余部分归 16 的主分支。0.0.1 范围已完成（2026-11-20）。

- [x] `tools/ai/aictl`：无人值守驱动 Illustrator（macOS）。脚本走隐藏的 CEP 扩展，不用 AppleEvent（那要人点自动化授权）；能识别并恢复脚本弹出的对话框、卡死、文件授权阻塞和崩溃；不是脚本弹出的框原样留给人处理，不点、不关、不重启。用法和踩过的坑见 `tools/ai/README.md`。
- [x] 导出脚本 `bridges/ai/bae-ai-export.jsx` 和桥接格式 `bae-ai-bridge` 0.1（`bridges/ai/FORMAT.md`）：画板名、图层树（子图层、编组、剪切组、符号）、锁定 / 隐藏 / 模板图层、路径与复合路径、填充描边、渐变（含中点和高光）、文字（含样式段和可选的轮廓）。Rust 读取在 `src/bridge.rs`（`read_bridge`；`read_bytes` 识别出桥接 JSON 时调用）。
- [x] 真机测试数据：`bridges/ai/tests/make-fixtures.jsx` 在 Illustrator 里造 5 个文档，`regen.sh` 存成 .ai（PDF 兼容）并导出 JSON，放在 `crates/bae-interop-ai/tests/data/illustrator/`。#69 的 PDF 读取器也用这些 .ai。
- [x] 测试：固定数据（`tests/illustrator.rs`，CI 两个平台都跑）、模拟 DOM（`tests/export_script.rs`，需要 node 或 jsc，没有就跳过）、真机（`BAE_TEST_ILLUSTRATOR=1`，2026-11-20 在 Illustrator 30.4.0 上通过：重新生成的 5 个文档全部通过检查，导出的 JSON 与仓库里的一致）。
- 限制：Illustrator 只能读写 `/private/tmp` 这类不受保护的目录，碰 `~/Documents`、`~/Desktop` 等会弹 macOS 隐私授权框；aictl 只支持 macOS。
- 以后做：在 Ai 里重建 BAE 画板的导入脚本（BAE → Ai）；外观面板的多重填充 / 描边、实时效果脚本 DOM 读不到，靠 PDF 读取补（见 `bridges/ai/FORMAT.md` 第 7 节）。

## 请求（其他工作流在这里留言）
