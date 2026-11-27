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
- [x] 清理离开的客户端（spec 4.7）：默认关闭（`inactive_after: None`），用 `DirConfig::RECOMMENDED_INACTIVE_AFTER`（3 天）打开，Hub 给自己的工程目录打开了，应用等 4.8 的 NAS 实测之后再打开；别人的日志在主人离开（超过 `inactive_after` 日志、心跳都没动且锁没人持有）、并且被另一个以快照开头的日志完全包含时删除，两个完全相同的只删 ID 小的；没有容器时让自己的日志接管（向会话要快照）；过期临时文件、离开的客户端的锁文件也清掉；被误删的正在运行的客户端在下一次追加时自己修复（追加后再检查一次长度）；压缩只用包含整个日志的快照覆盖自己的日志，会话导入本轮数据后才回答快照请求
- [x] 在线状态与建议性锁（PR D）：`PresenceState`（用户、颜色、选中、播放头、工作区、`editing` 建议性锁）、心跳 + 接收方时钟判断离线、Hub 转发/新人同步/断线通知、共享目录心跳文件
- [x] 版本（PR C）：命名版本（`History`，同步的 Loro 文档 + `history.loro`）、任意版本的只读视图、按 Prim 的差异（新增/删除/修改/移动，含作者和时间）
- [x] 回滚（PR F）：`History::revert_to` / `history::revert`，以新的撤销单元恢复整个工程、一个工程层或一个 Prim 子树，不改写历史；命令 `version.revert`（`bae_sync::register_commands`）供 Agent 调用
- [x] 差异和回滚按节点身份（G0，分支与合并的前置）：`identity.rs` 的 `NodeIndex`（TreeID → 父节点、原始名、兄弟序号、spec）和 `Matching`（同一 TreeID，加"重建别名"）；`diff` 和 `revert` / `version.revert` 不再按路径匹配 Prim，镜像因并发同名新建而在客户端之间不一致时，两边算出的差异和回滚结果相同；恢复时与留下的同名兄弟冲突用旧节点自己的后缀 `<名字>_<peer>_<counter>`（按重建别名仍对上旧节点，回滚可重复执行）；`Prim` 范围也按节点判断。镜像不一致本身是 bae-model 的问题，已报给 02
- [x] 连接管理（`collab/sync-connection`，epic #737 子任务 3 的引擎部分）：`Connection`（共享目录 / Hub 目标、后台连接、断线按退避重连、401/403/404/ClientId 被占用时停下等用户处理、读写权限、状态栏状态、在线状态、`unknown_docs`、不阻塞的关闭）；Hub 握手响应带 `x-bae-access`；只读时关掉 `SessionConfig::send_data`；`DirTransport::open_read_only`；规格见 spec 第 7 节
- [x] 语义冲突提示（#737 子任务 5 的引擎部分，界面另做）：`ConflictTracker`（`conflicts.rs`，只观察不修改）。在线：按元素（值、表达式、某时间的关键帧）记住自己最近的修改，远端导入覆盖了它报 `Overridden`（含胜出作者、是否真正同时），我们胜出时报较轻的 `Kept`，Prim 被删报 `PrimRemoved`；只解码新到操作里写了相关 map 的部分，大导入不扫描整个工程。离线：`connection_lost` / `caught_up`（`SyncSession::is_caught_up` 判断补齐）给出 `MergeSummary`：保留的、被覆盖的、新建和删除的 Prim 的结果、别人改了什么（按节点身份的 `history::diff`）。Hub 和共享目录都可用。评审后补上：断开前没送出去的修改补齐时照样提示；本地事件晚于导入交给 `observe` 也不漏报；共享目录重连先重读目录再回 `Hello`，补齐分几次 pump 时 `is_caught_up` 不会提前为真；补齐中途又改了东西时 `merged` 不丢别人的修改；别人失败事务的回滚胜出时报告；自己也删了的 Prim 不算被别人删；连续冲突按 `repeat_quiet` 只报一次；空闲检查只比较 frontiers，不随记着的修改数和 peer 数增长。规格见 spec 6.4
- [ ] 分支与合并
- [x] 多客户端模拟测试：4 个客户端经 Hub、20% 丢包 + 10% 重复 + 乱序延迟，编辑 150 轮后全部收敛；两端直连；Hub 重启后补齐 + 新客户端加入；协作者新建的工程层

### 2026-11-20

- PR A 已合并（#14）。给 02 的请求见 02 任务书"请求"一节：导入性能（增量镜像或批量导入）、记录提交时间、`TreeID` → 路径。02 在 PR E（#17，已合并）里实现了，`Stage` 的 `DocSet::import_batch` 已改用 `Stage::import_updates_batch`（同一轮同一层只重建一次镜像）。
- PR B（共享目录，#22）、PR C（版本，#27）已合并。PR D（在线状态，#34）、PR E（Hub WebSocket 客户端，#37）、PR F（回滚）已提交。还没做：两台机器经 NAS 实测（需要第二台机器）；清理离开的客户端的日志（心跳文件已有，规则见 spec 4.7，待实现）。

### 2026-11-26

- 10 的请求 1–4 已处理（见下面"请求"一节的说明）。
- 评审时发现的老问题：`DirTransport` 某次写日志失败后，后面的追加照样成功，日志里就有了空洞，而目录的 `Hello` 声称没有（记录的版本覆盖它之前的一切），重新挂上的会话不会补写。现在写失败后这个文档不再追加，改为请求快照把日志整个重写（spec 4.3）。
- `Transport::close()`：`ThreadedTransport` 的 `send` 只是排队，`close` 等后台线程交付完，返回还没报告的错误（spec 2.6）。
- 评审后又改了几处：
  - `HubLimits` 改为按有没有进展判断对方是否还在读（积压超过 `max_backlog` 并且 `stall_timeout` 内一个字节都没发出去才断开）。原来按积压总量（约 272 MiB）断开，读得慢的新客户端第一次同步大工程时会被断开，重连后再发同一批，永远同步不完（spec 3.1）。
  - `pump` 开头的定期 `Hello` 发不出去时也先收、先导入；`HubTransport::recv` 写失败时先读完已经收到的消息再报错（spec 2.4、3.1）。
  - 发送时某个文档出错，同一轮的其他文档照样发。原来排在后面的文档已经算作发出却没交给传输层：共享目录里它们的日志会留下空洞（spec 2.4、4.3）。
  - `HubTransport::close` 等对方确认关闭；`ThreadedTransport::close` 会关闭内层传输（原来只等队列交出去，`HubTransport` 缓冲区里剩下的随丢弃一起丢掉，`close` 却返回成功）；`DirTransport::close` 在日志落后时报错（spec 2.6、3.1、4.3）。
  - 快照请求的退避：每次翻倍（原来重写失败时又翻一倍，实际每轮 4 倍），起点不超过 60 秒，每次 `recv` 都检查（原来只在轮询目录时检查，轮询间隔 1 小时就要等 1 小时）。
- 清理离开的客户端的日志已实现（spec 4.7，分支 `collab/sync-log-cleanup`）。还没做：两台机器经 NAS 实测。
- 评审后修正：压缩只用包含整个自己日志的快照覆盖日志，否则追加（重启后第一轮清理就"接管"时，会话还没导入自己日志的记录，旧实现会用从已保存工程导出的快照覆盖日志、丢掉没保存的修改和它所容纳的别人的操作）；会话在本轮导入之后才回答 `SnapshotRequest`；锁探测以读写方式打开（NFS）；时间按服务器时钟；日志摘要增量读取，容器检查不再整个重读；先读离开的客户端的日志。
- 第二轮评审后修正（spec 2.4、4.3、4.7、6.2）：
  - 会话把本轮已经取出的 `SnapshotRequest` 全部回答，收消息出错或某个回答发不出去也一样（原来后面的请求被丢掉，共享目录每次压缩只请求一次，那个日志就不再压缩；修复请求丢了，日志会一直是一条新客户端导入不了的增量）。传输层也不再依赖每个回答都到：日志被删或被换掉后的修复走"写失败之后"的流程（发现的那条增量不单独写，不再追加，按退避重复请求；原来只写这一条增量，日志声称有它没有的操作，会话断开后谁也不补）；压缩请求没有回答时，日志长一倍再请求。
  - 第一次扫描漏掉的自己的日志（列目录出错），在写之前先读回来；读不了就不写。原来下一次追加或压缩会在它上面新建文件，丢掉没保存的修改和它代替被删日志保存的别人的操作。
  - 锁探测改用只读句柄拿共享锁（只要读权限；NFS 上也和排它锁冲突），打不开时记在 `last_error`；`open` 等锁 2 秒（原来 200 毫秒，比经 VPN 删除一个日志时持锁的时间短）；心跳文件的检查放在拿锁之前，持锁期间只看一眼日志、删除。
  - 心跳文件超过一天才删，这个"一天"按服务器时钟算。
  - 两个清理者真正交错的单元测试（原来的集成测试里一个清理者先删完了）。
  - 修了 PR1 和清理分支交叉时漏掉的测试编译错误（`Client::collide`）。
- 写清理的压力测试时发现一个可能属于 02 的问题（还没转给 02）：几个客户端并发创建同名的 Prim 后，两个客户端的 Loro 版本向量完全相同，镜像（`to_trees`）里的命名却不同（一个叫 `shared`，另一个叫 `shared_<peer>_<counter>`），大约每 5 次出现 1 次。猜测和"本地操作增量更新镜像、远端导入整体重建"有关，未确认。同步本身已经收敛，只是看到的路径不同；关掉清理也一样出现。压力测试里避开了同名创建。
- 连接管理（`collab/sync-connection`，叠在 `collab/sync-hub-requests` 上）：`bae_sync::Connection`，应用（11）和 Agent 在持有 Stage 的线程上每帧 `pump`，连接、握手、丢弃断开的链路、关闭都在后台线程（spec 第 7 节）。
  - Hub 拒绝握手改为 `SyncError::HubRefused { status, message }`；握手响应带 `x-bae-access`（`HubTransport::accept_granting` / `access()`），10 的 Hub 已经发送。
  - 只读：会话不发任何文档数据（原来是 `SessionConfig::read_only`，合并 #808 后统一成 dev 的 `SessionConfig::send_data`）；`DirTransport::open_read_only`（`updates/` 不可写的共享目录，什么都不写）。
  - 顺带修了一个老问题：重连后 `reset()` 没有重新发布自己的在线状态，而且 Hub 替断开的连接发"离开"用的序号正好是重连后的下一条，别人要等下一次心跳（5 秒）才重新看到我们。现在 `reset()` 立即重新发布、跳过那个序号、清空旧的在线列表（spec 6.2）。
  - 还发现一个竞争：丢弃或关闭共享目录的连接后立即重新分享同一个目录，旧链路还没放开文件锁，新连接报 `ClientIdInUse`。现在本进程正在放开的链路按"目录 + `ClientId`"登记，连接线程先等它们放开；其他情况这个错误持续 2 秒以上才算要用户处理的失败。
  - 评审后补的（spec 7.2、7.4、7.7）：连接中 `close` 或丢弃时，连接线程做好的链路也在后台线程丢弃；断线后交给链路、对方没确认的修改算 `unsynced`（`SyncSession::forget_unconfirmed`），退出时会提示；Hub 静默 20 秒（`hub_idle_timeout`）算断开；共享目录 I/O 卡住超过 `io_stall` 时交出去的修改算 `unsynced`（`ThreadedTransport::stalled`）；`retry()` 不连接不能用的 URL；`wake` 只在后台线程调用；闹钟不会丢掉之后的唤醒；`Debug` 不打印 URL 里的 `?token=`；Hub 地址解析出多个时逐个尝试、IPv4 优先；`Access` 移到 `transport.rs`。
  - 合并 dev（#782、#783 已合并）：只读打开的 `DirTransport` 不清理（spec 4.5、4.7）：不压缩、不接管、不删任何文件（过期的心跳文件也不删），也不写探测服务器时钟的临时文件（`a_folder_opened_read_only_cleans_nothing_up`）。`SnapshotRequest` 照 #783 在本轮导入之后回答，只读会话不回答；回答的快照和增量一样记为"可能还在路上"（2.4）。`open` 现在自己等锁 2 秒（`lock_client`），所以另一个进程占着同一个 `ClientId` 时，连接管理要大约 4.5 秒（两次各等 2 秒的尝试，中间退避 0.5 秒）才报 `ClientIdInUse`。
  - 合并 dev（#789、#808）：#808 的 `SessionConfig::send_data` 和本分支的 `read_only` 意思相同，只留 `send_data`（已经在 dev 里发布）。`Connection` 在对方只给 `Read` 时关掉它，配置里设成 `false` 的一直不发。不发的数据、被流量控制挡下的文档都没交给传输，不记为"在路上"（`documents_held_back_are_not_on_their_way`）。`ThreadedTransport` 的积压（#808）和本分支的 `stalled()` 合在一个 `Backlog` 里。Hub 握手按权限给上限（`client_limits`）并带 `x-bae-access`；只读令牌的 `Connection` 在 Hub 只读上限 64 KiB、本地改了 256 KiB 时也不被断开。
  - #798 评审跟进 1（spec 7.5）：`DirTransport::open` 报 `PermissionDenied` 不再直接算只读（Windows/SMB 上"等待删除"的锁文件也是这个错误）：再在 `updates/` 里建、删一个探测文件，建不了才只读，建得了就当普通失败重试。只读的共享目录链路每隔 `Backoff::failed_retry` 再探测一次，能写了就重新连接（可写）。
  - #798 评审跟进 2（spec 7.3、7.4）：连接超过 `ConnectionConfig::unresponsive_after`（默认 10 秒）还没在线时，状态是新的 `ConnectionState::Unresponsive { since }`（NAS 卡在打开、扫描或等旧链路放开时不再一直显示"连接中"）；连接线程照样等，闹钟按时唤醒一次。
  - 合并 #808 后的评审跟进（spec 2.4、3.1、4.1、7.4、7.5、7.7、7.8）：`close` 把 `flush` 导出的数据交给关闭线程按 `max_backlog` 发（一次全交给链路时，大工程刚分享就关闭会超过 Hub 链路的 `max_unsent`，关闭失败）；Hub 的连接超过 2 × `hub_timeout` 加 1 秒才算 `Unresponsive`（以前每次超时重试都在 `Offline` 前闪一下）；锁文件连续 3 次被拒而目录能写时改为只读加入，只读链路的定期探测也看锁文件能不能打开（以前只读的锁文件让连接一直离线）；可写打开时删掉自己留下的探测文件（清理默认关闭，以前说"清理会删"不对）；规格 2.4 第 4 步和 3.1 改成：重发由"在路上"记录挡住，流量控制管积压，`a_slow_reader_is_not_flooded_by_its_own_hellos` 要求发出不到 1.25 倍；Hub README 写明握手用 `client_limits`。
  - 合并 dev（#791、#806）（spec 2.4、4.1、4.3、4.5、6.4、7.4、7.5、7.8）：#791 的 `remove_leftovers`（拿到锁才删自己的 `<client-id>.tmp-*`，`updates/**` 和 `presence/`）和本分支的 `remove_own_probes`（只删 `updates/` 下的，不论有没有拿到锁）做的是同一件事，只留前者；写权限探测的文件名（`write_probe_path`）就是它删的那种，单元测试 `our_leftover_probe_files_are_removed_on_open` 覆盖。#806 的 `is_caught_up` / `remote_versions` 改读对方确认过的版本（`confirmed`），不读含"在路上"部分的 `remote`：问的是对方有的我们是否都导入了，两者结果相同（我们发出的本来就在本地版本里），确认过的这份不依赖这一点。断线（`forget_unconfirmed`）之后、下一次 `reset` 之前它还按旧链路回答，所以接在 `Connection` 上时先看 `is_online()`（spec 6.4 写了怎么接，Hub 测试 `the_offline_summary_comes_once_the_new_link_caught_up` 走了一遍断开、两边离线修改、重连补齐、汇总）。共享目录收到会话的 `Hello` 立刻重读目录（#806）对只读打开同样适用。

- 同一台机器上的多进程回环测试（epic #737 子任务 7 第一部分，`collab/sync-multiprocess-tests`）：客户端是测试程序自己再启动一次，不另外发布程序。
  - Hub 模式（`services/bae-hub/tests/multiprocess.rs`）：真实的 `bae-hub` 进程，杀掉后在同一个数据目录和端口重启；客户端进程被杀后用同一个 `ClientId` 从保存的状态或从头重启；一个会断线、会延迟的代理。
  - 共享目录（`crates/bae-sync/tests/multiprocess_dir.rs`）：4 个进程，编辑中途被杀（测试补上写到一半留下的半条记录和压缩临时文件；新进程先等着锁，再杀旧进程），同一个 `ClientId` 重启后继续；目录大小不超过压缩阈值。
  - 默认测试各约 3 秒，长时间测试在 `--ignored` 里。说明见 spec 3.1、4.6。
  - 评审后：子进程每轮合并排队的唤醒、落后时不追赶编辑（原来一轮只取一条消息，机器忙时唤醒堆在父进程的命令前面，命令 30 秒等不到回答，共享目录的长时间测试每次都这样失败）；测试客户端不定期发 `Hello`，`SyncSession::reset` 失效时测试会失败；Hub 被杀后单独靠磁盘恢复、离线保存后重启、截掉半条记录、等锁这几处原来不管实现对不对都能通过，现在分别检查；`bae-hub` 由一个随测试进程退出的子进程启动，测试进程死掉时不再留下 Hub。
  - 测出的问题：进程在压缩（或写在线状态心跳）时写完临时文件、改名之前被杀，临时文件永远留在目录里，每次在这个时候崩溃就多一个（macOS 上压缩的 `F_FULLFSYNC` 慢，这个窗口不小）。现在 `DirTransport::open` 拿到锁之后删除自己的 `<client-id>.tmp-*`（spec 4.3，回归测试 `temporary_files_of_a_dead_session_are_removed`）。Hub 的工程目录用的是同一份代码。
  - 合并 dev（#782、#783）后：`DirTransport::open` 删自己的临时文件，和 4.7 清理按时间删任何人过期的临时文件并存。前者启动时拿到锁就删、不论新旧，只删自己的；后者要等超过 `inactive_after`（默认 3 天），而且要有客户端在清理（上面"永远留在目录里"是 4.7 合并之前的情况）。两者用同一个目录遍历，删掉的都算进 `DirStats::stale_files_removed`；`lock_client` 现在同时告诉 `open` 是否真的拿到了锁。
  - 合并后跑测试时共享目录的多进程测试失败过一次（约 40 次里 2 次）：90 秒内不收敛，所有进程版本相同、没有没发出去的修改，只有一个进程（被杀后先等锁再启动的那个）的指纹不同。就是上面"并发创建同名的 Prim 后镜像里的命名不同"的问题，不是同步出错。现在这个测试和清理的压力测试一样不并发创建同名的 Prim（`random_edit_with(.., false)`），不收敛时的报错里写出各进程的版本是否相同。Hub 的多进程测试本来就不创建同名的 Prim。
  - 同一个测试在 3 个并行跑的压力下还出现过"先等锁再启动的进程重启后没有压缩过"（30 次里 2 次）：它接手的日志的阈值是上次快照的两倍；大概因为合并 dev 后 `open` 遇到锁被占用先重试 2 秒，旧进程多编辑 2 秒，快照更大；机器忙时同样的 900 次编辑合并成更少的字节，到不了阈值。现在重启后的进程一直编辑到压缩过为止再停（同样的压力下 30 次都通过）。

### 2026-11-27（#782、#783 评审的跟进）

- **Hub 内存上限**（spec 2.4、3.1）：
  - 原来 Hub 连接的写缓冲没有上限，只在对方停滞时断开；读得慢的客户端每 5 秒一个 `Hello`，Relay 就把它缺的全部再排一份。
  - 现在 `Transport::backlog()`（默认 0；`HubTransport` 是写缓冲，`ThreadedTransport` 是队列加内层传输，`PeerLink` 转发）配合 `SessionConfig::max_backlog`（默认 16 MiB）做流量控制：积压超过时不发文档数据，清空后从停下的文档接着发；一条消息不管多大，积压低于上限时都能发出去。
  - `HubLimits` 拆出 `max_incoming`（接收上限），新增硬上限 `max_unsent`（默认 528 MiB，超过就断开）。
  - 读得慢的客户端经 Relay 拿 20 MiB 的工程：最多积压约 3 MiB（没有流量控制时 50–60 MiB）。spec 3.1 写明了真实的上限。
  - 和 #798（`Hello` 带确认）互相独立，合起来时各管各的：流量控制管排队多少，确认管不重发还在路上的。
- **清理（spec 4.7）**：每次轮询最多花 `cleanup_budget`（默认 50 ms），一轮可以分几次轮询做完，从停下的地方接着做，日志每次最多读 8 MiB；每轮最多接管 `absorb_per_pass`（默认 1）个文档，避免一次 `pump` 在界面线程上导出几十个快照。
- **更新的格式版本的日志**（spec 4.3）：自己的不改名、不重写、不追加，这个文档什么都不写（`last_error` 说明，`close` 报错）；自己的日志里有读不懂的记录时只追加、不用快照覆盖；别人的照旧不删。
- **清理默认关闭**：Hub 给自己的工程目录打开；应用等 4.8 的 NAS 实测之后再打开。
- 文档：`DirTransport::open` 可能阻塞最多 2 秒（锁被占用时重试），要在后台线程调用（rustdoc、spec 2.6）；压力测试的 `collide: false` 加了 TODO（同名 Prim 的问题已在 #737 上报给 02）。
- 评审这些跟进时又修了：
  - **只读会话**：`SessionConfig::send_data = false` 时不发任何文档数据（增量、快照回答、`flush` 都不发），`Hello`、在线状态、接收照常（spec 2.4 第 5 步）。应用用只读令牌连 Hub 时设它；Hub 对只读连接的接收上限从 1 MiB 改成 16 MiB（10）。
  - **清理卡住**：分块读摘要时，一条坏记录（校验和或长度不对）后面还有 8 MiB 以上，或者大于 8 MiB 的日志文件头用不了（坏的、更新的格式），每次都从同一个地方重读，这一轮清理永远做不完，时间上限为无限时 `poll` 不返回。现在读到这种地方摘要就到此为止、算不完整，和一次读完的结果一样（spec 4.7）。
  - **唤醒**：`ThreadedTransport` 以前只在积压撑过后台线程一整轮时才在清空后叫醒；最常见的"发一个大文档、其余暂停、一轮就写完"反而叫不醒。现在 `backlog()` 报告过大于 0 就记下，清空时叫醒（spec 2.6）。
  - 慢读者测试的发送量上限放宽到 3 倍工程大小：流量控制看不到系统套接字缓冲，缓冲大的平台（Windows、macOS 的自动调整）上正确的代码也会超过原来的 2 倍。
  - Hub 的清理开关和每个连接的 `max_backlog` 加了单元测试（10）。

### 2026-11-27（#789、#791、#798、#806、#808 评审的跟进，`collab/sync-review-followups-2`）

- **冲突提示（#806）**（spec 6.4）：
  - `ConflictTracker::caught_up` 自己先对补齐进来的内容做一次检查，再算汇总，返回 `CaughtUp { notices, summary }`，调用顺序不会再错（以前先汇总后检查时，断开前没送出去的修改漏报冲突）。出错时离线标记保留，这次检查的提示留到下一次检查返回。
  - `cancel_offline()` 和 `ConflictConfig::catch_up_timeout`（默认不超时，从补齐开始、不从断开时算）：补齐一直完不成（导入失败、Loro 挂起缺依赖的操作）时，在线提示不再永远静默。
  - 评审后（`catch_up_timeout`）：补齐到一半链路又断了、应用再次调用 `connection_lost` 时，计时作废，判断"别人的操作到来"也改从这一次断开时的版本算，下一条链路补齐时重新开始。以前保留第一次的计时：Wi-Fi 时断时续之后再离线一小时，重连后的第一次检查就去掉标记，那一小时没有汇总；离线时每帧检查也会在 30 秒后去掉。离线时加进层栈的、带别人历史的工程层也不再开始计时（从第一次看到它的检查算起）。原来的超时测试在离线期间也检查，现在能分出"从补齐开始"和"从第一次检查开始"（`an_interrupted_catch_up_restarts_the_timeout`、`a_layer_added_offline_does_not_start_the_catch_up_timeout`）。
  - 离线合并汇总要导出、导入旧版本快照（每个变了的工程层最多 3 个，加上 `history::diff` 的 2 个），文档写明不要在界面线程上算；`caught_up_detached` / `DetachedMerge` 在调用线程上只做检查、钉住工程（`Stage::snapshot_reader`，只复制句柄），汇总在后台线程上算，结果和在原 Stage 上算的相同。
  - 给 02 的请求（02 任务书"请求"一节）：公开存储布局的名字或解析辅助函数（`ops.rs` 现在是抄的）、明确的回滚标记（现在靠 `rollback: ` 标签前缀）、导入结果带上"有挂起的操作"（可选）。
- **流量控制和清理（#808）**（spec 2.4、2.6、4.3、4.7、7.3）：
  - 自己的日志是更新的格式版本（降级）的文档：`DirTransport::not_written()`，`Transport::notes()`（`LinkNotes { last_error, not_written }`，`ThreadedTransport` 在每次调用内层传输后按 `notes_seq` 复制），`Connection::link_notes()` 给界面显示，这些文档上的修改算作没同步（状态和 `Closing::unsynced`）。Hub 加载工程时打 warn、`/v1/projects` 列出（10）。
  - 会话每次 `pump` 最多回答 `SessionConfig::max_snapshots_per_pump`（默认 2）个快照请求，其余留到之后的 `pump`（不丢、同一个文档只留一个、`reset` 时丢掉）；`Connection` 有没回答的请求时由闹钟立即再唤醒。
  - 评审后：只留会回答的请求（自己有的文档、`send_data`），对方（包括只读的 Hub 客户端）发多少，队列都不超过文档数（以前每轮收 256 个、只回答 2 个，不存在的文档的请求越积越多，去重越来越慢，拖慢工程的整个 Relay 线程）；`flush`（`Connection::close`、Hub 卸载和退出前的 `Relay::flush`）回答所有还在等的请求（以前关闭时最多再回答 2 个，其余丢掉，共享目录没等到修复快照，日志一直落后）。唤醒测试去掉了 `io_stall`、`unresponsive_after` 的闹钟（以前它们每 5、10 秒叫醒一次，去掉修复测试也通过）。Hub 加载时的 warn 由测试在加载线程上抓日志检查（`RelayThread::start`；以前测试只看 `/v1/projects`，去掉加载时的检查也通过）。
  - 文档：50 ms 的清理时间不是硬上限（列目录、stat、删临时文件、锁文件不计时，每次至少读一块）；含读不懂记录的自己的日志永远不压缩，每长到 4 倍就追加一份快照（只在降级后）。
  - Hub 的 `max_backlog_mib = 0` 和其他上限一样拒绝启动（10）。
- **多进程测试（#791）**（spec 3.1）：丢弃 `Proc` 时先关标准输入、给 5 秒自己退出再杀，等 `listening` 超时的 Hub 启动不再留下 Hub 进程（新测试强制走这条路径）；杀 Hub 的测试一直编辑到重连之后再叫停（原来 400 次编辑在忙的机器上可能不够）；同一端口重启时并行测试抢到端口的极小概率写在注释里。默认测试现在 5 个，本机约 2.3 秒（1 个 CPU 约 6 秒）。
- **按节点身份的回滚（#789）**（spec 5.3、5.4）：按 Prim 回滚时父节点已经不在、又不在范围里，报 `SyncError::RevertParentGone`，指出要先回滚的最上层祖先，什么都不改；`version.revert` 的说明写明按范围回滚会连带删掉移出范围的 Prim 下后来新建的子节点（可撤销）；补了规则 (b) 配对手动重建的同名同类型 Prim 的例子和测试。
- **连接管理（#798 跟进 3、4）**（spec 7.5、7.9）：不发 `x-bae-access` 的旧 Hub（0.0.1）在连接线程上再 `GET /v1/status` 查权限，只读令牌也变成 `Read`；接入应用的注意事项（`wake` 不能只接 `request_repaint`、按 `Failure` 显示本地化文字、汇总放到后台线程）写进 spec 7.9 和 11 任务书的请求。
- 体积（#808 的跟进）：`bae-hub` release 单文件（Linux x86_64，glibc）#798 分支 5,347,512 字节 → 本分支 5,363,624 字节，+15.7 KiB。应用经 `bae-agent` 只用到 bae-sync 的版本和 `version.*` 命令（不开 `hub`、`watch`），本分支里能到达的只有 `history.rs`、`commands.rs`、`error.rs` 的改动；连接管理、冲突提示、Hub 客户端都不链接进应用。
- 没做：
  - 应用的 `cargo xtask size`：`apps/bae` 在 Linux 上编译不过（`rfd` 只是 macOS、Windows 上的依赖，`shell/mod.rs` 却直接用它；不是本分支的改动），这里量不了，以 CI full tier 的 "Release size budget"（Windows、macOS）为准。
  - `remove_leftovers` 那一遍列目录并进第一次扫描（#791 的可选项）：会把删除从 `open` 挪到第一次 `recv`，契约、文档和测试都要改，只省一次列目录，而 `open` 本来就在后台线程上。
  - 按范围回滚时把移出范围的后来新建的子节点留在原处：先把它们移出要删的 Prim，可移到哪里都不是它们原来的位置，只写了说明。
- 以后（性能，不急）：每次 `diff` / `revert` 都为整个工程层的两个版本各建一份 `NodeIndex`（`Prim` 范围的回滚也是全量建），开销和以前按路径遍历同一量级。界面要实时显示差异时，缓存当前版本的 `NodeIndex`（按 Loro frontiers 失效）。

### 2026-11-27（一条消息放不下的文档，`collab/sync-too-large`）

- **一条消息放不下的文档**（spec 2.4 第 3 步、2.6、3、3.1、7.3、7.7；移植自 #760）：
  - 原来一个文档的增量超过 Hub 链路的 `max_message`（默认 256 MiB）时，`ThreadedTransport` 把它放进队列，后台线程发送失败，整条链路断开；`Connection` 重连后，会话又导出同一个几百 MiB 的增量、又失败，一直循环，别的文档也同步不了。Hub 一侧也一样：发给某个客户端的工程层太大时，Relay 断开这个客户端。
  - 现在传输层可以说出自己的上限（`Transport::max_message_size()`：`HubTransport` 是发送上限；`ThreadedTransport` 在 `spawn` 时读内层的；`Box`、`PeerLink` 转发）。超过上限时 `send` 直接返回 `SyncError::MessageTooLarge`（文档、大小、上限），什么都不发，链路照常。会话只跳过这个文档：报一次错，不算发出、不算在路上，`unsynced` 和新的 `too_large(docs)` 列出它；其他文档照发，流量控制的轮转照常往下走。
  - 按 #760 的做法记下增量由哪些 peer 的操作组成：只有对方拿到了其中某个 peer 更多的操作，或者 `reset()` 之后，才再试。本地修改、协作者的修改只会让增量变大，不重试。快照回答放不下时，这条链路上不再回答这个文档的快照请求（快照只会变大）。`Connection::close` 的 `flush` 也按链路的上限检查，放不下的文档留在 `Closing::unsynced()` 里，关闭结果不因它出错。
  - **握手时说出接收上限**：Hub 的配置比客户端小（`max_message_mib` 配小了、只读连接的 16 MiB）时，原来客户端照发，Hub 断开链路，客户端重连后再发，一直循环。现在握手的请求和响应都带 `x-bae-max-incoming`（`MAX_INCOMING_HEADER`，这一端的 `max_incoming`），`HubTransport` 的发送上限取自己的 `max_message` 和对方说的值中较小的（`peer_max_incoming()`），放不下的文档按上面的规则只跳过。没有这个头、值不对时照旧，新旧版本两个方向都兼容。测试 `each_side_sends_at_most_what_the_other_takes`、`a_client_skips_what_a_hub_with_a_smaller_limit_would_not_take`（去掉这个头时失败：Hub 断开链路）、`peers_that_do_not_say_keep_the_old_limits`（旧 Hub 是直接用的 tungstenite），bae-hub 的 `read_only_connections_cannot_send_large_messages` 跟着改。
  - 审查后补：放不下的文档换成同一个 key 下的新文档时（删掉工程层再新建），原来的记录只看旧文档的 peer，新文档哪怕很小也要等重连才发，`too_large` 还一直列着它。现在记录里也存下当时的本地版本，本地版本不再包含它就当是另一个文档，马上再试；快照的记录也一样；文档从 `docs` 里删掉时记录一起删掉（`a_document_replaced_under_the_same_key_is_sent`）。
  - 审查后补：收的一方原来不知道少了哪个文档。Hub 对新加入的客户端跳过一个放不下的工程层时，只在 Hub 的日志里记一条；客户端的状态是在线、没有未同步，`is_caught_up` 整条链路都为假，等它的离线合并汇总一直不来。现在 `SyncSession::behind(docs)`、`Connection::behind(docs)`（只在在线时）列出对方确认有、本地还没有全的文档，界面可以把一直列着的显示成"没有收到"。`is_caught_up` 改为用它；`ConflictTracker` 的 `catch_up_timeout`、`cancel_offline` 的说明里加上这种情况（spec 2.4"补齐"、6.4、7.3；测试 `documents_the_remote_has_more_of_are_listed_while_online`，`tests/too_large.rs` 和 bae-hub 的 `a_document_too_large_for_one_client_does_not_drop_its_link` 检查 `behind`）。
  - #760 里"对方拒收的文档"（`NewerLogVersion`）没有移植：更新的格式版本的日志现在由 `DirTransport::not_written`、`LinkNotes` 处理。
  - 测试（去掉对应的修复时都失败）：`tests/too_large.rs`（移植并补充：另一个文档照常同步、不算在路上、本地修改不重试、快照请求只导出一次）、会话的 `a_document_that_does_not_fit_is_skipped_and_reported_once`、`ThreadedTransport` 的 `a_message_over_the_limit_fails_at_once_and_the_link_goes_on`、`Connection` 的 `a_document_that_does_not_fit_stays_unsynced_and_closing_lists_it`、`tests/hub.rs` 的 `a_document_over_the_message_limit_is_skipped_and_the_link_stays`、bae-hub 的 `a_document_too_large_for_one_client_does_not_drop_its_link`。
  - 第二轮审查后改：每次发送时删掉已不在 `docs` 里的文档的记录，原来对每条记录在文档列表里线性查找，很多文档同时放不下时每次 `pump` 要花"文档数 × 记录数"（文档多 8 倍慢 50 多倍）；现在先建一次集合（`many_documents_that_do_not_fit_do_not_make_pumps_quadratic`，去掉修复时失败）。`MessageTooLarge`、`Transport::max_message_size` 和 spec 2.4 原来说放不下的消息一律只跳过、`pump` 不报错，其实只对文档数据成立：`Hello`、在线状态放不下时 `pump` 当链路出错返回它（`doc` 为空），`Connection` 按退避重连，`Relay` 断开；行为不改，文档写明（`a_hello_or_presence_that_does_not_fit_fails_the_pump` 固定这个行为）。`ConnectionOutcome::errors`、`ConnectionStatus::unsynced`、`Closing::unsynced` 的说明补上放不下的文档，spec 7.9 加一条给界面的提示。spec 2.4 里"没有记录时快照导出 21 次"改成 10 次（测试只发 10 个请求；21 是两种记录都去掉时的报错次数）。

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

> 2026-11-26 zjzcoder（C 多人协作）：1–3 已在 `collab/sync-hub-requests` 处理，Hub 的 `PeerLink` 绕行已去掉；4 已做：`Relay::connect_with` / `peer_config` 按连接设置会话，`Relay::flush` / `SyncSession::flush` 不管 `min_send_interval` 立即发出，`Relay::close_peer` 等传输交付完并报告错误；Hub 的存盘连接默认 300 ms 最多写一次、不发定期 `Hello`，最后一个客户端断开时、卸载 / 退出前都先 `flush`（spec 第 3 节）。

1. **`SyncSession::pump_at` 在传输出错时丢掉本轮已收到的数据（会丢修改）。**
   - 现象：`transport.recv()?` 直接返回错误，本轮已经放进 `incoming` 的 `Updates` 没有导入。客户端发完最后几条修改马上断开时，Hub 收到了修改和关闭帧，但修改没有导入；而客户端的会话已经认为 Hub 有了这些修改。只有这个客户端以后重连，才会补发。
   - 复现：`services/bae-hub/tests/hub.rs` 的 `idle_projects_are_unloaded_and_reloaded`，去掉 `src/link.rs` 里的绕行就会超时。
   - 建议：遇到传输错误时先 `import` 已经收到的，再返回错误。Hub 现在在 `PeerLink` 里绕开了：出错时先返回 `Ok(None)`，下一次再报错。客户端一侧同样受影响，只是影响较小（重连后会补齐）。
2. **WebSocket 消息大小上限。** `HubTransport` 用的是 tungstenite 的默认 `WebSocketConfig`：单帧最大 16 MiB，单条消息最大 64 MiB。tungstenite 发送时不会自动分片，所以一个工程层的快照或首次上传超过 16 MiB 时会被对方拒绝，连接断开，重连后又是同一条消息，会一直卡住。建议 `accept` / `connect` 把 `max_frame_size` 设成和 `max_message_size` 一样大（比如 256 MiB），或者让调用方传入配置。
3. **写缓冲上限**（不急）。`send` 遇到 `WouldBlock` 时把数据留在 tungstenite 的写缓冲里，默认没有上限。一个只发不收的客户端会让 Hub 的内存一直涨，Hub 只能等 60 秒超时。如果能设 `max_write_buffer_size`，满了返回错误，Relay 就会马上断开它。
4. **按连接设置 `SessionConfig`**（不急，优化）。比如 `Relay::connect_with(transport, config)`：Hub 想给存盘用的对端（`DirTransport`）单独设 `min_send_interval` 为几百毫秒，拖动时少写几次盘。不影响正确性。
