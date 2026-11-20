# 剪辑数据模型（序列、轨道、片段、过渡、标记）

> 状态：设计提案（工作流 14 的路线图配套文档）。基线是 #66（14 PR A，`crates/bae-edit`，WIP）已经采用的模型；本文在它上面扩展，标出哪些必须在 0.0.1 框架里定下来（**M1 预留**），哪些以后再加。schema 的正式定义归 02（`crates/bae-model/src/schema.rs`、`docs/spec/data-model.md`），本文的改动已作为请求写进 02 任务书。
>
> 约定沿用 `docs/spec/data-model.md`：时间是整数 flicks，属性名用 `:` 分命名空间，单位与 Pr/AE 一致（位置用像素、缩放和不透明度用百分比、角度用度、音量用 dB）。

## 1. 原则

1. **剪辑数据就是普通的 Prim。** 序列、轨道、片段、过渡、标记都在同一个工程的 Prim 树里，和动效的合成、图层并列，享有同样的分层、协同、撤销、Agent 能力。没有单独的"剪辑工程"。
2. **命令参数自带上下文。** Pr 里插入、覆盖、粘贴、匹配帧的结果取决于轨道定位、源修补、播放头这些界面状态。BAE 的命令不读界面状态：目标轨道、源修补、时间点都是命令参数，由 `bae-ui-edit` 从当前界面状态填好再调用。这样 Agent 和脚本调用时结果确定、可复现。
3. **编辑点对齐帧。** 视频剪辑的编辑点（入点、出点、开始位置）吸附到序列帧边界（`FrameRate::frame_to_time`）。音频剪辑默认也按帧，打开"显示音频时间单位"后可以按采样编辑（flicks 能整除 44.1/48/96 kHz 的采样间隔）。
4. **在 #66 上扩展，不推翻。** #66 的 Prim 布局、属性名、命令 ID 保留；本文只补充和收紧语义。和 #66 不一致的只有一处：过渡从片段属性改成子 Prim（第 6 节，已采纳）。

## 2. Prim 布局

```
/                                   Project
  <任意文件夹>/                      Folder（= Pr 的素材箱 Bin，和 AE 的项目文件夹是同一种东西）
    <asset>                          Asset：素材、静帧、纯色、调整图层、生成器、子剪辑
      markers/<m>                    Marker：剪辑标记（源时间）
      effects/<fx>                   Effect：主剪辑效果（作用于所有使用它的片段）
    <comp>                           Comp：动效合成，可以直接作为片段来源
  sequences/                         #66 的默认位置；序列也可以放在任意文件夹里
    <seq>                            Sequence
      video, video_1, …              Track（track_kind = video），按子节点顺序依次是 V1、V2 …
        <clip>                       Clip
          effects/<fx>               Effect：片段效果，子节点顺序 = 渲染顺序
          masks/<m>                  Mask：不透明度蒙版
          transition_in              Transition：片段开头的过渡（第 6 节）
          transition_out             Transition：片段结尾的过渡
      audio, audio_1, …              Track（track_kind = audio），依次是 A1、A2 …；片段结构同上
      <caption track>                Track（track_kind = caption），以后
      <submix>                       Track（track_kind = submix），以后
      master                         Track（track_kind = master），主声道，M4（M1 预留类型）
      markers/<m>                    Marker：序列标记（序列时间）
      graphics/<g>                   Comp：这条序列里的文字、形状图形（第 5.5 节）
```

- **轨道编号来自位置**（#66 的做法）：Prim 名是 `video`、`video_1` 这类不带序号含义的名字，V1/V2 由子节点顺序决定。在中间插入轨道不需要给别的轨道改名，协同时也不会因改名冲突。视频轨在音频轨之前；V1 在画面最下层，编号大的盖在上面。
- **片段是轨道的子节点。** 片段换轨道 = `move_prim`；同一轨道里子节点顺序没有意义，按 `start` 排序。
- 显示名一律是 `name` 属性，不是 Prim 名（data-model.md 第 1 节）。

## 3. Sequence

| 属性 | 类型 | 默认 | 说明 | 状态 |
|---|---|---|---|---|
| `name` | string | | | 已有 |
| `frame_rate` | frame_rate | 24/1 | 时基 | 已有 |
| `width` / `height` | int | 1920 / 1080 | 帧大小 | 已有 |
| `mark_in` / `mark_out` | time | 无 | 序列入点 / 出点（出点不含） | #66，**M1 预留** |
| `pixel_aspect` | float | 1.0 | 像素长宽比 | **M1 预留** |
| `field_order` | enum `progressive \| upper \| lower` | progressive | 场序 | M4 |
| `audio_sample_rate` | int | 48000 | 音频采样率 | **M1 预留** |
| `audio:master_format` | enum `stereo \| 5.1 \| multichannel \| mono` | stereo | 主声道类型 | **M1 预留** |
| `audio:master_channels` | int | 2 | 多声道主轨的声道数 | M4 |
| `start_timecode` | time | 0 | 序列开始时间码（Pr 默认 00:00:00:00，广播常用 01:00:00:00） | **M1 预留** |
| `timecode_display` | enum `ndf \| df \| frames \| feet_16 \| feet_35` | 按帧率 | 时间显示格式（29.97/59.94 默认丢帧） | M4 |
| `color:working_space` | string | 空 = 跟工程 | 序列工作色彩空间（06 的色彩空间 id） | M4 |
| `render:max_bit_depth` / `render:max_quality` | bool | false | 最大位深 / 最高渲染品质 | M4 |
| `preview:format` | string | 空 | 预览文件格式（08 的编码预设 id） | M4 |
| `multicam` | bool | false | 多机位源序列（第 8 节） | 以后（M1 预留名字） |

序列预设（Pr 的 `.sqpreset`）在 BAE 里是一份属性值的集合，放在 `bae-edit` 的内置数据里，不是 Prim。

## 4. Track

| 属性 | 类型 | 默认 | 说明 | 状态 |
|---|---|---|---|---|
| `name` | string | 空 | 空时界面显示 V1/A1 | 已有 |
| `track_kind` | enum `video \| audio \| caption \| submix \| master` | video | 后三种是新增 | 已有（**M1 扩展枚举**） |
| `muted` | bool | false | 音频轨静音（M） | 已有 |
| `enabled` | bool | true | 视频轨"切换轨道输出"（眼睛） | #66 |
| `solo` | bool | false | 音频轨独奏（S） | #66 |
| `locked` | bool | false | 锁定：不能编辑，但照常播放 | #66 |
| `sync_lock` | bool | true | 同步锁定（Pr 新建轨道默认打开） | **M1 预留** |
| `audio:channel_format` | enum `standard \| mono \| stereo \| 5.1 \| adaptive` | standard | 音频轨类型；标准轨能放单声道和立体声剪辑 | **M1 预留** |
| `audio:level` | float（dB，可动画） | 0 | 轨道音量（音轨混合器推子，关键帧 = 轨道自动化） | M4 |
| `audio:pan` | float（−100…100，可动画） | 0 | 轨道声像 / 平衡 | M4 |
| `audio:output` | prim_ref | 空 = 主声道 | 输出到哪个子混合轨或主轨 | 以后 |
| `audio:automation_mode` | enum `off \| read \| latch \| touch \| write` | read | 音轨混合器自动化模式 | 以后 |
| `caption:format` | enum `subtitle \| cea608 \| cea708 \| teletext \| op42 \| op47` | subtitle | 字幕轨格式 | 以后 |

- 轨道下可以有 `effects/`（轨道效果，Effect 加 `pre_fader` 属性区分推子前/后）、`sends/`（发送），都在 M4 之后。
- **不进工程的状态**：轨道定位（target）、源修补（source patching）、轨道高度、展开状态、播放头、当前工具。它们是 `bae-ui-edit` 的视图状态（随工作区布局保存，见 `docs/spec/ui-shell.md` 第 2 节 `save_state`），命令通过参数接收（原则 2）。

## 5. Clip

### 5.1 位置与来源

| 属性 | 类型 | 默认 | 说明 | 状态 |
|---|---|---|---|---|
| `source` | prim_ref | | Asset、Comp（动效合成，实时引用）、Sequence（嵌套）、Panel（分镜格，动态分镜用，#68）。没有来源的片段只用于字幕项 | 已有 |
| `start` | time | 0 | 在序列中的开始时间 | 已有 |
| `duration` | time | 1 s | 在序列中的长度 | 已有 |
| `source_in` | time | 0 | 对应的源时间入点 | 已有 |
| `speed` | float（%，不可动画） | 100 | 恒定速度，必须 > 0 | 已有 |
| `reverse` | bool | false | 倒放 | **M1 预留** |
| `shot` | prim_ref | | 所属镜头 | 已有 |
| `name` | string | 来源名 | | #66 |
| `enabled` | bool | true | 启用 / 禁用（Shift+Cmd+E），禁用的片段不渲染不发声 | **M1 预留** |
| `label` | int | 跟来源 | 标签颜色（与 AE 的 16 色标签同一套） | M4 |
| `link` | string | 空 | 链接组 ID，相同的非空值互相链接 | #66 |
| `group` | string | 空 | 编组 ID（Cmd+G），与链接独立 | **M1 预留** |
| `source_channels` | int list | 全部 | 音频片段使用来源的哪几个声道（Pr 的"音频声道"映射） | **M1 预留** |

**源时间换算**（`bae-edit::query` 提供，渲染和播放共用）：

- 片段在序列中覆盖 `[start, start + duration)`；它用到的源区间是 `[source_in, source_in + duration × speed / 100)`。
- 正放：`源时间(t) = source_in + (t − start) × speed / 100`。
- 倒放：`源时间(t) = source_in + (start + duration − 一帧 − t) × speed / 100`（第一帧显示源区间的最后一帧）。
- 打开时间重映射时（5.3），先按上式得到"片段时间"，再经 `time:remap` 曲线得到源时间。
- 源时间超出来源长度的部分是"媒体不足"：视频显示最后一帧（静帧和生成器没有长度限制），界面用斜纹标出，和 Pr 一致。

### 5.2 运动、不透明度、音量（Pr 的固有效果）

Pr 每个视频片段都有"运动""不透明度""时间重映射"，音频片段都有"音量""声道音量""声像器"。BAE 把它们存成片段属性（和 AE 图层的 `transform:*` 同一套命名，渲染端可以把视频片段当成素材图层处理），不是 Effect Prim：

| 属性 | 类型 | 默认 | Pr 对应 | 状态 |
|---|---|---|---|---|
| `transform:position` | vec2（px，可动画） | 序列中心 | 运动 > 位置 | **M1 预留名字**，M4 实现 |
| `transform:scale` | vec2（%，可动画） | 100,100 | 运动 > 缩放 / 缩放宽度 | 同上 |
| `transform:uniform_scale` | bool | true | 运动 > 等比缩放 | 同上 |
| `transform:rotation` | float（度，可动画） | 0 | 运动 > 旋转 | 同上 |
| `transform:anchor_point` | vec2（px，可动画） | 来源中心 | 运动 > 锚点 | 同上 |
| `transform:anti_flicker` | float（可动画） | 0 | 运动 > 防闪烁滤镜 | M4 |
| `transform:opacity` | float（%，可动画） | 100 | 不透明度 > 不透明度 | 同上 |
| `blend_mode` | enum（同 Layer 的混合模式 token） | normal | 不透明度 > 混合模式 | M4 |
| `fit` | enum `none \| set_to_frame \| scale_to_frame` | 按偏好 | "设为帧大小" 只改缩放值；"缩放为帧大小" 改变片段的固有尺寸 | M4 |
| `audio:gain` | float（dB，不可动画） | 0 | 音频增益（G），在音量之前 | M4 |
| `audio:level` | float（dB，可动画） | 0 | 音量 > 级别 | **M1 预留名字**，M4 实现 |
| `audio:channel_levels` | float_list（dB，可动画） | 0… | 声道音量 | M4 |
| `audio:pan` | float（−100…100，可动画） | 0 | 声像器 > 平衡 | M4 |

- **渲染顺序与 Pr 一致：** 先按 `effects/` 的顺序做标准效果，最后做运动和不透明度（Pr 的固有效果在标准效果之后渲染，所以 Pr 用户需要"效果之后再变换"时会改用"变换"效果）。
- 不透明度蒙版是片段下的 `Mask` Prim，沿用 schema 里 Mask 的属性（`path`、`feather`、`opacity`、`expansion`、`inverted`）。Pr 的蒙版没有相加/相减模式，多个蒙版取并集；`mode` 固定写 `add`。蒙版跟踪（逐帧路径关键帧）M4 以后。

### 5.3 速度、时间重映射、帧定格

| 属性 | 类型 | 默认 | 说明 | 状态 |
|---|---|---|---|---|
| `frame_interp` | enum `frame_sampling \| frame_blend \| optical_flow` | frame_sampling | 时间插值 | M4 |
| `maintain_pitch` | bool | false | 变速时保持音调 | M4 |
| `time:remap_enabled` | bool | false | 时间重映射开关 | 以后（**M1 预留名字**） |
| `time:remap` | time（可动画） | | 片段时间 → 源时间（与 AE 时间重映射同一种表达）；Pr 界面上的"速度百分比 + 速度坡度"由 `bae-ui-edit` 换算显示 | 以后（**M1 预留名字**） |
| `frame_hold` | enum `none \| in \| out \| time` | none | 帧定格选项；`time` 时取 `frame_hold:time`（源时间） | M4 |

### 5.4 效果

- `effects/` 下的 `Effect` Prim 与 AE 图层效果完全同一种（`match_name`、`param:NNNN`、`state`，data-model.md 第 11 节），用同一个 `EffectRegistry`。07 为 AE 实现的效果直接可以用在剪辑里。
- Pr 里从 AE 移植来的效果 matchName 是 `AE.` 加 AE 的 matchName（例如 `AE.ADBE Gaussian Blur 2`）。导入 Pr 工程或 XML 时去掉 `AE.` 前缀就能对上 BAE 的 AE 效果；Pr 独有的效果（Lumetri、极致键、变形稳定器……）另建对照表（见 effects-transitions.md 附录）。
- 主剪辑效果放在来源（Asset）的 `effects/` 下，渲染时排在片段效果之前，所有使用这个来源的片段都受影响。

### 5.5 特殊片段

| 种类 | 来源 | 说明 |
|---|---|---|
| 调整图层 | Asset `kind = adjustment` | 对它下面所有轨道的合成结果做效果，和 AE 调整图层同语义 |
| 彩条和色调、黑场视频、透明视频、通用倒计时片头 | Asset `kind = generator`，`generator:type` 区分 | 由 `bae-effects` 的生成器渲染，没有长度限制 |
| 颜色遮罩 | Asset `kind = solid` | 复用纯色 |
| 文字和形状图形（Pr 的图形剪辑） | 序列 `graphics/` 下的 Comp | **BAE 的做法：图形剪辑就是一个内嵌的小动效合成。** 在剪辑工作区用文字工具（T）创建时，`bae-edit` 新建这个 Comp 并放一个片段引用它；基本图形面板编辑它的图层；需要复杂动画时直接在动效工作区打开同一个 Comp。Pr 要在 AE 里做 .mogrt 再导回来，BAE 不需要中间文件 |
| 嵌套序列 | Sequence | 不能直接或间接引用自己（命令检查，求值时最多 16 层，和 data-model.md 的引用深度一致） |
| 分镜格 | Panel | 动态分镜（13 的 `board.make_animatic`）生成的片段；画面取分镜格当前的图（已升级为动效的镜头取 Shot 的合成，`bae_storyboard::query::panel_picture`），长度不受限制，和静帧一样 |
| 多机位片段 | 多机位源序列 | 第 8 节 |
| 字幕项 | 无来源，在 caption 轨上 | `caption:text`、`caption:style`，以后 |

### 5.6 片段时间与关键帧时间基准（M1 必须定下来）

**提案：片段上属性（运动、不透明度、音量、效果参数、蒙版）的关键帧时间用"片段时间" τ = `source_in + (t − start) × speed / 100`，也就是未经时间重映射的源时间。**

- 这和 AE 图层属性用"图层时间"是同一个思路（data-model.md 第 10 节）：修剪片段入点时关键帧跟着画面内容不动；改变速度时关键帧跟着内容在时间轴上伸缩。这是 Pr 的行为（关键帧附着在媒体上）。外滑时关键帧随内容移动，这一点 Pr 的实际表现待核。
- 静帧、纯色、生成器没有源时间，τ = `source_in + (t − start)`，`source_in` 为 0。
- 序列标记用序列时间；剪辑标记用来源的源时间（存在来源上，所有实例共享）。
- 过渡参数不做关键帧，过渡进度由位置决定。

03 求值片段属性时按这个时间基准换算（已写进 03 的请求）。

## 6. Transition（新 Prim 类型，M1 预留）

#66 把过渡存成片段属性 `transition_in:kind` / `transition_in:duration`（以及 `transition_out:*`），只支持交叉溶解和黑场过渡。**已采纳（协调人，2026-11-20）：改成片段的子 Prim，米兰 14 恢复后在 #66（PR A）合并前改好**：

```
<clip>/transition_in    Transition
<clip>/transition_out   Transition
```

| 属性 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `match_name` | string | | 过渡种类，注册在 `EffectRegistry` 里（`EffectDef` 增加"过渡"种类，见 02、07 的请求）。内置：`bae.cross_dissolve`、`bae.dip_to_black`、`bae.dip_to_white`、`bae.film_dissolve`、`bae.additive_dissolve`、`bae.push`、`bae.slide`、`bae.wipe` …（M4） |
| `duration` | time | 序列偏好（Pr 默认 1 秒） | 过渡总长 |
| `alignment` | enum `center \| start \| end \| custom` | center | 中心切入 / 起点切入 / 终点切入 / 自定义 |
| `offset` | time | duration / 2 | `custom` 时过渡开始于切点之前多久 |
| `reverse` | bool | false | 反向 |
| `param:NNNN` | | | 过渡参数（边框宽度、颜色、方向……），和效果参数同一套 |
| `plugin:id` / `plugin:version` / `state` | | | 插件过渡，和 Effect 相同 |

**归属规则：** 两个相邻片段 A（前）和 B（后）之间的过渡只存一份，存在 **B 的 `transition_in`**。`transition_out` 只用于片段尾部接空白或序列结尾的单侧过渡（例如结尾黑场）。B 被移走、前面变成空白时，它的 `transition_in` 自然变成单侧过渡（从透明/静音渐入）。Pr 在拖走片段后过渡的具体去留待核，但这条规则保证数据不会重复、不会悬空。

**为什么改成子 Prim：** 过渡要带参数（推、擦除的方向，渐隐的颜色）和插件状态，属性前缀放不下；子 Prim 可以直接复用 Effect 的参数机制、插件缺失占位、协同按属性合并。#66 合并前就改，所以不保留属性写法。

**手柄不足：** 过渡需要 A 在切点之后、B 在切点之前有多余的源媒体。不够时 Pr 提示"媒体不足。此过渡将包含重复的帧"并用定格帧补，BAE 一样处理，求值结果里标出重复帧的区间，界面画斜纹。

## 7. Marker

沿用 schema 里的 `Marker`（`time`、`duration`、`comment`、`chapter`、`url`、`frame_target`、`cue_point_name`、`label`、`protected_region`），补充：

| 属性 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `name` | string | 空 | Pr 标记有名称和注释两个字段 |
| `marker:type` | enum `comment \| chapter \| segmentation \| web_link \| flash_cue` | comment | Pr 的标记类型 |

- `label` 用 Pr 的 8 种标记颜色（绿、红、紫、橙、黄、白、蓝、青；默认绿色），按序号存。
- 序列标记在 `/sequences/<seq>/markers/`，时间是序列时间；剪辑标记在来源的 `markers/` 下，时间是源时间。
- "波纹序列标记"（Pr 标记菜单里的开关）打开时，波纹类命令把受影响区间之后的序列标记一起移动。开关是用户偏好，命令参数 `ripple_markers` 显式传入（原则 2）。

## 8. 多机位（以后，M1 只预留名字）

- 多机位源序列 = `Sequence`，`multicam = true`；每个视频轨是一个机位，`name` 是机位名；音频按创建时的设置放在音频轨上。
- 序列里的多机位片段：`source` 指向多机位源序列，`multicam:angle`（int，机位序号，从 1 开始）、`multicam:audio_angle`（int 或空 = 跟随视频）。切换机位 = 在播放头处添加编辑后改 `multicam:angle`。
- "拼合"（Flatten）= 把每个多机位片段替换成对应机位轨上的原始片段。

## 9. 不同前端的预留（Pr / 达芬奇 / Final Cut Pro）

底层模型是 Pr 式的"轨道 + 绝对时间"。另外两种前端需要的差异只在行为层，模型只预留两个属性：

| 前端 | 差异 | 模型需要 |
|---|---|---|
| Premiere | 默认 | 无 |
| 达芬奇 | 布局、键位、修剪工具的交互不同，模型相同 | 无 |
| Final Cut Pro | 磁性时间线：主故事情节没有空隙（相当于 V1 上总是波纹编辑）；连接片段跟着主故事情节里的片段移动 | 片段属性 `anchor:clip`（prim_ref，连接到哪个主片段）、`anchor:offset`（time）——以后实现，名字先留 |

前端预设 = 工作区布局 + 键位表 + 工具行为参数（例如"拖动默认波纹"），注册方式见 14 任务书。

## 10. 序列求值接口（bae-edit，纯数据）

渲染、播放、缩略图、Agent 看图都要回答"序列在时刻 t 由什么组成"。这件事归 `bae-edit`（不依赖 GPU），渲染归 `bae-render`：

```rust
// bae-edit（示意，命名以实现为准）
pub struct SequenceFrame {
    pub video: Vec<VideoItem>,   // 从下到上（V1 在前），已排除禁用片段、关闭输出的轨道
    pub audio: Vec<AudioItem>,   // 已处理静音、独奏
}
pub struct VideoItem {
    pub clip: PrimPath,
    pub track_index: usize,
    pub source: ClipSource,       // Asset / Comp / Sequence（嵌套，递归求值）/ Panel / Generator / Adjustment
    pub source_time: Time,        // 第 5.1 节的换算结果
    pub clip_time: Time,          // 关键帧时间基准（5.6）
    pub repeated_frame: bool,     // 媒体不足、用定格帧补
    pub transition: Option<TransitionState>, // 在过渡区间内：种类、进度 0..1、另一侧的片段
}
pub fn frame_at(stage: &Stage, seq: &PrimPath, t: Time) -> SequenceFrame;
pub fn segments(stage: &Stage, seq: &PrimPath, range: TimeRange) -> Vec<Segment>; // 组成不变的区间，给播放调度和缓存键
pub fn used_source_ranges(stage: &Stage, seq: &PrimPath) -> Vec<(PrimPath, TimeRange)>; // 给 21 打包裁片
```

`bae-render` 把 `VideoItem` 转成 `RenderComp` 的图层（来源、变换、不透明度、混合模式、效果），过渡变成有两个输入的节点；`bae-media` 按 `AudioItem` 混音。#66 的 `query` 已经有"某时刻要叠的视频片段和能听到的音频片段"，扩展成上面的结构即可。

## 11. 命令约定

#66 的命令 ID 保留（`sequence.*`、`track.*`、`clip.*`、`marks.set`）。所有剪辑命令遵守：

- **显式目标：** 需要轨道定位的命令接收 `targets: { video: [track…], audio: [track…] }`；插入、覆盖接收源修补 `patch: { "V1": track, "A1": track, … }`；时间点（播放头）作为参数传入。
- **同步锁定：** 插入和波纹类命令按轨道的 `sync_lock` 处理没有被定位的轨道（在相同位置插入空白或一起移动），参数 `sync_lock: bool`（默认 true）可以整体关闭。
- **帧对齐：** 命令把输入时间吸附到帧，并在输出里返回实际使用的时间。
- **预演（dry run）：** 拖动修剪、拖动片段时界面需要实时预览而不产生撤销步骤；Agent 也需要"先看结果再执行"。每个命令提供纯函数 `plan_*`（输入参数 + Stage，输出将要写入的 Op 列表和受影响的时间范围），命令本身只是执行计划。另外请 02 提供通用的 `Stage::preview_transact`（执行后回滚、不发事件、不进撤销栈）作为兜底。
- **一个命令一个撤销单元：** 例如插入同时移动后面的片段、推开同步锁定的轨道，都在一个事务里。

## 12. 协同（M2）

- 两个人同时把片段放到同一条轨道的同一位置，CRDT 合并后会出现重叠。规则：读取时重叠部分以子节点顺序靠后的片段为准（相当于后放的覆盖先放的），界面给重叠的片段加冲突标记，`clip.resolve_overlaps` 命令按"覆盖"语义修复。数据永远不会损坏。
- 波纹类编辑一次改很多片段的 `start`，两人同时在同一条序列上做波纹编辑时结果可能不符合任何一方的预期。M2 默认用 ARCHITECTURE 第 7 节的"锁（提示）"：打开序列编辑时广播"我在剪这条序列"，其他人看到提示。Pr 的团队项目也是按序列加锁。
- 剪辑岗位默认写自己的工程层 `edit`（ARCHITECTURE 4.3），动效的层改合成内容，剪辑层改序列，互不覆盖。
- **入出点**（协调人定的默认方向，2026-11-20，M2 时由 14 和 09 确认）：序列的 `mark_in` / `mark_out` 是文档数据，参与同步；源监视器的入出点是每个用户的本地状态，不同步（存放位置 M2 定，例如 `session` 工程层或面板视图状态）。

## 13. M1 预留清单

0.0.1 框架里必须定下来、以后改动代价大的：

1. 第 2 节的 Prim 布局（含 `markers/`、`effects/`、`masks/`、`graphics/` 子文件夹名）。
2. Track 的 `track_kind` 枚举包含 `caption`、`submix`、`master`；`sync_lock`、`audio:channel_format`。
3. Clip 的 `reverse`、`enabled`、`group`、`source_channels`，以及固有效果、时间重映射的属性名（第 5.2、5.3 节，先只定名字）。
4. Sequence 的 `pixel_aspect`、`audio_sample_rate`、`audio:master_format`、`start_timecode`、`mark_in` / `mark_out`。
5. Transition 子 Prim 和归属规则（第 6 节）。
6. 片段时间与关键帧时间基准（第 5.6 节）。
7. 命令约定：显式目标、源修补、同步锁定参数、`plan_*` 预演（第 11 节）。
8. 序列求值接口的形状（第 10 节）。
9. Asset 的 `kind` 增加 `adjustment`、`generator`、`subclip`；代理文件属性（`proxy:file:*`，见 proxy-render-export.md）。源监视器入出点不进 schema（第 12 节）。
