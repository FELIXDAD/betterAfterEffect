# 09 · 实时协同与版本管理

| 项 | 内容 |
|---|---|
| 分支 | `collab/sync` |
| Orca 工作区 | `bae-09-sync` |
| 阶段 | P1 |
| 依赖 | 02 |
| 负责目录 | `crates/bae-sync/` |

## 目标

让多人（以及 Agent）同时编辑同一个工程并实时看到彼此的修改；提供完整的版本管理；不需要服务器也能在 NAS 共享目录上协作。

## 要做

- **同步协议**：以 Sublayer 为单位交换 Loro 增量；交换版本向量、按需拉取缺失部分。传输层抽象成 `Transport`，实现两种：共享目录、WebSocket（服务端在 10）。
- **共享目录模式**：
  - 每个客户端只追加写自己的 `updates/<sublayer>/<client-id>.log`，读取别人的日志后合并。
  - 定期把日志压缩进快照；多个客户端同时压缩时不能互相破坏。
  - 文件监听 + 轮询兜底（SMB 的变更通知不可靠）；断网后重连补齐。
- **在线状态**：用户名、颜色、选中的 Prim、播放头位置、正在使用的工作区；在共享目录模式下也要能用（写心跳文件）。
- **版本**：命名版本（记录所有 Sublayer 的版本向量）、分支与合并、按 Prim 的差异、回滚（以新提交的方式回滚，不改写历史）、查看任意时刻的工程（只读）。
- **建议性锁**：声明"我在改这个合成"，其他人收到提示。
- **语义冲突提示**：两个人在短时间内改同一个关键帧时，以最后写入为准，但在界面上提示并保留历史。

## 对外接口

- `SyncSession`：把 Stage 的 Sublayer 接到某个 Transport。
- `Presence` 数据流、`History` API（列表、差异、恢复、命名、分支）。

## 验收

- 测试：同一进程里模拟 N 个客户端，加入乱序、丢包、延迟，最终所有人的工程完全一致。
- 共享目录模式在模拟的 SMB 延迟（1–3 秒）下正常工作；两台机器经 NAS 实测。
- 1 小时连续编辑后，工程目录体积增长受控（压缩生效）。

## 不做

- Hub 服务端和部署（10）、协同相关的界面（11/12 调用本工作流的 API 来做）。

## 进度

- [x] Hub 的 WebSocket 客户端传输（PR E，feature `hub`）：`HubTransport`，回环延迟中位数 2.9 ms；测试里有基于 `Relay` 的参考 Hub
- [x] 同步协议与 Transport 抽象（PR A，#14 已合并）：`Message` 二进制编码、`SyncSession`、`DocSet`、`Relay`（Hub 端逻辑）、`MemoryTransport`；规格见 `docs/spec/sync.md`
- [x] 共享目录模式（PR B）：`DirTransport`（每客户端追加日志、校验和、撕裂尾部修复、只压缩自己的日志、文件锁、基础快照也作为来源）、`ThreadedTransport`（I/O 放后台线程）、可选 `watch` feature（notify 文件监听 + 轮询兜底）、会话按文档批量导入；模拟 SMB 1–3 秒延迟下收敛；一小时编辑 `updates/` 只有 26 KB
- [x] 在线状态与建议性锁（PR D）：`PresenceState`（用户、颜色、选中、播放头、工作区、`editing` 建议性锁）、心跳 + 接收方时钟判断离线、Hub 转发/新人同步/断线通知、共享目录心跳文件
- [x] 版本（PR C）：命名版本（`History`，同步的 Loro 文档 + `history.loro`）、任意版本的只读视图、按 Prim 的差异（新增/删除/修改/移动，含作者和时间）
- [x] 回滚（PR F）：`History::revert_to` / `history::revert`，以新的撤销单元恢复整个工程、一个工程层或一个 Prim 子树，不改写历史；命令 `version.revert`（`bae_sync::register_commands`）供 Agent 调用
- [ ] 分支与合并
- [x] 多客户端模拟测试：4 个客户端经 Hub、20% 丢包 + 10% 重复 + 乱序延迟，编辑 150 轮后全部收敛；两端直连；Hub 重启后补齐 + 新客户端加入；协作者新建的工程层

### 2026-11-20

- PR A 已合并（#14）。给 02 的请求见 02 任务书"请求"一节：导入性能（增量镜像或批量导入）、记录提交时间、`TreeID` → 路径。02 在 PR E（#17，已合并）里实现了，`Stage` 的 `DocSet::import_batch` 已改用 `Stage::import_updates_batch`（同一轮同一层只重建一次镜像）。
- PR B（共享目录，#22）、PR C（版本，#27）已合并。PR D（在线状态，#34）、PR E（Hub WebSocket 客户端，#37）、PR F（回滚）已提交。还没做：两台机器经 NAS 实测（需要第二台机器）；清理离开的客户端的日志（心跳文件已有，规则见 spec 4.7，待实现）。

### 2026-11-26

- 10 的请求 1、2、3、4 已完成（分支 `collab/hub-reliability`）：
  - `pump` 遇到传输错误时先导入本轮已收到的数据，再返回错误；这一轮的事件和计数留到下一次 `pump`，或用 `take_interrupted()` 取。Relay 把出错连接最后发来的数据在同一次 `pump` 里转发出去。
  - `Relay::connect_with(transport, config)` 按连接设置会话；Hub 存盘用的对端节流 250 ms。
  - `HubLimits`：单条消息和单帧上限都是 256 MiB（两端默认一致），`connect_with` / `accept_with` 可以改。超过上限的消息发送前就拒绝。
  - 写缓冲上限默认"消息上限 + 16 MiB"，至少能放下一条最大的消息；满了 `send` 报错，Relay 马上断开这个连接。
  - 测试：`tests/transport_errors.rs`（出错前收到的数据被导入，结果只报告一次；旧代码上第一条测试失败）、`tests/relay.rs`（`connect_with` 的节流只作用于那一个连接）、`tests/hub_limits.rs`（20 MiB 快照双向传输；超限被拒；不读的对端让 `send` 失败、被 Relay 断开。用 tungstenite 默认设置时这几条都失败）。
  - 还没解决：一次 `pump` 给慢的对端发的总量超过写缓冲上限（比如新客户端第一次同步好几个大工程层）时也会断开；一个文档超过 256 MiB 时一条消息放不下。见 spec 3.1。

## 请求（其他工作流在这里留言）

### 来自 10 hub-nas（2026-11-20）

bae-hub 已经直接建立在 `Relay` + `HubTransport::accept` 上（PR #47），协议没有改动，测试里的客户端就是 `HubTransport::connect` + `ThreadedTransport`。约定见 `services/bae-hub/README.md`：

- **地址**：`ws://<主机>:7341/projects/<工程 ID>`，端口沿用 spec 3.1 的例子。工程 ID 1–64 个 ASCII 字母、数字、`_`、`-`。
- **鉴权与路由**：
  - 鉴权和路由由 Hub 在握手前自己做，支持多个令牌、只读/可写、限定工程。
  - 之后调用 `accept(stream, None)` 完成握手。
  - 拒绝时返回 401 / 403 / 404 / 400，`connect` 报的错误里带状态码。
  - 客户端想知道自己是不是只读，可以 `GET /v1/status`（带令牌），看返回里的 `access`。
- **超时**：Hub 60 秒没收到客户端任何消息就断开，所以客户端会话的 `hello_interval` 不要设成 `None`。

请求（按重要程度）：

1. **`SyncSession::pump_at` 在传输出错时丢掉本轮已收到的数据（会丢修改）。**（已完成，collab/hub-reliability）
   - 做法：出错时先导入本轮已收到的，再返回错误；这一轮的结果留到下一次 `pump`，或用 `take_interrupted()` 取。见 spec 2.4。
   - 现象：`transport.recv()?` 直接返回错误，本轮已经放进 `incoming` 的 `Updates` 没有导入。客户端发完最后几条修改马上断开时，Hub 收到了修改和关闭帧，但修改没有导入；而客户端的会话已经认为 Hub 有了这些修改。只有这个客户端以后重连，才会补发。
   - 复现：`services/bae-hub/tests/hub.rs` 的 `idle_projects_are_unloaded_and_reloaded`，去掉 `src/link.rs` 里的绕行就会超时。
   - 建议：遇到传输错误时先 `import` 已经收到的，再返回错误。Hub 现在在 `PeerLink` 里绕开了：出错时先返回 `Ok(None)`，下一次再报错。客户端一侧同样受影响，只是影响较小（重连后会补齐）。
2. **WebSocket 消息大小上限。**（已完成，collab/hub-reliability）`HubTransport` 用的是 tungstenite 的默认 `WebSocketConfig`：单帧最大 16 MiB，单条消息最大 64 MiB。tungstenite 发送时不会自动分片，所以一个工程层的快照或首次上传超过 16 MiB 时会被对方拒绝，连接断开，重连后又是同一条消息，会一直卡住。建议 `accept` / `connect` 把 `max_frame_size` 设成和 `max_message_size` 一样大（比如 256 MiB），或者让调用方传入配置。
   - 做法：`HubLimits`，单帧上限 = 单条消息上限，默认 256 MiB，客户端和 Hub 一致；`HubTransport::connect_with` / `accept_with` 传入，Hub 用 `hub.toml` 的 `max_message_mib` 配置。超过上限的消息发送前就拒绝，报错写明原因。见 spec 3.1。
3. **写缓冲上限**（已完成，collab/hub-reliability）（不急）。`send` 遇到 `WouldBlock` 时把数据留在 tungstenite 的写缓冲里，默认没有上限。一个只发不收的客户端会让 Hub 的内存一直涨，Hub 只能等 60 秒超时。如果能设 `max_write_buffer_size`，满了返回错误，Relay 就会马上断开它。
   - 做法：`HubLimits::max_write_buffer_size`，默认消息上限 + 16 MiB，至少是 tungstenite 的写缓冲 + 一条最大的消息。满了 `send` 返回 `SyncError::Hub("write buffer full: the peer is not reading")`，Relay 马上断开这个连接。见 spec 3.1。
4. **按连接设置 `SessionConfig`**（已完成，collab/hub-reliability）（不急，优化）。比如 `Relay::connect_with(transport, config)`：Hub 想给存盘用的对端（`DirTransport`）单独设 `min_send_interval` 为几百毫秒，拖动时少写几次盘。不影响正确性。
   - 做法：`Relay::session_config()`、`connect_with(transport, config)`、`peer_config_mut(handle)`，见 spec 第 3 节。Hub 存盘用的对端节流 250 ms，关闭工程前去掉节流。
