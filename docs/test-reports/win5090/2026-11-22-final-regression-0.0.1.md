# 0.0.1 最终回归 · Windows（缩减版 C）

- 日期：2026-11-22
- 执行：米兰 Mac 上的 Claude 远程操作 5090（只测试，不改代码）
- 范围：只测 Windows 特有的风险；共用的界面逻辑以 macOS 回归为准

## 结论

- **没有发现 0.0.1-blocker。** 发布包在带空格和中文的路径下能启动、新建、保存、另存为、重新打开、渲染、退出提示都正常。
- 新开的 issue（都是 `bug`，不阻塞发布）：
  - #215 关闭项目后，合成和时间轴面板标签仍显示旧合成名（共用代码，macOS 上应该也有）
  - #216 Windows：渲染队列默认输出目录优先用 `%USERPROFILE%\Movies`，忽略被移动过的“视频”已知文件夹
  - #217 Windows 发布包：README 没写 SmartScreen“仍要运行”的放行步骤；`bae.exe` 没有版本资源
- 建议：第 1 批的结果支持发布。#217 的 README 说明建议在发布前补上（只改文档）。
- 后续批次的结论见下面各节（做完一批更新一次）。

## 环境

| 项 | 值 |
|---|---|
| dev 提交 | `160fab5b6f07f306b4983571549198a4bf496057`（Merge PR #98 release/0.0.1） |
| 被测包 | CI run 44995382166，artifact `bae-x86_64-pc-windows-msvc` |
| zip | `BAE-0.0.1-x86_64-windows.zip`，12,729,147 字节，SHA256 `FB8A697FFFA207913C8BEC21F3FC9EEC6418EE8E2A258C04016C749C453433F0` |
| exe | `bae.exe` 32,380,928 字节（30.88 MiB，预算 32 MiB，`size-report.json` pass）；`bae.exe --version` = `bae 0.0.1 (x86_64-windows, release)` |
| Windows | Windows 11 专业版 25H2，10.0.26200 |
| 显卡 | NVIDIA GeForce RTX 5090 D v2，驱动 32.0.16.1692；BAE 状态栏：`NVIDIA GeForce RTX 5090 D v2 (D3D12, Discrete GPU)` |
| 显示 | 主屏 3840×2160，150%（BAE 状态栏 `2160×1320 @1.50x`） |
| 输入法 | 只装了中文（简体）· 微软拼音（Preload = 00000804） |

### 操作方式

米兰到 5090 的 Orca 配对没有 `desktop-control` 权限，`orca computer …` 全部返回 forbidden。所以界面操作全部改在 5090 的终端里用 PowerShell 做：

- 控件：`System.Windows.Automation`（BAE 的 accesskit 树，88 个左右的元素）；
- 菜单：BAE 的菜单栏是原生 Win32 菜单，用 `GetMenu` 枚举（646 项）后 `WM_COMMAND` 调用；
- 键鼠：`keybd_event` / `SetCursorPos` + `mouse_event`，每次输入前检查前台窗口必须是 bae.exe；
- 文字：剪贴板粘贴（避开微软拼音的组字）；
- 原生文件对话框：置前后 Alt+N、粘贴路径、回车。

## 自动化结果

规划者的后台脚本在 5090 上对 `160fab5` 源码跑（日志 `E:\bae-test\final-0.0.1\logs\`），没有设置 `WGPU_DX12_COMPILER`：

| 项 | 结果 |
|---|---|
| `cargo test -p bae` | 退出 0；2 个测试集，54 passed / 0 failed |
| `cargo test --workspace`（默认适配器，RTX 5090 DX12） | 退出 0；117 个测试集，1059 passed / 0 failed / 6 ignored |
| `cargo test --workspace`（`BAE_GPU_ADAPTER=software`，WARP） | 退出 0；117 个测试集，1059 passed / 0 failed / 6 ignored |
| `bae-render --test render` 在 WARP 下循环 50 次 | 0 次 0xC0000005，0 次其他失败 |
| `bae-text --test cpu` 在 WARP 下循环 50 次 | 0 次 0xC0000005，0 次其他失败 |

## 第 1 批：发布包首次运行

zip 复制到 `E:\bae-test\final 0.0.1\发布 包\`，加 Zone.Identifier（`[ZoneTransfer] ZoneId=3`），用 `Shell.Application` 的 `CopyHere` 解压到 `…\发布 包\BAE\`。解压出的 `bae.exe`、`LICENSE`、`README.md` 都带上了 `ZoneId=3`。

| 用例 | 结果 | 说明 |
|---|---|---|
| 来自网络的 exe 首次启动 | 通过（有提示，见 #217） | `Start-Process`（ShellExecute）和资源管理器打开都弹 SmartScreen“Windows 已保护你的电脑”，只有“不运行”；“更多信息”里是“应用：bae.exe / 发行者：发布者未知”；点“仍要运行”后 3 秒内出现主窗口。README 没写这一步 → #217 |
| 从带空格和中文的路径启动 | 通过 | `E:\bae-test\final 0.0.1\发布 包\BAE\bae.exe`，欢迎框 ▸ Motion 正常进入 |
| 关于里是 0.0.1 | 通过 | 帮助 ▸ 关于 BAE：`BAE 0.0.1 (x86_64-windows, release)`、`License: Apache-2.0` |
| exe 版本资源 | 未通过 #217 | `VersionInfo.FileVersion/ProductVersion` 都是空的 |
| 新建合成 | 通过 | Ctrl+N 打开合成设置，名字改成“合成 测试”，确定后项目里出现合成，标题栏加 `*`；Ctrl+Y 加纯色层正常 |
| 保存为 .bae（中文路径） | 通过 | Ctrl+S 弹原生“Save As”对话框（主窗口被禁用），存到 `E:\bae-test\final 0.0.1\工程 测试\回归 工程.bae`：生成目录，里面有 `project.json`、`sublayers\root.loro`；标题栏变成 `BAE - 回归 工程.bae` |
| 另存为 .baez | 通过 | 按已知情况 Ctrl+Shift+S 被系统热键占用，走菜单 文件 ▸ 另存为 ▸ 另存为…；存成 `回归 打包.baez`（3,412 字节 zip，内含 `project.json`、`sublayers/root.loro`），标题栏更新 |
| 重新打开 .bae | 通过 | 关闭项目后 Ctrl+O；对话框过滤器是 `BAE Project (.baez, project.json)`，输入 `.bae` 路径会进入目录，选里面的 `project.json` 打开；合成、纯色层、`comps`/`solids` 文件夹都在 |
| 重新打开 .baez | 通过 | Ctrl+O 选 `.baez`，内容完整 |
| 关闭项目后的界面 | 未通过 #215 | 面板标签仍是“Composition 合成 测试”“Timeline 合成 测试”（显示残留，无数据问题） |
| 渲染队列输出 PNG 到中文路径 | 通过 | Ctrl+M 加入渲染队列；输出改成 `E:\bae-test\final 0.0.1\渲染 输出\帧_[####].png`，帧 0–4；状态 `Done · 5`；生成 `帧_0000.png`…`帧_0004.png`，1920×1080，中心像素 (45,45,45) 与深灰纯色一致 |
| 渲染队列默认输出目录 | 未通过 #216 | 默认是 `C:\Users\<用户>\Movies\合成 测试\…`；这台机器“视频”已知文件夹在 D 盘 |
| 有未保存修改时退出 | 通过 | 改动后 Ctrl+Q 弹原生对话框“Save changes to “回归 打包.baez” before closing?”（是 / 否 / 取消）；“取消”留在应用、`*` 还在；点窗口关闭（WM_CLOSE）再次提示，“否”后进程退出，磁盘上的 .baez 没变 |

## 第 2 批：键盘

（进行中）

## 第 3 批：渲染（DX12 真显卡）

（进行中）

## 第 4 批：DPI

（进行中）

## 第 5 批：资源管理器拖放和原生文件对话框

（进行中）

## 测试过程中的意外

- 一次脚本错误（按名字没找到控件时点到了屏幕左上角 (0,0)），点中了 5090 上的 Orca 窗口，随后的一次粘贴和回车把字符“4”发进了 Orca 里一个空闲的 agent 会话（它回了一句澄清问题，没有执行任何操作）。之后所有键鼠操作都加了“前台必须是 bae.exe、控件必须找到”的检查。

## 没做的批次

（全部做完后更新）

## 建议补的用例（留给 macOS 验收 agent）

- 关闭项目后检查合成 / 时间轴 / 渲染队列面板标签和状态都清空（#215）。
