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
| 单条消息上限（MiB）：发给客户端的、可写客户端发来的 | `[limits]` `max_message_mib` | — | — | 256 |
| 流量控制（MiB）：给某个客户端排队的数据超过这么多时先不发工程数据 | `[limits]` `max_backlog_mib` | — | — | 16 |
| 给一个客户端排队的数据最多这么多（MiB），超过就断开 | `[limits]` `max_unsent_mib` | — | — | 528 |
| 排队超过 `max_backlog_mib` 又一直不读的客户端几秒后断开 | `[limits]` `stall_timeout_secs` | — | — | 30 |
| 只读连接发来的单条消息上限（KiB） | `[limits]` `read_only_max_message_kib` | — | — | 1024 |

令牌：

```toml
[[token]]
name = "team"            # 日志和状态接口里显示的名字（不显示密钥）
token = "…至少 16 个字符…"
access = "write"          # write：可编辑、可新建工程；read：只能看
projects = ["film-a"]     # 可选：只能打开这些工程；不写或含 "*" 表示全部
```

`[limits]` 一般不用改（含义见下面的"消息大小与内存"）。在代码里是 `HubConfig::limits`（`bae_sync::HubLimits`）和 `HubConfig::read_only_max_incoming`。

一个数据目录同时只能被一个 `bae-hub` 使用（靠 `hub.lock` 保证）。

## HTTP 接口

- `/healthz` 和 `/v1/info` 不需要令牌，其余都需要。
- 令牌的传法：`Authorization: Bearer <令牌>`；不能设置请求头的客户端用查询参数 `?token=<令牌>`。
- 错误响应是 `{"error": "…"}`。所有接口都只接受 `GET`。

| 路径 | 需要令牌 | 说明 |
|---|---|---|
| `/healthz` | 否 | 返回 `ok` |
| `/v1/info` | 否 | 返回 `{"service": "bae-hub", "version", "protocol": 同步协议版本, "sync_path": "/projects/<project>", "anonymous": null / "read" / "write"}`。客户端连接前用它确认对方是 Hub、协议是否兼容、要不要向用户要令牌 |
| `/v1/status` | 是 | 运行时间、已加载的工程数、在线客户端数，以及**当前令牌的名字和权限**（`access`）。客户端用它判断是否只读 |
| `/v1/projects` | 是 | 这个令牌能看到的工程。每个工程有：`id`、`loaded`、`docs`、`peers`（客户端 ID、令牌名、权限、地址、连接时间）、`disk_bytes`、`saving`、消息计数 |
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

客户端就是 `bae_sync::HubTransport::connect(url, Some(令牌), 超时)`，外面套 `ThreadedTransport`（见 `docs/spec/sync.md` 3.1）。

- **工程 ID**：1–64 个 ASCII 字母、数字、`_`、`-`，同时也是服务端的目录名。用可写令牌连接一个不存在的工程会新建它。
- **帧**：每条同步协议消息是一个二进制帧，编码见 `docs/spec/sync.md` 第 2.3 节。
- **Hub 侧会话**：每个连接一个会话，`hello_interval` 为 5 秒，收到修改立即转发。
- **在线状态**（`Presence`）由 `Relay` 转发：新连接马上收到所有人的状态；连接断开时，Hub 替它向其他人发"离开"。
- **只读连接**：可以发 `Hello`、`SnapshotRequest`、`Presence`。发来的 `Updates`、`Snapshot` 会被丢弃，并在日志里记一次。只读连接发来的单条消息最大 1 MiB（`read_only_max_message_kib`），更大的读到帧头就断开连接，不会先收进内存；发给它们的消息照样可以很大。
- **超时**：Hub 60 秒没有收到某个客户端的任何消息，就断开这个连接。正常客户端每 5 秒发一次 `Hello`，所以只有死掉的链路（比如合上盖子的笔记本）才会被断开。
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

- **备份**：复制整个数据目录即可。最好先停服务，或者用 NAS 的快照功能。
- **意外断电**：Hub 丢了最后一点数据也没关系。客户端重连时双方交换版本，客户端会把 Hub 缺的部分重新上传。

## 实现

| 文件 | 内容 |
|---|---|
| `src/server.rs` | 一个线程阻塞地 `accept`。每个新连接交给一个短命线程：先 `peek` 请求头（不读走），普通 HTTP 请求直接回答；同步请求先做鉴权和路由，再把原封不动的套接字交给 `HubTransport::accept(stream, None)` 完成握手，然后挂到工程的 Relay 上。 |
| `src/project.rs` | 一个工程 = 一个线程上的 `Relay`。工程目录通过 `DirTransport`（放在 `ThreadedTransport` 后台线程里）作为 Relay 的一个对端接上，所以读盘、写盘、压缩、修复都是 `bae-sync` 的代码。打开工程时一直 pump，直到目录发来 `Hello`（它排在所有记录之后），才开始接受连接。目录这个对端单独设置（`Relay::connect_with`）：最多每 300 ms 写一次（`HubConfig::store_send_interval`），拖动时不会每秒追加几十次。最后一个客户端断开时立即 `Relay::flush()` 把还没写的写出去；关闭工程（卸载、退出）前再 `flush` 一次，然后 `Relay::close_peer` 等存盘线程写完，写盘失败记进日志。 |
| `src/link.rs` | 包在 `HubTransport` 外面：丢弃只读连接的修改；60 秒无消息就断开；把 WebSocket 的积压报告给会话（流量控制）。 |
| `src/registry.rs` | 工程按需加载；10 分钟没人连接就卸载（数据都在磁盘上）。 |
| `src/auth.rs` | 令牌只保存 BLAKE3 哈希，比较用常数时间。 |
| `src/config.rs` | `hub.toml`、环境变量。 |

**链路出错时不丢修改。** 客户端发完最后的修改马上断开时，`SyncSession::pump` 先导入这一轮已经收到的修改，再报告传输错误（spec 2.4），`Relay` 再多转一轮，把它们转给其他连接和工程目录。所以 `PeerLink` 出错时直接报告，不需要绕行。

**消息大小与内存**（`bae_sync::HubLimits`，spec 3.1）：

- 单条消息（也是单帧）默认最大 256 MiB，一个工程层的快照要放得下。
- **流量控制**：给某个客户端排队、等它读的数据超过 16 MiB 时，Relay 先不给它发工程数据（照常收它的消息、回 `Hello`、转发在线状态），等它读下去再接着发。客户端每 5 秒一个 `Hello`，读得慢的客户端（比如经 VPN 第一次同步大工程）还没收完时，以前每个 `Hello` 都会让 Relay 把它缺的全部再排一份，NAS 很快内存不够。现在每个连接最多排队 16 MiB 加一条消息。
- 不管怎样，给一个客户端排队的数据不超过 528 MiB，超过就断开它（守流量控制的会话到不了这里）。
- 排队超过 16 MiB、并且 30 秒内一个字节都没读的客户端被断开；还在读的不断开，读得再慢也能收完。

## 测试

`cargo test -p bae-hub`。客户端用的是应用自己的 `HubTransport::connect` 加 `ThreadedTransport`，所以这些测试同时验证了客户端和服务端能互通。

- **收敛与重启**：3 个客户端经 Hub 随机编辑后收敛。Hub 重启后，新加入的客户端只从 Hub 的磁盘就拿到了全部内容。
- **HTTP 接口与令牌**：
  - 没有令牌或令牌错误：401；只读令牌新建工程：404；限定工程的令牌打开别的工程：403；工程 ID 不合法：400。
  - 不带 WebSocket 升级头访问同步地址：426。
  - `?token=` 可以代替请求头。
  - `/v1/projects` 只列出令牌能看到的工程。
- **只读**：只读客户端收得到别人的修改，自己的修改不会传出去；状态接口里显示它是只读。只读客户端发来超过上限的消息时连接被断开，它照样收得到 1 MiB 的修改，可写客户端不受影响（`read_only_connections_cannot_send_large_messages`）。
- **匿名只读**：打开匿名只读后，不带令牌也能连接，但不能新建工程。
- **在线状态和建议性锁**：经 Hub 转发；只读客户端也能发布在线状态；断开连接后其他人立刻看到离开。
- **空闲卸载**：空闲的工程被卸载，再次连接时从磁盘重新加载，内容一致。
- **写盘合并与关闭前写出**：拖动 100 步，`hub.log` 只有 3 条记录（不合并时 101 条）。把写盘间隔设成 1 分钟后，拖动后立即退出 Hub（客户端还连着）、最后一个客户端断开、工程被卸载，三种情况下拖动都写进了磁盘，重新加载后每一步都在。
- **10 个客户端的延迟**：一个客户端改一个值，测到其他 9 个全部收到为止，中间故意让 Hub 安静下来，覆盖放慢轮询的情况。本机回环、debug 构建下中位数约 5.5 ms，最大约 21 ms（验收要求局域网 < 100 ms）。
- **数据目录锁**：同一个数据目录不能启动两个 Hub。
