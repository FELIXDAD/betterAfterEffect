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
- [x] 差异和回滚按节点身份（G0，分支与合并的前置）：`identity.rs` 的 `NodeIndex`（TreeID → 父节点、原始名、兄弟序号、spec）和 `Matching`（同一 TreeID，加"重建别名"）；`diff` 和 `revert` / `version.revert` 不再按路径匹配 Prim，镜像因并发同名新建而在客户端之间不一致时，两边算出的差异和回滚结果相同；恢复时与留下的同名兄弟冲突用旧节点自己的后缀 `<名字>_<peer>_<counter>`（按重建别名仍对上旧节点，回滚可重复执行）；`Prim` 范围也按节点判断。镜像不一致本身是 bae-model 的问题，已报给 02
- [x] 语义冲突提示（#737 子任务 5 的引擎部分，界面另做）：`ConflictTracker`（`conflicts.rs`，只观察不修改）。在线：按元素（值、表达式、某时间的关键帧）记住自己最近的修改，远端导入覆盖了它报 `Overridden`（含胜出作者、是否真正同时），我们胜出时报较轻的 `Kept`，Prim 被删报 `PrimRemoved`；只解码新到操作里写了相关 map 的部分，大导入不扫描整个工程。离线：`connection_lost` / `caught_up`（`SyncSession::is_caught_up` 判断补齐）给出 `MergeSummary`：保留的、被覆盖的、新建和删除的 Prim 的结果、别人改了什么（按节点身份的 `history::diff`）。Hub 和共享目录都可用。评审后补上：断开前没送出去的修改补齐时照样提示；本地事件晚于导入交给 `observe` 也不漏报；共享目录重连先重读目录再回 `Hello`，补齐分几次 pump 时 `is_caught_up` 不会提前为真；补齐中途又改了东西时 `merged` 不丢别人的修改；别人失败事务的回滚胜出时报告；自己也删了的 Prim 不算被别人删；连续冲突按 `repeat_quiet` 只报一次；空闲检查只比较 frontiers，不随记着的修改数和 peer 数增长。规格见 spec 6.4
- [ ] 分支与合并
- [x] 多客户端模拟测试：4 个客户端经 Hub、20% 丢包 + 10% 重复 + 乱序延迟，编辑 150 轮后全部收敛；两端直连；Hub 重启后补齐 + 新客户端加入；协作者新建的工程层

### 2026-11-20

- PR A 已合并（#14）。给 02 的请求见 02 任务书"请求"一节：导入性能（增量镜像或批量导入）、记录提交时间、`TreeID` → 路径。02 在 PR E（#17，已合并）里实现了，`Stage` 的 `DocSet::import_batch` 已改用 `Stage::import_updates_batch`（同一轮同一层只重建一次镜像）。
- PR B（共享目录，#22）、PR C（版本，#27）已合并。PR D（在线状态，#34）、PR E（Hub WebSocket 客户端，#37）、PR F（回滚）已提交。还没做：两台机器经 NAS 实测（需要第二台机器）；清理离开的客户端的日志（心跳文件已有，规则见 spec 4.7，待实现）。

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

1. **`SyncSession::pump_at` 在传输出错时丢掉本轮已收到的数据（会丢修改）。**
   - 现象：`transport.recv()?` 直接返回错误，本轮已经放进 `incoming` 的 `Updates` 没有导入。客户端发完最后几条修改马上断开时，Hub 收到了修改和关闭帧，但修改没有导入；而客户端的会话已经认为 Hub 有了这些修改。只有这个客户端以后重连，才会补发。
   - 复现：`services/bae-hub/tests/hub.rs` 的 `idle_projects_are_unloaded_and_reloaded`，去掉 `src/link.rs` 里的绕行就会超时。
   - 建议：遇到传输错误时先 `import` 已经收到的，再返回错误。Hub 现在在 `PeerLink` 里绕开了：出错时先返回 `Ok(None)`，下一次再报错。客户端一侧同样受影响，只是影响较小（重连后会补齐）。
2. **WebSocket 消息大小上限。** `HubTransport` 用的是 tungstenite 的默认 `WebSocketConfig`：单帧最大 16 MiB，单条消息最大 64 MiB。tungstenite 发送时不会自动分片，所以一个工程层的快照或首次上传超过 16 MiB 时会被对方拒绝，连接断开，重连后又是同一条消息，会一直卡住。建议 `accept` / `connect` 把 `max_frame_size` 设成和 `max_message_size` 一样大（比如 256 MiB），或者让调用方传入配置。
3. **写缓冲上限**（不急）。`send` 遇到 `WouldBlock` 时把数据留在 tungstenite 的写缓冲里，默认没有上限。一个只发不收的客户端会让 Hub 的内存一直涨，Hub 只能等 60 秒超时。如果能设 `max_write_buffer_size`，满了返回错误，Relay 就会马上断开它。
4. **按连接设置 `SessionConfig`**（不急，优化）。比如 `Relay::connect_with(transport, config)`：Hub 想给存盘用的对端（`DirTransport`）单独设 `min_send_interval` 为几百毫秒，拖动时少写几次盘。不影响正确性。
