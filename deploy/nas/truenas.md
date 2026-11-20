# 在 TrueNAS 上部署 bae-hub

> 按 TrueNAS SCALE / Community Edition 24.10 及以上编写（应用基于 Docker）。总体说明见 [deploy/README.md](../README.md)。
>
> TrueNAS CORE（FreeBSD）不能直接运行 bae-hub，请在 CORE 上建一个 Linux 虚拟机，再按总说明第 2 节用单文件 + systemd 部署。

前提：

- 应用（Apps）里已经选好存放应用的存储池。
- 准备好镜像文件 `bae-hub-<版本>-amd64.tar`，构建方法见总说明第 1 节。

步骤：

1. **建数据集**：数据保护 → 存储 → 在存储池下新建数据集 `bae-hub`，例如 `/mnt/tank/apps/bae-hub`，预设选"Apps"。
2. **导入镜像**：
   - 把 `.tar` 拷到这个数据集里，例如通过 SMB 共享。
   - 系统 → 高级 → Shell（或 SSH 登录），执行：

   ```sh
   sudo docker load -i /mnt/tank/apps/bae-hub/bae-hub-<版本>-amd64.tar
   ```

3. **安装应用**：应用 → 发现应用 → 右上角菜单 → 通过 YAML 安装（Install via YAML）。
   - 名称：`bae-hub`
   - 粘贴下面的 YAML，然后保存：

   ```yaml
   services:
     bae-hub:
       image: bae-hub:latest
       pull_policy: never
       restart: unless-stopped
       ports:
         - "7341:7341"
       volumes:
         - /mnt/tank/apps/bae-hub:/data
   ```

   也可以用"自定义应用"（Custom App）的表单填写，字段对应如下：
   - 镜像：`bae-hub`，标签：`latest`，拉取策略选"不拉取 / 仅在不存在时拉取"
   - 端口：`7341` TCP
   - 存储：主机路径 `/mnt/tank/apps/bae-hub` 挂到 `/data`
4. **拿令牌**：两处都能找到，任选其一。
   - 应用 → `bae-hub` → 工作负载里的日志，找 `team token:` 那一行。
   - 打开数据集里的 `hub.toml`。
5. **检查**：在电脑浏览器里打开 `http://<TrueNAS 地址>:7341/v1/info`。

日常维护：

- **权限**：
  - 容器默认以 root 运行，能写数据集。
  - 如果在应用里指定了用户（例如 `568:568`，即 TrueNAS 的 apps 用户），要先把数据集的所有者改成这个用户。
- **升级**：先 `docker load` 新镜像，再在应用页面重新部署（Redeploy）。
- **备份**：给 `bae-hub` 数据集设置定期快照。
