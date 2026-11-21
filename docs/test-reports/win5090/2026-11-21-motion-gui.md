# Windows 实机动效工作区界面实测（#64/#67）

- 日期：2026-11-21
- 测点：`origin/ui/motion-gpu-preview` @ **`79db75d09493ccc41b6b3288b882457f7037fde7`**（短哈希 `79db75d`；叠在 #64 上，Windows CI 已绿）
- 机器：Windows 11 + RTX 5090 D v2 / D3D12
- 约定：先默认、再 `WGPU_DX12_COMPILER=fxc`；截图以 fxc 为准
- 约束：只测试；临时置前脚本在仓库外，未提交

## 结论摘要

| 步骤 | 默认 | fxc | 说明 |
|---|---|---|---|
| 0 构建 | — | — | `cargo build --release -p bae` 成功 |
| 1 启动截图 | **通过** | **通过** | 均 DX12/5090D；frame ~1–7 ms；无崩溃 |
| 2 新建合成 Comp 1 | **部分通过** | **受阻** | 默认曾出现 Composition/Timeline「Comp 1」；fxc 多次 Ctrl+N / 按钮点击未稳定复现 |
| 3 矩形 + 纯色图层 | **受阻** | **受阻** | 见下 |
| 4–7 关键帧 / F9 / 空格播放 / 拖动撤销 | **未做** | **未做** | 依赖步骤 3 |
| 8 `BAE_LANG=zh` | **未做** | **未做** | 同上 |

**总评：** 应用在默认与 fxc 下均能稳定启动真显卡窗口；Welcome 可关掉。后续 AE 式交互因 **egui 无障碍树** + **1.5x DPI 下坐标点击不准** + **工作区切到 Review/Agent 后快捷键无效** 而卡住。需本机人工把窗口留在 **Motion** 并完成图层/关键帧步骤后补测。

## 环境与构建

```
git fetch origin
git checkout --detach 79db75d
cargo build --release -p bae   # OK
```

启动日志（默认与 fxc 同类）：

```
GPU device created adapter=NVIDIA GeForce RTX 5090 D v2 backend=Dx12 device_type=DiscreteGpu
window surface: Bgra8Unorm, Fifo
```

状态栏可见：`NVIDIA GeForce RTX 5090 D v2 (D3D12, discrete)`、`2160x1320 @ 1.50x`。

## 双跑细节

### 默认（不设 `WGPU_DX12_COMPILER`）

1. 启动成功，Welcome 对话框出现 → 截图 [`motion-gui/01-launch-default.png`](./motion-gui/01-launch-default.png)
2. Alt+`SetForegroundWindow` 可置前；Esc/Enter 关不掉 Welcome；Win32 鼠标扫点可关掉 Welcome（易误点到 Review/Agent）
3. 在某一轮点击「New Composition」后得到 **Composition Comp 1 / Timeline Comp 1** → [`motion-gui/02-comp1-default.png`](./motion-gui/02-comp1-default.png)；History 有「New Composition」
4. 时间轴右键 / Ctrl+Y 未稳定建出 Shape/Solid；工作区多次落在 Review/Agent，Motion 快捷键（`Cmd+N`/`Cmd+Y`/F9/Space 等挂在 AE keymap）不生效
5. 进程未崩溃；stderr 无 panic

### fxc（截图/观感以本遍为准）

1. 启动成功 → [`motion-gui/03-launch-fxc.png`](./motion-gui/03-launch-fxc.png)
2. Welcome 关掉后常停在 **Review** → [`motion-gui/04-after-welcome-fxc.png`](./motion-gui/04-after-welcome-fxc.png)
3. 扫点切 Motion、Ctrl+N、点「New Composition」多次仍常停在空合成提示 → [`motion-gui/05-empty-fxc.png`](./motion-gui/05-empty-fxc.png)
4. 无崩溃；未完成图层/关键帧/播放/撤销/中文界面

## 受阻原因（给后续补测）

1. `orca computer`：egui 内容无 element；坐标点击需前台，`--restore-window` 曾被 Windows 拒绝；改用仓库外 Win32 Alt+置前 + `mouse_event`（不提交）
2. UI scale **1.50x**：物理窗口与逻辑坐标不一致，欢迎框/工作区标签易点偏
3. 欢迎框与工作区标签相邻操作易进入 Review/Agent，导致 Motion 专用快捷键无效
4. 时间轴右键菜单坐标菜单项未可靠命中

**补测需要：** 人工将 BAE 留在 **Motion** 工作区前台，或提供稳定的自动化入口（无障碍 / 命令行造场景）。

## 截图清单（≤10，均 &lt; 400 KB）

| 文件 | 内容 |
|---|---|
| `motion-gui/01-launch-default.png` | 默认启动 + Welcome |
| `motion-gui/02-comp1-default.png` | 默认路径 Comp 1 |
| `motion-gui/03-launch-fxc.png` | fxc 启动 |
| `motion-gui/04-after-welcome-fxc.png` | fxc 关 Welcome 后（Review） |
| `motion-gui/05-empty-fxc.png` | fxc 仍无合成 |

## 问题列表

1. 动效 GUI 自动化在 Windows + 1.5x DPI 下不可靠（非 GPU 崩溃问题）
2. 默认路径曾成功建 Comp 1，fxc 路径未稳定复现——更像输入命中问题，不像 fxc/DXC 渲染差异
3. 步骤 3–8 待人工或改进自动化后补测；本 PR 先记录已证实的启动/GPU/部分合成结果

---

## 重测（#6 之后 / 2026-11-21 午）：缩放与快捷键解法

- 测点：再次 `git fetch` 后 `origin/ui/motion-gpu-preview` @ **`79db75d09493ccc41b6b3288b882457f7037fde7`**（未变）
- 环境：`WGPU_DX12_COMPILER=fxc`，`BAE_GPU_VALIDATION=0`；状态栏仍 `2160x1320 @ 1.50x`，GPU `RTX 5090 D v2 (D3D12)`
- 方法：`orca computer get-app-state` / `click` / `hotkey` 均加 `--screenshot-scale 1`；前台用已授权的 `--restore-window`（拿 `gui` 锁后可用）。`--restore-window` 被拒时用仓库外 Alt+`AttachThreadInput`/`SendInput` 后备（不提交）
- 机器锁：遵守 `E:\bae-test\locks\`（本测持有 `gui.lock`）

### 重测结论表（fxc）

| 步骤 | 结果 | 说明 |
|---|---|---|
| 1 启动截图 | **通过** | DX12/5090D；frame ~2–7 ms；无崩溃 → [`retest-01-launch.jpg`](./motion-gui/retest-01-launch.jpg) |
| 2 新建合成 | **通过** | 先点合成/项目空白再 `Ctrl+N`；亦可用查看器中央「New Composition」。出现 Comp 1（后又 Ctrl+N 出 Comp 2）→ [`retest-03-comp1.jpg`](./motion-gui/retest-03-comp1.jpg) |
| 3 矩形 + 纯色 | **部分通过** | `Ctrl+Y` 稳定新建 **Solid 1** 并在查看器可见 → [`retest-04-solid.jpg`](./motion-gui/retest-04-solid.jpg)。时间轴右键「新建 → 矩形」菜单偶发出现「New ▸」，子项坐标命中不稳定，**矩形形状层未稳定建出**（反馈给 12） |
| 4 位置关键帧 | **部分通过** | 焦点在时间轴时 `P` 展开 Position；`Shift+PageDown`×6 到约 2s；`Alt+Shift+P` 可见关键帧菱形 → [`retest-05-position-p.jpg`](./motion-gui/retest-05-position-p.jpg) [`retest-06-at-2s.jpg`](./motion-gui/retest-06-at-2s.jpg)。查看器拖动改位置有时命中、有时仍停在 960,540 |
| 5 F9 缓动 | **部分通过** | `orca computer press-key --key F9` 报 `Unsupported key`；改用 Win32 `SendInput` VK_F9。截图未见明确沙漏形差异，**缓动视觉证据不足** |
| 6 空格播放 | **通过** | Space 可播放/停止；播放中 frame ~2–3 ms；`nvidia-smi` 采样 GPU util **6%** / mem util 2% / used 15246 MiB → [`retest-07-play.jpg`](./motion-gui/retest-07-play.jpg) |
| 7 拖动 + Ctrl+Z | **通过** | 查看器拖动图层（大目标）后 `Ctrl+Z` 可撤销 → [`retest-08-undo.jpg`](./motion-gui/retest-08-undo.jpg) |
| 8 `BAE_LANG=zh` | **通过** | 重开后菜单/面板/欢迎框为中文（文件/编辑/窗口/帮助、动效/审阅、欢迎使用 BAE 等）→ [`retest-09-zh.jpg`](./motion-gui/retest-09-zh.jpg) |

**总评（重测）：** 在 `gui` 锁 + `--screenshot-scale 1` + AE 快捷键路径下，fxc@79db75d 可完成启动→合成→纯色→位置属性/关键帧操作→播放→撤销→中文界面。矩形形状层仍依赖时间轴右键子菜单，自动化命中不稳。File 菜单无 New Composition（已知，反馈 12）。无 GPU 崩溃。

### 界面问题反馈（给 12）

1. **File 菜单没有「New Composition」**——动效快捷键需项目/合成/时间轴焦点；文档写「合成面板点新建合成」，与 File 菜单不一致。
2. **欢迎框**无独立关闭钮；需点 Motion/Review/Agent 角色按钮才关掉（Esc 无效）。
3. **时间轴右键 New 子菜单**在 1.5x + 坐标点击下难稳定点到「Rectangle」。
4. `orca computer` 单键不支持 `F9`（需系统 SendInput）。

### 重测截图

| 文件 | 内容 |
|---|---|
| `motion-gui/retest-01-launch.jpg` | fxc 启动 + Welcome |
| `motion-gui/retest-02-after-welcome.jpg` | 关掉 Welcome 后空合成 |
| `motion-gui/retest-03-comp1.jpg` | Comp 1 打开 |
| `motion-gui/retest-04-solid.jpg` | Solid 1 |
| `motion-gui/retest-05-position-p.jpg` | P 展开 Position |
| `motion-gui/retest-06-at-2s.jpg` | 约 2s + 关键帧 |
| `motion-gui/retest-07-play.jpg` | 播放相关 |
| `motion-gui/retest-08-undo.jpg` | 拖动/撤销后 |
| `motion-gui/retest-09-zh.jpg` | BAE_LANG=zh 中文界面 |

