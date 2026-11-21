# 最终验收 B′ / C′（dev `0c63a4b`，win5090）

> 接 Milan：B′ 走 `bae mcp`；C′ 走 Tab→Render。默认设置。驱动脚本在仓库外 `E:\bae-test\acc\`，本目录 `final-acc/` 放摘要与截图副本。

## 环境

| 项 | 值 |
|---|---|
| tip | `0c63a4b`（≥ 624a136） |
| 二进制 | `dev/target/release/bae.exe` |
| GPU | RTX 5090 D v2 · D3D12 |
| 分辨率 | 2160×1200 @ 1.50x |

## B′ — MCP 关键帧（通过）

### 步骤

1. `bae.exe mcp --create --permission any-layer` → `mcpA.bae` / `mcpB.bae`  
   `initialize` → `tools/list` → `command_batch`：`comp_create` + `layer_create`（shape/rect）+ 两次 `key_set`（`transform:position`，`"0f"` / `"60f"`）
2. `bae.exe export-json` 读回
3. GUI：`bae.exe mcpA.bae` → Open in Timeline → 选 Rect → `P` → `Home` / `K` / `J` 截图时间码
4. `.baez`：`bae mcp --create mcpA.baez` 同样流程并 `export-json`

### 结果

| 文件 | 关键帧 | 值 | 结论 |
|---|---|---|---|
| `mcpA.bae` | 2（0 / 1411200000 flicks = 60f@30） | `[400,540,0]` → `[1400,540,0]` | 通过 |
| `mcpB.bae` | 2 | `[960,540,0]` → `[960,540,0]` | 通过 |
| `mcpA.baez` | 2 | 同 mcpA | 通过（mcp 可新建 .baez） |

GUI J/K（`mcp-shots/111-Home.png` / `112-K.png` / `113-J.png`）：

- Home：`0:00:00:00`，Position `400.0, 540.0`，◆ 在 0s 与 2s
- K：`0:00:02:00`（frame 60），Position `1400.0, 540.0`
- J：回到 `0:00:00:00` / `400.0, 540.0`

驱动：`E:\bae-test\acc\run_mcp_bp.py`、`run_mcp_jk_v2` 相关、`run_mcp_open_exact` 等。摘要：`final-acc/mcp-bp-summary.json`。

## C′ — 渲染队列 Tab→Render（部分）

### 已确认

| 检查 | 结果 |
|---|---|
| Ctrl+M 入队 | 通过（Comp 1 Queued，Format PNG，Frames 0–299） |
| Output To 已填 | `C:\Users\king\Movies\Comp 1\Comp 1_…` |
| Render 非灰 | 通过（`add_enabled` 可点外观） |
| Tab 焦点 | 通过：Render 出现白框焦点环（`c-tab3/01-rq-left.png`、`c-fin/tab-on-render-left.png`） |
| Enter / Space | **不触发**渲染（egui Button 键盘激活未通） |
| 鼠标点 Render | **不稳定**：焦点环中心约物理 `(41,244)`；多数自动化点击后仍 Queued；**一次**会话中点中后 `bae-render-queue` 启动并写出 **300** 张 PNG（日志 `bae-20261121-073040-*.log` @ 07:34:41Z），后被后续清理删掉，未稳定复现 |
| TIFF / EXR / Stop / Reveal / 坏路径 | 未闭环（依赖稳定触发 Render） |

### 界面问题反馈（给产品）

1. **Tab 能聚焦 Render，但 Enter/Space 不激活**（egui `Button` 键盘路径）。
2. **无障碍树**，自动化只能靠坐标；Render 在左栏约 `(40,244)`（2160×1200@1.5x），与更早记录的 `(20,178)` 不一致（布局/缩进变化）。
3. 建议：为 Render 提供菜单/快捷键，或 MCP/`bae` CLI `render-queue run`。

驱动：`E:\bae-test\acc\run_c_tab_*.py`、`run_c_finish.py`、`run_c_orca_render.py`。截图：`final-acc/c-tab3-shots/`、`c-fin-shots/`。

## 收尾总表（更新）

| 项 | 结果 | 一句话 |
|---|---|---|
| **A 速度** | **通过** | 沿用：BAE PNG ~2.0s / TIFF ~2.0s vs AE ~22s / ~18.5s |
| **B′ 关键帧（MCP）** | **通过** | mcpA/mcpB/baez 建键+export-json+GUI J/K 时间码与值均对齐 |
| **C′ 渲染队列** | **部分** | 入队/路径/非灰/Tab 焦点已证；Enter/Space 无效；鼠标点 Render 仅偶发成功（曾 300 PNG），格式/停止/Reveal 未闭环 |

## 复现材料

- 仓库外：`E:\bae-test\acc\`（工程、脚本、日志、完整截图）
- 报告目录：`docs/test-reports/win5090/final-acc/`（入库精选：`mcp-bp-summary.json`、`mcpA.json`、`mcpB.json`、`mcpA-baez.json`，以及 J/K 关键帧跳转截图 [`mcp-shots/111-Home.png`](./final-acc/mcp-shots/111-Home.png)、[`mcp-shots/112-K.png`](./final-acc/mcp-shots/112-K.png)）
