# aectl：无人值守驱动 After Effects（macOS）

给开发 Agent 和脚本用：在本机 AE 里执行 ExtendScript，遇到弹框、卡死、崩溃都能自己处理，不需要人去点。

```sh
tools/ae/aectl start                      # 启动并等待脚本可用（一般 2–5 秒）
tools/ae/aectl eval 'app.version'         # 执行一段脚本，stdout 输出 JSON
tools/ae/aectl run path/to/script.jsx     # 执行脚本文件
tools/ae/aectl status | diagnose          # 状态 / 诊断（主线程调用栈、最近报错框原文、崩溃标记）
tools/ae/aectl stop                       # 不保存关闭工程并结束 AE
```

输出：`{"ok": true|false, "value": "<脚本最后一个表达式的字符串>", "error": {"message", "line", "file"}, "ms": …}`。
退出码：0 成功；1 脚本抛异常；3 出现模态框并已自动恢复（`error.evidence` 是现场文件）；4 超时。
选项：`--timeout <秒>`（默认 300）；`--force`：允许处理不是 aectl 启动的 AE（可能丢掉别人没保存的工作，慎用）。

## 写给 AE 的脚本要遵守

- **不要用 `alert()` / `confirm()` / `prompt()`**：会弹模态框。在 ExtendScript 里这几个全局函数是只读的，没法替换。结果和日志都写进文件或作为返回值。
- **交给 AE 的路径要用真实路径**：`Folder.create()` 碰到符号链接会失败，比如 `/tmp` → `/private/tmp`。请用 `/private/tmp/...`、`Folder.temp` 或主目录，并检查 `create()` 的返回值。
- `aectl` 会自动包上 `app.beginSuppressDialogs()` 和 try/catch，异常不会变成弹框。
- 全局变量可以照常用：脚本在全局作用域里执行（用 `$.evalFile`）。

## 它处理了哪些情况（都是实际遇到过的）

| 现象 | 原因 | aectl 的做法 |
|---|---|---|
| 启动后弹 "We detected a crash in your last session…"，建议安全模式，脚本调用一直挂着 | 上次 AE 被结束或崩溃，`com.Adobe.After Effects.<版本>.plist` 里留下了 `CrashOccurred` / `ForceQuitOccured` | 启动前删掉这两个标记、`com.Adobe.After Effects` 的 `AppStates` 会话记录，以及 Sentry 残留的 `*.run` |
| "Unable to execute script file. Scripting Plugin is not installed." | AE 还没加载完脚本插件（或者卡在上面那个框）时就发了 DoScript | 用户级 `Scripts/Startup/bae_automation_ready.jsx` 在脚本可用时写出 `ready.json`，等它出现后才发脚本 |
| 弹 "A copy of your project was saved at …/After Effects Crash_N.aep" | 用 `SIGTERM` 结束、甚至正常 `app.quit()`，AE 都会走崩溃流程 | 结束 AE 一律用 `SIGKILL`；`stop` 先不保存关闭工程再结束进程 |
| 脚本里调用了 `alert()` 等 | 原生模态框 | 主线程连续 3 次（约 9 秒）处在阻塞式模态循环里（`OS_Dialog::ModalLoop`、`runModalForWindow`、`_doModalLoop`），就判定卡住：保存现场 → 结束 → 清理 → 下次自动重启 |
| 长脚本运行时主线程出现 `runModalSession` | AE 在跑长操作时一边处理一边刷新界面，并没有在等人点 | 不算卡住 |
| 需要"允许脚本写入文件和访问网络" | 偏好 `Pref_SCRIPTING_FILE_NETWORK_SECURITY` | AE 关着的时候由 `aectl install` 打开（会先备份） |

检测卡死不需要辅助功能权限：用 `sample <pid>` 读主线程调用栈。对话框原文取自 AE 的 Sentry breadcrumb（`AEErrorDialog`）。每次恢复前，现场都会存到 `/private/tmp/bae-ae-automation/recoveries/`。

## 限制

- 只支持 macOS。Windows 版以后再说（AfterFX.exe `-r` / COM）。
- 恢复时会丢掉 AE 里没保存的内容。自动化脚本应该自己保存需要的结果。
- 同一时间只有一个 AE 实例。`start` / `eval` / `run` / `stop` 会拿一把互斥锁（`/private/tmp/bae-ae-automation/lock.d`），多个 Agent 同时调用时会自动排队（默认最多等 30 分钟，`--lock-timeout` 可调），持锁的进程退出后锁自动释放。用完**不要** `stop`：后面的调用会复用正在运行的 AE，`stop` 只在 AE 出问题时用。
