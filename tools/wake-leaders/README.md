# wake-leaders：领导会话撞上用量上限后自动叫醒

米兰那边的账号到期后，没人再盯美国额度、到点叫醒领导。这个工具跑在本机的 Orca 定时任务里：**领导会话因为用量上限停住，并且额度窗口已经恢复**，就给它发一条消息把它叫醒。

## 它怎么判断

每 30 分钟（每小时 :07 和 :37）Orca 先跑 `precheck.sh`，几百毫秒完成。退出码 0 才会启动 Codex 去执行叫醒，其余情况直接跳过，不花任何额度。

`precheck.sh` 对 `config.json` 里的每个被监视会话依次判断，全部满足才放行：

1. **停住了**：会话记录 `~/.claude/projects/-Users-a4-Documents-Orca-betterAfterEffect/<claude 会话>.jsonl` 里，最后一条对话记录（忽略后面的 system/attachment 等元数据）是 `isApiErrorMessage: true` 且 `error: "rate_limit"` 的合成消息，内容形如 `You've hit your 5-hour limit · resets 4:10pm (America/Los_Angeles)`。这是核实过的可靠信号：Orca 里这类会话停住后状态是 `done`，看不出区别。
2. **窗口已恢复**：按报错里写的重置时间（含时区、`Tue 1:40am`、`Nov 27, 1am` 这几种写法）算出绝对时间，已过 2 分钟以上。读不出时间就按报错后 5 小时 10 分钟算。**不依赖 `orca account list` 的 Claude 读数**，因为这个读数本身常被限流（`rate-limited`）；它只做一个否决：读数正常且仍是 100% 时不叫。
3. **Codex 还有额度**：`orca account list` 里 codex 周额度必须小于 100%，读不到也不放行。不会动用 Codex 的 extraUsage（付费额度）。
4. **没在自己恢复**：会话记录文件 90 秒内没有变化。
5. **没叫得太频繁**：同一个会话 29 分钟内最多叫一次（30 分钟减去调度抖动，保证失败后下一个周期能重试）。
6. **没被停掉**：同一个会话连续叫醒失败 2 次后进入 `halted`，不再自动叫，等人处理。会话自己恢复正常（最后一条不再是限流报错）时自动解除。

## 叫醒动作

automation 的 prompt（`automation-prompt.txt`）让 `codex / gpt-6.2-sol`（低 effort）只做一件事：运行 `wake.sh wake`。动作本身是确定性脚本，不靠模型发挥：

1. 重新判断一遍，对满足条件的会话执行 `orca orchestration send --to orca_session_id:<id>`，消息说明停住原因并让它继续。
2. 最多等 150 秒，确认会话记录里出现了比限流报错更新的对话，且最新的一条是正常的助手回复，才记为成功；又出现限流报错或超时，记为失败（计入第 6 条的失败次数）。
3. 发送失败（会话已关闭等，退出码 3）时，prompt 允许 Codex 退而用 `orca computer` 点 Orca 里该会话的「继续」按钮：只在界面上恰好有一个名为「继续」的按钮时才点，然后 `wake.sh verify <名字>` 确认。本机 Orca 有辅助功能权限，屏幕不锁才能用。

被监视的会话在 `config.json` 的 `watch` 里：默认 `ca3a6511`（Claude 会话 8e562540…）和 `09fe9cc0`（Claude 会话 2f5efdb3…）。要增删会话就改这里，Claude 会话 ID 和 Orca 会话 ID 都要填（前者对应 jsonl 文件名，后者是 `orca agent list` 里的 id）。

## 安装

先把本分支合进 dev（脚本要在 dev 工作树里），然后：

```bash
tools/wake-leaders/install.sh --dry-run   # 先看命令
tools/wake-leaders/install.sh             # 注册：cron "7,37 * * * *"，workspace 为 dev 工作树
orca automations list --json              # 记下 automation id
```

等价于 `orca automations create --name wake-leaders --trigger "7,37 * * * *" --precheck "bash <dev>/tools/wake-leaders/precheck.sh" --provider codex --model gpt-6.2-sol --effort low --workspace path:<dev> --prompt "$(cat automation-prompt.txt)"`。

> 注意：`orca agent installed` 在本机只列出 grok，注册后先 `orca automations run <id>` 试一次（会绕过 precheck，里面的 wake.sh 看到没人停住会什么都不做），确认 Codex 能在这台机器上被 Orca 启动。不行的话用下面的 launchd 方案，它不需要模型也不要 Codex 额度。

## 卸载 / 暂停 / 看日志

```bash
orca automations edit <id> --trigger none        # 暂停（要恢复再 edit 回 "7,37 * * * *"）
orca automations remove <id>                     # 卸载，同时删除运行历史
orca automations runs --id <id> --limit 20       # Orca 里的运行历史，被 precheck 挡掉的是 skipped_precheck
orca automations run <id>                        # 立刻跑一次（绕过 precheck）

tail -f ~/.local/state/bae-wake-leaders/wake.log # 本工具自己的日志（JSON 行，超过 1 MB 轮转为 wake.log.1）
tools/wake-leaders/wake.sh status                # 每个会话当前状态 + 失败计数
tools/wake-leaders/wake.sh reset [名字]          # 处理完问题后清除 halted 和失败计数
tools/wake-leaders/precheck.sh -v; echo $?       # 手动看一次判断结果，0 = 会叫醒
```

日志里 `check`（发现有会话停住时才写）、`sent`、`wake_ok` / `wake_failed` 几类事件。状态文件是 `~/.local/state/bae-wake-leaders/state.json`。`precheck.sh` 不碰任何会话，只写自己的状态目录和日志。

### 不用 Codex 的方案

`wake.sh wake` 本身不需要模型。若 Codex 不能用或额度紧，可以不注册 Orca automation，改用 `com.bae.wake-leaders.plist.example`（复制到 `~/Library/LaunchAgents/`，改 `REPO_PATH`，`launchctl load`）：每 30 分钟直接跑 `wake.sh wake`，判断和限频逻辑完全一样，只是没有 Codex 额度检查（也就不需要）。

## 演练

`test/drill.sh` 用伪造的会话记录和假的 `orca` 命令演练全流程，不碰真会话，几秒跑完，输出 `DRILL PASS`：

- 停住但窗口未恢复 / 刚恢复 1 分钟 / 恢复 5 分钟：只有最后一种放行；
- Codex 周额度 100%：不放行；已 halted：不放行；100 秒前刚叫过：不放行；配置缺失：不放行；
- `wake.sh wake` 叫醒成功（健康会话不会被打扰）；叫醒后又被限流（退出码 4）；会话已关闭发送失败（退出码 3）；失败 2 次后 halted；`wake.sh reset` 后恢复。

## 已知限制

- 判断依赖 Claude Code 当前的报错文案（`hit your … limit · resets …`）。文案变了，precheck 会判成"没停住"（不会误叫）；此时看 `wake.sh status` 或日志，改 `wake.py` 里的 `LIMIT_RE`。
- 消息送达靠 Orca 的会话地址。会话被关闭后叫不醒，需要人重新打开（或换成 `orca computer` 点按钮）。
- Codex 周额度到 100% 时不会叫醒（precheck 放行条件）。这种情况用 launchd 方案。
