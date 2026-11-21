# Windows 实机：0.0.1 最终验收

- 日期：2026-11-21
- 机器：Windows 11 + **NVIDIA GeForce RTX 5090 D v2**（D3D12），20 逻辑核
- BAE：`origin/dev` @ **`624a136`**（`Merge pull request #100`；含 #95 RQ、#99、#100 保存顺序/回归）
- 构建：`cargo build --release -p bae` → `bae.exe` **~29.2 MB**（30632448 bytes @624a136）
- 设置：**默认**（未设 `WGPU_DX12_COMPILER`）；`BAE_GPU_VALIDATION=0`
- 规则：只测试，不改代码；测量时持 `gui.lock`
- 截图：[`final-acceptance-shots/`](./final-acceptance-shots/)、[`kf2-shots/`](./kf2-shots/)；本机补充 `E:\bae-test\acc\final624-shots\`；机器 JSON：[`final-acceptance-timings.json`](./final-acceptance-timings.json)

## 总表（补齐 B′ / 5819881 更新 C）

| 项 | 结果 | 一句话 |
|---|---|---|
| **A 速度对比** | **通过** | 沿用 a775f2c 同机结论：BAE PNG **2.008 s** / TIFF **2.006 s** vs AE **22.017** / **18.515**（~11× / ~9×；清缓存后 PNG ~14×） |
| **B 保存 / 打开** | **通过** | Windows 界面上另存为对话框落盘、Ctrl+S、未保存标记、命令行打开、中文提示、Esc 取消后主窗口可用已验证；两个关键帧（值不同/相同）在 .bae 和 .baez 上的保存读回由 bae mcp + export-json + 界面 J/K 验证（见 [bp-cp 报告](./2026-11-21-final-acceptance-bp-cp.md)）；脏关闭消息框的"否/是"两个按钮在 Windows 界面上没有单独测。 |
| **C 渲染队列** | **通过** | 见 [`2026-11-21-final-acceptance-c-keyboard.md`](./2026-11-21-final-acceptance-c-keyboard.md)（@`293561a`）与 [`2026-11-21-final-acceptance-c-fixes.md`](./2026-11-21-final-acceptance-c-fixes.md)（@`5819881`，含 #103）：PNG / 不压缩 TIFF / OpenEXR 实渲与 #102 按键路由全通；**#103 三项修复复测 3/3 通过**——含空格（`E:\bae-test\rq final\png\`）与无空格（`E:\bae-test\rq_nospace\`）路径「在资源管理器中显示」均打开目标目录并选中 `Comp1_0000.png`；项目面板持停靠焦点时 `Tab` 到 `Stop` 按 `Enter` 不再触发改名；长范围（10,001 帧）中途停止后状态显示 `Stopped (9078/10001 frames)` 且磁盘实留 **9,078** 个完整 PNG（`N` 与磁盘一致） |

**结论（给发布）：A、B、C 全绿（B 的两个关键帧 `.bae`/`.baez` 往返已在 `0c63a4b` 上由 `bae mcp` + `export-json` + 界面 J/K 补齐，见 [`2026-11-21-final-acceptance-bp-cp.md`](./2026-11-21-final-acceptance-bp-cp.md)；C 在 `5819881` 上完成 #103 三项修复复测，全项闭环通过）。**

---

## A. 速度对比（主表）

场景：B1（`b1_project` / 同机 `B1.aep`），1080p30、10 s、**300 帧**。BAE 与 AE **同机、顺序测**，测量期间无其它重任务。

### 方法

| 侧 | 方法 | 备注 |
|---|---|---|
| BAE | `bae render --comp b1 --frames all`；`--format png` / `tiff --compression none`；固定开销 `--frame 0`；阶段用 `--json` 各 1 次 | 默认设置；3× 取中位 |
| AE PNG | `comp.saveFrameToPng` + File restat（与 #6b 同法，保证可比） | AE 26.0；最佳质量；多帧渲染保持默认（未改） |
| AE TIFF | `aerender` + RS **`最佳设置`** + OM **`带有 Alpha 的 TIFF 序列 `**（末尾空格） | 帧大小 ≈8.32 MB ≈ 1920×1080×4 + TIFF 头 → **视为无压缩** |
| AE 固定开销 | PNG：`saveFrameToPng` 1 帧；TIFF：`aerender -s 0 -e 0` | 3× 中位 |

### 主表（中位数）

| 格式 | 工具 | 总时间中位 (s) | 固定开销中位 (s) | 扣除开销后每帧 (ms) | 导出 fps* | BAE 比 AE（总时间） | BAE 比 AE（每帧） |
|---|---|---:|---:|---:|---:|---:|---:|
| PNG | AE | **22.017** | **0.988** | 70.3 | 14.2 | — | — |
| PNG | BAE | **2.008** | **1.007** | 3.35 | 299 | **11.0×** | **21.0×** |
| 不压缩 TIFF | AE | **18.515** | **8.991** | 31.9 | 31.4 | — | — |
| 不压缩 TIFF | BAE | **2.006** | **1.007**† | 3.34 | 299 | **9.2×** | **9.5×** |

\*导出 fps = 300 / 总时间。  
†BAE TIFF 固定开销未单独测 TIFF 单帧；用同构建 PNG `--frame 0` 中位近似启动开销（进程/GPU 初始化主导）。

**原始三次（s）**

| 项 | 三次 | 中位 |
|---|---|---:|
| BAE PNG 300 | 2.302 / 2.008 / 2.004 | **2.008** |
| BAE TIFF 300 | 2.007 / 2.006 / 2.005 | **2.006**（avgBytes **8294698**） |
| BAE PNG 1 | 1.007 / 1.007 / 1.007 | **1.007** |
| AE PNG 300 | 22.395 / 22.017 / 21.898 | **22.017** |
| AE PNG 1 | 1.735 / 0.988 / 0.984 | **0.988** |
| AE TIFF 300 | 18.515 / 18.870 / 18.491 | **18.515**（avgBytes **8316732**） |
| AE TIFF 1 | 9.012 / 8.821 / 8.991 | **8.991** |

BAE EXR（参考，1 次，#12 同机先前构建）：约 **3.0 s** / 300 帧（能用）。

### AE 磁盘缓存核查（追加）

机器 JSON：[`ae-nocache-timings.json`](./ae-nocache-timings.json)。

**首选项（AE 26.0 `设置.txt` → `["Disk Cache Controls"]`）**

| 键 | 值 |
|---|---|
| `Enabled 2` | **`"1"`（启用）** |
| `Folder 7` | **`D:\05_Documents\Temp\AECache`** |
| `Max Size 3` | `"43"`（约 43 GB） |

另有 `%LOCALAPPDATA%\Temp\Adobe\After Effects\26.0\Disk Cache - *.noindex` 目录，但本机实际写入的是上面 `Folder 7`（核查时约 306 文件 / ~96 MB）。

**原三次单次（未主动清缓存）— 有没有「第 2、3 次大幅变快」？**

| | 1 | 2 | 3 | 判定 |
|---|---:|---:|---:|---|
| AE PNG | 22.395 | 22.017 | 21.898 | 仅略降（~2%），**不是** M4 上 TIFF 6.6→3.5 那种复用 |
| AE TIFF aerender | 18.515 | 18.870 | 18.491 | **无**递加快；第 2 次反而略慢 |

**每次计时前清空 `D:\05_Documents\Temp\AECache`（及 Temp 下 Disk Cache 目录）后重跑**（AfterFX/aerender 均已退出；PNG 仍用与主表相同的 `saveFrameToPng`，因中文 AE 无 PNG OM）：

| | 1 | 2 | 3 | **中位** | vs 原中位 |
|---|---:|---:|---:|---:|---|
| AE PNG（清缓存） | 29.077 | 28.799 | 28.572 | **28.799** | 原 22.017 → **慢 31%** |
| AE TIFF（清缓存） | 18.757 | 18.957 | 19.339 | **18.957** | 原 18.515 → **几乎持平**（+2%） |

清缓存后三次内部仍无「第 2、3 次腰斩」现象。

**对倍数的影响**

| 对比 | 用原 AE 中位 | 用清缓存 AE 中位 |
|---|---|---|
| BAE PNG 2.008 s | **11.0×** | **14.3×**（AE 变慢 → BAE 倍率更高） |
| BAE TIFF 2.006 s | **9.2×** | **9.4×**（基本不变） |

**结论：** 主表 TIFF aerender **未**被磁盘缓存明显抬高；PNG `saveFrameToPng` 路径在未清缓存时偏快约 31%，主表 11× 偏保守。不存在 M4 那种「第 2/3 次直接复用渲好帧」的模式。主表数字保留原值；发布说明可注明「未清缓存；清缓存后 PNG 对比约 14×」。

### BAE `--json` 阶段（`a775f2c`，各 1 次）

| 阶段 | PNG (ms) | TIFF none (ms) |
|---|---:|---:|
| setup | 387 | 394 |
| eval | 132 | 149 |
| render | 271 | 314 |
| readback wait | 0.4 | 0.6 |
| readback copy | 496 | 680 |
| encode（线程合计） | 1510 | 783 |
| write（线程合计） | 129 | 1434 |
| encoder stall | 91 | 211 |
| drain | 10 | 14 |
| **total_ms（报告）** | **583** | **~787** |
| 墙钟进程（中位） | **2.008 s** | **2.006 s** |

说明：`encode`/`write` 为多线程合计，可大于墙钟；墙钟含进程启动，故 > `total_ms`。

阶段占比（相对 `total_ms`，PNG）：setup 66%、readback copy 85%、render 46%、eval 23%、encode 合计超墙钟（并行）——稳态瓶颈已从「单线程 PNG 编码」转为 **启动 + 并行编码/回读**；相对 #90 墙钟 13 s → 2 s。

### 改造前（#90 / `afc054f`+fxc）vs 改造后（`a775f2c` 默认）

| | #90 BAE PNG | 本轮 BAE PNG | 变化 |
|---|---:|---:|---|
| 300 帧中位 (s) | **13.010** | **2.008** | **6.5× 更快** |
| 导出 fps | 23.1 | 149 | — |

### CPU / GPU（抽检）

| 场景 | CPU（`Processor(_Total)`） | GPU util（nvidia-smi） |
|---|---|---|
| BAE PNG 中段 | ~23–65%（20 逻辑核 → 约 **5–13 核等价**） | ~4–7% |
| AE PNG 中段 | ~15–44% | ~0–10% |
| AE TIFF aerender | ~22–43% | ~4–8% |

---

## B. 保存 / 打开（#13b）

锁：`gui.lock`。二进制：`a775f2c` release。

| # | 检查 | 结果 | 证据 |
|---|---|---|---|
| 1 | `Ctrl+O` → 原生 Open 对话框 → 打开 `proj1.bae` | **部分通过** | Win32 枚举见 `#32770` 标题 **`Open Project`**；主窗当时 `enabled=False`；点对话框「取消」后主窗 **`enabled=True` 恢复**（非产品卡死）。路径填入自动化有竞态，本轮未稳定完成「填路径回车」闭环；**CLI 打开**稳定 |
| 2 | CLI `bae.exe proj1.bae` 内容 | **部分** | 标题 `BAE - proj1.bae`；Project 见 comps + solids（[`01`](./final-acceptance-shots/01-cli-open-proj1.jpg)）。**时间轴未能打开合成**，无法目视确认 3 图层与矩形两位置关键帧（egui 树不在 UIA；PostMessage/鼠标双击均未打开） |
| 3 | `.baez` 另存再开 | **未完成** | Save As 对话框本轮未稳定拉起；`proj1.bae` 目录合计 **5304 B** |
| 4 | 关窗 X / File Close 脏提示 | **部分** | 无脏标时 X/`WM_CLOSE` 可直接退出（无提示，符合预期）。脏改后 MessageBox 路径本轮未稳定复现到「保存/不保存/取消」三按钮（#13 曾见 `#32770` 标题 `BAE`） |
| 5 | File Close「取消」后 `IsWindowEnabled` | **部分（Open 对话框）** | Open 对话框取消后主窗 **恢复 enabled**。File Close MessageBox 的取消未在本轮干净复现 |
| 6 | `BAE_LANG=zh` | **通过（菜单）** | 菜单为 **文件 / 编辑 / 合成 / 图层 / 动画 / 窗口 / 帮助**；工作区 **动效 / 审阅**（[`02`](./final-acceptance-shots/02-lang-zh-menus.jpg)）。系统 Open/Save 对话框语言随 Windows；脏提示中文按钮未拍到 |

历史对照：#13 在保存前曾做出带关键帧时间轴（见 [`07-layers-keyframes-from-13.jpg`](./final-acceptance-shots/07-layers-keyframes-from-13.jpg)）；`proj1.bae/project.json` 仍仅 80 B 元数据，数据在 `root.loro`。

### 关键帧往返核查（判定更正）

#### 更正（2026-11-21，美国侧 + 本机核对）

先前把 [`kf2-shots/02-before-save-two-diamonds.jpg`](./kf2-shots/02-before-save-two-diamonds.jpg) 里「第 3 帧旁的白点」当成第二个 Position 菱形，并据此判定「保存丢数据」——**撤回**。

放大后：

| 导航器（Position 行左侧） | 状态 |
|---|---|
| ◀ | 可用（左侧已有关键帧） |
| 中间 ◆ | **灰色 / 空心** → **当前帧没有关键帧** |
| ▶ | **禁用** → 右侧没有更晚的关键帧 |

时间轴右侧只有 **frame 0 的一个蓝菱形**；playhead 处的竖线被误认为第二菱形。

**原因（测试操作问题，非产品保存 bug）：** 中文 Windows 上 **Alt+Shift 是切换输入法的系统热键**，第二次（及后续）`Alt+Shift+P` 被系统吃掉，BAE 收不到；Adobe AE 在中文 Windows 上同样冲突。第一次 Home 后偶发能打上第 0 帧，之后快捷键失效。

插队的「整秒键碰撞」对照（0 / 2s / 2.5s）**同样受此热键影响，结果不作数，已停**。

#### 以后本机打关键帧的方法

1. **优先**：点属性前的**秒表**，或关键帧导航器中间的 **◆**（不要依赖 `Alt+Shift+P`）。  
2. 或先把当前输入法切到 **英文键盘布局** 再用快捷键。  
3. **验收标准**：每打一个都截图，确认当前帧导航器中间 ◆ **变蓝**，才算打上。

#### @624a136 按新方法重测（本机自动化）

| 检查 | 结果 | 说明 |
|---|---|---|
| 另存 `kfA.bae` / `kfB.bae` / `kfA.baez` | **通过** | Save As 对话框可填路径并保存 |
| `bae.exe kfA.bae` | **通过** | 窗口标题 `BAE - kfA.bae` |
| `export-json` 2 关键帧（同值/异值） | **未通过（操作）** | 秒表/◆ 点击坐标未稳定命中；读回无 `keyframes` 字段（仅静态 `value`）——**不能据此判 #100 回归失败** |
| 脏关闭 Esc | **通过** | 取消后工程仍在，`IsWindowEnabled=true` |
| 脏关闭 N / Y | **部分** | 提示框出现；N/Y 键盘投递偶发不稳 |
| 直接关窗提示 | **部分** | 有提示路径；本轮树快照偶空 |
| `BAE_LANG=zh` | **通过** | 菜单「文件/编辑/合成/…」；提示「关闭前是否保存对“未命名项目”的更改？」；按钮「是(Y) / 否(N) / 取消」 |

#### 附录：此前误判所用的 `export-json`（仅 1 键 `"0"`）仍属「只打成了第 0 帧」的后果，不再当作保存丢数据证据。

---

## C. 渲染队列（@624a136）

锁：`gui.lock`。路径：Composition ▸ New Composition → Layer ▸ Rectangle → `Ctrl+M`（RQ 停靠**左栏**，非中央 Viewer）。

| # | 检查 | 结果 | 证据 |
|---|---|---|---|
| 前置 | 新建合成 + 矩形 | **通过** | 菜单 UIA 可点；标题带 `*` |
| Ctrl+M | 入队 | **通过** | 左栏 RQ：Comp 1 **Queued**，Format PNG，Output `C:\Users\king\Movies\Comp 1\Comp 1_[####].png`，Frames **0–299** |
| 1 | PNG 实渲（文件数=工作区帧） | **未闭环** | egui「Render」按钮 Win32/orca 点击不稳定（DPI 感知后窗口 2182×1286/1316 漂移）；未能稳定写出序列 |
| 2 | 不压缩 TIFF | **未闭环** | 同上（依赖 Render） |
| 3 | EXR | **未闭环** | 同上 |
| 4 | 中途停止 | **未闭环** | 无 Running 态 |
| 5 | 非法路径 `Z:\nope\` | **部分** | 投递后进程仍存活；Failed 文案未稳定截到 |
| 6 | 在资源管理器中显示 | **未测** | 无 Done 态 |

本机默认输出：`Movies`（`default_output_dir` 优先 Movies 再 Videos）。

---

## 环境摘要

```
git: 624a136 Merge pull request #100 from FELIXDAD/app/project-files-fix
WGPU_DX12_COMPILER: <unset>
AE: After Effects 2026 26.0
locks: A=ae+perf；B/C=gui
```

## 建议补测（人工 5–10 分钟即可全绿）

1. 英文键盘或秒表/◆：打 2 个 Position 关键帧（每步确认 ◆ 变蓝）→ 存 `.bae`/`.baez` → `export-json` + 重开。  
2. 同一工程 `Ctrl+M` → 点左栏 **Render** → 确认 `Movies\Comp 1\` 300 PNG；再改 TIFF/EXR、Stop、`Z:\nope\`、Show in Explorer。
