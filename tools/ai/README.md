# aictl：无人值守驱动 Illustrator（macOS）

给开发 Agent 和脚本用：在本机 Illustrator 里执行 ExtendScript，遇到弹框、卡死、崩溃都能自己处理，不需要人去点。做法参照 `tools/ae/aectl`。

```sh
tools/ai/aictl start                      # 启动并等待脚本通道可用（一般 5–7 秒）
tools/ai/aictl eval 'app.version'         # 执行一段脚本，stdout 输出 JSON
tools/ai/aictl run /private/tmp/x.jsx     # 执行脚本文件
tools/ai/aictl status | diagnose          # 状态 / 诊断（主线程调用栈、窗口、runner 日志、CEP 日志）
tools/ai/aictl stop                       # 不保存关闭所有文档并结束 Illustrator
tools/ai/aictl install | uninstall        # 安装 / 卸载脚本通道扩展（start 会自动安装）
```

输出：`{"ok": true|false, "value": "<脚本最后一个表达式的字符串>", "error": {"message", "line", "file"}, "ms": …}`。
退出码：0 成功；1 脚本抛异常；3 出现模态框或卡死（`error.evidence` 是现场文件）：脚本自己弹出的框、卡死、文件授权阻塞已自动恢复（`error.recovered: true`），不是脚本弹出的框原样留给人处理（`error.recovered: false`，见下表）；4 超时，已重启 Illustrator；5 Illustrator 在执行中退出（崩溃）。
选项：`--timeout <秒>`（默认 300，按墙钟计）；`--force`：允许处理不是 aictl 启动的 Illustrator（可能丢掉别人没保存的工作，慎用）；`--lock-timeout <秒>`（默认 1800）。
状态目录：`/private/tmp/bae-ai-automation`（`BAE_AI_STATE` 可改），恢复现场在其中的 `recoveries/`。

## 原理

- **脚本通道是一个隐藏的 CEP 扩展**（`tools/ai/cep/`，装到 `~/Library/Application Support/Adobe/CEP/extensions/org.bae.aictl/`）。它在 Illustrator 里每 100 ms 看一次队列目录，用 CEP 的 `evalScript` 执行 aictl 放进去的脚本，每秒写一次心跳。没有用 AppleEvent（`do javascript`）：那要 macOS「自动化」授权，第一次会弹框等人点「允许」。
- 未签名的 CEP 扩展只有在调试模式下才会加载：`defaults write com.adobe.CSXS.12 PlayerDebugMode 1`（12 是 Illustrator 30 的 CEP 版本，aictl 自动识别）。**这个 key 是本机所有 CEP 宿主共用的**（Illustrator、Premiere、After Effects），只能设，不要删；aictl 每次启动前都会检查。
- 扩展靠 Illustrator 的 `documentAfterActivate` 事件启动：锁屏或后台启动时应用不会被激活，`applicationActivate` 不会来。所以 `start` 启动 Illustrator 时顺带打开一个 8×8 的空 SVG，扩展起来后再把它关掉。
- 每段脚本都包在 `app.userInteractionLevel = DONTDISPLAYALERTS` 和 try/catch 里，结果和错误写进 JSON 文件，不弹框。
- 卡死检测不需要辅助功能权限：用 `sample <pid>` 看主线程调用栈。

## 写给 Illustrator 的脚本要遵守

- **只读写 `/private/tmp` 这类不受保护的位置。** Illustrator 没有访问 `~/Documents`、`~/Desktop`、`~/Downloads`、iCloud 云盘和外接卷的授权，一碰就会弹 macOS 隐私授权框，`open()` 一直阻塞，直到有人点。仓库通常就在 `~/Documents` 下，所以要先把脚本和素材复制到 `/private/tmp`，结果也写在那里，再由 shell 拷回仓库（`bridges/ai/tests/regen.sh` 就是这样做的）。`aictl run` 遇到受保护目录里的脚本会自动复制一份再执行，但脚本里如果再去读写这些目录，照样会卡住，aictl 会在约 15 秒后识别出来并重启。
- 路径用真实路径：`/private/tmp/...`，不要用 `/tmp/...`（ExtendScript 的 `Folder.create()` 碰到符号链接会失败）。
- 不要用 `alert()` / `confirm()` / `prompt()`，也不要打开文件对话框（`File.saveDialog`、`saveDlg`、`openDlg`），参数通过全局变量传进去。
- 全局变量挂在 `$.global` 上，不要用 `this`：用 `$.evalFile` 从函数里执行的文件，`this` 是那个函数，不是全局对象。
- `breakLink()`、`createOutline()` 这类操作只对**当前文档**生效，对别的打开的文档静默不做事：先 `app.activeDocument = doc`。
- 所有调用共用 Illustrator 的主 ExtendScript 引擎，全局变量会在调用之间保留。自己打开的文档自己关掉（`doc.close(SaveOptions.DONOTSAVECHANGES)`）。
- 用完**不要** `stop`：后面的调用会复用正在运行的 Illustrator。`stop` 只在需要干净环境或测试结束时用。

## 它处理了哪些情况（都是 2026-11-20 在 Illustrator 30.4 上实际遇到过的）

| 现象 | 原因 | aictl 的做法 |
|---|---|---|
| 驱动 Illustrator 要弹「想要控制 Adobe Illustrator」的授权框 | AppleEvent 需要 macOS 自动化授权 | 改用 CEP 扩展当脚本通道，不发 AppleEvent |
| 扩展不加载，CEP 日志里是 `Signature verification failed` | `com.adobe.CSXS.12` 的 `PlayerDebugMode` 被删掉了（共用 key，别的 agent 收尾时清理过） | 启动前检查并设回 1；启动中发现签名错误就设回并重启 |
| 后台启动后扩展一直不启动 | 锁屏 / `open -g`，应用从不激活，`applicationActivate` 不触发 | 启动时打开一个空 SVG，触发 `documentAfterActivate`；30 秒还没起来就再打开一次 |
| 脚本调用 `alert()`，主线程停在 `ScCore::Dialogs::alert → NSAlert runModal` | 原生模态框 | 模态循环位于正在执行的脚本之上（调用栈更深），连续 3 次采样（约 9 秒）就判定卡住：保存现场（含对话框窗口截图）→ SIGKILL → 下次自动重启 |
| 脚本打开了「存储为」对话框（`NSSavePanel runModal`） | 交互式脚本在无人值守时走到了对话框分支 | 同上，报告为 `file-dialog` |
| 脚本卡在 `ScCore::File::open → fopen → __open_nocancel` | 访问 `~/Documents`，macOS 在等人回答隐私授权框 | 主线程连续约 15 秒停在同一个文件系统调用上，就恢复并提示把文件放到 `/private/tmp` |
| 超时之后下一个调用方也跟着超时 | Illustrator 还在执行被放弃的脚本 | 超时就保存现场、结束、清理，再放锁，下一个调用方拿到干净的 Illustrator |
| 设了 `--timeout 300`，实际等了 450 秒 | 每次 `sample` 本身要 1 秒多，按循环次数计时会被拉长 | 改成按墙钟（`$SECONDS`）计时 |
| Illustrator 在执行中退出 | 崩溃或被别人结束 | 退出码 5，附上 `~/Library/Logs/DiagnosticReports` 里最新的崩溃报告路径 |
| 下次启动弹「恢复文档」 | 上次被结束时有未保存的文档，留在 `DataRecovery` 里 | 启动前把 `~/Library/Preferences/Adobe Illustrator 30 Settings/*/DataRecovery/` 里的文件移到 `recoveries/` 下（不删除） |
| 长脚本执行时主线程里有 `runModalSession` | Illustrator 一边执行一边刷新界面 | 不算卡住 |
| 启动时或空闲时主线程卡在模态框里，调用栈里没有正在执行的脚本 | 不是脚本弹出的框（Illustrator 或别的程序、系统的提示） | **不点、不关、不重启**：保存现场（含对话框窗口截图），报错退出，`recovered: false`，还没被取走的脚本从队列撤回。要放弃它只能由人决定 `aictl stop --force`（会丢掉未保存的内容）。唯一例外是 Illustrator 自己的「恢复文档」提示：保存现场后结束进程重启，最多 3 次 |

## 限制

- 只支持 macOS。Windows 以后再说：CEP 扩展这条路在 Windows 上同样可行。
- 对话框截图要用 `swift`（Xcode 命令行工具）列窗口，没有也不影响恢复，只是现场里没有图。截的只是 Illustrator 自己的小窗口，不截整个屏幕。
- 恢复时会丢掉 Illustrator 里没保存的内容。自动化脚本应该自己保存需要的结果。
- 同一时间只有一个 Illustrator 实例。`start` / `eval` / `run` / `stop` 会拿一把互斥锁（`/private/tmp/bae-ai-automation/lock.d`），多个 Agent 同时调用时自动排队，持锁的进程退出后锁自动释放。

## 测试

CI（macOS、Windows）上没有 Illustrator，需要真机的测试默认跳过，在装了 Illustrator 的 Mac 上用环境变量打开：

```sh
BAE_TEST_ILLUSTRATOR=1 cargo test -p bae-interop-ai --test illustrator live_illustrator -- --nocapture
```

它先检查 aictl 的约定（`eval` 的返回值，脚本抛异常时退出码 1），再用 `bridges/ai/tests/regen.sh --out <临时目录>` 在 Illustrator 里重建 16 的 5 个测试文档并导出，对新数据跑与固定数据相同的检查；新数据和仓库里的不一致时只提示。最后让脚本调用 `alert()`，检查 aictl 识别出这个框并恢复（退出码 3，`recovered: true`，Illustrator 会被结束）。全程约 30 秒（Illustrator 已经开着时）。
