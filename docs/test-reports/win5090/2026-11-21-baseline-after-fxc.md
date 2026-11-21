# Windows 实机：#80 后默认设置基线重跑（#8）

- 日期：2026-11-21
- 机器：Windows 11 + **NVIDIA GeForce RTX 5090 D v2**（驱动 32.0.16.1692 / nvidia-smi 616.92）
- BAE：`origin/dev` @ **`94ae6a5`** = `Merge pull request #80`（bae-gpu 显式 FXC）
- **`WGPU_DX12_COMPILER`：未设置**（验证 #80 默认即 FXC）
- `BAE_GPU_VALIDATION=0`
- 工具链：`rustc`/`cargo` 1.100.0；MSVC VS 18

## 结论

| 项 | 结果 |
|---|---|
| `cargo test --workspace --locked`（默认） | **通过**（各 crate `test result: ok`；日志中 **0 次** `0xc000001d`） |
| GPU 重点：`bae-color` / `bae-plugin` / `bae-vector` / `bae-effects` / `bae-gpu` / `bae-render` | **全部通过**，无 `0xc000001d` |
| 无窗口冒烟：`bae gpu-info` + `bae render --frame 0` | **通过**（D3D12 5090 D；PNG 写出） |
| 开窗口启动 | **通过**（窗口标题 BAE，表面 `Bgra8Unorm`，未见崩溃） |
| `WGPU_DX12_COMPILER=dxc` → `bae-color` `snippets_match_cpu_reference` | **仍崩**：`exit code: 0xc000001d, STATUS_ILLEGAL_INSTRUCTION`（覆盖生效） |

原先在默认/隐式 DXC 下会崩的 bae-color / bae-plugin / bae-vector / bae-effects **在未设环境变量时已全部绿**。

## 细节

### Workspace

约 108 s；日志：`E:\bae-test\baseline2\workspace-test.log`（不进仓）。全文无 `0xc000001d`。

### GPU 重点（默认）

| crate | exit | 备注 |
|---|---:|---|
| bae-color | 0 | 含 GPU 用例 |
| bae-plugin | 0 | |
| bae-vector | 0 | |
| bae-effects | 0 | |
| bae-gpu | 0 | |
| bae-render | 0 | |

### 冒烟 / 开窗

- `bae gpu-info`：Adapters 含 `NVIDIA GeForce RTX 5090 D v2 [D3D12 / discrete]`；shader model Sm5。
- `bae render … --frame 0`：exit 0，~1.0 s，写出 `smoke.png`。
- GUI：`INFO bae_gpu::context: GPU device created … backend=Dx12`；`window surface: Bgra8Unorm, Fifo`。

### DXC 覆盖（预期失败）

```text
WGPU_DX12_COMPILER=dxc cargo test -p bae-color --locked snippets_match_cpu_reference -- --nocapture
…
error: test failed …
(exit code: 0xc000001d, STATUS_ILLEGAL_INSTRUCTION)
```

用例跑到 LUT 段后非法指令退出——与 #80 前「DXC 在 5090 D 上崩」一致，说明环境变量仍能强制 DXC。

## 备注

- 开窗日志曾提示上一会话 crash report（验收杀进程/Alt+F4 残留），本轮默认启动本身稳定。
- 本报告只测不改代码。
