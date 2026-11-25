# betterAfterEffects 开发进度看板

看板使用 Python 3 标准库、SQLite 和浏览器原生 SSE。代码与 Rust 工作区独立。可用 Python 3.8 或更新版本。

## 本机启动

在完整仓库里运行：

```sh
cd tools/progress-web
python3 server.py
```

打开 http://127.0.0.1:8787/ ，用户名为 `bae`。首次启动生成以下私有文件，默认目录权限为 0700，文件权限为 0600：

| 文件 | 用途 |
|---|---|
| `~/.config/bae-board/server.env` | 管理员查看 `BAE_BOARD_PASSWORD`。此文件还含写入密钥，请只向查看者提供密码值 |
| `~/.config/bae-board/env` | 汇报脚本读取 `BAE_BOARD_URL`、`BAE_BOARD_TOKEN` 和可选的 `BAE_BOARD_AGENT` |
| `~/.config/bae-board/board.sqlite3` | 能力、效果、Agent、动态 |

首次导入读取同目录的里程碑清单，以及仓库里的 AE 26 分类（排除 obsolete* 与 hidden-presets）。扫描 bae-effects 的 src 和 packages 中出现的完整 matchName，设置初始实现状态。完成百分比按完成项数除以总项数计算。尚无真实汇报的能力显示“待认领”，初始更新时间为导入时间。

评估标签按领导的 fx-tags.md 整理在 tags.json。3D 通道和沉浸式视频整类后放。未指定策略的效果显示“待评估”。两个 AI 后续方向没有对应的 AE 26 matchName，保留在策略文件中。重启会保留已有汇报和数据。

有 gh 授权的开发机可用 `python3 server.py --github` 启动并导入 GitHub 开放及已合并的 PR。已知编号按清单归属，效果批次分支按前缀归入 fx-a，其他 PR 显示在动态里。导入按 PR 号去重，合并状态不会自动覆盖 agent 对能力的判断。

## Agent 汇报

在“里程碑”或“效果矩阵”搜索名称或 ID，复制名称下的完整 ID。效果 ID 以 `fx:` 开头，含空格时加引号。Agent 页面列出该 agent 当前负责的所有进行中项和最后汇报时间。

将管理员分发的 env 放在本机 `~/.config/bae-board/env`（Windows 为 `$HOME\.config\bae-board\env`），使用 UTF-8，每行一个 `键=值`，无需引号。跨机器使用部署地址。仅向有写入权限的 agent 分发 token。默认 agent 为机器名/当前分支，可设置环境变量或在 env 文件指定 `BAE_BOARD_AGENT`。

在包含本 PR 的工作树根目录执行：

```sh
tools/progress-web/report.sh expr-ui doing 336 '面板已接入，正在验证'
tools/progress-web/report.sh expr-ui done 336 '验证通过'
tools/progress-web/report.sh 'fx:ADBE Gaussian Blur 2' done
```

5090 的 Git Bash 可使用相同的路径和命令，会转调 Windows PowerShell。PowerShell 也可直接运行：

```powershell
.\tools\progress-web\report.ps1 -Item expr-ui -Status done -Pr 336 -Note '验证通过'
```

macOS/Linux 的 shell 脚本需要 Bash、Python 3、Git。Windows 脚本使用系统 PowerShell 和 Git。只有备注时，Bash 传空的第三参数，PowerShell 使用具名参数 `-Note`。

## 接口

| 方法和路径 | 凭证 | 返回或行为 |
|---|---|---|
| GET / | HTTP Basic | 中文看板 |
| GET /api/state | HTTP Basic | 全部 items、agents 和 activity |
| GET /api/events | HTTP Basic | 建连或状态变化即发 SSE 消息，空闲每 15 秒心跳。重连时重新获取完整状态 |
| POST /api/report | Bearer token | 接收 JSON，返回 changed 布尔值 |

报告示例：

```json
{"agent":"5090/ui","item":"expr-ui","status":"done","pr":336,"note":"验证通过"}
```

状态接受 todo、doing、done。相同能力只保留一项，相同 PR 只关联一次。当前状态、agent、备注完全一致且没有新 PR 时，返回 `{"changed":false}`，保留原时间、不追加动态。不同备注或状态会产生新动态。省略备注相当于提交空字符串。提交先前的状态会更新当前状态。并发汇报按 SQLite 事务依次生效。

提交新 ID 即创建能力，可加 `name` 和 `module`（默认为 ID 和“新增能力”）。前缀 `board-` 专用于看板工具自身的交付，放在 Agent 页并独立计数。既有效果使用页面显示的完整 ID。

## AWS / systemd 部署

1. 用 EC2 Instance Connect 或 SSM 连接选定的 Linux 实例。确认 8787 可用，且独立目录和同名服务未被其他应用使用。
2. 将 server.py、index.html 安装至 `/opt/bae-progress-web/`。在本机用 Python 的 SQLite backup API 制作一致性快照，连同 server.env 通过加密通道传至服务器。在服务停止时，将文件安装为 `/var/lib/bae-progress-web/board.sqlite3` 和 `server.env`，目录权限 0700，文件 0600；systemd 的 StateDirectory 会分配服务目录所有权。后续升级只更新代码，保留服务器数据库与凭证。
3. 将 bae-progress-web.service 安装至 `/etc/systemd/system/`，运行 `sudo systemctl daemon-reload` 和 `sudo systemctl enable --now bae-progress-web`。通过 `systemctl is-enabled bae-progress-web`、`systemctl is-active bae-progress-web` 和实际带认证的状态请求核验。服务无需管理员权限。
4. 有域名时在 HTTPS 入口转发到本服务，关闭 SSE 响应缓冲，保持长连接，把后端监听地址改为 127.0.0.1。临时 IP 模式按来源 IP 限制安全组 TCP 8787 的访问。HTTP Basic 和 Bearer 在 HTTP 链路上以明文传递。
5. 将各汇报机 env 的 BAE_BOARD_URL 改为已验证的 AWS 地址。用两个浏览器窗口验证汇报动态与实时更新。

制作一致性快照（目标路径为新文件）：

```sh
umask 077
python3 -c 'import sqlite3, sys; from pathlib import Path; sqlite3.connect(Path.home()/".config/bae-board/board.sqlite3").backup(sqlite3.connect(sys.argv[1]))' /private/tmp/bae-board-deploy.sqlite3
```
