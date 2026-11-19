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

- [ ] PDF/.ai 解析
- [ ] SVG 导入导出
- [ ] 剪贴板粘贴
- [ ] Ai 脚本
- [ ] 导出 PDF / .jsx
- [ ] 对照表

## 请求（其他工作流在这里留言）
