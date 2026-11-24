# QA 工具：AE 快捷键扫描

按 AE 命令目录（`crates/bae-ui-motion/src/catalog`，生成的 `docs/ui/ae-commands.md` 和 `docs/ui/ae-shortcuts.json`）逐条检查 BAE 的 AE 默认快捷键。分两层：

| 层 | 在哪跑 | 做什么 |
|---|---|---|
| CI 层 | `apps/bae` 的 `acceptance::shortcut_sweep`（`cargo test -p bae acceptance::shortcut_sweep`），CI 两个平台每次都跑 | 每条本平台能绑定的 AE 快捷键：让 AE 上下文对应的面板获得焦点，按 egui-winit 的方式发按键（Cmd/Ctrl+C/X/V 带其他修饰键时发 Copy/Cut/Paste 事件），断言已实现的命令只触发一次目录里的动作；未实现的命令按下什么都不做；同一作用域没有冲突；菜单项置灰时不显示快捷键。设 `BAE_SHORTCUT_REPORT=<文件>` 写覆盖率报告，CI 把 `shortcut-sweep.md` 作为 artifact 上传。 |
| 真机层（macOS） | 本机：`tools/qa/shortcut_sweep_mac.py` | 启动 release 版 BAE（`BAE_HOME` 指向临时目录，不碰用户自己的设置和工程；`BAE_ACTION_LOG=<文件>` 让 BAE 把执行的每个动作写成一行 JSON），用 `orca computer hotkey` / `press-key`（后台输入，不抢前台）逐条发真实按键（orca 还发不了 F 键，这些记为没按、由 CI 层覆盖），读动作日志核对，出报告。持有本机统一的界面目录锁（`--lock <目录>` 或 `BAE_GUI_LOCK`，mkdir 加锁，排队等待，一次不超过 20 分钟）。 |
| 真机层（Windows） | 5090：`tools/qa/shortcut_sweep_windows.ps1` | 同上，用 SendInput 发键（需要把 BAE 放到前台）。 |

## macOS 真机扫描

```sh
cargo build --release -p bae
tools/qa/shortcut_sweep_mac.py --bae target/release/bae --lock <界面目录锁> \
    --report docs/test-reports/m4/$(date +%F)-shortcut-sweep.md
# 量完删掉 target/release（几 GB）
```

- 先建一个测试工程：Cmd+N（合成设置，回车）、Cmd+Y（纯色设置，回车），纯色图层选中。
- 只测全局快捷键；面板作用域的键由 CI 层覆盖（真机不点面板）。
- 会退出、关闭工程或面板、打开文件 / 保存对话框或浏览器的命令跳过（脚本里的 `SKIP`）；macOS 自己占用的组合键（帮助菜单搜索 Cmd+Shift+/、调度中心 Ctrl+方向键、截屏等，`SYSTEM`）不按。
- 某个键没有触发预期的动作时，重启 BAE 再建测试工程，后面的键从干净状态开始。
- 每个键之后按两次 Esc 关掉打开的对话框；BAE 退出了就记失败并重新启动。
- 报告：总数、已实现、真机通过 / 失败清单、跳过、面板作用域、AE 有默认键但 BAE 还没实现的清单（按负责人）。

## Windows 真机扫描（5090）

```powershell
cargo build --release -p bae
powershell -ExecutionPolicy Bypass -File tools\qa\shortcut_sweep_windows.ps1 `
    -Bae target\release\bae.exe -Report docs\test-reports\win5090\<日期>-shortcut-sweep.md
```

SendInput 发给前台窗口：脚本会把 BAE 放到前台，跑的时候不要动键盘鼠标。

## 动作日志

`BAE_ACTION_LOG=<文件>`：BAE 的外壳把分派的每个动作（快捷键、菜单、面板请求）追加成一行 `{"t": <启动后毫秒>, "id": "<动作 id>", "params": {…}}`（`apps/bae/src/action_log.rs`）。只读，不影响行为；没设这个变量时什么都不写。
