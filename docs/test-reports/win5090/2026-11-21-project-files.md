# Windows 实机：工程保存与打开（#13 / #93）

- 日期：2026-11-21
- 机器：Windows 11 + **NVIDIA GeForce RTX 5090 D v2**（D3D12）
- BAE：`origin/dev` @ **`6e4fb38`**（`Merge pull request #93`）
- 设置：**默认**（未设 `WGPU_DX12_COMPILER`）；`BAE_GPU_VALIDATION=0`
- `bae.exe` release：**28.28 MB**（29653504 bytes）
- 锁：`gui.lock`；`--screenshot-scale 1`
- 截图：[`project-files-shots/`](./project-files-shots/)（≤8 张）

## 步骤结果

| # | 步骤 | 结果 | 证据 |
|---|---|---|---|
| 1 | 欢迎框：关闭按钮 / Esc / 点框外 | **通过**（各重启一次）。三种均可关 | `01-welcome.jpg`, `01a-after-x.jpg`, `01b-after-esc.jpg`, `01c-after-outside.jpg` |
| 2 | 新建合成 + 纯色 + 矩形 + 两位置关键帧；标题「未保存」 | **通过**。标题变为 `BAE - Untitled Project *`；时间轴可见 Shape Layer Position 两关键帧 | `02-layers-keyframes.jpg` |
| 3 | 另存为 → 原生对话框 → `E:\bae-test\acc\proj1.bae` | **通过（有保留）**。`list-windows --app bae` **只列主窗**；Win32 枚举可见同进程独立窗 **`Save As`**。写入路径后回车，目录落盘（约 **4.5 KB** 量级含 `project.json`+loro），标题 → `BAE - proj1.bae`（`*` 消失） | `03-saved.jpg`；本机枚举日志 |
| 4 | 加文本 → `*` → Ctrl+S | **通过**。出现 `*`；Ctrl+S **未再弹对话框**；`project.json` mtime 更新（11:50:28 → 11:50:56）；标题回到无 `*` | 日志 `step4` |
| 5 | 关闭：保存/不保存/取消 | **部分通过**。File ▸ Close 后出现 `#32770` 标题 **`BAE`** 的 MessageBox（Yes/No/Cancel）。取消后工程仍在（标题仍带路径）。后续自动化曾把主窗卡在 **disabled**（模态未干净释放），「不保存」路径未稳定复测完 | `05-close-cancel.jpg` |
| 6 | 打开 proj1 | **未完整闭环**（受步骤 5 模态卡住影响） | — |
| 7 | `bae.exe proj1.bae` CLI 启动 | **部分**：曾启动到 `BAE - proj1.bae *`；内容完整性未用截图确认三图层 | — |
| 8 | 另存 `.baez` / 再打开 | **未完成**（时间转 #12） | — |
| 9 | 未保存改动直接关窗 | **未完成** | — |
| 10 | `BAE_LANG=zh` 菜单/对话框中文 | **未完成** | — |

## 重要观察

1. **原生对话框对 orca `list-windows --app bae` 不可见**，但属于 bae 进程的顶层窗（`Save As` / `#32770|BAE`）。测法须辅以 Win32 枚举。
2. 落盘的 `proj1.bae/project.json` 仅含 `format/version/layer_stack`（**80 B**），实质数据在 `sublayers/root.loro`（~5 KB）。是否含完整合成/图层/关键帧需打开读回确认——本轮读回未闭环，**不能宣称内容完整往返已验收**。
3. 欢迎框现为 `egui::Modal`，有关闭图标；Esc / 点框外 / `should_close()` 均可关。

## 结论（给 0.0.1）

- **保存/另存为/增量 Ctrl+S 主路径可用**（对话框会出现并落盘，脏标记行为正确）。
- **关闭未保存提示会出现**（Windows MessageBox Yes/No/Cancel）。
- **打开 / `.baez` / 中文提示 / 关窗提示** 本轮未完整测完；建议补一轮专项（避免与导出测速抢锁）。

## 环境

```
git: 6e4fb38 Merge pull request #93 from FELIXDAD/app/project-files
WGPU_DX12_COMPILER: <unset>
```
