# 在 Unraid 上部署 bae-hub

> 按 Unraid 6.12 / 7 编写。总体说明见 [deploy/README.md](../README.md)。

前提：

- 设置 → Docker 里已经启用 Docker。
- 准备好镜像文件 `bae-hub-<版本>-amd64.tar`，构建方法见总说明第 1 节。

步骤：

1. **导入镜像**：
   - 把 `.tar` 拷到某个共享里，例如 `appdata`。
   - 在 Unraid 网页右上角打开终端（Terminal），执行：

   ```sh
   docker load -i /mnt/user/appdata/bae-hub-<版本>-amd64.tar
   ```

2. **安装模板**：把仓库里的 `deploy/unraid/bae-hub.xml` 拷到 U 盘的 `config/plugins/dockerMan/templates-user/my-bae-hub.xml`。
   - 通过 `flash` 共享拷，或者在终端里执行：

   ```sh
   cp bae-hub.xml /boot/config/plugins/dockerMan/templates-user/my-bae-hub.xml
   ```

3. **建容器**：Docker 页 → 添加容器（Add Container）→ 模板（Template）里选 `bae-hub`。
   - 默认端口是 `7341`，数据目录是 `/mnt/user/appdata/bae-hub`，按需修改。
   - 令牌可以不填，首次启动会自动生成。
   - 点"应用"（Apply）。
4. **拿令牌**：两处都能找到，任选其一。
   - Docker 页点 `bae-hub` 的图标 → 日志（Logs），找 `team token:` 那一行。
   - 打开 `appdata/bae-hub/hub.toml`。
5. **检查**：在电脑浏览器里打开 `http://<Unraid 地址>:7341/v1/info`。模板里也设置了 WebUI，点图标里的 WebUI 会打开这个地址。

日常维护：

- **数据位置**：`appdata` 默认放在缓存盘上，速度快。备份可以用 Appdata Backup 插件。
- **升级**：先 `docker load` 新镜像，再在 Docker 页编辑 `bae-hub`，直接点"应用"，会用新镜像重建容器。
