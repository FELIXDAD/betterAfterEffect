# 在威联通（QNAP）上部署 bae-hub

> 按 QTS 5 / QuTS hero 5 和 Container Station 3 编写，其他版本的菜单名称可能略有不同。总体说明见 [deploy/README.md](../README.md)。

前提：

- 在 App Center 安装 **Container Station**。
- 准备好镜像文件 `bae-hub-<版本>-amd64.tar`（ARM 型号用 `arm64`），构建方法见总说明第 1 节。

步骤：

1. **建目录**：打开 File Station，在 `Container` 共享文件夹下新建 `bae-hub`。`Container` 文件夹在安装 Container Station 时会自动创建，在系统里的路径是 `/share/Container/bae-hub`。
2. **导入镜像**：Container Station → 映像 → 导入，选择 `.tar` 文件。
3. **建应用程序**：Container Station → 应用程序 → 创建。
   - 应用程序名称：`bae-hub`
   - 粘贴下面的 YAML，然后点"创建"：

   ```yaml
   services:
     bae-hub:
       image: bae-hub:latest
       container_name: bae-hub
       restart: unless-stopped
       ports:
         - "7341:7341"
       volumes:
         - /share/Container/bae-hub:/data
   ```

4. **拿令牌**：两处都能找到，任选其一。
   - Container Station → 容器 → `bae-hub` → 日志，找 `team token:` 那一行。
   - File Station 打开 `Container/bae-hub/hub.toml`。
5. **防火墙**：如果装了 QuFirewall，要放行 TCP `7341`。
6. **检查**：在电脑浏览器里打开 `http://<NAS 地址>:7341/v1/info`。

日常维护：

- **修改配置**：编辑 `hub.toml` 后，在 Container Station 里重启容器。
- **升级**：先导入新镜像，再在应用程序页面重新创建（或"重新部署"）`bae-hub`。数据目录不受影响。
- **备份**：用 HBS 3 或快照备份 `Container/bae-hub` 文件夹。
