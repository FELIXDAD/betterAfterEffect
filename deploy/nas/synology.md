# 在群晖（Synology）上部署 bae-hub

> 按 DSM 7.2 和 Container Manager 编写，其他版本的菜单名称可能略有不同。总体说明见 [deploy/README.md](../README.md)。

## 方式一：Container Manager（推荐）

前提：

- 在套件中心安装 **Container Manager**。不是所有型号都支持，在套件中心搜不到时用方式二。
- 准备好镜像文件 `bae-hub-<版本>-amd64.tar`（ARM 型号用 `arm64`），构建方法见总说明第 1 节。在控制面板 → 信息中心可以查看处理器型号。

步骤：

1. **建目录**：打开 File Station，在 `docker` 共享文件夹下新建 `bae-hub`。`docker` 文件夹在安装 Container Manager 时会自动创建。
2. **导入镜像**：Container Manager → 映像 → 新增 → 从文件添加，选择 `.tar` 文件。
3. **建项目**：Container Manager → 项目 → 新增。
   - 项目名称：`bae-hub`
   - 路径：选第 1 步建的 `docker/bae-hub`
   - 来源：创建 docker-compose.yml，粘贴下面的内容：

   ```yaml
   services:
     bae-hub:
       image: bae-hub:latest
       container_name: bae-hub
       restart: unless-stopped
       ports:
         - "7341:7341"
       volumes:
         - ./data:/data
   ```

   - 一路下一步（网页门户不用设置），完成后项目会自动启动。
4. **拿令牌**：两处都能找到，任选其一。
   - Container Manager → 容器 → `bae-hub` → 日志，找 `team token:` 那一行。
   - File Station 打开 `docker/bae-hub/data/hub.toml`。
5. **防火墙**：如果开启了防火墙，到控制面板 → 安全性 → 防火墙 → 编辑规则，允许 TCP 端口 `7341`。
6. **检查**：在电脑浏览器里打开 `http://<群晖地址>:7341/v1/info`，能看到 `"service":"bae-hub"` 就说明正常。

日常维护：

- **修改配置**：用 File Station 下载 `hub.toml`，改好后上传覆盖，再在项目里点"重新启动"。
- **升级**：先导入新的镜像，然后在项目里依次点"停止"、"构建"、"启动"。数据在 `data` 目录里，不受影响。
- **备份**：Hyper Backup 或快照备份 `docker/bae-hub` 即可。

## 方式二：直接运行单文件（没有 Container Manager 的型号）

1. 用 `deploy/scripts/build-linux.sh` 生成程序包。架构要对应：`x86_64` 用于 Intel/AMD 型号，`aarch64` 用于 ARMv8 型号（如 DS223j）。
2. 新建共享文件夹 `bae-hub`，把压缩包里的 `bae-hub` 文件放进去。
3. 控制面板 → 任务计划 → 新增 → 触发的任务 → 用户定义的脚本：
   - 用户：`root`
   - 事件：开机
   - 任务设置 → 运行命令：

   ```sh
   chmod +x /volume1/bae-hub/bae-hub
   /volume1/bae-hub/bae-hub --data /volume1/bae-hub/data >> /volume1/bae-hub/hub.log 2>&1 &
   ```

4. 选中这个任务点"运行"，立即启动一次。令牌在 `bae-hub/hub.log` 和 `bae-hub/data/hub.toml` 里。
5. 停止：SSH 登录后执行 `sudo pkill -TERM bae-hub`。服务会写完工程再退出。
