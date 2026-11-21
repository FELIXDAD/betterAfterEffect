# After Effects macOS 默认键盘快捷键与 Windows 差异对照

> 来源：Adobe After Effects 26.2.1（build `26.2.1.2`）| 平台：macOS 26.7.3 (25G342, arm64) | 提取日期：2026-11-21
> 对照基准：Adobe After Effects 26.0x67 | 平台：Windows 11 Pro (10.0.26200, x64)

完整 32 个上下文、674 条命令的解析结果见 [`shortcuts.json`](shortcuts.json)（与 Windows 版 [`../shortcuts.json`](../shortcuts.json) 保持相同顶层结构，并附加 `windowsDiff` 机器可读差异表）。

## 1. 总体统计

| 指标 | 数值 | 说明 |
|---|---:|---|
| 上下文数（Mac / Win） | 32 / 32 | 32 个上下文名称完全一致 |
| 命令总数（Mac / Win） | 674 / 669 | Mac 26.2.1 比 Win 26.0x67 多 5 条命令，少 0 条 |
| 简单换算完全一致（`Ctrl→Cmd`、`Alt→Option`） | 643 | 占双方共有 669 条命令的 96.1% |
| **非简单换算例外** | **26** | 见第 2 节逐条清单 |
| 仅在 Mac `aeks` 出现的命令 | 5 | 见第 4 节（含 26.2 vs 26.0 版本差异说明） |
| 含 `macControl`（`⌃`）的有效 Mac 绑定 | 34 | 其中 19 条在 Windows `aeks` 中也有同名字符串但 Windows 无 `macControl` 键，仅在 Mac 生效（见第 3 节） |

## 2. 非简单 `Ctrl→Cmd` / `Alt→Option` 换算的全部例外（26 条）

以下 26 条命令不能通过将 Windows 绑定中的 `Ctrl` 替换为 `Cmd`、`Alt` 替换为 `Option` 得到，开发快捷键系统时必须按平台单独处理。

### 2.1 Mac 独占 `Option+<属性键>` 直接添加关键帧（8 条）

Windows 因 `Alt+字母` 会触发菜单栏助记符，这 8 条命令在 Windows 默认留空（需用 `Alt+Shift+<键>` 的 `*AddStateNewKF`）；macOS 无 `Alt` 菜单助记符，直接绑定为 `Option+<键>`（同时保留 `Option+Shift+<键>`）。

| 上下文 | 命令标识 | Windows 默认 | 简单换算预期 | macOS 实际默认 |
|---|---|---|---|---|
| `CCompCompCmd` | `CompToggleMaskFeathersNewKF` | *(无)* | *(无)* | `Option+F` |
| `CCompCompCmd` | `CompToggleMaskShapesNewKF` | *(无)* | *(无)* | `Option+M` |
| `CCompCompCmd` | `CompTwirlAnchorPointNewKF` | *(无)* | *(无)* | `Option+A` |
| `CCompCompCmd` | `CompTwirlAudioNewKF` | *(无)* | *(无)* | `Option+L` |
| `CCompCompCmd` | `CompTwirlOpacityNewKF` | *(无)* | *(无)* | `Option+T` |
| `CCompCompCmd` | `CompTwirlPositionNewKF` | *(无)* | *(无)* | `Option+P` |
| `CCompCompCmd` | `CompTwirlRotationNewKF` | *(无)* | *(无)* | `Option+R` |
| `CCompCompCmd` | `CompTwirlScaleNewKF` | *(无)* | *(无)* | `Option+S` |

### 2.2 `Ctrl` 映射为物理 `macControl`（`⌃`）而非 `Cmd`（`⌘`）（10 条）

为避开 macOS 系统快捷键（`` Cmd+` `` 切换同应用窗口）以及视图缩放冲突，以下命令在 Mac 上把 Windows 的 `Ctrl` 改映射为 `macControl`（Control 键）。注意：`CTopic.TwirlExplode` 在 `aeks` 中仍保留 `` Cmd+` ``，而时间轴上下文 `CCompCompCmd` 覆盖为 `` macControl+` ``。

| 上下文 | 命令标识 | Windows 默认 | 简单换算预期 | macOS 实际默认 |
|---|---|---|---|---|
| `CCompCompCmd` | `NudgeOpacityDown` | `Ctrl+Alt+PadMinus` | `Cmd+Option+PadMinus` | `macControl+Option+PadMinus` |
| `CCompCompCmd` | `NudgeOpacityDownMore` | `Ctrl+Alt+Shift+PadMinus` | `Cmd+Option+Shift+PadMinus` | `macControl+Option+Shift+PadMinus` |
| `CCompCompCmd` | `NudgeOpacityUp` | `Ctrl+Alt+PadPlus` | `Cmd+Option+PadPlus` | `macControl+Option+PadPlus` |
| `CCompCompCmd` | `NudgeOpacityUpMore` | `Ctrl+Alt+Shift+PadPlus` | `Cmd+Option+Shift+PadPlus` | `macControl+Option+Shift+PadPlus` |
| `CCompCompCmd` | `TwirlExplode` | `` Ctrl+` `` | `` Cmd+` `` | `` macControl+` `` |
| `CCompCompCmd` | `TwirlPreserveSoloExplode` | `` Ctrl+Shift+` `` | `` Cmd+Shift+` `` | `` macControl+Shift+` `` |
| `CPanoProjLayerPano` | `NudgeOpacityDown` | `Ctrl+Alt+PadMinus` | `Cmd+Option+PadMinus` | `macControl+Option+PadMinus` |
| `CPanoProjLayerPano` | `NudgeOpacityDownMore` | `Ctrl+Alt+Shift+PadMinus` | `Cmd+Option+Shift+PadMinus` | `macControl+Option+Shift+PadMinus` |
| `CPanoProjLayerPano` | `NudgeOpacityUp` | `Ctrl+Alt+PadPlus` | `Cmd+Option+PadPlus` | `macControl+Option+PadPlus` |
| `CPanoProjLayerPano` | `NudgeOpacityUpMore` | `Ctrl+Alt+Shift+PadPlus` | `Cmd+Option+Shift+PadPlus` | `macControl+Option+Shift+PadPlus` |

### 2.3 避开 macOS 系统注销快捷键而追加 `macControl`（`⌃`）（2 条）

macOS 将 `Cmd+Shift+Q` 和 `Cmd+Option+Shift+Q` 保留为退出登录，AE 在 Mac 上追加了 `macControl` 修饰键。

| 上下文 | 命令标识 | Windows 默认 | 简单换算预期 | macOS 实际默认 |
|---|---|---|---|---|
| `CSwitchboard` | `ScanForChangedFootage` | `Ctrl+Alt+Shift+Q` | `Cmd+Option+Shift+Q` | `Cmd+macControl+Option+Shift+Q` |
| `TextLayerUI` | `TextResetTracking` | `Shift+Ctrl+Q` | `Shift+Cmd+Q` | `macControl+Shift+Cmd+Q` |

### 2.4 macOS 系统菜单快捷键（6 条，受偏好开关控制）

Windows 上这 6 条全部为空；macOS 上由 `Main Pref Section v2` 中的 `Pref_USE_STD_SYSTEM_SHORTCUTS_v2`（界面项「使用系统快捷键」/ *Use System Shortcut Keys*）控制启用哪一组。

| 上下文 | 命令标识 | Windows 默认 | 简单换算预期 | macOS 实际默认 |
|---|---|---|---|---|
| `MacSysShortcutsAlt` | `MacSysMenuHideMe` | *(无)* | *(无)* | `Cmd+macControl+H` |
| `MacSysShortcutsAlt` | `MacSysMenuHideOthers` | *(无)* | *(无)* | `Cmd+Option+macControl+H` |
| `MacSysShortcutsAlt` | `MacSysMenuMinimize` | *(无)* | *(无)* | `Cmd+macControl+M` |
| `MacSysShortcutsStd` | `MacSysMenuHideMe` | *(无)* | *(无)* | `Cmd+H` |
| `MacSysShortcutsStd` | `MacSysMenuHideOthers` | *(无)* | *(无)* | `Cmd+Option+H` |
| `MacSysShortcutsStd` | `MacSysMenuMinimize` | *(无)* | *(无)* | `Cmd+M` |

#### 「使用系统快捷键」开关与 AE 快捷键冲突说明

- 偏好键：`[Main Pref Section v2]` / `Pref_USE_STD_SYSTEM_SHORTCUTS_v2`，本机 AE 26.2.1 默认值为 **`true`（启用 `MacSysShortcutsStd`）**（仅存在于 macOS 偏好文件，Windows 偏好文件无此键）。
- 开启时（默认），以下 3 个 macOS 系统菜单快捷键会覆盖同键位的 AE 命令；关闭后系统菜单快捷键改为追加 `Control`（`MacSysShortcutsAlt`），释放原键位给 AE：

| 快捷键 | 系统菜单命令（开启时生效） | 被覆盖的 AE 命令 | AE 备用绑定 | 关闭开关后的系统快捷键 |
|---|---|---|---|---|
| `Cmd+H` | `MacSysShortcutsStd.MacSysMenuHideMe` | `CSwitchboard.ReplaceFootage` | *(无)* | `Cmd+macControl+H` |
| `Cmd+Option+H` | `MacSysShortcutsStd.MacSysMenuHideOthers` | `CSwitchboard.ToggleKeyframeHoldInterp` | *(无)* | `Cmd+Option+macControl+H` |
| `Cmd+M` | `MacSysShortcutsStd.MacSysMenuMinimize` | `CSwitchboard.AddCompToRenderQueue` | `Cmd+Shift+/` | `Cmd+macControl+M` |

## 3. 仅在 macOS 实际生效的 `macControl`（`⌃`）绑定（19 条）

以下 19 条命令在 Windows `aeks` 文件中也保留了含 `macControl` 的跨平台字符串，但 Windows 没有第四修饰键（Control 已用作 `Ctrl`），因此这些 `macControl+...` 绑定**仅在 macOS 上可用**。它们主要是为无数字小键盘的 MacBook 提供的替代键位，以及带 `Cmd+Ctrl` 的递归品质/分辨率命令：

| 上下文 | 命令标识 | macOS 绑定（含 `macControl`） | 用途分类 |
|---|---|---|---|
| `CCompCmd` | `AddLayerMarkerWithDialog` | `Option+PadMultiply` / `Option+macControl+8` | 无数字小键盘（MacBook）替代快捷键 |
| `CCompCmd` | `AddMarker` | `PadMultiply` / `macControl+8` | 无数字小键盘（MacBook）替代快捷键 |
| `CEggApp` | `PreviewAudio` | `PadDecimal` / `PadComma` / `PadDelete` / `macControl+.` | 无数字小键盘（MacBook）替代快捷键 |
| `CEggApp` | `PreviewAudioWorkArea` | `Option+PadDecimal` / `Option+PadComma` / `Option+PadDelete` / `macControl+Option+.` | 无数字小键盘（MacBook）替代快捷键 |
| `CEggApp` | `PreviewNumpad0` | `Pad0` / `PadInsert` / `macControl+0` | 无数字小键盘（MacBook）替代快捷键 |
| `CEggApp` | `PreviewNumpad0NFrames` | `Option+Pad0` / `Option+PadInsert` / `macControl+Option+0` | 无数字小键盘（MacBook）替代快捷键 |
| `CEggApp` | `PreviewShiftNumpad0` | `Shift+Pad0` / `Shift+PadInsert` / `macControl+Shift+0` | 无数字小键盘（MacBook）替代快捷键 |
| `CEggApp` | `ToggleTransmit` | `PadSlash` / `macControl+Shift+/` | 无数字小键盘（MacBook）替代快捷键 |
| `CEggApp` | `WriteStringFile` | `Cmd+Option+Shift+macControl+S` | 内部调试命令 |
| `CPanoProjItem` | `ProofColors` | `Shift+PadSlash` / `macControl+Option+/` | 无数字小键盘（MacBook）替代快捷键 |
| `CSwitchboard` | `BestQualityRecurse` | `Cmd+macControl+U` | 四修饰键递归设置嵌套合成品质/分辨率 |
| `CSwitchboard` | `CustResNoDialogRecurse` | `Cmd+Option+macControl+J` | 四修饰键递归设置嵌套合成品质/分辨率 |
| `CSwitchboard` | `DraftQualityRecurse` | `Cmd+Shift+macControl+U` | 四修饰键递归设置嵌套合成品质/分辨率 |
| `CSwitchboard` | `HighResolutionRecurse` | `Cmd+macControl+J` | 四修饰键递归设置嵌套合成品质/分辨率 |
| `CSwitchboard` | `LowResolutionRecurse` | `Cmd+Option+Shift+macControl+J` | 四修饰键递归设置嵌套合成品质/分辨率 |
| `CSwitchboard` | `MedResolutionRecurse` | `Cmd+Shift+macControl+J` | 四修饰键递归设置嵌套合成品质/分辨率 |
| `CSwitchboard` | `PurgeAllMemory` | `macControl+PadClear` / `Cmd+Option+PadSlash` | Mac 键盘 Clear 键配合 Control 清理内存 |
| `CSwitchboard` | `SavePreviewCurrent` | `macControl+Shift+C` / `Cmd+Pad0` / `Cmd+PadInsert` | 无数字小键盘（MacBook）替代快捷键 |
| `CSwitchboard` | `WireframeQualityRecurse` | `Cmd+Option+Shift+macControl+U` | 四修饰键递归设置嵌套合成品质/分辨率 |

## 4. 仅在 Mac 26.2.1 出现的命令（5 条）

> 注意：Windows 参考数据提取自 AE `26.0x67`，本机 Mac 为 AE `26.2.1`，以下差异包含小版本新增项。

| 上下文 | 命令标识 | macOS 绑定 | 差异归因 |
|---|---|---|---|
| `CCompCompCmd` | `CompTimeZoomWorkArea` | `Option+;` | AE 26.2.1 默认 `aeks` 新增项，Windows `26.0x67` 快照时尚未收录（推测为 26.1/26.2 跨平台新增） |
| `CItem` | `CopyFrameToClipboard` | `Cmd+Option+Shift+F5` | AE 26.2.1 默认 `aeks` 新增项，Windows `26.0x67` 快照时尚未收录（推测为 26.1/26.2 跨平台新增） |
| `CSwitchboard` | `Clear` | `PadClear` | 绑定为 `PadClear`（Apple 扩展键盘数字区 Clear 键；`CSwitchboardModal.Clear` 在双平台均有 `PadClear`） |
| `CSwitchboard` | `PasteReversedKeyframes` | `Cmd+Shift+V` | AE 26.2.1 默认 `aeks` 新增项，Windows `26.0x67` 快照时尚未收录（推测为 26.1/26.2 跨平台新增） |
| `TLOutlinePano` | `CompTimeZoomWorkArea` | `Option+;` | AE 26.2.1 默认 `aeks` 新增项，Windows `26.0x67` 快照时尚未收录（推测为 26.1/26.2 跨平台新增） |

## 5. 给 BAE 快捷键实现的 macOS 键位约定说明

1. **修饰键标识映射**：AE `aeks` 文件使用 4 个修饰键词元：`Cmd`（macOS `⌘`，对应 Windows `Ctrl`）、`Option`（macOS `⌥`，对应 Windows `Alt`）、`Shift`（`⇧`）、`macControl`（macOS 物理 Control `⌃`，**不是** `Cmd`）。
2. **删除键区分**：AE 的 `Delete` 对应主键盘退格键（macOS 标为 Delete `⌫`，Windows 标为 Backspace），`FwdDel` 对应向前删除键（macOS 扩展键盘 `⌦` 或 MacBook `Fn+Delete`，Windows 标为 Delete）。可从 `TextLayerUI.TextDeleteBackward = ["Delete"]` 与 `TextLayerUI.TextDeleteForward = ["FwdDel"]` 交叉印证。
3. **回车键区分**：`Return` 为主键盘回车（`↩`，用于重命名 `COutline.Rename`、`Cmd+Return` 提交文字），`Enter` 为数字小键盘 Enter（`⌤`，MacBook 上为 `Fn+Return`，用于打开项目项 `POutlinePano.OpenItemWindow`、提交文字 `TextLayerUI.TextCommit`）。
4. **无小键盘导航键（MacBook）**：AE 大量时间轴/定位快捷键使用 `HOME`、`END`、`PageUP`、`PageDOWN`（如 `Cmd+HOME` 视点居中、`Cmd+Option+HOME` 居中锚点、`PageUP`/`PageDOWN` 逐帧步进），在 MacBook 上对应 `Fn+←/→/↑/↓`；此外 `macControl+0` / `macControl+.` / `macControl+8` 等为小键盘 `Pad0`/`PadDecimal`/`PadMultiply` 的官方 Mac 替代键。
5. **设置对话框快捷键**：在 `aeks` 默认配置中，`CSwitchboard.PrefsGeneral` 与 `CSwitchboard.PrefsStartupAndRepairV2` 均绑定为 `Cmd+Option+;`（与 Windows 的 `Ctrl+Alt+;` 对应），**而非** macOS 常规应用的 `Cmd+,`（实际菜单栏快捷键显示将在菜单调研中进一步核对）。
6. **文件换行符**：macOS 版 `After Effects Default.txt` 使用经典 CR（`\r`）换行，解析时需开启通用换行处理。
