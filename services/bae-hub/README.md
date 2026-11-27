# bae-hub 协作服务

> 负责：10 `collab/hub-nas`。架构依据：`docs/ARCHITECTURE.md` 第 7 节（Hub 模式）。同步协议：`docs/spec/sync.md`（09）。部署：`deploy/`。

bae-hub 运行在团队局域网里一台常开的机器上，通常是 NAS。客户端通过 WebSocket 连上来，彼此实时同步工程。服务把工程保存在自己的数据目录里，所以即使谁都不在线，工程也不会丢。

- **实时同步**：
  - 每个工程一个 `bae_sync::Relay`（09 的中继核心），每个连接是它上面的一个 `Transport`。
  - WebSocket 握手和收发帧用 `bae_sync::HubTransport`，和客户端用的是同一份代码、同一个 WebSocket 库（tungstenite）。
  - 协议和共享目录模式完全相同。
- **工程日志**：
  - 每个工程保存在 `<数据目录>/projects/<工程 ID>/`。
  - 用的就是 `bae-sync` 共享目录的日志格式：Hub 是客户端 `hub`，写 `updates/<文档>/hub.log`，变大后自动压缩成快照。
  - 重启时从这里读回。
- **团队令牌**：每个连接带一个令牌，令牌决定只读还是可写，以及能打开哪些工程。
- **没有异步运行时**，只用普通线程：
  - 每个已加载的工程一个线程，另有一个后台线程负责写它的目录。
  - 每个新连接一个短命线程，只负责握手。

还没做（任务书后续阶段）：素材存储、mDNS 自动发现、历史快照的保留策略、代理生成、管理页。

## 运行

```sh
bae-hub --data /path/to/data          # 等同于 bae-hub serve --data ...
bae-hub gen-token                     # 生成一个新令牌，填进 hub.toml
bae-hub healthcheck                   # 本机的服务正常就返回 0（Docker 健康检查用）
```

第一次启动时，如果没有配置任何访问方式，服务会在数据目录里生成 `hub.toml`，里面有一个可写令牌，并在日志里显示一次。

收到 SIGTERM（`docker stop`、systemd）或 Ctrl-C 时，服务会先关闭所有连接、写完所有工程，再退出。

### 配置

优先级从低到高：`hub.toml` → 环境变量 → 命令行参数。

| 设置 | `hub.toml` | 环境变量 | 命令行 | 默认 |
|---|---|---|---|---|
| 数据目录 | — | `BAE_HUB_DATA` | `--data` | `./bae-hub-data` |
| 配置文件 | — | `BAE_HUB_CONFIG` | `--config` | `<数据目录>/hub.toml` |
| 监听地址 | `listen` | `BAE_HUB_LISTEN` | `--listen` | `0.0.0.0:7341` |
| 日志级别 | — | `BAE_HUB_LOG` | `--log` | `info` |
| 不带令牌的访问 | `anonymous`（`none` / `read` / `write`） | `BAE_HUB_ANONYMOUS` | — | `none` |
| 额外的可写令牌 | — | `BAE_HUB_TOKEN` | — | — |
| 额外的只读令牌 | — | `BAE_HUB_READ_TOKEN` | — | — |
| 空闲卸载（秒，0 不卸载） | `idle_unload_secs` | — | — | 600 |
| 单条消息上限（MiB）：发给客户端的、可写客户端发来的；不能超过 256 | `[limits]` `max_message_mib` | — | — | 256 |
| 流量控制（MiB）：给某个客户端排队的数据超过这么多时先不发工程数据 | `[limits]` `max_backlog_mib` | — | — | 16 |
| 给一个客户端排队的数据最多这么多（MiB），超过就断开；不能小于前两个的和 | `[limits]` `max_unsent_mib` | — | — | 2 × `max_message_mib` + `max_backlog_mib`（528） |
| 排队超过 `max_backlog_mib` 又一直不读的客户端几秒后断开 | `[limits]` `stall_timeout_secs` | — | — | 30 |
| 只读连接发来的单条消息上限（KiB） | `[limits]` `read_only_max_message_kib` | — | — | 16384 |

令牌：

```toml
[[token]]
name = "team"            # 日志和状态接口里显示的名字（不显示密钥）
token = "…至少 16 个字符…"
access = "write"          # write：可编辑、可新建工程；read：只能看
projects = ["film-a"]     # 可选：只能打开这些工程；不写或含 "*" 表示全部
```

`[limits]` 一般不用改（含义见下面的"消息大小与内存"）。每一项都不能是 0（包括 `max_backlog_mib`：0 会让流量控制只要有一个字节在排队就不发工程数据），填 0 时拒绝启动。几个值要配得上，配不上时拒绝启动：`max_message_mib` 不能超过 256（客户端最多收这么大，超过的消息客户端拒收，重连后又发，永远卡住）；`max_unsent_mib` 不能小于 `max_message_mib + max_backlog_mib`（被流量控制暂停的客户端那里可能合法地排队这么多），不写时跟着另外两个算。`max_message_mib` 调小的话，比它大的工程层就同步不了：Hub 发给客户端时只跳过这个工程层，连接不断，日志里记一条 warn；新版本的客户端从握手知道 Hub 的上限（`x-bae-max-incoming`，见下面"WebSocket 同步"里的"接收上限"），自己的修改放不下时也只跳过这个工程层，不发给 Hub，所以 Hub 日志里什么都没有，只在客户端上报告（`MessageTooLarge`、`SyncSession::too_large`，这个工程层一直算没同步）；旧版本的客户端照样发来，连接被断开，重连后又发。在代码里是 `HubConfig::limits`（`bae_sync::HubLimits`）和 `HubConfig::read_only_max_incoming`。

一个数据目录同时只能被一个 `bae-hub` 使用（靠 `hub.lock` 保证）。

## HTTP 接口

- `/healthz` 和 `/v1/info` 不需要令牌，其余都需要。
- 令牌的传法：`Authorization: Bearer <令牌>`；不能设置请求头的客户端用查询参数 `?token=<令牌>`。
- 错误响应是 `{"error": "…"}`。所有接口都只接受 `GET`。

| 路径 | 需要令牌 | 说明 |
|---|---|---|
| `/healthz` | 否 | 返回 `ok` |
| `/v1/info` | 否 | 返回 `{"service": "bae-hub", "version", "protocol": 同步协议版本, "sync_path": "/projects/<project>", "anonymous": null / "read" / "write"}`。客户端连接前用它确认对方是 Hub、协议是否兼容、要不要向用户要令牌 |
| `/v1/status` | 是 | 运行时间、已加载的工程数、在线客户端数，以及**当前令牌的名字和权限**（`access`）。同步连接自己的权限在握手响应里（见下一节），不需要另外查 |
| `/v1/projects` | 是 | 这个令牌能看到的工程。每个工程有：`id`、`loaded`、`docs`、`peers`（客户端 ID、令牌名、权限、地址、连接时间、`max_incoming`：它握手时说的单条消息上限，旧版本为 `null`）、`disk_bytes`、`saving`、`not_saved`（修改照常转发、但不保存的文档，见下）、消息计数 |
| `/projects/<工程 ID>` | 是 | WebSocket 同步，见下一节。不带 WebSocket 升级头时返回 426 |

状态码：

| 状态码 | 含义 |
|---|---|
| 400 | 工程 ID 不合法 |
| 401 | 没有令牌，或令牌不对 |
| 403 | 令牌不能打开这个工程 |
| 404 | 工程不存在，且令牌只读（只读令牌不能新建工程） |
| 426 | 同步地址必须用 WebSocket |
| 503 | 服务正在关闭 |

## WebSocket 同步

```text
ws://<主机>:7341/projects/<工程 ID>
Authorization: Bearer <令牌>
```

客户端就是 `bae_sync::HubTransport::connect(url, Some(令牌), 超时)`，外面套 `ThreadedTransport`（见 `docs/spec/sync.md` 3.1）。应用和 Agent 一般不直接用它，而是用 `bae_sync::Connection`（spec 第 7 节）：后台连接、断线后按退避重连、401/403/404 不反复重试、只读时不发修改、退出前把最后的修改送到。

- **工程 ID**：1–64 个 ASCII 字母、数字、`_`、`-`，同时也是服务端的目录名。用可写令牌连接一个不存在的工程会新建它。
- **帧**：每条同步协议消息是一个二进制帧，编码见 `docs/spec/sync.md` 第 2.3 节。
- **Hub 侧会话**：每个连接一个会话，`hello_interval` 为 5 秒，收到修改立即转发。
- **在线状态**（`Presence`）由 `Relay` 转发：新连接马上收到所有人的状态；连接断开时，Hub 替它向其他人发"离开"。
- **权限**：握手成功的响应（101）带 `x-bae-access: read` 或 `x-bae-access: write`，是这个连接按令牌和工程得到的权限。客户端用 `HubTransport::access()` 读出，应用据此在只读时禁用编辑、不发修改。没有这个头的旧 Hub，客户端当作"未知"，照常发送。
- **接收上限**：握手的请求和响应都带 `x-bae-max-incoming: <字节数>`，是这一端收的单条消息上限（Hub 这边：可写连接是 `max_message_mib`，只读连接是 `read_only_max_message_kib` 和 `max_message_mib` 中较小的那个）。两边发送时都不超过对方说的值：放不下的工程层、在线状态只跳过，不发出去被断开（spec 2.4 第 3 步、3.1）。旧版本没有这个头，照旧按自己的上限发。
- **只读连接**：可以发 `Hello`、`SnapshotRequest`、`Presence`。发来的 `Updates`、`Snapshot` 会被丢弃，并在日志里记一次。只读连接发来的单条消息最大 16 MiB（`read_only_max_message_kib`），更大的读到帧头就断开连接，不会先收进内存；发给它们的消息照样可以很大。
  - 客户端用只读令牌连接时（`/v1/status` 返回 `"access": "read"`），会话要设 `SessionConfig::send_data = false`（`bae_sync::Connection` 按握手响应的 `x-bae-access: read` 自动设置）：本地的修改不发出去，只收别人的。不设的话，会话会在 Hub 每个 `Hello`（5 秒一次）之后、每次重连之后把本地修改再导出、再发一遍；一个文档里超过上限时，新版本的客户端从握手知道上限，只跳过这个文档（`MessageTooLarge`），连接不断；不认识 `x-bae-max-incoming` 的旧客户端每次都被断开。
  - 上限不能设得太小：`Hello` 随文档数和编辑过它们的会话数增长（每个文档、每个编辑过它的会话约 11 字节），超过上限的话这个查看者根本连不上。在线状态随选中的图层数增长（每个约 30 字节），超过上限时这个查看者照常连着，只是别人看不到它和它的选中。
- **超时**：Hub 60 秒一个字节都没有收到某个客户端发来的东西，就断开这个连接（按字节算：一条很大的消息上传超过 60 秒，只要字节还在到就不断开）。正常客户端每 5 秒发一次 `Hello`，所以只有死掉的链路（比如合上盖子的笔记本）才会被断开。反过来，Hub 每 5 秒给每个客户端发一次 `Hello`，`bae_sync::Connection` 20 秒一个字节都没收到就当 Hub 不在了（`hub_idle_timeout`；Hub 断电、VPN 断开时不会有重置，TCP 要很多分钟才发现），所以 Hub 不能关掉这个定期 `Hello`。
- **重连**：断线后重新 `connect`，再调用 `SyncSession::reset()`。双方交换 `Hello` 后各自补发对方缺的部分，什么都不会丢。
- **延迟**：
  - 有消息往来时，Hub 每 1 毫秒查看一次各个连接；安静下来后逐步放慢到每 10 毫秒一次。
  - 所以空闲时几乎不占 CPU，一次修改最多多等约 10 毫秒。

## 数据目录

```text
<数据目录>/
  hub.toml                              配置（含令牌，注意文件权限）
  hub.lock                              防止两个 bae-hub 同时使用
  projects/<工程 ID>/
    updates/<文档>/hub.log              工程层的增量日志（09 的共享目录日志格式，自动压缩）
    updates/@history/hub.log            命名版本等工程历史
    updates/hub.lock
```

- **清理**：`bae-sync` 的共享目录模式默认不删任何日志（spec 4.7），Hub 给自己的工程目录打开了清理（3 天）：平时那里只有 Hub 自己的日志，所以只删它崩溃时留下的、3 天以上的临时文件；从共享目录复制进来的工程里别人的日志，等 Hub 的日志包含了它们就删掉。
- **备份**：复制整个数据目录即可。最好先停服务，或者用 NAS 的快照功能。
- **意外断电**：Hub 丢了最后一点数据也没关系。客户端重连时双方交换版本，客户端会把 Hub 缺的部分重新上传。
- **换回旧版本（降级）**：新版本的 bae-hub 写过的日志如果是更新的格式版本，旧版本不读、不改、不覆盖它（spec 4.3），这个文档的修改照常在客户端之间转发，但不保存。Hub 加载工程时打一条警告（`WARN`，带工程 ID 和这些文档），`/v1/projects` 的 `not_saved` 也列出它们。客户端各自留着这些修改，换回新版本的 Hub 之后重连时会补写进去。选择警告而不是拒绝打开工程：拒绝的话其他文档也都不能同步了。

## 实现

| 文件 | 内容 |
|---|---|
| `src/server.rs` | 一个线程阻塞地 `accept`。每个新连接交给一个短命线程：先 `peek` 请求头（不读走），普通 HTTP 请求直接回答；同步请求先做鉴权和路由，再把原封不动的套接字交给 `HubTransport::accept_granting(stream, None, client_limits(权限), 权限)` 完成握手（响应里带 `x-bae-access` 和 `x-bae-max-incoming`），然后挂到工程的 Relay 上。上限是 `hub.toml` 的 `[limits]`（`HubConfig::limits`），只读连接的 `max_incoming` 换成 `read_only_max_incoming`（默认 16 MiB）。 |
| `src/project.rs` | 一个工程 = 一个线程上的 `Relay`。工程目录通过 `DirTransport`（放在 `ThreadedTransport` 后台线程里）作为 Relay 的一个对端接上，所以读盘、写盘、压缩、修复都是 `bae-sync` 的代码。打开工程时一直 pump，直到目录发来 `Hello`（它排在所有记录之后），才开始接受连接。目录这个对端单独设置（`Relay::connect_with`）：最多每 300 ms 写一次（`HubConfig::store_send_interval`），拖动时不会每秒追加几十次。最后一个客户端断开时立即 `Relay::flush()` 把还没写的写出去；关闭工程（卸载、退出）前再 `flush` 一次，然后 `Relay::close_peer` 等存盘线程写完，写盘失败记进日志。 |
| `src/link.rs` | 包在 `HubTransport` 外面：丢弃只读连接的修改；60 秒一个字节都没收到就断开；把 WebSocket 的积压报告给会话（流量控制）。 |
| `src/registry.rs` | 工程按需加载；10 分钟没人连接就卸载（数据都在磁盘上）。 |
| `src/auth.rs` | 令牌只保存 BLAKE3 哈希，比较用常数时间。 |
| `src/config.rs` | `hub.toml`、环境变量。 |

**链路出错时不丢修改。** 客户端发完最后的修改马上断开时，`SyncSession::pump` 先导入这一轮已经收到的修改，再报告传输错误（spec 2.4），`Relay` 再多转一轮，把它们转给其他连接和工程目录。所以 `PeerLink` 出错时直接报告，不需要绕行。

**消息大小与内存**（`bae_sync::HubLimits`，spec 3.1）：

- 单条消息（也是单帧）默认最大 256 MiB，一个工程层的快照要放得下。
- 要发给某个客户端的一个工程层超过这个上限时（比如工程层是一点点改大的，新加入的客户端缺整段），Relay 只对这个客户端跳过这个工程层，其他工程层照发，连接不断（spec 2.4 第 3 步）。日志里记一条 warn（"a document does not fit in one message to this client"）：跳过时一次；客户端从别的途径拿到了一部分、或者重连之后，Hub 再试，仍放不下时再记一次。warn 的括号里按这条消息的大小说怎么办：超过 256 MiB（Hub 一条消息最多发这么多，`max_message_mib` 不能更大）时，这个工程层通不过 Hub：删掉其中的内容没用（放不下的是客户端缺的全部修改历史，不是现在的大小，删除只会再加修改），只能把内容移到新的、更小的工程层（`@history` 也通不过，没有办法）；否则说是 Hub 的 `max_message_mib`、客户端自己的上限（握手时说的，`/v1/projects` 的 `max_incoming`），还是两个都比它小，要调大到多少：只是客户端的小时，调 Hub 没用。以前 Relay 断开这个客户端，它重连后又被断开，别的工程层也拿不到。
- 别人的在线状态（选中的图层很多）发不给某个客户端时也只跳过，记一条 warn（"a collaborator's presence ... does not fit"），连接不断；那个客户端暂时看不到这个人。
- **流量控制**：给某个客户端排队、等它读的数据超过 16 MiB 时，Relay 先不给它发工程数据（照常收它的消息、回 `Hello`、转发在线状态），等它读下去再接着发。客户端每 5 秒一个 `Hello`，读得慢的客户端（比如经 VPN 第一次同步大工程）还没收完时，以前每个 `Hello` 都会让 Relay 把它缺的全部再排一份，NAS 很快内存不够。现在每个连接最多排队 16 MiB 加一条消息。
- 不管怎样，给一个客户端排队的数据不超过 `max_unsent_mib`（默认 528 MiB），超过就断开它（守流量控制的会话到不了这里：它不能小于 `max_message_mib + max_backlog_mib`）。
- NAS 内存小时调小 `max_backlog_mib`：平时每个读得慢的客户端那里最多排队它加一条消息。只调小 `max_unsent_mib` 省不了平时的内存。
- 排队超过 16 MiB、并且 30 秒内一个字节都没读的客户端被断开；还在读的不断开，读得再慢也能收完。

## 测试

`cargo test -p bae-hub`。客户端用的是应用自己的 `HubTransport::connect` 加 `ThreadedTransport`，所以这些测试同时验证了客户端和服务端能互通。

- **收敛与重启**：3 个客户端经 Hub 随机编辑后收敛。Hub 重启后，新加入的客户端只从 Hub 的磁盘就拿到了全部内容。
- **HTTP 接口与令牌**：
  - 没有令牌或令牌错误：401；只读令牌新建工程：404；限定工程的令牌打开别的工程：403；工程 ID 不合法：400。
  - 不带 WebSocket 升级头访问同步地址：426。
  - `?token=` 可以代替请求头。
  - `/v1/projects` 只列出令牌能看到的工程。
- **只读**：只读客户端收得到别人的修改，自己的修改不会传出去；状态接口里显示它是只读。只读客户端照样收得到 1 MiB 的修改；它从握手知道自己的上限，超过的消息自己不发、连接不断；不知道上限的客户端（手写的握手）发来超过上限的消息时，Hub 读到帧头就断开它；可写客户端不受影响（`read_only_connections_cannot_send_large_messages`）。本地改了 1.5 MiB 的查看者：默认上限下不设 `send_data` 也不断开；设了 `send_data = false` 时上限只有 64 KiB 也不断开，什么数据都不发（`viewers_with_large_local_edits_stay_connected`）。
- **放不下的工程层**：上限设成 256 KiB，一个客户端分 4 次各写 100 KiB，新加入的客户端缺整段：Hub 只对它跳过这个工程层，另一个工程层的修改照常收到，三个连接都不断；新加入的客户端的 `behind` 列出缺的这个工程层（`a_document_too_large_for_one_client_does_not_drop_its_link`；以前新加入的客户端被断开）。哪边记日志（`project.rs` 的 `the_hub_warns_only_about_layers_it_cannot_send`，内存链路，上限 64 KiB）：客户端自己的一次修改放不下时，它自己跳过、报告一次，Hub 日志里什么都没有；Hub 已有、发不给新加入的客户端的工程层，Hub 记一条 warn。
- **应用的连接管理**（`tests/connection.rs`，客户端是 `bae_sync::Connection`）：两个应用经 Hub 收敛；Hub 停掉后两边离线、重试间隔翻倍，在同一端口重新启动后自动重连、收敛，离线时的修改都在，在线状态立即恢复；错误令牌、没有令牌、令牌不能开这个工程、只读令牌开不存在的工程、工程 ID 不合法时停下等用户处理，不反复重试；只读令牌拿到 `x-bae-access: read`，收得到修改、自己的修改不发；关闭时最后的修改（被 `min_send_interval` 推迟的）和"离开"都送到；Hub 不在时退出的人重连后不再显示，新加入的人马上看到重连的人；连接稳定一段时间后断开，重试间隔从头算；空闲 11 秒链路不断（Hub 的 `Hello` 让客户端知道它还在）。
- **匿名只读**：打开匿名只读后，不带令牌也能连接，但不能新建工程。
- **在线状态和建议性锁**：经 Hub 转发；只读客户端也能发布在线状态；断开连接后其他人立刻看到离开。
- **空闲卸载**：空闲的工程被卸载，再次连接时从磁盘重新加载，内容一致。
- **写盘合并与关闭前写出**：拖动 100 步，`hub.log` 只有 3 条记录（不合并时 101 条）。把写盘间隔设成 1 分钟后，拖动后立即退出 Hub（客户端还连着）、最后一个客户端断开、工程被卸载，三种情况下拖动都写进了磁盘，重新加载后每一步都在。
- **10 个客户端的延迟**：一个客户端改一个值，测到其他 9 个全部收到为止，中间故意让 Hub 安静下来，覆盖放慢轮询的情况。本机回环、debug 构建下中位数约 5.5 ms，最大约 21 ms（验收要求局域网 < 100 ms）。
- **数据目录锁**：同一个数据目录不能启动两个 Hub。
- **多进程**（`tests/multiprocess.rs`）：真实的 `bae-hub` 程序和每个客户端各占一个进程（客户端是测试程序自己再启动一次），进程用 `Child::kill` 直接杀掉。测试进程死掉时不留下 Hub 进程；测试失败时也不留下（丢弃子进程时先关它的标准输入、等 5 秒再杀，守护 Hub 的子进程来得及先杀掉 Hub）。
  - 编辑中途杀掉 Hub，在同一个数据目录和端口重新启动：客户端离线继续编辑，重连后收敛，不丢修改。客户端都退出后再杀一次、重启，新客户端只能从 Hub 的磁盘拿到全部修改。
  - 杀掉客户端，用同一个 ID 从保存的状态（离线时保存，Hub 没见过）或从头重新启动：追上进度，经过 Hub 的修改、保存过的修改都在。
  - 经一个会切断连接（有时切在一帧中间）、两个方向都延迟的代理连接：反复重连，故障停止后收敛。
  - 说明见 `docs/spec/sync.md` 3.1。
