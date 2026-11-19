# 给开发 Agent 的规则

开始工作前必须读：

1. `docs/VISION.md`：要做什么、为什么
2. `docs/ARCHITECTURE.md`：技术选型和全体共同遵守的约定
3. `docs/WORKSTREAMS.md`：工作流划分、依赖、领取和合并流程
4. 你自己的任务书：`docs/workstreams/NN-*.md`（看当前分支名对应哪一个）

## 基本规则

- 只修改任务书里"负责目录"中的文件。确实需要改别人的目录（包括根 `Cargo.toml`、`apps/bae`）时，改动尽量小，并在 PR 里说明原因。
- 所有对工程数据的修改都通过 `bae-model` 的 Command。不要在界面或其他 crate 里直接改数据。
- 命令只能定义在不依赖界面和 GPU 的 crate 里。界面 crate 只调用命令。
- `bae-model`、`bae-anim`、`bae-sync` 和各领域命令 crate 不能依赖 GPU、窗口或平台 API。
- 新增依赖前先看有没有已经在用的同类库；PR 里写明 release 体积增加了多少（`cargo xtask size` 的结果）。不允许引入 Chromium/WebView、Qt、静态链接的 FFmpeg、OpenColorIO、OpenUSD 这类大型运行时。
- 代码、标识符、代码注释用英文。面向用户的文档用中文；界面文字走 i18n，中英文都要有。
- 不复制 Adobe 的图标、图片、示例工程、SDK 代码或头文件。可以参考公开文档、脚本 API 文档和界面布局。
- 不提交大于 1MB 的二进制文件。测试素材尽量用脚本生成。

## 分支与提交

- 你的分支见任务书。分支从 `dev` 拉出，PR 也回到 `dev`。
- 提交信息格式：`<领域>: <做了什么>`，例如 `render: add track matte pass`。
- 经常 `git fetch origin && git rebase origin/dev`。
- 不要 force push 到 `dev` 或 `main`。

## 构建与测试

- 需要 Rust 工具链（用 rustup 安装，版本见 `rust-toolchain.toml`）。
- Cargo 工作区由 01 `core/foundation` 建立。在它合并之前，其他分支可以先创建自己的 crate；工作区会通过 `crates/*` 通配自动包含进来。
- 提交前运行：`cargo fmt`、`cargo clippy --workspace -- -D warnings`、`cargo test -p <你的 crate>`。
- 涉及渲染效果的改动，PR 里附上改动前后的截图或基准数据。

## 同步进度

- 在 Orca 工作区卡片上更新状态和备注：
  `orca worktree set --worktree active --workspace-status in-progress --comment "当前进度" --json`
- 在任务书末尾的"进度"一节记录已完成、受阻和对其他工作流的请求。
