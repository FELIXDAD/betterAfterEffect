# Windows RTX 5090 D 实机基线（dev）

- 日期：2026-11-21
- 提交：`ef75e1367ec74dbee076af7254684738a5d62103`（短哈希 `ef75e13`，`origin/dev`）
- 机器：Windows 11 专业版 10.0.26200；CPU Intel Core Ultra 7 265KF；内存 ~128 GB；GPU NVIDIA GeForce RTX 5090 D v2；驱动 / NVIDIA-SMI 616.92（`nvidia-smi` 报 KMD 616.92；`gpu-info` 报 driver `32.0.16.1692`）
- 工具链：新装 `rustup 1.29.3` + 按 `rust-toolchain.toml` 安装 `1.100.0`（rustfmt/clippy，目标含 `x86_64-pc-windows-msvc` / `aarch64-apple-darwin`）；已有 VS 2022 Build Tools 17.14.40（MSVC）；未另装 cargo-bloat
- 角色约束：只测试、不改业务代码；本报告与同目录截图/`b1.json` 为唯一新增产物

## 结论摘要

| 项 | 结果 |
|---|---|
| `gpu-info` | **通过**：RTX 5090 D v2 / **D3D12 discrete**，非 WARP |
| `cargo test --workspace --locked`（`WGPU_DX12_COMPILER=fxc`） | **644 passed / 0 failed / 2 ignored** |
| 默认 DX12（PATH 上 Auto→DXC） | 多个 GPU 用例 **0xc000001d**（见问题清单） |
| 重点 GPU crate 复测（fxc） | **184 passed / 0 failed**；AE 对照见下 |
| `cargo xtask size` | **26.62 MiB / 30 MiB PASS** |
| 无窗口冒烟 30 帧 | 默认与 fxc **均通过** |
| B1（fxc） | preview full **2584.6 fps**（对照 M4 852 fps） |
| 有窗口启动 | 默认与 fxc **均存活 10s+**；截图见下 |

## 1. 机器与环境

```
OS: Microsoft Windows 11 专业版 10.0.26200
CPU: Intel(R) Core(TM) Ultra 7 265KF
RAM: ~127.6 GiB
GPU: NVIDIA GeForce RTX 5090 D v2, 24455 MiB
nvidia-smi: 616.92
rustc 1.100.0 (0ef1bc386 2026-11-09) host x86_64-pc-windows-msvc
cargo 1.100.0
MSVC: Visual Studio Build Tools 2022 17.14.40
```

本机原先无 rustup；测试前安装了 rustup 与 1.100 工具链。未改仓库文件。

## 2. gpu-info（release）

命令：`cargo run --release -p bae -- gpu-info`

要点：

- 编译后端：D3D12；搜索 `Backends(DX12)`
- 选用适配器：`NVIDIA GeForce RTX 5090 D v2 [D3D12 / discrete]`（列表中亦有 Microsoft Basic Render Driver / software，**未选用**）
- `software rasterizer: no`
- 特性：timestamps / shader f16 / subgroups / float32 filterable & blendable / rgba16f&32f storage RW 等均为 yes

完整文本留在本机工作树旁路文件（未提交）：`docs/test-reports/win5090/gpu-info.raw.txt`。

## 3. workspace 测试

### 3.1 默认路径（不设 `WGPU_DX12_COMPILER`）

多次在 GPU 相关 harness 上以 **`STATUS_ILLEGAL_INSTRUCTION`（0xc000001d）** 整进程退出，**无 Rust panic / backtrace 正文**。已定位用例：

| 包 / harness | 用例 | 现象 |
|---|---|---|
| `bae-color --test gpu` | `snippets_match_cpu_reference` | 打印到 `lut1d 5000` 后硬退出；下一 case 应为 `lut1d 5000 inverse` |
| `bae-plugin --test declarative_gpu` | `box_blur_example_matches_cpu_and_negotiates_roi` | 几乎无输出即崩 |
| `bae-vector --test evaluated` | `evaluated_shape_layer_renders_on_the_gpu` | 前一用例通过后崩（真显卡 + Vello/DX12） |

同目录另两用例 `two_snippets_in_one_shader` / `premultiplied_entry_point` 单独跑通过。

### 3.2 `WGPU_DX12_COMPILER=fxc`

```
cargo test --workspace --locked
→ 644 passed; 0 failed; 2 ignored; EXIT=0（约 52s，增量编译后）
```

忽略项含慢测标记（如 sync 的 real SMB latency）。**未出现** `skipping Vello GPU test on software adapter` / WARP 跳过行（真显卡路径按预期跑 GPU 测试）。唯一命中 `skip` 字样的是与 GPU 无关的用例名 `options_select_artboards_and_skip_text_and_hidden`。

## 4. 重点 GPU crate 复测（fxc）

```
cargo test --locked -p bae-gpu -p bae-render -p bae-vector -p bae-color -p bae-plugin
→ 合计 184 passed; 0 failed; 0 ignored; EXIT=0
```

### AE 对照（`bae-render --test ae_golden`，真显卡，tolerance slack +0）

默认与 fxc 数值一致，例如：

| case | max | mean | tol |
|---|---:|---:|---:|
| blend_normal | 1 | 0.125 | 1 |
| blend_overlay | 1 | 0.188 | 1 |
| matte_alpha | 1 | 0.250 | 1 |
| geo_footage_down | 10 | 0.058 | 12 |
| … | … | … | … |

`matches_after_effects`：**ok**。Vello 相关 GPU 用例在 fxc 下通过（含 `evaluated_shape_layer_renders_on_the_gpu`）。

## 5. 体积

`cargo xtask size --json size-report.json`：

| | |
|---|---|
| binary | `target/release/bae.exe` |
| size | **26.62 MiB**（27,910,656 B） |
| budget | 30.00 MiB |
| result | **PASS**（余量约 +3.38 MiB） |

未装 `cargo-bloat`，无按 crate 分解。

## 6. 无窗口冒烟（双跑）

二进制：`target/release/bae.exe --headless --smoke-test 30`

| 模式 | 退出码 | 输出 |
|---|---|---|
| 默认（不设 DX12 compiler） | 0 | `smoke test passed: 30 frames 1280×720 … 19.5 ms … RTX 5090 D v2 (D3D12, discrete)` |
| `WGPU_DX12_COMPILER=fxc` | 0 | 同样通过，约 19.3 ms |

说明：冒烟路径未触发上述 DXC 崩溃用例；**普通用户跑简单 headless 冒烟在本机可过**，但复杂着色器/测试路径会踩 DXC。

## 7. B1 基准（`WGPU_DX12_COMPILER=fxc`）

`cargo xtask bench -p bae-render --bench b1`；JSON 见同目录 [`b1.json`](./b1.json)。

| 指标 | RTX 5090 D（本机） | Apple M4（docs/perf.md） |
|---|---:|---:|
| preview full | **2584.6 fps** | 852 fps |
| preview half | **3339.1 fps** | 1076 fps |
| latency p50 / p95 | **0.531 / 0.652 ms** | 1.44 / 1.67 ms |
| export fps | **303.7 fps** | 334 fps |
| eval mean | 0.120 ms | 0.098 ms |
| draws → draw calls / passes | 51 → 21 / 8 | 同结构 |

GPU 字段：`NVIDIA GeForce RTX 5090 D v2 (D3D12, discrete)`。

## 8. 有窗口启动（双跑）

`bae.exe`（无 `--headless`），等待 ≥10 s，`nvidia-smi` 空闲占用，再截图后结束进程。

| 模式 | 存活 | stderr | 空闲 GPU | 截图 |
|---|---|---|---|---|
| 默认 | 是 | 正常创建 DX12 device + `Bgra8Unorm, Fifo` | util ~2% | [`window-default.png`](./window-default.png) |
| `WGPU_DX12_COMPILER=fxc`（截图/观感以本遍为准） | 是 | 同上 | util ~0% | [`window-fxc.png`](./window-fxc.png) |

两遍均未在 10 s 空闲窗口期内崩溃。空闲时 GPU 利用率接近 0–2%（本机尚有其他进程占显存）。

## 9. 问题清单：DX12 专属崩溃（0xc000001d）

### 9.1 现象

- 退出码 **0xc000001d / STATUS_ILLEGAL_INSTRUCTION**；Rust 侧常无 panic 正文 / `cannot unwind` 字样（`RUST_BACKTRACE=full`、`RUST_LOG=wgpu_hal=warn,wgpu_core=warn` 亦然）。
- **Vulkan**（`--features bae-gpu/vulkan` + `BAE_GPU_BACKEND=vulkan`）：`snippets_match_cpu_reference`、`evaluated` **通过**。
- **`BAE_GPU_VALIDATION=0` 仍崩**（同 DX12 默认）→ **不能单独用「调试层 FFI panic」解释**。
- **`WGPU_DX12_COMPILER=fxc`：上述崩溃用例全部通过**；**`=dxc`：全部复现崩溃**。

### 9.2 DXC 库存（`where` + 版本）

`where dxcompiler.dll` / `where dxil.dll` **首选**（PATH 最先命中）：

| 文件 | 路径 | FileVersion | 来源判断 |
|---|---|---|---|
| dxcompiler.dll | `C:\Program Files\Obsidian\dxcompiler.dll` | **1.8.2505.0** | Obsidian 安装目录 |
| dxil.dll | `C:\Program Files\Obsidian\dxil.dll` | **101.7.2308.24** | 同上；版本编号体系旧，与同目录 dxcompiler 可能不匹配 |

另有（未优先）：

- Windows SDK 10.0.26100.0 x64：`...\Windows Kits\10\bin\10.0.26100.0\x64\dxcompiler.dll` / `dxil.dll` → **1.8.2502.11**
- Edge / Chrome / Copilot 等 CEF 附带多份 1.8–1.10；Adobe Illustrator 2026 附带更旧 1.7.x

背景（策划者转述、与观测一致）：wgpu 30 DX12 默认 compiler 为 Auto，PATH 有 `dxcompiler.dll` 则用之；旧/不匹配 DXC 处理新 naga HLSL 时可能内部 abort，表现为 0xc000001d 且无 Rust 输出。本机 PATH 优先 Obsidian DXC，与 **fxc 过 / dxc 崩** 完全吻合。

### 9.3 编译器矩阵（摘录）

| 用例 | 默认/DXC | FXC | Vulkan |
|---|---|---|---|
| `bae-color::snippets_match_cpu_reference` | 崩（lut1d 5000 后） | 过（含 lut3d） | 过 |
| `bae-plugin::box_blur_…` | 崩 | 过 | （未单测） |
| `bae-vector::evaluated_shape_layer_renders_on_the_gpu` | 崩 | 过 | 过 |

**含义：真显卡上的 Vello/复杂 WGSL→HLSL 路径在默认 DX12+DXC 下也会崩，不只是 WARP。**

### 9.4 建议（仅记录，未改代码）

- CI / 开发机：固定 `WGPU_DX12_COMPILER=fxc`，或从 PATH 移除第三方 `dxcompiler.dll`，或安装满足 wgpu 30 要求且 dxil 匹配的 DXC 并确保优先于 Obsidian。
- 勿把「Windows GPU 测试失败」默认当成 WARP/软件适配器问题。

## 10. 产物

- 本报告
- [`b1.json`](./b1.json)
- [`window-fxc.png`](./window-fxc.png)（约定：界面观感以 fxc 为准）
- [`window-default.png`](./window-default.png)（默认路径对照）

未提交：`test.log` 全文、旁路诊断 txt/json（体积大；关键段落已摘入上文）。
