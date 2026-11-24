# 数据模型（bae-model）

> 状态：PR A（基础类型、存储、命令、撤销、事件）+ PR B（引用、下推/合并）+ PR C（工程文件，见 `project-format.md`）。Prim schema 在 PR D 补充，见文末"还没做"。
> 代码：`crates/bae-model`。概念来源：`docs/ARCHITECTURE.md` 第 4 节。

## 1. 概念

| 类型 | 说明 |
|---|---|
| `Stage` | 打开的工程。持有若干 `Sublayer`，按层叠顺序（强 → 弱）合成；同时负责事务、按作者撤销、事件。 |
| `Sublayer` | 工程层。一组"意见"（每个 Prim 路径上的一个 `PrimSpec`），底层是一个 Loro 文档，是单独保存、单独同步的单元。 |
| `SublayerId` | 工程层 ID，例如 `root`、`anim/shot010`、`agent/claude-01`。字母、数字、`_`、`-`，`/` 作分隔。 |
| `PrimPath` | 绝对路径，例如 `/comps/main/layers/title`。`/` 是伪根。 |
| `PrimSpec` | 某一层对某个 Prim 的意见：`specifier`（`def` 定义 / `over` 只覆盖）、`prim_type`、`attrs`、`references`。 |
| `AttrSpec` | 某一层对某个属性的意见：静态值 `value`、关键帧 `keyframes`、表达式 `expression`。 |
| `Value` | 属性值（见第 3 节）。 |
| `Time` | 整数 flicks，1 秒 = 705,600,000。`FrameRate` 是有理数帧率（如 30000/1001），常用帧率的每一帧都能整除表示。 |
| `Op` / `Edit` | 对一个工程层的最小修改；`Edit` = 目标层 + `Op`。 |
| `Command` / `CommandRegistry` | 有 ID、说明、参数 JSON Schema 的命令；在事务里运行，是一个撤销单元。 |
| `StageEvent` | 每次提交后发出，列出受影响的路径和时间范围。 |
| `AuthorId` | 修改者，例如 `user:alice`、`agent:claude-01`。撤销按作者区分。 |

### 命名

- Prim 名（路径的一段）是标识符：字母（含中文等 Unicode 字母）、数字、`_`、`-`，不能以 `-` 开头。AE 图层名可以重复、可以带空格和斜杠，所以**界面上显示的名字是属性，不是 Prim 名**（具体属性名在 Prim schema 里定）。
- 属性名：字母、数字、`_`、`-`，`:` 用作命名空间，例如 `transform:position`。
- 插件定义的 Prim 类型写成 `"<插件 ID>:<类型>"`，例如 `com.example.particles:Emitter`。

## 2. 合成规则

- **意见的强弱顺序**（`Stage::opinions(path)`，强 → 弱）：
  1. 本工程层叠里同一路径上的 spec，强层在前；
  2. 这个 Prim 自己引用的 Prim 的对应 spec；
  3. 祖先引用进来的对应 spec，近的祖先在前。
  被引用的内容会递归合成（它自己的层叠和引用），所以引用可以嵌套。
- **Prim 是否存在：** 父 Prim 存在，并且至少有一条意见是 `def`。只有 `over` 的路径不算存在（不会出现在遍历里）。
- **属性：** 第一条对该属性有意见的生效，整条意见（值 + 关键帧 + 表达式）一起生效。`PrimView::attr_origin` 返回生效的意见：来自哪个工程、哪一层、层里的哪个路径，以及是不是经过引用来的。界面上"这个值来自哪一层"就用它。
- **类型、引用：** 取最强的非空意见。同一个 Prim 在多层都写了引用列表时，最强的那份生效（暂不做列表合并）。
- **子 Prim 顺序：** 最弱的来源（被引用的内容、弱层）里的顺序在前，强的来源新增的子 Prim 排在后面。
- **在强层里改弱层定义的 Prim：** 命令会自动在目标层为该 Prim 及其祖先补 `over` 空壳（`Transaction::ensure_spec`），再写属性。
- **删除、移动、改名是命名空间操作：** 作用于层叠里所有在该路径上有意见的层，覆盖会跟着 Prim 走。只能对本工程层叠里有 `def` 的 Prim 做；经引用来的 Prim 只能覆盖属性（报 `ProvidedByReference`）。

### 引用（Reference）

- `Reference { project, prim }`：`project` 为空表示同一工程；否则是另一个工程相对本工程的路径，要先用 `Stage::attach_project(key, stage)` 挂上（PR C 的工程文件会自动加载）。没挂上或目标不存在的引用由 `Stage::unresolved_references()` 列出，界面显示为缺失。
- 被引用的子树挂在引用方下面，比引用方自己的所有本地意见都弱。在引用方路径下改属性，会在目标层里补 `over` 并写入覆盖，不会改到被引用的原件。
- 引用只带入目标 Prim 及其子树；目标祖先上的引用不会带进来。
- 循环引用和超过 16 层的嵌套会被截断，不会死循环。
- **改动传播：** 修改被引用的内容时，事件里会同时报告引用方路径上的对应改动（嵌套引用也会逐层映射）。目标本身被删除、新建或改了引用时，引用方报 `resynced`。

### 平铺（flatten）

`Stage::flatten()` 把合成结果输出成只有 `def` 的普通 `PrimTree`：每个属性取生效的意见，引用已经展开。用于导出到没有分层概念的格式（AE、Lottie），也用来给 Agent 做摘要。

## 3. 值与 JSON

`Value` 序列化成带标签的 JSON，整数和浮点不会混淆：

```json
{"type": "float", "value": 50.0}
{"type": "vec2", "value": [960.0, 540.0]}
{"type": "color", "value": [1.0, 0.5, 0.0, 1.0]}
{"type": "enum", "value": "multiply"}
{"type": "prim_ref", "value": "/comps/main/layers/bg"}
{"type": "bytes", "value": "AQID"}
```

类型：`float`、`int`、`bool`、`string`、`vec2`、`vec3`、`color`、`bezier`（AE 的顶点 + 相对切线 + closed，可选遮罩羽化点 `feather: [{segment, t, radius, tension, hold, inner, corner_angle}]`，对应 AE 的可变宽度遮罩羽化）、`enum`、`asset_ref`、`prim_ref`、`bytes`（base64，对应 AE"任意数据"参数和插件状态）、`float_list`、`time`（整数 flicks，用于入点、出点、开始时间、时长）、`frame_rate`（`{"num": 30000, "den": 1001}`）、`text_document`（文字图层的源文本，每个关键帧是一整份文档：`text`、`font`（PostScript 名）、`font_size`、`fill_color`、`stroke_color`、`stroke_width`、`justification`、`tracking`、`leading`……逐字符样式以后再加）。

关键帧（时间是 `AttrSpec.keyframes` 的键，单位 flicks）：

```json
{
  "value": {"type": "vec2", "value": [0.0, 0.0]},
  "interp_in": "bezier", "interp_out": "bezier",
  "ease_in": [{"speed": 0.0, "influence": 33.333}],
  "ease_out": [{"speed": 0.0, "influence": 33.333}],
  "spatial_in": [0.0, 0.0, 0.0], "spatial_out": [10.0, 0.0, 0.0],
  "flags": {"temporal_continuous": true, "roving": false},
  "label": 0
}
```

字段与 AE 对齐：插值 `linear | bezier | hold`；时间缓动每维一组（空间属性只有一组）`speed` + `influence`（百分比，AE 默认 33.33）；空间切线相对关键帧值；`flags` 是时间/空间连续、自动贝塞尔、漂移。空数组和默认值在 JSON 里省略。插值计算和表达式求值由 03 负责，本 crate 只存数据。

## 4. 存储（每个工程层一个 Loro 文档）

- 可移动树 `prims`：每个节点是一个 PrimSpec，子节点有序（分数索引）。节点元数据：
  - `name`：Prim 名
  - `spec`：`"def"` / `"over"`
  - `type`：类型（可选）
  - `refs`：引用列表的 JSON（可选）
  - `attrs`：可合并 map，属性名 → 可合并 map：`value`（Value 的 JSON）、`expr`（表达式 JSON）、`keys`（可合并 map，flicks 十进制字符串 → 关键帧 JSON）
- 每个字段各自"后写者胜"，所以两个人改同一 Prim 的不同属性、同一属性的不同关键帧，都能干净合并。用"可合并容器"（`ensure_mergeable_map`），两端同时创建同一个属性时不会互相覆盖。
- 两端并发创建同名的兄弟 Prim 时两者都保留，后出现的那个路径变成 `<名字>_<peer>_<counter>`，各端结果确定且一致。
- 读取不经过 Loro：每层在内存里维护一份按路径索引的镜像；本地 Op 增量更新镜像，导入远端更新时重建镜像，并对比前后差异生成事件。

## 5. Op、事务与撤销

Op（JSON 里 `"op"` 字段区分）：`create_prim`、`delete_prim`、`restore_prim`（恢复整棵子树）、`move_prim`（移动 / 改名 / 重排）、`set_prim_type`、`set_specifier`（def ↔ over）、`set_value`、`set_keyframe`、`remove_keyframe`、`set_expression`、`set_attr`（整条意见替换或清除）、`set_references`。

- 每个 Op 在应用前完成全部校验，失败时这一层不会被改动；应用后返回逆 Op。
- `Stage::transact(author, label, |tx| ...)`：一个撤销单元。闭包失败时，已经应用的 Op 按逆序撤回，不发事件，也不进撤销栈。提交时给每个被修改的 Loro 文档写一条提交信息 `{"author", "label"}`（供历史和审计）。
- **按作者撤销：** 每个作者有自己的撤销/重做栈（最多 1000 步）。撤销只回放自己那一步的逆 Op；如果某个逆 Op 已经不适用（比如协作者删了这个 Prim），就跳过它，其余照常撤销。新的修改会清空该作者的重做栈。远端导入不进任何撤销栈。
- 层叠操作（新建/删除/移动工程层）和切换 Edit Target 不可撤销，单独发 `origin = stack` 的事件。

## 6. 命令

`Command` trait：`ID`、`TITLE`、`DESCRIPTION`、`Params`（用 schemars 生成 JSON Schema，字段的文档注释就是给 Agent 看的说明）、`Output`。`CommandRegistry` 支持运行时注册和按插件 ID 整体注销（`register_owned` / `unregister_owner`），插件也可以直接实现 `DynCommand`。

内置命令：

| ID | 作用 |
|---|---|
| `prim.create` | 在已存在的父 Prim 下创建 Prim |
| `prim.delete` | 删除 Prim 及其子树（所有层） |
| `prim.move` | 移动 / 重排 / 改名（所有层） |
| `prim.rename` | 改名 |
| `attr.set` | 设置静态值（目标层） |
| `attr.clear` | 清除目标层对该属性的意见，露出弱层的值 |
| `key.set` / `key.remove` | 增改 / 删除关键帧 |
| `attr.set_expression` | 设置、停用或删除表达式 |
| `prim.set_references` | 设置引用 |
| `sublayer.create` | 新建工程层（默认放在最强） |
| `sublayer.set_edit_target` | 切换 Edit Target |
| `opinion.move` | 把某个 Prim 的覆盖（一个属性或全部，含类型、引用、def）从一层移到另一层，即"下推"；`keep: true` 时为复制。源层里变空的 `over` 及其空祖先会被删掉 |
| `asset.set_file` | 设置或重新链接素材文件（见 `project-format.md` 第 5 节） |
| `sublayer.merge` | 把一层的全部意见合并进另一层（接受 Agent 的提案层、把岗位层合进 root），一个撤销单元 |

## 7. 事件

`StageEvent { origin, author, sublayers, changes }`，`origin` 为 `local | undo | redo | remote | stack`。`Change`：

- `prim_created`、`prim_removed`（含子树）、`prim_moved`（`from == to` 表示重排）、`prim_changed`（类型、specifier、引用）
- `attr_changed { path, attr, range }`：静态值和表达式的范围是全部时间；关键帧的范围是前一个关键帧到后一个关键帧，渲染缓存据此精确失效
- `resynced { path }`：该路径下任何东西都可能变了（层叠变化）

`Stage::subscribe` 的回调在修改所在线程同步执行，要保持轻量。事件是保守的：弱层被强层遮住的修改也会报告。

## 8. 给 09 协同的接口

- `Sublayer::doc()`：底层 Loro 文档（只读使用；直接写入会绕过镜像）。
- `Sublayer::version()`、`export_snapshot()`、`export_updates(&since)`、`Sublayer::from_snapshot(id, bytes)`。
- `Stage::import_updates(sublayer, bytes)`：导入远端增量，返回并广播 `origin = remote` 的事件。
- `Stage::import_updates_batch(sublayer, &[blob])`：一次导入多条增量，只重建一次镜像、只发一个事件（Hub 模式下一秒内收到很多小增量时用这个）。
- `Sublayer::node_path(TreeID)` / `node_id(path)`：Loro 树节点和 Prim 路径互查（按 Prim 列历史差异时用），节点 ID 在改名、移动后不变。
- 每个工程层的 Loro 文档都开启了提交时间戳（`set_record_timestamp`），历史里每次提交都有时间。
- 只有大小写不同的工程层 ID（`Anim` 和 `anim`）会被拒绝，因为在 macOS 和 Windows 上它们会落到同一个文件。
- `bae_model::loro` 重新导出了 Loro crate，版本由 `bae-model` 决定。

## 9. 性能（验收）

`cargo test --release -p bae-model --test stage performance_budget -- --nocapture`，Apple M4：

| 指标 | 结果 | 要求 |
|---|---|---|
| 打开 1 万个 Prim（每个带一个属性和一个关键帧，快照 2.5 MB） | 111 ms（空闲时）；160–330 ms（机器满载，load 20） | < 500 ms |
| 单次属性修改到事件发出（平均 200 次） | 5.3 µs | < 1 ms |

## 10. Prim schema 与约定（PR D）

每种内置 Prim 类型的属性（名字、值类型、默认值、能否加关键帧、枚举选项）在 `crates/bae-model/src/schema.rs`，`bae_model::schema::to_json()` 可以导出全部（给 Agent、属性面板、文档用）。`attr.set`、`key.set`、`attr.set_expression` 会按 schema 检查：类型不对报 `TypeMismatch`（`float` 属性接受 `int`），枚举值必须在选项里，不能加关键帧的属性报 `NotAnimatable`。schema 里没有的属性（自定义属性、`ae:` 开头的、插件类型的属性）不检查，原样保存。

约定（回复 15 的请求）：

- **单位**与 AE 一致：缩放和不透明度用百分比，角度用度，位置用像素，时间用 flicks。AE 的值可以 1:1 搬过来。
- **关键帧的时间基准**：图层上的属性（以及它下面的遮罩、效果、形状内容、标记）的关键帧用**图层时间**，移动图层时关键帧跟着走，和 AE 内部一致；`合成时间 = start_time + 图层时间 × stretch / 100`。合成上的属性用合成时间。03 求值时负责换算。
- **AE 的十进制帧率**（回复 15 第 10 条）：AE 报告的 29.97、23.976、59.94 是十进制数（第 30 帧在 `30/29.97` 秒），不能被 flicks 整除。导入时一律换成 NTSC 有理数（`FrameRate::from_fps`：29.97 → 30000/1001 等），时间按帧号对齐（关键帧在第 n 帧就存 `frame_to_time(n)`）。两种算法一小时只差约 0.1 帧，以帧号为准，导回 AE 时也按帧号换算。不能被 flicks 整除的帧率也能用，`frame_to_time` 向上取整，保证 `time_to_frame` 能还原同一帧。
- **图层顺序**：合成下 `layers` 文件夹里的第一个子 Prim 是最上面的图层（AE 序号 1）。
- **图层来源**：预合成、素材、纯色都用图层的 `source` 属性（`prim_ref` 指向 Comp 或 Asset）。`references` 用于实例化其他工程（或本工程）里的 Prim 子树，例如链接的 Ai 画板、共享的角色资产。
- **属性命名**：图层变换是 `transform:anchor_point`、`transform:position`（vec3；`transform:separate_dimensions` 打开时用 `transform:position_x/_y/_z`）、`transform:scale`（vec3，百分比）、`transform:orientation`、`transform:rotation_x`、`transform:rotation_y`、`transform:rotation`、`transform:opacity`、`transform:point_of_interest`（摄像机、灯光）。图层设置是 `name`、`kind`、`source`、`start_time`、`in_point`、`out_point`、`stretch`、`parent`、`blend_mode`、`track_matte`、`track_matte_layer`、`three_d`、`adjustment_layer`、`effects_enabled`（效果开关 fx，默认 true；旧工程没有这个属性时按 true 读）、`motion_blur`……。按图层种类区分的属性带前缀：`text:source`、`camera:*`、`light:*`、`audio:levels`、`material:*`。遮罩是单独的 `Mask` Prim，属性不加前缀：`path`、`mode`、`inverted`、`feather`、`opacity`、`expansion`。完整列表以 `schema.rs` 为准。
- **AE 专有、BAE 不认识的属性**以 `ae:` 开头原样保存（编码规则见 15 的交换格式文档 4.8），导回 AE 时不丢。
- 文字图层的动画器组位于 `<layer>/animators/<name>`。`text.create_animator` 在一个撤销步骤里创建组及其 `range` 子选择器。只有已写入的 `animator:*` 属性参与组求值。多个 `TextSelector` 是组的有序直接子项。
- 新增 Prim 类型：`Marker`（`time`、`duration`、`comment`、`chapter`、`url`……，图层标记用图层时间）、`TextAnimator`（`animator:*` 属性）、`TextSelector`（范围、摆动、表达式选择器）。

## 11. 效果参数（PR D）

- `Effect` Prim：`match_name`（效果身份，例如 `ADBE Gaussian Blur 2`）、`name`、`enabled`、`plugin:id`、`plugin:version`、`state`（bytes，不透明的实例状态，对应 AE 的 sequence data）。效果实例放在图层的 `effects` 文件夹下，按子节点顺序组成效果栈（上面的先算，和 AE 一样）；`name` 是实例的显示名（AE 的 "Gaussian Blur"、"Gaussian Blur 2"……）。`effect.apply`（bae-motion）应用效果时把每个参数的默认值都写成 `param:NNNN`，所以参数和其他属性一样能直接打关键帧、加表达式、在时间轴里显示；其余效果命令：`effect.remove`、`effect.remove_all`、`effect.reorder`、`effect.set_enabled`、`effect.reset`、`effect.duplicate`。
- 渲染顺序（04 / 07，AE-CMP-038）：图层源 → 蒙版 → 效果（从上到下，跳过 `enabled = false` 的效果；图层 `effects_enabled = false` 时整个效果栈都不算）→ 变换 → 轨道遮罩 / 混合。调整图层（`adjustment_layer` 或 `kind = adjustment`）不画自己：它下面的图层先合成，结果经过它的效果，再按它自己像素的覆盖度（alpha、蒙版、轨道遮罩、不透明度）混回去；没有启用的效果时什么都不改。
- 参数值是 `param:NNNN` 属性，`NNNN` 是稳定的数字 ID，**直接等于 AE 参数 matchName 末尾的序号**（`ADBE Gaussian Blur 2-0001` → `param:0001`），导入导出不需要查表。参数和其他属性一样可以加关键帧、表达式、覆盖、协同。
- `EffectDef` / `EffectParamDef`（`crates/bae-model/src/effect.rs`）：参数种类是 AE 的超集，包括 `slider`（含整数滑块、有效范围、滑块范围、精度、显示单位）、`angle`、`checkbox`、`color`、`point`、`point3d`、`popup`（值是从 1 开始的序号，和 AE 一样）、`layer`、`group_start` / `group_end`、`arbitrary_data`、`path`（指向 Mask）、`button`。参数标志：`hidden`、`disabled`、`supervise`、`cannot_animate`、`cannot_time_vary`、`collapsed`、`custom_ui`。效果本身可以声明 `thread_safe`、`gpu`。
- `EffectRegistry`：内置效果和插件效果都注册到这里（07 和 19 使用），插件卸载时用 `unregister_plugin` 整体撤销。`status(prim)` 返回 `Known` 或 `MissingPlugin`（插件没装：界面显示占位，**所有参数和 `state` 原样保留**，重新装上插件或导回 AE 时不丢）。`check_param` 按定义检查参数值。应用里内置效果（`bae_effects::EffectsModule`）和插件效果都发布在 `bae_app::Registry<EffectDef>` / `Registry<dyn bae_render::Effect>`，id 是 `bae_render::effect::registry_id(match_name)`（注册表 id 不允许空白，AE matchName 里的空白写成 `%20`）；渲染器用 `EffectLibrary::sync` / `Renderer::sync_effects` 跟着注册表走，插件后加载也能用。

## 12. 还没做（后续 PR）

- 逐字符文字样式；形状操作器和文字动画器的完整属性；schema 的版本迁移。
- 引用列表跨层合并（目前取最强的一份）；`active` / 停用 Prim。
