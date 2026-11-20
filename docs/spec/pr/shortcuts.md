# Pr 清单：默认快捷键（PR-KEY）

## 范围与来源

Pr 26.2 默认键位（编辑模式），按用途分组，并给出对应的 BAE 命令或界面动作。**macOS 键位全部取自本机 `Keyboard Shortcuts/en/Adobe Premiere Pro Defaults V6.kys`**（26.x 的默认键位文件，分"编辑""颜色"两种模式；同目录下的 `Adobe Premiere Pro Defaults.kys` 是旧版格式）。写法与 `docs/spec/ui-shell.md` 第 4 节一致：`Cmd` = ⌘、`Opt` = ⌥、`Ctrl` = macOS 的 Control 键。

**Windows：** 一般规则是 `Cmd` → `Ctrl`、`Opt` → `Alt`，macOS 的 `Ctrl` 在 Windows 上仍是 `Ctrl`。已知例外：微移剪辑（macOS `Cmd+←/→`，Windows `Alt+←/→`）、外滑（macOS `Opt+Cmd+←/→`，Windows `Ctrl+Alt+←/→`）、波纹删除（macOS `Shift+向前删除`，Windows `Shift+Delete`）。本机没有 Windows 版 Pr，其余 Windows 键位未逐项核对（待核）。这些例外要求 11 的键位表支持"按平台覆盖某条绑定"（见 11 的请求）。

**BAE 命令列：** `clip.*`、`track.*`、`marks.set`、`sequence.*` 是 #66 已有或计划中的 `bae-edit` 命令（改工程数据，Agent 可用）；`edit.*` 是 `bae-ui-edit` 的界面动作（播放、缩放、工具切换这类只影响视图的操作，注册为 `ActionInfo`，ui-shell.md 第 1 节）。标"拟"的是本清单建议的新 ID。

### PR-KEY-001 · 剪辑工作区键位表（Premiere Keymap）

- **行为要点：** Pr 的快捷键按面板分作用域：全局、时间轴、节目监视器、项目、效果控件、音轨混合器、图形、文本等；同一个键在获得焦点的面板里可以有不同含义（例如 Delete 在时间轴里是清除、在项目面板里是删除项目项）。键盘快捷键编辑器（Opt+Cmd+K）用键盘图显示占用情况，可以另存为自定义键位，Pr 还自带"Avid Media Composer"和"Final Cut Pro 7"两套键位预设。
- **优先级：** M1 — 剪辑工作区注册键位表 `pr`，骨架阶段先放 M1 用到的键。
- **BAE 模块：** bae-ui-edit (+ bae-ui)
- **数据模型：** 无。键位表注册到 `Registry<Keymap>`，面板作用域用 `Binding.when`（ui-shell.md 第 4 节）。
- **备注：** 达芬奇、Final Cut Pro 风格前端各自另加键位表（`resolve`、`fcp`），对应同一批命令（edit-model.md 第 9 节）。Pr 自带的 Avid / FCP7 预设不必照搬。

### PR-KEY-002 · 文件与项目（File / Project）

- **行为要点：** 新建、打开、保存、导入、导出类快捷键，见下表。
- **优先级：** M4（Cmd+S、Cmd+I 等通用键由外壳提供，M1 已有）
- **BAE 模块：** bae-ui (+ bae-ui-edit)
- **数据模型：** 无
- **备注：** Cmd+N 在剪辑工作区是"新建序列"，在动效工作区是"新建合成"——两个工作区的键位表各自绑定，含义对应。

| 功能 | macOS | BAE 命令 / 动作 |
|---|---|---|
| 新建序列 | Cmd+N | `sequence.create` |
| 新建项目 | Opt+Cmd+N | `file.new` |
| 打开项目 | Cmd+O | `file.open` |
| 保存 / 另存为 / 保存副本 | Cmd+S / Shift+Cmd+S / Opt+Cmd+S | `file.save` / `file.save_as` / `file.save_copy` |
| 导入 | Cmd+I | `file.import` |
| 从媒体浏览器导入 | Opt+Cmd+I | `edit.import_from_browser`（拟） |
| 新建素材箱 | Cmd+/（项目面板内 Cmd+B） | `prim.create`（Folder） |
| 用所选项新建素材箱 | Shift+B（项目面板） | `edit.bin_from_selection`（拟） |
| 导出媒体 | Cmd+M | `sequence.export`（拟） |
| 发送至 Media Encoder 队列 | Opt+Shift+M | `sequence.queue_export`（拟） |
| 导出帧 | Shift+E | `edit.export_frame`（拟） |
| 关闭面板 / 关闭项目 / 退出 | Cmd+W / Shift+Cmd+W / Cmd+Q | `window.close_panel` / `file.close` / `app.quit` |
| 属性（所选项） | Shift+Cmd+H | `edit.properties`（拟） |
| 首选项 / 键盘快捷键 | Cmd+, / Opt+Cmd+K | `app.preferences` / `app.keyboard_shortcuts` |

### PR-KEY-003 · 编辑（Edit）

- **行为要点：** 撤销、剪贴板、删除、编组链接、子剪辑、速度、音频增益等。
- **优先级：** M1（撤销、剪贴板、删除、波纹删除、链接）；其余 M4
- **BAE 模块：** bae-ui-edit (+ bae-edit)
- **数据模型：** 无
- **备注：** Delete 在全局是"向前删除"键，时间轴作用域里 Backspace（Mac 的 Delete）也是清除，Opt+Backspace 是波纹删除。

| 功能 | macOS | BAE 命令 / 动作 |
|---|---|---|
| 撤销 / 重做 | Cmd+Z / Shift+Cmd+Z | `edit.undo` / `edit.redo`（外壳） |
| 剪切 / 复制 / 粘贴 | Cmd+X / Cmd+C / Cmd+V | `edit.cut` / `edit.copy` / `edit.paste` |
| 粘贴插入 | Shift+Cmd+V | `clip.paste`（`insert: true`，拟） |
| 粘贴属性 | Opt+Cmd+V | `clip.paste_attributes`（拟） |
| 清除 | 向前删除；时间轴内 Delete | `clip.lift` |
| 波纹删除 | Shift+向前删除；时间轴内 Opt+Delete | `clip.ripple_delete` |
| 全选 / 取消全选 | Cmd+A / Shift+Cmd+A | `edit.select_all` / `edit.deselect_all` |
| 复制（副本） | Shift+Cmd+/ | `prim.duplicate`（拟） |
| 查找 | Cmd+F | `edit.find`（拟） |
| 编辑原始 | Cmd+E | `edit.open_source`（拟；动效合成在动效工作区打开，PR-XCH-008） |
| 编组 / 取消编组 | Cmd+G / Shift+Cmd+G | `clip.group` / `clip.ungroup`（拟） |
| 链接 / 取消链接 | Cmd+L | `clip.link` / `clip.unlink` |
| 启用 | Shift+Cmd+E | `clip.set_enabled`（拟） |
| 制作子剪辑 | Cmd+U | `asset.make_subclip`（拟） |
| 速度/持续时间 | Cmd+R | `clip.set_speed`（拟） |
| 音频增益 | G | `clip.set_gain`（拟） |
| 音频声道 | Shift+G | `asset.set_audio_channels`（拟） |
| 剪辑音量 −1 dB / +1 dB | [ / ] | `clip.adjust_volume`（拟） |
| 剪辑音量大幅降低 / 提高 | Shift+[ / Shift+] | 同上 |

### PR-KEY-004 · 工具（Tools）

- **行为要点：** 单键切换工具，见下表。按住工具键（例如按住 C）临时切换、松开回到原工具（待核 Pr 是否支持"弹簧键"）。
- **优先级：** M1（V、C、B、N、Y、U）；其余 M4
- **BAE 模块：** bae-ui-edit
- **数据模型：** 无
- **备注：** 工具按钮在 Pr 26 的工具面板里成组（例如波纹/滚动/比率拉伸一组，外滑/内滑一组），长按展开。

| 工具 | macOS | BAE 动作 |
|---|---|---|
| 选择工具 | V | `edit.tool`（`selection`） |
| 向前选择轨道工具 / 向后 | A / Shift+A | `edit.tool`（`track_select_forward` / `_backward`） |
| 波纹编辑工具 | B | `edit.tool`（`ripple`） |
| 滚动编辑工具 | N | `edit.tool`（`roll`） |
| 比率拉伸工具 | R | `edit.tool`（`rate_stretch`） |
| 剃刀工具 | C | `edit.tool`（`razor`） |
| 外滑工具 | Y | `edit.tool`（`slip`） |
| 内滑工具 | U | `edit.tool`（`slide`） |
| 钢笔工具 | P | `edit.tool`（`pen`） |
| 手形工具 | H | `edit.tool`（`hand`） |
| 缩放工具 | Z | `edit.tool`（`zoom`） |
| 文字工具 | T | `edit.tool`（`type`） |
| 范围选择工具（26.x，待核） | Opt+Cmd+G | `edit.tool`（`range_select`） |
| 对象选择工具（26.x，待核） | Opt+Cmd+O | 以后 |

### PR-KEY-005 · 播放与导航（Playback / Navigation）

- **行为要点：** 播放、J/K/L 穿梭、逐帧、跳转编辑点。
- **优先级：** M1（空格、J/K/L、←/→、Home/End、↑/↓）；其余 M4
- **BAE 模块：** bae-ui-edit (+ bae-media)
- **数据模型：** 无
- **备注：** 都是界面动作（不改工程），作用于获得焦点的监视器或时间轴。

| 功能 | macOS | BAE 动作 |
|---|---|---|
| 播放/停止 | 空格 | `edit.play_toggle` |
| 向左往复 / 停止 / 向右往复 | J / K / L | `edit.shuttle`（`-1` / `0` / `+1`，重复按加速） |
| 向左慢速往复 / 向右慢速往复 | Shift+J / Shift+L | `edit.shuttle_slow` |
| 播放邻近区域 | Shift+K | `edit.play_around` |
| 从入点播放到出点 | Opt+K | `edit.play_in_to_out` |
| 通过预卷/过卷从入点播放到出点 | Shift+空格 | `edit.play_in_to_out_preroll` |
| 从播放指示器播放到出点 | Ctrl+空格 | `edit.play_to_out` |
| 后退一帧 / 前进一帧 | ← / → | `edit.step`（`-1` / `+1`） |
| 后退五帧 / 前进五帧 | Shift+← / Shift+→ | `edit.step`（`-5` / `+5`） |
| 转到序列开头 / 结尾 | Home / End | `edit.go_to_start` / `edit.go_to_end` |
| 转到所选剪辑开头 / 结尾 | Shift+Home / Shift+End | `edit.go_to_clip_start` / `_end` |
| 转到上一个 / 下一个编辑点（目标轨道） | ↑ / ↓ | `edit.go_to_edit`（`prev` / `next`） |
| 转到上一个 / 下一个编辑点（所有轨道） | Shift+↑ / Shift+↓ | `edit.go_to_edit`（`all_tracks: true`） |
| 选择上一个 / 下一个剪辑 | Cmd+↑ / Cmd+↓ | `edit.select_adjacent_clip` |
| 选择播放指示器处的剪辑 | D | `edit.select_clip_at_playhead` |
| 音频擦洗开关 | Shift+S | `edit.toggle_audio_scrub` |

### PR-KEY-006 · 入出点与源编辑（Marks / Source Editing）

- **行为要点：** 入出点、标记剪辑、插入、覆盖、提升、提取、匹配帧。
- **优先级：** M1（I、O、Opt+X、,、.、;、'）；其余 M4
- **BAE 模块：** bae-ui-edit (+ bae-edit)
- **数据模型：** 无
- **备注：** `,` `.` `;` `'` 四个键是三点编辑的核心，M1 骨架必须可用。

| 功能 | macOS | BAE 命令 / 动作 |
|---|---|---|
| 标记入点 / 出点 | I / O | `marks.set` |
| 转到入点 / 出点 | Shift+I / Shift+O | `edit.go_to_in` / `edit.go_to_out` |
| 清除入点 / 出点 / 入点和出点 | Opt+I / Opt+O / Opt+X | `marks.set`（清除） |
| 标记剪辑 | X | `marks.set_around_clip`（拟） |
| 标记选择项 | / | `marks.set_around_selection`（拟） |
| 插入 | ,（逗号） | `clip.insert` |
| 覆盖 | .（句号） | `clip.overwrite` |
| 提升 | ;（分号） | `clip.lift`（按入出点范围） |
| 提取 | '（单引号） | `clip.extract`（拟，按范围的波纹删除） |
| 匹配帧 | F | `edit.match_frame` |
| 反转匹配帧 | Shift+R | `edit.reverse_match_frame` |
| 制作子序列 | Shift+U | `sequence.make_subsequence`（拟） |
| 设置工作区栏入点 / 出点（旧功能） | Opt+[ / Opt+] | 不做 |

### PR-KEY-007 · 时间轴编辑与修剪（Timeline Editing / Trimming）

- **行为要点：** 添加编辑、修剪到播放头、键盘修剪、微移、外滑、内滑、过渡、对齐、轨道定位。
- **优先级：** M1（Cmd+K、S、Cmd+←/→、Opt+↑/↓、Cmd+D）；其余 M4
- **BAE 模块：** bae-ui-edit (+ bae-edit)
- **数据模型：** 无
- **备注：** 微移、外滑的 Windows 键位与 macOS 修饰键不对应，见本文开头。

| 功能 | macOS | BAE 命令 / 动作 |
|---|---|---|
| 添加编辑（目标轨道） / 添加编辑到所有轨道 | Cmd+K / Shift+Cmd+K | `clip.razor` |
| 波纹修剪上一个编辑点到播放指示器 / 下一个 | Q / W | `clip.ripple_trim`（到播放头，拟参数） |
| 修剪入点 / 出点到播放指示器（不合拢） | Opt+Q / Opt+W | `clip.trim`（到播放头） |
| 将所选编辑点扩展到播放指示器 | E | `clip.extend_to_playhead`（拟） |
| 将上一个 / 下一个编辑点扩展到播放指示器 | Shift+Q / Shift+W | 同上 |
| 向后 / 向前修剪 1 帧 | Opt+← / Opt+→ | `clip.trim` / `clip.ripple_trim` / `clip.roll`（按编辑点类型） |
| 向后 / 向前修剪多帧 | Opt+Shift+← / Opt+Shift+→ | 同上（大修剪偏移） |
| 切换修剪类型 | Ctrl+T | `edit.cycle_trim_type` |
| 修剪编辑（进入修剪模式） | Shift+T（键位文件里绑定的是内部名 `cmd.placeholder`，待核） | `edit.trim_mode` |
| 将剪辑左移 / 右移 1 帧 | Cmd+← / Cmd+→ | `clip.move` |
| 将剪辑左移 / 右移 5 帧 | Shift+Cmd+← / Shift+Cmd+→ | `clip.move` |
| 将剪辑上移 / 下移一条轨道 | Opt+↑ / Opt+↓ | `clip.move` |
| 外滑 1 帧 / 多帧 | Opt+Cmd+← / → ；Opt+Shift+Cmd+← / → | `clip.slip` |
| 内滑 1 帧 / 多帧 | Opt+, / Opt+. ；Opt+Shift+, / Opt+Shift+. | `clip.slide` |
| 应用默认视频过渡 / 音频过渡 / 音视频过渡 | Cmd+D / Shift+Cmd+D / Shift+D | `clip.set_transition` |
| 时间轴对齐开关 | S | `edit.toggle_snap` |
| 转到下一个 / 上一个间隙 | Shift+; / Opt+; | `edit.go_to_gap` |
| 切换所有视频 / 音频轨道定位 | Cmd+0 / Cmd+9 | `edit.toggle_all_targets` |
| 切换所有源视频 / 源音频 | Opt+Cmd+0 / Opt+Cmd+9 | `edit.toggle_all_source_patches` |
| 在嵌套序列中显示（待核名称） | Opt+Cmd+F | `edit.reveal_nested` |
| 时间轴内粘贴到同一轨道 / 粘贴插入到同一轨道 | Cmd+V / Shift+Cmd+V（时间轴作用域） | `clip.paste` |

### PR-KEY-008 · 时间轴视图（Timeline View）

- **行为要点：** 缩放、轨道高度、翻页。
- **优先级：** M1（=、-、\）；其余 M4
- **BAE 模块：** bae-ui-edit
- **数据模型：** 无
- **备注：** 无

| 功能 | macOS | BAE 动作 |
|---|---|---|
| 放大 / 缩小 | = / - | `edit.zoom_in` / `edit.zoom_out` |
| 缩放到序列 | \ | `edit.zoom_to_sequence` |
| 增加 / 减小视频轨道高度 | Cmd+= / Cmd+-（时间轴） | `edit.track_height`（`video`） |
| 增加 / 减小音频轨道高度 | Opt+= / Opt+-（时间轴） | `edit.track_height`（`audio`） |
| 展开所有轨道 / 最小化所有轨道 | Shift+= / Shift+- | `edit.expand_all_tracks` / `edit.minimize_all_tracks` |
| 显示上一屏 / 下一屏 | PageUp / PageDown（时间轴） | `edit.page`（`-1` / `+1`） |

### PR-KEY-009 · 标记（Markers）

- **行为要点：** 添加、导航、清除标记，范围标记。
- **优先级：** M1（M）；其余 M4
- **BAE 模块：** bae-ui-edit (+ bae-edit)
- **数据模型：** 无
- **备注：** 在已有标记处再按 M 打开标记对话框。

| 功能 | macOS | BAE 命令 / 动作 |
|---|---|---|
| 添加标记 | M | `sequence.add_marker` / 剪辑标记 `asset.add_marker`（拟） |
| 转到下一个 / 上一个标记 | Shift+M / Shift+Cmd+M | `edit.go_to_marker` |
| 清除当前标记 / 所有标记 | Opt+M / Opt+Cmd+M | `marker.remove`（拟） |
| 按入点和出点添加范围标记 | Ctrl+M | `sequence.add_marker`（带 `duration`） |
| 添加范围标记（待核） | Ctrl+Shift+M | 同上 |

### PR-KEY-010 · 多机位（Multi-Camera）

- **行为要点：** 实时切换、录制开关、多机位视图。
- **优先级：** 以后
- **BAE 模块：** bae-ui-edit (+ bae-edit)
- **数据模型：** 无
- **备注：** 数字键 1–9 在多机位视图之外的含义（Pr 默认没有其他绑定）。

| 功能 | macOS | BAE 命令 / 动作 |
|---|---|---|
| 选择摄像机 1–9（实时切换，不剪切时只切预览） | 1–9 | `clip.set_multicam_angle`（拟） |
| 切换当前剪辑为摄像机 1–9 | Ctrl+1–9 | 同上 |
| 多机位录制开关 | 0 | `edit.toggle_multicam_record` |
| 切换多机位视图 | Shift+0 | `edit.toggle_multicam_view` |

### PR-KEY-011 · 面板、工作区与模式（Panels / Workspaces / Modes）

- **行为要点：** 聚焦面板、最大化、切换工作区和模式。
- **优先级：** M4（`~` 最大化外壳已有）
- **BAE 模块：** bae-ui (+ bae-ui-edit)
- **数据模型：** 无
- **备注：** Opt+1–4 切换 Pr 的四个模式；BAE 用工作区栏切岗位工作区，剪辑工作区内的子布局可以复用 Opt+Shift+1–9。

| 功能 | macOS | BAE 动作 |
|---|---|---|
| 项目 / 源监视器 / 时间轴 / 节目监视器 | Shift+1 / Shift+2 / Shift+3 / Shift+4 | `window.panel` |
| 效果控件 / 音频混合器 / 效果 / 媒体浏览器 / 音频剪辑混合器 | Shift+5 / Shift+6 / Shift+7 / Shift+8 / Shift+9 | `window.panel` |
| 最大化或恢复当前面板组 / 光标下的面板组 | ` / Shift+` | `window.maximize_frame` |
| 全屏显示监视器 | Ctrl+` | `edit.fullscreen_monitor`（拟） |
| 导入 / 编辑 / 颜色 / 导出模式 | Opt+1 / Opt+2 / Opt+3 / Opt+4 | `workspace.switch`（子布局） |
| 用户工作区 1–9 / 重置为已保存的布局 | Opt+Shift+1–9 / Opt+Shift+0 | `workspace.switch` / `workspace.reset` |
| 下一个 / 上一个面板 | Ctrl+Shift+. / Ctrl+Shift+, | `window.next_panel`（拟） |

### PR-KEY-012 · 监视器、图形与字幕（Monitor / Graphics / Captions）

- **行为要点：** 监视器缩放、标尺参考线，图形图层操作和文字对齐，字幕轨。
- **优先级：** M4（监视器缩放）；以后（图形、字幕）
- **BAE 模块：** bae-ui-edit (+ bae-motion)
- **数据模型：** 无
- **备注：** 图形图层顺序的 Cmd+[ / ] 与 AE 调整图层顺序的 Cmd+[ / ] 含义一致。

| 功能 | macOS | BAE 命令 / 动作 |
|---|---|---|
| 节目监视器缩放到适合 / 100% | Shift+Cmd+0 / Shift+Cmd+1 | `edit.monitor_zoom` |
| 源监视器缩放到适合 / 100% | Opt+Shift+Cmd+0 / Opt+Shift+Cmd+1 | `edit.monitor_zoom` |
| 显示标尺 / 参考线 / 对齐参考线 | Cmd+R / Cmd+; / Shift+Cmd+;（节目监视器作用域） | `edit.toggle_rulers` / `_guides` / `_snap_guides` |
| 新建文本图层 / 矩形 / 椭圆 | Cmd+T / Opt+Cmd+R / Opt+Cmd+E | `edit.add_graphic_layer`（拟） |
| 进入文本编辑 | Opt+Cmd+' | `edit.enter_text_edit` |
| 图层下移 / 上移 / 移到最底 / 移到最顶 | Cmd+[ / Cmd+] / Shift+Cmd+[ / Shift+Cmd+] | `layer.reorder`（12 的命令） |
| 选择下一个 / 上一个图层 | Opt+Cmd+] / Opt+Cmd+[ | `edit.select_layer` |
| 文本左对齐 / 居中 / 右对齐 | Shift+Cmd+L / Shift+Cmd+C / Shift+Cmd+R | 文字属性命令（05/12） |
| 添加字幕轨 / 添加字幕段 | Opt+Cmd+A / Opt+Cmd+C | `track.add`（caption） / `caption.add`（拟） |
| 转到上一个 / 下一个字幕段 | Opt+Cmd+↑ / Opt+Cmd+↓（时间轴） | `edit.go_to_caption` |

### PR-KEY-013 · 项目面板（Project Panel）

- **行为要点：** 项目面板作用域里的键：打开、视图切换、缩略图大小、悬停擦洗、删除。
- **优先级：** M4
- **BAE 模块：** bae-ui-edit
- **数据模型：** 无
- **备注：** 与动效工作区共用项目面板时，这些面板作用域的键需要两个键位表都包含或放在共享的绑定里。

| 功能 | macOS | BAE 命令 / 动作 |
|---|---|---|
| 在源监视器中打开 | Shift+O | `edit.open_in_source` |
| 悬停擦洗开关 | Shift+H | `edit.toggle_hover_scrub` |
| 缩略图变大 / 变小 | Shift+] / Shift+[ | `edit.thumbnail_size` |
| 列表视图 / 图标视图 | Cmd+PageUp / Cmd+PageDown | `edit.bin_view` |
| 切换视图 | Shift+\ | `edit.bin_view` |
| 删除 / 带选项删除 | Delete / Cmd+向前删除 | `prim.delete` |
| 放大 / 缩小 | = / - | `edit.bin_zoom` |
| 添加到收藏夹（待核含义） | Opt+Shift+A | 以后 |

### PR-KEY-014 · 与 AE 键位的冲突（Conflicts with AE Keymap）

- **行为要点：** 同一个键在 Pr 和 AE 里含义不同的常见例子见下表。BAE 按工作区切换键位表（11 已实现），剪辑工作区用 Pr 键位、动效工作区用 AE 键位，所以两边都能保持原有习惯；需要注意的是在两个工作区之间切换时，用户不会因为焦点还停在共用面板上而触发另一套含义。
- **优先级：** M1 — 键位表按工作区切换的机制已有，剪辑工作区注册自己的 `pr` 键位表即可。
- **BAE 模块：** bae-ui (+ bae-ui-edit)
- **数据模型：** 无
- **备注：** 共用面板（项目面板、效果控件）的面板作用域绑定，应该随当前工作区的键位表变化，而不是随面板固定。

| 键 | Pr（剪辑工作区） | AE（动效工作区） |
|---|---|---|
| J / K | 向左往复 / 停止 | 上一个 / 下一个可见项（关键帧、标记） |
| L | 向右往复 | 显示音频级别（LL 显示波形） |
| U | 内滑工具 | 显示有关键帧的属性 |
| I / O | 标记入点 / 出点 | 转到图层入点 / 出点 |
| Cmd+K | 添加编辑 | 合成设置 |
| Cmd+D | 应用默认视频过渡 | 复制图层 |
| Shift+Cmd+D | 应用默认音频过渡 | 拆分图层 |
| Cmd+M | 导出媒体 | 添加到渲染队列 |
| Cmd+R | 速度/持续时间 | 显示标尺 |
| Cmd+L | 链接 / 取消链接 | 锁定图层 |
| T | 文字工具 | 显示不透明度属性 |
| C | 剃刀工具 | 摄像机工具 |
| ,（逗号） / .（句号） | 插入 / 覆盖 | 缩小 / 放大（合成查看器） |
