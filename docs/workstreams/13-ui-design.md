# 13 · 分镜/设计工作区（Ai 式）

| 项 | 内容 |
|---|---|
| 分支 | `ui/design` |
| Orca 工作区 | `bae-13-ui-design` |
| 阶段 | P3（现阶段只做框架） |
| 依赖 | 05（矢量与编辑几何）、11 |
| 负责目录 | `crates/bae-storyboard/`（分镜/设计领域命令）、`crates/bae-ui-design/`（界面） |

> **现阶段只做框架和接口**（见 `docs/VISION.md` 优先级总原则）：数据结构、命令接口、工作区注册入口和最小可用的骨架界面，完整功能是后续"大一统"方向。

## 目标

让分镜师和设计师在一个 Illustrator 用户熟悉的界面里画画板、画分镜，并且能把分镜直接交给动效和剪辑。

## 要做

- **领域命令（bae-storyboard，不依赖界面）**：画板的增删改与排列；分镜表和分镜格的增删、排序、时长；由分镜表生成动态分镜（Sequence）；把分镜格"升级"成动效合成并建立 Shot 关联；路径编辑命令（调用 05 的几何 API）。
- **工具栏（Ai 键位）**：选择 V、直接选择 A、钢笔 P、曲率、矩形/椭圆/多边形/星形、直线、铅笔、画笔、文字 T、吸管 I、渐变 G、形状生成器 Shift+M、旋转 R、镜像 O、比例缩放 S、抓手 H、缩放 Z、画板工具 Shift+O。
- **面板**：图层、属性、外观、描边、颜色、色板、对齐、路径查找器、变换、画板。
- **分镜**：
  - 分镜表视图（网格 / 条带两种）。
  - 分镜格属性：镜头号、时长、景别、台词、动作、镜头运动、备注。
  - 一键生成动态分镜，送到剪辑工作区继续剪。
  - 分镜格与动效合成通过 Shot 关联：动效那边更新了，分镜表里可以看到最新画面。
- **从 Ai 粘贴**：与 16 对接，在 Ai 里复制、在这里粘贴得到可编辑矢量。
- **手绘草图**：先用矢量的铅笔/画笔工具；光栅笔刷放到后续阶段。

## 对外接口

- 分镜/设计工作区注册到 11；`bae-storyboard` 的命令注册到 CommandRegistry。

## 验收

- 一位 Ai 用户不看文档，能用钢笔、形状、路径查找器画出一个 Logo，并排好多个画板。
- 能画 12 格分镜，填写镜头信息，一键生成动态分镜，在剪辑工作区里打开。
- 把一个分镜格升级成合成后，动效工作区里能打开，画面元素是可编辑的矢量。

## 不做

- 矢量渲染与几何算法（05）、Ai 文件解析（16）、光栅绘画引擎（后续单独立项）。

## 进度

**2026-11-20（PR 1，Draft）**：`crates/bae-storyboard` 第一版接口完成，`cargo test -p bae-storyboard` 10 个测试全部通过。

- 命令：`artboard.create/update/delete/rearrange`、`board.create/update`、`panel.create/update/move/delete`、`board.make_animatic`、`panel.promote_to_comp`；`StoryboardModule` 发布到 `bae_app::Registry<dyn DynCommand>`。
- 数据布局：画板 `/artboards/<名>`（作品在画板下的 `layers`，画板坐标）；分镜表 `/boards/<名>/panels/<格>`；镜头 `/shots/<镜头号>`；动态分镜 `/sequences/<名>/V1/<片段>`，片段 `source` 指向分镜格，用 `query::panel_picture` 解析成最新画面（已升级的取动效合成，否则取画板/素材）。
- 分镜格字段定稿：`shot_code`、`duration`、`shot_size`（none/els/ls/fs/ms/mcu/cu/ecu）、`dialogue`、`action`、`camera_move`、`sound`、`transition`、`notes`、`image`、`shot`。
- 改了别人目录：`crates/bae-model/src/schema.rs` 的 Board/Panel 两条（原标注 framework only；`camera_notes` 改名 `camera_move`，无人使用）；根 `Cargo.toml` 加一行 workspace 依赖。
- 还没做：`apps/bae` 接入 `StoryboardModule`（一行）；全工作区 clippy、bae-model 测试未跑（额度暂停）；路径编辑命令（调 05 几何 API）；`bae-ui-design` 工作区骨架。
- 下一步：跑 clippy 和 bae-model 测试后把 PR 1 改成 Ready；PR 2 做 `bae-ui-design`（分镜工作区预设、Ai 键位、工具栏、分镜表/分镜格属性/画板面板，其余 Ai 面板占位，中英文）；PR 3 做路径编辑命令。

- [x] bae-storyboard 命令（第一版：画板、分镜表、分镜格、动态分镜、升级为合成）
- [ ] 工具栏与基础绘图工具
- [ ] Ai 式面板
- [ ] 分镜表与分镜格
- [ ] 动态分镜与 Shot 关联
- [ ] 从 Ai 粘贴

## 请求（其他工作流在这里留言）

- **18 agent/guide（2026-11-20）**：分镜规范（`docs/agent-guide/04-storyboard.md`）建议分镜格在已列字段（镜头号、时长、景别、台词、动作、镜头运动、备注）之外，再加"转场"（到下一镜头）和"声音"（音效、音乐提示）两个字段。字段名定稿后请告诉 18，指南和评测集会同步。
