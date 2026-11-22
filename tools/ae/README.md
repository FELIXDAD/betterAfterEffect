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
退出码：0 成功；1 脚本抛异常；3 出现模态框并已自动恢复（`error.evidence` 是现场文件）；4 超时（按墙钟计；AE 会被重启。osascript 报 `AppleEvent timed out (-1712)` 也按超时处理，因为这时 AE 要么还在跑脚本、要么根本不处理 Apple 事件）；5 osascript 在脚本运行之前就失败了（Apple 事件没送到或被拒绝，`error.message` 带 osascript 的原始报错，立刻返回，不等超时）。测试时可以用环境变量 `BAE_AE_OSASCRIPT` 换掉 osascript。
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
| 主线程停在同一个不在执行脚本的调用栈上不动（实际遇到过：挂载磁盘卷时卡在 `DiskArbitration → NetFS → NSXPC` 的同步等待里） | 不是对话框，但 AE 永远不会去执行脚本 | 有脚本在等结果时，连续约 30 秒（10 次采样）主线程调用栈完全一样、并且不在执行 ExtendScript，就判定卡死，按同样的流程恢复 |
| 调用方的脚本超时后，下一个调用方也跟着超时 | osascript 放弃等待之后，AE 仍在执行那段脚本，后来的脚本只能排在它后面 | 超时就保存现场、结束 AE、清理，然后才放锁，下一个调用方会拿到一个干净的 AE。`--timeout` 要按脚本的实际耗时留足余量 |
| 长脚本运行时主线程出现 `runModalSession` | AE 在跑长操作时一边处理一边刷新界面，并没有在等人点 | 不算卡住 |
| 需要"允许脚本写入文件和访问网络" | 偏好 `Pref_SCRIPTING_FILE_NETWORK_SECURITY` | AE 关着的时候由 `aectl install` 打开（会先备份） |

检测卡死不需要辅助功能权限：用 `sample <pid>` 读主线程调用栈。对话框原文取自 AE 的 Sentry breadcrumb（`AEErrorDialog`）。每次恢复前，现场都会存到 `/private/tmp/bae-ae-automation/recoveries/`。

## 已知问题

- **这台 Mac 上 aerender 用不了**（2026-11-20，AE 26.2.1）：独立启动的 `aerender` → `aerendercore` 卡在 Adobe 授权库的初始化里（主线程停在 `aelicensing::DoSetup → licensingglue::LicenseSetup → … → adobe::ngl::internal::DataStorageFs::GetInstance → std::mutex::lock`），CPU 占用为 0，一直不出帧。这是本机授权环境的问题。**不要去改任何授权相关的文件或设置。** 绕开的办法：在已经运行的 AE 里用渲染队列渲染（`app.project.renderQueue.render()`，通过 `aectl run`），示例见 `docs/test-reports/m4/scripts/ae_time.jsx`。用 `sample <aerendercore 的 pid>` 可以确认是不是这种情况，确认后用 `kill -9` 结束 aerender 和 aerendercore。
- **计时前要清 AE 的缓存**：AE 会复用内存和磁盘缓存里已经渲好的帧，同一个合成第二次渲染会明显变快。测速时每次先 `app.purge(PurgeTarget.ALL_CACHES)`。
- **ExtendScript 里不要把 Error 对象直接和字符串拼接**：`"x" + e` 在 AE 里会再抛一个"Object of type Error found where a Number, Array, or Property is needed"，用 `e.message`。
- **AE 的输出模块格式在脚本里是只读的**，本机也没有 PNG 输出模板（只有 `TIFF Sequence with Alpha` 等），要 PNG 只能用 `comp.saveFrameToPng`，而且它返回时文件还没写完，要用新的 `File` 对象轮询 `exists` 和 `length`。

## 测试

- `tools/ae/tests/osascript_exit.sh`：osascript 提前失败时 aectl 要立刻报错（退出码 5），不能等到超时；osascript 报 -1712 时按超时处理并重启 AE（退出码 4）。需要本机装有 AE，会重启一次 AE，大约 15 秒。

## 限制

- 只支持 macOS。Windows 版以后再说（AfterFX.exe `-r` / COM）。
- 恢复时会丢掉 AE 里没保存的内容。自动化脚本应该自己保存需要的结果。
- 同一时间只有一个 AE 实例。`start` / `eval` / `run` / `stop` 会拿一把互斥锁（`/private/tmp/bae-ae-automation/lock.d`），多个 Agent 同时调用时会自动排队（默认最多等 30 分钟，`--lock-timeout` 可调），持锁的进程退出后锁自动释放。用完**不要** `stop`：后面的调用会复用正在运行的 AE，`stop` 只在 AE 出问题时用。
