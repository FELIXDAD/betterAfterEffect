# CI 分档

负责：01 `core/foundation`。CI 分快、全两档。快检查每次推送都跑，几分钟出结果；全量只在合并前和 dev/main 上跑。合并脚本（`tools/merge-train`）按下面的检查名判断能不能合并。

## 检查名和触发条件

| 检查名（GitHub check run 的 `name`） | 工作流 | 什么时候跑 |
|---|---|---|
| `fast / macOS arm64`、`fast / Windows x64` | `.github/workflows/ci-fast.yml` | PR 的 opened、synchronize（推送、update-branch）、reopened。加标签不触发 |
| `full / macOS arm64`、`full / Windows x64` | `.github/workflows/ci.yml` | 推送到 dev 或 main；PR 带 `merge-ready` 标签时的 labeled（加的就是这个标签）、synchronize、opened、reopened |

- 快检查每次都会出结论：没有改到任何 crate 的 PR（纯文档等）也会跑这个 job，只是跳过编译，结论是 success，不会是 skipped。
- PR 没有 `merge-ready` 标签时，full job 被跳过（结论 skipped），名字可能是没展开的 `full / ${{ matrix.name }}`。
- 合并脚本对每个检查名取 PR 当前 head 上最新的一条 check run（id 最大），**只认 success**；skipped、cancelled、没有记录都算不通过。同一个 head 上可能先有一条 skipped，加标签后再有一条 success。
- 同一个 PR 推了新提交，旧的运行会被取消（cancelled）。没带标签而被跳过的 full 运行不会取消正在跑的 full。dev/main 上的 full 不取消，每次都跑完并保存构建缓存。
- 合并脚本 update-branch 以后，PR 带着标签，synchronize 会自动重跑快检查和全量。
- PR 有合并冲突时 GitHub 不触发 pull_request 运行，head 上就没有检查。

## 快检查（目标约 5 分钟）

两个平台都跑：
1. `changes` job（ubuntu）用 `tools/ci/affected.py` 算出要测哪些 crate，结果是三种之一：
   - `none`：没有改到任何 crate，比如只改了 `docs/` 里没人读的文件、根目录的 `.md`、`tools/ae` 这类脚本。快检查跳过编译。
   - `crates`：只测改到的 crate，以及（用 `cargo metadata` 算出的）所有直接或间接依赖它们的 crate，正常、dev、build 依赖都算。
   - `full`：改了根 `Cargo.toml`、`Cargo.lock`、`rust-toolchain.toml` 等根目录的配置，或者 `.github/`、`.cargo/`、`xtask/`、`vendor/`，或者 `crates/`、`apps/`、`services/` 下不属于现有成员的路径（新增、删除、改名 crate），就测整个工作区。
2. `fmt`（只在 macOS 上跑一次）、整个工作区的 `clippy --all-targets`、第 1 步算出的测试。Windows 上测试不开 D3D12 调试层（全量里另有一遍开调试层的）。

**代码读的文档不算纯文档。** crate 源码里的字符串字面量提到的仓库路径（`include_str!("../../../../docs/reference/ae/menus.json")`、`concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/ui/ae-commands.md")`）改了，就当作那个 crate 改了。例如改 `docs/agent-guide/mcp-instructions/` 会测 `bae-agent` 和它的下游，改 `tools/size-budget/budget.toml` 会测 `xtask`。只认字符串里带 `/` 的路径，单独的 `"tools"`、`"docs"` 这种词不算。

缓存：两档都用 `Swatinem/rust-cache`（固定版本），按平台共用一份缓存。只有 dev/main 上的全量会保存，PR 只恢复。

单独测一部分 crate 时，cargo 按这几个 crate 重新做特性合并，和整个工作区一起编译时的特性可能不同，一部分依赖会重新编译。这是用 `-p` 只测改到的 crate 的代价；快检查的耗时见引入分档的 PR。

## 全量

`ci.yml` 里原来的全部检查：fmt、clippy、工作区测试、Windows 上开 D3D12 调试层再跑一遍（WARP）、几个界面 crate 逐个单独测、只有一个 wgpu、只有一套字体栈、release 体积预算（`cargo xtask size`）、无窗口冒烟、打包、macOS 签名检查、上传安装包和体积报告。

## 改 CI 的规矩

改 `.github/` 要在 PR 里写明原因（AGENTS.md）。改检查名或触发条件时，同时改本文和 `tools/merge-train` 的配置。
