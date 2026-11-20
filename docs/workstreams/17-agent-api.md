# 17 · Agent 接入（MCP）

| 项 | 内容 |
|---|---|
| 分支 | `agent/api` |
| Orca 工作区 | `bae-17-agent-api` |
| 阶段 | P1 |
| 依赖 | 02（命令）；04（截图，未完成前先模拟） |
| 负责目录 | `crates/bae-agent/` |

## 目标

让任何支持 MCP 的 Agent 都能连接 BAE、读懂工程、通过命令修改工程、看到渲染结果，并且作为一个受控的协作者参与团队工作。

## 要做

- **MCP 服务器**：使用 rmcp，支持 stdio（`bae mcp`）和 Streamable HTTP（连接正在运行的桌面应用或 Hub，本地用 token 认证）；跟随最新的 MCP 规范。
- **工具自动生成**：从 CommandRegistry 生成 MCP 工具（名字、描述、参数 Schema 都来自命令定义）。新增命令后无需修改本工作流的代码。
- **查询与视觉工具**：
  - `stage.describe`：给 Agent 读的工程摘要（合成树、图层、关键帧概况），长度可控。
  - `prim.get`、`prim.find`、`selection.get`、`attribute.evaluate`（某时刻的值）。
  - `render.frame`：返回 PNG。
  - `render.contact_sheet`：把多帧缩略图拼成一张图，用来检查动画节奏。
  - `history.*`、`sublayer.*`。
- **Agent 会话**：身份与在线状态；默认 Edit Target 是自己的提案层 `agent/<name>`；接受/拒绝/合并的 API（界面由 11 的 Agent 面板调用）。
- **安全与控制**：三级权限（只读 / 只写自己的层 / 可写任意层）；速率限制和渲染预算；审计日志，每个 Agent 操作都能追溯。
- **MCP instructions**：内置一段精简的操作规范（内容来自 18）。
- **端到端示例**：用 Claude Code 和 Codex 分别连接，完成"做一个 3 秒的标题出场动画并渲染预览"，写成可重复运行的测试脚本。

## 对外接口

- `bae mcp` 子命令、本地 HTTP 端点、Agent 会话 API。

## 验收

- Claude Code 和 Codex 都能连接并完成端到端示例。
- Agent 的修改只出现在它的提案层，用户可以逐条接受或拒绝。
- 只读权限下的 Agent 无法修改任何数据（有测试）。

## 不做

- 操作规范的内容（18）、Agent 面板的界面（11）。

## 进度

- [x] MCP 服务器（stdio，`bae mcp`）：协议层自己实现（同步 JSON-RPC，`bae_agent::protocol`），应用不链接 rmcp / tokio（release 比用 rmcp 时小 1.37 MiB，见 #29 / #30），rmcp 的客户端只在测试里检查互通；一个连接 = 一个 Agent 会话；Agent 名默认取 MCP 客户端名（Claude Code → `claude-code`），`--name` 可指定。
- [x] 从 CommandRegistry 生成工具：工具名 = 命令 ID 把 `.` 换成 `_`（Claude / OpenAI 的工具名只允许 `[a-zA-Z0-9_-]`）；描述、参数 Schema 都来自命令定义；新命令无需改 17 的代码。
- [x] 查询工具：`stage_describe`（文字摘要，`root` / `detail` / `max_chars`）、`prim_get`（每个属性的来源层和被覆盖的层、关键帧帧号、所在合成的尺寸和帧速率）、`prim_find`、`sublayer_list`、`sublayer_diff`、`schema_get`。
- [x] 视觉工具：`render_frame`、`render_contact_sheet`（帧号标在每格左上角）。渲染走 `FrameRenderer` trait，现在是占位图（结果里标 `placeholder: true`），04 合并后由 app 注入真正的渲染器。
- [x] 修改：`command_batch`（多条命令 = 一个撤销单元，全部成功或全部不生效，代替 `transaction.*`）、`history_undo` / `history_redo`（只撤销自己的）。
- [x] Agent 会话与提案层：默认写进 `agent/<name>`（第一次写入时创建在最强位置）；`proposal.accept` / `proposal.reject` 命令（整体、单个 Prim 子树或单个属性），`proposal::proposals()` 给 11 的 Agent 面板列出每个提案层的改动。
- [x] 三级权限：只读（看不到也调不了修改类工具）/ 只写自己的层（默认）/ 可写任意层。用 bae-model 新增的 `Stage::transact_guarded` 在写入之前拦截，被拒的调用不会在别人的层里留下任何记录（连 CRDT 历史都没有）。有测试。
- [x] 审计日志：每次工具调用一条（工具、参数、结果 ok / error / denied、写了哪些层、改动数、耗时），内存 + JSON Lines 文件（默认在日志目录的 `agent-audit.jsonl`）。
- [x] MCP instructions：直接内置 18 的 `docs/agent-guide/mcp-instructions/en.md`（`--instructions zh` 用中文版），后面附一段本服务器说明（工具名写法、用 `command_batch` 代替事务、哪些工具还没有）。测试检查两份文件不超过 3000 字符、提到的工具名都存在或在"暂未提供"名单里。
- [x] 端到端测试：rmcp 客户端和手写 JSON-RPC 两种方式（`crates/bae-agent/tests/mcp_e2e.rs`），并用真实的 `bae mcp` 二进制走过一遍 stdio。
- [x] 领域命令接入：`Workspace::new` 注册 bae-package 的命令（`package.*`、`asset.relink` …），带可选顶层 `project` 参数的命令由服务器自动填入当前工程位置；`Workspace::with_commands` 接收模块命令，`bae mcp` 从 `bae_app::Registry<dyn DynCommand>` 收集（bae-motion 的 `MotionModule` 发布在这里）。
- [x] 帧号和时码：所有 `Time` 参数还接受 `{"frame": 12}`、`"12f"`、时码 `"0:00:00:12"`（`;` 表示丢帧），在事务里按合成帧速率换算成 flicks（帧速率取 `frame_rate` / `fps` 参数或它的默认值，否则取参数里第一个 Prim 路径所在的合成）。时间基准跟参数本身一致：合成时间；图层上的关键帧用图层时间，和 `prim_get` 的 `key_frames` 一致。
- [x] 命名版本（09）：`Workspace` 持有 `bae_sync::History`（`.bae` 目录里的 `history.loro`，创建版本时保存），注册 `version.revert`；工具 `history_list`、`history_diff`、`history_checkpoint`、`history_revert`（经 `version.revert` 命令，受权限守卫约束）；`stage_describe` / `prim_get` 传 `checkpoint` 查看旧版本（只读）。
- [ ] 在线状态与锁提示（09 第 3 条）：等 HTTP / 协同模式里 Agent 作为 `SyncSession` 客户端时再接（stdio 模式没有会话）
- [ ] `attribute.evaluate`（等 03 的求值）、`selection.get`（等桌面应用接入）、`proposal.describe`、`comment.add`、`history.list`
- [ ] 速率限制和渲染预算
- [ ] HTTP 传输（连接正在运行的桌面应用或 Hub，本地 token 认证）
- [ ] 把完整指南作为 MCP 资源暴露（18 的建议）
- [ ] 用 Claude Code 和 Codex 实际连接跑"3 秒标题出场动画"示例

## 请求（其他工作流在这里留言）

- **18 agent/guide（2026-11-20）**：
  1. 指南按 `docs/agent-guide/tools.md` 里的预期工具名编写（`stage.describe`、`prim.get/find`、`attribute.evaluate`、`render.frame/contact_sheet`、`transaction.begin/commit/abort`、`sublayer.list/diff`、`history.*`、`proposal.describe`、`comment.add` 等）。实际命名不同没关系，合并前在 PR 里列一张对照表，18 统一改指南、Skill 和评测集。
  2. MCP instructions 请用 `include_str!` 引用 `docs/agent-guide/mcp-instructions/en.md`，**默认发英文版**（用户已确认）；`zh.md` 作为可选配置。加测试：不超过 3,000 字符、文中工具名都存在。
  3. 建议把指南作为 MCP 资源暴露（如 `bae://agent-guide/03-animation`）。
  4. `stage.describe` 输出里请直接包含：合成帧速率、时长、工作区域、当前编辑目标、Agent 自己的权限级别、锁提示。
  5. `render.contact_sheet` 每格标出帧号；时间参数接受相对合成开始的整数帧号（也接受时码），由服务器换算 flicks。
     - **17 回复**：已做（帧号、`"12f"`、时码含丢帧）。帧号用参数本身的时间基准：合成时间不变；图层上的关键帧是图层时间，图层从 0 开始、伸缩 100% 时和合成帧号相同。要按合成帧号写图层关键帧，命令参数需要标明"这是关键帧时间"，由 02 / 12 在 schema 里约定后再做。
  6. 评测集 `docs/agent-guide/evals/` 需要审计日志能导出完整的工具调用序列（含事务边界和写入的工程层）。

- **21 io/packaging（2026-11-20）**：`bae-package`（无 GPU 依赖）提供打包命令 `package.scan`、`package.pack`、`package.collect`、`package.unpack`、`asset.relink`、`asset.set_files`，注册函数是 `bae_package::register_commands(&mut registry)`。请在 `Workspace::new` 里一起注册，Agent 才能调用。这些命令只能看到 Stage，工程位置通过参数 `project`（`.bae` 目录或 `.baez` 文件）传入；如果方便，服务器可以在调用时自动填入当前工程的位置。
  - **17 回复**：已做。`Workspace::new` 注册 bae-package 的命令；顶层可选参数 `project` 由服务器自动填入，工具说明里也写了可以省略。

- **09 collab/sync（2026-11-20）**：版本相关的能力已经在 `bae-sync` 里，Agent 需要的话：
  1. **回滚是命令**：`bae_sync::register_commands(&mut registry)` 注册 `version.revert`（参数 `version`、可选 `sublayer` 或 `prim`），请在 `Workspace::new` 里一起注册。`version` 的 JSON 形状是"工程层 → (Loro peer ID 字符串 → 操作数)"，即 `ProjectVersion::to_json()` 的输出。
  2. **命名版本是元数据，不在 Stage 里**：请让 `Workspace` 持有一个 `bae_sync::History`（打开时 `History::load(工程目录)`，保存工程时 `history.save(工程目录)`；协同时用 `ProjectDocs { stage, history }` 交给 `SyncSession`）。建议的工具（18 的指南里叫 `history.*`）：`history.list`（`history.checkpoints()`，每项带 `version.to_json()`）、`history.checkpoint`（`create_checkpoint(stage, author, name, note)`）、`history.diff`（`History::diff(stage, from, to)`，`to` 省略表示当前；输出 `PrimDiff` 的路径、变化类型、属性、作者、时间）、`history.revert`（按命名版本 ID 查到 `version` 后执行 `version.revert` 命令，这样修改仍然经过命令总线）。只读查看旧版本可以用 `History::view(stage, id)` 拿到只读 `Stage`，复用 `stage.describe` 的输出。
  3. **锁提示**：协同时 Agent 也是一个客户端，建议用 `SyncSession::set_presence`（`user` 填 `agent:...`）让人看到它在线；`stage.describe` 里的"锁提示"可以用 `SyncSession::editing_by_others(path)` 得到"谁声明在改这个合成"。
  规格见 `docs/spec/sync.md` 第 5、6 节。
  - **17 回复**：第 1、2 条已做，`history.list/checkpoint/diff/revert` 四个工具都加了，查看旧版本是给 `stage.describe` / `prim.get` 加了 `checkpoint` 参数。`history.revert` 只是按版本 ID 查到 `version`，然后执行 `version.revert` 命令，修改照常经过命令总线和权限守卫：只能写自己层的 Agent，回滚整个工程时如果会动到别人的层，就会被拒绝。第 3 条（在线状态、"谁在改"）要等 Agent 以 `SyncSession` 客户端的身份参与协同时再做（HTTP 传输 / Hub）。stdio 的 `bae mcp` 是单独一个进程，没有会话。
