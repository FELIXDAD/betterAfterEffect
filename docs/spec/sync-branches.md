# 分支与合并（设计草案）

> 负责：C（@zjzcoder），epic #737 子任务 2。状态：**草案，待确认**（见文末"待确认的问题"）。确认后，实现 PR G0 会把这一章并入 `docs/spec/sync.md`（5.5），本文件删除。
> 产生方式：三个独立方案（分叉文档 / USD 式分支层 / 最小可行），两位评审打分后以"分叉文档"为基础合成，并入了其他方案的改进。下文的小节编号沿用并入后的 `sync.md` 5.5。

> 实现计划：PR G0–G5，见 5.5.12。并入 `sync.md` 时，原 5.5"还没做"改为 5.6，去掉"分支与合并"一项。
> 来源：以 fork-docs 方案为基础，并入两位评审建议的改进：按需同步、预览和提交走同一套代码、`expect`、靠提交来源判断合并是否已撤销、遮挡提示、精简冲突存储、不做 GC。

#### 5.5.1 概念

- **线（`Line`）**：分两种。
  - `Main`（主线）：就是现在的工程，即 `Project.stage` 加 `project.json`。
  - `Branch(BranchId)`：一个分支。
- **分支**：从父线（主线或另一个分支）的某个 `ProjectVersion` 分出来的线。
  - 父线层叠里的每个工程层各自用 Loro `fork_at` 复制一份文档。
  - 复制品保留分叉点之前的全部历史。所以分叉前就有的 Prim 在两边 TreeID 相同，分叉点的内容两边都能读出来（已在 loro 1.18 验证）。
  - 分支可以独立编辑，有自己的命名版本和撤销栈（按作者分）。
- **合并**：只在分支和它的直接父线之间进行，并且总是写入当前检出的线（目标，"ours"）。另一条线是来源（"theirs"）。
  - 合并到父线（`into_parent`）：在主线上执行"合并分支 X"，例如客户选定 B 方案后合回主线。
  - 从父线更新（`from_parent`）：在分支上执行"从主线更新"，例如把主线上改过的 logo 和字幕带进 B 方案。
- 合并是**一个命令** `branch.merge`。
  - 按节点 ID 做逐元素的三方合并，结果作为普通 `Op` 写入目标线。
  - 整个合并是一个 `Stage::transact`，也就是合并者的一个撤销单元。
  - **从不**把来源的 CRDT 操作导入目标文档。
- **冲突**：两边自基准以来都改了同一个元素，而且改成了不同的值。
  - 报告里有基准值、双方的值，以及双方最后修改的作者和时间。
  - 按 `prefer`（默认 ours）或逐条的 `take` 决定取哪边。
  - 合并后还能逐条改选（`branch.resolve`）。

| 类型 | 说明 |
|---|---|
| `BranchId` / `MergeId` | UUID v7 十六进制，32 位，按时间排序，同 `CheckpointId` |
| `Lines` | 新模块 `lines.rs`：本客户端打开的分支文档，加上当前检出的是哪条线。主线不在里面 |
| `NodeKey` | 节点身份 `"peer:counter"`，以父线空间为准（5.5.5） |
| `MergeInput` | 来源侧的变化集，在事务外从来源文档和基准文档算出，可序列化 |
| `MergeReport` / `Conflict` | 合并结果和冲突报告（5.5.6） |

#### 5.5.2 数据与存储

**文档键**

- 新增 `DocKey::Branch(BranchDoc { branch, sublayer })`。线上和文件名的形式都是 `@branch/<32 位小写十六进制>/<工程层 ID>`。
- 解析步骤：去掉前缀，`split_once('/')`，校验 32 位十六进制，余下部分用 `SublayerId::new` 解析。
- `as_sublayer()` 返回 `None`，所以 `impl DocSet for Stage` 永远不会把分支数据写进主线。
- `min_protocol()` 对分支键为 2，`PROTOCOL_VERSION` 升到 2。

**文件**（`.bae` 目录）

- `branches/<branch-id>/<工程层>.loro`：完整快照，带历史，能离线单独打开。
  - 保存时由 `Lines::save_reader` 在保存线程写入（临时文件加改名）。
  - 工程层 ID 里的 `/` 变成子目录。
- 共享目录日志：`updates/@branch/<branch-id>/<工程层>/<client>.log`。沿用 `DirTransport` 现有逻辑，格式不变。`poll` 增加遍历 `branches/`，作为分支键的 `Base` 来源。
- 分支数据**永不**写进 `sublayers/`、`project.json` 或 `.baez`。
  - 分支的 Stage 不进入 `Project`。原因是 `Project::save`、`remove_stale`、`DirTransport` 读 `sublayers/*.loro` 都只认主线；分支 Stage 放进 `Project` 会把分支操作不可逆地漏进主线。
  - 旧客户端解析不了这些键，会跳过 `@branch` 目录和 `branches/`。

**内存**

- `Lines { branches: BTreeMap<BranchId, LineDocs>, working: Line }`，其中 `LineDocs = Raw(BTreeMap<SublayerId, LoroDoc>) | Open(Stage)`。
- 只放本客户端打开过、或正在合并的分支。
  - Raw 只是 Loro 文档：导入便宜，没有镜像。
  - 检出或合并时转成 Open，用分支层叠做 `Stage::from_sublayers`。
  - 切走以后仍保持 Open，所以每条线的撤销栈都还在。
- 应用和 Agent 只能通过 `lines.stage(&project.stage)` / `lines.stage_mut(&mut project.stage)` 访问"当前线"。命令、撤销、面板、渲染、保存提示都走这两个访问器。
- 如果直接用 `project.stage`，用户在分支上时就会改到主线。11 和 17 必须改用访问器，有测试覆盖（5.5.11）。

**历史文档**（`@history`，只增加字段，旧客户端会忽略）

根 `checkpoints` 不变，仍然只放主线。新增两个根 map，提交信息照旧是 `{"author","label"}`。

`branches/<branch-id>`：

| 键 | 类型 | 说明 |
|---|---|---|
| `name`、`note`、`author`、`created` | | 同命名版本 |
| `parent` | string | `""` 表示主线，否则是父分支的 ID |
| `fork` | map | 工程层 → 父线在分叉点的版本向量（`VersionVector::encode`） |
| `fork_checkpoint` | string | 可选：从哪个命名版本分出 |
| `stack` | string | 分支层叠，JSON 数组，强 → 弱，后写者胜。分支上新建工程层时更新 |
| `status` | string | `active` 或 `abandoned`，另有 `status_by`、`status_at`，后写者胜 |
| `checkpoints` | map | 分支上的命名版本，结构同根 `checkpoints`，版本向量指分支文档 |
| `links` | map | 工程层 → mergeable map。键 `"<分支节点>><父线节点>"`，值是写入时间（毫秒） |

`links` 记录"这个 Prim 是合并时复制过去的，两边是同一个"。键里同时带两边的 ID，所以并发合并不会互相覆盖，重复也能查出来。

`merges/<merge-id>` 的字段：

- `branch`
- `direction`：`into_parent` 或 `from_parent`
- `author`、`created`
- `label`
- `prefer`
- `base`：`{record, line, version}`
- `theirs`：来源版本
- `before` / `result`：目标线在合并前后的版本
- `touched`：写过的工程层
- `summary`：JSON 计数
- `conflicts`：JSON，见 5.5.6，每次合并上限 256 KiB
- `resolutions`：冲突 ID → `{side, author, at_ms, changed_since_merge}`，后写者胜
- `undone_hint`：只在过渡期使用，见 5.5.7

#### 5.5.3 同步

- **发现**：分支记录在 `@history` 里，所以所有人都能看到分支列表、作者、状态和合并记录。
- **内容按需同步**：客户端只同步自己打开的分支，或正在合并的分支。
  - 打开时先在本地对父线文档做 `fork_at(fork)` 物化，`Hello` 里报出分叉点的版本，对方就只发分叉之后的增量。已验证：`fork_at` 再导入 `export(updates(fork_vv))`，结果与原分支完全一致。
  - 本地父线还没有分叉点的操作时，从空文档开始，由对方发全量。
- `SessionConfig::push_unlisted_branches` 控制是否推送对方没列出的分支：
  - 客户端默认 `true`：把自己有的分支都上传，保证分支不会只留在创建者的机器上。
  - `Relay` 的客户端连接用 `false`：只发给 `Hello` 里列出了这个分支、或者发来过它的数据的客户端。否则 `send_missing` 会对没列出的键从空版本开始导出，而 `remote` 在每次 `Hello` 时都会被替换，结果是每次 `Hello` 都把整份分支重发给所有人。
  - Hub 的存储连接用 `true`，所以所有分支都会写到 `updates/@branch/…` 落盘。
- `DirTransport`：会话最近一次 `Hello` 没列出的分支键，`poll` 不读，`on_hello` 也不重发。列出之后从头读取，流程同 2.5。
- **协议门槛**：`SyncSession` 只把 `min_protocol() <= remote_protocol.unwrap_or(1)` 的键放进 `Hello` 和数据消息。
  - 第一个 `Hello` 发出时还不知道对方的协议版本，因此不含分支键。对方的 `Hello` 表明协议 ≥ 2 以后，分支键按现有的 `announced` 逻辑当作新文档补发。
  - v2 的解码器会跳过不认识的 `@…` 键。今天的解码器遇到一个解析不了的键会让整条 `Hello` 失败。
  - 旧 Hub 不转发分支，主线和 `@history` 照常同步。界面根据 `/v1/info` 报的 `protocol` 提示"Hub 需要升级才能共享分支"。
- **路由**：新增 `ProjectDocs::with_lines(stage, history, lines)`；`new` 的签名不变，相当于没有分支。
  - `Sublayer` 键 → 主线。
  - `History` 键 → 历史文档。
  - `Branch` 键 → `Lines`：Open 的走 `Stage::import_updates`（origin = remote，不进撤销栈），Raw 的走 `LoroDoc::import`。收到本地没打开的分支键时先建一个 Raw 存起来，不丢数据。
  - `PumpOutcome::branch_events: Vec<(BranchId, StageEvent)>` 与主线事件分开。
  - `SyncSession::remote_version(&DocKey)` 用于显示"分支已与协作者同步"。
- **在线状态**：`PresenceState` 增加 `line` 和 `merging`，旧客户端把它们放在 `extra` 里转发。建议性锁只显示给同一条线上的人。

#### 5.5.4 生命周期

- **新建**：入口是历史面板的"从此版本新建分支"、分支菜单的"新建分支…"，Agent 用 `branch_create`。调用 `Lines::create(main, history, author, name, note, BranchFrom::Current | Checkpoint(id))`。
  - 父线：当前线，或命名版本所在的线。
  - 分叉版本：当前版本，或命名版本记录的版本。
  - 层叠：父线当前的层叠。从命名版本分出时，取命名版本的层叠与父线现有工程层的交集，缺少的工程层报告出来。
  - 父线文档还没有分叉版本的操作时，返回 `VersionNotAvailable`。
  - 写入 `branches/<id>` 记录，并加入一个 Raw。这不是撤销单元，因为它不改任何线的数据，与 `create_checkpoint` 相同。
- **检出**：入口是标题栏的"主线 ▾"或"分支：客户 A 方案 ▾"，Agent 用 `branch_checkout`（只在独立运行的 `bae mcp` 里提供）。调用 `Lines::checkout(main, history, &line) -> Switched { from, to, event }`。
  - 不交换 `Project` 里的任何东西。
  - 应用收到 `Switched` 后，把 `HistoryLog` 等订阅挂到新的 Stage 上，面板整体刷新。
  - 与用户共享工作区的 Agent 不能切线，否则会改变用户看到的内容。
- **编辑**：用普通命令，撤销按线、按作者分开。
  - 分支上的命名版本写在 `branches/<id>/checkpoints`。
  - `view_at`、`diff`、`version.revert` 对分支 Stage 原样可用，因为分支文档含分叉前的历史。回到分叉之后的主线版本会返回 `VersionNotAvailable`。
  - 在分支上新建工程层，会产生新文档 `@branch/<id>/<新 ID>`，并由 `Lines::sync_stacks` 更新 `stack`。
- **保存与打开**：
  - 保存：先 `project.save()`（只有主线，不变），再 `history.save`，然后 `lines.save_reader(dir)` 在保存线程执行。
  - 打开：`Lines::load_reader(dir, &history)` 在加载线程执行，只加载上次打开过的分支。
- **放弃**：设 `status = abandoned`，分支从列表隐藏，可以恢复。**不删除**任何数据，日志清理要等 4.7。
  - 如果有人放弃了你正检出的分支，界面显示横幅"恢复 / 回到主线"，编辑不受影响。
- **提示**：
  - `has_unmerged_changes`：分支的最新版本不在最近一次有效合并的 `theirs` 里时，显示"有尚未合并的修改"，所有人可见。
  - 在分支上时，后台线程定期对"从主线更新"做一次预览，显示"主线改了 N 个你也改过的属性"。

#### 5.5.5 三方合并（按节点 ID）

**基准**（`MergeBaseChoice::Auto`）

- 候选：这个分支的合并记录 R 里**有效**的那些。有效要同时满足两条：
  - R 写入的那条线已经包含 `R.result`；
  - R 没有被撤销（5.5.7）。
- 如果目标线不包含最新一条记录的 `result`（记录先于数据到达），返回 `VersionNotAvailable`（"等待 <作者> 的合并数据同步"）。用户可以手动改用 `Fork` 或 `After(id)`。
- 每条 R 的含义是"线 X 在版本 v 的内容被并入线 Y，得到 Y@r"：
  - `into_parent` 时 X 是分支，`from_parent` 时 X 是父线；
  - v = `R.theirs`，r = `R.result`。
- R2 晚于 R1：父线一侧和分支一侧的版本都包含 R1 的（逐层 `includes_vv`）。
- 有唯一一条最新的 R 时：
  - 基准内容 = X@v，相当于 git 的 merge base。
  - 作者窗口：来源一侧从 (X 是来源 ? v : r) 开始算，目标一侧从 (X 是目标 ? v : r) 开始算。
- 没有记录，或者因为并发合并没有唯一最新的：退回分叉点（父线@fork 等于分支@fork），并在报告里提示。
- 用到的每个版本都在它所在线的文档里，分叉点则两边都有。基准层用现有的 `layer_at`（`SnapshotAt`）构建。

**节点身份**（`identity.rs`，与 diff 和回滚共用）

- 父线空间的 TreeID 本身就是 `NodeKey`。分支空间的 TreeID t，如果 `links` 里有 (t, p)，就映射成 p；否则就是 t。分叉前的节点两边 TreeID 相同。
- 来源节点与目标节点按以下顺序匹配：
  1. `NodeKey` 相等。
  2. 一个来源节点链接到了多个目标节点（并发合并造成）：取合并 ID 最小的那个，并报告 `Duplicate`。
  3. **重建别名**：某一边在父节点 p 下新建了一个节点，p 在基准里恰好有一个同名同类型、且在这一边已经被删掉的子节点，就把两者视为同一个。这是为了撤销删除的情况：`RestorePrim` 重建时 TreeID 会变。
  4. **合并产物**：两边都没匹配上的节点，在同一个已匹配的父节点下同名同类型，而且目标节点是由本分支对的合并或改选提交创建的（提交标签带 `[<merge-id>]`），就视为同一个，并补写 `links`。这条覆盖"合并后崩溃、没写记录"和"撤销再重做合并"两种情况。
  5. 其余同名同类型的双方新增：报告 `AddAdd` 冲突，由用户决定，**不自动合并**。
- "同名"比较的是原始名。只有当镜像名的后缀恰好等于该节点自己的 `_<peer>_<counter>` 时才去掉，与 `Sublayer::rebuild` 的规则一致。
- 目标线的路径永远在应用每个 Op 之前，由目标 TreeID 现查（`node_path`）。基准和来源的路径只用于显示。

**元素**（逐工程层合并，同 ID 对同 ID；不做合成层面的合并）

- 每个匹配上的 Prim：`Exists`、`Name`、`Parent`（按 `NodeKey`）、`Specifier`、`Type`、`References`（整个列表当一个值）。
- 每个属性：`Value`、`Expression`，以及每个关键帧时间一个 `Keyframe{time}`。关键帧本来就按时间存在 mergeable map 里，所以一边在 3 秒加帧、另一边改 1 秒的帧，不算冲突。
- 每个父节点：`ChildOrder`。合成里的图层顺序就是子节点顺序。

**三方规则** `merge3(base, ours, theirs)`（不存在记为 `None`）

- theirs == base：不动。
- ours == base：取 theirs。
- ours == theirs：不动（两边做了相同的修改）。
- 其他情况是**冲突**：先看 `take[id]`，没有就用 `prefer`（默认 ours）。

**结构**

- 来源新增（基准里没有）：
  - 在已匹配的父节点下用 `RestorePrim{tree: 来源子树}` 建出最上层的新节点。位置放在"来源里前一个、目标里也存在的兄弟"之后。
  - 并行遍历两棵子树，为每个节点写一对链接。
  - 父节点在目标里已被删除：`AddedUnderDeleted` 冲突。
  - 同名但类型不同，或名字被无关节点占用：`NameClash` 冲突，默认（`both`）以 `<name>_2` 建立。
- 来源删除、目标仍然有：
  - 目标子树自基准以来没改过，也没有目标独有的子孙：`DeletePrim`，只删最上层。
  - 否则报告 `DeletedByTheirs` 冲突。
- 目标删除、来源修改过：`DeletedByOurs` 冲突。取 theirs 时从来源恢复子树并写链接。
- 移到目标里已删除的父节点下：`Reparent` 冲突。
- 会形成环：`Cycle` 冲突，永远不应用。
- `ChildOrder`：
  - 设 C 为三边都有的子节点，seqB、seqO、seqT 分别是 C 在基准、目标、来源里的顺序。
  - seqT == seqB，或 seqO == seqT：不动。
  - seqO == seqB：保留槽位重排。C 按来源的相对顺序填回 C 在目标里占的那些位置，目标独有的子节点位置不变，用 `MovePrim{from: p, to: p, index}` 实现。
  - 其他情况：`ChildOrder` 冲突。
- 只在来源里有的工程层：
  - 报告为跳过。对话框提供"先在<目标线>新建这些工程层"，运行现有的 `sublayer.create`（层叠变化照旧不可撤销），再重新预览；这些工程层的内容以空基准合并。
  - **不在** `branch.merge` 事务里新建工程层，因为事务回滚撤不掉层叠变化。
- 只在目标里有的工程层不动。层叠顺序的差异只报告，不合并。
- 锁定的 AE 图层：不写，报告为 `Locked`，带双方的值；除非指定 `include_locked`（用 `tx.allow_locked`）。
- `tx.may_write` 为假的工程层（Agent 写权限不够）：在写任何东西之前就整层跳过并报告。
- **遮挡提示** `Shadowed{by}`：合并进较弱工程层的值，被目标线里更强工程层的意见挡住了，合并后看到的结果会和在来源线上看到的不同。

**应用顺序**（全部经 `tx.apply_to(sublayer, op)`，在同一个事务里）

1. 新建子树，自上而下。因为名字被占而失败的先推迟。
2. 移动和改名，按依赖排序。互换名字时经过临时名 `__merge_<n>`。
3. 推迟的新建。
4. 删除，只删最上层。放在移动之后，这样来源移出去的子节点能保留。
5. `SetSpecifier`、`SetPrimType`、`SetReferences`。
6. `SetValue`、`SetExpression`、`SetKeyframe`、`RemoveKeyframe`。属性在基准和目标里都没有时用 `SetAttr`。与 `version.revert` 一样，不重新做 schema 校验。
7. 重排。

出错处理：

- `WriteDenied` 中止整个合并。
- 其他模型错误（名字被占、父节点不在等）变成不可选的 `Structural` 冲突，保留目标，合并继续。
- 目标一侧的作者在应用第一个 Op 之前就算好。

**作者**

- 在各自的作者窗口里，用 `export_json_in_id_span` 一次遍历所有操作（扩展现有的 `node_authors`）。
- 用 `get_path_to_container` 把操作定位到 `[prims, Node(t), attrs, <attr>, (keys)]`，这个路径形状已验证。树操作对应 `Exists` 和 `Parent`；元数据键 `name`、`spec`、`type`、`refs` 对应各字段。
- 作者取自提交信息，时间取自提交时间戳，Lamport 最大的是最后修改者。
- 已删除节点上的修改只能归到删除操作，因为 `get_path_to_container` 对它返回 None。现有的 diff 也有同样的限制。

#### 5.5.6 冲突报告与解决

```rust
pub struct Conflict {
    pub id: ConflictId,           // blake3(sublayer|NodeKey|kind|element) 前 8 字节十六进制；同一份输入多次预览结果稳定
    pub sublayer: SublayerId,
    pub path: PrimPath,           // 目标线当时的路径，只用于显示
    pub node: NodeRef,            // { key: NodeKey, target: Option<String>, source: Option<String> }（TreeID "peer:counter"）
    pub kind: ConflictKind,
    pub parts: Vec<ConflictPart>, // 同一属性的多个关键帧 = 一个冲突、多个 part
    pub applied: Side,            // 合并后留下的是哪边
    pub choices: Vec<Side>,       // 可选项；Structural 为空
}
pub struct ConflictPart {
    pub element: Element, // Value{attr} | Expression{attr} | Keyframe{attr,time} | Name | Parent | Specifier | Type | References | Exists | ChildOrder
    pub base: Option<Json>, pub ours: Option<Json>, pub theirs: Option<Json>, // bae-model 的 JSON 形式；超限时为 by_ref
    pub ours_by: Vec<Authorship>, pub theirs_by: Vec<Authorship>,            // 新 → 旧
}
pub struct Authorship { pub author: AuthorId, pub at_ms: i64, pub via_merge: Option<MergeId> } // via_merge 从提交标签解析
pub enum Side { Ours, Theirs, Both /* 仅 AddAdd、NameClash */ }
pub enum ConflictKind { Attr { name: String }, Field { name: String }, Rename, Reparent, ChildOrder, DeletedByTheirs,
    DeletedByOurs, AddedUnderDeleted, AddAdd, NameClash, Cycle, Locked, Duplicate, Structural { reason: String } }
pub struct MergeReport { pub merge: MergeId, pub base: BaseRef, pub applied: usize, pub changes: Vec<PrimDiff>,
    pub conflicts: Vec<Conflict>, pub notices: Vec<Notice /* Shadowed | BaseFellBackToFork | MatchedByMerge */>,
    pub skipped: Vec<Skipped>, pub links: Vec<NodeLink>, pub result: Option<ProjectVersion> }
```

- **不算冲突的情况**：
  - 两边改成同一个值；
  - 只有一边改了；
  - 同一属性、不同时间的关键帧；
  - 不同的属性或不同的字段。
- 干净的修改以现有的 `PrimDiff` 列在 `changes` 里，界面复用差异视图。
- 合并时还没收到的协作者编辑不在报告里：它们按 CRDT 后写者胜处理，属于子任务 5 的"并发修改提示"。
- **合并前决定**：
  - 对话框（或 Agent 的预览）先给出报告，用户切换 `prefer` 或逐条设 `take`。
  - 提交时用预览时的**同一份** `MergeInput`（来源版本固定），并带上 `expect`，即预览时可选冲突 ID 的集合。
  - 提交时只重新规划目标一侧。如果冲突集合变了（例如中间导入了远端修改），返回 `InvalidParams` 并列出增减的 ID，什么都不写，需要重新预览。
- **合并后改选**：`branch.resolve { merge, conflict, side, input }`，一次改选是一个撤销单元，标签 `resolve [<merge-id>/<conflict-id>]`。
  - 如果当前值与合并时留下的值不同（之后有人改过），仍然执行，返回 `changed_since_merge: true`，界面给出提示。
  - 结构类的改选包括：恢复子树（并写链接）、删除、改名、重排。
  - `NameClash` 取 ours：删掉副本。取 theirs：删掉自己的，把副本改回原名。
  - "全部使用分支的值"用一次 `run_steps` 执行多条改选，合起来是一个撤销单元。
  - "标记为已解决"只写 `resolutions`，不改数据。
  - 清理 `Duplicate` 时删掉合并 ID 较大的那一份，这样两个人分别清理也会删同一份。
- **存储**：`merges/<id>/conflicts` 存 JSON。单个值超过 16 KiB，或一次合并合计超过 256 KiB 时，改存 `by_ref { line, version, node }`，改选时再从那条线的文档用 `layer_at` 重算。
- **界面**（`crates/bae-ui-motion/src/collab/`）：
  - "合并冲突"面板按 Prim 分组。每行例如"不透明度 · 1:00 处关键帧 | 主线 50% — 张三 10:32 | 分支 80% — 李四 09:15 | [使用分支的值] [保留主线的值] [标记为已解决]"。
  - 属性面板对未解决的冲突和 `Shadowed` 显示角标，由 12 挂接。
  - 所有文字用英文作键，配 zh 表。

#### 5.5.7 撤销语义

- 一次合并等于一个 `Stage::transact`，也就是合并者的一个 `UndoEntry`，标签为 `merge "<分支名>" [<merge-id>]`。
  - 应用的调用方式：`stage.transact(author, &merge_label(&input), |tx| registry.get("branch.merge")…run_json(tx, params))`。
  - Agent 用 `run_steps(Some(label), …)`。
  - 不用 `CommandRegistry::execute`，因为它会把 TITLE 当标签。改选也照此处理。
- 协作者以远端修改的形式收到合并，不进他们的撤销栈。只有合并者能撤销。撤销会产生新的反向操作，不改写历史。
- **合并是否仍然有效**从目标文档的提交信息推出：在 `R.touched` 的文档里，找标签带 `[<merge-id>]`、且不以 `rollback:` 开头的最后一个提交（按 Lamport），如果它是撤销，就是无效。
  - 这需要 02 改一行：`Stage::replay` 提交时在信息里加 `"origin": "undo" | "redo"`。
  - 为什么不能只数这个标签出现次数的奇偶：Loro 会把同一个 peer、提交信息相同、间隔在 1000 秒内的连续提交合并成一个 Change（loro-internal 1.18 的 `Change::can_merge_right` 要求 `commit_msg` 相等）。合并后马上撤销，两次提交就会被合成一个，数不出来。带上 `origin` 后两次的提交信息不同，就不会被合并。
  - 02 的修改到位之前的过渡做法：应用和 Agent 在撤销或重做前读 `undo_label` / `redo_label`，调用 `bae_sync::note_undo(history, label, undone)` 写 `undone_hint`。
- 选基准时跳过无效的合并。所以"撤销合并后再合并"会把那些修改重新带回来；`Fork` 是手动兜底。
- 预览**从不**在真实的 Stage 上运行。失败的事务会写反向操作并提交 `rollback: <label>`，在历史里留下噪声。
  - `preview` 在 `snapshot_reader()` 得到的副本上、在工作线程里运行**同一套**规划和应用代码，然后丢弃副本。
  - 所以预览和实际合并的冲突 ID、`Structural` 条目完全一致。

#### 5.5.8 并发与离线

- 合并事务在 Stage 线程上两次 `pump` 之间执行，远端导入不会插进来。预览和提交之间导入的远端修改由 `expect` 发现。
- 合并时还没收到的目标线并发修改：按 CRDT 逐字段、逐关键帧后写者胜，与任何并发编辑一样。
- 合并之后来源线上的新修改由下一次合并带上，因为基准已经前移。
- 两个人（或离线时）同时合并同一个分支：
  - 两边写的属性值相同，无害。
  - 新增的 Prim 各建一份，于是两个目标节点链接到同一个来源节点。下次预览报告 `Duplicate`，按确定的规则清理。
  - 两条合并记录都保留。选基准时因为没有唯一最新的记录，退回分叉点：冲突可能变多，但不会悄悄丢东西。
  - 合并期间通过 `PresenceState.merging` 提示其他人"李四 正在合并此分支"。
- 两个人同时改选同一个冲突：都写操作（值按后写者胜），`resolutions` 也按后写者胜，界面显示最后改选的人。
- 离线：新建（要求本地父线有分叉版本）、打开本地已有的分支、编辑、预览、合并、改选都可以离线做，重新连上后照常同步。
  - 没打开过的分支在离线时只能从分叉点物化，看不到别人在分支上的修改，界面标明"可能不是最新"。
- 线程：
  - 分支文件只在加载和保存线程读写。
  - 大分支的 `MergeInput` 用 `merge_input_reader` 在工作线程上，从固定版本的副本构建。
  - Stage 线程只做规划和应用，不做 I/O。

#### 5.5.9 API 与命令

```rust
// protocol.rs / session.rs
pub const PROTOCOL_VERSION: u32 = 2;
pub enum DocKey { Sublayer(SublayerId), History, Branch(BranchDoc) }
impl DocKey { pub fn branch(b: &BranchId, s: &SublayerId) -> Self; pub fn as_branch(&self) -> Option<(&BranchId, &SublayerId)>; pub fn min_protocol(&self) -> u32; }
pub struct SessionConfig { /* … */ pub push_unlisted_branches: bool }
pub struct PumpOutcome { /* … */ pub branch_events: Vec<(BranchId, StageEvent)> }
impl SyncSession { pub fn remote_version(&self, doc: &DocKey) -> Option<&VersionVector>; }
// lines.rs
pub enum Line { Main, Branch(BranchId) }
pub enum BranchFrom { Current, Checkpoint(CheckpointId) }
impl Lines {
    pub fn load_reader(dir: &Path, history: &History) -> impl FnOnce() -> Result<Lines, SyncError> + Send;
    pub fn save_reader(&self, dir: &Path) -> impl FnOnce() -> Result<(), SyncError> + Send + use<>;
    pub fn working(&self) -> &Line;
    pub fn stage<'a>(&'a self, main: &'a Stage) -> &'a Stage;
    pub fn stage_mut<'a>(&'a mut self, main: &'a mut Stage) -> &'a mut Stage;
    pub fn open(&mut self, main: &Stage, history: &History, id: &BranchId) -> Result<&Stage, SyncError>; // fork_at 物化或从文件加载
    pub fn create(&mut self, main: &Stage, history: &mut History, author: &AuthorId, name: &str, note: &str, from: BranchFrom) -> Result<Branch, SyncError>;
    pub fn checkout(&mut self, main: &Stage, history: &History, line: &Line) -> Result<Switched, SyncError>;
    pub fn close(&mut self, id: &BranchId);
    pub fn sync_stacks(&mut self, history: &mut History, author: &AuthorId) -> Result<(), SyncError>;
}
// history.rs（新增）
impl History {
    pub fn branches(&self) -> Vec<Branch>;  pub fn branch(&self, id: &BranchId) -> Option<Branch>;
    pub fn rename_branch(&mut self, id: &BranchId, author: &AuthorId, name: &str, note: Option<&str>) -> Result<(), SyncError>;
    pub fn set_branch_status(&mut self, id: &BranchId, author: &AuthorId, s: BranchStatus) -> Result<(), SyncError>;
    pub fn checkpoints_in(&self, line: &Line) -> Vec<Checkpoint>;  pub fn create_checkpoint_in(/* line, stage, author, name, note */);
    pub fn links(&self, id: &BranchId, s: &SublayerId) -> Vec<NodeLink>;
    pub fn merges(&self, id: &BranchId) -> Vec<MergeRecord>;
    pub fn set_resolution(&mut self, merge: &MergeId, c: &ConflictId, r: &Resolution, author: &AuthorId) -> Result<(), SyncError>;
}
// merge.rs
pub struct MergeRequest { pub source: Line, pub base: MergeBaseChoice /* Auto | Fork | After(MergeId) */, pub scope: RevertScope }
pub struct MergeOptions { pub prefer: Side, pub take: BTreeMap<ConflictId, Side>, pub expect: Option<BTreeSet<ConflictId>>, pub include_locked: bool }
pub fn merge_input(main: &Stage, lines: &mut Lines, history: &History, req: &MergeRequest) -> Result<MergeInput, SyncError>;
pub fn merge_input_reader(/* 同上，固定版本的副本 */) -> impl FnOnce() -> Result<MergeInput, SyncError> + Send;
pub fn preview(target_copy: Stage, input: &MergeInput, opts: &MergeOptions) -> Result<MergeReport, SyncError>;
pub fn merge_label(input: &MergeInput) -> String;
pub fn record_merge(history: &mut History, author: &AuthorId, input: &MergeInput, report: &MergeReport) -> Result<(), SyncError>;
pub fn merge(main: &mut Stage, lines: &mut Lines, history: &mut History, author: &AuthorId, req: &MergeRequest, opts: &MergeOptions) -> Result<MergeReport, SyncError>;
pub fn merge_in_effect(target: &Stage, record: &MergeRecord) -> Option<bool>;
pub fn note_undo(history: &mut History, label: &str, undone: bool) -> bool; // 过渡期
pub fn resolve_input(history: &History, lines: &mut Lines, main: &Stage, merge: &MergeId, c: &ConflictId, side: Side) -> Result<ResolveInput, SyncError>;
pub fn has_unmerged_changes(history: &History, lines: &Lines, id: &BranchId) -> bool;
```

- **命令**：由 `register_commands` 注册，与 `version.revert` 放在一起。
  - `branch.merge`（TITLE "Merge Branch"）：参数 `{input: MergeInput, prefer?: "ours"|"theirs", take?: {id: side}, expect?: [id], include_locked?: bool}`，输出 `MergeReport`。DESCRIPTION 写明：`input` 由 bae-sync 或 `branch_merge` 工具生成，不要手写。
  - `branch.resolve`（TITLE "Resolve Merge Conflict"）：参数 `{merge, conflict, side, input: ResolveInput}`，输出 `{edits, changed_since_merge}`。
  - 新建、改名、放弃、恢复、检出、记录合并都不是命令：它们是 History 元数据和线的生命周期操作，先例是 `create_checkpoint`。
- **Agent 工具**（由 17 实现，模式同 `history_revert`）：
  - `branch_list`、`branch_create`、`branch_rename`、`branch_abandon`、`branch_restore`；
  - `branch_diff`：分叉以来分支上的修改；
  - `branch_merge_preview {source, prefer?, take?}`；
  - `branch_merge {source, prefer?, take?, expect?, prim?, sublayer?}`；
  - `merge_conflicts`、`merge_resolve`、`merge_resolve_all`；
  - `branch_checkout`：只在独立的 `bae mcp` 里提供。

#### 5.5.10 现有 diff 和回滚的修改（G0，先于分支实现）

问题：`diff_layers` 和 `revert_ops` 先按路径匹配 Prim，再用 TreeID 认出移动。但镜像路径在并发同名新建后，会暂时在客户端之间不一致（后缀只在完整重建时才加），跨版本也不稳定。改法如下：

- 新增 `identity::NodeIndex`：对一个工程层的某个版本建立 `TreeID → { parent, 原始名, 兄弟序号, &PrimSpec }`。只用公开的镜像 API：`paths`、`node_id`、`node_path`、`spec`、`child_index`。
- 差异：
  - 两个版本都有的 TreeID：比较字段和属性。父节点或原始名不同时报告 `Moved { from }`，显示路径分别取自各自的版本。
  - 只在一个版本里的：`Added` 或 `Removed`。再用"重建别名"规则（5.5.5 第 3 条）把撤销删除后重建的 Prim 合成为 `Modified`。这保留了今天的显示效果，但靠的是身份，而不是路径恰好相同。
  - 只是后缀不同、实际是同一个节点的，不再报告为移动。
- 回滚：
  - 按 TreeID 决定删除、恢复和设字段。
  - 每个 Op 的路径在应用前从当前镜像查出。
  - 恢复的子树放到父节点（按 TreeID 找）当前的路径下。
  - 兄弟顺序仍然不恢复，与现在相同。
- `node_authors` 推广为按 `(TreeID, Element)` 记录作者。diff 的输出不变，合并复用这套数据。
- `view_at`、`diff`、`revert`、`ProjectVersion::current` 对分支的 Stage 原样可用。

#### 5.5.11 测试计划

- **协议**：
  - `@branch/<hex>/anim/shot010` 能往返解析。
  - 畸形的键被拒绝：ID 太短、缺工程层、`..`、大写十六进制。
  - `DocSet for Stage` 对分支键返回 `UnknownDoc`。
  - v2 的 `Hello` 里有 `@future/x` 时跳过这一项，其余照常。
  - 模拟 v1 对端时，永远收不到分支键；对端 `Hello` 表明协议 2 之后立即补发。
- **按需同步**：
  - 三个客户端经 `Relay`，从未打开分支的客户端在 10 个 hello 周期里收到 0 字节分支数据。
  - 先物化再打开的客户端只收到增量，比完整快照小。
  - Hub 重启后分支仍在（`services/bae-hub/tests/hub.rs`）。
  - `DirTransport` 在模拟 SMB 延迟下，不读没列出的分支，列出后能读到。
- **分支**：
  - 新分支的版本向量等于分叉版本，TreeID 相同，peer 不同。
  - 在分支上编辑不改变主线的版本向量和展平后的内容，反过来也一样。
  - 引用的操作还没到的命名版本返回 `VersionNotAvailable`。
  - 保存后 `sublayers/` 和 `project.json` 与之前逐字节相同；`branches/` 重新加载后与分支一致。
  - 切走再切回时，各线的撤销栈都还在。
  - 在分支上走一遍所有 Agent 工具和应用命令路径，断言主线的版本向量不变。
- **合并矩阵**（表驱动）：元素种类 × ours {未改, 改, 改成与 theirs 相同} × theirs {未改, 改} × prefer。断言结果状态、是否冲突，以及报告里的三方值。
- **结构**：
  - 新增子树：写链接，第二次合并不重复建。
  - 删除没改过的 Prim。
  - `DeletedByTheirs` 和 `DeletedByOurs`，各在两种 prefer 下。
  - `AddedUnderDeleted`、`Reparent`、`Cycle`。
  - `NameClash` 产生 `<name>_2`。
  - 普通的 `AddAdd` 不自动合并；撤销删除后重建的节点按别名匹配。
  - 只有分支重排时应用，两边都重排时报冲突。
- **身份**：
  - 两个客户端并发同名新建、后缀不同时，仍按 TreeID 匹配，diff 和回滚也一致。
  - 撤销再重做一次合并后再合并，不产生重复（第 4 条规则）。
  - 在事务和 `record_merge` 之间模拟崩溃（跳过写记录），下次合并能自行恢复。
- **撤销**：
  - `undo(合并者)` 后展平内容与合并前完全一致。
  - 只有一个撤销单元，带合并标签；其他作者的撤销栈不变。
  - 对端以 Remote 事件收到合并，撤销栈为空。
  - 注入失败时全部回滚。
  - 1000 秒内连续"合并、撤销、重做、撤销"，`merge_in_effect` 每一步都正确（需要 02 的 `origin`）；选基准时跳过已撤销的合并。
- **基准**：
  - 合并到父线后分支继续改，下次基准是上次的 theirs。
  - 先从父线更新、再合并到父线：已经带进分支的主线值不报冲突。
  - 记录先于数据到达时返回 `VersionNotAvailable`。
  - 并发的记录退回分叉点。
- **报告**：
  - 双方的作者和时间来自提交信息（每边两个作者，取最新的）。
  - `via_merge` 能从标签解析出来。
  - 记录在保存、重新加载、同步之后仍在。
  - 超过上限时改为 `by_ref`，改选仍然可用。
  - `expect` 不一致时返回 `InvalidParams`，什么都不写。
  - 预览报告与实际合并报告一致（`result` 和 `links` 除外）。
- **改选**：
  - 一次改选是一个撤销单元，撤销后恢复。
  - 之后有人改过时返回 `changed_since_merge`。
  - 结构类改选都能执行；全部改选合起来是一个撤销单元。
- **锁和权限**：
  - 锁定图层上的元素报告为 `Locked`，除非指定 `include_locked`。
  - 只有自己层写权限的 Agent 合并时，越权的工程层被跳过，不会只写了一部分。
- **随机属性测试**（固定种子，两个方向都做）：
  - 主线自基准以来没改：合并后主线的 specs 等于分支的（`to_trees` 相等，前提是没有 Structural）。
  - 分支没改：合并产生 0 个修改。
  - 撤销能完全恢复。
- **性能**（release 构建，默认忽略）：5k 个 Prim、作者窗口 5 万个操作时，Stage 线程上规划加应用少于 200 ms；`MergeInput` 在工作线程构建。
- **界面**：每个 collab 字符串都有 zh；对话框和冲突面板能渲染固定的报告样例。

#### 5.5.12 PR 拆分

| PR | 内容 | 规模（含测试） |
|---|---|---|
| G0 identity | ① `NodeIndex` 和原始名 ② diff 按 TreeID，加重建别名 ③ 回滚按 TreeID ④ 按元素记录作者 | ~400 |
| G1 协议 v2 与按需同步 | ① `DocKey::Branch`、`PROTOCOL_VERSION` 2、宽松解码 `Hello` ② 按对端协议过滤键 ③ `push_unlisted_branches`（`Relay` 和 Hub 存储连接），bae-hub 升级加测试 ④ `DirTransport` 遍历 `branches/`、按已列出的键过滤 ⑤ `branch_events`、`remote_version` | ~650 |
| G2 分支与线 | ① History 的 `branches` 记录、分支命名版本、`links`、`status` ② `Lines` 的新建（`fork_at`）、打开、物化、检出、关闭和访问器 ③ `ProjectDocs::with_lines` 路由 ④ 加载和保存 reader ⑤ presence 的 `line`、`merging`，`sync_stacks` | ~1.2k |
| G3 合并到父线 | ① 选基准和有效性判断（含过渡期的 `note_undo`） ② `MergeInput` 构建器和 reader ③ 规划器（身份、三方、结构、顺序、锁、Shadowed） ④ 应用器、`branch.merge`、`take`、`expect` ⑤ 在快照上预览、`record_merge` | ~2.2k |
| G4 从父线更新与改选 | ① `from_parent` 方向 ② `branch.resolve`、全部改选、标记已解决 ③ 冲突存储的上限和 `by_ref` ④ 未合并提示、后台"落后主线"预览 ⑤ `Duplicate` 的检测和清理 | ~1.1k |
| G5 界面 `collab/` | ① 标题栏分支标签和分支列表（新建、切换、放弃、恢复） ② 合并对话框（预览、prefer、逐条 take） ③ 冲突面板 ④ 属性角标和 Shadowed 挂钩 ⑤ zh 表和 i18n 测试 | ~1.4k |

- 顺序：G0 和 G1 并行，然后 G2、G3、G4。G5 在 G2 之后可以先用固定样例开工。
- 分两次发布：
  - **发布 1** = G0 到 G3，加上 G5 的①②，功能是合并回主线。
  - **发布 2** = G4，加上 G5 的③④。
- 不新增依赖：blake3、uuid、serde_json、loro 都已经在依赖树里。每个 PR 用 `cargo xtask size` 确认体积。
- 对其他工作流的请求：
  - **02**：`Stage::replay` 的提交信息加 `origin`，约 10 行，G3 需要；可选的 `Sublayer::from_doc` 和 `Sublayer::node_name(TreeID)`。
  - **11**：`Document` 持有 `History` 和 `Lines`，所有访问改走 `stage()` / `stage_mut()`，用保存和加载 reader，处理 `Switched` 时重新订阅，接入用 `with_lines` 的 `SyncSession`。约 300 行。
  - **12**：属性和时间线面板的角标挂钩，历史面板的"从此版本新建分支"菜单项。约 100 行。
  - **17**：`Workspace` 持有 `lines` 并全部改用访问器，实现上面的工具，禁止在共享工作区里检出。约 400 行。
  - **deploy/**：文档加一句"共享分支需要 Hub 支持协议 2"。

#### 5.5.13 不做的

- CRDT 层面的合并，即把分支操作直接导入主线文档。
- 改写历史：rebase、squash、cherry-pick 单个提交。
- 兄弟分支之间直接合并（要经过父线）。
- 合并层叠顺序、`project.json` 里的设置和路径映射（只报告）。
- 字符串内部的字符级合并、关键帧的语义重定时。值是原子的，关键帧按时间是原子的。
- `.baez` 或 bae-package 里包含分支；"把分支导出为新工程"。
- 分支权限、只读分支（属于 10 的鉴权）。
- 删除已放弃分支的数据（等 4.7 的日志清理）；分支的浅快照或增量快照存储。
- 按属性的自动解决策略；在冲突对话框里输入第三个值（可以合并后直接编辑属性，再"标记为已解决"）。
- 合并时尚未收到的并发编辑提示（子任务 5）；分支视图里的跨工程引用。

---

## 待确认的问题

1. **用户眼里的分支是什么。**
   - 本方案：分支是整个工程的另一条线。所有工程层一起分叉；在分支上可以删除、移动、改名、重排 Prim，也可以新建工程层；切换时整个界面换到那条线。
   - 另一种：主线之上一组可开关的覆盖层（USD 风格）。好处是主线的新修改会实时出现在分支里；代价是在分支上不能删、移、重排主线的 Prim，新图层总在最下面，而且要改 bae-model 的合成。
   - 推荐前者。
2. **协作者是否同步分支内容。**
   - 本方案：所有人都能看到分支列表和合并记录，内容只在打开时下载，Hub 保存全部内容。代价是没打开过的分支离线时看不到别人的修改。
   - 另一种：每个客户端都常驻所有活跃分支。离线随时能切换，但内存、磁盘、流量都要乘以分支数。
   - 推荐按需。
3. **首发是否包含"从主线更新"。** A/B 方案通常需要把主线上的修改（logo、字幕）带进分支。本方案两个方向都设计好了，但分两次发布，先做"合并回主线"。是否接受？
4. **冲突默认取哪边。**
   - 推荐默认保留目标线：合并到主线时主线胜，从主线更新时分支胜。对话框里可以一键切换，也可以逐条选；Agent 调用必须显式给 `prefer`。
   - 另一种直觉是"合并回来就以分支为准"，即默认 theirs。
5. **合并的撤销粒度。** 推荐一整次合并是一个撤销单元，合并后的每次改选各是一个。不支持"只撤销合并里的某个工程层或某些 Prim"；需要部分合并时，用 `prim` 或 `sublayer` 限定范围。
6. **向 02 提一行请求：撤销和重做的提交信息加 `origin`。** 它用来判断一次合并是否已被撤销。如果不接受，只能靠应用和 Agent 在每次撤销、重做时调用 `note_undo` 通知；漏掉一次，下一次合并就会悄悄跳过被撤销的那些修改。
7. **是否允许分支的分支。** 本方案允许，只和直接父线合并。如果想先简化界面，v1 可以只允许从主线分出，数据格式不受影响。
8. **数据保留。** v1 里放弃分支只是隐藏，永不删除。每个分支在磁盘上大约多一份主线文档，Hub 上也一样；真正删除要等 4.7 的日志清理。是否接受？
