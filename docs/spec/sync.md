# 协同同步与版本（bae-sync）

> 负责：09 `collab/sync`。代码：`crates/bae-sync`。架构依据：`docs/ARCHITECTURE.md` 第 4.6、7 节。
> 状态：第 1–3 节（协议、会话、Hub 中继）PR A 已合并；第 4 节（共享目录）PR B；第 5 节（命名版本、只读视图、按 Prim 差异）PR C 已合并；第 6 节（在线状态、建议性锁）PR D；Hub 的 WebSocket 传输（3.1）PR E；回滚（5.4）PR F。分支与合并在后续 PR。

## 1. 概念

| 类型 | 说明 |
|---|---|
| `DocKey` | 同步的单位：一个工程层（`SublayerId`），或工程历史 `@history`（命名版本等）。`@` 不会出现在工程层 ID 里，所以不会冲突。 |
| `Message` | 协议消息，所有传输方式通用（见第 2 节）。 |
| `Transport` | 把消息送到"对方"：共享目录、Hub，或另一个客户端。`send` / `recv` 都不能阻塞。 |
| `DocSet` | 会话读写的一组文档。`Stage` 实现了它（各工程层）；`MemDocs` 是 Hub 用的纯 Loro 文档集合。 |
| `SyncSession` | 把一个 `DocSet` 和一个远端保持同步。和传输方式无关。 |
| `Relay` | Hub 端逻辑（不含网络）：每个连上的客户端一个会话，共用一份 `MemDocs`。10 hub-nas 用 WebSocket 包一层即可。 |
| `ClientId` | 一个正在运行的客户端（应用实例、Agent 进程、Hub）。1–64 个 ASCII 字母、数字、`_`、`-`，可以直接当文件名。同时运行的会话必须用不同的 ID；应用建议每个安装固定一个（跨次启动继续写同一个日志），Agent 用自己的。 |

远端导入一律走 `Stage::import_updates`：发出 `origin = remote` 的 `StageEvent`，不进任何撤销栈。

## 2. 协议

### 2.1 版本向量的含义

每条带数据的消息都附带 `version`，意思是"发送方拥有这个版本之前的全部操作"。所以：

- 接收方如果已经拥有 `version` 里的全部操作，可以跳过这条数据不导入（省掉重建镜像的开销）；
- 收到之后，接收方知道"对方至少有 `version`"。

### 2.2 消息

| 消息 | 字段 | 含义 |
|---|---|---|
| `Hello` | `protocol`、`client`、`versions: [(DocKey, 版本向量)]` | "我有这些。" 连接时发，之后定期发。没列出的文档视为空。 |
| `Updates` | `doc`、`version`、`data` | 一段 Loro 增量；`version` 是发送方在导出这段增量时的版本。 |
| `SnapshotRequest` | `doc` | 请对方发一份完整快照（共享目录模式用它压缩自己的日志）。 |
| `Snapshot` | `doc`、`version`、`data` | 完整 Loro 快照（含历史）。导入方式与 `Updates` 相同。 |
| `Presence` | `client`、`seq`、`ttl_ms`、`state` | 在线状态（第 6 节），不进任何文档的历史。`state` 是 `PresenceState` 的 JSON，`None` 表示离开。 |

### 2.3 二进制编码

一条消息 = 1 字节类型 + 字段。字符串和字节串前面是 LEB128 长度；版本向量用 Loro 的 `VersionVector::encode()`，作为字节串写入。

```text
Hello            0x01  protocol:varint  client:str  n:varint  n × (doc:str  vv:bytes)
Updates          0x02  doc:str  vv:bytes  data:bytes
SnapshotRequest  0x03  doc:str
Snapshot         0x04  doc:str  vv:bytes  data:bytes
Presence         0x05  client:str  seq:varint  ttl_ms:varint  (0x00 | 0x01 state:bytes)
```

- 当前协议版本 `PROTOCOL_VERSION = 1`。以后只以兼容的方式扩展：新字段追加在已有字段之后（旧版本解码时忽略多出来的字节），新消息用新的类型字节（旧版本收到未知类型时跳过，`SyncError::UnknownMessage`）。
- Hub 模式一条消息就是一个 WebSocket 二进制帧；共享目录模式见第 4 节的记录格式。

### 2.4 会话规则（`SyncSession`）

会话为每个文档记住"远端已知拥有的版本"（`remote`）。每次 `pump`：

1. 第一次，或距离上次超过 `hello_interval`，或本地出现了新文档：发 `Hello`（列出本地所有文档的版本）。
2. 处理收到的消息（每次最多 `max_messages_per_pump` 条，避免卡住持有 Stage 的线程）：
   - `Hello`：用对方的版本**替换**（不是合并）`remote`。替换是为了对方丢了数据时（比如 Hub 重启）能重新补发；代价最多是重复发送，CRDT 导入是幂等的。
   - `Updates` / `Snapshot`：`remote[doc]` 并入 `version`；本地已经包含 `version` 就跳过，否则导入。本地没有这个文档时不导入，在 `PumpOutcome::unknown_docs` 里报告（见 2.5）。
   - `SnapshotRequest`：导出快照发回去。
3. 对每个本地文档，如果 `remote[doc]` 不包含本地版本，导出 `remote[doc]` 之后的增量发出去，并把本地版本并入 `remote[doc]`。距离上次发送不到 `min_send_interval` 时这一步推迟到之后的 `pump`（共享目录模式用几百毫秒，避免拖动时每秒几十次 SMB 追加；Hub 模式为 0）。

在收到对方第一个 `Hello` 之前不发任何数据（不知道对方有什么，全量发送太浪费）。

**丢包、重复、乱序、延迟**：重复和乱序由 CRDT 吸收（依赖缺失的操作 Loro 会挂起，等依赖到了再应用）。丢包由定期 `Hello` 修复：对方的 `Hello` 让我们知道它实际有什么，缺的会重新发。断线重连时调用 `SyncSession::reset()`。

### 2.5 别人新建的工程层

工程层的层叠顺序在 `project.json` 里，不在 CRDT 里，所以协作者新建的工程层不会自动出现在本地层叠中。会话收到本地没有的文档时只报告（`unknown_docs`），由应用决定插到层叠的什么位置。应用把它加进 Stage 后（`Stage::insert_sublayer`），会话下一次 `pump` 会因为"出现新文档"发 `Hello`，远端随即把内容补过来。

### 2.6 线程

`SyncSession::pump` 在持有 `Stage` 的线程上调用，不做阻塞 I/O。共享目录、网络的 I/O 由传输层放在后台线程里做，通过队列交给会话：`ThreadedTransport::spawn(内层传输, tick, on_receive)` 把任何会阻塞的传输搬到后台线程，收到消息后调用 `on_receive`（接到"请求重绘 / 尽快 pump"上）。

同一轮 `pump` 里同一个文档收到的多条数据合并成一批导入（`DocSet::import_batch`），`Stage` 只重建一次镜像、只发一个事件。

## 3. Hub 中继（`Relay`）

- `Relay` 持有一份 `MemDocs`（每个文档一个 Loro 文档，收到未知文档时自动创建），每个连接一个 `SyncSession`。一个客户端发来的增量导入后，其他客户端的会话在下一轮 `pump` 时发现本地版本变了，就把对方缺的部分发过去。
- `Relay::pump` 在同一次调用里最多循环 4 轮，让一个客户端的修改在一次调用内转发给其他人。
- Hub 用 `MemDocs::load` 恢复持久化的快照，用 `docs().export_snapshot` 保存。Hub 重启丢了数据也没关系：客户端重连后对方 `Hello` 里版本变小，客户端会把缺的重新上传。
- 传输出错的连接会被断开（`RelayOutcome::disconnected`）。

### 3.1 WebSocket 传输（PR E，feature `hub`）

- `HubTransport::connect(url, token, timeout)`：`url` 形如 `ws://nas.local:7341/projects/<工程>`，路径和查询参数怎么选工程由 Hub（10）决定；`token` 作为 `Authorization: Bearer <token>` 发送，Hub 拒绝时返回 `SyncError::Hub("... 401 ...")`。暂时只支持 `ws://`（局域网），`wss://` 报错。
- 一条协议消息就是一个 WebSocket **二进制帧**；文本帧、ping/pong 忽略（pong 自动回复）；解码失败或未知类型的帧跳过；收到 close 帧或连接断开返回 `SyncError::Closed`。
- 握手后套接字是非阻塞的，`send` 写不完的部分留在缓冲区，在之后的 `recv` 里继续发。应用里套一层 `ThreadedTransport`：`ThreadedTransport::spawn(HubTransport::connect(..)?, Duration::from_millis(2), 请求重绘)`。
- 断线后：重新 `connect`，换掉传输，调用 `SyncSession::reset()`；双方 `Hello` 之后自动补齐（Hub 丢了数据也能补回去）。
- `HubTransport::accept(tcp_stream, token)` 是服务端一侧，给测试和简单工具用：`tests/hub.rs` 里用 `Relay` + `accept` 写了一个几十行的阻塞式参考 Hub。
- 验证（`cargo test -p bae-sync --features hub`）：3 个客户端经真实 WebSocket（回环）随机编辑后收敛、在线状态可见；一次修改从一个客户端经 Hub 到另一个客户端的延迟中位数 2.9 ms（最大 5.3 ms）；错误或缺失的 token 被拒（401）；Hub 重启并丢失全部数据后，客户端发现断线、离线继续编辑、重连新 Hub 后收敛，新加入的客户端拿到完整工程。

## 4. 共享目录模式（PR B）

工程目录放在 NAS 的 SMB/NFS 共享上，不需要服务器。

### 4.1 目录

```text
MyFilm.bae/
  sublayers/<工程层>.loro            基础快照（Project::save 写，02 负责）
  updates/<文档>/<client-id>.log     每个客户端只追加写自己的日志
  updates/<client-id>.lock           会话期间持有的建议性文件锁（尽力而为）
  presence/<client-id>.presence      在线状态心跳文件（第 6 节）
  history.loro                       工程历史（第 5 节，History::save 写）
```

`<文档>` 是工程层 ID（`/` 变成子目录，`anim/shot010` → `updates/anim/shot010/`），或 `@history`。

### 4.2 日志格式

```text
log     := record*
record  := len:u32 LE   sum:8 字节   payload[len]
sum     := BLAKE3(payload) 的前 8 字节
```

- 第一条记录是文件头，payload 是 UTF-8 JSON：

  ```json
  {"format": "bae.sync.log", "version": 1, "doc": "anim/shot010", "client": "felix-mbp", "generation": "9f3c…"}
  ```

  `generation` 是随机值，日志每被重写（压缩）一次就换一个。
- 之后每条记录的 payload 是一条编码后的 `Updates` 或 `Snapshot` 消息（第 2.3 节），读者跳过其他类型。

### 4.3 写（只有日志的主人写）

- 追加：整条记录一次 `write` 写入，然后关闭文件（不逐条 `fsync`：关闭时数据交给 SMB 服务器；崩溃留下的半条记录下次启动时截掉；修改本身还在会话的文档里）。压缩重写的新文件先 `fsync` 再改名。写之前检查文件长度是否等于自己记的长度：更长且文件头没变，说明上次写了一半，先截掉；文件不见了或文件头变了，就重写整个文件。
- 启动时主人也像读别人的日志一样读一遍自己的日志（里面可能有还没存进 `sublayers/*.loro` 的修改），并截掉末尾写了一半的记录。文件头都读不出来的日志改名为 `<client-id>.corrupt-<随机>` 留着排查，下次写入时新建。
- **压缩**：日志超过阈值（默认 `max(1 MiB, 上次压缩结果 × 4)`）时，传输层向会话发 `SnapshotRequest`，拿到快照后写一个新文件（新 `generation`，文件头 + 一条 `Snapshot`），再改名覆盖旧日志。每个客户端只压缩自己的日志，所以多个客户端同时压缩不会互相破坏；目录总大小约为"客户端数 × 快照大小"，不随编辑次数增长。快照覆盖了旧日志的全部内容，因为会话的文档包含自己写过的一切。

### 4.4 读

- 每隔 `poll_interval`（默认 1 秒）轮询一次；开启 `watch` feature 时文件变化会提前唤醒轮询（SMB 的变更通知不可靠，轮询始终保留）。
- 每个日志记住 `(generation, 已读到的位置, 长度, 修改时间)`。长度和修改时间都没变就不打开文件；`generation` 变了就从头读；否则从上次的位置往后读。每 `full_check_every`（默认 30）次轮询无论如何都打开一次（时间戳粒度粗、SMB 属性缓存过期的情况）。
- 记录不完整（长度超出文件末尾）或校验和不对时停在这里，下次轮询再试（写入方可能还没写完，或者 SMB 缓存还没刷新）。
- `sublayers/<文档>.loro` 也是一个来源：长度或修改时间变了就重新读，作为 `Snapshot` 交给会话（会话按版本判断是否需要导入）。

### 4.5 和协议的对应

共享目录被当作一个"远端"：

- 第一次扫描完后，传输层发一个 `Hello`（`client` 为 `shared-folder`），版本 = 目录里所有记录和基础快照版本的并集。会话据此把目录里还没有的本地修改（比如离线时的修改）写进自己的日志。
- 会话发来的 `Hello`：如果某个文档目录里的版本没有被会话的版本包含，就从头重新交付这个文档的所有日志（用于 2.5 的场景），然后回一个自己的 `Hello`（会话 `reset()` 之后靠它恢复）。
- 共享目录会话的推荐设置：`hello_interval = None`（目录不会丢数据），`min_send_interval` 几百毫秒。
- 会话发来的 `Updates` → 追加到自己的日志；`Snapshot` → 压缩重写自己的日志。

### 4.6 验证（`crates/bae-sync/tests/shared_dir.rs`）

- 3 个客户端在同一个目录上随机编辑后收敛；之后从旧的已保存工程加入的新客户端追上进度。
- 重启后从自己的日志恢复没保存的修改；末尾半条记录被截掉，之后的追加别人能读。
- 一小时连续编辑（3600 次修改，大多是拖动同一个属性）：自己的日志最大 33 KB（压缩阈值 32 KB），压缩 29 次，`updates/` 总共 26 KB。
- 3 个客户端频繁同时压缩，不丢数据，新客户端读到的结果一致。
- 同一个 ClientId 不能同时开两个会话（文件锁）。
- **模拟 SMB 延迟**：每个客户端用自己的目录副本，后台线程把别人的日志延迟 1–3 秒复制过来，30% 的情况先给一个截断的前缀（模拟没传完的追加），覆盖写也不是原子的；客户端用 `ThreadedTransport`。默认测试把延迟缩小 10 倍（100–300 ms，停止编辑后 0.3 秒收敛）；`--ignored` 跑真实 1–3 秒（停止编辑后 3.0 秒收敛）。
- `watch` feature：轮询间隔设成 1 小时，文件事件仍能在 0.4 秒内唤醒读取。

### 4.7 还没做

- **清理离开的客户端的日志。** 安全的条件是：这个日志的内容被另一个"自包含"（以快照开头）的日志完全包含，并且主人长时间不活跃；为了避免两个客户端互相删对方，只能删"严格更小"的那个（版本相同时按 ID 排序）。在线状态的心跳文件做好之后再实现。
- 多台机器经 NAS 的实测。

## 5. 版本（PR C）

每个工程层的 Loro 文档都保留完整历史，所以"工程的一个版本"就是每个工程层一个版本向量（`ProjectVersion`，没列出的工程层在这个版本里是空的）。

### 5.1 历史文档

- 命名版本存在一个单独的 Loro 文档里（`History`，同步时的 `DocKey` 是 `@history`），和工程层一样同步：会话用 `ProjectDocs { stage, history }` 作为 `DocSet` 就会同时同步两者。
- 保存为 `.bae` 目录里的 `history.loro`（`History::save` / `History::load`，临时文件 + 改名）；应用在 `Project::save` 时一起保存。共享目录模式下它的日志在 `updates/@history/`，`history.loro` 也作为基础快照来源读取。
- 结构：根 map `checkpoints`，键是命名版本 ID（UUID v7 十六进制，按时间排序），值是 map：

  | 键 | 类型 | 说明 |
  |---|---|---|
  | `name` | string | 名字，例如"给客户的第 1 版" |
  | `note` | string | 说明 |
  | `author` | string | 创建者 `AuthorId` |
  | `created` | i64 | Unix 毫秒 |
  | `stack` | string | 当时的层叠顺序（强 → 弱），JSON 字符串数组 |
  | `versions` | map | 工程层 ID → 版本向量（`VersionVector::encode()` 的二进制） |

- 每次修改的 Loro 提交信息和 Stage 一样是 `{"author", "label"}`（`checkpoint.create` / `checkpoint.rename` / `checkpoint.delete`）。多人同时创建命名版本不会冲突（ID 不同）；同时改名后写者胜。删除命名版本不影响工程本身的历史。

### 5.2 只读视图

- `History::view(stage, id)` / `history::view_at(stage, version, stack)`：为每个工程层导出那个版本的快照（`ExportMode::SnapshotAt`），组装成一个新的 `Stage`，包在 `VersionView` 里，只给出 `&Stage`，所以不能修改。版本等于当前版本的工程层直接复用。
- 层叠顺序用命名版本里记录的；后来从工程里删掉的工程层显示不了，列在 `VersionView::missing()`。
- 协作者的命名版本已经同步过来、但它引用的操作还没到时，返回 `SyncError::VersionNotAvailable`。
- 跨工程引用在视图里不挂载（显示为未解析）。

### 5.3 按 Prim 的差异

`History::diff(stage, from, to)`（`to` 为 `None` 表示当前）/ `history::diff(stage, from, to)` 返回 `Vec<PrimDiff>`，按当前层叠从强到弱、同一层内按路径排序：

| 字段 | 说明 |
|---|---|
| `sublayer`、`path` | 哪一层的哪个 Prim（被删除的用旧路径） |
| `change` | `Added` / `Removed` / `Modified` / `Moved { from }`（移动或改名） |
| `attrs` | 意见（值、关键帧、表达式）不同的属性名 |
| `fields` | 其他不同的字段：`type`、`specifier`、`references` |
| `authors` | 两个版本之间改过这个 Prim 的作者 |
| `last_changed` | 其中最晚一次提交的时间（Unix 秒） |

- 两个版本里同一路径的 Prim 视为同一个；路径不同的靠 Loro 树节点 ID（`Sublayer::node_id` / `node_path`）认出移动和改名。
- 作者：遍历两个版本之间的所有 Loro 操作（两个方向都算，所以也能倒着比较），把每个操作对应到它改的树节点（树操作的目标节点，或 map 所在节点），作者取提交信息里的 `author`。
- 没有报告的：兄弟之间的顺序变化；撤销删除后恢复的 Prim 因为路径相同显示为修改。

### 5.4 回滚（PR F）

- `History::revert_to(stage, author, id, scope)` / `history::revert(stage, author, label, version, scope)`：把工程放回某个版本的样子，作为**一个新的撤销单元**（标签 `revert to <名字>`），不改写历史：协作者像收到普通修改一样收到回滚，作者可以撤销它，其他人的撤销栈不受影响。
- 范围 `RevertScope`：`Project`（当前层叠的所有工程层）、`Sublayer(id)`、`Prim(path)`（这个 Prim 及其子孙，所有工程层）。
- 做法：那时不存在的 Prim 删除（只删最上层，子树随之删除）；那时存在、现在没有的 Prim 连同整个子树恢复到原来的位置；两边都有的 Prim 把类型、specifier、引用和不同的属性意见设回去（`SetAttr`，那时没有的属性清除意见）。
- 不恢复的：两边都存在的兄弟 Prim 之间的顺序；已经从层叠里删掉的工程层。那个版本里还不存在的工程层会被清空（工程层本身保留，层叠变化不可撤销）。
- 验证：整个工程回到 v1 后内容和 v1 的只读视图一致（包括子 Prim 顺序），v1 到当前的差异为空，回滚的修改都记在执行者名下，旧的版本向量仍被包含（历史没改写）；一次撤销恢复 v2 的内容，另一个作者的撤销栈不变；只回滚一个 Prim、只回滚一个工程层。

### 5.5 还没做

- 分支与合并。
- 命名版本和回滚的命令（让 Agent 通过 CommandRegistry 调用）：`Command` 只能拿到 `Transaction`，碰不到 `History`，需要和 02/17 商量接口。

## 6. 在线状态与建议性锁（PR D）

### 6.1 内容

`PresenceState`（JSON，新版本加的字段原样保留在 `extra` 里转发）：

| 字段 | 说明 |
|---|---|
| `user` | `AuthorId` |
| `name`、`color` | 显示名、颜色（sRGB 三个字节） |
| `selection` | 选中的 Prim 路径 |
| `playhead` | `{comp, time}`：哪个合成、播放头时间（flicks） |
| `workspace` | 正在用的工作区，例如 `motion`、`storyboard`、`edit`、`review` |
| `editing` | **建议性锁**：声明"我在改这些 Prim"（通常是合成）。只提示，不阻止任何人修改。 |

### 6.2 规则

- `SyncSession::set_presence(state)`：变化时下一次 `pump` 发出，之后每 `presence_heartbeat`（默认 5 秒）发一次心跳。`clear_presence()`：发"离开"。
- 每条 `Presence` 带递增的 `seq`，接收方只保留每个客户端最新的一条，旧的、重放的、JSON 解析不了的都忽略。`seq` 从会话开始时的毫秒时间算起，所以重启后的客户端不会被旧记录压住。
- **在线判断只用接收方自己的时钟**：超过发送方声明的 `ttl_ms`（默认 20 秒）没有收到新的 `seq` 就视为离线。不同机器的时钟不用对齐。
- `SyncSession::presence()` 列出其他在线的人（不含自己）；`editing_by_others(path)` 列出声明正在改 `path`、它的祖先或后代的人（界面据此提示"Alice 正在改这个合成"）；`PumpOutcome::presence_changed` 表示有人上线、离线或状态变了。
- **Hub**：`Relay` 把每条被接受的在线状态（包括只刷新存活的心跳）转发给其他所有连接；新连接一上来就收到当前所有人的状态；连接断开（`disconnect` 或传输出错）时替那个客户端向其他人发"离开"。
- **共享目录**：每个客户端用临时文件 + 改名重写自己的 `presence/<client-id>.presence`（一条第 4.2 节格式的记录，内容是编码后的 `Presence` 消息），离开时写一条"离开"记录。读者随轮询读取变了的文件。文件修改时间早于两倍 TTL 的不予理会（崩溃的客户端留下的）；超过一天没动的文件任何人都可以删除。这一步要用文件修改时间对比本地时钟，所以 SMB 服务器和客户端的时钟差不要超过两倍 TTL（默认 40 秒）。

### 6.3 验证（`crates/bae-sync/tests/presence.rs`）

- 经 Hub：选中、播放头、工作区、建议性锁都能传到其他人；自己看不到自己；状态不变时不重复报告变化；新加入的人立即看到所有人；心跳维持在线远超 TTL，停止心跳后按 TTL 离线；主动离开立即生效；连接断开时其他人立即收到离开。
- 重放的旧心跳不能改写或复活别人；坏 JSON 被忽略。
- 经共享目录：心跳文件、TTL 离线、主动离开、一小时前的残留文件被忽略（把修改时间改成现在后又能看到，证明是按时间过滤的）。

## 7. 给其他工作流

- **02 doc-model**：需求记在 `docs/workstreams/02-doc-model.md` 的"请求"一节。
- **10 hub-nas**：服务端用 `Relay` + 每个 WebSocket 连接一个 `Transport` 实现即可；消息编码见 2.3，客户端的约定见 3.1。
- **11/12 界面**：`PumpOutcome::events` 已经通过 `Stage::subscribe` 广播，界面照常刷新；`SyncSession::unsynced` 可用于"有未同步的修改"提示；`unknown_docs` 用于提示"协作者新建了工程层"；`presence()` / `editing_by_others()` / `presence_changed` 用于在线头像、别人的选中框和播放头、"有人正在改这个合成"的提示。
