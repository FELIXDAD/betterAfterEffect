# 工作流拆分

整个项目拆成 21 个工作流。每个工作流对应一个 Git 分支和一个 Orca 工作区，任务书在 `docs/workstreams/`。

当前版本优先做好"更快的 AE"（性能第一）。13 分镜、14 剪辑现在只做框架和接口；20 AE 插件兼容现在只做设计和接口预留。

## 总表

| # | 分支 | Orca 工作区 | 名称 | 阶段 | 依赖 | 负责目录 |
|---|---|---|---|---|---|---|
| 01 | `core/foundation` | `bae-01-foundation` | 基础工程与构建 | P0 | — | 根 `Cargo.toml`、`rust-toolchain.toml`、`apps/bae`、`crates/bae-platform`、`crates/bae-gpu`、`.github/`、`xtask/`、`tools/` |
| 02 | `core/doc-model` | `bae-02-doc-model` | 文档模型与命令总线 | P0 | — | `crates/bae-model`、`docs/spec/` |
| 03 | `core/animation` | `bae-03-animation` | 动画求值与表达式 | P1 | 02 | `crates/bae-anim` |
| 04 | `render/compositor` | `bae-04-compositor` | GPU 合成与渲染图 | P1 | 01、02、03 | `crates/bae-render`、`benches/`、`docs/perf.md` |
| 05 | `render/vector-text` | `bae-05-vector-text` | 矢量与文字 | P1 | 02、04 | `crates/bae-vector`、`crates/bae-text` |
| 06 | `render/color` | `bae-06-color` | 色彩管理 | P1 | 04 | `crates/bae-color` |
| 07 | `render/effects` | `bae-07-effects` | 效果库 | P2 | 04、06 | `crates/bae-effects` |
| 08 | `io/media` | `bae-08-media` | 媒体编解码与格式 | P1 | 01 | `crates/bae-media` |
| 09 | `collab/sync` | `bae-09-sync` | 实时协同与版本管理 | P1 | 02 | `crates/bae-sync` |
| 10 | `collab/hub-nas` | `bae-10-hub-nas` | 协作服务与 NAS | P2 | 09 | `services/bae-hub`、`deploy/` |
| 11 | `ui/shell` | `bae-11-ui-shell` | 界面外壳与工作区 | P1 | 01、02 | `crates/bae-ui`、`assets/icons/`、`docs/glossary.md` |
| 12 | `ui/motion` | `bae-12-ui-motion` | 动效工作区（AE 式） | P2 | 03、04、11 | `crates/bae-motion`、`crates/bae-ui-motion` |
| 13 | `ui/design` | `bae-13-ui-design` | 分镜/设计工作区（Ai 式）——现阶段只做框架 | P3 | 05、11 | `crates/bae-storyboard`、`crates/bae-ui-design` |
| 14 | `ui/edit` | `bae-14-ui-edit` | 剪辑工作区——现阶段只做框架 | P3 | 04、08、11 | `crates/bae-edit`、`crates/bae-ui-edit` |
| 15 | `interop/ae` | `bae-15-interop-ae` | AE 工程互转 | P0 起步 | 02 | `crates/bae-interop-ae`、`bridges/ae/`、`docs/interop/ae-*` |
| 16 | `interop/ai` | `bae-16-interop-ai` | Ai 互转 | P1 | 02、05 | `crates/bae-interop-ai`、`bridges/ai/`、`docs/interop/ai-*` |
| 17 | `agent/api` | `bae-17-agent-api` | Agent 接入（MCP） | P1 | 02（04 可先模拟） | `crates/bae-agent` |
| 18 | `agent/guide` | `bae-18-agent-guide` | Agent 动画操作规范 | P0 | —（与 17 对齐） | `docs/agent-guide/`、`skills/` |
| 19 | `core/plugins` | `bae-19-plugins` | 自有插件体系（宿主、SDK、C ABI、文档） | P1（接口在 P0 契约里先定） | 01、02、04 | `crates/bae-plugin`、`crates/bae-plugin-sdk`、`sdk/c/`、`plugins/`、`docs/plugins/` |
| 20 | `interop/aex-compat` | `bae-20-aex-compat` | AE 插件兼容（现阶段只做设计与接口预留） | P1 设计 / P3 实现 | 02、04、19 | `crates/bae-aex-host`、`docs/spec/aex-compat.md` |
| 21 | `io/packaging` | `bae-21-packaging` | 工程与素材打包、视频裁片 | P2 | 02、08 | `crates/bae-package` |

## 阶段

- **P0（现在就能开始）：** 01、02、18，以及 15 中不依赖 Rust 代码的部分（AE 导出脚本、交换格式调研）。
- **P1（01 和 02 的接口合并进 `dev` 之后）：** 03、04、05、06、08、09、11、16、17、19，以及 20 的设计文档。
- **P2（所依赖的 P1 工作流可用之后）：** 07、10、12、21。
- **P3（后续"大一统"方向，现在只做框架和接口）：** 13、14，以及 20 的兼容层实现。

02 是所有人的地基：先把公共类型和 trait 尽快合并进 `dev`，内部实现可以之后继续完善。P1 的工作流在依赖没合并前，可以先对着接口写模拟实现（mock）。

## 依赖图

```
P0   01 foundation ──────┬───────────────┬──────────────┐
     02 doc-model ───┬───┼───────┬───────┼──────┐       │
     18 agent/guide  │   │       │       │      │       │
     15 interop/ae（导出脚本部分先行）       │      │       │
                     ▼   ▼       ▼       ▼      ▼       ▼
P1   03 animation   04 compositor   09 sync   11 ui/shell   08 media   17 agent/api
                     │                │          │            │
                     ├─► 05 vector-text ─► 16 interop/ai       │
                     └─► 06 color                              │
                                                               │
     19 plugins ◄── 01、02、04        20 aex-compat（设计）◄── 02、04、19

P2   07 effects ◄── 04、06、19       10 hub-nas ◄── 09          │
     12 ui/motion ◄── 03、04、11                                │
     21 packaging ◄── 02、08 ◄────────────────────────────────────┤
P3   13 ui/design（框架）◄── 05、11                              │
     14 ui/edit（框架） ◄── 04、08、11 ◄──────────────────────────┘
```

## 怎么领取一个工作流

1. 在 Orca 里打开对应的工作区（已经切到对应分支），在里面启动 Agent，让它先读 `AGENTS.md` 和自己的任务书。
2. 开始后把 Orca 工作区状态改成 `in-progress`，备注写当前进度：
   `orca worktree set --worktree active --workspace-status in-progress --comment "..." --json`
3. 小步提交，经常 `git fetch origin && git rebase origin/dev`。
4. 每完成一个能合并的阶段就开 PR 到 `dev`，不要攒到最后。
5. 修改跨 crate 的公共类型或 trait 时，在 PR 标题里标注 `[interface]`，并同步更新 `docs/ARCHITECTURE.md` 或 `docs/spec/`。

## 合并规则

- 功能分支 → PR → `dev`。到达里程碑时再把 `dev` 合并到 `main`。
- PR 合并前：CI 通过，体积预算通过。
- 只改自己负责的目录。确实需要改别人的目录时，改动尽量小并在 PR 里说明；或者在对方任务书末尾的"请求"一节留言。
- 根 `Cargo.toml` 的成员用通配（`apps/*`、`crates/*`、`services/*`），新增 crate 不需要改它；只有新增 workspace 依赖时才改。

## 协同流程（2026-11-23 起）

- **合并**：领导审完 PR 后打 `merge-ready` 标签，由合并脚本（`tools/merge-train/`，launchd 常驻）按打标签的先后一个一个合进 `dev`。合什么由领导决定，脚本只负责执行。CI 失败、有冲突时，脚本会去掉标签，评论原因，并通知负责 agent 和领导。dev CI 变红时整个队列暂停。具体规则见 `tools/merge-train/README.md`。
- **进度**：看置顶 issue「BAE 进度看板」，它每 10 分钟根据 GitHub 和 orca 的数据自动更新。领导每一轮在里面写“交接快照”。
- **消息只用来做决定**：进度、合并、CI 结果看看板和脚本通知，不要用消息互相报告。
- **省额度**：
  - 等 CI、合进 dev、没有冲突的 rebase 这类机械活，交给脚本，或者用 Sonnet；Opus xhigh 只用于设计和写代码。
  - worker 做完一定要发 `worker_done`。
  - 只是在等待的，用一次性后台命令（条件满足就退出），不要轮询，也不要空转。
- **少冲突**：
  - 没做完的功能藏在开关后面，小步合进 `dev`。
  - 叠起来的 PR 最多两层。
  - PR 被卡住时，打 `blocked` 标签，并评论 `blocked: 原因`，看板会显示。
