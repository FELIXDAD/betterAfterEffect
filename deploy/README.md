# 部署 bae-hub（协作服务）

bae-hub 让团队在局域网里实时协作：大家连到同一台常开的机器（通常是 NAS），改动毫秒级同步。工程同时保存在这台机器上，谁都不在线时也不会丢。接口和实现见 `services/bae-hub/README.md`。

> 不想装服务也可以协作：把工程目录直接放在 NAS 的共享文件夹里（共享目录模式，延迟在秒级）。Hub 只是让同步更快，并且多了令牌权限控制。

## 你需要知道的三件事

| 项 | 说明 |
|---|---|
| **端口** | `7341`（TCP）。客户端连接 `ws://<NAS 地址>:7341/projects/<工程 ID>`。 |
| **数据目录** | 工程和配置都在这里，容器里是 `/data`，请挂到 NAS 的一个共享文件夹上。备份这个目录就是备份全部工程。 |
| **团队令牌** | 第一次启动时自动生成一个**可写令牌**，写进数据目录的 `hub.toml`，同时在日志里显示一次。把它发给团队成员，在 BAE 里连接 Hub 时填写。 |

体积与资源：程序是一个约 5–6 MB 的静态单文件。空闲时内存只有几 MB，CPU 几乎为零；10 个人同时编辑时，局域网内的同步延迟在毫秒级。

## 选一种安装方式

| 方式 | 适合 | 说明 |
|---|---|---|
| **Docker**（推荐） | 群晖、威联通、Unraid、TrueNAS SCALE，任何能跑容器的 NAS | 下面第 1 节，以及各品牌的分步说明：[群晖](nas/synology.md)、[威联通](nas/qnap.md)、[Unraid](nas/unraid.md)、[TrueNAS](nas/truenas.md) |
| **单文件 + systemd** | 普通 Linux 服务器、能 SSH 登录的 NAS | 第 2 节 |
| **直接运行** | 临时试用、在自己电脑上测试 | `bae-hub --data ./hub-data`，Ctrl-C 停止 |

### 1. Docker

**准备镜像。** 目前还没有发布到公共镜像仓库，需要先在一台装了 Docker 的电脑上构建：

```sh
# 在仓库根目录
deploy/scripts/build-image.sh --tar amd64     # Intel / AMD 的 NAS
deploy/scripts/build-image.sh --tar arm64     # ARM（aarch64）的 NAS、树莓派
```

- 得到 `dist/bae-hub-<版本>-<架构>.tar`，在 NAS 的容器管理器里"导入镜像"即可。
- 不确定 NAS 是什么架构：SSH 登录后执行 `uname -m`。`x86_64` 用 amd64，`aarch64` 用 arm64。
- 有自己的镜像仓库时，可以一次构建两种架构并推送：`deploy/scripts/build-image.sh --push <仓库>/bae-hub`。

**运行。** 用 `deploy/docker/compose.yaml`（把数据目录改成 NAS 上的路径）：

```sh
docker compose -f deploy/docker/compose.yaml up -d
docker logs bae-hub          # 第一次启动时这里会显示团队令牌
```

或者不用 compose：

```sh
docker run -d --name bae-hub --restart unless-stopped \
  -p 7341:7341 -v /volume1/docker/bae-hub:/data bae-hub:latest
```

镜像说明：

- 基于空镜像（`scratch`），里面只有 bae-hub 一个文件，没有 shell。
- 自带健康检查（`bae-hub healthcheck`），容器管理器里会显示"健康"。
- `docker stop` 时会先关闭连接、写完工程再退出。
- 默认以 root 运行，这样不管数据目录归谁都能写。想用普通用户运行，见第 3 节"权限"。

### 2. 单文件 + systemd（不用 Docker）

```sh
deploy/scripts/build-linux.sh           # 在 macOS 或 Linux 上，生成 dist/bae-hub-<版本>-{x86_64,aarch64}-linux-musl.tar.gz
```

- 构建不需要 Docker 或交叉编译器：Rust 自带的链接器就能生成 Linux 静态文件。
- 把压缩包拷到服务器上解开，然后：

```sh
sudo install -m 755 bae-hub /usr/local/bin/bae-hub
sudo useradd --system --home /var/lib/bae-hub --shell /usr/sbin/nologin bae-hub
sudo cp bae-hub.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now bae-hub
sudo journalctl -u bae-hub              # 第一次启动时这里会显示团队令牌
```

数据目录是 `/var/lib/bae-hub`。

## 3. 配置

配置文件是数据目录里的 `hub.toml`，第一次启动时自动生成，里面每一项都有中文注释。改完后重启服务生效。常用的几项：

```toml
# 团队令牌：可以有多个；read 只能看，write 可以编辑和新建工程
[[token]]
name = "team"
token = "…自动生成的 64 位十六进制…"
access = "write"

[[token]]
name = "client-review"
token = "…用 bae-hub gen-token 生成…"
access = "read"
projects = ["film-a"]      # 只能打开这个工程
```

| 需求 | 怎么做 |
|---|---|
| 增加一个只读令牌（给客户、审阅的人） | 加一段 `[[token]]`，`access = "read"`。只读的人看得到所有修改，自己改的不会传出去 |
| 生成新令牌 | `bae-hub gen-token`（Docker：`docker run --rm bae-hub gen-token`） |
| 某个令牌泄露了 | 从 `hub.toml` 删掉或改掉它，重启服务 |
| 不想写配置文件 | 用环境变量 `BAE_HUB_TOKEN`（可写）、`BAE_HUB_READ_TOKEN`（只读） |
| 局域网完全可信，不想要令牌 | `anonymous = "read"` 或 `"write"`（不推荐） |
| 换端口 | 改 Docker 的端口映射（例如 `-p 8000:7341`），或者 `hub.toml` 里的 `listen` |
| NAS 内存很小 | `max_message_mib = 64`（默认 256，范围 1–1024）：每个连接最多占约 65 MiB（一条消息 + 1 MiB）。代价：客户端需要发超过 64 MiB 的消息时（新客户端第一次同步一个大工程层，或 Hub 丢数据后重新上传），Hub 会断开它，它反复重连，期间同步基本停住；Hub 上超过 64 MiB 的工程层也发不给新客户端。客户端用的是 256，改大没有用 |

**权限**（容器用普通用户运行时）：数据目录必须归这个用户所有，或者对它可写。

- compose 里写 `user: "<uid>:<gid>"`。在 NAS 的 SSH 里执行 `id 用户名` 可以查到 uid 和 gid。
- 群晖第一个管理员通常是 `1026:100`，Unraid 是 `99:100`，威联通的 admin 是 `0:0`。

## 4. 检查是否正常

在团队的任意一台电脑上：

```sh
curl http://<NAS 地址>:7341/healthz                       # ok
curl http://<NAS 地址>:7341/v1/info                       # 版本、协议
curl -H "Authorization: Bearer <令牌>" http://<NAS 地址>:7341/v1/projects   # 工程和在线的人
```

## 5. 备份、升级、迁移

- **备份**：备份数据目录即可，最好用 NAS 的快照功能，或者停服务后复制。
- **升级**：换新镜像或新程序后重启服务。工程日志带格式版本号，以后如果有不兼容的变化，版本说明里会写明迁移方法。
- **迁移到另一台 NAS**：把数据目录整个拷过去，在新机器上指向它启动即可（令牌都在 `hub.toml` 里）。
- **万一数据目录丢了**：
  - 每个团队成员电脑上都有完整的工程。
  - 他们重新连上新的 Hub 后，会自动把工程补传上去。
  - 但只读令牌连接的人不会上传。

## 6. 常见问题

| 现象 | 原因与处理 |
|---|---|
| 客户端报 401 | 令牌不对或没填。核对 `hub.toml`；令牌前后不要带空格 |
| 客户端报 404 | 用只读令牌打开了一个 Hub 上还不存在的工程。先用可写令牌连一次（会新建工程） |
| 客户端报 403 | 这个令牌的 `projects` 里没有这个工程 |
| 连不上 | 防火墙或端口映射。NAS 的防火墙要放行 TCP 7341；`curl http://<NAS 地址>:7341/healthz` 应该返回 ok |
| 日志里 `another bae-hub is already using the data directory` | 两个容器或进程用了同一个数据目录。停掉其中一个 |
| 日志里 `project directory failed` | 数据目录写不进去（满了、权限、卷没挂好）。服务会每 30 秒重试，修好后自动恢复 |
| 日志里 `write buffer full: the peer is not reading` | 某个客户端很久不收数据（程序卡住），排队的数据超过上限，Hub 断开了它。网络慢不会这样：Hub 会等它收完再发。客户端恢复后会自动重连 |
| 日志里 `over the limit` | 一条同步消息超过了上限（默认 256 MiB）。`hub.toml` 里改过 `max_message_mib` 的话，改回 256，和客户端一致。只读令牌的客户端发来超过 16 MiB 的修改也会这样（它的修改本来就会被丢掉） |
| 容器启动后马上退出 | `docker logs bae-hub` 查看原因，常见的是 `hub.toml` 写错（会指出哪一行） |

**目前不支持**：公网访问和 HTTPS（只用于局域网）。需要远程协作时，请用 VPN（例如群晖 VPN Server、Tailscale、ZeroTier）连回办公室局域网。
