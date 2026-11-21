# 14 · 剪辑工作区

| 项 | 内容 |
|---|---|
| 分支 | `ui/edit` |
| Orca 工作区 | `bae-14-ui-edit` |
| 阶段 | P3（现阶段只做框架） |
| 依赖 | 04、08、11 |
| 负责目录 | `crates/bae-edit/`（剪辑领域命令）、`crates/bae-ui-edit/`（界面）、`crates/bae-interop-edit/`（交换格式，M2 以后开工） |

> **现阶段只做框架和接口**（见 `docs/VISION.md` 优先级总原则）：序列/轨道/片段的数据结构与命令接口、工作区注册入口、骨架界面。底层剪辑逻辑统一，界面按操作习惯提供 Premiere、达芬奇（DaVinci Resolve）、Final Cut Pro 风格的前端预设（布局 + 键位 + 工具行为），完整功能是后续"大一统"方向。
>
> **路线图依据：** `docs/spec/pr/`——Premiere Pro 基础功能清单（逐项写了 Pr 的行为要点、优先级、负责模块、对数据模型的影响），以及 `docs/spec/pr/edit-model.md`（序列、轨道、片段、过渡、标记的数据模型）。下面的分阶段计划和"框架必须预留的接口"都从那里来；清单 README 第 7 节有按优先级生成的 M1 清单，落地时逐项打勾。

## 目标

在同一个工程里完成剪辑，动效合成可以直接作为片段放进序列，并且实时引用（类似 Adobe Dynamic Link，但不需要渲染中间文件）。

## 要做（0.0.1 框架，阶段 0）

- **领域命令（bae-edit，不依赖界面）**：插入/覆盖（含同步锁定）、提升/提取、波纹删除、剃刀/添加编辑、常规修剪、波纹编辑、滚动编辑、外滑、内滑、三点编辑、入出点、移动与微移、轨道增删与锁定、链接/取消链接音视频、嵌套、序列标记、基础过渡（交叉溶解、黑场过渡）。命令约定见下文"框架必须预留"第 2 条。
- **查询与求值（bae-edit）**：某时刻序列由哪些片段组成、源时间换算、编辑点、空隙、链接组、每条序列用到的源区间（下文第 3 条）。
- **界面（bae-ui-edit）**：
  - 源监视器、节目监视器（能加载、播放、打入出点）。
  - 多轨序列时间轴：视频轨、音频轨（波形）、轨道头开关（输出、锁定、静音、独奏、同步锁定）。
  - 工具：选择、剃刀、波纹、滚动、外滑、内滑（键位参考 Premiere）。
  - 播放控制：空格、J/K/L（先做 1 倍正反放）、←/→、↑/↓、入出点、标记。
  - 剪辑工作区预设和 Premiere 键位表 `pr`（M1 用到的键，见 `docs/spec/pr/shortcuts.md`）。
- **与其他岗位联动**：
  - 动效合成作为片段放进序列，实时引用最新版本。
  - 分镜工作区生成的动态分镜在这里继续剪。
  - 片段与 Shot 关联：在序列里双击片段，可以跳到对应的动效合成。

## 分阶段计划

| 阶段 | 对应里程碑 | 内容（清单条目见 `docs/spec/pr/`） | 完成标志 |
|---|---|---|---|
| **0 框架** | 0.0.1 / M1 | PR A（#66）：`bae-edit` 命令和查询。PR A2：补齐 M1 预留——schema 属性（经 02）、过渡改成子 Prim、同步锁定、源修补映射、`plan_*` 预演、序列求值接口、测试。PR B：`bae-ui-edit` 骨架——监视器、时间轴、六个工具、`pr` 键位表、剪辑工作区预设、中英文。 | Agent 通过 MCP 只用命令剪出一条 3 条视频轨 + 2 条音频轨、带链接和过渡的序列；桌面应用里能打开并播放这条序列的画面；清单 README 的 M1 清单全部打勾。 |
| **1 交接与协同** | M2 | 动效合成作为片段 + 双击跳转（PR-XCH-008）；替换为动效合成（PR-XCH-007）；在动效工作区编辑图形（PR-GFX-009）；Shot 关联与分镜→序列（PR-XCH-010，与 13 对齐）；给 21 提供序列的源区间（PR-EXP-015）；协同的重叠修复和序列提示锁（edit-model.md 第 12 节）；Agent 工具：序列摘要、按序列取帧、`sequence.export`（PR-XCH-012、PR-EXP-010）；OTIO 和 FCP XML 导出（PR-XCH-004、PR-XCH-001，新建 `crates/bae-interop-edit`，归 14）。 | 本任务书"验收"两条。 |
| **2 剪辑工作区基础可用** | M4 | 清单里全部 M4 条目，按组推进：监视器（三点四点、J/K/L 变速、匹配帧、回放分辨率、修剪监视器）→ 时间轴（修剪模式、Q/W、键盘修剪、复制粘贴、粘贴属性、替换剪辑、速度/持续时间、帧定格、比率拉伸）→ 效果（效果控件、运动/不透明度、蒙版、调整图层、过渡与 14 个常用过渡）→ 关键帧 → 音频（增益、剪辑混合器、音量关键帧、交叉淡化、音频表）→ 调色入门（Lumetri 基本校正、Look、RGB 曲线、波形和矢量示波器）→ 图形（文字、形状、属性面板）→ 标记 → 代理 → 导出（导出模式、预设、快速导出、队列）→ 默认快捷键全集 → 项目面板的剪辑视图。 | 一位 Pr 用户不看文档，用 Pr 的快捷键和习惯剪完一条带音乐、字幕条、调色的 3 分钟片子。 |
| **3 增强** | M4 之后 | 清单里标"以后"的条目：多机位、时间重映射速度坡度、音轨混合器与自动化、子混合与发送、基本声音、Lumetri 高级部分、字幕与转录、基于文本的编辑、动态图形模板、响应式设计、EDL/AAF/OMF、读取 .prproj、场景编辑检测、自动重构、变形稳定器、变形剪切。 | 按需排期。 |

**前端预设：** Premiere 风格在阶段 0–2 完成。达芬奇、Final Cut Pro 风格在阶段 2 之后，只新增布局、键位表和工具行为参数，复用同一批命令和数据（edit-model.md 第 9 节）。

## 框架必须预留的接口和数据结构

米兰的 14 恢复后，PR A 合并前后按这一节补齐。这些东西以后再改要迁移已有工程或改所有调用方，所以必须在 0.0.1 定下来。详细定义见 `docs/spec/pr/edit-model.md`，下面是清单。

1. **数据结构**（edit-model.md 第 2–8、13 节；schema 改动已写进 02 的请求）：
   - Prim 布局：`/sequences/<seq>` 下 Track 按子节点顺序编号（#66 已是这样）；片段下的 `effects/`、`masks/`、`transition_in`、`transition_out`；序列下的 `markers/`、`graphics/`；来源下的 `markers/`、`effects/`。
   - Sequence：`mark_in`、`mark_out`（文档数据，参与同步）、`pixel_aspect`、`audio_sample_rate`、`audio:master_format`、`start_timecode`。
   - Track：`track_kind` 枚举含 `caption`、`submix`、`master`；`enabled`、`locked`、`solo`、`sync_lock`、`audio:channel_format`。
   - Clip：`name`、`link`、`group`、`enabled`、`reverse`、`source_channels`；固有效果和时间重映射的属性名（`transform:*`、`audio:level`、`audio:pan`、`time:remap*`）先定名字。
   - **Transition 子 Prim** 和归属规则（两个片段之间的过渡存在后一个片段的 `transition_in`）。#66 现在用的 `transition_in:kind` / `:duration` 属性在 PR A 合并前改成子 Prim（已决定第 3 条），不保留属性写法。
   - **片段时间与关键帧时间基准**：片段属性的关键帧时间 = `source_in + (t − start) × speed / 100`（edit-model.md 第 5.6 节）。
   - Asset：`kind` 增加 `adjustment`、`generator`、`subclip`；代理文件引用 `proxy:file:*`。源监视器的入出点按已决定第 4 条的默认方向是每个用户的本地状态，不进 schema；#66 目前写在来源上的 `mark_in` / `mark_out` 先保留，M2 时和 09 一起确认后再迁移。
2. **命令约定**（edit-model.md 第 11 节）：
   - 不读界面状态：目标轨道 `targets`、源修补 `patch`（源轨道 → 序列轨道的映射，替代 #66 的 `track` + `audio_track`）、播放头时间都是参数。
   - 插入和波纹类命令有 `sync_lock`（默认 true，按各轨道的 `sync_lock`）和 `ripple_markers` 参数；涉及链接的命令有 `linked_selection` 参数。
   - 修剪命令接收"编辑点列表 + 各自的修剪类型"，一次执行（为多轨不对称修剪留好接口）。
   - 时间吸附到帧，输出返回实际使用的时间；一个手势一个撤销步骤。
   - 每个命令有纯函数 `plan_*`（返回将要写的 Op 和受影响的时间范围），界面拖动预览和 Agent 预演都用它。
3. **查询与求值接口**（edit-model.md 第 10 节，纯数据、不依赖 GPU）：`frame_at(seq, t)`（从下到上的视频项、可听的音频项、过渡进度、重复帧标记）、`segments(seq, range)`、`used_source_ranges(seq)`、源时间换算、编辑点和空隙、链接组。04 渲染、08 混音、17 Agent、21 打包都从这里取数据，不各自解析序列。#66 的 `query` 已经有雏形。
4. **界面框架（bae-ui-edit）**：
   - 面板类型：源监视器、节目监视器、序列时间轴（可多开，面板 ID 带序列，如 `edit_timeline:<序列>`）、音频表。项目面板、效果控件、效果和预设面板由 12 实现并注册成全局面板，剪辑工作区直接复用，剪辑特有的行为（固有效果和过渡设置、素材箱的剪辑列和悬停擦洗、按种类过滤过渡）通过 12 提供的扩展点加，不复制面板（已决定第 2 条，扩展点已写进 12 的请求）。
   - 工作区预设 `edit`，键位表 `pr`。
   - 编辑视图状态（轨道定位、源修补、播放头、缩放、轨道高度、当前工具、链接选择项、对齐开关）存在面板的 `save_state` 里，不进工程。
   - 工具接口：一个 `EditTool` 状态机（命中测试 → 拖动时调用 `plan_*` 预览 → 松手执行命令），选择、波纹、滚动、外滑、内滑、剃刀、比率拉伸共用。
   - 播放控制器（暂称 `Transport`）：播放/停止、穿梭倍速、逐帧、循环、预卷，作为应用资源由监视器和时间轴共用；时钟来自 08。
5. **前端预设接口**：`EditFrontend { id, 工作区预设, 键位表, 工具行为参数（拖放默认插入还是覆盖、主轨是否总是波纹……） }` 的注册表，Premiere 是第一个条目，以后加达芬奇、Final Cut Pro 不改命令。
6. **交换格式入口**：导入器 / 导出器按 ID 注册（`otio`、`fcpxml`、`edl`、`aaf`），实现放在 `crates/bae-interop-edit`，阶段 1（M2 以后）先做 OTIO 和 FCP XML 导出；0.0.1 不建这个 crate。

## 对外接口

- 剪辑工作区注册到 11；`bae-edit` 的命令发布到 `Registry<dyn DynCommand>`（#66 的 `EditModule`），自动成为 MCP 工具。
- `bae_edit::query` 的序列求值接口给 04（渲染）、08（混音）、17（Agent 查询工具）、21（打包裁片）。
- `bae-ui-edit` 的 `Transport`、`EditTool`、`EditFrontend` 只在界面层使用。

## 验收

- （阶段 0）见上表"完成标志"。
- （阶段 1）用 10 段 4K 素材和 3 个动效合成剪一条 1 分钟的片子，播放流畅、音画同步。
- （阶段 1）修改其中一个动效合成，序列里马上看到更新。

## 不做

- 现阶段（0.0.1）不做阶段 1 以后的功能，但对应的数据结构名字按上面第 1 条预留。
- 调色工作区、多机位、字幕轨在阶段 3（数据结构名字现在预留）。
- Pr 的团队项目、Frame.io、AI 生成功能、磁带采集、旧版标题编辑器（见 `docs/spec/pr/README.md` 第 1 节）。

## 已决定（协调人，2026-11-20）

1. **交换格式 crate 归 14。** 新建 `crates/bae-interop-edit`（FCP XML、EDL、AAF、OTIO、读取 .prproj），剪辑功能进入 M2 以后再开工，0.0.1 不做。到时候工作量大的话，再拆成独立的工作流（编号 22，`interop/edit`）。
2. **共用面板归 12。** 项目面板、效果控件等共用面板由 12 实现（它在 0.0.1 的关键路径上），通过 11 的 Panel API 注册成全局面板，14 的剪辑工作区直接复用，不另做一份。剪辑特有的行为通过扩展点（trait 或 hook）加，不复制面板。如果以后面板的依赖太重，由 12 把它们提取到 11 的 `bae-ui`，11 审查接口。
3. **#66 的过渡改成子 Prim。** 米兰 14 恢复后，在 PR A 合并前改好（#66 现在还是 WIP 草稿）。
4. **入出点的协同语义 M2 再定**，默认方向：序列的入出点是文档数据，参与同步；源监视器的入出点算每个用户的本地状态，不同步。M2 时由 14 和 09 一起确认。

## 进度

- [ ] bae-edit 命令（进行中，2026-11-20，PR A 草稿，**尚未编译**）
- [ ] 监视器
- [ ] 序列时间轴与工具
- [ ] 音频轨与波形
- [ ] 与动效、分镜联动

### 2026-11-20 状态（额度暂停时的 wip）

已完成（代码已写，**还没跑过 `cargo check` / 测试**，因额度不足暂停）：

- `crates/bae-edit`：`query`（序列/轨道/片段信息、某时刻的视频叠放顺序和可听音频、链接组、片段来源长度、双击片段跳到的动效合成）和 21 个命令：`sequence.create`、`sequence.add_marker`、`marks.set`、`track.add` / `track.remove` / `track.set_state`、`clip.insert` / `clip.overwrite`（三点编辑）、`clip.lift`、`clip.ripple_delete`、`clip.razor`、`clip.move`、`clip.trim` / `clip.ripple_trim` / `clip.roll` / `clip.slip` / `clip.slide`、`clip.link` / `clip.unlink`、`clip.nest`、`clip.set_transition`；`EditModule` 发布到 `bae_app::Registry<dyn DynCommand>`。

还差（按顺序）：

1. 根 `Cargo.toml` 加 `bae-edit` 工作区依赖；`cargo check -p bae-edit` 修编译错误；写 `tests/commands.rs`（每个命令至少一个用例）；fmt / clippy / test 通过后 `gh pr ready`。
2. `apps/bae` 加一行 `.add_module(bae_edit::EditModule)`，让菜单和 `bae mcp` 拿到剪辑命令。
3. PR B：`crates/bae-ui-edit` 骨架——源监视器、节目监视器、序列时间轴面板（只读显示轨道和片段，工具切换、J/K/L、I/O 调用命令），`edit` 工作区预设和 Premiere 键位表，中英文 i18n。

已知问题：尚未编译，可能有小的编译错误。

## 请求（其他工作流在这里留言）
