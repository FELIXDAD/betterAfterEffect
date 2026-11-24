# 0.0.1 最终回归（macOS，Apple M4）

## 结论

**没有 0.0.1-blocker。** 建议可以发布 macOS 版。

这一轮提了 4 个 bug（都不挡发布）：

| issue | 级别 | 问题 |
|---|---|---|
| #204 | bug | 编辑 ▸ 复制 / 粘贴 选中图层时什么都不做（只支持关键帧），菜单项却可用、也没有提示 |
| #205 | bug | 合成设置：在“持续时间”按 Tab 后焦点落到背景颜色色块，再按 Enter 打开拾色器而不是确定 |
| #206 | bug | 首选项 ▸ 项目：中文界面下“新建项目纯色文件夹”默认值是 Solids，和实际新建的“纯色”文件夹不一致 |
| #207 | bug | 渲染：范围只有 1 帧时，输出文件名里的 `####` / `[####]` 不替换成帧号，逐帧渲染会互相覆盖 |

旧清单（第 1–8 节和复测记录）在新基准上全部仍然通过。

## 被测版本

- dev `160fab5`（发布 PR #98 合入，版本 0.0.1）。
- CI run 44995382166 的产物 `bae-aarch64-apple-darwin`（`BAE-0.0.1-aarch64-macos.zip`），解压到带空格和中文的路径 `…/验收 应用 目录/中文 路径/BAE.app`。真实程序的结论都来自这个包。
- 这个提交的 CI（run 44995382166）两平台都绿，包括工作区里开发写的测试（`bae-prefs`、`bae-ui`、`bae-ui-motion` 等）；下面说“由开发的测试覆盖”的项以这次 CI 为准。
- 验收用例在同一提交上本机编译运行（界面用例在程序内驱动真实的工作台和文档，不需要窗口）。
- 测试时机器上还有别的工作树在编译（load average 5–18），耗时数字有波动，下面写明。

## 怎么测的

- **A 节（旧清单）**：`cargo test -p bae -- --include-ignored`，把 `apps/bae/src/acceptance/` 的全部用例（含两个手动的满盘用例）和 `tests/acceptance_cli.rs` 跑一遍；没自动化的项（签名、Gatekeeper、图标和文档类型、发布包首次打开、耗时和内存）用 CI 包手工测。
- **B 节（AE 对齐 P0）**：新加 `acceptance::release_001`，以 `docs/ui/ae-commands.md`（由命令目录生成，逐项对应 AE 26 的菜单和快捷键文件）为准，数据驱动地检查每个快捷键和菜单项；工具、对话框、首选项、蒙版、保存往返各写了用户操作级的用例；蒙版渲染用 CI 包的 `bae mcp` 建工程、`bae render` 出图、读像素。
- **C 节**：中英文界面扫描（画出来的文字和无障碍树，按 `i18n_scan.rs` 的白名单约定）、50 次连续操作、200 层工程、发布包检查。

## 结果

### A. 旧清单全量复跑

| 项 | 结果 | 说明 |
|---|---|---|
| 自动化用例（第 1–8 节 + 复测补的边界用例） | 通过 | 程序内 216 个全部通过（含 2 个手动用例：4 MB 内存盘上渲染满盘），命令行 24 个全部通过 |
| 1.1 发布包从带空格和中文的路径启动 | 通过 | `--headless --smoke-test 9`（中文）、窗口 `--smoke-test 60` 都通过（屏幕锁着，窗口帧在屏外，和之前一样） |
| 1.2 签名 / Gatekeeper | 通过 | `codesign --verify --deep --strict` 有效；`syspolicy_check` 0 个签名错误；`spctl` 只有普通的 rejected（ad-hoc 签名、没有公证，走“仍要打开”，不是“已损坏”），和 #105 修好后一致 |
| 1.3 版本号 | 通过 | `bae --version` = `bae 0.0.1 (aarch64-macos, release)`，Info.plist `CFBundleShortVersionString` 0.0.1 |
| 图标、文档类型 | 通过 | `bae.icns`；`io.github.felixdad.bae.project` / `.package` 两个 UTI |
| 包体积 | 通过 | 24,309,280 B = 23.18 MiB，预算 32 MiB（上次 24,072,784 B，+236,496 B） |
| 7.1 B1（61 层）导出 | 通过 | PNG 300 帧 0.531 / 0.502 / 0.481 s（中位 0.502 s，上次 0.484 s）；不压缩 TIFF 1.375 / 1.117 / 1.223 s（上次中位 1.323 s）。另有两次 TIFF 5.4 s、10.4 s 和 5.7 s，都发生在别的工作树编译写盘时（每次写 2.5 GB），空闲时复测回到 1.1–1.4 s；EXR 300 帧文件数正确 |
| 7.2 内存 | 通过 | B1 300 帧峰值 RSS：PNG 189–261 MB，TIFF 361–363 MB（上次 170–179 / 356 MB） |

本轮按 AE 26 改掉的旧行为（用例已按 AE 26 更新，不算失败）：

- 图层 ▸ 新建 里的“矩形 / 椭圆”删除（AE 没有），形状改由形状工具创建；用例用 `add_layer(h, "shape:rect")`（#172）。
- Cmd+N / Cmd+Y / 新建摄像机 / 新建灯光 先打开设置对话框，确定后才新建（AE 的行为，`docs/reference/ae/ui/dialogs/composition-settings.json` 等）；用例里 `comp_with` / `new_solid` 按确定处理。
- 效果菜单整体置灰（0.0.1 不接效果库），只有 效果 ▸ 效果控件 可用（AE 里也可用，打开面板）。

### B. 本轮新增（AE 对齐 P0）

| 项 | 结果 | 说明 |
|---|---|---|
| B1 菜单栏 | 通过 | 每个可用的菜单项（556 个里可用的 112 个），在“有合成、有三种图层、选中一个、有关键帧”的状态下单独点一次：除 #204 外都有处理（没有点了没反应的）；效果菜单置灰。菜单结构、顺序、中英文名和 AE 26 的对照由开发的目录测试覆盖（`catalog::tests`），这里没有重复 |
| B2 快捷键 | 通过 | 目录里已实现（✅ / ◐）且有 macOS 快捷键的 120 个组合，逐个在新状态下按：每个都只运行一次对应命令（查看器的缩放在查看器有焦点时按，AE 也只在查看器上下文里绑定 `.` `,`）。菜单上显示的快捷键和 AE 一致（只看带修饰键的；单键如 `.` `U` 空格 原生菜单不显示，避免在输入框里误触发，这是有意的）。跳过的：只在素材 / 图层面板生效的 17 个、对话框内的 2 个、小键盘 `PadClear` |
| B3 工具 | 通过 | 查看器里拖拽：Q 依次切到矩形 / 圆角矩形 / 椭圆 / 多边形 / 星形，没选中图层时各建一个形状图层（内容是“矩形 1”“椭圆 1”“多边星形 1”），每次一个撤销步骤；Shift 拖出正方形；选中形状图层时往它里面加一个组；选中纯色时画蒙版（一个撤销步骤）；钢笔（G）点三点再点起点：选中纯色时是闭合蒙版，没选中时是形状图层；文字工具（Cmd+T）点击、输入“Hi ”再用输入法组字“你好”、Cmd+Enter 提交，图层名“Hi 你好”。工具快捷键、弹出组、按住临时切换由开发的 `acceptance::tools` 覆盖，都通过 |
| B4 对话框 | 通过（#205） | 合成设置：宽度 0 / -100 → 1，999999 / 1e9 → 30000，abc / 空 → 保持上一次的值，12.7 → 12；帧速率 0 / -5 / 1000 / abc / 空 → 保持 30；持续时间 0 / -1 / abc → 1 帧；“100” 按 AE 的时间码输入法是 0:00:01:00。名字“我的 合成 戏 ✨”（空格、中文直接输入加输入法组字、emoji）原样保存；新建是一个撤销步骤；Esc 不建合成、不加撤销步骤。纯色设置：Esc 不建，Enter 建，中文名正确，宽度 0 不出错。另存 .bae 再打开，导出的 JSON 完全相同。**问题**：在持续时间按 Tab 再按 Enter 会打开拾色器（#205）。图层设置、摄像机、灯光、预合成、导入（Cmd+I 和拖文件）、拾色器由开发的 `acceptance::dialogs` 覆盖，都通过 |
| B5 首选项 | 通过（#206） | 16 页都能打开（`app.settings` 带页面 id），中英文扫描见 C 节；设置文件损坏时，CI 包正常启动（GUI 模式），坏文件被改名为 `preferences.json.corrupt`，用默认值。页面顺序、每页控件和 AE 一致、确定 / 取消 / Esc、已接线的设置（撤销级数、标签颜色、颜色主题、网格）由开发的测试覆盖，都通过。**问题**：项目页的“新建项目纯色文件夹”中文默认值是 Solids（#206） |
| B6 图表编辑器 | 通过 | 值 / 速度图表切换、中文“像素”“像素/秒”（#203 的用例）通过；拖关键帧和手柄、右键菜单、关键帧插值 / 速度对话框由 `bae-ui-motion` 的 `timeline_tests` 覆盖，都通过；两个关键帧对话框的中英文扫描通过 |
| B7 蒙版 | 通过 | CI 包，200×200 黑底合成里全画面白色纯色，中心 100×100 矩形蒙版，渲染第 0 帧取像素（红通道）：相加 中心 255 / 外面 0；反转 中心 0 / 外面 255；单个相减 中心 0 / 外面 255；羽化 40 中心 255、边上 124（约一半）、边内 10 像素 187。`bae pack` 成 .baez 后 `export-json` 和 .bae 完全相同 |
| B8 保存往返 | 通过 | 有摄像机、灯光、蒙版的工程上，把 图层 的子菜单（开关：消隐、锁定、独奏、折叠、运动模糊、调整图层、视频 / 音频，品质，排列，蒙版自由变换点）和 编辑 ▸ 标签 的 36 个可用项都点一遍，另存 .bae 和 .baez，分别重新打开，`export_flattened_json` 和保存前完全相同 |

### C. 跨项检查

| 项 | 结果 | 说明 |
|---|---|---|
| 中英文界面扫描 | 通过（#206） | 16 个首选项页、合成设置、新建纯色、图层设置、新建摄像机、新建灯光、预合成、关键帧插值、关键帧速度、图表编辑器、形状和钢笔工具的选项栏，扫画出来的文字和无障碍树：英文界面没有中文；中文界面除 #206 外没有英文。扫出来的其他英文都是 AE 官方中文本来就保留的（`docs/reference/ae/ui/preferences-dialog.json` 的 labelZh：Dynamic Link、NTSC、Alpha、Hz、MME、XMP、RAM、AE/Pr/AME/Au、Mercury Transmit、JavaScript、MB/GB；项目页的分组标题 Project Options、Solids Folder 和音频的 System Default 在 AE 中文版里也是英文；合成设置的 HD、8bpc；钢笔的 RotoBezier），按约定加进 `i18n_scan.rs` 的白名单并写明出处；颜色值（#2D2D2D）和带单位的数字（50mm）不算 |
| 性能 | 通过 | 200 层（一半形状、一半纯色）、10 分钟的合成，CI 包 `--headless --smoke-test`：9 帧 557 ms、30 帧 589 ms，即首次载入和首帧约 0.55 s，之后每帧约 1.5 ms；B1 每帧约 1.9 ms。工具拖拽和菜单在程序内用例里（debug 构建）没有可见的卡顿 |
| 稳定 | 通过 | 合成设置（Esc / Enter 交替）、新建纯色、新建摄像机、新建灯光、图层设置、预合成各连续 50 次，再用矩形工具连续拖 50 次、撤销 50 次：不崩溃，没有残留对话框，图层数每一步都对（debug 构建 40 s） |
| 渲染输出命名 | **#207** | 范围只有 1 帧时 `-o name_####.png` 写出的文件就叫 `name_####.png`（`[####]` 也一样），逐帧渲染会互相覆盖 |
| 效果菜单 | 通过 | 除 效果 ▸ 效果控件 外全部置灰 |

## 其他观察（不开 issue）

- 视图 ▸ 放大 / 缩小 在时间轴有焦点时可用但不起作用；查看器有焦点时正常。AE 的快捷键文件只在查看器上下文里绑定 `.` `,`，菜单在时间轴焦点下的行为参考数据里没有，先记录。
- `docs/ui/ae-commands.md` 里 设置 ▸ 各页 的状态还是 ☐（计划中），但首选项已经实现、菜单也可用（菜单可用与否由运行时有没有处理器决定，不看文档状态）。
- 合成宽度最小是 1（AE 是 4）；输入 12.7 得 12。

## 本机无法自动验证的项，以及替代方式

| 项 | 原因 | 替代方式 |
|---|---|---|
| 系统输入法真实组字 | agent 终端没有辅助功能权限 | 程序内注入 IME 事件（CompositionStart/Update/End），文字工具、合成名都测了 |
| 原生文件对话框、拖文件的真实界面 | 同上 | `FileDialogs` 注入路径；拖文件由 `acceptance::dialogs` 注入 `DroppedFile` |
| 窗口画面 | 屏幕锁着，窗口帧在屏外 | `--headless --smoke-test` 加 `BAE_SMOKE_SCREENSHOT` 截图（中文 B1 截图里工具栏图标、菜单、面板都正常） |
| Gatekeeper 弹框 | 会弹系统框 | `codesign` / `spctl` / `syspolicy_check` 的结果 |
| 首选项“重启后仍保留” | 不能在真实 GUI 里点设置 | `bae-prefs` 的文件往返测试 + 程序内“确定后生效”的用例；真实程序验证了坏文件的处理 |

## 回归测试（随本报告的 PR）

- `apps/bae/src/acceptance/release_001.rs`（新）：
  - `every_implemented_ae_shortcut_runs_its_command_once`、`menus_show_the_ae_shortcuts`、`every_enabled_menu_item_does_something`（读 `docs/ui/ae-commands.md`；#204 写在 `KNOWN_NOOPS`）
  - `viewer_tools_create_shapes_masks_paths_and_text`
  - `dialogs_handle_bad_numbers_names_escape_undo_and_reopen`
  - `new_layer_properties_survive_save_as_bae_and_baez`
  - `new_dialogs_and_tools_fifty_times_in_a_row`
  - `new_ui_has_no_english_in_chinese`、`new_ui_has_no_chinese_in_english`
- `i18n_retest.rs`：扫描时整句放行 AE 中文版保留的三句英文、颜色值和带单位的数字；#206 写在 `KNOWN_ISSUES`。
- `i18n_scan.rs`：白名单加了上面 AE 官方中文保留的缩写，每组写明出处。
- 只改测试和报告，没有产品代码。

## Launch Services

现在注册了三个 BAE：`bae-01-size-pass/target/dist/BAE.app`（开发构建）、`~/Downloads/BAE-0.0.1-rc/app/BAE.app`（之前的候选版）、`/private/tmp/bae-acc-m4/final/…/BAE.app`（本轮回归用的 160fab5 CI 包）。按约定先把清单给协调人，确认要保留哪一个后再用 `lsregister -u` 注销其余的。

## 复测记录

### dev `c4537d7`（#210）：#204 图层复制 / 剪切 / 粘贴

`acceptance::release_001::layers_copy_cut_and_paste_like_ae`（新）和 `every_enabled_menu_item_does_something`（去掉了对复制 / 粘贴的放行）都通过：

- 剪贴板是空的时候，编辑 ▸ 粘贴 置灰；复制后可用。
- 时间轴有焦点，选中形状图层和纯色两个图层 Cmd+C，再选中中间的文本图层 Cmd+V：多出 2 个图层，放在文本图层正上方，名字和对同样两个图层按 Cmd+D 重复得到的一样，粘贴出来的两个图层是选中状态；一个撤销步骤（撤销回到 3 个、重做回到 5 个）。编辑 ▸ 粘贴 菜单项效果一样。
- Cmd+X 剪掉纯色（一个撤销步骤），Cmd+V 粘回来；新建第二个合成后 Cmd+V，纯色进了第二个合成。
- 粘贴后另存 .bae（带空格和中文的路径）重新打开，导出的 JSON 完全相同。
- 按 Enter 改名、输入框有焦点时，Cmd+C / Cmd+V 不复制、不粘贴图层（原生菜单的剪切 / 复制 / 粘贴在输入框有焦点时交给输入框，`shell::menu::TEXT_EDIT_ACTIONS`）。
- 观察：粘贴的名字按“重复”的规则递增。剪掉“Dark Gray Solid 1”后粘回同一个合成叫“Dark Gray Solid 4”（2、3 已存在），粘到空合成叫“Dark Gray Solid 2”。AE 粘贴到别的合成时是否保留原名，参考数据里没有，先记录。

**#204 已关闭。**

### dev `d384697`（#211；#157 的修复在 #169）：#207 单帧输出名、#157 开始时的进度总数

- **#207**：`bae render --frames 5-5` 再 `--frames 6-6`，PNG / TIFF / EXR 各用 `帧 a_####.ext` 和 `帧 b_[####].ext` 两种写法，输出到带空格和中文的目录：6 种组合都写出 `…_0005` 和 `…_0006` 两个文件，第二次没有覆盖第一次，没有名字里带 `#` 的文件（`acceptance_cli::single_frame_renders_number_the_output_name`）。渲染队列里起止帧相同（5–5、6–6）、输出 `单帧_[####].ext`，三种格式都是 `单帧_0005`、`单帧_0006`（`render_queue::a_single_frame_numbers_the_output_name`）。**已关闭。**
- **#157**：`progress_of_a_limited_range_counts_the_frames_it_renders`（1780–1900 帧，合成只有 1800 帧，实际渲 20 帧）现在连“0 / x”那一刻也检查，连跑 8 次，第一个状态都是 (0, 20)，之后的总数也都是 20，没有再出现 121。**已关闭。**
- 顺手发现：`bae render --comp` 只接受内部路径名（`主_合成`），用界面上的合成名“主 合成”会报 invalid prim name，提了 **#214**（bug）。

### dev `5aa3b76`（#209）：#205 Tab 到色块后按 Enter

`acceptance::release_001::enter_is_ok_after_tab_to_a_swatch`（新）：合成设置里在“持续时间”输入 100、按 Tab（焦点到背景颜色色块）、按 Enter：确定并关闭，持续时间 0:00:01:00。纯色设置从“宽度”一路 Tab 到色块再按 Enter：确定，建出纯色。焦点在色块上时按空格仍然打开拾色器；拾色器里按 Enter 只关拾色器，再按 Enter 才确定合成设置。**已关闭。**

### dev `6911155`（#212 #221 #219 #224）：#206 #215 #216 #220

- **#206**（#212）：`i18n_retest` 去掉了对 `Solids` 的放行，`new_ui_has_no_english_in_chinese` 照常通过：中文界面首选项 ▸ 项目 显示“纯色”；英文界面扫描也通过（没有中文）。开发的 `settings::solids_folder_default_matches_the_folder_bae_makes_in_chinese` 通过。**已关闭。**
- **#215**（#221）：`close_project_leaves_no_old_comp_name_in_the_panels`（新）：合成名“合成 测试”、加纯色、另存，关闭前面板标签和无障碍树里有这个名字；文件 ▸ 关闭项目 之后，画出来的文字和无障碍树里都没有了，也没有打开的合成；重新打开工程后名字又出现。**已关闭。**
- **#216**（#219，Windows 问题）：`render_queue_output_defaults_to_movies_on_macos`（新）：macOS 上新渲染项的默认输出仍在 `~/Movies` 下。Windows 部分由 5090 复核，issue 等那边再关。
- **#220**（#224，Windows 问题）：`cmd_modifier_c_and_v_run_their_commands_on_macos`（新）：Cmd+Shift+C 打开预合成对话框；Cmd+Opt+Shift+C 打开摄像机设置，确定后建出摄像机；Cmd+C / Cmd+V 照样复制粘贴图层；改名输入框里粘贴（shell 把原生菜单的粘贴交给 egui 的 Paste）得到文字“粘贴 文字”，图层数不变。目录里全部已实现快捷键的用例（121 个组合）也通过。Windows 部分由 5090 复核。

### dev `bf7ccb0`（#238）：#222 输入框撤销恢复出输入法组字文本（macOS 部分）

`acceptance::release_001::undo_in_text_fields_skips_ime_composition`（新）：按 egui-winit 转来的 macOS 输入法事件顺序注入（逐步 Preedit，每步停约 1.25 秒 → 空 Preedit → Commit），在项目面板搜索框、图层改名框、文字工具的输入里各做一遍：组字 `w'q'g't` 提交 `wqgt`，停一下，全选后粘贴 `Dark`，停一下，组字 `ni'hao` 提交 `你好`，再打 `X`，然后连按 6 次 Cmd+Z。三处的撤销序列都是 `Dark你好X` → `Dark` → `wqgt` → 空，没有出现任何组字中间文本，工程的撤销也没变。反向检查：换回修复前的 `bae-ui/src/workbench.rs`，三处都撤销出 `Darkni'hao`、`Darkni'h`、`w'q'g't`，用例失败。Windows 微软拼音部分等 5090 测试，issue 先不关。
