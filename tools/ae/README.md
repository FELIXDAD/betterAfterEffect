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

## macOS 多实例实测（2026-11-24）

在 macOS 26.7.3 上，`open -n -g -a "Adobe After Effects 2026"` 能为 AE 26.2.1 启动新的 PID。实测用启动环境标记区分两个 `codex-multi` 测试进程，只向标记匹配的进程发脚本和 SIGKILL。默认进程一直保留。

| 检查项 | 结果 |
|---|---|
| 启动和弹框 | 单个额外进程加载了脚本，随后主线程卡在 `CEggApp::Dawdle → AnnoyUserPlus → ModalLoop`。同时启动两个额外进程时，两者都在 `CEggApp::BirthPrefs → ModalLoop` 阻塞，尚未到启动脚本阶段。 |
| 首选项和恢复 | 常规启动打开同一用户的 AE 26.2 首选项目录，两个进程也使用同一个版本下的 Sentry 数据库目录，分别创建会话记录。现有恢复会清理用户级崩溃标志及会话记录，影响范围超过单个 PID。给测试进程设置独立的 `HOME` 和 `CFFIXED_USER_HOME` 后，Sentry 路径移入临时目录，AE 仍卡在 `BirthPrefs`，临时目录里的启动脚本没有写出就绪文件。 |
| 磁盘缓存和自动保存 | 常规启动读取同一份首选项，其中磁盘缓存根目录为 `~/Library/Caches`，`open -n` 本身不会分配缓存目录。AE 的自动保存按首选项，可以放在项目旁边或指定目录，分别保存到独立的项目目录可以隔开项目旁的自动保存。双实例启动被弹框阻塞，未验证缓存写入和自动保存。 [Adobe 首选项说明](https://helpx.adobe.com/after-effects/desktop/get-started/preferences-and-settings/preferences.html) |
| 脚本路由 | `tell application id ...` 按 bundle ID 定位，两个实例的 ID 相同，无法用这条语句选 PID。为避免请求落到默认 AE，实测只用 `osascript -l JavaScript` 通过 `NSAppleEventDescriptor.descriptorWithProcessIdentifier` 直接指定测试 PID，`DoScript` 写出的启动标记与目标一致。`DoScriptFile` 在双实例启动模态框期间收到事件回复，但没有产出脚本写入的结果。 [AppleScript 应用寻址](https://developer.apple.com/library/archive/documentation/AppleScript/Conceptual/AppleScriptLangGuide/reference/ASLR_classes.html) · [Apple Event PID 寻址](https://developer.apple.com/documentation/applicationservices/apple_event_manager/1542936-typekernelprocessid) |

这台 Mac 继续使用单实例排队。文件收件箱也依赖 AE 先运行启动脚本，无法解决上述启动阻塞。启动这一关没有通过，因此两路各 20 次 eval、并行渲染和独立崩溃恢复均未验证。

## Worker 排队

运行 golden 或批量操作时，用 `aectl lock` 把整组操作放在同一把共享锁里：

```sh
tools/ae/aectl lock -- tools/ae/aectl eval 'app.version'
tools/ae/aectl lock --lock-timeout 600 -- bash path/to/golden.sh
```

它通过原子创建 `/private/tmp/bae-locks/ae.lock` 排队，与现有手动创建目录或文件的 worker 兼容。默认最多等 30 分钟，等待超时返回 1，拿到锁后返回所执行命令的退出码。命令结束时释放锁，无需单独解锁。若持锁进程被 SIGKILL，确认持有者和它启动的任务都已退出后，再清理遗留的锁。已经手动持有共享锁的脚本可以继续直接调用 `aectl`。

`start` / `eval` / `run` / `stop` 仍使用 `/private/tmp/bae-ae-automation/lock.d` 串行执行单条命令。调用方用 `lock` 保证多条命令之间也没有别的共享锁持有者插队。实例参数省略或写 `--instance default` 均指向现有 AE，其他实例名会报错。

## 测试

- `bash tools/ae/tests/shared_lock.sh` 检查排队、兼容现有锁、超时、退出码及释放行为。它在临时目录中运行，无需启动 AE。
- `tools/ae/aectl lock -- bash tools/ae/tests/osascript_exit.sh` 检查 osascript 提前失败和 -1712 超时处理（退出码分别是 5 和 4）。需要本机装有 AE，会结束一次 AE，下一条正常 eval 会重新启动它。

## 限制

- 只支持 macOS。Windows 版以后再说（AfterFX.exe `-r` / COM）。
- 恢复时会丢掉 AE 里没保存的内容。自动化脚本应该自己保存需要的结果。
- 用完 AE 后让它继续运行，供后续调用复用。`stop` 留给需要恢复 AE 的情形。
