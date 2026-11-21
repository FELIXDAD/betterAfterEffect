# 界面外壳接口（bae-ui）

负责工作流：11 `ui/shell`。本文说明其他工作流怎样把面板、工作区、快捷键接进界面。代码在 `crates/bae-ui`，模块文档里有可运行的示例。

## 1. 注册方式

所有条目都在各自 crate 的 `AppModule::build` 里注册到 `bae-app` 的注册表（见 `app-shell.md` 第 2 节），插件以后也走同一条路。

| 注册表 | id | 内容 |
|---|---|---|
| `Registry<PanelKind>` | 面板类型，如 `"timeline"` | 标题（英文，界面经 `tr()` 翻译）、是否可以开多个、工厂函数 |
| `Registry<WorkspacePreset>` | 工作区，如 `"motion"` | 标题、默认布局（`dock::Layout`）、使用的键位表 id、在工作区栏里的顺序 |
| `Registry<Keymap>` | 键位表，如 `"ae"`、`"ai"` | 快捷键列表（`Binding`） |
| `Registry<KeymapBindings>` | 任意唯一 id，如 `"motion.timeline"` | 模块往某个键位表追加的快捷键（12 的时间轴 `U`、`J`/`K` 等），不必替换整个键位表 |
| `Registry<ActionInfo>` | 命令 id，如 `"edit.undo"` | 界面层动作的标题；`bae-model` 的命令不用在这里注册 |

`bae_ui::UiModule` 由外壳加载：注册外壳自己的动作；所有模块 build 完之后，给工作区布局里用到、但没有模块注册的面板补一个占位面板（显示"尚未实现"）。所以 12 号先注册 `"timeline"` 也好、晚一点注册也好，布局都不会出错；注册了同名 id 就替换掉占位。

## 2. 面板

```rust
app.register("timeline", PanelKind::new("Timeline", |spawn| Box::new(Timeline::new(spawn))).multi_instance())?;
```

- **面板 id**（`PanelId`）是布局里保存的字符串：单实例面板就是类型名（`"project"`），可以开多个的面板写成 `类型:实例`（`"timeline:comp-1"`）。`PanelId::kind()` 取类型。
- **`Panel` trait**（在界面线程上，不要求 `Send`）：
  - `ui(ui, cx)`：画面板内容。
  - `tab(cx)`：标签上的标题、标签色块、锁（默认用类型标题）。
  - `menu(ui, cx)`：追加到面板 ☰ 菜单里的项。
  - `on_action(invocation, cx) -> bool`：面板获得焦点时，快捷键和菜单命令先交给它（缩放、视图切换这类只影响视图的动作）。
  - `save_state()`：视图状态随工作区布局保存，重新打开时通过 `PanelSpawn::state` 交回工厂。
- **`PanelContext`**：
  - `stage: &Stage`：当前工程合成后的 Stage，只读。
  - `author: &AuthorId`：本机用户。
  - `resources: &Resources`：应用资源（`Arc<GpuContext>`、`Arc<ViewportTextureRegistry>`，以及别的模块插入的服务，例如 17 的 Agent 会话）。
  - `invoke(id)` / `invoke_with(id, params)`：请求执行一个命令 id，本帧结束后由外壳执行。**面板不直接改数据**，改动都经过 `bae-model` 的 `CommandRegistry`，成为带作者的一步撤销。
  - `open_panel(id)`、`close()`：打开 / 关闭面板。
  - `focused`：是否是获得焦点的面板（AE 的蓝框）。

## 3. 命令 id 的解析顺序

菜单项、快捷键、工具栏按钮、面板请求都只给出命令 id 和 JSON 参数（`Invocation`）。外壳依次尝试：

1. 获得焦点的面板的 `on_action`；
2. 外壳自己的动作（`bae_ui::action::ids`）：`edit.undo` / `edit.redo`（`Stage::undo/redo`，按本机作者）、`workspace.switch`（参数 `{"id": "motion"}`）、`workspace.reset`、`workspace.save`、`window.panel`（参数 `{"id": "<面板 id>"}`）、`window.close_panel`、`window.maximize_frame`、`app.quit`、`app.about`、`app.keyboard_shortcuts`、`app.language`、`file.*`；
3. `bae-model` 的命令：`CommandRegistry::execute(stage, author, id, params)`，没有参数时传 `{}`。

只有第 3 步会改工程数据。

## 4. 快捷键

- `Chord` 写法和 AE 文档一致：`"Cmd+Shift+Z"`、`"Alt+["`、`"U"`。`Cmd` 是平台命令键（macOS 的 ⌘、Windows 的 Ctrl），`Ctrl` 是 macOS 的 Control 键。匹配时同时看逻辑键和物理键，所以 `Shift+[` 能匹配，非拉丁键盘布局下快捷键也有效。
- `Binding { chord, action, params, when }`：`when` 是面板类型，表示只在这类面板获得焦点时生效；同一个键，面板内的绑定优先于全局绑定。
- 每个工作区指定一个键位表，切换工作区就切换快捷键。
- 最终键位表 = 键位表本身 + 所有指向它的 `KeymapBindings` + 用户自定义。模块卸载（插件）时它的 `KeymapBindings` 随之撤销。
- 自定义：`KeymapOverrides { add, remove }` 保存在用户设置里，`Keymap::with_overrides` 合成最终键位表。`Keymap::conflicts()` 列出同一作用域里重复的键，`Keymap::conflict_for(chord, when)` 在用户确认前检查冲突。
- 不带修饰键的快捷键（`U`、`P`、`J`、`K` …）在文本框获得键盘焦点时不触发（`Chord::is_typing_key`）。

## 5. 工作区

`WorkspacePreset::new(title, keymap, root)` 给出默认布局。用户的修改由外壳保存：切换工作区时保留各自当前的布局；"重置为已保存的布局"回到用户上次"保存对此工作区所做的更改"时的布局，没保存过就回到预设。

## 6. 外壳提供的应用资源

| 资源 | 谁插入 | 用途 |
|---|---|---|
| `Arc<bae_ui::history::HistoryLog>` | `UiModule` | 历史面板的数据。持有 Stage 的一方用 `stage.subscribe(move \|e\| log.record(e))` 喂事件，执行命令前调用 `log.label_next(标题)` 记下命令名（等 02 给 `StageEvent` 加上 `label` 后就不需要了）。 |
| `Arc<dyn bae_ui::agent::AgentFeed>` | 外壳（`apps/bae`，基于 bae-agent） | Agent 活动记录（工具调用）和提案列表。提案的接受 / 拒绝走 `proposal.accept` / `proposal.reject` 命令。 |
| `Arc<bae_agent::AuditLog>` | 外壳 | 应用内的 MCP 服务（17，HTTP 传输）应该写进同一个日志，活动面板才能看到。 |

## 7. 输入法

`Workbench` 每帧把 `PlatformOutput::ime` 的锚点换成光标矩形（`workbench::ime_follows_caret`），拼音候选框出现在光标下方。自己画文字编辑的面板（例如合成查看器里的文字工具）设置 `PlatformOutput::ime` 时同样会被处理。文本框有焦点时，不带修饰键的快捷键和文本编辑键不会触发命令。验证报告见 `docs/workstreams/11-ui-shell.md`。

## 无障碍（accesskit）

自绘控件（`ui.interact` / `allocate_exact_size` 做的按钮、开关、标签页、拆分条、热文本）都要用 `bae_ui::Accessible`（`labeled` / `labeled_toggle` / `labeled_value`）给出标签，固定文字走 `tr`、中英文都要有；egui 自带控件自己带标签。读屏（VoiceOver、讲述人）和 Windows 上的 UI 自动化都靠这些标签找控件，验收测试 `startup::controls_have_accessibility_labels` 要求没有标签的控件不超过 10%。工具栏工具和工作区栏条目用单选按钮角色（同时只有一个处于选中状态）。
