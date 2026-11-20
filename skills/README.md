# Agent Skills

这里放给 Agent（Claude、Codex 等）加载的 Skill。负责工作流：18 `agent/guide`。

| Skill | 内容 |
|---|---|
| [bae-animation](bae-animation/SKILL.md) | BAE 动画操作规范：流程、动画规则、协作礼仪、质量清单、配方 |

## 目录结构

```
skills/
  sync-references.py          从 docs/agent-guide/ 生成 references/
  bae-animation/
    SKILL.md                  精简规则 + 参考文件索引（手写）
    references/               docs/agent-guide/ 的副本（生成，不要手改）
```

`SKILL.md` 是手写的精简版；细节放在 `references/` 里，Agent 按需读取。`references/` 由脚本从 `docs/agent-guide/` 复制生成，指南是唯一的来源。

**评测集（`docs/agent-guide/evals/`）不会复制进 Skill**：被评测的 Agent 不能看到期望结果。

## 修改指南之后

```sh
python3 skills/sync-references.py          # 重新生成 references/
python3 skills/sync-references.py --check  # 只检查是否过期
```

只需要 Python 3 标准库。目前不在 CI 里检查（2026-11-20 决定先不加），修改指南的 PR 提交前请手动运行一次 `--check`。

## 安装

Skill 目录是自包含的，复制整个 `skills/bae-animation/` 即可：

| Agent | 位置 |
|---|---|
| Claude Code（个人） | `~/.claude/skills/bae-animation/` |
| Claude Code（项目内） | `<项目>/.claude/skills/bae-animation/` |
| Claude 应用 | 把 `bae-animation/` 打包成 zip，在设置的 Skills 里上传 |
| Codex | `~/.codex/skills/bae-animation/`（以 Codex 当前文档为准） |

安装后，连接 BAE 的 MCP 服务器（`bae mcp`，由 17 实现）并提出动效相关的任务，Agent 会根据 `SKILL.md` 的描述自动加载。

## 状态

第一版。工具名是预期名（待确认），17 `agent/api` 合并后统一核对。
