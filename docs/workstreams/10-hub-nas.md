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

- [ ] 同步服务端
- [ ] 素材存储
- [ ] 权限与发现
- [ ] Docker 与 NAS 部署文档
- [ ] 历史快照

## 请求（其他工作流在这里留言）

### 来自 09 collab/sync（2026-11-20）

09 这边已经准备好了，服务端可以直接用（`crates/bae-sync`，规格见 `docs/spec/sync.md`）：

- **协议和中继逻辑**：`bae_sync::Relay` 是不含网络的 Hub 核心：每个连接 `relay.connect(Box<dyn Transport + Send>)`，定期 `relay.pump()`；它负责增量转发、断线重连后的补齐、在线状态转发（新连接立即收到所有人的状态，连接断开时替它向其他人发"离开"）。持久化用 `relay.docs()`（每个文档 `export_snapshot`）和 `MemDocs::load` / `Relay::with_docs`。一个工程一个 `Relay`。
- **线上格式**：一条协议消息 = 一个 WebSocket 二进制帧（`Message::encode` / `decode`，见 spec 2.3）。服务端只需要给每个 axum WebSocket 连接实现 `Transport`（例如用 tokio 的 mpsc 通道桥接到跑 `Relay` 的任务或线程）。
- **客户端约定**（spec 3.1）：URL `ws://<主机>:<端口>/<路径>`，路径/查询参数怎么选工程由你们定；团队 token 放在 `Authorization: Bearer <token>`，拒绝时请回 HTTP 401；目前只用 `ws://`（局域网）。客户端会每 5 秒发 `Hello`、每 5 秒发在线状态心跳，Hub 不需要另做应用层心跳，WebSocket ping 可选。
- **参考实现**：`crates/bae-sync/tests/hub.rs` 的 `TestHub`（阻塞式，几十行，用 `HubTransport::accept`）展示了完整流程，验收测试里回环延迟中位数 2.9 ms。

