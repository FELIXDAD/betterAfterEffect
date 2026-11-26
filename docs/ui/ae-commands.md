# AE 命令目录

> 由 `crates/bae-ui-motion/src/catalog/` 生成，不要手改。改了目录后运行 `BAE_UPDATE_DOCS=1 cargo test -p bae-ui-motion catalog` 重新生成；文档过期时这个测试会失败。

这是 AE 一致性工作的总表：After Effects 26 主菜单的每一项（`docs/reference/ae/menus.json`，Windows）和默认快捷键文件的每一条命令（`docs/reference/ae/shortcuts.json`）都对应一个 BAE 动作 id。菜单栏、AE 键位表都从这份目录生成；测试逐项对照 AE 的参考数据，菜单少了、多了、顺序变了都会报错。新加动作的 agent 按下面的规则起名，B（`ui/ae-menus`）在后续 PR 里把它补进目录。

## 动作 id 命名

- 已有的 id 不改：外壳的 `app.* file.* edit.* window.* workspace.* help.*`，动效工作区的 `motion.*` 和面板动作 `viewer.* timeline.*`，领域命令 `comp.* layer.* key.* attr.*`。
- 新的界面动作用 `motion.<区域>.<名字>`：区域是 AE 顶层菜单（`file edit comp layer effect anim view window help`），或 `tool`（工具）、`timeline`、`viewer`、`project`、`panel`；名字是 AE 英文菜单文字转成 snake_case（去掉省略号）。例：`motion.comp.composition_settings`、`motion.tool.pen`、`motion.view.show_grid`、`motion.timeline.graph_editor`。
- 同一个动作的不同变体用参数区分：`motion.layer.blending_mode {"mode":"multiply"}`、`motion.edit.label {"label":1}`、`window.panel {"id":"info","toggle":true}`。
- 设置（首选项）各页用 `app.settings`，参数是页面 id：`{"page":"startup_repair"}`。
- 新增领域命令（会改工程数据的）放在 `bae-motion`，用 `<对象>.<动作>`，例如 `mask.create`、`shape.add_group`、`path.set_vertices`。

## 图例

| 状态 | 含义 |
|---|---|
| ✅ | 已实现，和 AE 一致 |
| ◐ | 已实现，有差异（见 `docs/ui/motion-parity.md`） |
| ☐ | 本轮计划实现；实现前菜单里置灰 |
| ⏸ | 以后再做；菜单里置灰 |
| Ⓐ | Adobe 的服务或产品集成（Bridge、Media Encoder、Dynamic Link……）；菜单里置灰 |
| ⋯ | 运行时填充的列表（最近的文件、脚本、扩展），不是固定菜单项 |

| 负责 | 工作 | 分支 |
|---|---|---|
| A | 工具栏和合成查看器里的工具、蒙版和路径编辑、查看器 | `ui/motion-tools` |
| B | 主菜单、AE 快捷键、菜单背后的简单命令、本目录 | `ui/ae-menus` |
| C | 时间轴、图表编辑器、关键帧对话框、标记、表达式 | `ui/motion-timeline` |
| D | 设置（首选项） | `ui/ae-settings` |
| E | 新建 / 编辑类对话框、面板、项目面板、导入 | `ui/motion-dialogs` |
| F | 渲染器功能 | `render/masks` |

快捷键列：Windows 是 AE 26.0 的默认值（`docs/reference/ae/shortcuts.json`）；macOS 是 AE 26.2.1 的默认值（`docs/reference/ae/mac/shortcuts.json`）：一般是 Ctrl→Cmd、Alt→Opt，例外按 Mac 文件（`catalog::MAC_KEYS`）。`Control` 是 macOS 的 Control 键（AE 文件里的 `macControl`）。`PadX` 是小键盘键。

各状态的数量（菜单项、快捷键）不写在这里，免得每个 PR 都改这一段：见 CI 每次运行上传的 `shortcut-sweep.md`（完整 CI 的运行页面 → Artifacts → `bae-<平台>`）。本地生成：`BAE_SHORTCUT_REPORT=shortcut-sweep.md cargo test -p bae --bin bae shortcut_sweep`。

## 主菜单

### File

| AE 菜单路径 | AE id | BAE 动作 id | 参数 | Windows | macOS | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| New ▸ New Project | 16187393 | `file.new_project` |  | Ctrl+Alt+N | Cmd+Opt+N | B | ✅ |
| New ▸ New Folder | 16187394 | `motion.file.new_folder` |  | Ctrl+Alt+Shift+N | Cmd+Opt+Shift+N | B | ✅ |
| New ▸ Adobe Photoshop File... | 16187395 | `motion.file.adobe_photoshop_file` |  |  |  | B | Ⓐ |
| New ▸ Maxon Cinema 4D File... | 16187396 | `motion.file.maxon_cinema_4d_file` |  |  |  | B | Ⓐ |
| Open Project... | 131074 | `file.open_project` |  | Ctrl+O | Cmd+O | B | ✅ |
| Open Recent ▸ (recent projects) |  | `motion.file.open_recent` |  | Ctrl+Alt+Shift+P | Cmd+Opt+Shift+P | B | ⋯ |
| Browse in Bridge... | 131076 | `motion.file.browse_in_bridge` |  | Ctrl+Alt+Shift+O | Cmd+Opt+Shift+O | B | Ⓐ |
| Close | 131078 | `window.close_panel` |  | Ctrl+W | Cmd+W | B | ✅ |
| Close Project | 131079 | `file.close_project` |  |  |  | B | ✅ |
| Save | 131080 | `file.save` |  | Ctrl+S | Cmd+S | B | ✅ |
| Save As ▸ Save As... | 21889025 | `file.save_as` |  | Ctrl+Shift+S | Cmd+Shift+S | B | ✅ |
| Save As ▸ Save a Copy... | 21889026 | `file.save_a_copy` |  |  |  | B | ✅ |
| Save As ▸ Save a Copy As XML... | 21889027 | `motion.file.save_a_copy_as_xml` |  |  |  | B | Ⓐ |
| Save As ▸ Save a Copy As 25.x... | 21889028 | `motion.file.save_a_copy_as_25_x` |  |  |  | B | Ⓐ |
| Save As ▸ Save a Copy As 24.x... | 21889029 | `motion.file.save_a_copy_as_24_x` |  |  |  | B | Ⓐ |
| Increment and Save | 131082 | `file.increment_and_save` |  | Ctrl+Alt+Shift+S | Cmd+Opt+Shift+S | B | ✅ |
| Revert | 131083 | `file.revert` |  |  |  | B | ✅ |
| Import ▸ File... | 9764865 | `motion.file.import_file` |  | Ctrl+I | Cmd+I | E | ✅ |
| Import ▸ Multiple Files... | 9764866 | `motion.file.import_multiple_files` |  | Ctrl+Alt+I | Cmd+Opt+I | E | ✅ |
| Import ▸ From Libraries... | 9764867 | `motion.file.from_libraries` |  |  |  | B | Ⓐ |
| Import ▸ Import Adobe Premiere Project... | 9764868 | `motion.file.import_adobe_premiere_project` |  |  |  | B | Ⓐ |
| Import ▸ Vanishing Point (.vpe)... | 9764869 | `motion.file.vanishing_point_vpe` |  |  |  | B | Ⓐ |
| Import ▸ Placeholder... | 9764870 | `motion.file.import_placeholder` |  |  |  | E | ☐ |
| Import ▸ Solid... | 9764871 | `motion.file.import_solid` |  |  |  | E | ☐ |
| Import Recent Footage ▸ (recent footage) |  | `motion.file.import_recent_footage` |  |  |  | E | ⋯ |
| Export ▸ Maxon Cinema 4D Exporter... | 11665409 | `motion.file.maxon_cinema_4d_exporter` |  |  |  | B | Ⓐ |
| Export ▸ Export Adobe Premiere Project... | 11665410 | `motion.file.export_adobe_premiere_project` |  |  |  | B | Ⓐ |
| Export ▸ Add to Adobe Media Encoder Queue... | 11665411 | `motion.comp.add_to_adobe_media_encoder_queue` |  | Ctrl+Alt+M | Cmd+Opt+M | B | Ⓐ |
| Export ▸ Add to Render Queue | 11665412 | `motion.add_to_render_queue` |  | Ctrl+M / Ctrl+Shift+/ | Control+Cmd+M / Cmd+Shift+/ | B | ✅ |
| Add Fonts from Adobe... | 131089 | `motion.file.add_fonts_from_adobe` |  |  |  | B | Ⓐ |
| Adobe Dynamic Link ▸ New Premiere Sequence... | 12845057 | `motion.file.new_premiere_sequence` |  |  |  | B | Ⓐ |
| Adobe Dynamic Link ▸ Import Premiere Sequence... | 12845058 | `motion.file.import_premiere_sequence` |  |  |  | B | Ⓐ |
| Find | 131093 | `motion.file.find` |  | Ctrl+F | Cmd+F | E | ✅ |
| Add Footage to Comp | 131095 | `motion.file.add_footage_to_comp` |  | Ctrl+/ | Cmd+/ | E | ✅ |
| New Comp from Selection... | 131096 | `motion.file.new_comp_from_selection` |  | Alt+\\ | Opt+\\ | E | ✅ |
| Dependencies ▸ Collect Files... | 22806529 | `motion.file.collect_files` |  |  |  | B | ⏸ |
| Dependencies ▸ Consolidate All Footage | 22806531 | `motion.file.consolidate_all_footage` |  |  |  | B | ⏸ |
| Dependencies ▸ Remove Unused Footage | 22806532 | `motion.file.remove_unused_footage` |  |  |  | B | ⏸ |
| Dependencies ▸ Reduce Project | 22806533 | `motion.file.reduce_project` |  |  |  | B | ⏸ |
| Dependencies ▸ Find Missing Effects | 22806535 | `motion.file.find_missing_effects` |  |  |  | E | ⏸ |
| Dependencies ▸ Find Missing Fonts | 22806536 | `motion.file.find_missing_fonts` |  |  |  | E | ⏸ |
| Dependencies ▸ Find Missing Footage | 22806537 | `motion.file.find_missing_footage` |  |  |  | E | ⏸ |
| Watch Folder... | 131099 | `motion.file.watch_folder` |  |  |  | B | Ⓐ |
| Scripts ▸ Run Script File... | 917505 | `motion.file.run_script_file` |  |  |  | B | ⏸ |
| Scripts ▸ Install Script File... | 917506 | `motion.file.install_script_file` |  |  |  | B | ⏸ |
| Scripts ▸ Install ScriptUI Panel... | 917507 | `motion.file.install_scriptui_panel` |  |  |  | B | ⏸ |
| Scripts ▸ Recent Script Files ▸ (recent scripts) |  | `motion.file.recent_script` |  | Ctrl+Alt+Shift+D | Cmd+Opt+Shift+D | B | ⋯ |
| Scripts ▸ (installed script) | 917510 | `motion.file.run_script` |  |  |  | B | ⋯ |
| Scripts ▸ (installed script) | 917511 | `motion.file.run_script` |  |  |  | B | ⋯ |
| Scripts ▸ (installed script) | 917512 | `motion.file.run_script` |  |  |  | B | ⋯ |
| Scripts ▸ (installed script) | 917513 | `motion.file.run_script` |  |  |  | B | ⋯ |
| Scripts ▸ (installed script) | 917514 | `motion.file.run_script` |  |  |  | B | ⋯ |
| Scripts ▸ (installed script) | 917515 | `motion.file.run_script` |  |  |  | B | ⋯ |
| Scripts ▸ (installed script) | 917516 | `motion.file.run_script` |  |  |  | B | ⋯ |
| Scripts ▸ (installed script) | 917517 | `motion.file.run_script` |  |  |  | B | ⋯ |
| Scripts ▸ (installed script) | 917518 | `motion.file.run_script` |  |  |  | B | ⋯ |
| Scripts ▸ (installed script) | 917519 | `motion.file.run_script` |  |  |  | B | ⋯ |
| Scripts ▸ (installed script) | 917520 | `motion.file.run_script` |  |  |  | B | ⋯ |
| Scripts ▸ (installed script) | 917521 | `motion.file.run_script` |  |  |  | B | ⋯ |
| Create Proxy ▸ Still... | 10813441 | `motion.file.create_proxy_still` |  |  |  | E | ⏸ |
| Create Proxy ▸ Movie... | 10813442 | `motion.file.create_proxy_movie` |  |  |  | E | ⏸ |
| Set Proxy ▸ File... | 9830401 | `motion.file.set_proxy_file` |  | Ctrl+Alt+P | Cmd+Opt+P | E | ⏸ |
| Set Proxy ▸ None | 9830403 | `motion.file.set_proxy_none` |  |  |  | E | ⏸ |
| Interpret Footage ▸ Main... | 10420225 | `motion.file.interpret_footage_main` |  | Ctrl+Alt+G | Cmd+Opt+G | E | ✅ |
| Interpret Footage ▸ Proxy... | 10420226 | `motion.file.interpret_footage_proxy` |  |  |  | E | ⏸ |
| Interpret Footage ▸ Remember Interpretation | 10420228 | `motion.file.remember_interpretation` |  | Ctrl+Alt+C | Cmd+Opt+C | E | ☐ |
| Interpret Footage ▸ Apply Interpretation | 10420229 | `motion.file.apply_interpretation` |  | Ctrl+Alt+V | Cmd+Opt+V | E | ☐ |
| Replace Footage ▸ File... | 11141121 | `motion.file.replace_footage_file` |  | Ctrl+H | Control+Cmd+H | E | ✅ |
| Replace Footage ▸ With Layered Comp | 11141122 | `motion.file.replace_footage_with_layered_comp` |  |  |  | B | Ⓐ |
| Replace Footage ▸ Placeholder... | 11141124 | `motion.file.replace_footage_placeholder` |  |  |  | E | ⏸ |
| Replace Footage ▸ Solid... | 11141125 | `motion.file.replace_footage_solid` |  |  |  | E | ⏸ |
| Reload Footage | 131107 | `motion.file.reload_footage` |  | Ctrl+Alt+L | Cmd+Opt+L | E | ✅ |
| License... | 131108 | `motion.file.license` |  |  |  | B | Ⓐ |
| Reveal in Explorer | 131109 | `motion.file.reveal_in_explorer` |  |  |  | B | ✅ |
| Reveal in Bridge | 131110 | `motion.file.reveal_in_bridge` |  |  |  | B | Ⓐ |
| Project Settings... | 131112 | `motion.file.project_settings` |  | Ctrl+Alt+Shift+K | Cmd+Opt+Shift+K | E | ✅ |
| Exit（仅 Windows） | 131114 | `app.quit` |  | Ctrl+Q | Cmd+Q | B | ✅ |

### Edit

| AE 菜单路径 | AE id | BAE 动作 id | 参数 | Windows | macOS | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| Undo | 196609 | `edit.undo` |  | Ctrl+Z | Cmd+Z | B | ✅ |
| Redo | 196610 | `edit.redo` |  | Ctrl+Shift+Z | Cmd+Shift+Z | B | ✅ |
| Cut | 196613 | `edit.cut` |  | Ctrl+X | Cmd+X | B | ✅ |
| Copy | 196614 | `edit.copy` |  | Ctrl+C | Cmd+C | B | ◐ |
| Copy with Property Links | 196615 | `motion.edit.copy_with_property_links` |  | Ctrl+Alt+C | Cmd+Opt+C | B | ⏸ |
| Copy with Relative Property Links | 196616 | `motion.edit.copy_with_relative_property_links` |  |  |  | B | ⏸ |
| Copy Expression Only | 196617 | `motion.edit.copy_expression_only` |  |  |  | C | ⏸ |
| Paste | 196618 | `edit.paste` |  | Ctrl+V | Cmd+V | B | ◐ |
| Paste Reversed Keyframes | 196619 | `motion.edit.paste_reversed_keyframes` |  |  | Cmd+Shift+V | C | ☐ |
| Paste Text and Match Formatting | 196620 | `motion.edit.paste_text_and_match_formatting` |  | Ctrl+Shift+B | Cmd+Shift+B | E | ⏸ |
| Paste Text Formatting Only | 196621 | `motion.edit.paste_text_formatting_only` |  | Ctrl+Alt+Shift+B | Cmd+Opt+Shift+B | E | ⏸ |
| Clear | 196622 | `motion.delete` |  | Backspace / Delete | Backspace / Delete / PadClear | B | ✅ |
| Duplicate | 196624 | `motion.duplicate` |  | Ctrl+D | Cmd+D | B | ✅ |
| Split Layer | 196625 | `motion.split` |  | Ctrl+Shift+D | Cmd+Shift+D | B | ✅ |
| Lift Work Area | 196626 | `motion.edit.lift_work_area` |  |  |  | B | ☐ |
| Extract Work Area | 196627 | `motion.edit.extract_work_area` |  |  |  | B | ☐ |
| Select All | 196628 | `motion.select_all` |  | Ctrl+A | Cmd+A | B | ✅ |
| Deselect All | 196629 | `motion.deselect_all` |  | Ctrl+Shift+A / F2 | Cmd+Shift+A / F2 | B | ✅ |
| Label ▸ Select Label Group | 16646145 | `motion.edit.select_label_group` |  |  |  | B | ✅ |
| Label ▸ None | 16646147 | `motion.edit.label` | `{"label":0}` |  |  | B | ✅ |
| Label ▸ Red | 16646149 | `motion.edit.label` | `{"label":1}` |  |  | B | ✅ |
| Label ▸ Yellow | 16646150 | `motion.edit.label` | `{"label":2}` |  |  | B | ✅ |
| Label ▸ Aqua | 16646151 | `motion.edit.label` | `{"label":3}` |  |  | B | ✅ |
| Label ▸ Pink | 16646152 | `motion.edit.label` | `{"label":4}` |  |  | B | ✅ |
| Label ▸ Lavender | 16646153 | `motion.edit.label` | `{"label":5}` |  |  | B | ✅ |
| Label ▸ Peach | 16646154 | `motion.edit.label` | `{"label":6}` |  |  | B | ✅ |
| Label ▸ Sea Foam | 16646155 | `motion.edit.label` | `{"label":7}` |  |  | B | ✅ |
| Label ▸ Blue | 16646156 | `motion.edit.label` | `{"label":8}` |  |  | B | ✅ |
| Label ▸ Green | 16646157 | `motion.edit.label` | `{"label":9}` |  |  | B | ✅ |
| Label ▸ Purple | 16646158 | `motion.edit.label` | `{"label":10}` |  |  | B | ✅ |
| Label ▸ Orange | 16646159 | `motion.edit.label` | `{"label":11}` |  |  | B | ✅ |
| Label ▸ Brown | 16646160 | `motion.edit.label` | `{"label":12}` |  |  | B | ✅ |
| Label ▸ Fuchsia | 16646161 | `motion.edit.label` | `{"label":13}` |  |  | B | ✅ |
| Label ▸ Cyan | 16646162 | `motion.edit.label` | `{"label":14}` |  |  | B | ✅ |
| Label ▸ Sandstone | 16646163 | `motion.edit.label` | `{"label":15}` |  |  | B | ✅ |
| Label ▸ Dark Green | 16646164 | `motion.edit.label` | `{"label":16}` |  |  | B | ✅ |
| Label ▸ Edit Label Colors... | 16646166 | `motion.edit.edit_label_colors` |  |  |  | D | ✅ |
| Select Keyframe Label Group ▸ On Selected Layers | 23592961 | `motion.edit.select_keyframe_label_group` | `{"scope":"selected_layers"}` |  |  | C | ✅ |
| Select Keyframe Label Group ▸ On All Layers | 23592962 | `motion.edit.select_keyframe_label_group` | `{"scope":"all_layers"}` |  |  | C | ✅ |
| Select Keyframe Label Group ▸ Visible Keyframes on Selected Layers | 23592964 | `motion.edit.select_keyframe_label_group` | `{"scope":"visible_selected_layers"}` |  |  | C | ◐ |
| Select Keyframe Label Group ▸ Visible Keyframes on All Layers | 23592965 | `motion.edit.select_keyframe_label_group` | `{"scope":"visible_all_layers"}` |  |  | C | ◐ |
| Purge ▸ All Cache... | 11403265 | `motion.edit.purge` | `{"what":"all_caches"}` | Alt+Z | Opt+Z | B | ⏸ |
| Purge ▸ All Memory & Disk Cache... | 11403266 | `motion.edit.purge` | `{"what":"memory_and_disk_cache"}` |  |  | B | ⏸ |
| Purge ▸ All Memory | 11403267 | `motion.edit.purge` | `{"what":"all_memory"}` | Ctrl+Alt+PadSlash | Control+PadClear / Cmd+Opt+PadSlash | B | ⏸ |
| Purge ▸ All Disk Cache... | 11403268 | `motion.edit.purge` | `{"what":"disk_cache"}` |  |  | B | ⏸ |
| Purge ▸ All 3D Cache... | 11403269 | `motion.edit.purge` | `{"what":"3d_cache"}` |  |  | B | ⏸ |
| Purge ▸ Undo | 11403270 | `motion.edit.purge` | `{"what":"undo"}` |  |  | B | ☐ |
| Purge ▸ Image Cache Memory | 11403271 | `motion.edit.purge` | `{"what":"image_cache_memory"}` |  |  | B | ⏸ |
| Purge ▸ Snapshot | 11403272 | `motion.edit.purge` | `{"what":"snapshot"}` |  |  | B | ⏸ |
| Edit Original... | 196636 | `motion.edit.edit_original` |  | Ctrl+E | Cmd+E | B | ✅ |
| Edit in Adobe Audition | 196637 | `motion.edit.edit_in_adobe_audition` |  |  |  | B | Ⓐ |
| Templates ▸ Render Settings... | 16252929 | `motion.edit.templates_render_settings` |  |  |  | E | ⏸ |
| Templates ▸ Output Module... | 16252930 | `motion.edit.templates_output_module` |  |  |  | E | ⏸ |
| Preferences ▸ Startup & Repair... | 24182785 | `app.settings` | `{"page":"startup_repair"}` | Ctrl+Alt+; | Cmd+Opt+; | D | ☐ |
| Preferences ▸ Project... | 24182786 | `app.settings` | `{"page":"project"}` |  |  | D | ☐ |
| Preferences ▸ Composition... | 24182787 | `app.settings` | `{"page":"composition"}` |  |  | D | ☐ |
| Preferences ▸ Previews... | 24182788 | `app.settings` | `{"page":"previews"}` |  |  | D | ☐ |
| Preferences ▸ Appearance... | 24182790 | `app.settings` | `{"page":"appearance"}` |  |  | D | ☐ |
| Preferences ▸ Grids & Guides... | 24182791 | `app.settings` | `{"page":"grids_guides"}` |  |  | D | ☐ |
| Preferences ▸ Labels... | 24182792 | `app.settings` | `{"page":"labels"}` |  |  | D | ☐ |
| Preferences ▸ Type... | 24182793 | `app.settings` | `{"page":"type"}` |  |  | D | ☐ |
| Preferences ▸ Import... | 24182795 | `app.settings` | `{"page":"import"}` |  |  | D | ☐ |
| Preferences ▸ Export... | 24182796 | `app.settings` | `{"page":"export"}` |  |  | D | ☐ |
| Preferences ▸ Audio... | 24182798 | `app.settings` | `{"page":"audio"}` |  |  | D | ☐ |
| Preferences ▸ Disk... | 24182799 | `app.settings` | `{"page":"disk"}` |  |  | D | ☐ |
| Preferences ▸ Memory & CPU... | 24182800 | `app.settings` | `{"page":"memory_cpu"}` |  |  | D | ☐ |
| Preferences ▸ Video... | 24182801 | `app.settings` | `{"page":"video"}` |  |  | D | ☐ |
| Preferences ▸ 3D... | 24182803 | `app.settings` | `{"page":"3d"}` |  |  | D | ☐ |
| Preferences ▸ Scripting & Expressions... | 24182804 | `app.settings` | `{"page":"scripting_expressions"}` |  |  | D | ☐ |
| Keyboard Shortcuts | 196643 | `app.keyboard_shortcuts` |  | Ctrl+Alt+' | Cmd+Opt+' | B | ✅ |
| Paste Mocha Mask | 196644 | `motion.edit.paste_mocha_mask` |  |  |  | B | Ⓐ |

### Composition

| AE 菜单路径 | AE id | BAE 动作 id | 参数 | Windows | macOS | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| New Composition... | 327681 | `motion.new_comp` |  | Ctrl+N | Cmd+N | B | ◐ |
| Composition Settings... | 327683 | `motion.comp.composition_settings` |  | Ctrl+K | Cmd+K | E | ✅ |
| Set Poster Time | 327684 | `motion.comp.set_poster_time` |  |  |  | B | ☐ |
| Trim Comp to Work Area | 327685 | `motion.trim_comp` |  | Ctrl+Shift+X | Cmd+Shift+X | B | ✅ |
| Crop Comp to Region of Interest | 327686 | `motion.comp.crop_comp_to_region_of_interest` |  |  |  | B | ⏸ |
| Crop Comp to Selected Layer(s) Bounds | 327687 | `motion.comp.crop_comp_to_selected_layer_bounds` |  |  |  | B | ⏸ |
| Add to Adobe Media Encoder Queue... | 327689 | `motion.comp.add_to_adobe_media_encoder_queue` |  | Ctrl+Alt+M | Cmd+Opt+M | B | Ⓐ |
| Add to Render Queue | 327690 | `motion.add_to_render_queue` |  | Ctrl+M / Ctrl+Shift+/ | Control+Cmd+M / Cmd+Shift+/ | B | ✅ |
| Add Output Module | 327691 | `motion.comp.add_output_module` |  |  |  | B | ⏸ |
| Preview ▸ Play Current Preview | 10682369 | `motion.play` |  | Space | Space | B | ✅ |
| Preview ▸ Cache Frames When Idle | 10682370 | `motion.comp.cache_frames_when_idle` |  | Alt+Shift+I | Opt+Shift+I | B | ⏸ |
| Preview ▸ Audio | 10682372 | `motion.comp.preview_audio` |  |  |  | B | ⏸ |
| Save Frame As ▸ File... | 15335425 | `motion.comp.save_frame_as_file` |  | Ctrl+Alt+S | Cmd+Opt+S | B | ✅ |
| Save Frame As ▸ Photoshop Layers... | 15335427 | `motion.comp.save_frame_as_photoshop_layers` |  |  |  | B | ⏸ |
| Save Frame As ▸ ProEXR... | 15335428 | `motion.comp.save_frame_as_proexr` |  |  |  | B | Ⓐ |
| Pre-render... | 327695 | `motion.comp.pre_render` |  |  |  | B | ⏸ |
| Save Current Preview... | 327696 | `motion.comp.save_current_preview` |  | Ctrl+Pad0 / Ctrl+PadInsert | Control+Shift+C / Cmd+Pad0 / Cmd+PadInsert | B | ⏸ |
| Open in Essential Graphics | 327698 | `motion.comp.open_in_essential_graphics` |  |  |  | E | ⏸ |
| Responsive Design — Time ▸ Create Intro | 2097153 | `motion.comp.create_intro` |  |  |  | B | ⏸ |
| Responsive Design — Time ▸ Create Outro | 2097154 | `motion.comp.create_outro` |  |  |  | B | ⏸ |
| Responsive Design — Time ▸ Create Protected Region from Work Area | 2097155 | `motion.comp.create_protected_region_from_work_area` |  |  |  | B | ⏸ |
| Composition Flowchart | 327701 | `motion.comp.composition_flowchart` |  | Ctrl+Shift+F11 | Cmd+Shift+F11 | E | ⏸ |
| Composition Mini-Flowchart | 327702 | `motion.comp.composition_mini_flowchart` |  | Tab | Tab | E | ✅ |
| VR ▸ Create VR Environment... | 328466433 | `motion.comp.create_vr_environment` |  |  |  | B | ⏸ |
| VR ▸ Extract Cubemap... | 328466434 | `motion.comp.extract_cubemap` |  |  |  | B | ⏸ |

### Layer

| AE 菜单路径 | AE id | BAE 动作 id | 参数 | Windows | macOS | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| New ▸ Text | 11862017 | `motion.new_layer` | `{"kind":"text"}` | Ctrl+Alt+Shift+T | Cmd+Opt+Shift+T | B | ✅ |
| New ▸ Solid... | 11862018 | `motion.new_layer` | `{"kind":"solid"}` | Ctrl+Y | Cmd+Y | E | ◐ |
| New ▸ Light... | 11862019 | `motion.new_layer` | `{"kind":"light"}` | Ctrl+Alt+Shift+L | Cmd+Opt+Shift+L | E | ◐ |
| New ▸ Camera... | 11862020 | `motion.new_layer` | `{"kind":"camera"}` | Ctrl+Alt+Shift+C | Cmd+Opt+Shift+C | E | ◐ |
| New ▸ Null Object | 11862021 | `motion.new_layer` | `{"kind":"null"}` | Ctrl+Alt+Shift+Y | Cmd+Opt+Shift+Y | B | ✅ |
| New ▸ Shape Layer | 11862022 | `motion.new_layer` | `{"kind":"shape"}` |  |  | B | ✅ |
| New ▸ Adjustment Layer | 11862023 | `motion.new_layer` | `{"kind":"adjustment"}` | Ctrl+Alt+Y | Cmd+Opt+Y | B | ✅ |
| New ▸ Content-Aware Fill Layer... | 11862024 | `motion.layer.new_content_aware_fill_layer` |  |  |  | E | ⏸ |
| New ▸ Adobe Photoshop File... | 11862025 | `motion.layer.new_adobe_photoshop_file` |  |  |  | B | Ⓐ |
| New ▸ Maxon Cinema 4D File... | 11862026 | `motion.layer.new_maxon_cinema_4d_file` |  |  |  | B | Ⓐ |
| New ▸ Cube | 11862028 | `motion.layer.new_3d_shape` | `{"shape":"cube"}` |  |  | B | ⏸ |
| New ▸ Sphere | 11862029 | `motion.layer.new_3d_shape` | `{"shape":"sphere"}` |  |  | B | ⏸ |
| New ▸ Plane | 11862030 | `motion.layer.new_3d_shape` | `{"shape":"plane"}` |  |  | B | ⏸ |
| New ▸ Torus | 11862031 | `motion.layer.new_3d_shape` | `{"shape":"torus"}` |  |  | B | ⏸ |
| New ▸ Cone | 11862032 | `motion.layer.new_3d_shape` | `{"shape":"cone"}` |  |  | B | ⏸ |
| New ▸ Cylinder | 11862033 | `motion.layer.new_3d_shape` | `{"shape":"cylinder"}` |  |  | B | ⏸ |
| Layer Settings... | 393218 | `motion.layer.layer_settings` |  | Ctrl+Shift+Y | Cmd+Shift+Y | E | ✅ |
| Open Layer | 393220 | `motion.layer.open_layer` |  |  |  | E | ✅ |
| Open Layer Source | 393221 | `motion.layer.open_layer_source` |  | Alt+PadEnter | Opt+PadEnter | B | ✅ |
| Reveal in Explorer | 393222 | `motion.file.reveal_in_explorer` |  |  |  | B | ✅ |
| Mask ▸ New Mask | 8519681 | `motion.layer.new_mask` |  | Ctrl+Shift+N | Cmd+Shift+N | A | ✅ |
| Mask ▸ Mask Shape... | 8519683 | `motion.layer.mask_shape` |  | Ctrl+Shift+M | Cmd+Shift+M | E | ✅ |
| Mask ▸ Mask Feather... | 8519684 | `motion.layer.mask_feather` |  | Ctrl+Shift+F | Cmd+Shift+F | E | ✅ |
| Mask ▸ Mask Opacity... | 8519685 | `motion.layer.mask_opacity` |  |  |  | E | ✅ |
| Mask ▸ Mask Expansion... | 8519686 | `motion.layer.mask_expansion` |  |  |  | E | ✅ |
| Mask ▸ Reset Mask | 8519688 | `motion.layer.reset_mask` |  |  |  | B | ✅ |
| Mask ▸ Remove Mask | 8519689 | `motion.layer.remove_mask` |  |  |  | B | ✅ |
| Mask ▸ Remove All Masks | 8519690 | `motion.layer.remove_all_masks` |  |  |  | B | ✅ |
| Mask ▸ Mode ▸ None | 11468801 | `motion.layer.mask_mode` | `{"mode":"none"}` |  |  | B | ✅ |
| Mask ▸ Mode ▸ Add | 11468803 | `motion.layer.mask_mode` | `{"mode":"add"}` |  |  | B | ✅ |
| Mask ▸ Mode ▸ Subtract | 11468804 | `motion.layer.mask_mode` | `{"mode":"subtract"}` |  |  | B | ✅ |
| Mask ▸ Mode ▸ Intersect | 11468805 | `motion.layer.mask_mode` | `{"mode":"intersect"}` |  |  | B | ✅ |
| Mask ▸ Mode ▸ Lighten | 11468806 | `motion.layer.mask_mode` | `{"mode":"lighten"}` |  |  | B | ✅ |
| Mask ▸ Mode ▸ Darken | 11468807 | `motion.layer.mask_mode` | `{"mode":"darken"}` |  |  | B | ✅ |
| Mask ▸ Mode ▸ Difference | 11468808 | `motion.layer.mask_mode` | `{"mode":"difference"}` |  |  | B | ✅ |
| Mask ▸ Inverted | 8519693 | `motion.layer.mask_inverted` |  | Ctrl+Shift+I | Cmd+Shift+I | B | ✅ |
| Mask ▸ Locked | 8519694 | `motion.layer.mask_locked` |  |  |  | B | ✅ |
| Mask ▸ Motion Blur ▸ Same As Layer | 14352385 | `motion.layer.mask_motion_blur` | `{"mode":"same_as_layer"}` |  |  | B | ⏸ |
| Mask ▸ Motion Blur ▸ On | 14352386 | `motion.layer.mask_motion_blur` | `{"mode":"on"}` |  |  | B | ⏸ |
| Mask ▸ Motion Blur ▸ Off | 14352387 | `motion.layer.mask_motion_blur` | `{"mode":"off"}` |  |  | B | ⏸ |
| Mask ▸ Feather Falloff ▸ Smooth | 22544385 | `motion.layer.mask_feather_falloff` | `{"falloff":"smooth"}` |  |  | B | ⏸ |
| Mask ▸ Feather Falloff ▸ Linear | 22544386 | `motion.layer.mask_feather_falloff` | `{"falloff":"linear"}` |  |  | B | ⏸ |
| Mask ▸ Unlock All Masks | 8519698 | `motion.layer.unlock_all_masks` |  |  |  | B | ✅ |
| Mask ▸ Lock Other Masks | 8519699 | `motion.layer.lock_other_masks` |  |  |  | B | ✅ |
| Mask ▸ Hide Locked Masks | 8519700 | `motion.layer.hide_locked_masks` |  |  |  | B | ⏸ |
| Mask and Shape Path ▸ RotoBezier | 11010049 | `motion.layer.rotobezier` |  |  |  | A | ☐ |
| Mask and Shape Path ▸ Closed | 11010050 | `motion.layer.path_closed` |  |  |  | A | ☐ |
| Mask and Shape Path ▸ Convert To Bezier Path | 11010052 | `motion.layer.convert_to_bezier_path` |  |  |  | A | ☐ |
| Mask and Shape Path ▸ Group Shapes | 11010054 | `motion.layer.group_shapes` |  | Ctrl+G | Cmd+G | A | ☐ |
| Mask and Shape Path ▸ Ungroup Shapes | 11010055 | `motion.layer.ungroup_shapes` |  | Ctrl+Shift+G | Cmd+Shift+G | A | ☐ |
| Mask and Shape Path ▸ Set First Vertex | 11010057 | `motion.layer.set_first_vertex` |  |  |  | A | ☐ |
| Mask and Shape Path ▸ Free Transform Points | 11010059 | `motion.layer.free_transform_points` |  | Ctrl+T | Cmd+T | A | ☐ |
| Quality ▸ Best | 8585217 | `motion.layer.quality` | `{"quality":"best"}` | Ctrl+U | Cmd+U | B | ✅ |
| Quality ▸ Draft | 8585218 | `motion.layer.quality` | `{"quality":"draft"}` | Ctrl+Shift+U | Cmd+Shift+U | B | ✅ |
| Quality ▸ Wireframe | 8585219 | `motion.layer.quality` | `{"quality":"wireframe"}` | Ctrl+Alt+Shift+U | Cmd+Opt+Shift+U | B | ☐ |
| Quality ▸ Bilinear | 8585221 | `motion.layer.sampling` | `{"sampling":"bilinear"}` | Alt+B | Opt+B | B | ☐ |
| Quality ▸ Bicubic | 8585222 | `motion.layer.sampling` | `{"sampling":"bicubic"}` | Alt+Shift+B | Opt+Shift+B | B | ☐ |
| Switches ▸ Hide Other Video | 8847361 | `motion.layer.hide_other_video` |  |  |  | B | ✅ |
| Switches ▸ Show All Video | 8847362 | `motion.layer.show_all_video` |  |  |  | B | ✅ |
| Switches ▸ Unlock All Layers | 8847363 | `motion.layer.unlock_all_layers` |  | Ctrl+Shift+L | Cmd+Shift+L | B | ✅ |
| Switches ▸ Enable Expressions | 8847365 | `motion.layer.enable_expressions` |  |  |  | C | ⏸ |
| Switches ▸ Disable Expressions | 8847366 | `motion.layer.disable_expressions` |  |  |  | C | ⏸ |
| Switches ▸ Shy | 8847368 | `motion.layer.switch` | `{"switch":"shy"}` |  |  | B | ✅ |
| Switches ▸ Lock | 8847369 | `motion.layer.switch` | `{"switch":"locked"}` | Ctrl+L | Cmd+L | B | ✅ |
| Switches ▸ Audio | 8847370 | `motion.layer.switch` | `{"switch":"audio_enabled"}` |  |  | B | ✅ |
| Switches ▸ Video | 8847371 | `motion.layer.switch` | `{"switch":"enabled"}` | Ctrl+Alt+Shift+V | Cmd+Opt+Shift+V | B | ✅ |
| Switches ▸ Solo | 8847372 | `motion.layer.switch` | `{"switch":"solo"}` |  |  | B | ✅ |
| Switches ▸ Effect | 8847373 | `motion.layer.switch` | `{"switch":"effects_enabled"}` |  |  | B | ✅ |
| Switches ▸ Collapse | 8847374 | `motion.layer.switch` | `{"switch":"collapse_transformation"}` |  |  | B | ✅ |
| Switches ▸ Motion Blur | 8847375 | `motion.layer.switch` | `{"switch":"motion_blur"}` |  |  | B | ✅ |
| Switches ▸ Adjustment Layer | 8847376 | `motion.layer.switch` | `{"switch":"adjustment_layer"}` |  |  | B | ✅ |
| Transform ▸ Reset | 8912897 | `motion.layer.reset_transform` |  |  |  | B | ✅ |
| Transform ▸ Anchor Point... | 8912899 | `motion.layer.transform_dialog` | `{"attr":"anchor_point"}` |  |  | E | ✅ |
| Transform ▸ Position... | 8912900 | `motion.layer.transform_dialog` | `{"attr":"position"}` | Ctrl+Shift+P | Cmd+Shift+P | E | ✅ |
| Transform ▸ Scale... | 8912901 | `motion.layer.transform_dialog` | `{"attr":"scale"}` |  |  | E | ✅ |
| Transform ▸ Orientation... | 8912902 | `motion.layer.transform_dialog` | `{"attr":"orientation"}` | Ctrl+Alt+Shift+R | Cmd+Opt+Shift+R | E | ✅ |
| Transform ▸ Rotation... | 8912903 | `motion.layer.transform_dialog` | `{"attr":"rotation"}` | Ctrl+Shift+R | Cmd+Shift+R | E | ✅ |
| Transform ▸ Opacity... | 8912904 | `motion.layer.transform_dialog` | `{"attr":"opacity"}` | Ctrl+Shift+O | Cmd+Shift+O | E | ✅ |
| Transform ▸ Flip Horizontal | 8912906 | `motion.layer.flip_horizontal` |  |  |  | B | ✅ |
| Transform ▸ Flip Vertical | 8912907 | `motion.layer.flip_vertical` |  |  |  | B | ✅ |
| Transform ▸ Center In View | 8912908 | `motion.layer.center_in_view` |  | Ctrl+Home | Cmd+Home | B | ✅ |
| Transform ▸ Center Anchor Point in Layer Content | 8912909 | `motion.layer.center_anchor_point_in_layer_content` |  | Ctrl+Alt+Home | Cmd+Opt+Home | B | ✅ |
| Transform ▸ Fit to Comp | 8912911 | `motion.layer.fit_to_comp` |  | Ctrl+Alt+F | Cmd+Opt+F | B | ✅ |
| Transform ▸ Fit to Comp Width | 8912912 | `motion.layer.fit_to_comp_width` |  | Ctrl+Alt+Shift+H | Cmd+Opt+Shift+H | B | ✅ |
| Transform ▸ Fit to Comp Height | 8912913 | `motion.layer.fit_to_comp_height` |  | Ctrl+Alt+Shift+G | Cmd+Opt+Shift+G | B | ✅ |
| Transform ▸ Auto-Orient... | 8912915 | `motion.layer.auto_orient` |  | Ctrl+Alt+O | Cmd+Opt+O | E | ✅ |
| Time ▸ Enable Time Remapping | 1966081 | `motion.layer.enable_time_remapping` |  | Ctrl+Alt+T | Cmd+Opt+T | B | ✅ |
| Time ▸ Time-Reverse Layer | 1966082 | `motion.layer.time_reverse_layer` |  | Ctrl+Alt+R | Cmd+Opt+R | B | ✅ |
| Time ▸ Time Stretch... | 1966083 | `motion.layer.time_stretch` |  |  |  | E | ✅ |
| Time ▸ Freeze Frame | 1966084 | `motion.layer.freeze_frame` |  |  |  | B | ✅ |
| Time ▸ Freeze On Last Frame | 1966085 | `motion.layer.freeze_on_last_frame` |  |  |  | B | ✅ |
| Time ▸ Align Video to Data | 1966086 | `motion.layer.align_video_to_data` |  |  |  | B | ⏸ |
| Frame Blending ▸ Off | 10616833 | `motion.layer.frame_blending` | `{"mode":"off"}` |  |  | B | ⏸ |
| Frame Blending ▸ Frame Mix | 10616834 | `motion.layer.frame_blending` | `{"mode":"frame_mix"}` |  |  | B | ⏸ |
| Frame Blending ▸ Pixel Motion | 10616835 | `motion.layer.frame_blending` | `{"mode":"pixel_motion"}` |  |  | B | ⏸ |
| 3D Layer | 393231 | `motion.layer.switch` | `{"switch":"three_d"}` |  |  | B | ✅ |
| Guide Layer | 393232 | `motion.layer.switch` | `{"switch":"guide"}` |  |  | B | ✅ |
| Environment Layer | 393233 | `motion.layer.switch` | `{"switch":"environment"}` |  |  | B | ⏸ |
| Markers ▸ Add Marker | 23265281 | `motion.layer.add_marker` |  | PadMultiply | PadMultiply / Control+8 | C | ✅ |
| Markers ▸ Update Markers From Source | 23265282 | `motion.layer.update_markers_from_source` |  |  |  | C | ⏸ |
| Markers ▸ Lock Markers | 23265283 | `motion.layer.lock_markers` |  |  |  | C | ☐ |
| Markers ▸ Delete All Markers | 23265284 | `motion.layer.delete_all_markers` |  |  |  | C | ✅ |
| Preserve Transparency | 393236 | `motion.layer.switch` | `{"switch":"preserve_transparency"}` |  |  | B | ✅ |
| Blending Mode ▸ Normal | 16318465 | `motion.layer.blending_mode` | `{"mode":"normal"}` |  |  | B | ✅ |
| Blending Mode ▸ Dissolve | 16318466 | `motion.layer.blending_mode` | `{"mode":"dissolve"}` |  |  | B | ✅ |
| Blending Mode ▸ Dancing Dissolve | 16318467 | `motion.layer.blending_mode` | `{"mode":"dancing_dissolve"}` |  |  | B | ✅ |
| Blending Mode ▸ Darken | 16318469 | `motion.layer.blending_mode` | `{"mode":"darken"}` |  |  | B | ✅ |
| Blending Mode ▸ Multiply | 16318470 | `motion.layer.blending_mode` | `{"mode":"multiply"}` |  |  | B | ✅ |
| Blending Mode ▸ Color Burn | 16318471 | `motion.layer.blending_mode` | `{"mode":"color_burn"}` |  |  | B | ✅ |
| Blending Mode ▸ Classic Color Burn | 16318472 | `motion.layer.blending_mode` | `{"mode":"classic_color_burn"}` |  |  | B | ✅ |
| Blending Mode ▸ Linear Burn | 16318473 | `motion.layer.blending_mode` | `{"mode":"linear_burn"}` |  |  | B | ✅ |
| Blending Mode ▸ Darker Color | 16318474 | `motion.layer.blending_mode` | `{"mode":"darker_color"}` |  |  | B | ✅ |
| Blending Mode ▸ Add | 16318476 | `motion.layer.blending_mode` | `{"mode":"add"}` |  |  | B | ✅ |
| Blending Mode ▸ Lighten | 16318477 | `motion.layer.blending_mode` | `{"mode":"lighten"}` |  |  | B | ✅ |
| Blending Mode ▸ Screen | 16318478 | `motion.layer.blending_mode` | `{"mode":"screen"}` |  |  | B | ✅ |
| Blending Mode ▸ Color Dodge | 16318479 | `motion.layer.blending_mode` | `{"mode":"color_dodge"}` |  |  | B | ✅ |
| Blending Mode ▸ Classic Color Dodge | 16318480 | `motion.layer.blending_mode` | `{"mode":"classic_color_dodge"}` |  |  | B | ✅ |
| Blending Mode ▸ Linear Dodge | 16318481 | `motion.layer.blending_mode` | `{"mode":"linear_dodge"}` |  |  | B | ✅ |
| Blending Mode ▸ Lighter Color | 16318482 | `motion.layer.blending_mode` | `{"mode":"lighter_color"}` |  |  | B | ✅ |
| Blending Mode ▸ Overlay | 16318484 | `motion.layer.blending_mode` | `{"mode":"overlay"}` |  |  | B | ✅ |
| Blending Mode ▸ Soft Light | 16318485 | `motion.layer.blending_mode` | `{"mode":"soft_light"}` |  |  | B | ✅ |
| Blending Mode ▸ Hard Light | 16318486 | `motion.layer.blending_mode` | `{"mode":"hard_light"}` |  |  | B | ✅ |
| Blending Mode ▸ Linear Light | 16318487 | `motion.layer.blending_mode` | `{"mode":"linear_light"}` |  |  | B | ✅ |
| Blending Mode ▸ Vivid Light | 16318488 | `motion.layer.blending_mode` | `{"mode":"vivid_light"}` |  |  | B | ✅ |
| Blending Mode ▸ Pin Light | 16318489 | `motion.layer.blending_mode` | `{"mode":"pin_light"}` |  |  | B | ✅ |
| Blending Mode ▸ Hard Mix | 16318490 | `motion.layer.blending_mode` | `{"mode":"hard_mix"}` |  |  | B | ✅ |
| Blending Mode ▸ Difference | 16318492 | `motion.layer.blending_mode` | `{"mode":"difference"}` |  |  | B | ✅ |
| Blending Mode ▸ Classic Difference | 16318493 | `motion.layer.blending_mode` | `{"mode":"classic_difference"}` |  |  | B | ✅ |
| Blending Mode ▸ Exclusion | 16318494 | `motion.layer.blending_mode` | `{"mode":"exclusion"}` |  |  | B | ✅ |
| Blending Mode ▸ Subtract | 16318495 | `motion.layer.blending_mode` | `{"mode":"subtract"}` |  |  | B | ✅ |
| Blending Mode ▸ Divide | 16318496 | `motion.layer.blending_mode` | `{"mode":"divide"}` |  |  | B | ✅ |
| Blending Mode ▸ Hue | 16318498 | `motion.layer.blending_mode` | `{"mode":"hue"}` |  |  | B | ✅ |
| Blending Mode ▸ Saturation | 16318499 | `motion.layer.blending_mode` | `{"mode":"saturation"}` |  |  | B | ✅ |
| Blending Mode ▸ Color | 16318500 | `motion.layer.blending_mode` | `{"mode":"color"}` |  |  | B | ✅ |
| Blending Mode ▸ Luminosity | 16318501 | `motion.layer.blending_mode` | `{"mode":"luminosity"}` |  |  | B | ✅ |
| Blending Mode ▸ Stencil Alpha | 16318503 | `motion.layer.blending_mode` | `{"mode":"stencil_alpha"}` |  |  | B | ✅ |
| Blending Mode ▸ Stencil Luma | 16318504 | `motion.layer.blending_mode` | `{"mode":"stencil_luma"}` |  |  | B | ✅ |
| Blending Mode ▸ Silhouette Alpha | 16318505 | `motion.layer.blending_mode` | `{"mode":"silhouette_alpha"}` |  |  | B | ✅ |
| Blending Mode ▸ Silhouette Luma | 16318506 | `motion.layer.blending_mode` | `{"mode":"silhouette_luma"}` |  |  | B | ✅ |
| Blending Mode ▸ Alpha Add | 16318508 | `motion.layer.blending_mode` | `{"mode":"alpha_add"}` |  |  | B | ✅ |
| Blending Mode ▸ Luminescent Premul | 16318509 | `motion.layer.blending_mode` | `{"mode":"luminescent_premul"}` |  |  | B | ✅ |
| Next Blending Mode | 393238 | `motion.layer.blending_mode_step` | `{"step":1}` | Shift+= | Shift+= | B | ✅ |
| Previous Blending Mode | 393239 | `motion.layer.blending_mode_step` | `{"step":-1}` | Shift+- | Shift+- | B | ✅ |
| Track Matte ▸ No Track Matte | 16384001 | `motion.layer.track_matte` | `{"matte":"none"}` |  |  | B | ☐ |
| Track Matte ▸ Alpha Matte | 16384003 | `motion.layer.track_matte` | `{"matte":"alpha"}` |  |  | B | ☐ |
| Track Matte ▸ Alpha Inverted Matte | 16384004 | `motion.layer.track_matte` | `{"matte":"alpha_inverted"}` |  |  | B | ☐ |
| Track Matte ▸ Luma Matte | 16384005 | `motion.layer.track_matte` | `{"matte":"luma"}` |  |  | B | ☐ |
| Track Matte ▸ Luma Inverted Matte | 16384006 | `motion.layer.track_matte` | `{"matte":"luma_inverted"}` |  |  | B | ☐ |
| Track Matte ▸ Matte with Layer Above ▸ Alpha Matte | 23658497 | `motion.layer.track_matte` | `{"matte":"alpha","source":"above"}` |  |  | B | ☐ |
| Track Matte ▸ Matte with Layer Above ▸ Alpha Inverted Matte | 23658498 | `motion.layer.track_matte` | `{"matte":"alpha_inverted","source":"above"}` |  |  | B | ☐ |
| Track Matte ▸ Matte with Layer Above ▸ Luma Matte | 23658499 | `motion.layer.track_matte` | `{"matte":"luma","source":"above"}` |  |  | B | ☐ |
| Track Matte ▸ Matte with Layer Above ▸ Luma Inverted Matte | 23658500 | `motion.layer.track_matte` | `{"matte":"luma_inverted","source":"above"}` |  |  | B | ☐ |
| Track Matte ▸ Matte with Layer Below ▸ Alpha Matte | 23724033 | `motion.layer.track_matte` | `{"matte":"alpha","source":"below"}` |  |  | B | ☐ |
| Track Matte ▸ Matte with Layer Below ▸ Alpha Inverted Matte | 23724034 | `motion.layer.track_matte` | `{"matte":"alpha_inverted","source":"below"}` |  |  | B | ☐ |
| Track Matte ▸ Matte with Layer Below ▸ Luma Matte | 23724035 | `motion.layer.track_matte` | `{"matte":"luma","source":"below"}` |  |  | B | ☐ |
| Track Matte ▸ Matte with Layer Below ▸ Luma Inverted Matte | 23724036 | `motion.layer.track_matte` | `{"matte":"luma_inverted","source":"below"}` |  |  | B | ☐ |
| Arrange ▸ Bring to Front | 12910593 | `motion.order` | `{"to":"front"}` | Ctrl+Shift+] / Ctrl+Alt+Shift+Up / Ctrl+Shift+= | Cmd+Shift+] / Cmd+Opt+Shift+Up / Cmd+Shift+= | B | ✅ |
| Arrange ▸ Bring Forward | 12910594 | `motion.order` | `{"to":"forward"}` | Ctrl+] / Ctrl+Alt+Up | Cmd+] / Cmd+Opt+Up | B | ✅ |
| Arrange ▸ Send Backward | 12910595 | `motion.order` | `{"to":"backward"}` | Ctrl+[ / Ctrl+Alt+Down | Cmd+[ / Cmd+Opt+Down | B | ✅ |
| Arrange ▸ Send to Back | 12910596 | `motion.order` | `{"to":"back"}` | Ctrl+Shift+[ / Ctrl+Alt+Shift+Down / Ctrl+Shift+- | Cmd+Shift+[ / Cmd+Opt+Shift+Down / Cmd+Shift+- | B | ✅ |
| Reveal ▸ Reveal in Explorer | 23003137 | `motion.file.reveal_in_explorer` |  |  |  | B | ✅ |
| Reveal ▸ Reveal Layer Source in Project | 23003138 | `motion.layer.reveal_layer_source_in_project` |  |  |  | B | ☐ |
| Reveal ▸ Reveal Layer in Project Flowchart | 23003139 | `motion.layer.reveal_layer_in_project_flowchart` |  |  |  | E | ⏸ |
| Reveal ▸ Reveal Composition in Project | 23003140 | `motion.layer.reveal_composition_in_project` |  |  |  | B | ☐ |
| Reveal ▸ Reveal Expression Errors | 23003141 | `motion.layer.reveal_expression_errors` |  |  |  | C | ⏸ |
| Create ▸ Convert to Editable Text | 23068673 | `motion.layer.convert_to_editable_text` |  |  |  | B | ⏸ |
| Create ▸ Create Shapes from Text | 23068674 | `motion.layer.create_shapes_from_text` |  |  |  | A | ⏸ |
| Create ▸ Create Masks from Text | 23068675 | `motion.layer.create_masks_from_text` |  |  |  | A | ⏸ |
| Create ▸ Create Shapes from Vector Layer | 23068676 | `motion.layer.create_shapes_from_vector_layer` |  |  |  | A | ⏸ |
| Create ▸ Create Keyframes from Data | 23068677 | `motion.anim.create_keyframes_from_data` |  |  |  | B | ⏸ |
| Create ▸ Null Controllers for Positional Points | 23068678 | `motion.anim.null_controllers_for_positional_points` |  |  |  | B | ⏸ |
| Create ▸ Null Controllers for Path Points | 23068679 | `motion.anim.null_controllers_for_path_points` |  |  |  | B | ⏸ |
| Create ▸ Nulls Following Path Points | 23068680 | `motion.anim.nulls_following_path_points` |  |  |  | B | ⏸ |
| Create ▸ Null Tracing Along Path | 23068681 | `motion.anim.null_tracing_along_path` |  |  |  | B | ⏸ |
| Create ▸ Create 3D Layer Instance | 23068682 | `motion.layer.create_3d_layer_instance` |  |  |  | B | ⏸ |
| Camera ▸ Create Camera from 3D View | 21954561 | `motion.view.create_camera_from_3d_view` |  |  |  | A | ⏸ |
| Camera ▸ Create Stereo 3D Rig | 21954562 | `motion.layer.create_stereo_3d_rig` |  |  |  | B | ⏸ |
| Camera ▸ Create Orbit Null | 21954563 | `motion.layer.create_orbit_null` |  |  |  | B | ⏸ |
| Camera ▸ Create Cameras from 3D Model | 21954564 | `motion.layer.create_cameras_from_3d_model` |  |  |  | B | ⏸ |
| Camera ▸ Link Focus Distance to Point of Interest | 21954566 | `motion.layer.link_focus_distance_to_point_of_interest` |  |  |  | B | ⏸ |
| Camera ▸ Link Focus Distance to Layer | 21954567 | `motion.layer.link_focus_distance_to_layer` |  |  |  | B | ⏸ |
| Camera ▸ Set Focus Distance to Layer | 21954568 | `motion.layer.set_focus_distance_to_layer` |  |  |  | B | ⏸ |
| Camera ▸ Camera Settings... | 21954570 | `motion.layer.camera_settings` |  |  |  | E | ✅ |
| Camera ▸ Reset 3D View | 21954571 | `motion.view.reset_3d_view` |  |  |  | A | ⏸ |
| Light ▸ Create Lights from 3D Model | 328990721 | `motion.layer.create_lights_from_3d_model` |  |  |  | B | ⏸ |
| Light ▸ Control Light with Camera | 328990722 | `motion.layer.control_light_with_camera` |  |  |  | B | ⏸ |
| Light ▸ Create Environment Light Background Layer | 328990723 | `motion.layer.create_environment_light_background_layer` |  |  |  | B | ⏸ |
| Auto-trace... | 393250 | `motion.layer.auto_trace` |  |  |  | B | ⏸ |
| Pre-compose... | 393251 | `motion.layer.pre_compose` |  | Ctrl+Shift+C | Cmd+Shift+C | E | ✅ |
| Scene Edit Detection... | 393252 | `motion.layer.scene_edit_detection` |  |  |  | B | ⏸ |

### Effect

| AE 菜单路径 | AE id | BAE 动作 id | 参数 | Windows | macOS | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| Effect Controls | 458753 | `window.panel` | `{"id":"effect_controls","toggle":true}` | F3 / Ctrl+Shift+T | F3 / Cmd+Shift+T | B | ✅ |
| Last Effect | 458754 | `motion.effect.last_effect` |  | Ctrl+Alt+Shift+E | Cmd+Opt+Shift+E | B | ✅ |
| Remove All | 458755 | `motion.effect.remove_all` |  | Ctrl+Shift+E | Cmd+Shift+E | B | ✅ |
| Manage Effects... | 458757 | `motion.effect.manage_effects` |  |  |  | E | ✅ |
| 效果分类（3D Channel、Audio、Blur & Sharpen、Boris FX Mocha、Channel、Cinema 4D、Color Correction、Distort、Expression Controls、Generate、Immersive Video、Keying、Matte、Noise & Grain、Obsolete、Perspective、Simulation、Stylize、Text、Time、Transition、Utility） ▸ 各效果 | | `motion.effect.apply` | `{"match_name":"…"}` | | | B | ☐ |

### Animation

| AE 菜单路径 | AE id | BAE 动作 id | 参数 | Windows | macOS | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| Track in Boris FX Mocha | 851969 | `motion.anim.track_in_boris_fx_mocha` |  |  |  | B | Ⓐ |
| Save Animation Preset... | 851970 | `motion.anim.save_animation_preset` |  |  |  | E | ✅ |
| Apply Animation Preset... | 851971 | `motion.anim.apply_animation_preset` |  |  |  | E | ✅ |
| Recent Animation Presets ▸ (recent presets) | 11599873 | `motion.anim.recent_animation_preset` |  | Ctrl+Alt+Shift+F | Cmd+Opt+Shift+F | E | ⋯ |
| Browse Presets... | 851973 | `motion.anim.browse_presets` |  |  |  | E | ◐ |
| Set Keyframe | 851975 | `motion.anim.set_keyframe` |  |  |  | B | ☐ |
| Toggle Hold Keyframe | 851976 | `motion.toggle_hold` |  | Ctrl+Alt+H | Control+Cmd+Opt+H | B | ✅ |
| Keyframe Interpolation... | 851977 | `motion.anim.keyframe_interpolation` |  | Ctrl+Alt+K | Cmd+Opt+K | C | ✅ |
| Keyframe Velocity... | 851978 | `motion.anim.keyframe_velocity` |  | Ctrl+Shift+K | Cmd+Shift+K | C | ✅ |
| Keyframe Assistant ▸ RPF Camera Import | 11075585 | `motion.anim.rpf_camera_import` |  |  |  | B | Ⓐ |
| Keyframe Assistant ▸ Create Keyframes from Data | 11075586 | `motion.anim.create_keyframes_from_data` |  |  |  | B | ⏸ |
| Keyframe Assistant ▸ Null Controllers for Positional Points | 11075587 | `motion.anim.null_controllers_for_positional_points` |  |  |  | B | ⏸ |
| Keyframe Assistant ▸ Convert Expression to Keyframes | 11075588 | `motion.anim.convert_expression_to_keyframes` |  |  |  | C | ⏸ |
| Keyframe Assistant ▸ Convert Audio to Keyframes | 11075589 | `motion.anim.convert_audio_to_keyframes` |  |  |  | B | ✅ |
| Keyframe Assistant ▸ Sequence Layers... | 11075590 | `motion.anim.sequence_layers` |  |  |  | E | ✅ |
| Keyframe Assistant ▸ Exponential Scale | 11075591 | `motion.anim.exponential_scale` |  |  |  | B | ✅ |
| Keyframe Assistant ▸ Time-Reverse Keyframes | 11075592 | `motion.reverse_keys` |  |  |  | B | ✅ |
| Keyframe Assistant ▸ Null Tracing Along Path | 11075593 | `motion.anim.null_tracing_along_path` |  |  |  | B | ⏸ |
| Keyframe Assistant ▸ Nulls Following Path Points | 11075594 | `motion.anim.nulls_following_path_points` |  |  |  | B | ⏸ |
| Keyframe Assistant ▸ Easy Ease In | 11075595 | `motion.ease` | `{"mode":"ease_in"}` | Shift+F9 | Shift+F9 | B | ✅ |
| Keyframe Assistant ▸ Easy Ease Out | 11075596 | `motion.ease` | `{"mode":"ease_out"}` | Ctrl+Shift+F9 | Cmd+Shift+F9 | B | ✅ |
| Keyframe Assistant ▸ Easy Ease | 11075597 | `motion.ease` | `{"mode":"easy_ease"}` | F9 | F9 | B | ✅ |
| Keyframe Assistant ▸ Null Controllers for Path Points | 11075598 | `motion.anim.null_controllers_for_path_points` |  |  |  | B | ⏸ |
| Add Property to Essential Graphics | 851980 | `motion.anim.add_property_to_essential_graphics` |  |  |  | E | ⏸ |
| Animate Text ▸ Enable Per-character 3D | 1179649 | `motion.anim.enable_per_character_3d` |  |  |  | A | ⏸ |
| Animate Text ▸ Anchor Point | 1179651 | `motion.anim.animate_text` | `{"property":"anchor_point"}` |  |  | A | ⏸ |
| Animate Text ▸ Position | 1179652 | `motion.anim.animate_text` | `{"property":"position"}` |  |  | A | ✅ |
| Animate Text ▸ Scale | 1179653 | `motion.anim.animate_text` | `{"property":"scale"}` |  |  | A | ✅ |
| Animate Text ▸ Skew | 1179654 | `motion.anim.animate_text` | `{"property":"skew"}` |  |  | A | ⏸ |
| Animate Text ▸ Rotation | 1179655 | `motion.anim.animate_text` | `{"property":"rotation"}` |  |  | A | ✅ |
| Animate Text ▸ Opacity | 1179656 | `motion.anim.animate_text` | `{"property":"opacity"}` |  |  | A | ✅ |
| Animate Text ▸ All Transform Properties | 1179657 | `motion.anim.animate_text` | `{"property":"all_transform"}` |  |  | A | ⏸ |
| Animate Text ▸ Fill Color ▸ RGB | 1310721 | `motion.anim.animate_text` | `{"property":"fill_rgb"}` |  |  | A | ✅ |
| Animate Text ▸ Fill Color ▸ Hue | 1310722 | `motion.anim.animate_text` | `{"property":"fill_hue"}` |  |  | A | ⏸ |
| Animate Text ▸ Fill Color ▸ Saturation | 1310723 | `motion.anim.animate_text` | `{"property":"fill_saturation"}` |  |  | A | ⏸ |
| Animate Text ▸ Fill Color ▸ Brightness | 1310724 | `motion.anim.animate_text` | `{"property":"fill_brightness"}` |  |  | A | ⏸ |
| Animate Text ▸ Fill Color ▸ Opacity | 1310725 | `motion.anim.animate_text` | `{"property":"fill_opacity"}` |  |  | A | ⏸ |
| Animate Text ▸ Stroke Color ▸ RGB | 1376257 | `motion.anim.animate_text` | `{"property":"stroke_rgb"}` |  |  | A | ⏸ |
| Animate Text ▸ Stroke Color ▸ Hue | 1376258 | `motion.anim.animate_text` | `{"property":"stroke_hue"}` |  |  | A | ⏸ |
| Animate Text ▸ Stroke Color ▸ Saturation | 1376259 | `motion.anim.animate_text` | `{"property":"stroke_saturation"}` |  |  | A | ⏸ |
| Animate Text ▸ Stroke Color ▸ Brightness | 1376260 | `motion.anim.animate_text` | `{"property":"stroke_brightness"}` |  |  | A | ⏸ |
| Animate Text ▸ Stroke Color ▸ Opacity | 1376261 | `motion.anim.animate_text` | `{"property":"stroke_opacity"}` |  |  | A | ⏸ |
| Animate Text ▸ Stroke Width | 1179661 | `motion.anim.animate_text` | `{"property":"stroke_width"}` |  |  | A | ⏸ |
| Animate Text ▸ Tracking | 1179663 | `motion.anim.animate_text` | `{"property":"tracking"}` |  |  | A | ✅ |
| Animate Text ▸ Line Anchor | 1179664 | `motion.anim.animate_text` | `{"property":"line_anchor"}` |  |  | A | ⏸ |
| Animate Text ▸ Line Spacing | 1179665 | `motion.anim.animate_text` | `{"property":"line_spacing"}` |  |  | A | ⏸ |
| Animate Text ▸ Character Offset | 1179667 | `motion.anim.animate_text` | `{"property":"character_offset"}` |  |  | A | ⏸ |
| Animate Text ▸ Character Value | 1179668 | `motion.anim.animate_text` | `{"property":"character_value"}` |  |  | A | ⏸ |
| Animate Text ▸ Blur | 1179670 | `motion.anim.animate_text` | `{"property":"blur"}` |  |  | A | ⏸ |
| Add Text Selector ▸ Range | 1245185 | `motion.anim.add_text_selector` | `{"selector":"range"}` |  |  | A | ✅ |
| Add Text Selector ▸ Wiggly | 1245186 | `motion.anim.add_text_selector` | `{"selector":"wiggly"}` |  |  | A | ⏸ |
| Add Text Selector ▸ Expression | 1245187 | `motion.anim.add_text_selector` | `{"selector":"expression"}` |  |  | A | ⏸ |
| Remove All Text Animators | 851984 | `motion.anim.remove_all_text_animators` |  |  |  | A | ✅ |
| Add Expression | 851986 | `motion.anim.add_expression` |  | Alt+Shift+= | Opt+Shift+= | C | ☐ |
| Separate Dimensions | 851987 | `motion.anim.separate_dimensions` |  |  |  | B | ✅ |
| Track Camera | 851988 | `motion.anim.track_camera` |  |  |  | B | ⏸ |
| Warp Stabilizer VFX | 851989 | `motion.anim.warp_stabilizer_vfx` |  |  |  | B | ⏸ |
| Track Motion | 851990 | `motion.anim.track_motion` |  |  |  | B | ✅ |
| Track Mask | 851991 | `motion.anim.track_mask` |  |  |  | B | ⏸ |
| Track this Property | 851992 | `motion.anim.track_this_property` |  |  |  | B | ⏸ |
| Reveal Properties with Keyframes | 851994 | `motion.reveal` | `{"what":"animated","add":false}` | U | U | B | ✅ |
| Reveal Properties with Animation | 851995 | `motion.reveal` | `{"what":"animation","add":false}` |  |  | B | ✅ |
| Reveal All Modified Properties | 851996 | `motion.reveal` | `{"what":"modified","add":false}` |  |  | B | ✅ |

### View

| AE 菜单路径 | AE id | BAE 动作 id | 参数 | Windows | macOS | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| Split with New Locked Viewer | 589825 | `motion.view.split_with_new_locked_viewer` |  | Ctrl+Alt+Shift+N | Cmd+Opt+Shift+N | A | ⏸ |
| Zoom In | 589827 | `viewer.zoom_in` |  | . / Ctrl+Alt+= | . / Cmd+Opt+= | A | ✅ |
| Zoom Out | 589828 | `viewer.zoom_out` |  | , / Ctrl+Alt+- | , / Cmd+Opt+- | A | ✅ |
| Resolution ▸ Full | 8650753 | `motion.view.resolution` | `{"factor":1}` | Ctrl+J | Cmd+J | B | ☐ |
| Resolution ▸ Half | 8650754 | `motion.view.resolution` | `{"factor":2}` | Ctrl+Shift+J | Cmd+Shift+J | B | ☐ |
| Resolution ▸ Third | 8650755 | `motion.view.resolution` | `{"factor":3}` |  |  | B | ☐ |
| Resolution ▸ Quarter | 8650756 | `motion.view.resolution` | `{"factor":4}` | Ctrl+Alt+Shift+J | Cmd+Opt+Shift+J | B | ☐ |
| Resolution ▸ Custom... | 8650758 | `motion.view.resolution_custom` |  | Ctrl+Alt+J | Cmd+Opt+J | E | ⏸ |
| Use Display Color Management | 589832 | `motion.view.use_display_color_management` |  | Shift+PadSlash | Shift+PadSlash / Control+Opt+/ | B | ⏸ |
| Simulate Output ▸ No Output Simulation | 12517377 | `motion.view.simulate_output` | `{"profile":"none"}` |  |  | B | ⏸ |
| Simulate Output ▸ HDTV (Rec. 709) | 12517379 | `motion.view.simulate_output` | `{"profile":"rec709"}` |  |  | B | ⏸ |
| Simulate Output ▸ SDTV NTSC | 12517380 | `motion.view.simulate_output` | `{"profile":"ntsc"}` |  |  | B | ⏸ |
| Simulate Output ▸ SDTV PAL | 12517381 | `motion.view.simulate_output` | `{"profile":"pal"}` |  |  | B | ⏸ |
| Simulate Output ▸ Legacy Macintosh RGB (Gamma 1.8) | 12517382 | `motion.view.simulate_output` | `{"profile":"legacy_mac"}` |  |  | B | ⏸ |
| Simulate Output ▸ Internet Standard RGB (sRGB) | 12517383 | `motion.view.simulate_output` | `{"profile":"srgb"}` |  |  | B | ⏸ |
| Simulate Output ▸ Kodak 5218 to Kodak 2383 | 12517384 | `motion.view.simulate_output` | `{"profile":"kodak_5218_2383"}` |  |  | B | ⏸ |
| Simulate Output ▸ Universal Camera Film to Kodak 2383 | 12517385 | `motion.view.simulate_output` | `{"profile":"camera_film_2383"}` |  |  | B | ⏸ |
| Simulate Output ▸ My Custom RGB | 12517387 | `motion.view.simulate_output` | `{"profile":"custom_rgb"}` |  |  | B | ⏸ |
| Simulate Output ▸ Custom... | 12517388 | `motion.view.simulate_output_custom` |  |  |  | B | ⏸ |
| Show Rulers | 589835 | `motion.view.show_rulers` |  | Ctrl+R | Cmd+R | B | ☐ |
| Panel Background Color ▸ Black | 24051713 | `motion.view.panel_background_color` | `{"color":"black"}` |  |  | B | ⏸ |
| Panel Background Color ▸ Dark Gray | 24051714 | `motion.view.panel_background_color` | `{"color":"dark_gray"}` |  |  | B | ⏸ |
| Panel Background Color ▸ Medium Gray (Default) | 24051715 | `motion.view.panel_background_color` | `{"color":"medium_gray"}` |  |  | B | ⏸ |
| Panel Background Color ▸ Light Gray | 24051716 | `motion.view.panel_background_color` | `{"color":"light_gray"}` |  |  | B | ⏸ |
| Panel Background Color ▸ White | 24051717 | `motion.view.panel_background_color` | `{"color":"white"}` |  |  | B | ⏸ |
| Panel Background Color ▸ Custom | 24051718 | `motion.view.panel_background_color` | `{"color":"custom"}` |  |  | B | ⏸ |
| Panel Background Color ▸ Select Custom Background Color... | 24051720 | `motion.view.select_custom_background_color` |  |  |  | B | ⏸ |
| Show Guides | 589838 | `motion.view.show_guides` |  | Ctrl+; | Cmd+; | B | ☐ |
| Snap to Guides | 589839 | `motion.view.snap_to_guides` |  | Ctrl+Shift+; | Cmd+Shift+; | B | ☐ |
| Lock Guides | 589840 | `motion.view.lock_guides` |  | Ctrl+Alt+Shift+; | Cmd+Opt+Shift+; | B | ☐ |
| Clear Guides | 589841 | `motion.view.clear_guides` |  |  |  | B | ☐ |
| Import Guides... | 589842 | `motion.view.import_guides` |  |  |  | B | ⏸ |
| Export Guides... | 589843 | `motion.view.export_guides` |  |  |  | B | ⏸ |
| Show Grid | 589845 | `motion.view.show_grid` |  | Ctrl+' | Cmd+' | B | ☐ |
| Snap to Grid | 589846 | `motion.view.snap_to_grid` |  | Ctrl+Shift+' | Cmd+Shift+' | B | ☐ |
| View Options... | 589848 | `motion.view.view_options` |  | Ctrl+Alt+U | Cmd+Opt+U | E | ⏸ |
| Show Layer Controls | 589849 | `motion.view.show_layer_controls` |  | Ctrl+Shift+H | Cmd+Shift+H | B | ✅ |
| Camera Settings... | 589851 | `motion.layer.camera_settings` |  |  |  | E | ✅ |
| Reset 3D View | 589852 | `motion.view.reset_3d_view` |  |  |  | A | ⏸ |
| Create Camera from 3D View | 589853 | `motion.view.create_camera_from_3d_view` |  |  |  | A | ⏸ |
| Switch View Layout ▸ 1 View | 23396353 | `motion.view.view_layout` | `{"views":1}` |  |  | A | ⏸ |
| Switch View Layout ▸ 2 Views | 23396354 | `motion.view.view_layout` | `{"views":2}` |  |  | A | ⏸ |
| Switch View Layout ▸ 4 Views | 23396355 | `motion.view.view_layout` | `{"views":4}` |  |  | A | ⏸ |
| Switch View Layout ▸ Share View Options | 23396357 | `motion.view.share_view_options` |  |  |  | A | ⏸ |
| Switch 3D View ▸ Active Camera (Default) | 12058625 | `motion.view.switch_3d_view` | `{"view":"active_camera"}` |  |  | A | ⏸ |
| Switch 3D View ▸ Default | 12058627 | `motion.view.switch_3d_view` | `{"view":"default"}` |  |  | A | ⏸ |
| Switch 3D View ▸ Front | 12058629 | `motion.view.switch_3d_view` | `{"view":"front"}` |  |  | A | ⏸ |
| Switch 3D View ▸ Left | 12058630 | `motion.view.switch_3d_view` | `{"view":"left"}` |  |  | A | ⏸ |
| Switch 3D View ▸ Top | 12058631 | `motion.view.switch_3d_view` | `{"view":"top"}` |  |  | A | ⏸ |
| Switch 3D View ▸ Back | 12058632 | `motion.view.switch_3d_view` | `{"view":"back"}` |  |  | A | ⏸ |
| Switch 3D View ▸ Right | 12058633 | `motion.view.switch_3d_view` | `{"view":"right"}` |  |  | A | ⏸ |
| Switch 3D View ▸ Bottom | 12058634 | `motion.view.switch_3d_view` | `{"view":"bottom"}` |  |  | A | ⏸ |
| Switch 3D View ▸ Custom View 1 | 12058636 | `motion.view.switch_3d_view` | `{"view":"custom_1"}` |  |  | A | ⏸ |
| Switch 3D View ▸ Custom View 2 | 12058637 | `motion.view.switch_3d_view` | `{"view":"custom_2"}` |  |  | A | ⏸ |
| Switch 3D View ▸ Custom View 3 | 12058638 | `motion.view.switch_3d_view` | `{"view":"custom_3"}` |  |  | A | ⏸ |
| Switch 3D View ▸ Reset 3D View | 12058640 | `motion.view.reset_3d_view` |  |  |  | A | ⏸ |
| Assign 3D View Shortcut ▸ A | 11993089 | `motion.view.assign_3d_view_shortcut` | `{"slot":"a"}` |  |  | A | ⏸ |
| Assign 3D View Shortcut ▸ B | 11993090 | `motion.view.assign_3d_view_shortcut` | `{"slot":"b"}` |  |  | A | ⏸ |
| Assign 3D View Shortcut ▸ C | 11993091 | `motion.view.assign_3d_view_shortcut` | `{"slot":"c"}` |  |  | A | ⏸ |
| Switch to Last 3D View | 589857 | `motion.view.switch_to_last_3d_view` |  | Esc | Esc | A | ⏸ |
| Look at Selected Layers | 589858 | `motion.view.look_at_selected_layers` |  | Ctrl+Alt+Shift+\\ / F | Cmd+Opt+Shift+\\ / F | A | ⏸ |
| Look at All Layers | 589859 | `motion.view.look_at_all_layers` |  | Ctrl+Shift+F | Cmd+Shift+F | A | ⏸ |
| Go to Time... | 589861 | `motion.view.go_to_time` |  | Alt+Shift+J | Opt+Shift+J | B | ✅ |

### Window

| AE 菜单路径 | AE id | BAE 动作 id | 参数 | Windows | macOS | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| Workspace ▸ Animation | 9961473 | `workspace.switch` | `{"id":"ae_animation"}` |  |  | B | ✅ |
| Workspace ▸ Essential Graphics | 9961474 | `workspace.switch` | `{"id":"ae_essential_graphics"}` |  |  | B | ✅ |
| Workspace ▸ Learn | 9961475 | `workspace.switch` | `{"id":"ae_learn"}` | Shift+F12 | Shift+F12 | B | ✅ |
| Workspace ▸ Review | 9961476 | `workspace.switch` | `{"id":"review"}` | Shift+F11 | Shift+F11 | B | ✅ |
| Workspace ▸ Small Screen | 9961477 | `workspace.switch` | `{"id":"ae_small_screen"}` |  |  | B | ✅ |
| Workspace ▸ Libraries | 9961478 | `workspace.switch` | `{"id":"ae_libraries"}` |  |  | B | ✅ |
| Workspace ▸ All Panels | 9961479 | `workspace.switch` | `{"id":"ae_all_panels"}` |  |  | B | ✅ |
| Workspace ▸ Effects | 9961480 | `workspace.switch` | `{"id":"ae_effects"}` |  |  | B | ✅ |
| Workspace ▸ Text | 9961481 | `workspace.switch` | `{"id":"ae_text"}` |  |  | B | ✅ |
| Workspace ▸ Standard | 9961482 | `workspace.switch` | `{"id":"ae_standard"}` |  |  | B | ✅ |
| Workspace ▸ Undocked Panels | 9961483 | `workspace.switch` | `{"id":"ae_undocked_panels"}` |  |  | B | ✅ |
| Workspace ▸ Minimal | 9961484 | `workspace.switch` | `{"id":"ae_minimal"}` |  |  | B | ✅ |
| Workspace ▸ Paint | 9961485 | `workspace.switch` | `{"id":"ae_paint"}` |  |  | B | ✅ |
| Workspace ▸ Motion Tracking | 9961486 | `workspace.switch` | `{"id":"ae_motion_tracking"}` |  |  | B | ✅ |
| Workspace ▸ Color | 9961487 | `workspace.switch` | `{"id":"ae_color"}` |  |  | B | ✅ |
| Workspace ▸ Default | 9961488 | `workspace.switch` | `{"id":"motion"}` | Shift+F10 | Shift+F10 | B | ✅ |
| Workspace ▸ Reset to Saved Layout | 9961490 | `workspace.reset` |  |  |  | B | ✅ |
| Workspace ▸ Save Changes to this Workspace | 9961491 | `workspace.save` |  |  |  | B | ✅ |
| Workspace ▸ Save as New Workspace... | 9961492 | `workspace.new` |  |  |  | B | ✅ |
| Workspace ▸ Edit Workspaces... | 9961494 | `workspace.edit` |  |  |  | B | ✅ |
| Assign Shortcut to Workspace ▸ Shift+F10 | 12779521 | `motion.window.assign_workspace_shortcut` | `{"slot":0}` |  |  | B | ☐ |
| Assign Shortcut to Workspace ▸ Shift+F11 | 12779522 | `motion.window.assign_workspace_shortcut` | `{"slot":1}` |  |  | B | ☐ |
| Assign Shortcut to Workspace ▸ Shift+F12 | 12779523 | `motion.window.assign_workspace_shortcut` | `{"slot":2}` |  |  | B | ☐ |
| Find Extensions on Exchange... | 524292 | `motion.window.find_extensions_on_exchange` |  |  |  | B | Ⓐ |
| Extensions ▸ (extensions) |  | `motion.window.extension` |  |  |  | B | ⋯ |
| Lumetri Scopes | 524296 | `window.panel` | `{"id":"lumetri_scopes","toggle":true}` |  |  | E | ⏸ |
| Review with Frame.io | 524297 | `window.panel` | `{"id":"frame_io","toggle":true}` |  |  | B | Ⓐ |
| Info | 524298 | `window.panel` | `{"id":"info","toggle":true}` | Ctrl+2 | Cmd+2 | E | ☐ |
| Metadata | 524299 | `window.panel` | `{"id":"metadata","toggle":true}` |  |  | E | ⏸ |
| Content-Aware Fill | 524300 | `window.panel` | `{"id":"content_aware_fill","toggle":true}` |  |  | E | ⏸ |
| Motion Sketch | 524301 | `window.panel` | `{"id":"motion_sketch","toggle":true}` |  |  | E | ⏸ |
| Essential Graphics | 524302 | `window.panel` | `{"id":"essential_graphics","toggle":true}` |  |  | E | ⏸ |
| Media Browser | 524303 | `window.panel` | `{"id":"media_browser","toggle":true}` |  |  | E | ⏸ |
| Character | 524304 | `window.panel` | `{"id":"character","toggle":true}` | Ctrl+6 | Cmd+6 | E | ☐ |
| Learn | 524305 | `window.panel` | `{"id":"learn","toggle":true}` |  |  | E | ⏸ |
| Align | 524306 | `window.panel` | `{"id":"align","toggle":true}` |  |  | E | ☐ |
| Properties | 524307 | `window.panel` | `{"id":"properties","toggle":true}` |  |  | B | ✅ |
| Tools | 524308 | `window.panel` | `{"id":"tools","toggle":true}` | Ctrl+1 | Cmd+1 | A | ☐ |
| Smoother | 524309 | `window.panel` | `{"id":"smoother","toggle":true}` |  |  | E | ⏸ |
| Libraries | 524310 | `window.panel` | `{"id":"libraries","toggle":true}` |  |  | B | Ⓐ |
| Wiggler | 524311 | `window.panel` | `{"id":"wiggler","toggle":true}` |  |  | E | ⏸ |
| Effects & Presets | 524312 | `window.panel` | `{"id":"effects","toggle":true}` | Ctrl+5 | Cmd+5 | E | ✅ |
| Paragraph | 524313 | `window.panel` | `{"id":"paragraph","toggle":true}` | Ctrl+7 | Cmd+7 | E | ☐ |
| Brushes | 524314 | `window.panel` | `{"id":"brushes","toggle":true}` | Ctrl+9 | Cmd+9 | E | ⏸ |
| Paint | 524315 | `window.panel` | `{"id":"paint","toggle":true}` | Ctrl+8 | Cmd+8 | E | ⏸ |
| Mask Interpolation | 524316 | `window.panel` | `{"id":"mask_interpolation","toggle":true}` |  |  | E | ⏸ |
| Tracker | 524317 | `window.panel` | `{"id":"tracker","toggle":true}` |  |  | E | ⏸ |
| Progress | 524318 | `window.panel` | `{"id":"progress","toggle":true}` |  |  | E | ⏸ |
| Audio | 524319 | `window.panel` | `{"id":"audio","toggle":true}` | Ctrl+4 | Cmd+4 | E | ✅ |
| Preview | 524320 | `window.panel` | `{"id":"preview","toggle":true}` | Ctrl+3 | Cmd+3 | B | ✅ |
| Composition | 524322 | `window.panel` | `{"id":"composition","toggle":true}` |  |  | B | ✅ |
| Layer | 524323 | `window.panel` | `{"id":"layer","toggle":true}` |  |  | E | ⏸ |
| Effect Controls | 524324 | `window.panel` | `{"id":"effect_controls","toggle":true}` | F3 / Ctrl+Shift+T | F3 / Cmd+Shift+T | B | ✅ |
| Timeline | 524325 | `window.panel` | `{"id":"timeline","toggle":true}` |  |  | B | ✅ |
| Flowchart | 524326 | `window.panel` | `{"id":"flowchart","toggle":true}` |  |  | E | ⏸ |
| Render Queue | 524327 | `window.panel` | `{"id":"render_queue","toggle":true}` | Ctrl+Alt+0 | Cmd+Opt+0 | B | ✅ |
| Footage | 524328 | `window.panel` | `{"id":"footage","toggle":true}` |  |  | E | ⏸ |
| Project | 524329 | `window.panel` | `{"id":"project","toggle":true}` | Ctrl+0 | Cmd+0 | B | ✅ |
| Create Nulls From Paths.jsx（仅 Windows） | 524331 | `window.panel` | `{"id":"create_nulls_from_paths","toggle":true}` |  |  | B | ✅ |
| VR Comp Editor.jsx（仅 Windows） | 524338 | `window.panel` | `{"id":"vr_comp_editor","toggle":true}` |  |  | B | ✅ |

### Help

| AE 菜单路径 | AE id | BAE 动作 id | 参数 | Windows | macOS | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| About BAE...（仅 Windows） | 720897 | `app.about` |  |  |  | B | ✅ |
| BAE Help... | 720899 | `help.docs` |  | F1 | F1 | B | ✅ |
| BAE In-App Tutorials... | 720900 | `motion.help.in_app_tutorials` |  |  |  | B | ⏸ |
| BAE Online Tutorials... | 720901 | `motion.help.online_tutorials` |  |  |  | B | ⏸ |
| Scripting Help... | 720902 | `motion.help.scripting_help` |  |  |  | B | ⏸ |
| Expression Reference... | 720903 | `motion.help.expression_reference` |  |  |  | B | ⏸ |
| Effect Reference... | 720904 | `motion.help.effect_reference` |  |  |  | B | ⏸ |
| Animation Presets... | 720905 | `motion.help.animation_presets` |  |  |  | B | ⏸ |
| Keyboard Shortcuts... | 720906 | `motion.help.keyboard_shortcuts` |  |  |  | B | ☐ |
| System Compatibility Report... | 720908 | `motion.help.system_compatibility_report` |  |  |  | B | ⏸ |
| Enable Logging | 720909 | `motion.help.enable_logging` |  |  |  | B | ☐ |
| Reveal Logging File | 720910 | `motion.help.reveal_logging_file` |  |  |  | B | ☐ |
| Online User Forums... | 720912 | `motion.help.online_user_forums` |  |  |  | B | ⏸ |
| Provide Feedback... | 720913 | `motion.help.provide_feedback` |  |  |  | B | ☐ |

### macOS 应用菜单（BAE）

| AE 菜单路径 | AE id | BAE 动作 id | 参数 | Windows | macOS | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| About BAE... |  | `app.about` |  |  |  | B | ✅ |
| Settings ▸ Startup & Repair... |  | `app.settings` | `{"page":"startup_repair"}` | Ctrl+Alt+; | Cmd+Opt+; | D | ☐ |
| Settings ▸ Project... |  | `app.settings` | `{"page":"project"}` |  |  | D | ☐ |
| Settings ▸ Composition... |  | `app.settings` | `{"page":"composition"}` |  |  | D | ☐ |
| Settings ▸ Previews... |  | `app.settings` | `{"page":"previews"}` |  |  | D | ☐ |
| Settings ▸ Appearance... |  | `app.settings` | `{"page":"appearance"}` |  |  | D | ☐ |
| Settings ▸ Grids & Guides... |  | `app.settings` | `{"page":"grids_guides"}` |  |  | D | ☐ |
| Settings ▸ Labels... |  | `app.settings` | `{"page":"labels"}` |  |  | D | ☐ |
| Settings ▸ Type... |  | `app.settings` | `{"page":"type"}` |  |  | D | ☐ |
| Settings ▸ Import... |  | `app.settings` | `{"page":"import"}` |  |  | D | ☐ |
| Settings ▸ Export... |  | `app.settings` | `{"page":"export"}` |  |  | D | ☐ |
| Settings ▸ Audio... |  | `app.settings` | `{"page":"audio"}` |  |  | D | ☐ |
| Settings ▸ Disk... |  | `app.settings` | `{"page":"disk"}` |  |  | D | ☐ |
| Settings ▸ Memory & CPU... |  | `app.settings` | `{"page":"memory_cpu"}` |  |  | D | ☐ |
| Settings ▸ Video... |  | `app.settings` | `{"page":"video"}` |  |  | D | ☐ |
| Settings ▸ 3D... |  | `app.settings` | `{"page":"3d"}` |  |  | D | ☐ |
| Settings ▸ Scripting & Expressions... |  | `app.settings` | `{"page":"scripting_expressions"}` |  |  | D | ☐ |
| Services |  | `app.services` |  |  |  | B | ✅ |
| Hide BAE |  | `app.hide` |  |  | Cmd+H | B | ✅ |
| Hide Others |  | `app.hide_others` |  |  | Cmd+Opt+H | B | ✅ |
| Show All |  | `app.show_all` |  |  |  | B | ✅ |
| Quit BAE |  | `app.quit` |  | Ctrl+Q | Cmd+Q | B | ✅ |

## 只有快捷键的命令

AE 快捷键文件里不在主菜单上的命令（菜单上的命令见上表的快捷键列）。作用范围是 BAE 的面板种类，空 = 任何面板。

### `AE_TopLevelWindow`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `CloseTimelinePanelsToLeft` | `motion.timeline.close_timeline_panels_to_left` |  |  |  |  | C | ⏸ |
| `CloseTimelinePanelsToRight` | `motion.timeline.close_timeline_panels_to_right` |  |  |  |  | C | ⏸ |
| `SelectNextTab` | `motion.panel.select_next_tab` |  | Alt+Shift+. | Opt+Shift+. |  | B | ☐ |
| `SelectPrevTab` | `motion.panel.select_previous_tab` |  | Alt+Shift+, | Opt+Shift+, |  | B | ☐ |
| `ToggleTabPanelMaximize` | `window.maximize_frame` |  | \` | \` |  | B | ✅ |
| `ZoomWindowMega` | `motion.panel.maximize_application_window` |  | Ctrl+\\ | Cmd+\\ |  | B | ☐ |
| `ZoomWindowMegaMainMonitor` | `motion.panel.fit_application_window_to_main_monitor` |  | Ctrl+Alt+\\ | Cmd+Opt+\\ |  | B | ⏸ |

### `CCompCloneCmd`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `CloneSelectPreset1` | `motion.tool.clone_preset` | `{"preset":1}` | 3 | 3 | clone_tool | A | ⏸ |
| `CloneSelectPreset2` | `motion.tool.clone_preset` | `{"preset":2}` | 4 | 4 | clone_tool | A | ⏸ |
| `CloneSelectPreset3` | `motion.tool.clone_preset` | `{"preset":3}` | 5 | 5 | clone_tool | A | ⏸ |
| `CloneSelectPreset4` | `motion.tool.clone_preset` | `{"preset":4}` | 6 | 6 | clone_tool | A | ⏸ |
| `CloneSelectPreset5` | `motion.tool.clone_preset` | `{"preset":5}` | 7 | 7 | clone_tool | A | ⏸ |

### `CCompCmd`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `AddLayerMarkerWithDialog` | `motion.timeline.add_marker_with_dialog` |  | Alt+PadMultiply | Opt+PadMultiply / Control+Opt+8 |  | C | ✅ |
| `ClearMask` | `motion.layer.delete_mask_or_vertices` |  | Alt+Backspace / Alt+Delete | Opt+Backspace / Opt+Delete |  | A | ⏸ |
| `OpenSource` | `motion.layer.open_source` |  | PadEnter / Enter | PadEnter / Enter |  | B | ☐ |
| `PasteAlt` | `motion.edit.paste_at_current_time` |  | Ctrl+Alt+V | Cmd+Opt+V |  | B | ✅ |
| `SelectNext` | `motion.layer.select_next` | `{"add":false}` | Ctrl+Down | Cmd+Down |  | B | ✅ |
| `SelectNextAdd` | `motion.layer.select_next` | `{"add":true}` | Ctrl+Shift+Down | Cmd+Shift+Down |  | B | ✅ |
| `SelectPrevious` | `motion.layer.select_previous` | `{"add":false}` | Ctrl+Up | Cmd+Up |  | B | ✅ |
| `SelectPreviousAdd` | `motion.layer.select_previous` | `{"add":true}` | Ctrl+Shift+Up | Cmd+Shift+Up |  | B | ✅ |
| `SetWorkAreaEnd` | `motion.work_area_end` |  | N | N |  | B | ✅ |
| `SetWorkAreaStart` | `motion.work_area_start` |  | B | B |  | B | ✅ |
| `SetWorkAreaToSelection` | `motion.comp.set_work_area_to_selection` |  | Ctrl+Alt+B | Cmd+Opt+B |  | B | ✅ |

### `CCompCompCmd`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `CompOpenCompStuff` | `motion.panel.toggle_viewer_timeline` |  | \\ | \\ |  | B | ☐ |
| `CompScollSelectedLayerToTop` | `motion.timeline.scroll_selected_layer_to_top` |  | X | X |  | C | ✅ |
| `CompScrollToCurrentTime` | `motion.timeline.scroll_to_current_time` |  | D | D |  | C | ✅ |
| `CompSelectLayer1` | `motion.timeline.select_layer` | `{"index":1}` | Pad1 | Pad1 |  | B | ✅ |
| `CompSelectLayer2` | `motion.timeline.select_layer` | `{"index":2}` | Pad2 | Pad2 |  | B | ✅ |
| `CompSelectLayer3` | `motion.timeline.select_layer` | `{"index":3}` | Pad3 | Pad3 |  | B | ✅ |
| `CompSelectLayer4` | `motion.timeline.select_layer` | `{"index":4}` | Pad4 | Pad4 |  | B | ✅ |
| `CompSelectLayer5` | `motion.timeline.select_layer` | `{"index":5}` | Pad5 | Pad5 |  | B | ✅ |
| `CompSelectLayer6` | `motion.timeline.select_layer` | `{"index":6}` | Pad6 | Pad6 |  | B | ✅ |
| `CompSelectLayer7` | `motion.timeline.select_layer` | `{"index":7}` | Pad7 | Pad7 |  | B | ✅ |
| `CompSelectLayer8` | `motion.timeline.select_layer` | `{"index":8}` | Pad8 | Pad8 |  | B | ✅ |
| `CompSelectLayer9` | `motion.timeline.select_layer` | `{"index":9}` | Pad9 | Pad9 |  | B | ✅ |
| `CompTimeZoomFrames` | `motion.timeline.zoom_to_frames` |  | ; | ; |  | C | ✅ |
| `CompTimeZoomIn` | `timeline.zoom_in` |  | = | = |  | C | ✅ |
| `CompTimeZoomOut` | `timeline.zoom_out` |  | - | - |  | C | ✅ |
| `CompTimeZoomToggleFullCustom` | `motion.timeline.zoom_toggle_full` |  | Shift+; | Shift+; |  | C | ✅ |
| `CompToggleEffects` | `motion.reveal` | `{"what":"effects","add":false}` | E | E |  | B | ✅ |
| `CompToggleEffectsAddState` | `motion.reveal` | `{"what":"effects","add":true}` | Shift+E | Shift+E |  | B | ✅ |
| `CompToggleMaskFeathers` | `motion.reveal` | `{"what":"mask_feather","add":false}` | F | F |  | B | ✅ |
| `CompToggleMaskFeathersAddState` | `motion.reveal` | `{"what":"mask_feather","add":true}` | Shift+F | Shift+F |  | B | ✅ |
| `CompToggleMaskFeathersAddStateNewKF` | `motion.toggle_key` | `{"attr":"mask_feather"}` | Alt+Shift+F | Opt+Shift+F |  | B | ✅ |
| `CompToggleMaskFeathersNewKF` | `motion.toggle_key` | `{"attr":"mask_feather","only":true}` |  | Opt+F |  | B | ☐ |
| `CompToggleMaskShapes` | `motion.reveal` | `{"what":"mask_path","add":false}` | M | M |  | B | ✅ |
| `CompToggleMaskShapesAddState` | `motion.reveal` | `{"what":"mask_path","add":true}` | Shift+M | Shift+M |  | B | ✅ |
| `CompToggleMaskShapesAddStateNewKF` | `motion.toggle_key` | `{"attr":"mask_path"}` | Alt+Shift+M | Opt+Shift+M |  | B | ✅ |
| `CompToggleMaskShapesNewKF` | `motion.toggle_key` | `{"attr":"mask_path","only":true}` |  | Opt+M |  | B | ☐ |
| `CompToggleSelectLayer1` | `motion.timeline.select_layer` | `{"index":1,"toggle":true}` | Shift+Pad1 | Shift+Pad1 |  | B | ✅ |
| `CompToggleSelectLayer2` | `motion.timeline.select_layer` | `{"index":2,"toggle":true}` | Shift+Pad2 | Shift+Pad2 |  | B | ✅ |
| `CompToggleSelectLayer3` | `motion.timeline.select_layer` | `{"index":3,"toggle":true}` | Shift+Pad3 | Shift+Pad3 |  | B | ✅ |
| `CompToggleSelectLayer4` | `motion.timeline.select_layer` | `{"index":4,"toggle":true}` | Shift+Pad4 | Shift+Pad4 |  | B | ✅ |
| `CompToggleSelectLayer5` | `motion.timeline.select_layer` | `{"index":5,"toggle":true}` | Shift+Pad5 | Shift+Pad5 |  | B | ✅ |
| `CompToggleSelectLayer6` | `motion.timeline.select_layer` | `{"index":6,"toggle":true}` | Shift+Pad6 | Shift+Pad6 |  | B | ✅ |
| `CompToggleSelectLayer7` | `motion.timeline.select_layer` | `{"index":7,"toggle":true}` | Shift+Pad7 | Shift+Pad7 |  | B | ✅ |
| `CompToggleSelectLayer8` | `motion.timeline.select_layer` | `{"index":8,"toggle":true}` | Shift+Pad8 | Shift+Pad8 |  | B | ✅ |
| `CompToggleSelectLayer9` | `motion.timeline.select_layer` | `{"index":9,"toggle":true}` | Shift+Pad9 | Shift+Pad9 |  | B | ✅ |
| `CompToggleUberAnimatingKeyframesAddState` | `motion.reveal` | `{"what":"animated","add":true}` | Shift+U | Shift+U |  | B | ✅ |
| `CompToggleUberAnimatingKeyframesAndExpressionsAddState` | `motion.reveal` | `{"what":"animation","add":true}` |  |  |  | B | ✅ |
| `CompTwirlAnchorPoint` | `motion.reveal` | `{"what":"transform:anchor_point","add":false}` | A | A |  | B | ✅ |
| `CompTwirlAnchorPointAddState` | `motion.reveal` | `{"what":"transform:anchor_point","add":true}` | Shift+A | Shift+A |  | B | ✅ |
| `CompTwirlAnchorPointAddStateNewKF` | `motion.toggle_key` | `{"attr":"transform:anchor_point"}` | Alt+Shift+A | Opt+Shift+A |  | B | ✅ |
| `CompTwirlAnchorPointNewKF` | `motion.toggle_key` | `{"attr":"transform:anchor_point","only":true}` |  | Opt+A |  | B | ☐ |
| `CompTwirlAudio` | `motion.reveal` | `{"what":"audio:levels","add":false}` | L | L |  | B | ✅ |
| `CompTwirlAudioAddState` | `motion.reveal` | `{"what":"audio:levels","add":true}` | Shift+L | Shift+L |  | B | ✅ |
| `CompTwirlAudioAddStateNewKF` | `motion.toggle_key` | `{"attr":"audio:levels"}` | Alt+Shift+L | Opt+Shift+L |  | B | ✅ |
| `CompTwirlAudioNewKF` | `motion.toggle_key` | `{"attr":"audio:levels","only":true}` |  | Opt+L |  | B | ☐ |
| `CompTwirlOpacity` | `motion.reveal` | `{"what":"transform:opacity","add":false}` | T | T |  | B | ✅ |
| `CompTwirlOpacityAddState` | `motion.reveal` | `{"what":"transform:opacity","add":true}` | Shift+T | Shift+T |  | B | ✅ |
| `CompTwirlOpacityAddStateNewKF` | `motion.toggle_key` | `{"attr":"transform:opacity"}` | Alt+Shift+T | Opt+Shift+T |  | B | ✅ |
| `CompTwirlOpacityNewKF` | `motion.toggle_key` | `{"attr":"transform:opacity","only":true}` |  | Opt+T |  | B | ☐ |
| `CompTwirlPosition` | `motion.reveal` | `{"what":"transform:position","add":false}` | P | P |  | B | ✅ |
| `CompTwirlPositionAddState` | `motion.reveal` | `{"what":"transform:position","add":true}` | Shift+P | Shift+P |  | B | ✅ |
| `CompTwirlPositionAddStateNewKF` | `motion.toggle_key` | `{"attr":"transform:position"}` | Alt+Shift+P | Opt+Shift+P |  | B | ✅ |
| `CompTwirlPositionNewKF` | `motion.toggle_key` | `{"attr":"transform:position","only":true}` |  | Opt+P |  | B | ☐ |
| `CompTwirlRotation` | `motion.reveal` | `{"what":"transform:rotation","add":false}` | R | R |  | B | ✅ |
| `CompTwirlRotationAddState` | `motion.reveal` | `{"what":"transform:rotation","add":true}` | Shift+R | Shift+R |  | B | ✅ |
| `CompTwirlRotationAddStateNewKF` | `motion.toggle_key` | `{"attr":"transform:rotation"}` | Alt+Shift+R | Opt+Shift+R |  | B | ✅ |
| `CompTwirlRotationNewKF` | `motion.toggle_key` | `{"attr":"transform:rotation","only":true}` |  | Opt+R |  | B | ☐ |
| `CompTwirlScale` | `motion.reveal` | `{"what":"transform:scale","add":false}` | S | S |  | B | ✅ |
| `CompTwirlScaleAddState` | `motion.reveal` | `{"what":"transform:scale","add":true}` | Shift+S | Shift+S |  | B | ✅ |
| `CompTwirlScaleAddStateNewKF` | `motion.toggle_key` | `{"attr":"transform:scale"}` | Alt+Shift+S | Opt+Shift+S |  | B | ✅ |
| `CompTwirlScaleNewKF` | `motion.toggle_key` | `{"attr":"transform:scale","only":true}` |  | Opt+S |  | B | ☐ |
| `NextKeyframe` | `motion.next_key` |  | K | K |  | B | ✅ |
| `NextKeyframeInSelection` | `motion.timeline.next_keyframe_in_selection` |  | Shift+K | Shift+K |  | C | ✅ |
| `NudgeDown` | `motion.layer.nudge_position` | `{"dx":0,"dy":1}` | Down | Down |  | A | ☐ |
| `NudgeDownMore` | `motion.layer.nudge_position` | `{"dx":0,"dy":10}` | Shift+Down | Shift+Down |  | A | ☐ |
| `NudgeEarlier` | `motion.layer.nudge_time` | `{"frames":-1}` | Alt+PageUp | Opt+PageUp |  | B | ✅ |
| `NudgeEarlierMore` | `motion.layer.nudge_time` | `{"frames":-10}` | Alt+Shift+PageUp | Opt+Shift+PageUp |  | B | ✅ |
| `NudgeKeyframesEarlier` | `motion.timeline.nudge_keyframes` | `{"frames":-1}` | Alt+Left | Opt+Left |  | C | ✅ |
| `NudgeKeyframesEarlierMore` | `motion.timeline.nudge_keyframes` | `{"frames":-10}` | Alt+Shift+Left | Opt+Shift+Left |  | C | ✅ |
| `NudgeKeyframesLater` | `motion.timeline.nudge_keyframes` | `{"frames":1}` | Alt+Right | Opt+Right |  | C | ✅ |
| `NudgeKeyframesLaterMore` | `motion.timeline.nudge_keyframes` | `{"frames":10}` | Alt+Shift+Right | Opt+Shift+Right |  | C | ✅ |
| `NudgeLater` | `motion.layer.nudge_time` | `{"frames":1}` | Alt+PageDown | Opt+PageDown |  | B | ✅ |
| `NudgeLaterMore` | `motion.layer.nudge_time` | `{"frames":10}` | Alt+Shift+PageDown | Opt+Shift+PageDown |  | B | ✅ |
| `NudgeLeft` | `motion.layer.nudge_position` | `{"dx":-1,"dy":0}` | Left | Left |  | A | ☐ |
| `NudgeLeftMore` | `motion.layer.nudge_position` | `{"dx":-10,"dy":0}` | Shift+Left | Shift+Left |  | A | ☐ |
| `NudgeOpacityDown` | `motion.layer.nudge_opacity` | `{"delta":-1}` | Ctrl+Alt+PadMinus | Control+Opt+PadMinus |  | B | ✅ |
| `NudgeOpacityDownMore` | `motion.layer.nudge_opacity` | `{"delta":-10}` | Ctrl+Alt+Shift+PadMinus | Control+Opt+Shift+PadMinus |  | B | ✅ |
| `NudgeOpacityUp` | `motion.layer.nudge_opacity` | `{"delta":1}` | Ctrl+Alt+PadPlus | Control+Opt+PadPlus |  | B | ✅ |
| `NudgeOpacityUpMore` | `motion.layer.nudge_opacity` | `{"delta":10}` | Ctrl+Alt+Shift+PadPlus | Control+Opt+Shift+PadPlus |  | B | ✅ |
| `NudgeRight` | `motion.layer.nudge_position` | `{"dx":1,"dy":0}` | Right | Right |  | A | ☐ |
| `NudgeRightMore` | `motion.layer.nudge_position` | `{"dx":10,"dy":0}` | Shift+Right | Shift+Right |  | A | ☐ |
| `NudgeRotCCW` | `motion.layer.nudge_rotation` | `{"delta":-1}` | PadMinus | PadMinus |  | B | ✅ |
| `NudgeRotCCWMore` | `motion.layer.nudge_rotation` | `{"delta":-10}` | Shift+PadMinus | Shift+PadMinus |  | B | ✅ |
| `NudgeRotCW` | `motion.layer.nudge_rotation` | `{"delta":1}` | PadPlus | PadPlus |  | B | ✅ |
| `NudgeRotCWMore` | `motion.layer.nudge_rotation` | `{"delta":10}` | Shift+PadPlus | Shift+PadPlus |  | B | ✅ |
| `NudgeScaleLarger` | `motion.layer.nudge_scale` | `{"delta":1}` | Alt+PadPlus / Ctrl+PadPlus | Opt+PadPlus / Cmd+PadPlus |  | B | ✅ |
| `NudgeScaleLargerMore` | `motion.layer.nudge_scale` | `{"delta":10}` | Alt+Shift+PadPlus / Ctrl+Shift+PadPlus | Opt+Shift+PadPlus / Cmd+Shift+PadPlus |  | B | ✅ |
| `NudgeScaleSmaller` | `motion.layer.nudge_scale` | `{"delta":-1}` | Alt+PadMinus / Ctrl+PadMinus | Opt+PadMinus / Cmd+PadMinus |  | B | ✅ |
| `NudgeScaleSmallerMore` | `motion.layer.nudge_scale` | `{"delta":-10}` | Alt+Shift+PadMinus / Ctrl+Shift+PadMinus | Opt+Shift+PadMinus / Cmd+Shift+PadMinus |  | B | ✅ |
| `NudgeUp` | `motion.layer.nudge_position` | `{"dx":0,"dy":-1}` | Up | Up |  | A | ☐ |
| `NudgeUpMore` | `motion.layer.nudge_position` | `{"dx":0,"dy":-10}` | Shift+Up | Shift+Up |  | A | ☐ |
| `PrevKeyframe` | `motion.prev_key` |  | J | J |  | B | ✅ |
| `PrevKeyframeInSelection` | `motion.timeline.previous_keyframe_in_selection` |  | Shift+J | Shift+J |  | C | ✅ |
| `SelectAllVisibleKeyframes` | `motion.timeline.select_all_visible_keyframes` |  | Ctrl+Alt+A | Cmd+Opt+A |  | C | ✅ |
| `TemporalAlignEnd` | `motion.layer.move_out_point_to_comp_end` |  | Alt+End | Opt+End |  | B | ✅ |
| `TemporalAlignStart` | `motion.layer.move_in_point_to_comp_start` |  | Alt+Home | Opt+Home |  | B | ✅ |
| `ToggleGraph` | `motion.timeline.graph_editor` |  | Shift+F3 | Shift+F3 |  | C | ✅ |
| `ToggleTlwParentColumn` | `motion.timeline.toggle_parent_column` |  | Shift+F4 | Shift+F4 |  | C | ✅ |
| `Twirl` | `motion.timeline.twirl` | `{"explode":false,"preserve_solo":false}` |  |  |  | C | ⏸ |
| `TwirlExplode` | `motion.timeline.twirl` | `{"explode":true,"preserve_solo":false}` | Ctrl+\` | Control+\` |  | C | ✅ |
| `TwirlPreserveSolo` | `motion.timeline.twirl` | `{"explode":false,"preserve_solo":true}` | Shift+\` | Shift+\` |  | C | ☐ |
| `TwirlPreserveSoloExplode` | `motion.timeline.twirl` | `{"explode":true,"preserve_solo":true}` | Ctrl+Shift+\` | Control+Shift+\` |  | C | ☐ |
| `CompTimeZoomWorkArea`（只在 macOS 26.2 的快捷键文件里） | `motion.timeline.zoom_to_work_area` |  |  | Opt+; |  | C | ☐ |

### `CCompMarkerCmd`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `CompGotoMarker0` | `motion.timeline.go_to_comp_marker` | `{"number":0}` | 0 | 0 | timeline | C | ✅ |
| `CompGotoMarker1` | `motion.timeline.go_to_comp_marker` | `{"number":1}` | 1 | 1 | timeline | C | ✅ |
| `CompGotoMarker2` | `motion.timeline.go_to_comp_marker` | `{"number":2}` | 2 | 2 | timeline | C | ✅ |
| `CompGotoMarker3` | `motion.timeline.go_to_comp_marker` | `{"number":3}` | 3 | 3 | timeline | C | ✅ |
| `CompGotoMarker4` | `motion.timeline.go_to_comp_marker` | `{"number":4}` | 4 | 4 | timeline | C | ✅ |
| `CompGotoMarker5` | `motion.timeline.go_to_comp_marker` | `{"number":5}` | 5 | 5 | timeline | C | ✅ |
| `CompGotoMarker6` | `motion.timeline.go_to_comp_marker` | `{"number":6}` | 6 | 6 | timeline | C | ✅ |
| `CompGotoMarker7` | `motion.timeline.go_to_comp_marker` | `{"number":7}` | 7 | 7 | timeline | C | ✅ |
| `CompGotoMarker8` | `motion.timeline.go_to_comp_marker` | `{"number":8}` | 8 | 8 | timeline | C | ✅ |
| `CompGotoMarker9` | `motion.timeline.go_to_comp_marker` | `{"number":9}` | 9 | 9 | timeline | C | ✅ |
| `CompMarker0` | `motion.timeline.set_comp_marker` | `{"number":0}` | Shift+0 | Shift+0 | timeline | C | ✅ |
| `CompMarker1` | `motion.timeline.set_comp_marker` | `{"number":1}` | Shift+1 | Shift+1 | timeline | C | ✅ |
| `CompMarker2` | `motion.timeline.set_comp_marker` | `{"number":2}` | Shift+2 | Shift+2 | timeline | C | ✅ |
| `CompMarker3` | `motion.timeline.set_comp_marker` | `{"number":3}` | Shift+3 | Shift+3 | timeline | C | ✅ |
| `CompMarker4` | `motion.timeline.set_comp_marker` | `{"number":4}` | Shift+4 | Shift+4 | timeline | C | ✅ |
| `CompMarker5` | `motion.timeline.set_comp_marker` | `{"number":5}` | Shift+5 | Shift+5 | timeline | C | ✅ |
| `CompMarker6` | `motion.timeline.set_comp_marker` | `{"number":6}` | Shift+6 | Shift+6 | timeline | C | ✅ |
| `CompMarker7` | `motion.timeline.set_comp_marker` | `{"number":7}` | Shift+7 | Shift+7 | timeline | C | ✅ |
| `CompMarker8` | `motion.timeline.set_comp_marker` | `{"number":8}` | Shift+8 | Shift+8 | timeline | C | ✅ |
| `CompMarker9` | `motion.timeline.set_comp_marker` | `{"number":9}` | Shift+9 | Shift+9 | timeline | C | ✅ |

### `CCompPaintCmd`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `PaintColorResetBW` | `motion.tool.paint_reset_colors` |  | D | D | paint_tool | A | ⏸ |
| `PaintColorSwapFgBg` | `motion.tool.paint_swap_colors` |  | X | X | paint_tool | A | ⏸ |
| `PaintSetFlow10` | `motion.tool.paint_flow` | `{"percent":10}` | Shift+Pad1 | Shift+Pad1 | paint_tool | A | ⏸ |
| `PaintSetFlow100` | `motion.tool.paint_flow` | `{"percent":100}` | Shift+PadDecimal / Shift+PadComma / Shift+PadDelete | Shift+PadDecimal / Shift+PadComma / Shift+PadDelete | paint_tool | A | ⏸ |
| `PaintSetFlow20` | `motion.tool.paint_flow` | `{"percent":20}` | Shift+Pad2 | Shift+Pad2 | paint_tool | A | ⏸ |
| `PaintSetFlow30` | `motion.tool.paint_flow` | `{"percent":30}` | Shift+Pad3 | Shift+Pad3 | paint_tool | A | ⏸ |
| `PaintSetFlow40` | `motion.tool.paint_flow` | `{"percent":40}` | Shift+Pad4 | Shift+Pad4 | paint_tool | A | ⏸ |
| `PaintSetFlow50` | `motion.tool.paint_flow` | `{"percent":50}` | Shift+Pad5 | Shift+Pad5 | paint_tool | A | ⏸ |
| `PaintSetFlow60` | `motion.tool.paint_flow` | `{"percent":60}` | Shift+Pad6 | Shift+Pad6 | paint_tool | A | ⏸ |
| `PaintSetFlow70` | `motion.tool.paint_flow` | `{"percent":70}` | Shift+Pad7 | Shift+Pad7 | paint_tool | A | ⏸ |
| `PaintSetFlow80` | `motion.tool.paint_flow` | `{"percent":80}` | Shift+Pad8 | Shift+Pad8 | paint_tool | A | ⏸ |
| `PaintSetFlow90` | `motion.tool.paint_flow` | `{"percent":90}` | Shift+Pad9 | Shift+Pad9 | paint_tool | A | ⏸ |
| `PaintSetOpacity10` | `motion.tool.paint_opacity` | `{"percent":10}` | Pad1 | Pad1 | paint_tool | A | ⏸ |
| `PaintSetOpacity100` | `motion.tool.paint_opacity` | `{"percent":100}` | PadDecimal / PadComma / PadDelete | PadDecimal / PadComma / PadDelete | paint_tool | A | ⏸ |
| `PaintSetOpacity20` | `motion.tool.paint_opacity` | `{"percent":20}` | Pad2 | Pad2 | paint_tool | A | ⏸ |
| `PaintSetOpacity30` | `motion.tool.paint_opacity` | `{"percent":30}` | Pad3 | Pad3 | paint_tool | A | ⏸ |
| `PaintSetOpacity40` | `motion.tool.paint_opacity` | `{"percent":40}` | Pad4 | Pad4 | paint_tool | A | ⏸ |
| `PaintSetOpacity50` | `motion.tool.paint_opacity` | `{"percent":50}` | Pad5 | Pad5 | paint_tool | A | ⏸ |
| `PaintSetOpacity60` | `motion.tool.paint_opacity` | `{"percent":60}` | Pad6 | Pad6 | paint_tool | A | ⏸ |
| `PaintSetOpacity70` | `motion.tool.paint_opacity` | `{"percent":70}` | Pad7 | Pad7 | paint_tool | A | ⏸ |
| `PaintSetOpacity80` | `motion.tool.paint_opacity` | `{"percent":80}` | Pad8 | Pad8 | paint_tool | A | ⏸ |
| `PaintSetOpacity90` | `motion.tool.paint_opacity` | `{"percent":90}` | Pad9 | Pad9 | paint_tool | A | ⏸ |
| `PaintTimeStepBack` | `motion.tool.paint_step` | `{"frames":-1}` | 1 / Ctrl+PageUp | 1 / Cmd+PageUp | paint_tool | A | ⏸ |
| `PaintTimeStepForward` | `motion.tool.paint_step` | `{"frames":1}` | 2 / Ctrl+PageDown | 2 / Cmd+PageDown | paint_tool | A | ⏸ |

### `CCompTime`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `TimeFastForward` | `motion.go_end` |  | Ctrl+Alt+Right / End | Cmd+Opt+Right / End |  | B | ✅ |
| `TimeJumpToIn` | `motion.go_in` |  | I | I |  | B | ✅ |
| `TimeJumpToNextInOut` | `motion.comp.go_to_next_in_or_out_point` |  | Ctrl+Alt+Shift+Right | Cmd+Opt+Shift+Right |  | B | ✅ |
| `TimeJumpToOut` | `motion.go_out` |  | O | O |  | B | ✅ |
| `TimeJumpToPrevInOut` | `motion.comp.go_to_previous_in_or_out_point` |  | Ctrl+Alt+Shift+Left | Cmd+Opt+Shift+Left |  | B | ✅ |
| `TimeJumpToWAEnd` | `motion.comp.go_to_work_area_end` |  | Shift+End | Shift+End |  | B | ✅ |
| `TimeJumpToWAStart` | `motion.comp.go_to_work_area_start` |  | Shift+Home | Shift+Home |  | B | ✅ |
| `TimeRewind` | `motion.go_start` |  | Ctrl+Alt+Left / Home | Cmd+Opt+Left / Home |  | B | ✅ |
| `TimeSetIn` | `motion.move_in` |  | [ | [ |  | B | ✅ |
| `TimeSetInRelative` | `motion.move_in` | `{"relative":true}` | Shift+[ | Shift+[ |  | B | ✅ |
| `TimeSetOut` | `motion.move_out` |  | ] | ] |  | B | ✅ |
| `TimeSetOutRelative` | `motion.move_out` | `{"relative":true}` | Shift+] | Shift+] |  | B | ✅ |
| `TimeShiftSpacebarPlay` | `motion.play` | `{"alternate":true}` | Shift+Space | Shift+Space |  | B | ✅ |
| `TimeStepBack` | `motion.step` | `{"frames":-1}` | Ctrl+Left / PageUp | Cmd+Left / PageUp |  | B | ✅ |
| `TimeStepBackMore` | `motion.step` | `{"frames":-10}` | Shift+PageUp / Ctrl+Shift+Left | Shift+PageUp / Cmd+Shift+Left |  | B | ✅ |
| `TimeStepForward` | `motion.step` | `{"frames":1}` | Ctrl+Right / PageDown | Cmd+Right / PageDown |  | B | ✅ |
| `TimeStepForwardMore` | `motion.step` | `{"frames":10}` | Shift+PageDown / Ctrl+Shift+Right | Shift+PageDown / Cmd+Shift+Right |  | B | ✅ |
| `TimeStretchIn` | `motion.timeline.stretch_in_point_to_current_time` |  | Ctrl+Shift+, | Cmd+Shift+, |  | C | ✅ |
| `TimeStretchOut` | `motion.timeline.stretch_out_point_to_current_time` |  | Ctrl+Alt+, | Cmd+Opt+, |  | C | ✅ |
| `TimeTrimIn` | `motion.trim_in` |  | Alt+[ | Opt+[ |  | B | ✅ |
| `TimeTrimInRelative` | `motion.trim_in` | `{"relative":true}` | Alt+Shift+[ | Opt+Shift+[ |  | B | ✅ |
| `TimeTrimOut` | `motion.trim_out` |  | Alt+] | Opt+] |  | B | ✅ |
| `TimeTrimOutRelative` | `motion.trim_out` | `{"relative":true}` | Alt+Shift+] | Opt+Shift+] |  | B | ✅ |

### `CDirItemTabPanelTime`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `TimeFastForward` | `motion.go_end` |  | Ctrl+Alt+Right / PadEnd / End | Cmd+Opt+Right / PadEnd / End | footage | B | ✅ |
| `TimeRewind` | `motion.go_start` |  | Ctrl+Alt+Left / PadHome / Home | Cmd+Opt+Left / PadHome / Home | footage | B | ✅ |
| `TimeShiftSpacebarPlay` | `motion.play` | `{"alternate":true}` | Shift+Space | Shift+Space | footage | B | ✅ |
| `TimeStepBack` | `motion.step` | `{"frames":-1}` | Ctrl+Left / PadPageUp / PageUp | Cmd+Left / PadPageUp / PageUp | footage | B | ✅ |
| `TimeStepBackMore` | `motion.step` | `{"frames":-10}` | Shift+PageUp / Ctrl+Shift+Left | Shift+PageUp / Cmd+Shift+Left | footage | B | ✅ |
| `TimeStepForward` | `motion.step` | `{"frames":1}` | Ctrl+Right / PadPageDown / PageDown | Cmd+Right / PadPageDown / PageDown | footage | B | ✅ |
| `TimeStepForwardMore` | `motion.step` | `{"frames":10}` | Shift+PageDown / Ctrl+Shift+Right | Shift+PageDown / Cmd+Shift+Right | footage | B | ✅ |

### `CDirTabPanel`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `NewViewer` | `motion.view.new_viewer` |  | Alt+Shift+N | Opt+Shift+N | composition | A | ⏸ |

### `CEggApp`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `CancelAllPreviews` | `motion.comp.cancel_all_previews` |  | Esc | Esc |  | B | ☐ |
| `OpenComp` | `motion.panel.toggle_viewer_timeline` |  | \\ | \\ |  | B | ☐ |
| `OpenMRUContext` | `motion.panel.activate_most_recent_viewer` |  | Shift+Esc | Shift+Esc |  | B | ⏸ |
| `PreviewAudio` | `motion.comp.preview_audio` | `{"work_area":false}` | PadDecimal / PadComma / PadDelete | PadDecimal / PadComma / PadDelete / Control+. |  | B | ⏸ |
| `PreviewAudioWorkArea` | `motion.comp.preview_audio` | `{"work_area":true}` | Alt+PadDecimal / Alt+PadComma / Alt+PadDelete | Opt+PadDecimal / Opt+PadComma / Opt+PadDelete / Control+Opt+. |  | B | ⏸ |
| `PreviewNumpad0` | `motion.play` | `{"preview":"numpad0"}` | Pad0 / PadInsert | Pad0 / PadInsert / Control+0 |  | B | ✅ |
| `PreviewNumpad0NFrames` | `motion.play` | `{"preview":"numpad0_n_frames"}` | Alt+Pad0 / Alt+PadInsert | Opt+Pad0 / Opt+PadInsert / Control+Opt+0 |  | B | ⏸ |
| `PreviewShiftNumpad0` | `motion.play` | `{"preview":"shift_numpad0"}` | Shift+Pad0 / Shift+PadInsert | Shift+Pad0 / Shift+PadInsert / Control+Shift+0 |  | B | ☐ |
| `ShowConsole` | `motion.window.console` |  | Ctrl+F12 | Cmd+F12 |  | B | ⏸ |
| `ShowDebugMonitor` | `motion.window.debug_monitor` |  | Ctrl+Shift+9 | Cmd+Shift+9 |  | B | Ⓐ |
| `TestHostPlugin` | `motion.help.test_host_plugin` |  | Ctrl+Shift+F12 | Cmd+Shift+F12 |  | B | Ⓐ |
| `ToggleTransmit` | `motion.view.mercury_transmit` |  | PadSlash | PadSlash / Control+Shift+/ |  | B | Ⓐ |
| `WriteStringFile` | `motion.help.write_string_file` |  |  | Control+Cmd+Opt+Shift+S |  | B | Ⓐ |

### `CEggAppTool`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `GizmoModeLegacy`（由负责的 agent 绑定） | `motion.tool.gizmo_legacy` |  | Ctrl+Shift+1 | Cmd+Shift+1 |  | A | ☐ |
| `GizmoModePosition`（由负责的 agent 绑定） | `motion.tool.gizmo_position` |  | 4 | 4 |  | A | ☐ |
| `GizmoModeRotate`（由负责的 agent 绑定） | `motion.tool.gizmo_rotate` |  | 6 | 6 |  | A | ☐ |
| `GizmoModeScale`（由负责的 agent 绑定） | `motion.tool.gizmo_scale` |  | 5 | 5 |  | A | ☐ |
| `GizmoModeToggle`（由负责的 agent 绑定） | `motion.tool.gizmo_toggle` |  | Shift+V | Shift+V |  | A | ☐ |
| `ToolArrow`（由负责的 agent 绑定） | `motion.tool.selection` |  | V | V |  | A | ☐ |
| `ToolCamera`（由负责的 agent 绑定） | `motion.tool.camera` |  | C | C |  | A | ☐ |
| `ToolCameraDolly`（由负责的 agent 绑定） | `motion.tool.camera_dolly` |  | 3 | 3 |  | A | ☐ |
| `ToolCameraDollyToggle`（由负责的 agent 绑定） | `motion.tool.camera_dolly_cycle` |  | Shift+3 | Shift+3 |  | A | ☐ |
| `ToolCameraOrbit`（由负责的 agent 绑定） | `motion.tool.camera_orbit` |  | 1 | 1 |  | A | ☐ |
| `ToolCameraOrbitToggle`（由负责的 agent 绑定） | `motion.tool.camera_orbit_cycle` |  | Shift+1 | Shift+1 |  | A | ☐ |
| `ToolCameraPan`（由负责的 agent 绑定） | `motion.tool.camera_pan` |  | 2 | 2 |  | A | ☐ |
| `ToolCameraPanToggle`（由负责的 agent 绑定） | `motion.tool.camera_pan_cycle` |  | Shift+2 | Shift+2 |  | A | ☐ |
| `ToolCameraToggle`（由负责的 agent 绑定） | `motion.tool.camera_cycle` |  | Shift+C | Shift+C |  | A | ☐ |
| `ToolHand`（由负责的 agent 绑定） | `motion.tool.hand` |  | H | H |  | A | ☐ |
| `ToolMagnify`（由负责的 agent 绑定） | `motion.tool.zoom` |  | Z | Z |  | A | ☐ |
| `ToolMask`（由负责的 agent 绑定） | `motion.tool.shape` |  | Q | Q |  | A | ☐ |
| `ToolMaskToggle`（由负责的 agent 绑定） | `motion.tool.shape_cycle` |  | Shift+Q | Shift+Q |  | A | ☐ |
| `ToolPaint`（由负责的 agent 绑定） | `motion.tool.paint` |  | Ctrl+B | Cmd+B |  | A | ☐ |
| `ToolPan`（由负责的 agent 绑定） | `motion.tool.pan_behind` |  | Y | Y |  | A | ☐ |
| `ToolParametricMesh`（由负责的 agent 绑定） | `motion.tool.parametric_mesh` |  | Shift+D | Shift+D |  | A | ☐ |
| `ToolPen`（由负责的 agent 绑定） | `motion.tool.pen` |  | G | G |  | A | ☐ |
| `ToolPenToggle`（由负责的 agent 绑定） | `motion.tool.pen_cycle` |  | Shift+G | Shift+G |  | A | ☐ |
| `ToolPin`（由负责的 agent 绑定） | `motion.tool.puppet` |  | Ctrl+P | Cmd+P |  | A | ☐ |
| `ToolRotate`（由负责的 agent 绑定） | `motion.tool.rotation` |  | W | W |  | A | ☐ |
| `ToolRotoBrush`（由负责的 agent 绑定） | `motion.tool.roto_brush` |  | Alt+W | Opt+W |  | A | ☐ |
| `ToolText`（由负责的 agent 绑定） | `motion.tool.type` |  | Ctrl+T | Cmd+T |  | A | ☐ |

### `CItem`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `FastPreviewsAdaptiveResolutionKeyboard` | `motion.viewer.fast_previews` | `{"mode":"adaptive_resolution"}` | Ctrl+Alt+2 | Cmd+Opt+2 | composition | B | ⏸ |
| `FastPreviewsCalderRTEDraft3DKeyboard` | `motion.viewer.fast_previews` | `{"mode":"draft_3d"}` | Ctrl+Alt+4 | Cmd+Opt+4 | composition | B | ⏸ |
| `FastPreviewsOffKeyboard` | `motion.viewer.fast_previews` | `{"mode":"off"}` | Ctrl+Alt+1 | Cmd+Opt+1 | composition | B | ⏸ |
| `FastPreviewsWireframeKeyboard` | `motion.viewer.fast_previews` | `{"mode":"wireframe"}` | Ctrl+Alt+5 | Cmd+Opt+5 | composition | B | ⏸ |
| `PurgeSnapshot1` | `motion.viewer.purge_snapshot` | `{"slot":1}` | Ctrl+Shift+F5 | Cmd+Shift+F5 | composition | B | ⏸ |
| `PurgeSnapshot2` | `motion.viewer.purge_snapshot` | `{"slot":2}` | Ctrl+Shift+F6 | Cmd+Shift+F6 | composition | B | ⏸ |
| `PurgeSnapshot3` | `motion.viewer.purge_snapshot` | `{"slot":3}` | Ctrl+Shift+F7 | Cmd+Shift+F7 | composition | B | ⏸ |
| `PurgeSnapshot4` | `motion.viewer.purge_snapshot` | `{"slot":4}` | Ctrl+Shift+F8 | Cmd+Shift+F8 | composition | B | ⏸ |
| `ShowAlphaChannel` | `motion.viewer.show_channel` | `{"channel":"alpha","colorize":false}` | Alt+4 | Opt+4 | composition | B | ⏸ |
| `ShowAlphaChannelAlt` | `motion.viewer.show_channel` | `{"channel":"alpha","colorize":true}` | Alt+Shift+4 | Opt+Shift+4 | composition | B | ⏸ |
| `ShowAlphaOutline` | `motion.viewer.show_channel` | `{"channel":"alpha_boundary","colorize":false}` | Alt+5 | Opt+5 | composition | B | ⏸ |
| `ShowAlphaOverlay` | `motion.viewer.show_channel` | `{"channel":"alpha_overlay","colorize":false}` | Alt+6 | Opt+6 | composition | B | ⏸ |
| `ShowBlueChannel` | `motion.viewer.show_channel` | `{"channel":"blue","colorize":false}` | Alt+3 | Opt+3 | composition | B | ⏸ |
| `ShowBlueChannelAlt` | `motion.viewer.show_channel` | `{"channel":"blue","colorize":true}` | Alt+Shift+3 | Opt+Shift+3 | composition | B | ⏸ |
| `ShowGreenChannel` | `motion.viewer.show_channel` | `{"channel":"green","colorize":false}` | Alt+2 | Opt+2 | composition | B | ⏸ |
| `ShowGreenChannelAlt` | `motion.viewer.show_channel` | `{"channel":"green","colorize":true}` | Alt+Shift+2 | Opt+Shift+2 | composition | B | ⏸ |
| `ShowRedChannel` | `motion.viewer.show_channel` | `{"channel":"red","colorize":false}` | Alt+1 | Opt+1 | composition | B | ⏸ |
| `ShowRedChannelAlt` | `motion.viewer.show_channel` | `{"channel":"red","colorize":true}` | Alt+Shift+1 | Opt+Shift+1 | composition | B | ⏸ |
| `ShowSnapshot1` | `motion.viewer.show_snapshot` | `{"slot":1}` | F5 | F5 | composition | B | ⏸ |
| `ShowSnapshot2` | `motion.viewer.show_snapshot` | `{"slot":2}` | F6 | F6 | composition | B | ⏸ |
| `ShowSnapshot3` | `motion.viewer.show_snapshot` | `{"slot":3}` | F7 | F7 | composition | B | ⏸ |
| `ShowSnapshot4` | `motion.viewer.show_snapshot` | `{"slot":4}` | F8 | F8 | composition | B | ⏸ |
| `ShowXray` | `motion.viewer.xray` |  | Alt+X | Opt+X | composition | A | ⏸ |
| `TakeSnapshot1` | `motion.viewer.take_snapshot` | `{"slot":1}` | Shift+F5 | Shift+F5 | composition | B | ⏸ |
| `TakeSnapshot2` | `motion.viewer.take_snapshot` | `{"slot":2}` | Shift+F6 | Shift+F6 | composition | B | ⏸ |
| `TakeSnapshot3` | `motion.viewer.take_snapshot` | `{"slot":3}` | Shift+F7 | Shift+F7 | composition | B | ⏸ |
| `TakeSnapshot4` | `motion.viewer.take_snapshot` | `{"slot":4}` | Shift+F8 | Shift+F8 | composition | B | ⏸ |
| `TitleActionGridAnim` | `motion.view.proportional_grid` |  | Alt+' | Opt+' | composition | B | ☐ |
| `TitleActionGridRotate` | `motion.view.title_action_safe` |  | ' | ' | composition | B | ☐ |
| `TitleActionSafeToggle` | `motion.view.title_action_safe_center_cut` |  | Shift+' | Shift+' | composition | B | ⏸ |
| `CopyFrameToClipboard`（只在 macOS 26.2 的快捷键文件里） | `motion.viewer.copy_frame_to_clipboard` |  |  | Cmd+Opt+Shift+F5 | composition | B | ⏸ |

### `COutline`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `Rename` | `motion.timeline.rename` |  | Enter | Enter | timeline | C | ☐ |

### `CPanoECOutline`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `FitItemView` | `viewer.fit` |  | Shift+/ | Shift+/ | effect_controls | A | ✅ |
| `FitItemViewUpTo100` | `motion.viewer.fit_up_to_100` |  | Alt+/ | Opt+/ | effect_controls | A | ☐ |
| `NOP` | `motion.panel.nop` |  |  |  | effect_controls | B | Ⓐ |
| `OpenComp` | `motion.panel.toggle_viewer_timeline` |  | \\ | \\ | effect_controls | B | ☐ |
| `SelectNext` | `motion.panel.select_next` | `{"add":false}` | Down | Down | effect_controls | B | ☐ |
| `SelectNextAdd` | `motion.panel.select_next` | `{"add":true}` | Shift+Down | Shift+Down | effect_controls | B | ☐ |
| `SelectPrevious` | `motion.panel.select_previous` | `{"add":false}` | Up | Up | effect_controls | B | ☐ |
| `SelectPreviousAdd` | `motion.panel.select_previous` | `{"add":true}` | Shift+Up | Shift+Up | effect_controls | B | ☐ |
| `TwirlClosed` | `motion.panel.twirl_closed` |  | Left | Left | effect_controls | B | ☐ |
| `TwirlOpen` | `motion.panel.twirl_open` |  | Right | Right | effect_controls | B | ☐ |
| `Zoom100Percent` | `motion.viewer.zoom_100` |  | / | / | effect_controls | A | ☐ |
| `ZoomInResize` | `motion.viewer.zoom_in_resize` |  | Alt+. / Ctrl+= | Opt+. / Cmd+= | effect_controls | A | ⏸ |
| `ZoomNoScroll` | `motion.viewer.zoom_100_resize` |  | Ctrl+Alt+/ | Cmd+Opt+/ | effect_controls | A | ⏸ |
| `ZoomOutResize` | `motion.viewer.zoom_out_resize` |  | Alt+, / Ctrl+- | Opt+, / Cmd+- | effect_controls | A | ⏸ |

### `CPanoProjFootage`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `TimeJumpToIn` | `motion.go_in` |  | I | I | footage | B | ✅ |
| `TimeJumpToOut` | `motion.go_out` |  | O | O | footage | B | ✅ |
| `TimeStepBack` | `motion.step` | `{"frames":-1}` | Left | Left | footage | B | ✅ |
| `TimeStepForward` | `motion.step` | `{"frames":1}` | Right | Right | footage | B | ✅ |
| `TimeTrimIn` | `motion.trim_in` |  | Alt+[ | Opt+[ | footage | B | ✅ |
| `TimeTrimOut` | `motion.trim_out` |  | Alt+] | Opt+] | footage | B | ✅ |

### `CPanoProjItem`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `FitItemView` | `viewer.fit` |  | Shift+/ | Shift+/ | composition | A | ✅ |
| `FitItemViewUpTo100` | `motion.viewer.fit_up_to_100` |  | Alt+/ | Opt+/ | composition | A | ☐ |
| `SelectNextMask` | `motion.layer.select_next_mask` |  | Alt+\` | Opt+\` | composition | A | ⏸ |
| `SelectPreviousMask` | `motion.layer.select_previous_mask` |  | Alt+Shift+\` | Opt+Shift+\` | composition | A | ⏸ |
| `Zoom100Percent` | `motion.viewer.zoom_100` |  | / | / | composition | A | ☐ |
| `ZoomInResize` | `motion.viewer.zoom_in_resize` |  | Alt+. / Ctrl+= | Opt+. / Cmd+= | composition | A | ⏸ |
| `ZoomNoScroll` | `motion.viewer.zoom_100_resize` |  | Ctrl+Alt+/ | Cmd+Opt+/ | composition | A | ⏸ |
| `ZoomOutResize` | `motion.viewer.zoom_out_resize` |  | Alt+, / Ctrl+- | Opt+, / Cmd+- | composition | A | ⏸ |

### `CPanoProjLayer`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `ClearMask` | `motion.layer.delete_mask_or_vertices` |  | Alt+Backspace / Alt+Delete | Opt+Backspace / Opt+Delete | layer | A | ⏸ |
| `OpenComp` | `motion.panel.toggle_viewer_timeline` |  | \\ | \\ | layer | B | ☐ |
| `TimeSetIn` | `motion.move_in` |  | [ | [ | layer | B | ✅ |
| `TimeSetOut` | `motion.move_out` |  | ] | ] | layer | B | ✅ |
| `TimeStretchIn` | `motion.timeline.stretch_in_point_to_current_time` |  | Ctrl+Shift+, | Cmd+Shift+, | layer | C | ☐ |
| `TimeStretchOut` | `motion.timeline.stretch_out_point_to_current_time` |  | Ctrl+Alt+, | Cmd+Opt+, | layer | C | ☐ |
| `TimeTrimIn` | `motion.trim_in` |  | Alt+[ | Opt+[ | layer | B | ✅ |
| `TimeTrimOut` | `motion.trim_out` |  | Alt+] | Opt+] | layer | B | ✅ |

### `CPanoProjLayerPano`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `NudgeDown` | `motion.layer.nudge_position` | `{"dx":0,"dy":1}` | Down / Alt+Down | Down / Opt+Down | layer | A | ☐ |
| `NudgeDownMore` | `motion.layer.nudge_position` | `{"dx":0,"dy":10}` | Shift+Down / Alt+Shift+Down | Shift+Down / Opt+Shift+Down | layer | A | ☐ |
| `NudgeLeft` | `motion.layer.nudge_position` | `{"dx":-1,"dy":0}` | Left | Left | layer | A | ☐ |
| `NudgeLeftMore` | `motion.layer.nudge_position` | `{"dx":-10,"dy":0}` | Shift+Left | Shift+Left | layer | A | ☐ |
| `NudgeOpacityDown` | `motion.layer.nudge_opacity` | `{"delta":-1}` | Ctrl+Alt+PadMinus | Control+Opt+PadMinus | layer | B | ☐ |
| `NudgeOpacityDownMore` | `motion.layer.nudge_opacity` | `{"delta":-10}` | Ctrl+Alt+Shift+PadMinus | Control+Opt+Shift+PadMinus | layer | B | ☐ |
| `NudgeOpacityUp` | `motion.layer.nudge_opacity` | `{"delta":1}` | Ctrl+Alt+PadPlus | Control+Opt+PadPlus | layer | B | ☐ |
| `NudgeOpacityUpMore` | `motion.layer.nudge_opacity` | `{"delta":10}` | Ctrl+Alt+Shift+PadPlus | Control+Opt+Shift+PadPlus | layer | B | ☐ |
| `NudgeRight` | `motion.layer.nudge_position` | `{"dx":1,"dy":0}` | Right | Right | layer | A | ☐ |
| `NudgeRightMore` | `motion.layer.nudge_position` | `{"dx":10,"dy":0}` | Shift+Right | Shift+Right | layer | A | ☐ |
| `NudgeRotCCW` | `motion.layer.nudge_rotation` | `{"delta":-1}` | PadMinus | PadMinus | layer | B | ☐ |
| `NudgeRotCCWMore` | `motion.layer.nudge_rotation` | `{"delta":-10}` | Shift+PadMinus | Shift+PadMinus | layer | B | ☐ |
| `NudgeRotCW` | `motion.layer.nudge_rotation` | `{"delta":1}` | PadPlus | PadPlus | layer | B | ☐ |
| `NudgeRotCWMore` | `motion.layer.nudge_rotation` | `{"delta":10}` | Shift+PadPlus | Shift+PadPlus | layer | B | ☐ |
| `NudgeScaleLarger` | `motion.layer.nudge_scale` | `{"delta":1}` | Alt+PadPlus / Ctrl+PadPlus | Opt+PadPlus / Cmd+PadPlus | layer | B | ☐ |
| `NudgeScaleLargerMore` | `motion.layer.nudge_scale` | `{"delta":10}` | Alt+Shift+PadPlus / Ctrl+Shift+PadPlus | Opt+Shift+PadPlus / Cmd+Shift+PadPlus | layer | B | ☐ |
| `NudgeScaleSmaller` | `motion.layer.nudge_scale` | `{"delta":-1}` | Alt+PadMinus / Ctrl+PadMinus | Opt+PadMinus / Cmd+PadMinus | layer | B | ☐ |
| `NudgeScaleSmallerMore` | `motion.layer.nudge_scale` | `{"delta":-10}` | Alt+Shift+PadMinus / Ctrl+Shift+PadMinus | Opt+Shift+PadMinus / Cmd+Shift+PadMinus | layer | B | ☐ |
| `NudgeUp` | `motion.layer.nudge_position` | `{"dx":0,"dy":-1}` | Up / Alt+Up | Up / Opt+Up | layer | A | ☐ |
| `NudgeUpMore` | `motion.layer.nudge_position` | `{"dx":0,"dy":-10}` | Shift+Up / Alt+Shift+Up | Shift+Up / Opt+Shift+Up | layer | A | ☐ |

### `CPanoProjLayerPanoMask`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `Zoom100Percent` | `motion.viewer.zoom_100` |  | / | / | layer | A | ☐ |

### `CPanoRender`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `StopRenderQueue` | `motion.comp.stop_rendering` |  | Ctrl+. / Esc | Cmd+. / Esc | render_queue | B | ☐ |

### `CSwitchboard`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `AddItemToCompReplaceLayer` | `motion.file.replace_layer_with_footage` |  | Ctrl+Alt+/ | Cmd+Opt+/ |  | E | ☐ |
| `BestQualityRecurse` | `motion.layer.quality` | `{"quality":"best","nested":true}` |  | Control+Cmd+U |  | B | ⏸ |
| `CloseAll` | `motion.window.close_all` |  | Ctrl+Alt+W | Cmd+Opt+W |  | B | ☐ |
| `CloseWindow` | `motion.window.close_panel_group` |  | Ctrl+Shift+W | Cmd+Shift+W |  | B | ☐ |
| `CompToggleUberAnimatingKeyframesAddState` | `motion.reveal` | `{"what":"animated","add":true}` | Shift+U | Shift+U |  | B | ✅ |
| `CompToggleUberAnimatingKeyframesAndExpressionsAddState` | `motion.reveal` | `{"what":"animation","add":true}` |  |  |  | B | ✅ |
| `CustResNoDialogRecurse` | `motion.view.resolution_custom` | `{"nested":true}` |  | Control+Cmd+Opt+J |  | E | ⏸ |
| `DeselectAllKeyframes` | `motion.comp.deselect_all_keyframes` |  | Shift+F2 / Ctrl+Alt+Shift+A | Shift+F2 / Cmd+Opt+Shift+A |  | B | ✅ |
| `DraftQualityRecurse` | `motion.layer.quality` | `{"quality":"draft","nested":true}` |  | Control+Cmd+Shift+U |  | B | ⏸ |
| `HighResolutionRecurse` | `motion.view.resolution` | `{"factor":1,"nested":true}` |  | Control+Cmd+J |  | B | ⏸ |
| `LowResolutionRecurse` | `motion.view.resolution` | `{"factor":4,"nested":true}` |  | Control+Cmd+Opt+Shift+J |  | B | ⏸ |
| `MedResolutionRecurse` | `motion.view.resolution` | `{"factor":2,"nested":true}` |  | Control+Cmd+Shift+J |  | B | ⏸ |
| `NewDebugComp` | `motion.comp.new_debug_comp` |  | Alt+Shift+D | Opt+Shift+D |  | B | Ⓐ |
| `SavePreviewShiftNumpad0` | `motion.comp.save_current_preview` | `{"shift":true}` | Ctrl+Shift+Pad0 / Ctrl+Shift+PadInsert | Cmd+Shift+Pad0 / Cmd+Shift+PadInsert |  | B | ⏸ |
| `ScanForChangedFootage` | `motion.file.scan_for_changed_footage` |  | Ctrl+Alt+Shift+Q | Control+Cmd+Opt+Shift+Q |  | E | ⏸ |
| `SelectNextViewerContext` | `motion.panel.select_next_viewer` |  | Shift+. | Shift+. |  | B | ⏸ |
| `SelectPrevViewerContext` | `motion.panel.select_previous_viewer` |  | Shift+, | Shift+, |  | B | ⏸ |
| `ShowFrameFileName` | `motion.file.reveal_frame_source` |  | Ctrl+Alt+E | Cmd+Opt+E |  | E | ⏸ |
| `ShowProjectTree` | `motion.file.project_flowchart` |  | Ctrl+F11 | Cmd+F11 |  | E | ⏸ |
| `SwitchTo3DViewA` | `motion.view.switch_3d_view_shortcut` | `{"slot":"a"}` | F10 | F10 |  | A | ⏸ |
| `SwitchTo3DViewB` | `motion.view.switch_3d_view_shortcut` | `{"slot":"b"}` | F11 | F11 |  | A | ⏸ |
| `SwitchTo3DViewC` | `motion.view.switch_3d_view_shortcut` | `{"slot":"c"}` | F12 | F12 |  | A | ⏸ |
| `SwitchTo3DViewFirst`（同一快捷键 AE 先给了别的命令） | `motion.view.switch_3d_view_shortcut` | `{"slot":"first"}` | F10 | F10 |  | A | ⏸ |
| `ToggleCastsShadows` | `motion.layer.casts_shadows` |  | Alt+Shift+C | Opt+Shift+C |  | B | ⏸ |
| `ToggleSwitchesModes` | `motion.timeline.toggle_switches_modes` |  | F4 | F4 |  | C | ✅ |
| `WireframeQualityRecurse` | `motion.layer.quality` | `{"quality":"wireframe","nested":true}` |  | Control+Cmd+Opt+Shift+U |  | B | ⏸ |

### `CSwitchboardModal`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `Clear`（文本框自带） | `edit.clear_field` |  |  |  | dialog | B | ✅ |
| `SelectAll`（文本框自带） | `edit.select_all` |  |  |  | dialog | B | ✅ |

### `CTopic`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `Twirl` | `motion.timeline.twirl` | `{"explode":false,"preserve_solo":false}` |  |  | timeline | C | ⏸ |
| `TwirlExplode` | `motion.timeline.twirl` | `{"explode":true,"preserve_solo":false}` | Ctrl+\` | Cmd+\` | timeline | C | ✅ |
| `TwirlPreserveSolo` | `motion.timeline.twirl` | `{"explode":false,"preserve_solo":true}` | Shift+\` | Shift+\` | timeline | C | ☐ |
| `TwirlPreserveSoloExplode` | `motion.timeline.twirl` | `{"explode":true,"preserve_solo":true}` | Ctrl+Shift+\` | Cmd+Shift+\` | timeline | C | ☐ |

### `FloPano`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `ClearWithoutWarning` | `motion.edit.clear_without_warning` |  | Ctrl+Backspace | Cmd+Backspace | flowchart | E | ⏸ |
| `OpenComp` | `motion.panel.toggle_viewer_timeline` |  | \\ | \\ | flowchart | B | ☐ |
| `Zoom100Percent` | `motion.viewer.zoom_100` |  | / | / | flowchart | A | ☐ |

### `MacSysShortcutsAlt`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `MacSysMenuMinimize` | `window.minimize` |  |  |  |  | B | ☐ |

### `MacSysShortcutsStd`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `MacSysMenuMinimize` | `window.minimize` |  |  | Cmd+M |  | B | ☐ |

### `POutlinePano`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `AddItemToCompReplaceLayer` | `motion.file.replace_layer_with_footage` |  | Ctrl+Alt+/ | Cmd+Opt+/ | project | E | ☐ |
| `ClearWithoutWarning` | `motion.edit.clear_without_warning` |  | Ctrl+Backspace | Cmd+Backspace | project | E | ☐ |
| `OpenItemWindow` | `motion.project.open_item` |  | PadEnter | PadEnter | project | E | ☐ |
| `SelectNext` | `motion.panel.select_next` | `{"add":false}` | Down | Down | project | B | ☐ |
| `SelectNextAdd` | `motion.panel.select_next` | `{"add":true}` | Shift+Down | Shift+Down | project | B | ☐ |
| `SelectPrevious` | `motion.panel.select_previous` | `{"add":false}` | Up | Up | project | B | ☐ |
| `SelectPreviousAdd` | `motion.panel.select_previous` | `{"add":true}` | Shift+Up | Shift+Up | project | B | ☐ |
| `TwirlClosed` | `motion.panel.twirl_closed` |  | Left | Left | project | B | ☐ |
| `TwirlOpen` | `motion.panel.twirl_open` |  | Right | Right | project | B | ☐ |

### `RQOutlinePano`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `SelectNext` | `motion.panel.select_next` | `{"add":false}` | Down | Down | render_queue | B | ☐ |
| `SelectNextAdd` | `motion.panel.select_next` | `{"add":true}` | Shift+Down | Shift+Down | render_queue | B | ☐ |
| `SelectPrevious` | `motion.panel.select_previous` | `{"add":false}` | Up | Up | render_queue | B | ☐ |
| `SelectPreviousAdd` | `motion.panel.select_previous` | `{"add":true}` | Shift+Up | Shift+Up | render_queue | B | ☐ |
| `StartRenderQueue` | `motion.comp.start_rendering` |  | Enter / PadEnter | Enter / PadEnter | render_queue | B | ☐ |
| `StopRenderQueue` | `motion.comp.stop_rendering` |  | Ctrl+. / Esc | Cmd+. / Esc | render_queue | B | ☐ |
| `TwirlClosed` | `motion.panel.twirl_closed` |  | Left | Left | render_queue | B | ☐ |
| `TwirlOpen` | `motion.panel.twirl_open` |  | Right | Right | render_queue | B | ☐ |

### `TLOutlinePano`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `CompTimeZoomFrames` | `motion.timeline.zoom_to_frames` |  | ; | ; | timeline | C | ✅ |
| `CompTimeZoomIn` | `timeline.zoom_in` |  | = | = | timeline | C | ✅ |
| `CompTimeZoomOut` | `timeline.zoom_out` |  | - | - | timeline | C | ✅ |
| `CompTimeZoomToggleFullCustom` | `motion.timeline.zoom_toggle_full` |  | Shift+; | Shift+; | timeline | C | ✅ |
| `TimeSetIn` | `motion.move_in` |  | [ | [ | timeline | B | ✅ |
| `TimeSetInRelative` | `motion.move_in` | `{"relative":true}` | Shift+[ | Shift+[ | timeline | B | ✅ |
| `TimeSetOut` | `motion.move_out` |  | ] | ] | timeline | B | ✅ |
| `TimeSetOutRelative` | `motion.move_out` | `{"relative":true}` | Shift+] | Shift+] | timeline | B | ✅ |
| `TimeStretchIn` | `motion.timeline.stretch_in_point_to_current_time` |  | Ctrl+Shift+, | Cmd+Shift+, | timeline | C | ✅ |
| `TimeStretchOut` | `motion.timeline.stretch_out_point_to_current_time` |  | Ctrl+Alt+, | Cmd+Opt+, | timeline | C | ✅ |
| `TimeTrimIn` | `motion.trim_in` |  | Alt+[ | Opt+[ | timeline | B | ✅ |
| `TimeTrimInRelative` | `motion.trim_in` | `{"relative":true}` | Alt+Shift+[ | Opt+Shift+[ | timeline | B | ✅ |
| `TimeTrimOut` | `motion.trim_out` |  | Alt+] | Opt+] | timeline | B | ✅ |
| `TimeTrimOutRelative` | `motion.trim_out` | `{"relative":true}` | Alt+Shift+] | Opt+Shift+] | timeline | B | ✅ |
| `ToggleGraph` | `motion.timeline.graph_editor` |  | Shift+F3 | Shift+F3 | timeline | C | ✅ |
| `CompTimeZoomWorkArea`（只在 macOS 26.2 的快捷键文件里） | `motion.timeline.zoom_to_work_area` |  |  | Opt+; | timeline | C | ☐ |

### `TextLayerUI`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `TextAlignCenter` | `motion.tool.text_align_center` |  | Ctrl+Shift+C | Cmd+Shift+C | text_editing | A | ☐ |
| `TextAlignLeft` | `motion.tool.text_align_left` |  | Ctrl+Shift+L | Cmd+Shift+L | text_editing | A | ☐ |
| `TextAlignRight` | `motion.tool.text_align_right` |  | Ctrl+Shift+R | Cmd+Shift+R | text_editing | A | ☐ |
| `TextAllCaps` | `motion.tool.text_all_caps` |  | Ctrl+Shift+K | Cmd+Shift+K | text_editing | A | ☐ |
| `TextAutoLeading` | `motion.tool.text_auto_leading` |  | Ctrl+Alt+Shift+A | Cmd+Opt+Shift+A | text_editing | A | ☐ |
| `TextCancel` | `motion.tool.text_cancel` |  | Esc | Esc | text_editing | A | ☐ |
| `TextCommit` | `motion.tool.text_commit` |  | PadEnter / Ctrl+Enter | PadEnter / Cmd+Enter | text_editing | A | ☐ |
| `TextCursorToDocumentEnd` | `motion.tool.text_cursor_to_document_end` |  | Ctrl+End | Cmd+End | text_editing | A | ☐ |
| `TextCursorToDocumentStart` | `motion.tool.text_cursor_to_document_start` |  | Ctrl+Home | Cmd+Home | text_editing | A | ☐ |
| `TextCursorToDown` | `motion.tool.text_cursor_to_down` |  | Down | Down | text_editing | A | ☐ |
| `TextCursorToLeft` | `motion.tool.text_cursor_to_left` |  | Left | Left | text_editing | A | ☐ |
| `TextCursorToLineEnd` | `motion.tool.text_cursor_to_line_end` |  | End | End | text_editing | A | ☐ |
| `TextCursorToLineStart` | `motion.tool.text_cursor_to_line_start` |  | Home | Home | text_editing | A | ☐ |
| `TextCursorToRight` | `motion.tool.text_cursor_to_right` |  | Right | Right | text_editing | A | ☐ |
| `TextCursorToUp` | `motion.tool.text_cursor_to_up` |  | Up | Up | text_editing | A | ☐ |
| `TextCursorToWordDown` | `motion.tool.text_cursor_to_word_down` |  | Ctrl+Down | Cmd+Down | text_editing | A | ☐ |
| `TextCursorToWordLeft` | `motion.tool.text_cursor_to_word_left` |  | Ctrl+Left | Cmd+Left | text_editing | A | ☐ |
| `TextCursorToWordRight` | `motion.tool.text_cursor_to_word_right` |  | Ctrl+Right | Cmd+Right | text_editing | A | ☐ |
| `TextCursorToWordUp` | `motion.tool.text_cursor_to_word_up` |  | Ctrl+Up | Cmd+Up | text_editing | A | ☐ |
| `TextDecreaseBaselineShift` | `motion.tool.text_decrease_baseline_shift` |  | Alt+Shift+Down | Opt+Shift+Down | text_editing | A | ☐ |
| `TextDecreaseBaselineShiftALot` | `motion.tool.text_decrease_baseline_shift_a_lot` |  | Ctrl+Alt+Shift+Down | Cmd+Opt+Shift+Down | text_editing | A | ☐ |
| `TextDecreaseFontSize` | `motion.tool.text_decrease_font_size` |  | Ctrl+Shift+, | Cmd+Shift+, | text_editing | A | ☐ |
| `TextDecreaseFontSizeALot` | `motion.tool.text_decrease_font_size_a_lot` |  | Ctrl+Alt+Shift+, | Cmd+Opt+Shift+, | text_editing | A | ☐ |
| `TextDecreaseKerning` | `motion.tool.text_decrease_kerning` |  | Alt+Left | Opt+Left | text_editing | A | ☐ |
| `TextDecreaseKerningALot` | `motion.tool.text_decrease_kerning_a_lot` |  | Ctrl+Alt+Left | Cmd+Opt+Left | text_editing | A | ☐ |
| `TextDecreaseLeading` | `motion.tool.text_decrease_leading` |  | Alt+Up | Opt+Up | text_editing | A | ☐ |
| `TextDecreaseLeadingALot` | `motion.tool.text_decrease_leading_a_lot` |  | Ctrl+Alt+Up | Cmd+Opt+Up | text_editing | A | ☐ |
| `TextDeleteBackward` | `motion.tool.text_delete_backward` |  | Backspace | Backspace | text_editing | A | ☐ |
| `TextDeleteForward` | `motion.tool.text_delete_forward` |  | Delete | Delete | text_editing | A | ☐ |
| `TextIgnoreKey`（文本框自带） | `motion.tool.text_ignore_key` |  |  |  | text_editing | A | Ⓐ |
| `TextIncreaseBaselineShift` | `motion.tool.text_increase_baseline_shift` |  | Alt+Shift+Up | Opt+Shift+Up | text_editing | A | ☐ |
| `TextIncreaseBaselineShiftALot` | `motion.tool.text_increase_baseline_shift_a_lot` |  | Ctrl+Alt+Shift+Up | Cmd+Opt+Shift+Up | text_editing | A | ☐ |
| `TextIncreaseFontSize` | `motion.tool.text_increase_font_size` |  | Ctrl+Shift+. | Cmd+Shift+. | text_editing | A | ☐ |
| `TextIncreaseFontSizeALot` | `motion.tool.text_increase_font_size_a_lot` |  | Ctrl+Alt+Shift+. | Cmd+Opt+Shift+. | text_editing | A | ☐ |
| `TextIncreaseKerning` | `motion.tool.text_increase_kerning` |  | Alt+Right | Opt+Right | text_editing | A | ☐ |
| `TextIncreaseKerningALot` | `motion.tool.text_increase_kerning_a_lot` |  | Ctrl+Alt+Right | Cmd+Opt+Right | text_editing | A | ☐ |
| `TextIncreaseLeading` | `motion.tool.text_increase_leading` |  | Alt+Down | Opt+Down | text_editing | A | ☐ |
| `TextIncreaseLeadingALot` | `motion.tool.text_increase_leading_a_lot` |  | Ctrl+Alt+Down | Cmd+Opt+Down | text_editing | A | ☐ |
| `TextJustifyAll` | `motion.tool.text_justify_all` |  | Ctrl+Shift+F | Cmd+Shift+F | text_editing | A | ☐ |
| `TextJustifyLeft` | `motion.tool.text_justify_left` |  | Ctrl+Shift+J | Cmd+Shift+J | text_editing | A | ☐ |
| `TextJustifyRight` | `motion.tool.text_justify_right` |  | Ctrl+Alt+Shift+J | Cmd+Opt+Shift+J | text_editing | A | ☐ |
| `TextResetHorizontalScale` | `motion.tool.text_reset_horizontal_scale` |  | Ctrl+Shift+X | Cmd+Shift+X | text_editing | A | ☐ |
| `TextResetTracking` | `motion.tool.text_reset_tracking` |  | Ctrl+Shift+Q | Control+Cmd+Shift+Q | text_editing | A | ☐ |
| `TextResetVerticalScale` | `motion.tool.text_reset_vertical_scale` |  | Ctrl+Alt+Shift+X | Cmd+Opt+Shift+X | text_editing | A | ☐ |
| `TextSelectToDocumentEnd` | `motion.tool.text_select_to_document_end` |  | Ctrl+Shift+End | Cmd+Shift+End | text_editing | A | ☐ |
| `TextSelectToDocumentStart` | `motion.tool.text_select_to_document_start` |  | Ctrl+Shift+Home | Cmd+Shift+Home | text_editing | A | ☐ |
| `TextSelectToDown` | `motion.tool.text_select_to_down` |  | Shift+Down | Shift+Down | text_editing | A | ☐ |
| `TextSelectToLeft` | `motion.tool.text_select_to_left` |  | Shift+Left | Shift+Left | text_editing | A | ☐ |
| `TextSelectToLineEnd` | `motion.tool.text_select_to_line_end` |  | Shift+End | Shift+End | text_editing | A | ☐ |
| `TextSelectToLineStart` | `motion.tool.text_select_to_line_start` |  | Shift+Home | Shift+Home | text_editing | A | ☐ |
| `TextSelectToRight` | `motion.tool.text_select_to_right` |  | Shift+Right | Shift+Right | text_editing | A | ☐ |
| `TextSelectToUp` | `motion.tool.text_select_to_up` |  | Shift+Up | Shift+Up | text_editing | A | ☐ |
| `TextSelectToWordDown` | `motion.tool.text_select_to_word_down` |  | Ctrl+Shift+Down | Cmd+Shift+Down | text_editing | A | ☐ |
| `TextSelectToWordLeft` | `motion.tool.text_select_to_word_left` |  | Ctrl+Shift+Left | Cmd+Shift+Left | text_editing | A | ☐ |
| `TextSelectToWordRight` | `motion.tool.text_select_to_word_right` |  | Ctrl+Shift+Right | Cmd+Shift+Right | text_editing | A | ☐ |
| `TextSelectToWordUp` | `motion.tool.text_select_to_word_up` |  | Ctrl+Shift+Up | Cmd+Shift+Up | text_editing | A | ☐ |
| `TextSmallCaps` | `motion.tool.text_small_caps` |  | Ctrl+Alt+Shift+K | Cmd+Opt+Shift+K | text_editing | A | ☐ |
| `TextSubScript` | `motion.tool.text_sub_script` |  | Ctrl+Alt+Shift+= | Cmd+Opt+Shift+= | text_editing | A | ☐ |
| `TextSuperScript` | `motion.tool.text_super_script` |  | Ctrl+Shift+= | Cmd+Shift+= | text_editing | A | ☐ |
| `TextToggleComposer` | `motion.tool.text_toggle_composer` |  | Ctrl+Alt+Shift+T | Cmd+Opt+Shift+T | text_editing | A | ☐ |

### `Tracker`

| AE 命令 | BAE 动作 id | 参数 | Windows | macOS | 作用范围 | 负责 | 状态 |
|---|---|---|---|---|---|---|---|
| `MTNudgeDown` | `motion.tool.tracker_nudge_down` |  | Down | Down | tracker | E | ⏸ |
| `MTNudgeDownHoldPos` | `motion.tool.tracker_nudge_down_hold_pos` |  | Alt+Down | Opt+Down | tracker | E | ⏸ |
| `MTNudgeDownHoldPosMore` | `motion.tool.tracker_nudge_down_hold_pos_more` |  | Alt+Shift+Down | Opt+Shift+Down | tracker | E | ⏸ |
| `MTNudgeDownMore` | `motion.tool.tracker_nudge_down_more` |  | Shift+Down | Shift+Down | tracker | E | ⏸ |
| `MTNudgeLeft` | `motion.tool.tracker_nudge_left` |  | Left | Left | tracker | E | ⏸ |
| `MTNudgeLeftHoldPos` | `motion.tool.tracker_nudge_left_hold_pos` |  | Alt+Left | Opt+Left | tracker | E | ⏸ |
| `MTNudgeLeftHoldPosMore` | `motion.tool.tracker_nudge_left_hold_pos_more` |  | Alt+Shift+Left | Opt+Shift+Left | tracker | E | ⏸ |
| `MTNudgeLeftMore` | `motion.tool.tracker_nudge_left_more` |  | Shift+Left | Shift+Left | tracker | E | ⏸ |
| `MTNudgeRight` | `motion.tool.tracker_nudge_right` |  | Right | Right | tracker | E | ⏸ |
| `MTNudgeRightHoldPos` | `motion.tool.tracker_nudge_right_hold_pos` |  | Alt+Right | Opt+Right | tracker | E | ⏸ |
| `MTNudgeRightHoldPosMore` | `motion.tool.tracker_nudge_right_hold_pos_more` |  | Alt+Shift+Right | Opt+Shift+Right | tracker | E | ⏸ |
| `MTNudgeRightMore` | `motion.tool.tracker_nudge_right_more` |  | Shift+Right | Shift+Right | tracker | E | ⏸ |
| `MTNudgeUp` | `motion.tool.tracker_nudge_up` |  | Up | Up | tracker | E | ⏸ |
| `MTNudgeUpHoldPos` | `motion.tool.tracker_nudge_up_hold_pos` |  | Alt+Up | Opt+Up | tracker | E | ⏸ |
| `MTNudgeUpHoldPosMore` | `motion.tool.tracker_nudge_up_hold_pos_more` |  | Alt+Shift+Up | Opt+Shift+Up | tracker | E | ⏸ |
| `MTNudgeUpMore` | `motion.tool.tracker_nudge_up_more` |  | Shift+Up | Shift+Up | tracker | E | ⏸ |
| `MTSetEnd` | `motion.tool.tracker_set_end` |  | Alt+] | Opt+] | tracker | E | ⏸ |
| `MTSetStart` | `motion.tool.tracker_set_start` |  | Alt+[ | Opt+[ | tracker | E | ⏸ |

