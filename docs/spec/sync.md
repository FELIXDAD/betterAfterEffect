# 协同同步与版本（bae-sync）

> 负责：09 `collab/sync`。代码：`crates/bae-sync`。架构依据：`docs/ARCHITECTURE.md` 第 4.6、7 节。
> 状态：第 1–3 节（协议、会话、Hub 中继）PR A 已合并；第 4 节（共享目录）PR B；第 5 节（命名版本、只读视图、按 Prim 差异）PR C 已合并；第 6 节（在线状态、建议性锁）PR D；Hub 的 WebSocket 传输（3.1）PR E；回滚（5.4）PR F；传输出错时的处理（2.4）、按连接设置会话（第 3 节）、WebSocket 的大小上限、分帧、写缓冲上限和会话层的背压（2.4、3.1）、清理离开的客户端的日志（4.6）在 `collab/hub-reliability`。分支与合并在后续 PR。

## 1. 概念

| 类型 | 说明 |
|---|---|
| `DocKey` | 同步的单位：一个工程层（`SublayerId`），或工程历史 `@history`（命名版本等）。`@` 不会出现在工程层 ID 里，所以不会冲突。 |
| `Message` | 协议消息，所有传输方式通用（见第 2 节）。 |
| `Transport` | 把消息送到"对方"：共享目录、Hub，或另一个客户端。`send` / `recv` 都不能阻塞。`backlog()` 是已经交给它、还没离开本进程的字节数（会话据此限速，见 2.4），`max_message_size()` 是一条消息的上限（没有上限时为 `None`）。 |
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
- Hub 模式一条消息就是一个 WebSocket 二进制消息，发送时切成最多 256 KiB 的帧（3.1）；共享目录模式见第 4 节的记录格式。

### 2.4 会话规则（`SyncSession`）

会话为每个文档记住"远端已知拥有的版本"（`remote`）。每次 `pump`：

1. 第一次，或距离上次超过 `hello_interval`，或本地出现了新文档：发 `Hello`（列出本地所有文档的版本）。
2. 处理收到的消息（每次最多 `max_messages_per_pump` 条，避免卡住持有 Stage 的线程）：
   - `Hello`：用对方的版本**替换**（不是合并）`remote`。替换是为了对方缺数据时能重新补发：丢了、被拒了，或者对方当时还没有这个文档、收到后丢掉了（2.5）。代价最多是重复发送，CRDT 导入是幂等的。
     - 例外：**在途的数据**。发出去、还没被对方的 `Hello` 确认的数据，按文档记下（版本、最早一次没确认的发送时间），替换后再并回 `remote`。对方在收到之前发的 `Hello` 当然没有它；大消息在慢链路上要传很多个 hello 间隔，不这样做的话，每个 `Hello` 都会让会话再导出、再发一遍。
     - 记录在三种情况下删掉，之后照常替换：对方的 `Hello` 已经包含它（确认了）；这个文档是对方刚加上的（上一个 `Hello` 里没有，之前发的可能被当成未知文档丢了）；数据离开本进程已经超过两个 `hello_interval`（没设时按 10 秒），这时还没有就是丢了。"离开本进程"从发送时间和最后一次看到传输有积压（`backlog() > 0`）的时间中较晚的那个算起。
     - 代价：会丢包的传输（只有测试里的模拟网络）上，丢了的数据最多晚两个 hello 间隔补发。
   - `Updates` / `Snapshot`：`remote[doc]` 并入 `version`；本地已经包含 `version` 就跳过，否则导入。本地没有这个文档时不导入，在 `PumpOutcome::unknown_docs` 里报告（见 2.5）。
   - `SnapshotRequest`：导出快照发回去（也记为在途）。
3. 对每个本地文档，如果 `remote[doc]` 不包含本地版本，导出 `remote[doc]` 之后的增量发出去，并把本地版本并入 `remote[doc]`。距离上次发送不到 `min_send_interval` 时这一步推迟到之后的 `pump`（共享目录模式用几百毫秒，避免拖动时每秒几十次 SMB 追加；Hub 模式为 0）。
   - **背压**：一个文档一个文档地导出、发送。每个文档之前先看传输的 `backlog()`，超过 `SessionConfig::max_backlog`（默认 1 MiB，`SessionConfig::DEFAULT_MAX_BACKLOG`）就停下，剩下的文档等之后的 `pump`。所以慢链路上一次只排一条大消息，没发的还在文档里，不占内存。新客户端第一次同步很大的工程、Hub 丢了数据后客户端重新上传，都一点一点发完，不会把写缓冲（3.1）撑满、断开、重连后再来一遍。`Hello`、在线状态和快照回复不受背压限制（都很小，快照只有共享目录会要）。
   - **一条消息放不下**：`send` 返回 `SyncError::MessageTooLarge` 时（Hub 模式超过 `HubLimits::max_message_size`）只跳过这个文档：错误放进 `PumpOutcome::errors`，其他文档照发，链路不断。这个文档在 `remote[doc]` 变化之前（比如重连之后）不再导出，免得每次 `pump` 都导出几百 MiB。

在收到对方第一个 `Hello` 之前不发任何数据（不知道对方有什么，全量发送太浪费）。

**丢包、重复、乱序、延迟**：重复和乱序由 CRDT 吸收（依赖缺失的操作 Loro 会挂起，等依赖到了再应用）。丢包由定期 `Hello` 修复：对方的 `Hello` 让我们知道它实际有什么，缺的会重新发。断线重连时调用 `SyncSession::reset()`。

**传输出错**：`pump` 返回错误，表示链路断了，要重连（换传输，再 `reset()`）。

- 先导入，再报错：出错之前这一轮已经收到的数据照样导入，然后才返回错误。对方认为这些数据已经送到，重连之前不会重发。不导入的话，客户端发完最后几条修改马上断开时，Hub 就丢了这些修改。
- 这一轮的结果不丢：出错之前做过的事（`events`、`unknown_docs`、`errors`、收发计数、`presence_changed`）留在会话里，下一次 `pump` 的结果里一起报告。想马上拿到（比如重连之前先让渲染缓存失效），调用 `take_interrupted()`。每一轮只报告一次；`reset()` 不会清掉它。
- 出错那一轮收到的在线状态不交给 `Relay` 转发：Relay 会断开这个连接，并替它向其他人发"离开"。
- `PumpOutcome::errors` 只放单个文档的问题，链路还能用：导入失败的数据（数据损坏、模型错误）、导出失败（回复快照或发送增量时）、一条消息放不下（`MessageTooLarge`，见上面第 3 步）。未知类型的消息由传输层直接跳过，不会出现在这里。

### 2.5 别人新建的工程层

工程层的层叠顺序在 `project.json` 里，不在 CRDT 里，所以协作者新建的工程层不会自动出现在本地层叠中。会话收到本地没有的文档时只报告（`unknown_docs`），由应用决定插到层叠的什么位置。应用把它加进 Stage 后（`Stage::insert_sublayer`），会话下一次 `pump` 会因为"出现新文档"发 `Hello`，远端随即把内容补过来。

### 2.6 线程

`SyncSession::pump` 在持有 `Stage` 的线程上调用，不做阻塞 I/O。共享目录、网络的 I/O 由传输层放在后台线程里做，通过队列交给会话：`ThreadedTransport::spawn(内层传输, tick, on_receive)` 把任何会阻塞的传输搬到后台线程，收到消息后调用 `on_receive`（接到"请求重绘 / 尽快 pump"上）。

同一轮 `pump` 里同一个文档收到的多条数据合并成一批导入（`DocSet::import_batch`），`Stage` 只重建一次镜像、只发一个事件。

## 3. Hub 中继（`Relay`）

- `Relay` 持有一份 `MemDocs`（每个文档一个 Loro 文档，收到未知文档时自动创建），每个连接一个 `SyncSession`。一个客户端发来的增量导入后，其他客户端的会话在下一轮 `pump` 时发现本地版本变了，就把对方缺的部分发过去。
- `Relay::pump` 在同一次调用里最多循环 4 轮，让一个客户端的修改在一次调用内转发给其他人。
- Hub 用 `MemDocs::load` 恢复持久化的快照，用 `docs().export_snapshot` 保存。Hub 重启丢了数据也没关系：客户端重连后对方 `Hello` 里版本变小，客户端会把缺的重新上传（一个文档一个文档地传，见 2.4 的背压；单个文档超过消息上限的除外，见 3.1）。
- 传输出错的连接会被断开（`RelayOutcome::disconnected`）。它出错之前发来的数据已经导入（见 2.4），同一次 `pump` 里就转发给其他人。
- **按连接设置会话**：
  - `connect(transport)` 用默认设置：Relay 的 ID 和 `hello_interval`。`session_config()` 返回这份设置。
  - `connect_with(transport, config)` 给一个连接单独的设置。从 `session_config()` 改起；`config.client` 保持 Relay 的 ID，对端在 `Hello` 里看到的就是它。
  - 例子：Hub 存盘用的对端（工程目录的 `DirTransport`，放在 `ThreadedTransport` 里）`min_send_interval` 设成 250 ms，客户端拖动一个值时少写几次盘；`max_backlog` 设成 `usize::MAX`，不受背压（2.4）限制。背压是给慢链路用的，存盘的队列只在进程内存里，本机磁盘几毫秒就写完；限制它只会让修改晚写盘，关闭工程时还压着的修改就丢了。
  - `peer_config_mut(handle)` 改已连接的对端的设置，下一次 `pump` 生效。Hub 关闭（或卸载）工程前用它去掉存盘对端的节流和背压，然后 `pump` 到 `unsynced(handle)` 为空为止（最多 8 次）：所有修改都交给了存盘对端。丢掉 Relay 时 `ThreadedTransport` 把队列里的全部写完才结束。
  - `unsynced(handle)`：这个对端还没收到的文档（它的会话的 `SyncSession::unsynced`；它还没发 `Hello` 时是全部文档），对端不在时返回 `None`。

### 3.1 WebSocket 传输（PR E，feature `hub`）

- `HubTransport::connect(url, token, timeout)`：`url` 形如 `ws://nas.local:7341/projects/<工程>`，路径和查询参数怎么选工程由 Hub（10）决定；`token` 作为 `Authorization: Bearer <token>` 发送，Hub 拒绝时返回 `SyncError::Hub("... 401 ...")`。暂时只支持 `ws://`（局域网），`wss://` 报错。
- 一条协议消息就是一个 WebSocket **二进制消息**；文本帧、ping/pong 忽略（pong 自动回复）；解码失败或未知类型的消息跳过；收到 close 帧或连接断开返回 `SyncError::Closed`。
- **发送队列和分帧**：
  - 握手后套接字是非阻塞的。`send` 把编码好的消息放进 `HubTransport` 自己的队列（大消息的数据不复制，直接引用导出的那块内存），然后套接字收多少就交出去多少；`send` 和之后每次 `recv` 都继续写。`backlog()` 是队列里还没交给套接字的字节数。
  - 一条消息切成最多 256 KiB 的帧（第一帧 binary，后面 continuation），tungstenite 写完上一帧才拿到下一帧。原因：tungstenite 把一帧整个复制进自己的写缓冲，这块缓冲只增不减，每写出一部分还要把剩下的整体前移。一条 200 MiB 的消息作为一帧发时，这 200 MiB 在连接存续期间一直占着，慢的对端还会让 Hub 的工程线程每一轮都搬动几百 MiB。分帧后它的写缓冲最多一帧。
  - 应用里套一层 `ThreadedTransport`：`ThreadedTransport::spawn(HubTransport::connect(..)?, Duration::from_millis(2), 请求重绘)`。它的 `backlog()` 是排给后台线程的加上 `HubTransport` 自己的，会话的背压看得到。
- 断线后：重新 `connect`，换掉传输，调用 `SyncSession::reset()`；双方 `Hello` 之后自动补齐（Hub 丢了数据也能补回去，一个文档一个文档地传）。`HubTransport` 被丢弃时，队列里没发出去的也丢掉，重连后的 `Hello` 会让它们重新发送。
- 写出错（比如对方已经断开）时，`recv` 先把对方断开前发来、已经收到的消息读完，再报告错误。
- `HubTransport::accept(tcp_stream, token)` 是服务端一侧，给测试和简单工具用：`tests/hub.rs` 里用 `Relay` + `accept` 写了一个几十行的阻塞式参考 Hub。
- **消息大小上限**（`HubLimits`）：
  - `connect_with(url, token, timeout, limits)`、`accept_with(stream, token, limits)` 传入上限；`connect` / `accept` 用 `HubLimits::default()`。`HubTransport::limits()` 返回一个连接用的上限。
  - `max_message_size`：一条消息（编码后的 `Message`）最多多少字节，默认 256 MiB（`HubLimits::DEFAULT_MAX_MESSAGE_SIZE`）。`HubLimits::with_max_message_size(n)` 给出 n 和对应的默认写缓冲上限（Hub 的 `max_message_mib` 用它）。
  - 接收时单帧上限和消息上限相同。自己发的帧只有 256 KiB，但更早的版本（`HubLimits` 之前，用 tungstenite 的默认设置）一条消息只发一帧、收的消息最大 64 MiB：旧客户端发来、或者旧 Hub 转发来的一条 20 MiB 的修改就是一个 20 MiB 的帧，照样收得下。（单帧上限曾经是 16 MiB，那时新客户端在帧头就拒收这样的帧，断开重连后又是同一帧，一直卡住。）tungstenite 收到帧头就按帧长预留内存，帧长先和上限比过，所以预留的也不超过消息上限。超过上限的帧在帧头就拒收，分成多帧的消息在累计超过上限时拒收。
  - **两端必须一致。** 对方收不下的消息会让连接断开，重连后又发同一条，这个客户端会一直断开重连，同步基本停住。客户端和 Hub（`hub.toml` 的 `max_message_mib`）默认都是 256 MiB。
  - 发送前先查自己的上限：超过就不发，`send` 返回 `SyncError::MessageTooLarge { size, max }`，什么都没入队，链路照常可用；会话跳过这个文档（2.4）。`ThreadedTransport` 在 `send` 里马上做同样的检查（上限来自内层传输的 `max_message_size()`）。收到超过上限的消息时，`recv` 返回 `SyncError::Hub("received a message of at least … bytes, over the limit of … on this side (the sender allows larger messages)")`，要断开重连。发送方的上限更大：`HubLimits` 不一致、对方是旧版本，或者是 Hub 上的只读连接。
  - `set_receive_limit(n)`：之后只收不超过 n 字节的消息（单帧上限也是 n），发送的上限不变。Hub 给只读连接用（16 MiB）：它们的修改反正会被丢掉，不必让 Hub 缓冲 256 MiB 的消息。
  - 还没做：一个文档要发的增量超过上限时（比如新客户端第一次同步一个超过 256 MiB 的工程层），这个文档同步不了，只报 `MessageTooLarge`。要等协议支持分块发送。
- **写缓冲上限**（`HubLimits::max_write_buffer_size`）：
  - 限制 `backlog()`：放不下新消息时 `send` 返回 `SyncError::Hub("write buffer full: the peer is not reading …")`，这条消息不入队，链路要断开重连。Relay 随即断开这个连接（`RelayOutcome::disconnected`）。
  - 会话正常情况下碰不到它：有背压，队列里最多约 1 MiB 加一条消息，再加排在后面的小消息（`Hello`、在线状态）。不读数据的对端不会让内存一直涨，Relay 只是先不给它发修改；它恢复读以后会补齐。只有它长期不读、小消息越积越多时才会到上限。
  - 默认是 `max_message_size` + 16 MiB（`HubLimits::WRITE_BUFFER_MARGIN`）。下限 `min_write_buffer_size()` 是一条最大的消息 + 2 MiB（会话开始发它时队列里可能还有 1 MiB，后面还会排小消息）；设得更小时自动提到下限，实际生效的值是 `write_buffer_cap()`。
- **内存**（每个连接），发送和接收分开算：
  - 发送：`HubTransport` 的队列最多约 1 MiB 加一条消息（会话在积压不超过 `max_backlog` 时才开始下一个文档，见 2.4），后面可能还排着几条小消息。大消息的数据就是导出的那块内存，不另外复制。客户端外面的 `ThreadedTransport` 交给后台线程的部分也算在 `backlog()` 里。tungstenite 的写缓冲最多一帧（256 KiB）。
  - 接收：收一条大消息时最多临时占它大小的两倍左右，收完就释放（之后导入文档另算）。新版本发来的消息是一串 256 KiB 的帧，tungstenite 把它们拼成一整块（按倍数扩容，扩容时新旧两块可能同时存在）；解码直接用这一块（`Message::decode_owned`），不再复制数据。旧版本发来的单帧消息先整个读进 tungstenite 的读缓冲，解码时复制一份，也是两倍。
  - tungstenite 的读缓冲平时最多一帧加 128 KiB。它不会缩小：收过旧版本的一个大的单帧消息后，这个连接存续期间一直占着那么大。
  - 合起来，一个连接最坏约"一条消息 + 1 MiB"（发）加"两条消息"（收）。Hub 上 N 个客户端同时下载同一个大工程层时，最坏是 N 份；没有全 Hub 的总预算。
- **收发计数**：`bytes_received()` / `bytes_sent()` 是这个连接收发过的字节数（含握手）。大消息在慢链路上要传很久，期间没有完整的消息；Hub 用收到的字节判断连接还活着（`services/bae-hub` 的 `PeerLink`）。
- 验证（`cargo test -p bae-sync --features hub`）：3 个客户端经真实 WebSocket（回环）随机编辑后收敛、在线状态可见；一次修改从一个客户端经 Hub 到另一个客户端的延迟中位数 2.9 ms（最大 5.3 ms）；错误或缺失的 token 被拒（401）；Hub 重启并丢失全部数据后，客户端发现断线、离线继续编辑、重连新 Hub 后收敛，新加入的客户端拿到完整工程。
- 上限和背压的验证（`tests/hub_limits.rs`，回环上的真实 WebSocket）：
  - 20 MiB 的快照两个方向都能完整传过去（用 tungstenite 的默认设置时失败，对方报 16 MiB 帧上限）。
  - 和旧版本互通：用 tungstenite 默认设置的旧 Hub 把 20 MiB 的快照作为一个帧发给新客户端，旧客户端同样发给新 Hub，都能完整收到（单帧上限是 16 MiB 时，新的一方在帧头就拒收）。
  - 超过上限的消息：发送方不发，报 `MessageTooLarge`，同一个连接之后照常收发；上限不一致时，接收方报错。
  - 直接调 `send`、对方不读：队列满时 `send` 失败（测试里上限约 2 MiB，发了约 6 MiB 后失败，其余在系统的套接字缓冲里）。
  - 有会话时对方不读：编辑方发了 24 MiB 的修改（128 个文档），Relay 给不读的那一方排队的最多约 1 MiB（测试要求不超过 1.5 MiB），不断开它；它恢复读以后补齐。去掉背压时它被断开。
  - 第一次同步 16 MiB（40 个文档，写缓冲上限 2.5 MiB）：上传到空的 Relay、再下载到新客户端，都不断开，每个文档只发一次。去掉背压时上传方断开。
  - 一个文档超过消息上限：其他文档照常到达，错误只报告一次，不会每次 `pump` 都重新导出。
- 慢链路模拟（`tests/backpressure.rs`，每个 tick 发 256 KiB，最后一个字节离开后 5 个 tick 到达）：
  - 一条 8 MiB 的修改要传 32 个 tick，期间接收方每 5 个 tick 发一次 `Hello`，都说没有：只发一次。去掉在途记录时发两次，原来的代码每个 `Hello` 发一次。
  - 12 个 1 MiB 的文档第一次同步：队列最多一个文档，每个只发一次。去掉背压时队列一下子到 12 MiB。

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
- 启动时主人也像读别人的日志一样读一遍自己的日志（里面可能有还没存进 `sublayers/*.loro` 的修改），并截掉末尾写了一半的记录。文件头损坏的日志改名为 `<client-id>.corrupt-<随机>` 留着排查，下次写入时新建（它里面没有谁能读出的记录）。更新版本的格式写的日志（文件头完好、`version` 更大）不改名、不覆盖：读的时候记一次错误，往里写时报错（`SyncError::Io`）。新版本的客户端读得懂它，里面可能有别人没有的操作。
- 先读再写：要写的日志如果还没被轮询接管（列目录时漏了，或者读的时候出错），追加或压缩之前先读一遍、接管它，不会直接用新文件把它盖掉。清理（4.6）之后，它可能是某些操作的唯一一份。
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

### 4.6 清理离开的客户端的日志

每个客户端只压缩自己的日志。不再使用的客户端（换了电脑、删掉的 Agent）留下的日志没人压缩，会一直留在目录里。`DirTransport` 在轮询时顺便删掉**别人的**、满足下面三个条件的日志 L：

1. **内容被包含**：同一个文档下有另一个"自包含"的日志 C（第一条记录是完整快照，即压缩或修复后的日志），C 的快照包含 L 可能有的全部操作。
   - L 的版本取上界：每条记录的 `version`，加上记录里 Loro 数据实际的结束版本，全部并起来。
   - C 的版本取下界：只算第一条快照里实际有的操作（看 Loro 数据本身，不看 `version` 字段）。后面的增量不算：增量只含写的人认为目录里还没有的操作，`version` 说明不了这个文件里有什么。
   - L 必须能完整读出：每个字节都属于校验通过的记录，每条记录都能解码、都属于这个文档、Loro 数据都能读。末尾有半条记录、有读不懂的新类型记录、读的过程中文件变了，都不删，下一轮再看。C 只要求文件头和第一条快照完好。
   - 读日志用和轮询同一套代码（`read_header`、`parse_records`、`Message::decode`）。
2. **主人长时间不活跃**：L 的修改时间、主人的在线状态文件 `presence/<client-id>.presence` 的修改时间，都早于 `DirConfig::log_gc_after`（默认 7 天）。没有在线状态文件也算早。
   - "现在"也用共享目录的时钟，不用本机时钟：每轮清理先往自己的锁文件 `updates/<client-id>.lock` 写一行，读它的修改时间（文件服务器给的）。本机时钟快了、NAS 时钟慢了都不会让日志显得旧。写不了或读不到时间时这一轮不删。
   - 读不到的修改时间、比"现在"还晚的修改时间，都算"刚改过"。服务器缓存的属性偏旧时，"现在"也偏旧，只会让日志多留一阵。
   - 在线状态文件超过一天没动就会被删（第 6.2 节），所以实际上就是：主人至少一天不在线，日志 7 天没写。
3. **严格更小**：按"先比版本，版本相同时 ID 小的在前"排序，L 必须严格排在 C 前面。也就是 C 的下界包含 L 的上界，并且两者不相等；相等时只删 ID 大的那个。
   - 这样两个客户端不会同时删掉对方的日志。L 因为 C 被删、C 又因为 L 被删，需要 `下界(C) ⊇ 上界(L)` 和 `下界(L) ⊇ 上界(C)` 同时成立。每个日志都是下界 ⊆ 上界，所以四个版本只能全相等，这时只有 ID 大的那个能删。L 和 C 都很久没人动，所有客户端读到的内容一样，结论也一样。
   - 一轮里用来当 C 的日志这一轮不删，这一轮删掉的日志也不再当 C。所以删掉的内容总在一个还留着的日志里。别的客户端也只会为了严格更大的日志删 C，顺着"因为谁被删"往下找不会绕回来（严格的顺序没有环），最后停在一个谁也删不了的日志上，它包含前面所有的内容。

其他规则：

- 自己的日志永远不删。只删 `updates/<文档>/<client-id>.log`，每个文档只和同一文档的日志比较。不碰 `sublayers/`、`project.json`、`history.loro`、锁文件和在线状态文件（在线状态文件照旧按第 6.2 节清理）。
- **频率**：打开后的第二次轮询做一次（第一次轮询要尽快发出 `Hello`），之后每小时最多一次，都在轮询里做，也就是在 `ThreadedTransport` 的后台线程上，不卡界面。先只看修改时间：没有候选的文档一个文件都不读。删之前再确认一次：文件的长度和修改时间和读的时候一样，主人仍然不活跃。
- **不重复读**：一个文档的日志全部读过、什么都没删时（比如 L 末尾有半条记录，或者有别人快照里没有的操作），记住每个日志的长度、修改时间和哪些是候选。下一轮这些都没变就不再读这个文档的日志，否则每小时都要把它们整个读一遍、解码一遍，挡住后台线程上排队的写入。读过的日志个数记在 `DirStats::logs_scanned`。
- 出错（读不了、删不掉）记到 `last_error`，跳过，不影响轮询。删掉的个数记在 `DirStats::logs_removed`。
- `DirTransport::remove_departed_logs()` 立即做一轮（工具和测试用）。`log_gc_after = None` 时什么都不删。
- **主人回来了**：
  - 还在运行的主人：每次轮询列目录时发现自己的日志不见了（而且确实不存在，不是列目录出错），就记一次修复（`DirStats::repairs`），向会话要一份快照，写成新的自包含日志。所以即使它不再修改，目录里也马上重新有它的全部内容；万一它最后一次追加正好和删除撞上（写进了已经删掉的文件），这一步也把它补回来。快照到之前就有修改要写时，照第 4.3 节的规则新建日志写这条修改，再要快照；追加时打开文件发现它刚被删掉，也这样处理，不当成传输错误。
  - C 的主人不会把 C 盖掉：往自己的日志写之前，没接管的日志先读再写（4.3），所以一次读失败不会让它用只有新记录的文件换掉 C，C 里替 L 保存的操作不会丢。更新版本格式写的 C 不改名、不覆盖。
  - 重新启动的主人：和新客户端一样。以前的修改在 C 里，第一次轮询就读回来；之后的修改写进新日志。
  - 一直开着、但既不修改也不发在线状态的客户端（比如没设置在线状态的 Agent），日志可能每 7 天被删一次、再重写一次，不丢数据。

### 4.7 验证（`crates/bae-sync/tests/shared_dir.rs`）

- 3 个客户端在同一个目录上随机编辑后收敛；之后从旧的已保存工程加入的新客户端追上进度。
- 重启后从自己的日志恢复没保存的修改；末尾半条记录被截掉，之后的追加别人能读。
- 一小时连续编辑（3600 次修改，大多是拖动同一个属性）：自己的日志最大 33 KB（压缩阈值 32 KB），压缩 29 次，`updates/` 总共 26 KB。
- 3 个客户端频繁同时压缩，不丢数据，新客户端读到的结果一致。
- 同一个 ClientId 不能同时开两个会话（文件锁）。
- **模拟 SMB 延迟**：每个客户端用自己的目录副本，后台线程把别人的日志延迟 1–3 秒复制过来，30% 的情况先给一个截断的前缀（模拟没传完的追加），覆盖写也不是原子的；客户端用 `ThreadedTransport`。默认测试把延迟缩小 10 倍（100–300 ms，停止编辑后 0.3 秒收敛）；`--ignored` 跑真实 1–3 秒（停止编辑后 3.0 秒收敛）。
- `watch` feature：轮询间隔设成 1 小时，文件事件仍能在 0.4 秒内唤醒读取。
- 清理离开的客户端的日志（4.6，测试用 `File::set_modified` 把文件改成 30 天前，不用等）：
  - 离开的客户端的日志被别人的快照包含：新加入的客户端第二次轮询就删掉它，别人的日志、在线状态文件、基础快照都不动；剩下的客户端照常收敛；之后加入的新客户端拿到完整工程（离开的人的修改来自别人的快照）。
  - 有别人快照里没有的操作：保留。别人读到它之后追加的记录 `version` 里有它，也不算包含；对方重新压缩之后才删。
  - 主人的在线状态文件是新的：保留；在线状态文件也变旧以后才删。
  - 两个版本相同的日志：从 ID 大的一边清理，什么都不删；从 ID 小的一边清理，删掉 ID 大的那个；再从 ID 大的一边清理，还是什么都不删。
  - 末尾有半条记录：保留，同一个客户端另一个完好的日志照删；去掉半条记录后才删。
  - 主人还在运行：下一次轮询就重新写出完整快照（两个文档各记一次修复），之后的修改别人都收到。主人重新启动：从别人的快照读回自己没保存的修改，之后的修改别人都收到。
  - `log_gc_after = None`：什么都不删。
  - 把实现里的每一条检查（自己的日志、修改时间、在线状态、半条记录、只认快照、同版本按 ID、发现自己的日志被删）分别去掉，都有测试失败。
  - 删不掉的日志（没有快照包含它）：第一轮读 6 个日志（两个文档各 3 个），第二轮一个都不读；一个文档的日志变了，只重读那个文档的 3 个。
  - "现在"取锁文件的修改时间（`dir.rs` 的单元测试：锁文件原来是 1000 天前的，写过之后是现在；日志的年龄按这个时间算）。
- 主人自己的日志（4.3）：
  - 第一次轮询没看到自己的日志，之后才出现：下一次写之前先读它，旧记录和新记录都在，客户端也拿回了旧修改。去掉"先读再写"时旧记录丢失。
  - 更新版本格式写的自己的日志：不改名、不覆盖，轮询记一次错误，写入报错。原来会被改名成 `*.corrupt-*`。

### 4.8 还没做

- 多台机器经 NAS 的实测。
- 离开的客户端的锁文件 `updates/<client-id>.lock` 不删（空文件）。

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

- **命令**：`bae_sync::register_commands(&mut registry)` 注册 `version.revert`，参数 `version`（`ProjectVersion::to_json()` 的形状：工程层 → (Loro peer ID 十进制字符串 → 操作数)）、可选 `sublayer` 或 `prim`（两个都给是参数错误），输出 `{edits}`。Agent、脚本、菜单都走它；命名版本本身是元数据（`History`），由 17 的工具查到 `version` 后调用这个命令（见 17 任务书的请求）。

### 5.5 还没做

- 分支与合并。
- 命名版本的创建、列表、差异的工具：`History` 不在 Stage 里，由 17 在 `Workspace` 里持有并暴露成工具（已在 17 任务书提了请求）。

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
- **10 hub-nas**：服务端用 `Relay` + 每个 WebSocket 连接一个 `Transport` 实现即可；消息编码见 2.3，客户端的约定见 3.1。握手用 `HubTransport::accept_with`，大小上限要和客户端一致（默认 256 MiB，见 3.1）。包装 `HubTransport` 的传输要转发 `backlog()` 和 `max_message_size()`（背压靠它们）；超时按收到的字节算（`bytes_received()`），不要只看完整的消息；只读连接可以用 `set_receive_limit` 收小一点。
- **11/12 界面**：`PumpOutcome::events` 已经通过 `Stage::subscribe` 广播，界面照常刷新；`pump` 出错时这一轮已导入的事件用 `take_interrupted()` 取（或等下一次 `pump` 报告，见 2.4）；`SyncSession::unsynced` 可用于"有未同步的修改"提示；`unknown_docs` 用于提示"协作者新建了工程层"；`presence()` / `editing_by_others()` / `presence_changed` 用于在线头像、别人的选中框和播放头、"有人正在改这个合成"的提示。
