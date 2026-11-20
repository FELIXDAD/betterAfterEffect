# MCP instructions 精简版

MCP 服务器在初始化时可以给客户端一段 `instructions` 文本，大多数 Agent 会把它放进上下文。这里是给 17 `agent/api` 内置用的精简版操作规范：没有安装 Skill、没读过完整指南的 Agent，也能靠这一段做到不越权、不乱做。

| 文件 | 内容 | 长度 |
|---|---|---|
| [en.md](en.md) | 英文版（默认） | 约 2,900 字符，约 700 tokens |
| [zh.md](zh.md) | 中文版 | 约 1,300 字，约 1,000 tokens |

两个文件是纯文本（只用了最简单的 Markdown），没有标题以外的格式，可以直接作为 `instructions` 字符串发送。

**现状：** 17 已经用 `include_str!` 内置这两个文件（`crates/bae-agent/src/server.rs`），默认英文，`bae mcp --instructions zh` 用中文；服务器在后面附一段"本服务器说明"（工具名把 `.` 换成 `_`、用 `command_batch` 代替事务、哪些工具还没有）。

## 给 17 的集成建议

- 用 `include_str!` 直接引用这两个文件，不要复制一份到 `crates/bae-agent` 里，避免两边不同步。例如：
  `include_str!("../../../docs/agent-guide/mcp-instructions/en.md")`（相对路径以 17 的实际文件位置为准）。
- **默认发送英文版**（2026-11-20 用户确认）：各家 Agent 对英文指令的遵循最稳定，token 也更少；工具名、属性名本来就是英文。提供一个配置项（例如 `bae mcp --instructions zh`）切换到中文版。
- 加一个测试：两个文件都不超过 **3,000 字符**，并且文中出现的每个工具名都存在于实际导出的工具列表里。这样指南改了工具名或 17 改了命令 ID，测试都会提醒。
- 同时把完整指南作为 MCP 资源暴露（见 [tools.md](../tools.md) 第 10 节），instructions 的最后一行指向它。

## 维护规则

- 内容以完整指南为准，这里只是摘要。完整指南改了规则，这里同步改。
- 工具名按**命令 ID（点号）**写，例如 `stage.describe`、`attr.clear`；服务器附加说明会告诉 Agent 实际工具名用下划线。
- 17 有测试（`guide_instructions_fit_and_name_real_tools`）：两份都不超过 3,000 字符，必须提到 `stage.describe`，提到的每个点号名字都必须是 `bae mcp` 已提供的工具，或在 17 的"暂未提供"名单（`PENDING`）里。测试用的工具目录是 `Workspace::new`，只含 02 的内置命令、17 和 21 的命令，**不含应用模块发布的命令**（例如 12 的 `key.ease`）。所以模块命令写成实际工具名（`key_ease`，下划线不会被测试当成命令 ID 检查）。改完运行 `cargo test -p bae-agent --lib guide_instructions`。
- 长度上限 3,000 字符。要加内容就删掉不那么关键的内容，不要加长。
- 中英文两个版本的条目一一对应，改一个就改另一个。

