# 脚本

BAE 可以运行 After Effects 风格的脚本：用 JavaScript 写，对象模型沿用 AE 脚本的名字（`app.project`、`CompItem`、`layers`、`property()`、`setValue`、`setValueAtTime`……），大部分常用的 AE 脚本稍作修改就能运行。

- 脚本引擎是 QuickJS，和表达式用的是同一个引擎，没有引入新的运行时。
- 脚本对工程的**每一次修改都是一个 BAE 命令**（`layer.create`、`attr.set_at`、`attr.set_animated`、`key.delete`、`effect.apply`……），和菜单、快捷键、MCP 工具执行的是同一批命令。
- **一次运行 = 一个撤销步骤**：脚本做了多少修改，“编辑 ▸ 撤销”一次全部撤回。

代码在 `crates/bae-script`；菜单入口在 `crates/bae-ui-motion/src/scripts.rs`。

## 运行脚本

| 方式 | 说明 |
|---|---|
| 文件 ▸ 脚本 ▸ 运行脚本文件… | 选一个 `.jsx` 或 `.js` 文件运行 |
| 文件 ▸ 脚本 ▸ *脚本名* | 运行脚本文件夹里的脚本（和 AE 的 Scripts 文件夹一样，启动时列出） |
| 文件 ▸ 脚本 ▸ 安装脚本文件… | 把选中的脚本复制到脚本文件夹，立即出现在菜单里（AE 要重启，BAE 不用） |
| MCP 工具 `script.run` | Agent 直接运行一段脚本，见下文 |
| `bae.commands.invoke("script.run", …)` | 不允许：脚本不能再运行脚本 |

脚本文件夹：

| 平台 | 位置 |
|---|---|
| macOS | `~/Library/Application Support/BAE/Scripts` |
| Windows | `%LOCALAPPDATA%\BAE\Scripts` |
| 便携模式（`BAE_HOME`） | `<BAE_HOME>/data/Scripts` |

运行时脚本能拿到：当前合成（`app.project.activeItem`，即查看器和时间轴里打开的合成）、选中的图层（`comp.selectedLayers`）和当前时间（`comp.time`）。

## 撤销、错误和提示

- 整个脚本是一个撤销步骤，“编辑 ▸ 撤销”里显示为“撤销 运行脚本”。`app.beginUndoGroup(name)` / `app.endUndoGroup()` 照常可以写：直接调用 `bae_script::run` 的地方（测试、插件）用这个名字作为撤销名称；一次运行里写了多个撤销组，也仍然只是一个撤销步骤。
- 脚本出错时停止运行，**出错之前的修改保留**（和 AE 一样），弹窗显示错误类型、行号和信息，例如 `TypeError at line 12: cannot read property 'name' of null`。一次撤销就能撤回这次运行的全部修改。
- `alert(msg)` 不会中途打断脚本：所有提示在脚本结束后合并到一个弹窗里显示。
- `confirm(msg)` 总是返回 `false`（脚本运行中不弹出需要回答的对话框，避免意外执行破坏性操作），`prompt(msg, 默认值)` 返回默认值。
- 从「文件 ▸ 脚本」运行的 JSX 文件和 ScriptUI 面板例外：`confirm`、`prompt` 和对话框的 `show()` 会停下来等用户回答。等待前后的修改各算一个撤销步骤；面板里每次点击也是一个撤销步骤。回调出错时弹窗显示错误，面板继续可用。
- ScriptUI 的编辑框输入时触发 `onChanging`，按回车或失去焦点时才触发 `onChange`；滑块拖动时触发 `onChanging`，松手时触发 `onChange`。
- `$.writeln()`、`$.write()`、`console.log()` 写入日志（日志文件里的 `script` 目标），MCP 的返回结果里也有。
- 限制：每次运行最多 30 秒（死循环会被停下，之前的修改保留）、128 MiB 内存。脚本不参与逐帧渲染。

## API 参考

下面是目前支持的子集。名字、参数顺序和单位都和 AE 脚本一致：**索引从 1 开始**；**时间是合成时间，单位秒**；位置、缩放等数组和 AE 一样（图层的位置、锚点、缩放是三个分量 `[x, y, z]`，颜色是 `[r, g, b, a]`，取值 0–1）。不支持的成员调用时抛出 `Error: … is not supported by BAE scripting yet`。

### app

| 成员 | 说明 |
|---|---|
| `app.project` | 当前工程 |
| `app.version`、`app.buildName` | BAE 版本 |
| `app.beginUndoGroup(name)`、`app.endUndoGroup()` | 见“撤销” |
| `app.beginSuppressDialogs()`、`app.endSuppressDialogs()` | 接受调用，不做任何事 |

### Project

| 成员 | 说明 |
|---|---|
| `items` | 全部项目（ItemCollection，`items[1]`、`items.length`） |
| `numItems`、`item(i)` | 项目数、第 i 个项目 |
| `activeItem` | 当前合成（CompItem），没有时为 `null` |
| `rootFolder` | 根文件夹（FolderItem） |
| `itemByID(id)` | 按 `id` 取项目 |
| `selection` | 目前总是空数组 |
| `items.addComp(name, width, height, pixelAspect, duration, frameRate)` | 新建合成（`comp.create`） |
| `items.addFolder(name)` | 新建文件夹（`folder.create`） |

### Item、FolderItem、FootageItem、CompItem

所有项目：`id`、`name`（可写）、`typeName`（`"Composition"` / `"Folder"` / `"Footage"`）、`parentFolder`、`remove()`。

- `FolderItem`：`items`、`numItems`、`item(i)`；`items.addComp(...)`、`items.addFolder(name)` 建在这个文件夹里。
- `FootageItem`（素材和纯色）：`width`、`height`、`pixelAspect`、`duration`、`frameRate`、`file`（`{fsName, name}`）、`mainSource.color`（纯色的颜色）、`mainSource.isStill`。
- `CompItem`：
  - 可读写：`name`、`width`、`height`、`pixelAspect`、`duration`、`frameRate`、`bgColor`（写入走 `comp.set_settings`），以及合成播放头 `time`
  - 只读：`frameDuration`、`workAreaStart`、`workAreaDuration`、`displayStartTime`、`numLayers`、`selectedLayers`
  - `layer(index 或 名称)`、`layers`（LayerCollection，`layers[1]` 是最上面的图层）

### LayerCollection（`comp.layers`）

| 方法 | 说明 |
|---|---|
| `addSolid(color, name, width, height, pixelAspect[, duration])` | 纯色图层，`color` 是 `[r, g, b]` |
| `addText([text])` | 文本图层，`text` 可以是字符串或 `TextDocument` |
| `addBoxText(size[, text])` | 目前同 `addText` |
| `addNull([duration])` | 空对象 |
| `addShape()` | 形状图层 |
| `addCamera(name)`、`addLight(name)` | 摄像机、灯光 |
| `add(item[, duration])` | 把合成（预合成）或素材加成图层 |
| `byName(name)` | 按名称找图层 |

新图层和 AE 一样放在最上面。

### Layer（AVLayer、TextLayer、ShapeLayer、CameraLayer、LightLayer）

`instanceof` 可以区分图层类型（`layer instanceof TextLayer`）。

| 成员 | 说明 |
|---|---|
| `name` | 可写 |
| `index` | 只读，1 = 最上面 |
| `inPoint`、`outPoint` | 可写（`layer.trim`） |
| `startTime` | 可写（`layer.shift_time`，关键帧随图层移动） |
| `enabled`、`solo`、`shy`、`locked`、`label` | 可写（`layer.set_attr`） |
| `threeDLayer`、`adjustmentLayer`、`motionBlur`、`blendingMode` | AVLayer，可写；`blendingMode` 用 BAE 的混合模式名，例如 `"multiply"` |
| `parent` | 可写；和 AE 一样保持图层在画面上的位置（`layer.set_parent`）。`setParentWithJump(layer)` 不保持 |
| `selected` | 可写，只影响本次运行里的 `selectedLayers` |
| `containingComp`、`source`、`nullLayer`、`hasVideo`、`width`、`height`、`stretch` | 只读 |
| `transform` | 变换属性组；`layer.position`、`layer.opacity` 等快捷写法也可以 |
| `effect` / `property("Effects")` | 效果属性组 |
| `mask` / `property("Masks")` | 蒙版属性组 |
| `text` / `sourceText` | 文本图层：文本属性组 / “源文本”属性 |
| `property(名称 / matchName / 序号)` | 取属性组或属性 |
| `remove()`、`duplicate()` | 删除、复制（返回新图层） |
| `moveToBeginning()`、`moveToEnd()`、`moveBefore(layer)`、`moveAfter(layer)` | 调整图层顺序 |
| `activeAtTime(t)` | 图层在时间 t 是否显示 |

变换属性：`Anchor Point`、`Position`、`X Position`、`Y Position`、`Z Position`、`Scale`、`Orientation`、`X Rotation`、`Y Rotation`、`Rotation`（也叫 `Z Rotation`）、`Opacity`、`Point of Interest`，也可以用 AE 的 matchName（`ADBE Position`、`ADBE Opacity`……）或 `transform.position` 这种属性名。

### PropertyGroup

`name`、`matchName`、`numProperties`、`property(名称 / matchName / 序号)`、`parentProperty`。和 AE 一样可以直接当函数调用：`layer.effect("Gaussian Blur")("Blurriness")`。

- 效果组：`addProperty(matchName 或 名称)` 应用效果（`effect.apply`）并返回这个效果；`canAddProperty()`。
- 单个效果：`enabled`（可写）、`remove()`、`property(参数名 / 参数 matchName / 序号)`。
- 蒙版：`Mask Path`、`Mask Feather`、`Mask Opacity`、`Mask Expansion`。

### Property

| 成员 | 说明 |
|---|---|
| `value` | 当前时间的值 |
| `valueAtTime(t, preExpression)` | 返回指定时间的值。第二个参数为 `true` 时读取表达式计算前的值 |
| `setValue(v)` | 设静态值；属性有关键帧时和 AE 一样报错，请用 `setValueAtTime` |
| `setValueAtTime(t, v)` | 在时间 t 设关键帧（没有关键帧时打开秒表） |
| `setValuesAtTimes(times, values)`、`setValueAtKey(i, v)` | 批量设关键帧、改第 i 个关键帧的值 |
| `numKeys`、`keyTime(i)`、`keyValue(i)`、`nearestKeyIndex(t)` | 读关键帧 |
| `addKey(t)` | 在时间 t 用当前值加关键帧，返回序号 |
| `removeKey(i)` | 删除第 i 个关键帧（删完时关闭秒表，保留最后的值） |
| `setInterpolationTypeAtKey(i, in[, out])` | `KeyframeInterpolationType.LINEAR / BEZIER / HOLD` |
| `expression`、`expressionEnabled` | 可读写（`attr.set_expression`） |
| `isTimeVarying`、`name`、`matchName`、`parentProperty` | 只读 |

源文本的值是 `TextDocument`：`text`、`font`、`fontSize`、`fillColor`、`strokeColor`、`strokeWidth`。`setValue("新文字")` 只改字符；`setValue(doc)` 可以同时改字号、颜色等。`new TextDocument("文字")` 可以新建。

### 其他全局对象

| 对象 | 说明 |
|---|---|
| `alert`、`confirm`、`prompt` | 见“撤销、错误和提示” |
| `$.writeln`、`$.write`、`console.log` | 写日志 |
| `bae.commands.invoke(id, params)` | 直接执行任意 BAE 命令（和 MCP 工具同名同参数），返回命令的输出，例如 `bae.commands.invoke("key.ease", {...})` |
| `TextDocument`、`KeyframeInterpolationType` | 同 AE |

## 和 AE 的差别

- `item.id`、`layer.id` 是工程里的路径字符串（例如 `/comps/Comp_1`），不是数字；`app.project.itemByID()` 接受它。
- 没有 `File`、`Folder`、`Socket`，也不能读写文件系统；ScriptUI 只支持常用控件（按钮、文字、编辑框、复选框、单选按钮、下拉列表、列表框、滑块、进度条、面板和分组）；`app.project.save()`、`importFile()` 等暂不支持。
- ExtendScript 的预处理行（`#target aftereffects`、`#include`、`#targetengine`）会被忽略；可以使用现代 JavaScript 语法（`let`、箭头函数、模板字符串……）。
- 撤销：见上文，一次运行是一个撤销步骤（等用户回答的 JSX 文件和 ScriptUI 面板除外）。

## MCP：`script.run`

`script.run` 是一个普通命令，所以会自动出现在 MCP 工具列表里（`bae mcp`），Agent 可以一次提交一段脚本完成多步操作，结果仍是一个撤销步骤，并受 Agent 的写权限约束（脚本里的每个命令都写进 Agent 自己的提案层）。

参数：

| 参数 | 说明 |
|---|---|
| `source` | 脚本源码（必需） |
| `name` | 名称，用于提示信息 |
| `active_comp` | `app.project.activeItem` 返回的合成路径 |
| `selected_layers` | `comp.selectedLayers` 返回的图层路径 |
| `time` | 当前时间（秒） |

返回：`log`（日志）、`alerts`（提示）、`error`（出错时的错误和行号）、`result`（脚本最后一个表达式的值，图层和项目会转成 `{"layer": 路径, "name": 名称}`）。

脚本和 MCP 工具用的是同一套命令：脚本里能做的事，Agent 用单个工具也能做；反过来，任何命令都可以在脚本里用 `bae.commands.invoke` 调用。

## 示例

[`plugins/examples/scripts/`](../../plugins/examples/scripts)：

| 文件 | 内容 |
|---|---|
| `lower-third.jsx` | 新建合成，色条 + 标题 + 控制空对象，空对象 0.5 秒滑入 |
| `bounce-selected.jsx` | 给选中图层在当前时间加缩放弹跳关键帧 |
| `rename-layers.jsx` | 把选中图层（或全部图层）按顺序重命名 |
| `align-to-grid.jsx` | 把图层位置对齐到 10 像素网格 |

## 把脚本做成插件

将 [align-to-grid 示例](../../plugins/examples/align-to-grid) 放入 betterAfterEffects 的用户插件目录。打开合成，选中图层，使用「图层 ▸ 变换 ▸ 将图层对齐到网格」或 `Ctrl+Alt+G` 对齐到 10 像素网格。「文件 ▸ 脚本 ▸ 列出未对齐网格的图层」会在日志中列出未对齐的图层数量。通过 MCP 调用同一自动化入口会得到图层名称列表。

在 `[script]` 的 `entry` 中填写 ES 模块文件路径。模块可以从 `"bae"` 导入 `app`、`CompItem` 等脚本对象。`[[command]]` 和 `[[automation]]` 的 `entry` 分别填写导出的函数名。宿主在调用时把参数对象传给函数，并将其返回值转换成 JSON。命令的参数 Schema 来自清单指向的 JSON 文件，供 MCP 展示；导出函数负责检查传入值。

| 声明 | 调用 ID | 示例 |
| --- | --- | --- |
| `[[command]]`，`implementation = "script"` | `plugin.<插件 ID>.<命令 ID>` | `plugin.com.example.align-to-grid.align-to-grid`，参数 `{"size":20,"layers":["/Comp1/layers/Layer1"]}`；路径填实际工程中的图层路径 |
| `[[automation]]` | `script.<插件 ID>.<自动化 ID>` | `script.com.example.align-to-grid.report-off-grid`，参数 `{}` |

MCP 工具名会把调用 ID 中的点换成下划线，例如 `plugin_com_example_align-to-grid_align-to-grid`。

每次插件调用独占一个 QuickJS 运行时，执行时限 10 秒，堆上限由 `memory_limit_mb` 决定，默认 64 MiB。一次成功调用里的所有修改可以一次撤销。导出函数抛出错误时，命令返回错误，事务回滚。界面调用可读取当前合成、选区和时间。无界面调用请显式传入需要处理的图层。

脚本默认可读工程。需要修改工程的插件在 `[permissions]` 声明 `project = "write"` 并安装在用户、系统或应用插件目录。工程目录及 `BAE_PLUGIN_PATH` 按工程作用域加载，只授予读取权限。`project = "none"` 仅允许计算、日志和提示。脚本插件可调用模型和动效命令，模块导入支持内置 `"bae"`。

### 播放头、表达式和权限

`comp.time` 使用合成时间，单位为秒。赋值后播放头对齐到帧，各合成保留各自的时间。`Property.value` 返回该合成当前帧的表达式结果。用 `valueAtTime(t, true)` 读取表达式计算前的值，用 `valueAtTime(t, false)` 读取计算后的值。脚本可以用 `PropertyType.PROPERTY` 判断属性类型，用 `BlendingMode.MULTIPLY` 等常量读写图层混合模式。

插件安装时编译 ES 模块，调用时执行入口。每次调用的日志和提示合计最多 256 条、64 KiB。脚本通过应用命令注册表执行已加载模块的命令。涉及文件路径的命令需要宿主授予文件句柄，当前脚本接口不提供此类句柄。

[合成摘要](../../plugins/examples/composition-report) 演示只读自动化。打开合成后运行文件菜单里的 Composition Summary，或通过 MCP 获取名称、时长、帧率和图层数。[选中图层淡入](../../plugins/examples/fade-selected) 演示写入关键帧。选中图层后运行图层菜单里的 Fade Selected Layers，播放头处写入起始关键帧，指定帧数后恢复该帧原有的不透明度。

### 普通脚本示例

```js
app.beginUndoGroup("Fade In");
var comp = app.project.activeItem;
for (var i = 0; i < comp.selectedLayers.length; i++) {
  var opacity = comp.selectedLayers[i].opacity;
  opacity.setValueAtTime(comp.time, 0);
  opacity.setValueAtTime(comp.time + 0.5, 100);
}
app.endUndoGroup();
```

AE 脚本对象模型的公开文档：<https://ae-scripting.docsforadobe.dev/>（本页只列出 BAE 支持的部分）。
