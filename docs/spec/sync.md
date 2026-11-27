# 协同同步与版本（bae-sync）

> 负责：09 `collab/sync`。代码：`crates/bae-sync`。架构依据：`docs/ARCHITECTURE.md` 第 4.6、7 节。
> 状态：第 1–3 节（协议、会话、Hub 中继）PR A 已合并；第 4 节（共享目录）PR B；第 5 节（命名版本、只读视图、按 Prim 差异）PR C 已合并；第 6 节（在线状态、建议性锁）PR D；Hub 的 WebSocket 传输（3.1）PR E；回滚（5.4）PR F；第 7 节（连接管理）`collab/sync-connection`。分支与合并在后续 PR。

## 1. 概念

| 类型 | 说明 |
|---|---|
| `DocKey` | 同步的单位：一个工程层（`SublayerId`），或工程历史 `@history`（命名版本等）。`@` 不会出现在工程层 ID 里，所以不会冲突。 |
| `Message` | 协议消息，所有传输方式通用（见第 2 节）。 |
| `Transport` | 把消息送到"对方"：共享目录、Hub，或另一个客户端。`send` / `recv` 都不能阻塞。 |
| `DocSet` | 会话读写的一组文档。`Stage` 实现了它（各工程层）；`MemDocs` 是 Hub 用的纯 Loro 文档集合。 |
| `SyncSession` | 把一个 `DocSet` 和一个远端保持同步。和传输方式无关。 |
| `Relay` | Hub 端逻辑（不含网络）：每个连上的客户端一个会话，共用一份 `MemDocs`。10 hub-nas 用 WebSocket 包一层即可。 |
| `Connection` | 应用和 Agent 用来分享或加入工程的连接管理（第 7 节）：后台连接、断线重连、读写权限、状态、关闭。 |
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
| `Hello` | `protocol`、`client`、`versions: [(DocKey, 版本向量)]`、`seq`、`ack` | "我有这些。" 连接时发，之后定期发。没列出的文档视为空。`seq`：发送方 `Hello` 的编号（从 1 起递增，不编号时为 0）；`ack`：发送方收到的接收方最大的 `seq`（0：还没收到），`versions` 包括接收方在那个 `Hello` 之前发来、发送方导入了的数据；没有 `ack` 时 `versions` 就是全部实情（见 2.4）。 |
| `Updates` | `doc`、`version`、`data` | 一段 Loro 增量；`version` 是发送方在导出这段增量时的版本。 |
| `SnapshotRequest` | `doc` | 请对方发一份完整快照（共享目录模式用它压缩自己的日志）。 |
| `Snapshot` | `doc`、`version`、`data` | 完整 Loro 快照（含历史）。导入方式与 `Updates` 相同。 |
| `Presence` | `client`、`seq`、`ttl_ms`、`state` | 在线状态（第 6 节），不进任何文档的历史。`state` 是 `PresenceState` 的 JSON，`None` 表示离开。 |

### 2.3 二进制编码

一条消息 = 1 字节类型 + 字段。字符串和字节串前面是 LEB128 长度；版本向量用 Loro 的 `VersionVector::encode()`，作为字节串写入。

```text
Hello            0x01  protocol:varint  client:str  n:varint  n × (doc:str  vv:bytes)  seq:varint  ack:varint
Updates          0x02  doc:str  vv:bytes  data:bytes
SnapshotRequest  0x03  doc:str
Snapshot         0x04  doc:str  vv:bytes  data:bytes
Presence         0x05  client:str  seq:varint  ttl_ms:varint  (0x00 | 0x01 state:bytes)
```

- `Hello` 的 `ack` 没有时写 0，否则写 `ack + 1`。旧版本不写 `seq`、`ack`，解码为 `seq = 0`、没有 `ack`。
- 当前协议版本 `PROTOCOL_VERSION = 1`。以后只以兼容的方式扩展：新字段追加在已有字段之后（旧版本解码时忽略多出来的字节），新消息用新的类型字节（旧版本收到未知类型时跳过，`SyncError::UnknownMessage`）。
- Hub 模式一条消息就是一个 WebSocket 二进制帧；共享目录模式见第 4 节的记录格式。

### 2.4 会话规则（`SyncSession`）

会话为每个文档记住"远端已知拥有的版本"（`remote`）。每次 `pump`：

1. 第一次，或距离上次超过 `hello_interval`，或本地出现了新文档：发 `Hello`（列出本地所有文档的版本）。
2. 处理收到的消息（每次最多 `max_messages_per_pump` 条，避免卡住持有 Stage 的线程）：
   - `Hello`：用对方的版本**替换**（不是合并）`remote`。替换是为了对方丢了数据时能重新补发；CRDT 导入是幂等的。只有一个例外：我们在对方 `ack` 的那个 `Hello` 之后发出的数据，对方写这个 `Hello` 时可能还在路上，不退回。否则慢链路上一次比 `Hello` 间隔还长的大传输（新客户端加入大工程、上传大工程层）每隔 5 秒就被整个重新排一遍队，副本越积越多（`tests/convergence.rs` 的 `a_large_transfer_over_a_slow_link_is_sent_once`：50 kB/s 上传、下载 1 MB 各只发一份，以前上传发了 5 份；客户端另有一个文档导入不了、或者传输途中才把它加进来，也只发一份）。
     - 前提是那个 `Hello` 之前发的都到了。会话按文档、按我们的每个 `Hello` 记下它之后最后一次发送时的本地版本。对方 `ack` 了 `k`：拿我们 `Hello` `k` 之前的最后一次发送和对方的版本比，包含了，就去掉 `k` 之前的记录，`k` 之后的照旧算在路上；没包含，说明丢了（或者导入不了、乱序的链路还没送到），这个文档整个替换，对方缺的全部重发。只记最后一次发送不行：版本向量表示不了空洞，最后一次发送的版本盖住了丢掉的那次，只要一直在发（拖动），丢的部分就永远不会重发，对方因为缺依赖把之后的增量一直挂起，我们这边还报告全部已同步（`a_lost_update_is_sent_again_while_the_document_keeps_changing`：每 100 ms 改一次，一次丢包在 3 个 `Hello` 间隔内补上，直连、经 Hub 都是；`a_large_paste_during_a_drag_over_a_slow_link_is_sent_once`：拖动中、我们的 `Hello` 前后粘贴 1 MB，只发一份）。
     - 对方第一次列出的文档（应用刚加进层叠的工程层）：它没有这个文档时我们发的被它丢掉了，所以整个替换，马上重发；除非它的版本包含我们对这个文档的第一次发送（Hub 收到时就建了这个文档，之后发的还在路上，`a_new_document_filled_right_after_it_was_created_is_sent_once`：新建工程层后马上放进 1 MB，只发一份）。会话为此单独记下第一次发送。对方导入失败的数据不在它的版本里，下一次比对时重发，只影响这一个文档。
     - 不带 `ack` 的 `Hello`（旧版本、共享目录）照旧整个替换。`hello_interval = None` 的会话也是：对方的 `Hello` 确认不到我们之后的 `Hello`，丢了的就永远不会重发，所以什么都不算在路上（传输超过对方 `Hello` 间隔就会重发）。
   - `Updates` / `Snapshot`：`remote[doc]` 并入 `version`；本地已经包含 `version` 就跳过，否则导入。本地没有这个文档时不导入，在 `PumpOutcome::unknown_docs` 里报告（见 2.5）。
   - `SnapshotRequest`：记下来，等本轮收到的数据都导入之后再导出快照发回去（同一个文档一轮只答一次）。这样快照包含请求之前收到的一切：共享目录在交付自己日志里的记录之后才可能请求（启动时、重新交付时），并用这份快照覆盖自己的日志（4.3）。已经取出来的请求总是全部回答：本轮收消息时出错、或者前面某个回答发不出去（比如这个文档的日志写不了），后面的照样回答，最后返回第一个错误。它们已经不在传输层的队列里了，丢掉就再没人回答（`snapshot_requests_taken_before_a_failure_are_answered`、`a_failed_snapshot_answer_does_not_hold_back_other_documents`）。
3. 对每个本地文档，如果 `remote[doc]` 不包含本地版本，导出 `remote[doc]` 之后的增量发出去，并把本地版本并入 `remote[doc]`。距离上次发送不到 `min_send_interval` 时这一步推迟到之后的 `pump`（共享目录模式用几百毫秒，避免拖动时每秒几十次 SMB 追加；Hub 模式为 0）。被推迟的部分只有之后的 `pump` 才会发出，所以关闭连接前（最后一次 `pump` 之后）要调用 `SyncSession::flush`：不管 `min_send_interval`（也不管下面的 `max_backlog`），立即把对方缺的发出去（不收消息，也不发 `Hello` 和在线状态）。`Connection::close` 把 `flush` 的结果交给关闭线程，按下面的 `max_backlog` 一条条发（7.7）。
4. **流量控制**（`SessionConfig::max_backlog`，默认 16 MiB，和 `HubLimits::max_backlog` 一样）：传输层报告交给它、还没发出去的字节数（`Transport::backlog()`：Hub 连接写缓冲里等对方读的，`ThreadedTransport` 队列里后台线程还没交给内层传输的；默认实现返回 0，`send` 返回前就做完 I/O 的传输不用管）。第 3 步每发一个文档之前看一眼：超过 `max_backlog` 就停下，这一轮不再发任何文档的数据（照常收消息、发 `Hello`、在线状态、回答快照请求），积压降下来之后从停下的那个文档接着发，每个文档都轮得到。因为是发之前检查，积压低于上限时一条消息不管多大都能发出去（新客户端第一次同步一个很大的工程层），所以最多积压 `max_backlog` 加一条消息。它管的是内存：没有它，对方缺的一次全排进写缓冲（新客户端经 VPN 加入 200 MB 的工程，NAS 那边就先排 200 MB）。对方的 `Hello` 带 `ack` 时，还在路上的数据由第 2 步的"在路上"记录挡住，对方的 `Hello`（每 5 秒一次；慢链路上一次大传输比这长）不会让我们再排一份，有没有流量控制都一样（3.1 的 `a_slow_reader_is_not_flooded_by_its_own_hellos`：关掉流量控制时积压约 16 MiB，发出的仍是一份）。对方的 `Hello` 不带 `ack`（旧版本），或者我们的 `hello_interval = None`（没有"在路上"的记录）时，对方每个 `Hello` 都让我们把它还没收完的全部再排一份；流量控制把这限制在每次积压清空最多一次（积压清空时还在路上的那部分可能再发一次），而不是每个 `Hello` 一次。`SyncStats::held_back` 记暂停的次数；`None` 关闭流量控制。
5. **不发数据**（`SessionConfig::send_data = false`，默认 `true`；只读）：第 3 步和快照请求的回答都不做，`flush` 也什么都不发；`Hello`、在线状态、收消息照常。用在不能写的连接上：只读令牌的 Hub 连接（握手响应 `x-bae-access: read`，3.1；`GET /v1/status` 返回 `"access": "read"`）、这个用户写不了的共享目录（`DirTransport::open_read_only`，4.5）。`Connection` 按对方给的权限自动设置（7.5）。Hub 反正丢弃只读连接发来的数据，而且只收它们不太大的消息（3.1）：不这样设的话，查看者本地的修改会在 Hub 每个 `Hello` 之后、每次重连之后再发一遍，一个文档里超过上限就每次都被断开。本地修改留在本地，`unsynced` 照样列出它们；什么都没交给传输，所以也没有"在路上"的记录，对方确认了我们的 `Hello` 也不会把它们算作已同步（`a_session_without_send_data_only_receives`）。被第 4 步挡下的文档同样没交给传输、不算在路上：对方确认我们的 `Hello` 后照样缺它们，积压降下来就发（`documents_held_back_are_not_on_their_way`）。

在收到对方第一个 `Hello` 之前不发任何数据（不知道对方有什么，全量发送太浪费）。

**丢包、重复、乱序、延迟**：重复和乱序由 CRDT 吸收（依赖缺失的操作 Loro 会挂起，等依赖到了再应用）。丢包由定期 `Hello` 修复：对方的 `Hello` 让我们知道它实际有什么，缺的会重新发（发出之后对方又确认了我们的一个 `Hello` 时，所以两边都要定期发 `Hello`）。断线重连时调用 `SyncSession::reset()`。验证：`tests/convergence.rs` 的 `random_schedules_converge_while_edits_go_on`，随机调度（Hub 或直连、2–4 个节点、丢包、重复、乱序、重连、后加进来的文档、导入失败、没有定期 `Hello` 的节点）在网络变好之后，大家继续编辑的同时，坏网络期间丢的在几个 `Hello` 间隔内补齐，停下后一致。默认跑 24 个调度，`BAE_SYNC_STRESS_SEEDS=n` 跑 n 个。

**交给链路的和对方确认的**：`remote` 包含已经交给传输、可能还在路上的部分（决定发什么）。会话另外记一份对方确认过的版本：对方最近一次 `Hello`，加上之后它发来的数据。链路断了，交给它的消息可能没送到：`SyncSession::forget_unconfirmed()` 把 `remote` 退回确认过的那份，`unsynced` 于是包括交出去、但对方没确认的修改（`Connection` 断线时调用，7.4）。

**传输出错**：`pump` 收消息时遇到传输错误，先把本轮出错前已经收到的数据导入，再返回错误（导入不需要连接；对方已经认为这些数据送到了，不导入就会丢修改，例如客户端发完最后几条修改马上断开）。本轮开头的定期 `Hello` 发不出去时也一样：照样先收、先导入，再返回错误。导入的事件照常通过 `Stage::subscribe` 发出，只是 `PumpOutcome` 随错误一起丢掉。`Relay` 有连接出错时多转一轮，让这些数据在同一次 `pump` 里转给其他连接。

发送时某个文档出错，其他文档照样发：它们已经算作发出（并入了 `remote`），而传输层可能只是这一条失败（共享目录里某个文档的日志写不进去、消息超过 Hub 的大小上限），跳过它们就会留下没人补发的空洞（4.3）。全部发完再返回第一个错误。

### 2.5 别人新建的工程层

工程层的层叠顺序在 `project.json` 里，不在 CRDT 里，所以协作者新建的工程层不会自动出现在本地层叠中。会话收到本地没有的文档时只报告（`unknown_docs`），由应用决定插到层叠的什么位置。应用把它加进 Stage 后（`Stage::insert_sublayer`），会话下一次 `pump` 会因为"出现新文档"发 `Hello`，远端随即把内容补过来。

### 2.6 线程

`SyncSession::pump` 在持有 `Stage` 的线程上调用，不做阻塞 I/O。共享目录、网络的 I/O 由传输层放在后台线程里做，通过队列交给会话：`ThreadedTransport::spawn(内层传输, tick, on_receive)` 把任何会阻塞的传输搬到后台线程，收到消息后调用 `on_receive`（接到"请求重绘 / 尽快 pump"上）。

`ThreadedTransport` 的 `send` 只是放进队列，内层传输发送失败要等之后的 `recv` 才报告。它的 `backlog()` 是队列里还没交给内层传输的字节数加上内层传输自己的积压（后台线程每轮更新一次）；`backlog()` 报告过大于 0 之后，队列和内层传输都清空时，后台线程也调用一次 `on_receive`（在清空它的那一轮结束时，最晚再晚一个 `tick`），让因为积压暂停发送的会话（2.4 第 4 步）接着发。以前只在积压撑过后台线程一整轮时才叫醒：最常见的情况（发出一个大文档，其余的暂停，后台线程一轮就写完）反而叫不醒，只靠 `on_receive` 来 pump 的应用要等到别的事触发 pump 才接着发（`a_session_held_back_by_the_queue_is_woken_when_it_drained`）。内层传输自己的积压（`HubTransport` 的写缓冲）要降到 0 才叫醒，不是降到 `max_backlog` 以下就叫；应用定时的 pump 会更早接着发。`Transport::close()` 等后台线程把队列里的消息都交给内层传输，再关闭内层传输（调用它的 `close`），然后返回 `recv` 还没报告的第一个错误。关闭前想知道最后几条有没有送到，就调用 `close`。丢弃时同样会等队列交给内层传输，但不关闭内层传输（直接丢弃），也看不到错误。`close` 的默认实现什么都不做，只适合 `send` 返回前就做完 I/O 的传输；`HubTransport` 等对方确认关闭（3.1），`DirTransport` 还有日志落后时报错（4.3）。

会阻塞的不只是收发：`DirTransport::open` 要在共享目录上建目录、拿锁，锁被占用时最多重试 2 秒（4.7 的清理删别人的文件时会短暂拿住对方的锁）才报 `ClientIdInUse`；`HubTransport::connect` 最多等 `timeout`。两者都要在后台线程上调用（比如之后运行 `ThreadedTransport` 的那个线程），不要在界面线程上。

`ThreadedTransport::stalled()`：队列里有消息、后台线程却一条都没交给内层传输的时长（没有消息在等时为 0）。它很长，说明后台线程卡在一次读写里（NAS 不响应），交出去的消息还没上路（7.4）。

同一轮 `pump` 里同一个文档收到的多条数据合并成一批导入（`DocSet::import_batch`），`Stage` 只重建一次镜像、只发一个事件。

## 3. Hub 中继（`Relay`）

- `Relay` 持有一份 `MemDocs`（每个文档一个 Loro 文档，收到未知文档时自动创建），每个连接一个 `SyncSession`。一个客户端发来的增量导入后，其他客户端的会话在下一轮 `pump` 时发现本地版本变了，就把对方缺的部分发过去。
- `Relay::pump` 在同一次调用里最多循环 4 轮，让一个客户端的修改在一次调用内转发给其他人。
- Hub 用 `MemDocs::load` 恢复持久化的快照，用 `docs().export_snapshot` 保存。Hub 重启丢了数据也没关系：客户端重连后对方 `Hello` 里版本变小，客户端会把缺的重新上传。
- 传输出错的连接会被断开（`RelayOutcome::disconnected`）。
- **按连接设置会话**：`Relay::connect` 给每个连接同样的设置（`Relay::peer_config()`：Relay 自己的 `ClientId` 和 `hello_interval`，其他为默认值）。`Relay::connect_with(transport, config)` 给某个连接单独设置，一般从 `peer_config()` 改起。`config.client` 一律换成 Relay 自己的 ID（Relay 对所有连接都是同一个客户端，各会话也靠它认出自己的在线状态）。
  - Hub（10）给每个客户端连接的会话设 `max_backlog` = 连接的 `HubLimits::max_backlog`（`HubConfig::limits`，`hub.toml` 的 `[limits]`）。
  - Hub 用它接工程目录（`DirTransport`）：`min_send_interval` 默认 300 ms（`HubConfig::store_send_interval`）、`hello_interval = None`（4.5 的推荐设置）。客户端照常每次 `pump` 立即收到修改，只是写盘合并了：测试里 100 步的拖动，`hub.log` 从 101 条记录降到 3 条。最后一个客户端断开时 Hub 立即 `Relay::flush()`：没人在编辑，合并没有意义，而没有连接时的轮询间隔（500 ms）比写盘间隔长。
  - 有连接设了 `min_send_interval` 时，丢弃 Relay 之前要先 `pump` 再 `Relay::flush()`（对每个连接调用 `SyncSession::flush`），否则被推迟的最后几次修改永远发不出去。`Relay::close_peer(handle)` 先让这个连接的传输把已经发出的消息交付完（`Transport::close`，见 2.6）并返回出错情况，再断开它。Hub 卸载工程、退出时都这样做：`pump`、`flush`、对工程目录 `close_peer`，最后几次写盘失败会记进日志。
  - 验证：`tests/convergence.rs` 的 `a_peer_with_its_own_session_settings`（单独的发送间隔和 `Hello` 间隔生效、`flush` 立即发出、`ClientId` 用 Relay 的）和 `closing_a_threaded_peer_reports_its_last_writes`（`close` 返回前队列里的消息都已交付；最后一次 `pump` 之后才失败的写由 `close_peer` 报告）；`services/bae-hub/tests/hub.rs` 里把写盘间隔设成 1 分钟，拖动只能靠 `flush` 写出：拖动后立即退出 Hub（客户端还连着）、最后一个客户端断开（工程不卸载）、工程被卸载，三种情况重新加载后每一步都在。

### 3.1 WebSocket 传输（PR E，feature `hub`）

- `HubTransport::connect(url, token, timeout)`：`url` 形如 `ws://nas.local:7341/projects/<工程>`，路径和查询参数怎么选工程由 Hub（10）决定；`token` 作为 `Authorization: Bearer <token>` 发送。Hub 用 HTTP 错误拒绝握手时返回 `SyncError::HubRefused { status, message }`（`message` 是 Hub 返回的 `{"error": …}`，没随响应头一起收到时是状态码的标准说明），`Connection` 据此区分要用户处理的失败（7.4）。暂时只支持 `ws://`（局域网），`wss://` 报错。主机名解析出的每个地址都试，IPv4 优先（Hub 默认只监听 `0.0.0.0`，而 `localhost`、`nas.local` 常常先解析出 IPv6 地址），`timeout` 在剩下的地址之间平分。
- **读写权限**：Hub 在握手响应里带 `x-bae-access: read|write`（`ACCESS_HEADER`）。服务端用 `HubTransport::accept_granting(stream, token, limits, access)` 发出，客户端用 `HubTransport::access()` 读出；没有这个头（旧 Hub、`accept` / `accept_with`）是 `Access::Unknown`。
- 一条协议消息就是一个 WebSocket **二进制帧**；文本帧、ping/pong 忽略（pong 自动回复）；解码失败或未知类型的帧跳过；收到 close 帧或连接断开返回 `SyncError::Closed`。
- 握手后套接字是非阻塞的，`send` 写不完的部分留在缓冲区，在之后的 `recv` 里继续发。`recv` 继续发时写失败（比如对方发完最后一条就断开，我们还在给它发东西，连接被重置），先把已经收到的消息读完，再报告错误：对方认为它们已经送到了。这只是尽力而为：Windows 收到重置时会丢掉还没读的数据，所以客户端结束时要调用 `close`（等对方确认关闭，见下）。应用里套一层 `ThreadedTransport`：`ThreadedTransport::spawn(HubTransport::connect(..)?, Duration::from_millis(2), 请求重绘)`。
- **上限**（`HubLimits`，每个连接一份；`connect_with` / `accept_with` / `set_limits` 可改）：
  - `max_message`：本端发送的单条协议消息的上限，默认 256 MiB。消息不分片，所以对方要能收下这么大的一帧（tungstenite 默认单帧 16 MiB，超过 16 MiB 的工程层快照会被对方拒收、断线、重连后再发同一条，一直卡住）。发送超过上限的消息时 `send` 直接报错，不发出去（会话先把其他文档发完，见 2.4）。
  - `max_incoming`：接受对方的单条消息（也是单帧）的上限，默认也是 256 MiB；对方的 `max_message` 不能比它大。超过时读到帧头就报错、断开连接，不会先把整条收进内存。Hub 对只读连接默认只收 16 MiB（`HubConfig::read_only_max_incoming`，10），只读令牌不能让 Hub 收下和可写连接一样大的消息。只读客户端的会话应该设 `send_data = false`（2.4 第 5 步），只发 `Hello`、在线状态和快照请求，不受这个上限影响（对方的 `max_message` 不能比 `max_incoming` 大，说的是会发出去的消息）。上限不能太小：`Hello` 里每个文档一个版本向量，每个（文档，Loro peer）大约 11 字节，每次编辑会话都会给它编辑过的文档加一个 peer（1 MiB 大约 9.5 万对）；不知道自己只读的客户端还会发本地的修改，超过上限就每次连上都被断开、重连、再被断开（`viewers_with_large_local_edits_stay_connected`）。
  - `max_backlog`：流量控制。会话在积压超过它时不发文档数据（2.4 第 4 步，Relay 的每个会话都这样），所以读得慢的对方那里最多积压 `max_backlog` 加一条消息，再加上这期间的 `Hello`、在线状态和快照回答（都很小，或者很少）。它同时是"对方不再读"的判断：积压超过 `max_backlog`（默认 16 MiB）、并且 `stall_timeout`（默认 30 秒）内一个字节都没发出去，`send` / `recv` 报错（"peer is not reading"），`Relay` 随即断开这个连接。只要对方还在读，积压多少都不断开（低于下面的硬上限时）：一个大工程层就要积压这么多，按积压量断开的话，重连后又是同一条，永远同步不完。
  - `max_unsent`：硬上限，默认 528 MiB（`HubLimits::max_unsent_for`：两条 `max_message` 加 `max_backlog`）。哪一次 `send` 会让积压超过它，这次就失败，之后这个连接一直失败（`Relay` 断开它）。积压为 0 时不超过 `max_message` 的消息总能发出去。它至少要有 `max_message + max_backlog`：被流量控制暂停的会话可能合法地留下这么多（最好留出一条快照回答，就是默认的算法）；这样守流量控制的会话到不了它，它挡的是不守的发送（反复发快照请求、关掉了流量控制的会话）。Hub 的 `[limits]` 不写 `max_unsent_mib` 时按 `max_unsent_for` 跟着另外两个算，写得比 `max_message + max_backlog` 小、或者 `max_message` 超过客户端能收的 256 MiB 时拒绝启动（10，`limits_fit_together`）。
  - 所以每个连接写缓冲占的内存：正常最多 `max_backlog` 加一条消息（默认最坏 16 + 256 MiB，工程层越小越少），无论如何不超过 `max_unsent`（`max_unsent` 不小于 `max_message` 时；积压为 0 时一条消息总能发出去）。（系统的套接字缓冲另算，由系统限制。）之前写缓冲没有上限、只在停滞时断开，而读得慢的客户端每 5 秒一个 `Hello` 就让 Relay 把它缺的再排一份：经 VPN 加入一个 200 MB 的工程，NAS 每 5 秒多排约 200 MB，很快内存不够。
- **静默**：`set_idle_timeout(Some(时长))` 之后，这么久一个字节都没收到，`recv` 就报错（"nothing received"）。对方断电、VPN 或 Wi-Fi 断开时不会有重置，TCP 可能要很多分钟才发现，我们发的东西一直堆在内核缓冲区里。大消息只要字节还在到就不算静默。默认不开；`Connection` 默认 20 秒（Hub 每 5 秒给每个客户端发 `Hello`）。Hub 一侧给每个客户端设 60 秒（`PeerLink`，`HubConfig::peer_timeout`），同样按字节算：上传一条很大的消息超过 60 秒也不会被断开。只报告一次，之后 `recv` 返回 `SyncError::Closed`（`ThreadedTransport` 的后台线程就此结束，不会每个 tick 都报一次、唤醒一次）。
- `close()`：把缓冲区里剩下的消息和 close 帧发完，然后一直读（读到的丢掉），直到对方回应 close：这时对方已经读完我们发的一切，本端也没有没读的数据，关掉套接字不会重置连接（重置可能丢掉对方还没读的数据）。`stall_timeout` 内一个字节都没发出去就放弃，返回错误。丢弃 `HubTransport` 时只做一次不阻塞的关闭，不保证送到。
- 断线后：重新 `connect`，换掉传输，调用 `SyncSession::reset()`；双方 `Hello` 之后自动补齐（Hub 丢了数据也能补回去）。
- `HubTransport::accept(tcp_stream, token)` 是服务端一侧，给测试和简单工具用：`tests/hub.rs` 里用 `Relay` + `accept` 写了一个几十行的阻塞式参考 Hub。
- 验证（`cargo test -p bae-sync --features hub`）：3 个客户端经真实 WebSocket（回环）随机编辑后收敛、在线状态可见；一次修改从一个客户端经 Hub 到另一个客户端的延迟中位数 2.9 ms（最大 5.3 ms）；错误或缺失的 token 被拒（401）；Hub 重启并丢失全部数据后，客户端发现断线、离线继续编辑、重连新 Hub 后收敛，新加入的客户端拿到完整工程。对方读得慢（每 25 ms 读一条）时，一次发出的 10 条 3 MiB 消息全部送到，积压远超 `max_backlog` 也不断开；完全不读的对方在 `stall_timeout` 后被断开；积压超过 `max_unsent` 时连接失败，之后一直失败（`no_more_than_max_unsent_waits`）；读得慢的新客户端（每 100 ms 读一条 2 MiB 的消息，每次都发一个带 `ack` 的 `Hello`）经 `Relay` 拿到 20 MiB 的工程：Relay 那边最多积压约 3 MiB，文档数据正好发出一份；关掉流量控制时积压约 16 MiB（其余在系统套接字缓冲里，测试失败在积压上），发出的仍是一份，因为重发由 2.4 第 2 步的"在路上"记录挡住（`a_slow_reader_is_not_flooded_by_its_own_hellos`，积压超过 `max_backlog` 加一条消息、发出超过 1.25 倍都算失败。没有这些记录时，关掉流量控制发出五六倍、积压 50–60 MiB；开着时 Linux 上约 1.4 倍：积压清空时还在套接字缓冲里的部分再发一次，缓冲更大的平台更多）；对方发完最后一条就重置连接时（我们还在给它发 40 MiB），那一条照样读到；`ThreadedTransport(HubTransport)` 的 `close` 返回时，最后一条 20 MiB 的消息已经送到；握手响应里的权限 `read`、`write` 原样读出，不发时为 `Unknown`（`tests/hub.rs`）。

## 4. 共享目录模式（PR B）

工程目录放在 NAS 的 SMB/NFS 共享上，不需要服务器。

### 4.1 目录

```text
MyFilm.bae/
  sublayers/<工程层>.loro            基础快照（Project::save 写，02 负责）
  updates/<文档>/<client-id>.log     每个客户端只追加写自己的日志
  updates/<client-id>.lock           会话期间持有的建议性文件锁（尽力而为）
  updates/<client-id>.tmp-*          写权限探测、服务器时钟探测的临时文件（建好马上删；删不掉的由这个客户端下一次可写打开时删，4.7 的清理开着时也删）
  updates/<文档>/<client-id>.tmp-*   重写日志时的临时文件（写到一半崩溃会留下，4.7 清理）
  presence/<client-id>.presence      在线状态心跳文件（第 6 节）
  history.loro                       工程历史（第 5 节，History::save 写）
```

`<文档>` 是工程层 ID（`/` 变成子目录，`anim/shot010` → `updates/anim/shot010/`），或 `@history`。

目录名和文件名直接用 ID，ID 区分大小写，但 macOS（APFS 默认）、Windows 和多数 SMB 共享上的文件名不区分：只差大小写的两个工程层 ID（比如 `agent/Claude` 和 `agent/claude`）在那里是同一个目录，同一个客户端的两个日志互相覆盖，读者也会跳过其中一个文档的日志；`nul`、`con` 这类 Windows 设备名在 Windows 上也不是普通文件。现在没有检查这两种情况（待办），不要这样给工程层、客户端命名。

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

- 追加：整条记录一次 `write` 写入，然后关闭文件（不逐条 `fsync`：关闭时数据交给 SMB 服务器；崩溃留下的半条记录下次启动时截掉；修改本身还在会话的文档里）。压缩重写的新文件先 `fsync` 再改名。写之前检查文件长度是否等于自己记的长度（日志的位置上不是普通文件、比如是个目录时，算写失败：目录的长度没有意义，Windows 上是 0，APFS 上是 64 加每项 32，ext4 上是 4096，不能拿来比）：更长且文件头没变，说明上次写了一半，先截掉；文件不见了或文件头变了，就像下面的"写失败之后"一样处理（修复）：向会话要一份完整快照，在快照把日志重写之前什么都不写，请求丢了按退避再要（`a_repair_is_asked_for_until_a_snapshot_arrives`）。这条记录本身也不单独写：它的版本是写入者的整个版本，只有它的日志会声称有自己没有的操作，谁都不会再补发，新加入的客户端也导入不了；快照没来会话就断开了，重新挂上的会话会把目录缺的都写进去（`a_repair_whose_snapshot_never_comes_claims_nothing`）。写完用新打开的句柄再看一眼文件长度：文件不见了或变短了（被 4.7 的清理误删），同样按修复处理。Windows 上被删的文件还有别人开着时，名字留在目录里（"等待删除"），直到最后一个句柄关掉：`fs::metadata` 这时退回去读目录项、照样成功，打开它却被拒绝访问，所以用打开来看，拒绝访问也算不见了。
- **没读回来的自己的日志不能覆盖**：启动后的第一次扫描可能漏掉自己的日志（列目录时网络出错等，只记在 `last_error` 里）。在下一次轮询读回它之前，会话要写这个文档（追加，或者压缩、接管的快照）时，先把它读回来再写（和启动时一样交给会话、截掉半条记录）；读不了就不写，按写失败处理。所以不会在没读过的自己的日志上新建文件，丢掉里面没保存的修改，或者 4.7 里它代替被删日志保存的别人的操作（`an_own_log_the_first_scan_missed_is_not_replaced`、`an_own_log_the_first_scan_missed_is_read_back_before_writing`）。
- **写失败之后**：会话分不出一次写是否真的成功（`ThreadedTransport` 的 `send` 只是排队），已经把这些操作算作目录有了，下一条增量从它们之后开始。接着追加就会留下空洞：每条记录的版本覆盖它之前的一切，目录的 `Hello` 报的是并集，谁都不会再补发缺的部分。所以某个文档的日志写失败（追加或压缩）后，传输层不再给它追加任何东西，而是发 `SnapshotRequest`，等会话发来的快照把整个日志重写（和压缩一样）。第一次立即要；重写也失败、或者请求随着会话一起丢了，就再要，间隔从轮询间隔（至少 100 ms，最多 60 秒）起每次翻倍，最长 60 秒（每次 `recv` 都检查，不等下一次轮询目录）：会话每次都要在持有 `Stage` 的线程上导出整个快照，目录断开时不能每次 `pump` 都导出一遍（`a_stalled_log_is_not_snapshotted_on_every_pump`）。会话没来得及回答就断开了（Hub 出错后丢掉存盘连接）也不要紧：日志声称的版本就是它真有的，下一个打开目录的会话会把缺的写进去。在这之前，这期间的修改只在会话的文档里：还有日志落后时，`DirTransport::close()` 返回错误。
- 启动时主人也像读别人的日志一样读一遍自己的日志（里面可能有还没存进 `sublayers/*.loro` 的修改），并截掉末尾写了一半的记录。文件头都读不出来的日志改名为 `<client-id>.corrupt-<随机>` 留着排查，下次写入时新建。
- **更新的格式版本的日志不碰**：文件头完整、校验和对、`format` 是本格式，但 `version` 大于本版本的 `LOG_VERSION`（同一个客户端 ID 先用新版本运行过，现在降级了），不是"读不出来"：有了 4.7 以后，自己的日志可能是别人已删日志里那些操作唯一的容器，改名的话这些操作就从目录里"消失"了。所以这样的日志不读、不改名、不重写、不追加，这个文档什么都不写（会话发来的修改照收不误，只是不写进去；`last_error` 说明原因，`close()` 返回错误：这些修改没写进目录，新版本的客户端会写）。不会为它要修复用的快照，也不接管别人的日志到它上面。运行中自己的日志被换成了新版本的（锁不起作用时同 ID 的新版本客户端写的）：照常当作"被换掉"去修复，但重写前看一眼文件头，是新版本的就不重写，之后同样什么都不写（`an_own_log_of_a_newer_format_is_left_alone`、`a_newer_log_that_appears_underneath_us_is_not_replaced`）。
- **压缩**：日志超过阈值（默认 `max(1 MiB, 上次压缩结果 × 4)`）时，传输层向会话发 `SnapshotRequest`，拿到快照后写一个新文件（新 `generation`，文件头 + 一条 `Snapshot`），再改名覆盖旧日志。每个客户端只压缩自己的日志，所以多个客户端同时压缩不会互相破坏；目录总大小约为"客户端数 × 快照大小"，不随编辑次数增长。请求发出后一直没有快照来（会话出错或断开时请求丢了），日志比请求时长了一倍就再请求一次：丢一次请求不会让这个日志在整个会话里都不再压缩（`a_lost_compaction_request_is_asked_again_once_the_log_doubled`）。
- **压缩不能丢东西**：自己的日志可能是某些操作唯一的存放处（没保存的修改；或者 4.7 里因为它包含别人的日志、那个日志已经被删了）。传输层记着当前日志里所有记录版本的并集 `vmax`（包括启动时读回的上一次会话的记录），只有快照的版本包含 `vmax`、并且日志里没有读不懂的记录（新版本的消息类型，`vmax` 不一定包含它们的操作）时才用它覆盖日志（`own_records_this_build_cannot_read_are_not_compacted_away`）；否则（会话还没导入这些记录，比如刚启动、文档还是已保存的工程）把快照当普通记录追加在后面，什么都不丢，下一次压缩等日志再长到阈值（`DirStats::snapshots_appended`）。会话那边也只在导入本轮数据之后才回答快照请求（2.4），所以正常情况下快照总是包含整个日志。压缩写失败时清掉"已请求快照"标记，日志再长时会重新请求。

### 4.4 读

- 每隔 `poll_interval`（默认 1 秒）轮询一次；开启 `watch` feature 时文件变化会提前唤醒轮询（SMB 的变更通知不可靠，轮询始终保留）。
- 每个日志记住 `(generation, 已读到的位置, 长度, 修改时间)`。长度和修改时间都没变就不打开文件；`generation` 变了就从头读；否则从上次的位置往后读。每 `full_check_every`（默认 30）次轮询无论如何都打开一次（时间戳粒度粗、SMB 属性缓存过期的情况）。
- 记录不完整（长度超出文件末尾）或校验和不对时停在这里，下次轮询再试（写入方可能还没写完，或者 SMB 缓存还没刷新）。
- `sublayers/<文档>.loro` 也是一个来源：长度或修改时间变了就重新读，作为 `Snapshot` 交给会话（会话按版本判断是否需要导入）。

### 4.5 和协议的对应

共享目录被当作一个"远端"：

- 第一次扫描完后，传输层发一个 `Hello`（`client` 为 `shared-folder`），版本 = 目录里所有记录和基础快照版本的并集。会话据此把目录里还没有的本地修改（比如离线时的修改）写进自己的日志。
- 会话发来的 `Hello`：如果某个文档目录里的版本没有被会话的版本包含，就从头重新交付这个文档的所有日志（用于 2.5 的场景），然后回一个自己的 `Hello`（会话 `reset()` 之后靠它恢复）。
- 共享目录会话的推荐设置：`hello_interval = None`（目录不会丢数据），`min_send_interval` 几百毫秒（`FOLDER_SEND_INTERVAL` = 300 ms，`Target::session_config`）；关闭前调用 `flush`（2.4）。
- **只读打开**（`DirTransport::open_read_only`）：这个用户写不了 `updates/` 时用（`open` 建锁文件失败，错误是 `PermissionDenied` 或 `ReadOnlyFilesystem`，`Connection` 自动改用它，7.5）。照常读所有日志，自己以前写的日志也当作别人的读，不修复；什么都不写：不加锁、不写日志、不写在线状态文件，也不清理（4.7）：不压缩、不接管、不删任何文件（别人的日志、临时文件、锁文件、超过一天的心跳文件都不删），也不写探测服务器时钟的临时文件；发来的 `Updates` / `Snapshot` 返回错误，在线状态忽略。会话要设 `send_data = false`（2.4 第 5 步）。
- 会话发来的 `Updates` → 追加到自己的日志；`Snapshot` → 压缩重写自己的日志（快照不包含整个日志时改为追加，见 4.3）。

### 4.6 验证（`crates/bae-sync/tests/shared_dir.rs`）

- 3 个客户端在同一个目录上随机编辑后收敛；之后从旧的已保存工程加入的新客户端追上进度。
- 重启后从自己的日志恢复没保存的修改；末尾半条记录被截掉，之后的追加别人能读。
- 运行中自己的日志变长了、文件头没变（锁不起作用时，另一台机器上同 ID 的客户端写的）：下一次追加前截掉多出的部分，不算写失败，别人能读到新的记录（`a_log_that_grew_under_our_header_is_cut_back_before_the_next_append`）。截断用另开的写句柄：Windows 上追加模式的句柄没有改文件长度的权限。
- 一小时连续编辑（3600 次修改，大多是拖动同一个属性）：自己的日志最大 33 KB（压缩阈值 32 KB），压缩 29 次，`updates/` 总共 26 KB。
- 3 个客户端频繁同时压缩，不丢数据，新客户端读到的结果一致。
- 同一个 ClientId 不能同时开两个会话（文件锁）。
- 只读打开：读到别人和自己以前的日志，本地修改之后目录里一个文件都没多（没有锁、日志、在线状态文件），发修改报错，同一个 ID 的可写会话照样能打开（`a_folder_opened_read_only_is_read_but_never_written`）。它也不清理（4.7）：离开的客户端的日志（有容器的、要接管的）、过期的临时文件和超过一天的心跳文件都原样留着，不写探测文件；可写的客户端照样清理同一个目录（`a_folder_opened_read_only_cleans_nothing_up`）。
- 一次追加失败（日志暂时变成目录）、之后的写又能成功时，日志里不留空洞：会话还在，就回答快照请求、整个日志重写（快照也写失败就再要一次）；会话没回答就换了新会话（Hub 重新挂上存盘连接），目录只声称自己真有的，新会话把缺的补写进去。两种情况下另一个客户端读到的都和写的人一致（`a_failed_append_*`）。同一轮里其他文档的日志照样写进去，不留空洞（`a_failed_append_does_not_hold_back_other_documents`）。
- **模拟 SMB 延迟**：每个客户端用自己的目录副本，后台线程把别人的日志延迟 1–3 秒复制过来，30% 的情况先给一个截断的前缀（模拟没传完的追加），覆盖写也不是原子的；客户端用 `ThreadedTransport`。默认测试把延迟缩小 10 倍（100–300 ms，停止编辑后 0.3 秒收敛）；`--ignored` 跑真实 1–3 秒（停止编辑后 3.0 秒收敛）。
- `watch` feature：轮询间隔设成 1 小时，文件事件仍能在 0.4 秒内唤醒读取。

### 4.7 清理离开的客户端

每个写过的 ClientId（每个应用安装、每个 Agent 进程）都会留下 `updates/<文档>/<client-id>.log`，而每次轮询都要 stat 所有日志，在慢的 SMB 共享上越积越慢。`DirTransport` 每隔 `DirConfig::cleanup_interval`（默认 10 分钟；第一次在首次扫描完成后再等这么久）清理一轮。`DirConfig::inactive_after` 是"离开"的判断时间，`None` 就完全不清理。只读打开的客户端（4.5）从不清理：它什么都不写，也没有自己的日志可以接管。

**默认关闭**（`inactive_after: None`），要用的地方明确打开，推荐值 `DirConfig::RECOMMENDED_INACTIVE_AFTER`（3 天）。数据安全靠"另一个日志完整包含"（下面第 2–4 条），锁只影响是否误判对方还在运行；但 macOS smbfs 的 `flock`、NFS 的锁都还没在真的 NAS 上试过，所以应用等 4.8 的实测之后再打开（`the_cleanup_is_off_unless_asked_for`）。Hub（10）给自己的工程目录打开了：那里平时只有 Hub 自己的日志，清理只删它自己崩溃留下的临时文件；从共享目录复制进来的工程里别人的日志再也没人写，等 Hub 的日志包含了它们就删掉。"锁在 NAS 上靠不靠得住"的问题在那里不存在：没有别的客户端在那个目录上运行。

**只删别人的日志，从不删自己的。** 别人的日志 L（主人 X）要同时满足下面四条才删：

1. **X 已经离开**：X 的所有日志和 `presence/X.presence` 都超过 `inactive_after` 没有修改，并且 X 的锁 `updates/X.lock` 没人持有。时间按文件服务器的时钟算：每轮开始时在 `updates/` 里写一个临时文件、读回服务器给它的修改时间，再删掉它；这个时间和本机时钟取较早的一个当作"现在"。所以本机时钟快了好几天（还原的虚拟机、主板电池没电）也不会把所有人当成已经离开；取较早的只会让文件显得更新，服务器时间不对也不会让任何文件显得更旧。删除前再检查一遍 L 的修改时间和 X 的心跳文件，然后不等待地拿住 X 的锁（拿住期间 X 开不了会话；`open` 遇到锁被占用会重试 2 秒，跨过这一瞬间，经 VPN 访问的 NAS 上也够；所以 `open` 可能阻塞 2 秒，见 2.6）。用只读句柄拿共享锁：它和主人的排它锁互相冲突（`flock`、NFS 用 POSIX 锁对它的模拟、`LockFileEx` 都是），而且只需要读权限（NFS 上排它锁要可写的句柄，共享锁只要可读的；别人的锁文件通常不让别的用户写）。打不开锁文件（没有权限等）就当主人还在运行，原因记在 `last_error` 里。拿住锁之后只做两件事：用新打开的文件句柄看一眼 L 的长度和修改时间（打开文件会让 NFS 重新取属性，单纯 stat 可能用缓存），没变就立刻删除，然后放锁。主人在检查和拿锁之间运行过又退出了也不要紧：写过 L 的话最后一眼会发现，没写过的话 L 还是被容器包含。文件系统不支持锁时只看时间。比较客户端 ID 时不区分大小写：在不区分大小写的文件系统上（macOS、Windows、多数 SMB 共享）`Bob` 和 `bob` 是同一组文件，列目录看到的是文件创建时的写法（`bob` 的日志可能列成 `Bob.log`，心跳文件却是 `bob.presence`），区分大小写的话同一个客户端的活动迹象会被拆开，甚至把自己当成别人；在 Linux 上这只会让两个这样的客户端看起来像一个，只会少删、不会多删。
2. **L 完整**：文件头和每条记录都能解码（`Updates` / `Snapshot`，其中的 Loro 数据也能解析），末尾没有半条记录。这样才知道 L 里所有操作的上界 `vmax(L)`：每条记录的 `version` 是写入者导出时**整个**本地版本，不是这条记录里的操作，所以取各条记录的版本（以及 Loro 数据自己声明的结束版本）的并集。读不懂的记录里可能有任何东西，这样的日志不删；文件头是更新的格式版本的日志整个都读不懂，也不删，不当容器，也不接管（`departed_logs_of_a_newer_format_are_kept`）。
3. **另一个日志 M 完全包含 L**：M 的主人不是 X（可以是自己），第一条记录是完整（不是浅）快照，快照的版本 `base(M)` 包含 `vmax(L)`。只有开头的快照算数：后面的 `Updates` 是相对目录里已有内容的增量，可能依赖别的日志里的操作。删除前在磁盘上重新检查 M，不用缓存的结果：重新读 M 的文件头（`generation` 没变就还是同一个文件，主人对同一个 `generation` 只追加，第一条记录不会变；重写过的日志换了 `generation`，可能包含得更少，比如主人发现日志不见了或损坏后重写），再读上次之后追加的部分。M 读的时候正好在变、或者已经不包含 L 时，换下一个候选容器。基础快照（`sublayers/*.loro`、`history.loro`）不当容器：落后的客户端 `Project::save` 会覆盖它们。
4. **两个日志完全相同时只删 ID 小的**：如果 L 反过来也能当 M 的容器（`base(L)` 包含 `vmax(M)`，加上第 3 条，两者完全相同），只有 X 按字符串排在 M 的主人前面时才删。

**为什么几个客户端同时清理也不会丢数据**：把每个被删的日志连到删它时用的容器上，操作沿着这条链往上走：ops(L) ⊆ vmax(L) ⊆ base(M) ⊆ ops(M) ⊆ vmax(M) ⊆ base(N) ⊆ …。链不会成环：成环要求环上所有日志的版本都相等、每个都是前一个的容器，第 4 条就要求 X1 < X2 < … < X1。所以链的尽头总是一个留下来的日志。也就是说，排序只在"完全相同"这一种情况下需要：版本不同时第 3 条本来就只允许删小的那个（两个都删要求 vmax(L) ⊆ base(M) ⊆ vmax(M) ⊆ base(L) ⊆ vmax(L)）；不以快照开头的日志永远当不了容器，所以版本相同也可以删。被删的日志都已经好几天没动，同时运行的清理者看到的是同样的内容；还在写的容器只会变大：主人只往里追加，只用包含整个日志的快照覆盖它（4.3"压缩不能丢东西"）；唯一的另一种重写是日志不见了或被换掉之后的修复，那时它已经不是原来那个容器了。

- **接管**（`DirConfig::absorb_departed`，默认开）：离开的客户端的日志没有任何日志包含时（比如大家的日志都还没压缩过），向自己的会话要一份这个文档的快照（`SnapshotRequest`），自己的日志压缩成这份快照后就成了容器，下一轮就能删。每轮最多接管 `DirConfig::absorb_per_pass` 个文档（默认 1）：会话在同一次 `pump` 里回答所有快照请求，每个都要在持有 `Stage` 的线程上导出整个快照，一轮清理就给几十个工程层要快照的话，界面会卡住；其余的文档在之后几轮接管（`departed_logs_are_taken_over_one_document_per_pass`）。一个文档要过之后，目录没有变化就不再要（会话缺东西、或者会话没有这个文档根本不回答，都不会每轮重复）；不等回答，也不占用压缩用的"已请求快照"标记，所以没有回答也不会卡住任何东西。自己的日志已经包含它、只是第 4 条不让删时也不要（快照还是一样的，等有人再编辑这个文档自然就能删）。刚启动时第一轮清理就可能请求快照，这时会话可能还没导入自己日志里读回的记录：会话导入之后才回答（2.4），而且快照不包含整个日志时只追加、不覆盖（4.3），所以接管不会让自己的日志变小。
- **读多少**：每个日志的摘要（`generation`、读到的位置、`base`、`vmax`、是否完整）按 `(长度, 修改时间)` 缓存；变了也只读文件头和新追加的部分（`generation` 变了或文件变短了才从头读），和轮询一样。一次最多读 8 MiB（比这大的一条记录整条读），没读完的下次接着读（`summaries_are_read_a_chunk_at_a_time`）。读到的这一块在一条坏记录（校验和不对、长度不对：比如 NAS 崩溃后的全零块）处停下、后面还有数据，或者文件头用不了（坏的、别的文档的、更新的格式版本的）时，摘要就到此为止、算不完整，和一次读完整个日志的结果一样：下次从同一个地方读还是停在那里，接着读的话这一轮永远做不完，时间上限为无限时 `poll` 都不返回（`a_chunked_summary_ends_at_a_corrupt_record_or_header`、`a_corrupt_departed_log_does_not_stall_the_cleanup`）。一个文档里先读主人已经离开的日志，没有一个完整的就不读其他日志；容器的删除前检查也只读文件头和新追加的部分。所以一个删不掉的日志（末尾有半条记录、没有容器）不会让每一轮都把活跃客户端的日志整个重读一遍；同一个容器包含很多离开的日志时也不会为每个都整个重读。
- **每次轮询的时间上限**（`DirConfig::cleanup_budget`，默认 50 ms）：清理在 `recv` 里的轮询中同步执行（在传输层自己的线程上，应用和 Hub 里是 `ThreadedTransport` 的后台线程），这期间这个传输既不读也不写，Hub 的写盘也跟着推迟。第一轮清理要把离开的客户端写过的每个文档的日志从头读一遍，每条记录还要解析一次 Loro 元数据，慢的 SMB 上会很久。所以一轮清理可以分几次轮询做完：开头列一次目录（删过期临时文件、算每个客户端多久没动静、找出有离开的客户端日志的文档），然后一个文档一个文档地做，读每个日志（每 8 MiB）之前看一眼时间，超过上限就停下，下一次轮询从停下的地方接着做；读过的部分都在摘要缓存里，不会重读（`a_pass_over_many_documents_is_split`：时间上限为 0 时每次轮询只读一个日志或检查一个容器，每个日志只读一次）。每次轮询至少读一个日志，所以一轮总能做完。没做完的一轮还用开头列目录时的结果和"现在"的时间：只会让文件显得更新；删除前照样重新检查日志本身、主人的心跳文件和锁、容器，所以只是把"主人在检查之后又回来"的窗口从一次轮询拉长到几次，那些检查照样兜住（见下一条）。锁文件在一轮的最后处理。
- **误删正在运行的客户端**（文件锁不起作用，又好几天没写日志、没有心跳）：它下一次追加发现文件不见了，按 4.3 的修复要一份完整快照重写日志。"检查长度"和"写入"之间被删的情况（POSIX 上数据写进了一个已经没有名字的文件，谁也读不到），追加后的长度检查也能马上发现。剩下的窗口是：主人的打开、写入、关闭、检查全部落在清理者"用新句柄最后看一眼"到"删除"之间（几次往返），而且下一次追加还会修复；只有"这是退出前最后一次追加、之后也不保存"才可能丢这一次修改，可以接受。
- **其他残留文件**：`updates/` 和 `presence/` 里超过 `inactive_after`（同样按服务器时钟）的 `<client-id>.tmp-*`（写到一半崩溃留下的，自己的也算）直接删除。已经离开、没有任何日志的客户端的 `updates/<client-id>.lock` 只在拿住这把锁时删除（拿不到、或文件系统不支持锁就不删；正在启动的同 ID 会话拿到锁后发现文件不见了，会重新建一个。清理者删它时自己还开着它：Windows 和它提供的 SMB 共享上，这个名字在清理者关掉句柄之前处于"等待删除"状态，打开会被拒绝访问，`open` 这时也像锁被占用一样重试，不直接报错）。`.corrupt-*` 留给人排查，不删。空的文档目录不删（删了会和正在新建日志的客户端冲突）。
- 文件系统错误（没有权限、Windows/SMB 的共享冲突等）只跳过这个文件，记在 `last_error` 里（文件刚好不见了不算错误），传输照常工作。
- 统计：`DirStats::cleanups`（开始的清理轮数）、`cleanup_slices`（做了清理的轮询次数，一轮分几次做完时比 `cleanups` 多）、`cleanup_reads`（为清理读日志的次数）、`logs_removed`、`stale_files_removed`、`absorb_requests`；压缩相关的 `snapshots_appended`（4.3）。
- 验证（`tests/shared_dir.rs`，用 `File::set_modified` 把文件改旧，不用真的等）：
  - 离开的客户端的日志，在另一个客户端压缩后的日志包含它时被删；之后从旧的已保存工程加入的新客户端拿到完全相同的工程（每个工程层的版本向量也相同）。
  - 没有容器、容器的快照太旧、日志末尾有不是 Loro 数据的记录或半条记录时都不删；去掉这些之后就删。
  - 主人还在运行（锁被持有）、刚退出（心跳文件是新的）、它的另一个日志刚写过、换了大小写的心跳文件（`X.presence`）是新的时都不删；都旧了才删。
  - 两个内容完全相同、主人都已离开的日志，两个清理者先后清理：只删 ID 小的那个，数据都在。两轮清理真正交错的情况在单元测试里（`two_cleaners_at_once_remove_only_one_of_two_equal_logs`：一个清理者选好容器、正要检查时，另一个跑完整轮清理）。
  - 日志被删的客户端回来后照常工作，什么都不丢，所有人收敛。
  - 正在运行的客户端的日志被删（模拟锁不起作用）：下一次追加自己修复，新客户端拿到全部内容。
  - 接管：没有容器时清理者要快照，自己的日志成为容器，然后删除；之后不再重复请求。
  - 重启后马上接管：客户端从已保存的工程重启，自己的日志里有没保存的修改（第二次还是别人已被删除的日志的唯一容器），第一轮清理就请求快照：自己的日志不变小，新客户端拿到全部内容，离开的客户端的日志被接管后删除。
  - 过期的临时文件、离开的客户端的锁文件被删；新的临时文件、还有日志的或仍在运行的客户端的锁文件、自己的锁文件保留。
  - 8 轮客户端加入、编辑、压缩、离开，在线的客户端同时清理：新客户端拿到每一个操作；最后加入一个会接管的客户端后，离开的客户端的日志全部删掉（换 30 个随机种子都通过）。这个测试里客户端不并发创建同名 Prim：两个版本向量完全相同的客户端，镜像里这样的 Prim 命名可能不同（02 的问题，已在 #737 上报），修好后改回来（代码里有 TODO）。
  - 一轮清理分几次轮询做完时每个文档最多一次轮询删掉、每个日志只读一次；一轮最多接管一个文档，下一轮接管下一个，最后离开的日志都删掉；日志分块读和一次读完结果一样。
  - 单元测试：日志摘要（以快照开头的版本、上界、哪些情况算不完整）及其增量读取（追加、半条记录、换 `generation`、变短）、替换规则（第 3、4 条）；压缩只用包含整个日志的快照覆盖；在"打开日志"和"写入"之间日志被删或被截短时，追加后的检查发现并修复；容器在"选中"和"删除"之间被重写成不再包含 L 时不删 L。后两个用只在测试里编译的钩子在那一刻插手。修复请求丢了会再要、压缩请求丢了日志长一倍会再要、第一次扫描漏掉的自己的日志在写之前读回来、本机时钟快了也不删别人刚写的心跳文件（测试里把本机时钟调快）。

### 4.8 还没做

- 多台机器经 NAS 的实测。之后应用再打开 4.7 的清理（`inactive_after: Some(DirConfig::RECOMMENDED_INACTIVE_AFTER)`），重点看 macOS smbfs 的 `flock`、NFS 的锁是否像预期那样互相冲突。

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

`History::diff(stage, from, to)`（`to` 为 `None` 表示当前）/ `history::diff(stage, from, to)` 返回 `Vec<PrimDiff>`，按当前层叠从强到弱、同一层内按 `path` 排序（同一路径上，新版本里的 Prim 排在被删除的之前）：

| 字段 | 说明 |
|---|---|
| `sublayer`、`path` | 哪一层的哪个 Prim（被删除的用旧路径） |
| `change` | `Added` / `Removed` / `Modified` / `Moved { from }`（移动或改名） |
| `attrs` | 意见（值、关键帧、表达式）不同的属性名 |
| `fields` | 其他不同的字段：`type`、`specifier`、`references` |
| `authors` | 两个版本之间改过这个 Prim 的作者 |
| `last_changed` | 其中最晚一次提交的时间（Unix 秒） |

**按节点身份匹配，不按路径**（`identity.rs`，回滚也用它）

- 镜像路径不是标识。多人并发新建同名 Prim 后，只有完整重建镜像（`Sublayer::rebuild`，导入别人的修改时发生）才给其余节点加 `_<peer>_<counter>` 后缀；本地的删除、移动、撤销新建只就地修补镜像。所以版本向量相同的两个客户端可能把同一个 Prim 显示在不同路径上，同一路径在不同版本里也可能是不同的 Prim。
- `NodeIndex`：为一个工程层的一个版本建立 `TreeID → { 父节点, 原始名, 兄弟序号, 路径, spec }`，只用镜像的公开 API（`children`、`node_id`、`node_path`、`spec`）。
- 原始名：镜像名的后缀恰好等于该节点自己的 `_<peer>_<counter>` 时去掉，否则就是镜像名，与 `Sublayer::rebuild` 的规则一致。所以同一个节点在每个客户端上的原始名都一样，不管它的镜像有没有重建过。
- 两个版本的 Prim 按以下规则对应：
  1. 两个版本里 TreeID 相同的是同一个 Prim。
  2. **重建别名**：撤销删除（`RestorePrim`）和回滚重建 Prim 时 TreeID 会变，这条让它们仍然显示为修改而不是删除加新增。只考虑只在新版本里的节点 n（父节点是 p，p 在旧版本里的对应节点）和旧版本里 p 下只在旧版本里、同类型的子节点，而且两边必须互为唯一的候选，才视为同一个：
     - a. **按冲突后缀**：n 的原始名等于旧节点的原始名加上旧节点自己的 `_<peer>_<counter>`（倒着比较时反过来）。bae-model 撤销删除时用的是镜像名，所以一个显示为 `<名字>_<peer>_<counter>` 的 Prim 删除再撤销后，新节点的名字就是这个字面值；回滚在名字被占用时也这样命名（5.4）。
     - b. 否则按同原始名。
     从上往下逐层匹配，a 先于 b（a 配上的旧节点不再参与 b），所以重建的整棵子树都能对上，包括里面同名的兄弟。候选不唯一时不猜：例如同一父节点下两个同名 Prim 都被删掉、又有人手动新建了两个同名的，四个都分别报告删除和新增；类型不同的同名新建也报告删除加新增。
- 对应上的 Prim：父节点（按对应关系）或原始名不同时报告 `Moved { from }`，`from` 取旧版本的路径，`path` 取新版本的，同时列出字段和属性的不同；否则只在字段或属性不同时报告 `Modified`。只因祖先移动或改名而路径变了的、只差冲突后缀的，不算移动。
- 没对应上的：新版本里的报告 `Added`，旧版本里的报告 `Removed`。
- 显示的路径分别取自各自版本的镜像。当前版本用本客户端自己的镜像，所以路径可能和别的客户端显示的不同，但列出的是同样的 Prim 和同样的变化。

**作者**：遍历两个版本之间的所有 Loro 操作（两个方向都算，所以也能倒着比较），把每个操作对应到它改的树节点（树操作的目标节点，或 map 所在节点），作者取提交信息里的 `author`。对应上的一对节点，两个节点的作者合在一起。

**没有报告的**：兄弟之间的顺序变化；祖先移动或改名带来的子孙路径变化。

**验证**（`crates/bae-sync/tests/identity.rs`）：用"并发同名新建后一方删除"的复现，两个客户端的镜像不同时，差异都只报告被删的那个，另一个同名 Prim 不报告移动、新增或删除；同一个 Prim 在两个版本里只差后缀时报告修改而不是移动；撤销删除后（再改名其父节点）仍报告修改；删除再撤销一个显示为 `<名字>_<peer>_<counter>` 的 Prim，或者删除再撤销装着一对同名 Prim 的文件夹，差异为空、回滚什么都不改；两个同名兄弟都被删再新建两个、类型不同的同名新建，都报告删除加新增；字面上像后缀但不是自己 ID 的名字（`foo_1_2`）原样保留；真正的移动、改名、交换名字、路径被重用都报告 `Moved`；随机新建、改名、移动、删除、撤销（名字故意冲突，顶层和文件夹里都有）后，两个客户端和一个从头加载的客户端算出的差异按节点完全相同。

### 5.4 回滚（PR F）

- `History::revert_to(stage, author, id, scope)` / `history::revert(stage, author, label, version, scope)`：把工程放回某个版本的样子，作为**一个新的撤销单元**（标签 `revert to <名字>`），不改写历史：协作者像收到普通修改一样收到回滚，作者可以撤销它，其他人的撤销栈不受影响。
- 范围 `RevertScope`：`Project`（当前层叠的所有工程层）、`Sublayer(id)`、`Prim(path)`（这个 Prim 及其子孙，所有工程层）。`Prim` 的范围也按节点判断，一个 Prim 在范围里，只要它在任一边的节点在范围里：
  - 当前这边：当前镜像里 `path` 下的子树。
  - 那个版本这边：现在 `path` 上那个 Prim 在那个版本里的对应节点的子树（所以用现在的路径或改名前的路径都指同一个 Prim，改过名的 Prim 不会只删不恢复，也不会恢复出第二个）；现在 `path` 上没有 Prim，或者它在那个版本里没有对应节点时，退回那个版本里 `path` 下的子树。
  - 同一个 Prim 两边按同一个判断，所以移进或移出范围的 Prim 要么既删除又恢复，要么都不做。
  - `path` 由执行回滚的客户端自己的镜像解释：镜像不一致时（5.3），两个客户端上同一个路径可能指不同的 Prim。
- 做法：按节点身份（同 5.3，包括重建别名）决定删除、恢复和设字段：
  - 当前有、那个版本里没有对应节点的 Prim 删除（只删最上层，子树随之删除）。
  - 那个版本里有、现在没有（或者随祖先一起被删掉）的 Prim 连同整个子树恢复到原来的位置：父节点按 TreeID 找，放在它当前的路径下，兄弟序号用那个版本的。按那个版本的树序恢复（父节点在前，兄弟按序号），所以一起恢复的兄弟顺序不乱。
  - 只在 `Prim` 范围里：父节点已经不在、又不在范围里（不恢复）时，放到现在占着父节点旧路径的那个 Prim 下面；那里也没有 Prim 就报错 `PrimNotFound`（路径是父节点的旧路径），整个回滚不生效。
  - 不丢也不重复：随祖先一起被删掉、在那个版本里存在的 Prim，即使不在范围里也恢复到原来的位置；随祖先的子树一起恢复的 Prim 如果现在在别处（后来移出去了），那里的也删掉。
  - 移动或改名过的 Prim（对应上了，但父节点或原始名不同）在当前位置删除，再在原来的位置恢复，结果和以前按路径做的一样。
  - 两边都有的 Prim 把类型、specifier、引用和不同的属性意见设回去（`SetAttr`，那时没有的属性清除意见）。
- 每一步先按 TreeID 记下（`RevertPlan`），应用前才从当前镜像查出路径（`node_path`），再变成普通的 `Op`。
- 恢复用的名字：原始名。如果同一父节点下留下来的兄弟已经有这个原始名（例如协作者并发新建的同名 Prim），改用 `<名字>_<peer>_<counter>`，后缀是被恢复的那个旧节点自己的 TreeID，也就是镜像重建时它会显示的名字；留下来的那个不动。恢复的子树里的名字也用原始名，同级重名时按兄弟顺序，第一个用原始名，其余的同样加自己旧节点的后缀，和重建后的镜像一致。只比较原始名、后缀只取旧节点的 ID，所以不管哪个客户端执行、镜像有没有重建过，结果都一样。这个名字让新节点按重建别名 a（5.3）对上旧节点：回滚之后和那个版本的差异为空，再回滚一次什么都不改，协作者在恢复出来的 Prim 上的修改也不会被下一次回滚删掉。（字面上已经被占用的极少情况再加 `_2`、`_3`……，这时就对不上了。）
- 不恢复的：两边都存在的兄弟 Prim 之间的顺序；已经从层叠里删掉的工程层。那个版本里还不存在的工程层会被清空（工程层本身保留，层叠变化不可撤销）。
- 验证：整个工程回到 v1 后内容和 v1 的只读视图一致（包括子 Prim 顺序），v1 到当前的差异为空，回滚的修改都记在执行者名下，旧的版本向量仍被包含（历史没改写）；一次撤销恢复 v2 的内容，另一个作者的撤销栈不变；只回滚一个 Prim、只回滚一个工程层。按节点身份（`tests/identity.rs`）：回到删除之前，恢复的是被删的那个 Prim（名字带它自己的后缀），协作者的同名 Prim 不动，之后差异为空、再回滚什么都不改；镜像过时的客户端和另一个客户端执行回滚，交换后结果完全相同；装着一对同名 Prim 的文件夹被删后回滚，两个子 Prim 的名字和重建的镜像一致，两个客户端相同；一起删掉的兄弟恢复后顺序不变；随机新建、改名、移动、删除、撤销（名字故意冲突，顶层和文件夹里都有）后两个客户端各自回滚，内容相同、原有的每个 Prim 按节点都相同、和那个版本的差异为空、再回滚一次什么都不改；移动、改名、交换名字后回滚，内容和子顺序回到原样，差异为空。`Prim` 范围：改名后用新路径或旧路径回滚都恢复成原来的一个；祖先改名后回滚子 Prim 能恢复被删的孙 Prim；本客户端在该路径上显示的是协作者的同名 Prim 时只回滚它；移进移出范围的 Prim 不丢不重复；父节点被同名的新 Prim 替代时放到新的下面；父节点已经没有时报 `PrimNotFound`。

- **命令**：`bae_sync::register_commands(&mut registry)` 注册 `version.revert`，参数 `version`（`ProjectVersion::to_json()` 的形状：工程层 → (Loro peer ID 十进制字符串 → 操作数)）、可选 `sublayer` 或 `prim`（两个都给是参数错误），输出 `{edits}`（实际应用的操作数）。Agent、脚本、菜单都走它；命名版本本身是元数据（`History`），由 17 的工具查到 `version` 后调用这个命令（见 17 任务书的请求）。

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
- **重连**：`SyncSession::reset()` 也重置在线状态：清空别人的（由新链路重新报告），下一次 `pump` 立即重新发布自己的（新链路没见过）；序号至少跳过一个：Hub 在旧连接断开时替我们发的"离开"用的是"最后一个 + 1"，不跳过的话重连后的第一条会被当作旧消息丢掉，别人要等下一次心跳（默认 5 秒）才重新看到我们。
- `SyncSession::presence()` 列出其他在线的人（不含自己）；`editing_by_others(path)` 列出声明正在改 `path`、它的祖先或后代的人（界面据此提示"Alice 正在改这个合成"）；`PumpOutcome::presence_changed` 表示有人上线、离线或状态变了。
- **Hub**：`Relay` 把每条被接受的在线状态（包括只刷新存活的心跳）转发给其他所有连接；新连接一上来就收到当前所有人的状态；连接断开（`disconnect` 或传输出错）时替那个客户端向其他人发"离开"。
- **共享目录**：每个客户端用临时文件 + 改名重写自己的 `presence/<client-id>.presence`（一条第 4.2 节格式的记录，内容是编码后的 `Presence` 消息），离开时写一条"离开"记录。读者随轮询读取变了的文件。文件修改时间早于两倍 TTL 的不予理会（崩溃的客户端留下的）；超过一天没动的文件任何人都可以删除，这个"一天"按文件服务器的时钟算（和 4.7 一样写一个探测文件，读数复用一分钟）：本机时钟快了一天以上的客户端不会删掉正在运行的客户端的心跳文件，4.7 把它当作对方还在的证据。判断"早于两倍 TTL"要用文件修改时间对比本地时钟，所以 SMB 服务器和客户端的时钟差不要超过两倍 TTL（默认 40 秒）。

### 6.3 验证（`crates/bae-sync/tests/presence.rs`）

- 经 Hub：选中、播放头、工作区、建议性锁都能传到其他人；自己看不到自己；状态不变时不重复报告变化；新加入的人立即看到所有人；心跳维持在线远超 TTL，停止心跳后按 TTL 离线；主动离开立即生效；连接断开时其他人立即收到离开。
- 重放的旧心跳不能改写或复活别人；坏 JSON 被忽略。
- 连接断开（Hub 替它发了"离开"）后重连并 `reset()`：下一轮 `pump` 别人就重新看到它，它也看到别人（`a_reconnected_client_is_seen_again_at_once`）。
- 经共享目录：心跳文件、TTL 离线、主动离开、一小时前的残留文件被忽略（把修改时间改成现在后又能看到，证明是按时间过滤的）。

## 7. 连接管理（`Connection`）

应用和 Agent 用 `Connection` 分享或加入一个工程，不用自己管线程、重连和关闭。代码：`crates/bae-sync/src/connection.rs`。

### 7.1 用法

```rust
let mut config = ConnectionConfig::new(client_id, Arc::new(move || 请求重绘()));
config.presence = Some(我的在线状态);
let mut conn = Connection::start(
    Target::Hub { url: "ws://nas.local:7341/projects/film".into(), token: Some(令牌) },
    // 或 Target::Folder(工程目录)
    config,
);
// 每帧（以及 wake 被调用之后），在持有 Stage 的线程上：
let out = conn.pump(&mut ProjectDocs::new(&mut stage, &mut history));
// out.unknown_docs：协作者新建的工程层（2.5），由应用插进层叠
// out.status_changed、conn.status()：状态栏；conn.access()：只读时禁用编辑
// 退出或停止分享：
let mut closing = conn.close(&mut docs); // 立即返回
closing.unsynced(); // 没能发出去的修改
closing.wait(超时); // 退出时等关闭完成
```

- `Target::Folder(工程目录)`：`DirTransport` 放在 `ThreadedTransport` 里；会话用 4.5 的推荐设置（`hello_interval = None`、`min_send_interval = 300 ms`）。
- `Target::Hub { url, token }`（feature `hub`）：`HubTransport` 放在 `ThreadedTransport` 里；会话用默认设置（每 5 秒 `Hello`；Hub 60 秒一个字节都收不到会断开）。
- `ConnectionConfig`：`client`；初始 `presence`；`wake`（只在后台线程调用，从不在 `Connection` 的方法里直接调用；接到"请求重绘"上，不能阻塞）；可选的 `session`（不给就用 `Target::session_config`；`client` 换成上面的；`send_data` 在对方只给 `Read` 时关掉，这里设成 `false` 的话一直不发，用于只看不改的客户端）；`backoff`；`folder`（`DirConfig`）；`io_tick`（后台 I/O 线程查看新消息的间隔，默认 Hub 2 ms、共享目录 10 ms）；`io_stall`（链路 I/O 卡住多久算没同步，默认 5 秒，7.4）；`unresponsive_after`（连接多久还没连上就报 `Unresponsive`，默认 10 秒；Hub 至少 2 × `hub_timeout` 加 1 秒，7.4）；`hub_timeout`（连接、握手每一步的超时，默认 10 秒）；`hub_limits`（`HubLimits::default()`：两个方向单条消息 256 MiB；会话自己的流量控制是 `SessionConfig::max_backlog`，按整个链路的积压算：I/O 线程的队列加上 WebSocket 写缓冲，2.6）；`hub_idle_timeout`（Hub 静默多久算断开，默认 20 秒，`None` 不检查，7.4）。
- `Target` 的 `Debug` 不打印令牌，URL 里的 `?token=` 也换成 `…`。

### 7.2 线程

持有 Stage 的线程只调用 `pump`，从不等 I/O：

- 打开共享目录（要扫描 NAS 上的所有日志）、Hub 的 TCP 连接和 WebSocket 握手放在连接线程里，做完后通过通道交给 `pump`，并调用 `wake`。
- 链路断开后，旧传输在后台线程丢弃（丢弃 `ThreadedTransport` 要等它的 I/O 线程，可能正卡在 NAS 的读写里）。连接中 `close` 或丢弃 `Connection` 时，连接线程已经做好、还没被 `pump` 接手的链路也在后台线程丢弃。
- 本进程里正在放开的共享目录链路（丢弃、关闭、没人接手的）按"目录 + `ClientId`"登记；连接同一目录、同一 `ClientId` 时，连接线程先等它们放开文件锁（7.4）。
- `close` 剩下要发的（按流量控制发，7.7）和 `Transport::close` 在后台线程（Hub 要等对方确认关闭）。
- `wake` 的调用时机：收到消息；连接尝试结束；关闭结束；需要 `pump` 的时候到了：重试时间到、被 `min_send_interval` 推迟的修改、`Hello` 和在线状态心跳、链路 I/O 卡住快到 `io_stall`、连接到了 `unresponsive_after` 还没连上、只读共享目录该再探测能不能写了（7.5）。应用空闲时也要按时 `pump`，否则 Hub 60 秒后断开。定时由每个 `Connection` 的一个闹钟线程负责，它只记最早的一个时间：每次唤醒都会引起一次 `pump`，每次 `pump` 再定下一个。一次 `pump` 收满 `max_messages_per_pump` 条时（可能还有没收的）也由闹钟线程立即调用，不在 `pump` 里直接调用（Agent 的 `wake` 可能要拿 `pump` 的线程正持有的锁）。
- 连接一个只接受 TCP、永远不握手的地址时，每次 `pump` 不到 0.1 ms（debug 构建）。

### 7.3 状态

`status()` 返回 `ConnectionStatus`：

| 字段 | 说明 |
|---|---|
| `state` | `Connecting`：正在连接，或者已经连上、对方还没发 `Hello`。`Unresponsive { since }`：从 `since` 起一直在连接，超过了 `unresponsive_after`（7.4），界面显示"NAS 无响应"。`Online { since }`。`Offline { retry_in, error }`。`Failed { failure, reason }`：要用户处理（7.4） |
| `access` | `Read` / `Write` / `Unknown`（7.5）。离线时保留上一次的 |
| `unsynced` | 有本地修改还不能确定对方有了（`SyncSession::unsynced` 非空）：离线、只读、被 `min_send_interval` 推迟、重连后对方还没发 `Hello`、交给了后来断开的链路而对方没确认（7.4），或者等在 I/O 卡住超过 `io_stall` 的链路里 |
| `peers` | 在线的其他人数，不在线时为 0 |
| `generation` | 上面任何一项变化（`retry_in` 的倒计时除外），或者有人的在线状态变了，就加一。界面只在它变化时重画 |

`pump` 返回 `ConnectionOutcome`：`events`（远端导入的事件，也已经通过 `Stage::subscribe` 发出，不进撤销栈）、`unknown_docs`（对方每次 `Hello` 都会重新报告，直到加进来）、`presence_changed`（上线、下线时也为真）、`status_changed`、`errors`。

### 7.4 断线重连

- 任何传输错误或连接失败：进入 `Offline`，带错误文字。本地文档不受影响，离线时照常编辑。交给链路的消息可能没送到，所以会话退回对方确认过的版本（`SyncSession::forget_unconfirmed`，2.4）：交出去、对方还没确认的修改离线时算 `unsynced`，`close` 也报告它们，退出时会提示。共享目录的确认只有它的 `Hello`（第一次扫描完、回答我们因新文档发的 `Hello` 时）和它读到的别人的记录，自己写进去的不算确认：共享目录链路断开后，这个链路上改过的文档都算 `unsynced`，包括已经写进去的。偏保守，不会漏报。
- Hub 静默：`hub_idle_timeout`（默认 20 秒）里一个字节都没收到，就当链路断了（3.1）。Hub 每 5 秒发一次 `Hello`，所以正常的空闲链路不会断；对方断电、VPN 断开时不会有重置，不检查的话 TCP 要很多分钟才发现，这期间状态一直是在线、已同步。
- 连接迟迟连不上：从开始连接起超过 `unresponsive_after`（默认 10 秒）还没在线（连接线程卡在 NAS 上：等本进程的旧链路放开、等上一个丢弃线程结束、`DirTransport::open`；或者已经连上、对方还没发 `Hello`，比如共享目录的第一次扫描卡住），状态从 `Connecting` 变成 `Unresponsive { since }`，界面应显示"NAS 无响应"之类，不再只是"连接中"。连接线程照样等下去，不超时、不重开（再开一次也是等同一个 NAS），`retry()` 也不做什么；持有 Stage 的线程不受影响，闹钟按时唤醒一次，让只在被唤醒时 `pump` 的应用也看得到。连上或失败后状态照常变化。Hub 的连接每一步都有自己的超时（TCP 连接在 `hub_timeout` 内，握手的每次读写也是），Hub 不回应时尝试自己就以 `Offline` 结束、带着原因，所以 Hub 要超过 `unresponsive_after` 和 2 × `hub_timeout` 加 1 秒中较长的那个才报 `Unresponsive`（比如主机名解析卡住）；已经连上、Hub 还没发 `Hello` 时也按这个算（这段另有 `hub_idle_timeout` 兜底）。以前也按 `unresponsive_after` 算：默认两者都是 10 秒，丢包的 Hub 每次重试都在 `Offline` 之前闪几毫秒"无响应"，`generation` 多加一次。
- 共享目录的 I/O 线程卡在一次读写里（NAS 不响应）超过 `io_stall`（默认 5 秒），而有交出去的消息在等（`ThreadedTransport::stalled`，2.6）：`unsynced` 为真，状态仍是 `Online`。不断开：大工程第一次扫描在慢 NAS 上本来就可能很久，断开重连只会从头再扫。
- 重试间隔（`Backoff`）：从 0.5 秒起，每失败一次翻倍，最长 30 秒；每次随机减去最多 25%（`jitter`），免得同一个 Hub 的客户端同时涌回来。连接保持 10 秒（`stable_after`）以上再断开时，从 0.5 秒重新算。
- 重连：新的传输 + `SyncSession::reset()`，双方交换 `Hello` 后各自补发对方缺的，离线时的修改就这样上传。自己的在线状态重连后立即重新发布（6.2）。
- 不按计划重试的失败（`Failed`）：

  | `Failure` | 原因 |
  |---|---|
  | `BadToken` | Hub 401：令牌不对，或没有令牌 |
  | `NoAccess` | Hub 403：令牌不能打开这个工程 |
  | `NoSuchProject` | Hub 404：工程不存在，令牌也不能新建（只读令牌） |
  | `Refused(状态码)` | 其他 4xx（408、429 除外），例如 400：工程 ID 不合法 |
  | `ClientIdInUse` | 共享目录里同一个 `ClientId` 已经有会话。本进程里正在放开的同一目录、同一 `ClientId` 的链路（7.2）不算：连接线程先等它放开（NAS 卡住时可能很久，超过 `unresponsive_after` 后状态是 `Unresponsive`）。其他情况（例如刚退出、还没放开的另一个进程）持续 2 秒以上才算，之前照常重试（每次 `open` 本身也等锁 2 秒，4.7，所以默认设置下大约 4.5 秒才报） |
  | `BadTarget` | URL 不能用（不是 `ws://`、没有主机）。启动时就判断，不尝试连接；`retry()` 也不尝试 |

  之后只在调用 `retry()` 时，或者每隔 `Backoff::failed_retry`（默认 60 秒；`None` 表示不自动重试）再试一次。`Offline` 时也可以用 `retry()` 跳过等待。其他状态码（5xx、408、429）、连不上、连接被重置都按 `Offline` 处理。

### 7.5 读写权限

- Hub：握手响应里的 `x-bae-access`（3.1）。旧 Hub 不发，`access` 是 `Unknown`，照常发送修改，由 Hub 决定收不收。
- 共享目录：连接线程上 `DirTransport::open` 在 `updates/` 里建锁文件，成功时 `access` 为 `Write`。失败且错误是 `PermissionDenied` / `ReadOnlyFilesystem` 时还不能断定目录不能写：Windows 和它提供的 SMB 共享上，被别人的清理删掉、还处于"等待删除"状态的锁文件（4.7）打开时也是 `PermissionDenied`，等过 `open` 的 2 秒也可能还是这样。所以再在 `updates/` 里建一个自己的探测文件（`<client-id>.tmp-<随机>`，建好马上删；万一删不掉，这个客户端下一次可写打开目录时删掉，那时它拿着自己的锁，没有自己的会话在写这种文件；4.7 的清理打开时也按临时文件删，但它默认关闭）：
  - 也建不了（或者 `updates/` 不存在、建不出来）：改用 `DirTransport::open_read_only`（4.5），`access` 为 `Read`。
  - 建得了：目录能写，是锁文件本身打不开。这次尝试按普通失败处理（`Offline`，按退避重试），不当成只读。连续 3 次都这样（`LOCK_DENIED_ATTEMPTS`；每次 `open` 自己还等 2 秒），下一次就改用只读打开，`access` 为 `Read`：等待删除的锁文件这时早已删完，还打不开的是一直打不开的（从只读的地方拷来的、别的用户建的；Windows 上在资源管理器里给工程文件夹勾了"只读"时，文件都带上只读属性，目录照样能建文件，这个情况没在 Windows 上试过）。以前这样的目录一直 `Offline`，看不到别人的修改。
  - 探测本身出了别的错：同样按普通失败处理。
  `open` 遇到 `PermissionDenied` 会先重试 2 秒，所以连接写不了的目录要多等这 2 秒；这些都在连接线程上，不卡 `pump`。只读打开的目录不清理（4.5）。
- 只读的共享目录链路每隔 `Backoff::failed_retry`（默认 60 秒；`None` 不检查）在后台线程上再探测一次能不能写：同样建、删一个探测文件，再看自己的锁文件（有的话）能不能以写方式打开（`DirTransport::probe_lock`，不加锁、不改文件）。后一步是为了因为锁文件改成只读的链路：只看探测文件的话每次都判断能写，重新连接，又在锁文件上失败，再扫一遍目录。能写了（权限改了，或者当初判断错了）就在后台线程丢掉只读链路，立即重新连接（可写；不算失败，退避不变），只读期间的修改随之写进日志。探测还没结果时不开新的探测；NAS 卡住时探测线程一直等，`pump` 不受影响。
- `Read` 时会话的 `send_data` 关掉（2.4 第 5 步）：不发任何修改，`Hello` 和在线状态照发（共享目录只读时在线状态不写），照常接收；本地修改留在 `unsynced` 和 `Closing::unsynced` 里，状态栏如实显示。应用应在 `Read` 时禁用编辑。`Write` / `Unknown` 时用配置里的 `send_data`（默认 `true`）。Hub 对只读连接只收不太大的消息（`read_only_max_incoming`，3.1）：只读客户端本来就不发数据，本地改了多少都不会被断开（`a_read_only_token_reads_but_sends_no_edits`：上限 64 KiB、本地改了 256 KiB，连接一直不断）。
- 还没做：不发 `x-bae-access` 的旧 Hub 不会再用 `GET /v1/status` 查一次。

### 7.6 在线状态

`set_presence` / `clear_presence` / `local_presence` 直接交给会话，断线时保留，重连后立即重新发布。`presence()`、`editing_by_others(path)` 只在 `Online` 时有内容（离线时不知道谁在线）。

### 7.7 关闭

`close(self, docs)`：

1. 在线时：`clear_presence`；最后 `pump` 一次（导入已经收到的，发出"离开"）；再 `flush`（不管 `min_send_interval`，把对方缺的都导出来）。导出在持有 Stage 的线程上做（文档在这里），发送交给后台的关闭线程：照会话的流量控制（`max_backlog`，2.4 第 4 步）一条条发，积压超过上限时等链路送走，同时丢掉收到的消息、发现链路出错就停下。以前 `flush` 一次全交给链路：大工程第一次分享、大部分还被流量控制挡着时关闭，一下子超过 Hub 链路的 `max_unsent`，链路失败、关闭报错，而 Hub 其实一直在读（默认上限下要关闭时还有 512 MiB 以上没发）。Hub 链路的积压在 `stall_timeout` 内一点都没减少就放弃，返回错误（Hub 链路自己的停滞判断只看超过它的 `max_backlog` 的部分，而会话的可以更小）；共享目录等多久都行，和它的 `close` 一样。
2. `Transport::close` 也在关闭线程上：Hub 等对方确认关闭（最多 `stall_timeout` 没有进展），共享目录等后台线程写完。持有 Stage 的线程立即返回。
3. 返回 `Closing`。`unsynced()` 立即可用：有本地修改、但没交给链路（或关闭线程）的文档（离线、连接中、只读、对方还没发 `Hello`），以及交给了已经断开的链路、对方没确认的（离线时，或者最后一次 `pump` 发现链路已断）。`try_result()` 不阻塞，`wait(超时)` 阻塞，退出时用；结果是错误，或者超时还没关完（`None`），最后几条都可能没送到。关闭完成时调用 `wake`。

连接中关闭：连接线程之后交出的链路在后台线程丢弃，`close` 立即返回。

退出时 `unsynced()` 不为空、结果是错误，或者等到超时还没关完，就提示"还有修改没有同步"。

不调用 `close` 直接丢弃 `Connection`：传输在后台线程丢弃，不 `flush`，也不等对方确认，被推迟的最后几次修改可能发不出去。

### 7.8 验证

`crates/bae-sync/tests/connection.rs`（共享目录，以及永不握手的 Hub 地址）和 `services/bae-hub/tests/connection.rs`（真实 Hub）：

- 两个应用经共享目录、经 Hub 收敛，`unsynced` 清零，互相看到在线状态；什么都不变时 `generation` 不变。
- Hub 停掉：三个应用都 `Offline`，`peers` 为 0，`access` 保留；重试间隔 200、400、800 ms（测试设置）。离线继续编辑。新 Hub 在同一端口、同一数据目录启动后，留下的两个自动重连、收敛，离线修改都在；对方的在线状态马上回来（心跳间隔设成 1 分钟、在线状态 3 分钟才过期，证明是重连时重新发布的；去掉 6.2 的修复或重连时的 `reset` 就超时）。
- 错误令牌、没有令牌（401）、令牌不能开这个工程（403）、只读令牌开不存在的工程（404）、工程 ID 不合法（400）：`Failed`，之后 300 ms 内不再尝试，`retry()` 只试一次。
- 只读令牌：`access = Read`；收得到别人的修改；自己的修改不发出去（`bytes_sent = 0`），`unsynced` 为真；在线状态照常可见；Hub 的只读上限设成 64 KiB、本地改了 256 KiB 也不被断开（只连了一次）。配置里 `send_data = false` 的客户端在可写的共享目录上同样什么都不写，`access` 仍是 `Write`，`Closing::unsynced` 列出改过的文档（`a_viewer_sends_no_edits_where_it_could`）。共享目录 `updates/` 不可写时同样，什么文件都不写（Unix；root 不受权限限制时跳过，测试时用去掉 `CAP_DAC_OVERRIDE` 的 root 跑过）。
- 关闭：最后一次修改被 `min_send_interval` 推迟时也能送到（共享目录 300 ms；Hub 测试里设成 1 秒，去掉 `flush` 就收不到），"离开"立即可见，`Closing` 成功；之后加入 Hub 的人也拿到。刚分享一个大工程、大部分还被流量控制挡着时关闭（8 个 5 MiB 的工程层，假 Hub 每 100 ms 读一条，上限按默认的算法缩小：`max_message` 8 MiB、`max_backlog` 1 MiB）：全部送到，`Closing` 成功，`unsynced()` 为空；一次全交给链路时超过 `max_unsent`，Hub 只收到 3–4 个工程层和一个重置（`closing_after_a_large_held_back_share_delivers_it`；单元测试用一个超过上限就拒收、慢慢放行的假链路，不依赖系统套接字缓冲的大小）。Hub 确认关闭这一点由 `tests/hub.rs` 验证（`ThreadedTransport(HubTransport)` 关闭时 20 MiB 的最后一条已经送到）；回环上直接丢弃也能送到，这里区分不出来。
- 共享目录打开时锁文件被拒（Windows/SMB 上"等待删除"的锁文件，测试钩子模拟）而目录能写：不当成只读，先 `Offline`、重试后可写连上，探测文件不留下；锁文件一直被拒：3 次 `Offline` 后只读连上，之后的探测因为锁文件还打不开不重新连接（尝试次数不变），锁文件能打开后变成可写（单元测试）；真的只读的锁文件（Unix，0444；root 不受权限限制时跳过，用去掉 `CAP_DAC_OVERRIDE` 的 root 跑过）：只读连上，读到自己以前的修改，之后 1 秒里不重新连接（`a_lock_file_we_cannot_open_joins_read_only`）；可写打开时删掉自己以前留下的 `updates/<client-id>.tmp-*`，别人的、文档目录里的不动（`our_leftover_probe_files_are_removed_on_open`）；只读连上的目录变成可写后，下一次探测（测试里 100 ms）重新连接，`access` 变成 `Write`，只读期间的修改写进日志；还不能写时链路不动、只连了一次（单元测试）。
- 共享目录还不存在：按退避重试，出现后加入并写进去。同一个 `ClientId` 已经在用：先照常重试 2 秒，然后 `Failed`、不再自动重试，对方退出后 `retry()` 成功。丢弃或关闭后立即重新分享同一个目录（6 轮）不报冲突；旧链路的 I/O 卡在 NAS 读里超过 2 秒宽限时，新连接等它放开再连上，不报 `ClientIdInUse`（单元测试）。
- 连接只接受 TCP、永不握手的地址：500 ms 里每次 `pump` 不到 0.1 ms（上限 250 ms），状态一直是 `Connecting`，`close` 也立即返回。`hub_timeout` 和 `unresponsive_after` 都设成 300 ms 时，每次尝试都是 `Connecting` → `Offline`，没有 `Unresponsive`（以前每次都在 `Offline` 前闪一下，`a_hub_connect_that_times_out_is_offline_not_unresponsive`）；Hub 的界限是 2 × `hub_timeout` 加 1 秒，共享目录是 `unresponsive_after`，链路已经连上、对方还没发 `Hello` 时也一样（单元测试）。连接线程卡在 NAS 上（等旧链路放开，它的 I/O 卡在读里）：`unresponsive_after`（测试里 300 ms）后只在被唤醒时 `pump` 的应用也被闹钟唤醒，状态变成 `Unresponsive`（`generation` 只加一次），`pump` 照样很快；NAS 恢复后连上，只尝试了一次（单元测试）。连接线程已经做好链路、I/O 线程卡在读里时 `close` 或丢弃，调用线程不等（单元测试）。
- 收到第一条消息、连接尝试结束时调用 `wake`；只在被唤醒时 `pump` 也能连上。只在被唤醒时 `pump` 的应用：共享目录还不存在时照样按退避重试，出现后连上；拖动的最后一步被 `min_send_interval` 推迟，也由闹钟唤醒发出（去掉闹钟就超时）。收满 `max_messages_per_pump` 的 `pump` 不在调用线程上调用 `wake`。
- 假 Hub（`tests/connection.rs`，收下 `Hello` 后不再读写）：静默 `hub_idle_timeout`（测试里 500 ms）后 `Offline`，之前发出去的修改算 `unsynced`；挂断时刚交给链路的修改，离线时、以及还没 `pump` 就 `close` 时都在 `unsynced` 里。真实 Hub 上空闲 11 秒（`hub_idle_timeout` 设成 9 秒）链路不断。
- 共享目录链路的 I/O 卡住时，交出去的修改超过 `io_stall` 后算 `unsynced`，卡住结束后恢复（单元测试）。
- 假 Hub 静默后只报告一次：之后 1.3 秒不 `pump`，`wake` 最多被调用几次（以前 I/O 线程每个 tick 报告一次、唤醒一次，约 470 次）。Hub 一侧按字节算 `peer_timeout`：0.5 秒的超时下，以 40 KB/s 上传一条 1 MiB 消息 2 秒不被断开，停下后被断开（`services/bae-hub/tests/hub.rs`）。
- 闹钟：时钟过时的 `pump` 恰好在闹钟线程超时、还没拿到锁时要求立即唤醒，下一次 `pump` 又定了更晚的时间：更晚的那个照样触发，之后的 `set` 不会被当成已经定好而丢掉（单元测试，强制这个交错）。
- Hub 停掉再起来的测试里，Hub 不在时退出的第三个人重连后不再显示（`reset` 清掉旧的在线表），新加入的人马上看到重连的两个人（重连时重新发布）。连接稳定超过 `stable_after` 后断开，重试间隔从 `first` 重新算（之前失败过 3 次）。
- `retry()` 不连接不能用的 URL；URL 里的 `?token=` 不打印。

## 8. 给其他工作流

- **02 doc-model**：需求记在 `docs/workstreams/02-doc-model.md` 的"请求"一节。
- **10 hub-nas**：服务端用 `Relay` + 每个 WebSocket 连接一个 `Transport` 实现即可；消息编码见 2.3，客户端的约定见 3.1。
- **11/12 界面**：分享、加入工程用 `Connection`（第 7 节），每帧在 `Shell::about_to_wait` 里 `pump`，`wake` 接到"请求重绘"；状态栏看 `status()` / `generation`；`access() == Read` 时禁用编辑；退出前 `close` 并在 `Closing` 报告没同步的修改时提示。`PumpOutcome::events` 已经通过 `Stage::subscribe` 广播，界面照常刷新；`SyncSession::unsynced` 可用于"有未同步的修改"提示；`unknown_docs` 用于提示"协作者新建了工程层"；`presence()` / `editing_by_others()` / `presence_changed` 用于在线头像、别人的选中框和播放头、"有人正在改这个合成"的提示。
