# 14 · 剪辑工作区

| 项 | 内容 |
|---|---|
| 分支 | `ui/edit` |
| Orca 工作区 | `bae-14-ui-edit` |
| 阶段 | P3（现阶段只做框架） |
| 依赖 | 04、08、11 |
| 负责目录 | `crates/bae-edit/`（剪辑领域命令）、`crates/bae-ui-edit/`（界面） |

> **现阶段只做框架和接口**（见 `docs/VISION.md` 优先级总原则）：序列/轨道/片段的数据结构与命令接口、工作区注册入口、骨架界面。底层剪辑逻辑统一，界面按操作习惯提供 Premiere、达芬奇（DaVinci Resolve）、Final Cut Pro 风格的前端预设（布局 + 键位 + 工具行为），完整功能是后续"大一统"方向。

## 目标

在同一个工程里完成剪辑，动效合成可以直接作为片段放进序列，并且实时引用（类似 Adobe Dynamic Link，但不需要渲染中间文件）。

## 要做

- **领域命令（bae-edit，不依赖界面）**：插入/覆盖、波纹删除、剃刀、波纹编辑、滚动编辑、滑移、滑动、三点编辑、入出点、轨道增删与锁定、链接/取消链接音视频、嵌套。
- **界面（bae-ui-edit）**：
  - 源监视器、节目监视器。
  - 多轨序列时间轴：视频轨、音频轨（波形）、轨道开关。
  - 工具：选择、剃刀、波纹、滚动、滑移、滑动（键位参考 Premiere）。
  - 播放控制：J/K/L、入出点、标记。
  - 基础转场：交叉溶解、黑场过渡。
- **与其他岗位联动**：
  - 动效合成作为片段放进序列，实时引用最新版本。
  - 分镜工作区生成的动态分镜在这里继续剪。
  - 片段与 Shot 关联：在序列里双击片段，可以跳到对应的动效合成。

## 对外接口

- 剪辑工作区注册到 11；`bae-edit` 的命令注册到 CommandRegistry。

## 验收

- 用 10 段 4K 素材和 3 个动效合成剪一条 1 分钟的片子，播放流畅、音画同步。
- 修改其中一个动效合成，序列里马上看到更新。

## 不做

- 调色工作区、多机位、字幕轨（后续再定）。

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
