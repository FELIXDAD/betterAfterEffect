# Windows 实机：0.0.1 最终验收

- 日期：2026-11-21
- 机器：Windows 11 + **NVIDIA GeForce RTX 5090 D v2**（D3D12），20 逻辑核
- BAE：`origin/dev` @ **`a775f2c`**（`Merge pull request #95` render/export-ui；含 #94 导出管线）
- 构建：`cargo build --release -p bae` → `bae.exe` **29.20 MB**（30619136 bytes）
- 设置：**默认**（未设 `WGPU_DX12_COMPILER`）；`BAE_GPU_VALIDATION=0`
- 规则：只测试，不改代码；测量时持对应锁
- 截图：[`final-acceptance-shots/`](./final-acceptance-shots/)（8 张）；机器 JSON：[`final-acceptance-timings.json`](./final-acceptance-timings.json)

## 总表

| 项 | 结果 | 一句话 |
|---|---|---|
| **A 速度对比**（PNG / 不压缩 TIFF，AE vs BAE） | **通过** | BAE PNG **2.008 s** / TIFF **2.006 s** vs AE **22.017** / **18.515**（~11× / ~9×）。AE 磁盘缓存已启用但原三次无 M4 式递加快；清缓存后 TIFF 几乎不变，PNG 变慢→倍率约 **14×** |
| **B 保存 / 打开**（#13b） | **部分（关键帧保存疑似丢数据）** | `proj1` 来自 #13@6e4fb38；**a775f2c 复现：保存前时间轴 2 个 Position 菱形，存盘后 export-json 仅 1 个**（见「关键帧往返」节） |
| **C 渲染队列导出** | **部分** | `Composition ▸ Add to Render Queue` 在无活动合成时为 **disabled**（符合预期）；因自动化打不开 Project 面板里的合成，**未能跑完** PNG/TIFF/EXR/停止/坏路径/资源管理器全路径 |

**结论（给发布）：A 已过；B/C 主路径有证据但 GUI 往返与 RQ 实渲未闭环——是否进 0.0.1 由产品决定是否接受「部分」或要求补测。**

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

### 关键帧往返核查（优先追加）

#### 1) `proj1` 的 `export-json` 读的是什么

| | |
|---|---|
| 文件 | `E:\bae-test\acc\proj1.bae`（`project.json` + `sublayers\root.loro` mtime **2026-11-21 11:50:56**） |
| 写入时代 | **#13 / #96**，会话在 **`6e4fb38`（#93）** 上操作保存（早于 `a775f2c` 12:16 的 release 构建） |
| 读取二进制 | 后来用 **`a775f2c`** 的 `bae.exe` 跑 `export-json` |
| 统计 | 1 合成（Comp 1）、3 图层（Text / Shape Layer 1 / Solid 1）；Shape `transform:position` **仅 1 个关键帧**（`"0"` → `[960,540,0]`） |

因此 `proj1`「只有 1 个位置关键帧」**不能单独证明 a775f2c 保存丢数据**（也可能 #13 当时就没打成第 2 个）。需在 a775f2c 上从零复现。

#### 2) a775f2c 从零复现（有 UI 证据）

步骤（默认设置、`gui.lock`）：

1. Composition ▸ **New Composition...**（`Ctrl+N` 合成键在本机未可靠送达；菜单点击成功）→ Comp 2  
2. Layer ▸ New ▸ **Rectangle** → Shape Layer 1（红矩形）  
3. 选中图层，`P` 展开 Position；Home；**Alt+Shift+P** → 第 1 个关键帧  
4. 步进时间码（本轮实际到 **0:00:00:03**）；再 **Alt+Shift+P**（并拖动试图改值）  

**保存前截图**（[`kf2-shots/02-before-save-two-diamonds.jpg`](./kf2-shots/02-before-save-two-diamonds.jpg)）：时间轴 Position 行可见 **两个关键帧菱形**（约 0s 与第 3 帧）；秒表已点亮。对照 [`01-before-kf2-one-diamond.jpg`](./kf2-shots/01-before-kf2-one-diamond.jpg) 仅 1 个菱形。

#### 3–4) 存盘后 `export-json`（原样摘录）

- 另存为 `E:\bae-test\acc\kf2.bae`（目录合计 **3827 B**）  
- 另存为 `E:\bae-test\acc\kf2.baez`（**2998 B**）  
- `bae export-json`（`a775f2c`）两者 Shape Layer 1 的 `transform:position` **相同，均只有 1 个关键帧**：

```json
"transform:position": {
  "keyframes": {
    "0": {
      "interp_in": "linear",
      "interp_out": "linear",
      "value": { "type": "vec3", "value": [960.0, 540.0, 0.0] }
    }
  },
  "value": { "type": "vec3", "value": [960.0, 540.0, 0.0] }
}
```

（两关键帧在 UI 上数值同为 960,540——若产品对「同值关键帧」去重，也应在文档/行为上说明；当前表现为 **UI 2 个 → 磁盘 1 个**。）

#### 5) 判定

**判定为保存/序列化丢数据（产品 bug），不是「第 2 个关键帧没打成」。**

- 保存前截图明确 2 个 Position 菱形  
- 同一次会话 Save As 之后，`.bae` / `.baez` 的 `export-json` 都只剩 `"0"`  
- 复现：New Comp → Rectangle → Alt+Shift+P @0 → 移到非 0 帧 → Alt+Shift+P → Save As → `export-json` 看 `keyframes` 键数量  

---

## C. 渲染队列

锁：`gui.lock`。工程：`E:\bae-test\b1-bae\B1.bae`（62 items，含 `b1` 1080p）。

| # | 检查 | 结果 | 证据 |
|---|---|---|---|
| 前置 | 打开合成 | **阻塞** | Project 列出 `b1`，但自动化无法打开到 Timeline/Viewer（[`03`](./final-acceptance-shots/03-b1-loaded.jpg)） |
| Ctrl+M / 菜单 | Add to Render Queue | **部分** | 无活动合成时菜单项 **disabled**（[`04`](./final-acceptance-shots/04-add-to-rq-disabled.jpg)）——行为正确；因此无法加入队列 |
| 1–3 | PNG / TIFF / EXR 实渲 | **未测** | 依赖活动合成 |
| 4 | 中途停止 | **未测** | 同上 |
| 5 | 非法路径 `Z:\nope\` | **未测到失败 UI** | 未真正开渲；进程在误点后仍存活（[`08`](./final-acceptance-shots/08-still-alive-after-rq-attempts.jpg)） |
| 6 | 在资源管理器中显示 | **未测** | 无 Done 态 |

代码侧默认输出目录：用户 **Videos**（`default_output_dir`），格式 PNG/TIFF/EXR 可选——与任务要求的 `E:\bae-test\rq\...` 需在面板改「Output To」。

---

## 环境摘要

```
git: a775f2c Merge pull request #95 from FELIXDAD/render/export-ui
WGPU_DX12_COMPILER: <unset>
AE: After Effects 2026 26.0
locks: A=ae+perf；B/C=gui（已释放）
```

## 建议补测（若发布要求 B/C 全绿）

1. 人工双击 Project 中合成，确认 `proj1` 图层/关键帧；另存 `.baez`；脏关闭三按钮 + `IsWindowEnabled`。  
2. 打开 `b1` 后 `Ctrl+M`，改路径到 `E:\bae-test\rq\`，跑 PNG/TIFF/EXR、停止、坏路径、Show in Explorer。
