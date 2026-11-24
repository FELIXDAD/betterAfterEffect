# 合并脚本与进度看板（merge-train）

两个跑在协调人这台 Mac 上的 launchd 任务，不依赖任何 AI 会话：

- **合并脚本** `com.bae.merge-train`：常驻，每分钟看一次。领导审完 PR 后打 `merge-ready` 标签，脚本按打标签的先后一个一个合进 `dev`。合什么由领导决定，脚本只负责执行。
- **进度看板** `com.bae.progress-board`：每 10 分钟改写置顶 issue「BAE 进度看板」的正文。

只用系统自带的 `/usr/bin/python3`（3.9）和 bash，另外需要已经登录的 `gh`、`orca`，以及 `cargo`（只在重新生成 Cargo.lock 时用）。

## 文件

| 文件 | 作用 |
|---|---|
| `merge_train.py` | 主程序：`run`（合并）、`board`（看板）、`setup`（建标签）、`pause`、`resume`、`status` |
| `config.toml` | 仓库、必需检查名、标签名、路径、超时 |
| `leaders.toml` | 领导地址和“哪个 PR 归哪位领导”的规则 |
| `launchd/*.plist` | launchd 模板，`install.sh` 填好路径后装到 `~/Library/LaunchAgents` |
| `install.sh` / `uninstall.sh` | 安装 / 卸载 |
| `test_merge_train.py` | 离线测试（GitHub、orca 用 mock，git 用临时仓库） |

## 合并规则

队列：所有打了 `merge-ready`、还开着的 PR，按最后一次打这个标签的时间排序（GitHub timeline 的 labeled 事件）。脚本一次只处理排在最前面的那个（队首），它合并或出队以后才轮到下一个；唯一的例外见下面的“插队”。

对队首的 PR：

1. Draft、目标分支不是 `dev`：出队并通知。
2. GitHub 显示 `CONFLICTING`：在独立的工作树 `~/Library/Caches/bae-merge-train/work` 里把 `dev` 合进 PR head。
   - 只有 `Cargo.lock` 冲突：取 `dev` 的 `Cargo.lock`，运行 `cargo metadata` 让 cargo 重新生成（不手改），提交推送到 PR 分支，然后等 CI。
   - 还有别的文件冲突：出队，通知负责 agent 和领导。
3. 必需检查（`config.toml` 的 `[checks]`，快检查加全量）：按名字取当前 head 上最新的一条 check run，只有 `success` 算通过。
   - 有失败（`failure`、`timed_out` 等），或者别的检查失败：出队，在 PR 上评论失败的检查和链接，通知负责 agent 和领导。
   - `cancelled` 不算失败：它通常是 concurrency 取消了旧的运行，或者被 GitHub 那边取消，不是测试没过。脚本先等 `cancel_grace_minutes`（2 分钟），看同一个 head 上会不会有新的运行接上；没有的话就重跑那个 job（`POST /actions/jobs/<id>/rerun`；同一次运行的其他 job 还在跑时 GitHub 不让重跑，下一轮再试）。同一个检查在同一个 head 上被取消 `max_cancellations`（2）次，才出队并通知。head 变了，计数清零。
     - GitHub 因为没有空闲的 runner 取消的 job（注解里写着 `The job was not acquired by Runner …`；macOS arm64 runner 紧张时，排队 15 分钟左右就会这样）不计入次数，一直重跑，直到 `ci_timeout_minutes` 超时。
   - 缺失、还在跑、`skipped`（打标签前全量被跳过）：等。`skipped` 的 job 重跑也还是 `skipped`（重跑沿用原来的事件，那时还没有标签），所以它由下面“主动触发 CI”那条处理。超过 `ci_timeout_minutes`（150 分钟）还没结果，按失败处理。
   - 只有缺失或 `skipped`、没有任何检查在排队或在跑，持续 `missing_nudge_minutes`（10 分钟）：说明 CI 不会自己来了，比如 head 是 CI 改名之前推的。这时脚本主动触发一次：PR 落后 dev 就 `gh pr update-branch`；已经包含最新 dev 就往 PR 分支推一个空提交（`merge-train: retrigger CI …`）。同一个 head 只触发一次。
4. PR 和 `dev` 在“PR head 与 dev 的 merge-base”之后的改动有关联时，运行 `gh pr update-branch`，等新 head 的全量 CI。有关联指下面任一种：
   - **改了同样的文件**（PR 一侧不算 `Cargo.lock`；dev 一侧不算 `Cargo.lock` 和 `docs/`）。两边都没改文件时重叠为空，不会误判。
   - **改到的 crate 有依赖关系**：同一个 crate，或者一个是另一个的上游 / 下游（间接依赖也算）。比如 #303 改了 `bae-render` 的 `LayerDependency`，#299 在 `bae-effects` 里直接构造这个结构体。两边没有共同的文件，各自都绿，但合在一起 dev 就编译不过了。
     - 文件属于哪个 crate、crate 之间的依赖，复用 CI 快检查的 `tools/ci/affected.py`：依赖用 dev 检出上的 `cargo metadata --no-deps` 算，每个 dev 提交算一次（约 0.2 秒）。
     - `.github/`、`Cargo.lock`、不属于任何 crate 的文档和 `tools/` 不算；但被 crate 源码用字符串引用的文件（比如 `include_str!` 读的 `docs/ui/…`）算作那个 crate 的改动。
     - 根目录的 `Cargo.toml`、工具链文件、`vendor/`、`xtask/` 算作影响所有 crate。
     - 依赖关系算不出来时（比如找不到 cargo），当作有关联，宁可多 update-branch 一次。update-branch 以后 dev 一侧没有新改动，就不会再判，不会循环。
5. `mergeable` 是 `UNKNOWN`（GitHub 刚合完别的 PR 时常见）：等一会儿再查，超过 20 分钟才算失败。
6. 都满足：`gh pr merge --match-head-commit <head>` 直接合并。代码 PR 用 merge commit；参考数据 PR（标签 `reference-data` 或分支前缀 `ref/`）用 squash。合完给对应领导发一行：PR 号、标题、dev 新提交号。

兜底：每一轮都看 `dev` 最新提交的全量检查。如果是红的，就暂停整个队列，通知 `leaders.toml` 里 `pause_notify` 列出的领导。同一个红提交只暂停一次。

dev 上的全量 job 被取消时（dev 的运行不会被 concurrency 取消，通常是没分到 runner），dev 就没有任何 CI 结果，红了也发现不了。所以脚本会重跑那个 job，再被取消就再重跑，直到有真正的结果（同样先等 2 分钟；同一次运行里还有 job 在跑时下一轮再试）。

出队就是脚本去掉 `merge-ready` 标签。修好以后，由领导重新打标签。

**插队**：队首已经等了 `skip_ahead_after_minutes`（30 分钟）还没合（在等 CI、等 mergeable 或等 update-branch 之后的 CI），脚本就按打标签的顺序往后看，把第一个此刻已经可以合并（满足上面第 6 条：检查全绿、无重叠、MERGEABLE）、**并且和队首没有关联**（既没有改同样的文件，也没有有依赖关系的 crate，规则同第 4 条）的 PR 先合掉。和队首有关联的 PR 不插队，照样排在队首后面：它先合进去的话，队首又得 update-branch、重跑一遍全量，可能一直合不进去（#271 就出现过）。队首保持原位。对后面的 PR，脚本只做“合并”这一个动作，不会去 update-branch、修冲突或让它出队，这些都等它排到队首再做。

## 通知谁

- **负责 agent**：在 `orca worktree ps --json` 里按 PR 分支找对应的工作树，本机和 `config.toml` 里 `orca_environments` 列出的配对机器（米兰Mac）都找；取最近活动的那个 agent。远端的 agent 发消息时自动加 `--on <机器名>`。
- **领导**：按 `leaders.toml` 的 `rules` 从上往下匹配，第一条匹配的生效，都不匹配就用 `default`。可以按机器（`host`）、分支前缀、工作树名前缀、标签来匹配。
- 脚本发消息用 `orca orchestration send --from merge-train`。不在 agent 终端里运行时必须带 `--from`，否则 orca 报 `no_active_sender_terminal`；`merge-train` 只是发件人标签。

## 安装（由协调人做）

```bash
tools/merge-train/install.sh --dry-run    # 先干跑
tools/merge-train/install.sh              # 看过干跑日志以后，改成真跑
launchctl kickstart -k gui/$(id -u)/com.bae.merge-train      # 当场跑一轮（看板换成 com.bae.progress-board）
```

`install.sh` 做这些事：

1. 建立运行用的独立完整 clone：`~/Library/Application Support/bae-merge-train/repo`。它分离 HEAD，跟踪 `origin/dev`；冲突修复用的工作树也从这个 clone 建，放在 `~/Library/Caches/bae-merge-train/work`，用独立的 `CARGO_TARGET_DIR`。
   - **为什么不放在 `~/Documents`**：macOS 的隐私保护不允许 launchd 后台任务读“文稿”文件夹，会报 `Operation not permitted`，任务以退出码 2 退出。主仓库的工作树也不行，因为工作树的 `.git` 对象库还在 `~/Documents` 下。所以用放在 `~/Library` 下的独立 clone，不需要申请“完全磁盘访问权限”。
   - 如果 `runner_dir` 是旧版本留下的工作树，`install.sh` 会停下来，提示先 `git worktree remove`。
2. 运行 `merge_train.py setup`，建好四个标签：`merge-ready`、`merge-train-paused`、`blocked`、`reference-data`。
3. 把两个 plist 装进 `~/Library/LaunchAgents`，用 `launchctl bootstrap` 启动。开机登录后自动运行；合并脚本退出或崩溃后 30 秒内由 launchd 拉起。

合并脚本每一轮都会 `git fetch`。运行用的 clone 切到 `runner_ref`（默认 `origin/dev`）以后，如果 `merge_train.py`、`config.toml` 或 `leaders.toml` 变了，脚本会自己退出，由 launchd 用新版本重启。所以改配置只要走 PR 合进 dev。`runner_ref` 上还没有这个脚本时，clone 停在当前版本不动。`--ref <ref>` 可以让 clone 先检出别的 ref（比如还没合并的分支），用来试新版本。clone 只会向前跟：当前提交还没进 `runner_ref` 时就停在原地，等这个分支合进 dev 后再自动跟到 `origin/dev`。

`--no-board` 只装合并脚本，不装看板。

干跑（`--dry-run`）时脚本照常读取所有数据、做判断，但不合并、不 update-branch、不推送、不评论、不改标签、不发消息，只在日志里写 `DRY-RUN would: …`。同一个 PR 的判断不变就不重复写。看板在干跑时照常更新，队列状态显示“干跑”。

## 卸载

```bash
tools/merge-train/uninstall.sh
```

这会停掉并删除两个 launchd 任务。运行用的 clone、状态目录和日志会留下，要删就手动删（clone 在状态目录里面）：

```bash
rm -rf ~/Library/Caches/bae-merge-train "$HOME/Library/Application Support/bae-merge-train"
```

## 暂停和恢复

满足下面任一条件，队列就暂停：

- 暂停文件 `~/Library/Application Support/bae-merge-train/PAUSED` 存在（文件内容就是暂停原因）；
- 看板 issue 上有 `merge-train-paused` 标签。不在这台 Mac 上的人（比如米兰）可以在 GitHub 上打这个标签来暂停，去掉标签就恢复。

```bash
R="$HOME/Library/Application Support/bae-merge-train/repo/tools/merge-train"
python3 "$R/merge_train.py" pause --reason "等 #123 先合"   # 暂停：写暂停文件，给看板 issue 打标签
python3 "$R/merge_train.py" resume                         # 恢复：删暂停文件，去掉标签
python3 "$R/merge_train.py" status                         # 看队列和每个 PR 当前的判断
```

dev CI 变红时，脚本会自己暂停。修好 dev 以后运行 `resume`。`resume` 会记下当前 dev 提交，这个提交的红灯不会再触发暂停，下一个红提交才会。

想彻底停掉进程，用 `launchctl bootout gui/$(id -u)/com.bae.merge-train`。重新启动就再跑一次 `install.sh`。

## 日志和状态

- 日志：`~/Library/Logs/bae-merge-train.log`。两个任务都写这里，用 `[train]`、`[board]` 区分。查看：`tail -f ~/Library/Logs/bae-merge-train.log`。
- 状态：`~/Library/Application Support/bae-merge-train/state.json`，记录队列、每个 PR 的当前判断、最近合并和看板 issue 号。删掉它没有关系，脚本会重建。
- launchd 状态：`launchctl print gui/$(id -u)/com.bae.merge-train`。

## 进度看板

- 标题「BAE 进度看板」。第一次运行时脚本自动创建这个 issue 并置顶（`gh issue pin`），之后每 10 分钟改写正文。
- 正文内容：
  - dev 的 CI 状态；
  - 合并队列（顺序、每个 PR 当前的判断）和队列状态（运行中 / 暂停及原因 / 干跑 / 脚本没在跑）；
  - 每个工作树一行：最新的 agent 会话和状态、分支、开着的 PR（Draft / Ready）和检查结果、最近一次提交时间、卡住原因、Orca 卡片备注。在干活或开着 PR 的排在前面，空闲的折叠起来；
  - 没有对应工作树的 PR。
- **卡住**：给 PR 打 `blocked` 标签，再发一条以 `blocked: 原因` 或 `卡住：原因` 开头的评论，看板会显示这个原因。
- **交接快照**：正文里 `<!-- handoff:start -->` 和 `<!-- handoff:end -->` 之间的内容，脚本改写时原样保留。领导每一轮直接在 GitHub 上编辑这一段，写谁在做什么、有哪些待决定的事，任何会话几分钟就能接手。其他部分会被覆盖，不要改。
- 看板只用 GitHub 和 orca 的数据，不往仓库提交任何文件。

## 检查名

必需检查写在 `config.toml` 的 `[checks]`，以 `.github/workflows/ci.yml` 为准，和 CI 分档（分支 `build/ci-tiers`）约定一致：

- `fast = ["fast / macOS arm64", "fast / Windows x64"]`：每次推送 PR 都跑。
- `full = ["full / macOS arm64", "full / Windows x64"]`：PR 带 `merge-ready` 时在 labeled / synchronize 事件上跑，所以 update-branch 之后会自动重跑；dev / main 推送时也跑。

CI 改了 job 名字时，只需要改 `config.toml`。

## 测试

```bash
python3 tools/merge-train/test_merge_train.py
python3 tools/merge-train/merge_train.py run --once --dry-run            # 用真实 GitHub 数据干跑一轮
python3 tools/merge-train/merge_train.py run --once --dry-run --simulate 185 --ignore-draft
                                                                         # 把 #185 当作已打标签、非 Draft
python3 tools/merge-train/merge_train.py board --dry-run                 # 把看板正文打印到终端
```
