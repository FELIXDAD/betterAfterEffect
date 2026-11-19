# 17 · Agent 接入（MCP）

| 项 | 内容 |
|---|---|
| 分支 | `agent/api` |
| Orca 工作区 | `bae-17-agent-api` |
| 阶段 | P1 |
| 依赖 | 02（命令）；04（截图，未完成前先模拟） |
| 负责目录 | `crates/bae-agent/` |

## 目标

让任何支持 MCP 的 Agent 都能连接 BAE、读懂工程、通过命令修改工程、看到渲染结果，并且作为一个受控的协作者参与团队工作。

## 要做

- **MCP 服务器**：使用 rmcp，支持 stdio（`bae mcp`）和 Streamable HTTP（连接正在运行的桌面应用或 Hub，本地用 token 认证）；跟随最新的 MCP 规范。
- **工具自动生成**：从 CommandRegistry 生成 MCP 工具（名字、描述、参数 Schema 都来自命令定义）。新增命令后无需修改本工作流的代码。
- **查询与视觉工具**：
  - `stage.describe`：给 Agent 读的工程摘要（合成树、图层、关键帧概况），长度可控。
  - `prim.get`、`prim.find`、`selection.get`、`attribute.evaluate`（某时刻的值）。
  - `render.frame`：返回 PNG。
  - `render.contact_sheet`：把多帧缩略图拼成一张图，用来检查动画节奏。
  - `history.*`、`sublayer.*`。
- **Agent 会话**：身份与在线状态；默认 Edit Target 是自己的提案层 `agent/<name>`；接受/拒绝/合并的 API（界面由 11 的 Agent 面板调用）。
- **安全与控制**：三级权限（只读 / 只写自己的层 / 可写任意层）；速率限制和渲染预算；审计日志，每个 Agent 操作都能追溯。
- **MCP instructions**：内置一段精简的操作规范（内容来自 18）。
- **端到端示例**：用 Claude Code 和 Codex 分别连接，完成"做一个 3 秒的标题出场动画并渲染预览"，写成可重复运行的测试脚本。

## 对外接口

- `bae mcp` 子命令、本地 HTTP 端点、Agent 会话 API。

## 验收

- Claude Code 和 Codex 都能连接并完成端到端示例。
- Agent 的修改只出现在它的提案层，用户可以逐条接受或拒绝。
- 只读权限下的 Agent 无法修改任何数据（有测试）。

## 不做

- 操作规范的内容（18）、Agent 面板的界面（11）。

## 进度

- [ ] MCP 服务器（stdio）
- [ ] 从 CommandRegistry 生成工具
- [ ] 查询与视觉工具
- [ ] Agent 会话与提案层
- [ ] 权限与审计
- [ ] HTTP 传输
- [ ] 端到端示例

## 请求（其他工作流在这里留言）
