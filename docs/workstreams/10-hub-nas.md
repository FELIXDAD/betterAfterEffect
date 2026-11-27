# 10 · 协作服务与 NAS

| 项 | 内容 |
|---|---|
| 分支 | `collab/hub-nas` |
| Orca 工作区 | `bae-10-hub-nas` |
| 阶段 | P2 |
| 依赖 | 09（同步协议） |
| 负责目录 | `services/bae-hub/`、`deploy/` |

## 目标

一个能跑在 NAS 上的轻量协作服务：实时同步、在线状态、素材存储、历史快照。团队装好就能用。

## 要做

- **服务**：tokio + axum；WebSocket 同步（使用 09 的协议）、在线状态广播、工程列表、团队与权限（团队 token，只读/可写）、局域网 mDNS 自动发现。
- **素材存储**：BLAKE3 内容寻址、去重、分块上传与断点续传、客户端缓存协议。
- **代理生成（可选）**：NAS 一般是 Linux，系统里有 ffmpeg 时调用它生成代理；没有就跳过。
- **历史**：快照存储、保留策略、空间清理。
- **部署**：
  - Linux x64/arm64 的 musl 静态单文件。
  - Docker 镜像（多架构）。
  - 部署说明：群晖 Synology、威联通 QNAP、Unraid、TrueNAS；数据目录挂到 NAS 卷上。
- **管理页（可选）**：极简网页，显示状态、在线用户、存储用量。

## 对外接口

- 实现 09 的 WebSocket Transport 服务端。
- 素材存储的 HTTP API（上传、下载、存在性检查）。

## 验收

- 二进制 ≤ 15 MB；在 2 核 ARM NAS 上空闲内存 < 50 MB。
- 10 个客户端同时编辑，同步延迟 < 100 ms（局域网）。
- 在至少一款真实 NAS（或其 Docker 环境）上按文档部署成功。

## 不做

- 客户端同步逻辑（09）。
- 公网部署、账号系统（先只做局域网团队场景）。

## 进度

- [x] 同步服务端（PR 1，#47）：`services/bae-hub`。
  - 直接建立在 09 的 `Relay` + `HubTransport::accept` 上，和客户端是同一个 WebSocket 库（tungstenite 0.30）。没有异步运行时，不用 tokio / axum（协调人的决定）。
  - 每个工程一个 Relay 线程；在线状态和建议性锁由 Relay 转发；工程按需加载、空闲卸载。
  - 工程日志用 `DirTransport` 存在数据目录里（09 的共享目录日志格式，自动压缩，重启后读回）。
  - HTTP 接口：`/healthz`、`/v1/info`、`/v1/status`、`/v1/projects`。
  - 接口说明见 `services/bae-hub/README.md`。
- [ ] 素材存储
- [~] 权限与发现：
  - 团队令牌已做（PR 1）：只读/可写，可以限定工程；只保存哈希，比较用常数时间；首次启动自动生成。
  - mDNS 自动发现还没做。
- [x] Docker 与 NAS 部署文档（PR 2）：
  - `deploy/docker/Dockerfile`：多架构（amd64 / arm64）；构建阶段直接交叉编译，不需要 QEMU；最终镜像 `FROM scratch`，里面只有一个静态文件。
  - 其他文件：`compose.yaml`；`scripts/build-linux.sh`（生成 musl 静态单文件压缩包）、`scripts/build-image.sh`（生成可导入的镜像 .tar，或推送到镜像仓库）；systemd 服务；Unraid 模板。
  - 中文文档：`deploy/README.md`，以及群晖、威联通、Unraid、TrueNAS 的分步说明。
  - 真机部署验收还没做：这台 Mac 上没有 Docker，也没有 NAS。
- [ ] 历史快照

### 2026-11-20

- **PR 1 的测试**：8 个端到端测试，客户端是 `HubTransport::connect`：
  - 收敛，以及重启后从磁盘恢复
  - HTTP 接口与令牌
  - 只读
  - 匿名访问
  - 在线状态经 Hub 转发
  - 空闲卸载与重新加载
  - 10 个客户端的延迟（本机回环 debug 构建，中位数约 5.5 ms）
  - 数据目录锁
- **改架构**：第一版用 tokio + axum 写的。09 PR E 合并后，按协调人的要求改成直接用 `HubTransport`。
- **体积**：去掉 tokio / axum 之后，release 体积见 PR 描述（预算 15 MiB）。在 macOS 上用 `rust-lld` 就能直接交叉编译 musl 静态单文件，不需要 zig 或 Docker。
- **PR 1 已合并（#47）。**
- **PR 2 的验证**（只能在本机做）：
  - `build-linux.sh` 在 macOS 上生成了两个架构的压缩包：x86_64 版 2.4 MiB（程序 5.0 MiB），aarch64 版 2.2 MiB（程序 4.2 MiB）。
  - 按容器的方式运行（只用环境变量配置）：健康检查、只读令牌、SIGTERM 退出都正常。
  - compose 和各 NAS 文档里的 YAML 都能正常解析。
  - 还没验证：Docker 镜像的实际构建，以及在真实 NAS 上部署（任务书验收第 3 条）。需要一台装了 Docker 的机器，或一台 NAS。
- **给 09 的请求**：写在 09 任务书"请求"一节，共 4 条：
  - 传输出错时会话丢掉本轮已收到的数据（会丢修改；Hub 这边已经绕开）
  - WebSocket 单帧 16 MiB 的上限
  - 写缓冲上限
  - 按连接设置 `SessionConfig`

### 2026-11-26（zjzcoder，C 多人协作）

- 上面前三条已在 09 侧解决（`collab/sync-hub-requests`）：会话出错前先导入；`HubLimits` 默认单条消息 256 MiB；对方不再读时断开（积压超过 16 MiB、并且 30 秒内一个字节都没发出去），读得慢的不断开。`PeerLink` 的"晚一次报错"绕行已去掉，`idle_projects_are_unloaded_and_reloaded` 照常通过。
- 第 4 条也已解决：工程目录用 `Relay::connect_with` 接上，`min_send_interval` 默认 300 ms（`HubConfig::store_send_interval`）、`hello_interval = None`；客户端之间照常立即转发。最后一个客户端断开时立即 `Relay::flush()`（否则要等 500 ms 的空闲轮询）。关闭工程（卸载、退出）时先最后 `pump` 一次，再 `flush`，再 `Relay::close_peer` 等存盘线程写完；最后几次写盘失败会记进日志。
  - 新测试：拖动 100 步，`hub.log` 只有 3 条记录（不设间隔时 101 条）。另外三个测试把写盘间隔设成 1 分钟，拖动只能靠 `flush` 写出：拖动后立即退出 Hub（客户端还连着）、最后一个客户端断开（工程不卸载）、工程被卸载，重新加载后每一步都在。去掉退出时的 `flush`，第一个失败；去掉客户端走光时的 `flush`，第二个失败；两个都去掉，卸载的测试也失败。
  - 评审后还修了 09 侧的一个老问题：`DirTransport` 写日志失败后不再接着追加（否则日志里有空洞而目录的 `Hello` 不承认，重新挂上的存盘连接不会补写），改为请求快照重写（spec 4.3）。
- 握手响应带 `x-bae-access: read|write`（09 的 `HubTransport::accept_granting`，`collab/sync-connection`）：客户端不用再查 `/v1/status` 就知道自己是不是只读。新测试 `tests/connection.rs` 用应用的连接管理（`bae_sync::Connection`）跑真实 Hub：收敛；Hub 停掉再在同一端口启动后自动重连、离线修改不丢；401/403/404/400 不反复重试；只读令牌；关闭时最后的修改送到（README"测试"一节）。

- 多进程测试（`tests/multiprocess.rs`，epic #737 子任务 7 第一部分）：用真实的 `bae-hub` 程序（命令行参数加 `BAE_HUB_TOKEN`，和部署时一样），编辑中途杀掉后在同一个数据目录和端口重启，客户端重连后收敛、不丢修改；客户端进程被杀后重启；经故障代理断线、延迟。默认 5 个测试并行约 2.3 秒，长时间测试在 `--ignored` 里（spec 3.1）。Hub 被杀时压缩到一半留下的 `hub.tmp-*` 现在下次打开工程时删掉（09 的修复，spec 4.3）。评审后：客户端都退出后再杀一次 Hub、重启，新客户端只能从 Hub 的磁盘拿到全部修改（原来清空数据目录也能通过）；`bae-hub` 由一个随测试进程退出的子进程启动，测试进程被强行杀掉时不再留下占着端口和数据目录的 Hub。

### 2026-11-27（zjzcoder，C 多人协作，#782 评审的跟进）

- **连接的上限可以配置**：`hub.toml` 新增 `[limits]`（`HubConfig::limits`、`HubConfig::read_only_max_incoming`）：单条消息、流量控制、每个客户端最多排队多少、停滞超时、只读连接发来的单条消息上限。都有默认值，一般不用改。
- **只读连接只收不太大的消息**（默认 16 MiB）：原来所有连接都接受 256 MiB 的消息，只读连接的修改要整条收完、解码后才丢掉。现在接入时就用小一些的接收上限，超过的读到帧头就断开；发给只读连接的消息照样可以很大。
  - 起初是 1 MiB。评审发现两个问题：`Hello` 随文档数 × Loro peer 数增长（约 11 字节一对，1 MiB 约 9.5 万对）；会话不知道自己只读，查看者本地的修改会在 Hub 每个 `Hello` 之后、每次重连之后再发，一个文档里超过 1 MiB 的查看者每次连上都被断开（实测 4 秒断开几百次），以前 Hub 只是丢掉这些修改。
  - 现在默认 16 MiB，客户端知道令牌只读时（`/v1/status` 的 `access`）设 `SessionConfig::send_data = false`（09，spec 2.4 第 5 步），什么数据都不发（`viewers_with_large_local_edits_stay_connected`）。
- **`[limits]` 要配得上**（评审后）：不写 `max_unsent_mib` 时按 `2 × max_message_mib + max_backlog_mib` 算（原来固定 528，调大 `max_message_mib` 后合法的发送也会让连接失败）；`max_unsent_mib` 小于 `max_message_mib + max_backlog_mib`、`max_message_mib` 超过客户端能收的 256 MiB 时拒绝启动（`limits_fit_together`）。部署文档里"NAS 内存很小"改为调小 `max_backlog_mib`。
- **内存上限**：Relay 按每个连接的 `max_backlog` 做流量控制（`PeerLink` 把 WebSocket 的积压报告给会话），读得慢的客户端那里正常最多排队 16 MiB 加一条消息；排队超过 `max_unsent`（默认 528 MiB）就断开（09 侧，spec 3.1）。
- **工程目录的清理**：09 的清理（spec 4.7）改为默认关闭，Hub 明确打开，理由见 README"数据目录"。
- 评审后补了测试：工程目录的 `DirConfig` 打开了清理（`the_project_directory_is_cleaned_up`），客户端会话的 `max_backlog` 来自 `[limits]`（`client_sessions_use_the_configured_backlog`）。以前去掉这两行整个测试集照样通过。

### 2026-11-27（zjzcoder，C 多人协作，#791、#808 评审的跟进）

- **降级后不保存的文档**：Hub 自己在工程目录里的日志是更新的格式版本（新版本的 bae-hub 用过这个数据目录）时，09 的 `DirTransport` 不读、不改、不写这个文档（spec 4.3）。原来只在卸载工程时打一条 error；现在加载工程时、工程可用之前打 `WARN`（工程 ID 和文档），`/v1/projects` 的 `not_saved` 也列出它们（`a_newer_format_own_log_is_reported_at_load`：在加载线程上抓日志，检查这条 `WARN` 在加载完成时已经打出）。选择警告而不是拒绝打开：拒绝的话其他文档也都不能同步；这些文档的修改照常在客户端之间转发，客户端各自留着，换回新版本的 Hub 后重连时补写。
- **`max_backlog_mib = 0` 拒绝启动**，和其他上限一致（原来 PR 说明写的是拒绝，实际接受）；所有"不能是 0"的检查放在"配得上"的检查之前，报的就是"不能是 0"（`zero_limits_are_refused`）。README 和生成的配置文件里写明。
- **多进程测试**：测试失败时不再留下 `bae-hub` 进程（丢弃子进程时先关它的标准输入，守护 Hub 的子进程来得及先杀 Hub）；杀 Hub 的测试一直编辑到重连之后再叫停；同一端口重启时的端口竞争写在注释里（spec 3.1）。
- 09 侧：只读令牌连不发 `x-bae-access` 的旧 Hub（0.0.1）时，客户端会再查一次 `/v1/status`（spec 7.5）。

### 2026-11-27（zjzcoder，C 多人协作，`collab/sync-too-large`）

- **放不下的工程层不再断开客户端**（spec 2.4 第 3 步）：发给某个客户端的一个工程层超过 `max_message` 时，Relay 的会话只对它跳过这个工程层，连接不断（`PeerLink` 转发 `max_message_size`）。日志里记一条 warn，按这条消息的大小说怎么办（第三、四轮审查后的样子，见下）：超过 256 MiB（`max_message_mib` 的最大值）时这个工程层通不过 Hub：删掉其中的内容没用（放不下的是客户端缺的全部修改历史，不是现在的大小，删除只会再加修改），只能把内容移到新的、更小的工程层；否则说是 Hub 的 `max_message_mib`、客户端握手时说的上限还是两个都比它小，要调到多少。测试 `a_document_too_large_for_one_client_does_not_drop_its_link`。
  - 审查后改：原来 warn 一律说"调大 `max_message_mib`"。默认配置下照做的话 Hub 拒绝启动（`validate` 不让它超过 256）；上限是客户端自己的时候，调了也没用（`the_too_large_warning_says_what_helps`）。
  - 第二轮审查后改：默认上限下 warn 原来说工程层"拆小或者变小"之后能同步，可删内容只会再加修改，放不下的是客户端缺的修改历史，现在说删内容没用、要把内容移到新的、更小的工程层（README 同）。warn 里的大小原来一律按 MiB 写，客户端说自己收 5 KiB 以下时显示"0.00 MiB"，现在不到 1 MiB 按 KiB、不到 1 KiB 按字节写（`small_limits_in_the_too_large_warning_are_not_zero`）。`deploy/README.md` 原来说比上限大的工程层"Hub 日志里有 warn"，其实客户端自己的修改放不下时客户端自己跳过、Hub 什么都收不到也不记日志，只有 Hub 已有、发不给某个客户端的工程层才记（`the_hub_warns_only_about_layers_it_cannot_send` 抓 Relay 线程的日志验证两种情况）。
  - 第三、四轮审查后改：warn 的建议原来只看上限、不看这条消息多大，也不知道客户端握手时说了多少：300 MiB 的工程层在 64 MiB 的 Hub 上让调大 `max_message_mib`（调到 256 也没用），两边都是 64 MiB 时只怪客户端。现在连接记下客户端握手时说的上限（`PeerInfo::max_incoming`，`/v1/projects` 每个连接的 `max_incoming` 也显示它），建议按消息大小分情况：超过 256 MiB 时只能移到新的工程层（`@history` 没有办法，不提工程层）；否则说是 Hub 的、客户端的还是两个都小，要调到多少（`the_too_large_warning_says_what_helps`；`the_hub_warns_only_about_layers_it_cannot_send` 检查连接记下的上限用上了）。别人的在线状态发不给某个客户端时只跳过、记一条 warn，不再断开它（09）。记 warn 改在处理断开之前（原来同一次 pump 里先跳过、后断开的连接已经查不到是谁），空闲时和关闭前的 `flush` 跳过的也记。README 和 `read_only_max_message_kib` 的说明改正：只读连接握手时说的是它和 `max_message_mib` 中较小的那个；旧客户端才会因为放不下"每次都被断开"。
  - 第五轮审查发现（原来就有，没改，README 和 spec 4.2 写成已知问题）：Hub 的工程目录压缩时把整个工程层写成一条快照记录，不受链路 256 MiB 的限制；历史超过 1 GiB 的工程层，Hub 重启后会把这条记录当成写坏的尾巴截掉，这个工程层就从 Hub 上没了，客户端补传又超过 256 MiB；超过 4 GiB 时存盘线程反复 panic、重开。要另外修（分成几条记录的快照，或快照太大时不压缩并报告）。
- **握手时说出接收上限**：101 响应带 `x-bae-max-incoming`（可写连接是 `max_message_mib`，只读连接是 `read_only_max_message_kib` 和 `max_message_mib` 中较小的那个；09 的 `HubTransport::accept_granting` 发出），新版本的客户端不再发 Hub 收不下的消息，`max_message_mib` 配小了也只是那个工程层不同步，不会断开、重连、再断开。客户端的请求也带它，Hub 发给它的消息不超过它说的值。旧版本两边都照旧。`read_only_connections_cannot_send_large_messages` 改为：客户端自己不发、连接不断；手写握手的客户端发来时照样被断开。

## 请求（其他工作流在这里留言）

### 来自 09 collab/sync（2026-11-20）

09 这边已经准备好了，服务端可以直接用（`crates/bae-sync`，规格见 `docs/spec/sync.md`）：

- **协议和中继逻辑**：`bae_sync::Relay` 是不含网络的 Hub 核心：每个连接 `relay.connect(Box<dyn Transport + Send>)`，定期 `relay.pump()`；它负责增量转发、断线重连后的补齐、在线状态转发（新连接立即收到所有人的状态，连接断开时替它向其他人发"离开"）。持久化用 `relay.docs()`（每个文档 `export_snapshot`）和 `MemDocs::load` / `Relay::with_docs`。一个工程一个 `Relay`。
- **线上格式**：一条协议消息 = 一个 WebSocket 二进制帧（`Message::encode` / `decode`，见 spec 2.3）。服务端只需要给每个 axum WebSocket 连接实现 `Transport`（例如用 tokio 的 mpsc 通道桥接到跑 `Relay` 的任务或线程）。
- **客户端约定**（spec 3.1）：URL `ws://<主机>:<端口>/<路径>`，路径/查询参数怎么选工程由你们定；团队 token 放在 `Authorization: Bearer <token>`，拒绝时请回 HTTP 401；目前只用 `ws://`（局域网）。客户端会每 5 秒发 `Hello`、每 5 秒发在线状态心跳，Hub 不需要另做应用层心跳，WebSocket ping 可选。
- **参考实现**：`crates/bae-sync/tests/hub.rs` 的 `TestHub`（阻塞式，几十行，用 `HubTransport::accept`）展示了完整流程，验收测试里回环延迟中位数 2.9 ms。

