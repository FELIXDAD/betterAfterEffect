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

### 2026-11-26

- 09 修好了请求 1（分支 `collab/hub-reliability`）：会话遇到传输错误时先导入本轮已收到的数据。`PeerLink` 里"出错晚一次报告"的绕行已经去掉；`idle_projects_are_unloaded_and_reloaded` 不靠它也能通过。
- 请求 4 也做了：存盘用的对端（工程目录）用 `Relay::connect_with` 设 `min_send_interval` 250 ms，客户端拖动时少写几次盘。关闭工程前去掉这个节流，最后一次 pump 把修改全部写盘（不去掉的话，`clients_converge_and_the_hub_keeps_the_project` 重启后会少修改）。
- 请求 2、3 也做了（09 的 `HubLimits`）：
  - 单条消息和单帧上限都是 256 MiB，客户端和 Hub 一致。`server.rs` 改用 `HubTransport::accept_with(stream, None, limits)`。
  - `hub.toml` 新增 `max_message_mib`（可选，默认 256，范围 1–1024；上限来自工程日志一条记录最多 1 GiB）。没有对应的环境变量，和 `idle_unload_secs` 一样。
  - 写缓冲上限跟着它走（上限 + 16 MiB）：只发不收的客户端积压超过上限就被断开，不用等 60 秒超时。
  - 测试：`config::tests::max_message_mib`（默认值、显式值、越界值拒绝启动）、`tests/hub.rs` 的 `updates_over_16_mib_go_through_the_hub`（20 MiB 的修改经 Hub 传给另一个客户端；用 tungstenite 默认设置时 Hub 断开发送方）。

## 请求（其他工作流在这里留言）

### 来自 09 collab/sync（2026-11-20）

09 这边已经准备好了，服务端可以直接用（`crates/bae-sync`，规格见 `docs/spec/sync.md`）：

- **协议和中继逻辑**：`bae_sync::Relay` 是不含网络的 Hub 核心：每个连接 `relay.connect(Box<dyn Transport + Send>)`，定期 `relay.pump()`；它负责增量转发、断线重连后的补齐、在线状态转发（新连接立即收到所有人的状态，连接断开时替它向其他人发"离开"）。持久化用 `relay.docs()`（每个文档 `export_snapshot`）和 `MemDocs::load` / `Relay::with_docs`。一个工程一个 `Relay`。
- **线上格式**：一条协议消息 = 一个 WebSocket 二进制帧（`Message::encode` / `decode`，见 spec 2.3）。服务端只需要给每个 axum WebSocket 连接实现 `Transport`（例如用 tokio 的 mpsc 通道桥接到跑 `Relay` 的任务或线程）。
- **客户端约定**（spec 3.1）：URL `ws://<主机>:<端口>/<路径>`，路径/查询参数怎么选工程由你们定；团队 token 放在 `Authorization: Bearer <token>`，拒绝时请回 HTTP 401；目前只用 `ws://`（局域网）。客户端会每 5 秒发 `Hello`、每 5 秒发在线状态心跳，Hub 不需要另做应用层心跳，WebSocket ping 可选。
- **参考实现**：`crates/bae-sync/tests/hub.rs` 的 `TestHub`（阻塞式，几十行，用 `HubTransport::accept`）展示了完整流程，验收测试里回环延迟中位数 2.9 ms。

