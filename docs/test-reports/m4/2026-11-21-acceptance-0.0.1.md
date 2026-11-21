# macOS 实机：0.0.1 独立验收（Apple M4）

- 日期：2026-11-21
- 机器：MacBook（Apple M4），macOS 26.7.3，本机 Metal
- **被测提交：`origin/dev` @ `fb08040`**（`test-report: Windows Pr 实机核对 docs/spec/pr 待核条目 (#101)`）
- 被测程序：
  - 本地 `cargo build --release -p bae`（2 分 34 秒，`bae 0.1.0 (aarch64-macos, release)`，23 785 536 字节）
  - CI 产物 `bae-aarch64-apple-darwin`（run 44900440561，`BAE-0.1.0-aarch64-macos.zip`，size report 22.68 MiB / 预算 30 MiB）
- 分支：`test/acceptance-0.0.1`；回归测试和本报告在同一个 PR 里

## 结论

| | 项数（下面各表的行数） |
|---|---:|
| 验收项 | 55 |
| 通过 | 40 |
| 未通过（已开 issue） | 13 |
| 只记录数字 | 2（1.4 版本号、7.6 帧时间） |
| 0.0.1-blocker | 1（#105） |

一行里可能包含多个子用例，例如 4.4 是 12 种组合，1.5 是 3 种关法，都算一项。

**0.0.1-blocker：#105** —— CI 打出的 `BAE.app` 签名无效（bundle 组装后没有重签），用浏览器下载的发布包会被 Gatekeeper 当作“已损坏”打不开。协调人已转给开发 agent。

主流程本身（新建合成 → 加图层 → 打关键帧 → 预览 → 渲染队列导出 → 保存 / 重新打开）在 macOS 上走得通：没有发现数据丢失或崩溃。另外 11 个 bug 都是体验问题，和 AE 的习惯不一致，或者错误提示不清楚，列在最后。

## 怎么测的

本机 agent 终端没有辅助功能权限，合成的系统键盘鼠标事件会被丢掉；验收期间屏幕处于锁定状态（`CGSSessionScreenIsLocked=1`），窗口不能上屏。所以分三层测：

1. **程序内模拟输入（主要手段）**：`apps/bae/src/acceptance/` 里的 `Gui` 是没有 winit 窗口的 shell。
   - 应用按 `main.rs` 原样组装：同样的模块，同样的形状 / 文字图层源。
   - 每帧跑真实的 `Workspace::ui`，返回的调用经过同一个 `CommitGate`，按 `Shell::run_pending` / `Shell::dispatch` 的顺序分发：先给焦点面板和工作台，再处理退出和文件命令，最后交给 Document。
   - 输入是 egui 事件：按键（含修饰键）、文字、IME 组字（`Ime::Preedit` → `Ime::Commit`，和 egui-winit 0.36 在 macOS 上送的一样）、指针。每个事件单独一帧，帧后马上分发，和窗口里一样；另有一个用例故意把多个事件塞进同一帧。
   - 定位控件用 accesskit 标签（`egui_kittest`），找不到标签的自绘行用画出来的文字（例如时间轴的图层名）定位，不写死坐标。
   - 原生文件对话框和消息框换成 `FileDialogs` 的脚本实现：注入路径和“保存 / 不保存 / 取消”的回答，并记录弹过哪些框。
   - 原生菜单按 `Registry<MenuItemSpec>` 里的数据检查：标题、可用状态、显示的快捷键和执行的调用都和 `AppMenu::sync` 推给 NSMenu 的是同一份。
2. **真实可执行文件**（`apps/bae/tests/acceptance_cli.rs`，以及手动命令）：
   - `--version`、`--headless <工程>`、`--headless --smoke-test N`（配 `BAE_SMOKE_SCREENSHOT`）、`export-json`、`render`、`mcp`（stdio JSON-RPC）。
   - GUI 从带空格和中文的路径启动，用 `CGWindowList` 读窗口标题；单实例检查。
3. **性能**：B1（`cargo run --release -p bae-render --example b1_project`）用 release 二进制计时；另做了一次 4K、3600 帧的长渲染，每 0.25 秒采样一次 RSS。

全部回归测试：`cargo test -p bae`，bin 里 66 个通过 / 14 个忽略，CLI 测试 7 个通过 / 1 个忽略。连续跑 3 次结果相同，一次约 21 秒。忽略的测试都标了 `#[ignore = "issue #N"]`，只有磁盘满一项因为要建内存盘，标为手动。

## 结果

### 1. 启动

| # | 项 | 结果 | 说明 |
|---|---|---|---|
| 1.1 | .app 从带空格和中文的路径启动 | 通过 | CI 包解压到 `/private/tmp/…/验收 应用 目录/中文 路径/BAE.app`。`--headless --smoke-test 30` 通过（1.40 ms/帧）。GUI 用同目录下的 `打开 测试.bae` 启动，日志显示 `opened …/打开 测试.bae`，窗口标题 “BAE - 打开 测试.bae” |
| 1.2 | .app 签名 / Gatekeeper | **未通过 #105（blocker）** | `codesign --verify --deep --strict`：code has no resources but signature indicates they must be present；`syspolicy_check distribution`：Fatal |
| 1.3 | 应用图标、文档类型 | 未通过 #115 | 没有图标；`.bae` 在访达里是普通文件夹，双击不能用 BAE 打开 |
| 1.4 | 版本号 | 记录 | dev 上 `bae 0.1.0`，About 显示 `BAE 0.1.0 (aarch64-macos, …)`。改成 0.0.1 在 `release/0.0.1` 分支（#98）里做；#115 提醒合并后确认 Info.plist 和 zip 名也跟着改 |
| 1.5 | 欢迎框：关闭按钮 / Esc / 点框外 | 通过 ×3 | 三种关法都关得掉，关掉后 Cmd+N 正常；选“审阅”会切到审阅工作区 |
| 1.6 | 欢迎框打开时的快捷键 | 未通过 #112 | 欢迎框是模态的，但在框里按 Cmd+N 会在框后面新建合成 |
| 1.7 | 单实例 | 通过 | 第二个 GUI 进程打印 `BAE is already running (pid …)`，退出码 1 |

### 2. 合成和图层

| # | 项 | 结果 | 说明 |
|---|---|---|---|
| 2.1 | Cmd+N 新建合成 | 通过 | 依次得到 Comp 1、Comp 2；时间轴标签变成 “Timeline Comp 1”；撤销 / 重做 |
| 2.2 | 图层 ▸ 新建的每一项（菜单） | 通过 | 文本、纯色、灯光、摄像机、空对象、形状图层、调整图层、矩形、椭圆共 9 项：类型正确，新图层在最上面并被选中；9 次撤销清空，9 次重做恢复 |
| 2.3 | 新建图层的快捷键 | 通过 | Cmd+Y、Cmd+Alt+Y、Cmd+Alt+Shift+Y / T / C / L |
| 2.4 | 没有合成时置灰 | 通过 | 9 项新建、修剪、加入渲染队列、排列、重复、拆分、缓动、定格都置灰。对应的快捷键什么也不做：工程不变脏，没有失败的命令，Cmd+M 不加入队列。关闭项目后重新置灰 |
| 2.5 | 排列（快捷键和菜单） | 通过 | 置顶、前移、后移、置底的顺序都对。已经在最上 / 最下时不产生撤销步骤 |
| 2.6 | 重复、拆分 | 通过（功能） | 副本在原图层正上方；拆分出的两段在 CTI 处衔接；7 步撤销回到初始状态 |
| 2.7 | 重复 / 拆分后的选中 | 未通过 #108 | 仍选中原图层（AE 选中新图层），所以连续拆分时第二次静默失败 |

### 3. 时间轴

| # | 项 | 结果 | 说明 |
|---|---|---|---|
| 3.1 | 单击选中、Cmd+A、Cmd+Shift+A | 通过 | |
| 3.2 | Shift+单击 | 未通过 #107 | 效果是逐个加选，AE 是选中连续范围 |
| 3.3 | 改名：双击进入，Enter 提交 / 点别处提交 / Esc 取消 | 通过 | 每次提交是一个撤销步骤 |
| 3.4 | 改名：Enter 进入，改名框全选原名 | 未通过 #106 | 时间轴里按 Enter 不进入改名；直接输入得到 “Solid 1Hero”。项目面板的 Enter 改名正常 |
| 3.5 | 中文（模拟输入法组字）、空格、emoji、超长名字 | 通过 | “bei jing” 组字上屏为 “背景”，再接 “层”。“␣␣spaced␣␣name␣␣”、“Comp 背景 ✨ 🎬👩‍👩‍👧”、2000 字符的长名字都存得进去；全空白的名字不接受；5 次撤销回到 “Solid 1” |
| 3.6 | 秒表、Option+Shift+P、J / K | 通过 | 秒表在 0 处打关键帧；15、45 帧处用 Option+Shift+P 添加；在已有关键帧上再按是删除（和 AE 一样）。J / K 在 0 → 15 → 45 帧之间跳，到头停住。逐步撤销 / 重做；关掉秒表会删掉全部关键帧，撤销可以恢复 |
| 3.7 | 空格：焦点在文本框 / 按钮 / 空处 | 通过 ×3 | 文本框里输入空格（“A B”），不开始播放；焦点按钮上按空格是按下按钮（渲染队列的“移除”）；没有焦点时开始 / 停止播放，时间前进，每次按键只执行一次 |

### 4. 保存和打开

| # | 项 | 结果 | 说明 |
|---|---|---|---|
| 4.1 | 另存为 .bae / .baez，前后完全一致 | 通过 | 工程含 3 个图层（其中一个用输入法改名为 “标题 01”），3 个位置关键帧（其中 1 个定格）和 1 个缩放关键帧。另存为无扩展名的 “动画 工程 v1.2” 得到 `.bae`，再存一份 `打包 文件.baez`；两份读回的 `export_flattened_json` 都和保存前**完全相等**（关键帧数量、时间、值、插值、图层名） |
| 4.2 | Cmd+S 静默保存、标题上的未保存标记 | 通过 | 有修改时标题是 “BAE - … *”，保存后去掉 `*`；Cmd+S 不弹框，直接写回 .baez；之前那份 .bae 没被改动 |
| 4.3 | 改名框还开着就按 Cmd+S | 通过 | 先提交改名再保存（#100 的 CommitGate），文件里是新名字 |
| 4.4 | 有未保存修改时 关闭 / 退出 / 新建 / 打开 × 保存 / 不保存 / 取消 | 通过 ×12 | 每种都问了一次，问的是正确的文件名。取消：工程和磁盘都不变，也不退出。不保存：磁盘不变，按动作切换工程或退出。保存：写回磁盘后再切换或退出 |
| 4.5 | 未命名工程在提示里选“保存” | 通过 | 另存为框取消时什么都不关；给了路径就保存后退出；工程是干净的时候不问 |
| 4.6 | 命令行打开（路径带空格、中文） | 通过 | `bae --headless <.bae / .baez>`、GUI `bae <工程>` |
| 4.7 | 目录只读 | 通过 | 另存为 .bae / .baez 都报“无法保存项目”并写出目录名；工程仍是脏的，没有残留文件。退出时选“保存”失败，应用不退出。已保存工程所在目录变成只读后，Cmd+S 报错，工程仍是脏的 |
| 4.8 | 覆盖已有文件 | 通过 | 另存为覆盖已有的 .bae 目录和 .baez，读回是新内容，旁边没有残留的临时目录 |
| 4.9 | 打开损坏的文件 | 通过（不崩溃、不影响当前工程） | 试了 9 种：随机字节、截断、空的 .baez，截断、结构错误、二进制、未来版本的 project.json，空的 .bae 目录，不存在的路径。都只报一次错，在问“是否保存”之前就报，当前工程不变 |
| 4.10 | 损坏文件的提示是否清楚 | 未通过 #111 | 不存在的路径说成 “is not a BAE project”；损坏的 .baez 是 “zip: invalid Zip archive: Could not find EOCD”；`export-json` / `render` 的错误不写文件名 |
| 4.11 | 重新打开后的显示 | 未通过 #113 | 重新打开后时间轴和查看器都是空的（截图：`acceptance-0.0.1-shots/b1-reopened-zh.png`），要在项目面板里双击合成才显示；数据本身是完整的 |
| 4.12 | `bae mcp` 建工程 → export-json | 通过 | 中文路径，`--create`，3 个位置关键帧，自动保存 |

### 5. 渲染队列

| # | 项 | 结果 | 说明 |
|---|---|---|---|
| 5.1 | Cmd+M 加入 | 通过 | 默认 PNG，`~/Movies/Comp 1/Comp 1_[####].png`，范围 0–299（工作区域），同时打开渲染队列面板 |
| 5.2 | PNG / TIFF / EXR，范围，带空格和中文的路径 | 通过 | 分别渲 0–9、5–9、290–299 帧到 `…/输出 png/帧 序列_[####].png` 这类路径：文件数和编号都对；完成后显示“完成 · N · x 秒”和“在访达中显示” |
| 5.3 | 渲染中停止 | 通过 | 4K、1800 帧，渲到第 20 帧以后点“停止”。面板显示 “Stopped (N/1800 frames)”，N 等于磁盘上的文件数；文件从 0 开始连续编号，每个都是完整的 PNG（结尾是 IEND）。渲染时“渲染”按钮置灰 |
| 5.4 | 无效路径 | 通过 | 父路径是文件、目录只读：状态都是“失败”，面板上显示了原因 |
| 5.5 | 空路径、无扩展名、范围超出合成 | 未通过 #110 | 空路径会写到进程的当前目录；`noext_[####]` 写出没有扩展名的文件；298–305 帧把合成结束后的 6 帧也渲出来了 |
| 5.6 | 磁盘满 | 通过 | 4 MB 的内存盘（`hdiutil attach -nomount ram://8192`）上渲 TIFF：失败，提示 “cannot write /Volumes/BAEFULL/满 盘_0000.tif: No space left on device (os error 28)”，不留残缺文件，应用继续可用（手动用例 `full_disk_fails_cleanly`） |
| 5.7 | 在访达中显示 | 通过 | 用 `PATH` 最前面的假 `open` 记录参数，确认调用的是 `open -R …/显示 在 访达/帧 _0000.png`（一个参数，空格和中文原样）；没有真的弹出访达 |
| 5.8 | 渲染时继续编辑；队列里多项 | 通过 | 3 项（PNG 120 帧、TIFF 5 帧、EXR 5 帧）共 5.9 秒，期间界面跑了 2208 帧，新建了 5 个图层并撤销 1 次。3 项都完成，文件数正确，编辑都保留在工程里 |
| 5.9 | `bae render` 输出到带空格和中文的路径 | 通过 | 60 帧 |

### 6. 按键路由和菜单

| # | 项 | 结果 | 说明 |
|---|---|---|---|
| 6.1 | 每个注册菜单项的可用 / 置灰 | 通过 | 28 项分 4 种状态检查：没有合成、有合成但没选中、选中图层、选中关键帧。在选中关键帧状态下从菜单执行“缓动”会跑 `key.ease` |
| 6.2 | 显示的快捷键是否和 AE 一致 | 未通过 #109 | 26 项和 AE 对照，25 项一致；“将合成修剪至工作区域”缺 Cmd+Shift+X。撤消 / 重做在原生菜单里不显示快捷键（设计如此，快捷键可用），也写在 #109 里 |
| 6.3 | 中文菜单名是否用 AE 的译名 | 未通过 #109 | 26 项里 21 项和 AE 26 简体中文一致，4 个“排列”项和“修剪”不一致 |
| 6.4 | `BAE_LANG=zh` / `en` | 通过 | 在单独的子进程里跑。zh：菜单路径、文件对话框文字、面板（项目、时间轴）都是中文。en：界面上没有中文字符 |
| 6.5 | 各面板有焦点时快捷键是否生效、会不会重复执行 | 通过 | 焦点分别在项目、时间轴、合成查看器、效果和预设、渲染队列面板上：Cmd+Y 都只执行一次 `motion.new_layer`，Cmd+Z / Cmd+Shift+Z 生效 |
| 6.6 | 文本框保留自己的键 | 通过 | 项目搜索框里，Backspace / Delete 不删图层，Cmd+Z 不撤销工程，Cmd+A 不全选图层，J、K、P、空格、U 都当文字输入，不触发快捷键。离开文本框后，时间轴焦点下 Delete 删除选中图层 |

### 7. 性能和稳定

| # | 项 | 结果 | 说明 |
|---|---|---|---|
| 7.1 | B1（61 层）导出耗时 | 通过 | 下表，和发布说明里的 M4 数字一致 |
| 7.2 | 长时间渲染的内存 | 通过 | 4K、3600 帧 PNG，11.1 秒：RSS 在前两个四分之一时段最高都是 579 MB，后两段 611 MB，平稳不随帧数增长。B1 300 帧：PNG 峰值 170–179 MB，TIFF 356 MB |
| 7.3 | 快速重复执行命令 | 通过 | 连按 100 次 Cmd+Y、50 次 Cmd+D、20 次拆分、全选删除再撤销，没有崩溃；名字唯一（拆分失败见 #108） |
| 7.4 | 同一帧里的多次按键 | 未通过 #114 | 5 次 Cmd+Y 在同一帧里，得到 5 个都叫 “Solid 1” 的图层 |
| 7.5 | 很深的撤销栈 | 通过 | 1200 次编辑后连按 Cmd+Z，正好退回 1000 步（`UNDO_LIMIT = 1000`，比 AE 最多 99 级多），再重做 1000 步，工程和撤销前完全相等 |
| 7.6 | 200 层时界面一帧的耗时 | 记录 | debug 构建，不含 GPU 绘制：约 27 ms/帧 |

B1 导出（release，`bae render B1.bae --comp b1 --depth 8 --resolution full --frames all`，墙钟含进程启动；测量时机器负载 load average 6–8）：

| 输出 | 3 次（秒） | 中位数 | 发布说明（M4） |
|---|---|---:|---:|
| PNG | 0.758 / 0.484 / 0.476 | **0.484 s** | 0.459 s |
| 不压缩 TIFF | 1.323 / 1.255 / 1.409 | **1.323 s** | 1.429 s |

（PNG 第 1 次偏慢是冷缓存。TIFF 300 帧共 2.49 GB，瓶颈在写盘：encoder stall 871 ms。）

### 8. 其他（第一次使用时可能遇到的）

| # | 项 | 结果 | 说明 |
|---|---|---|---|
| 8.1 | 无障碍 / 自动化 | 未通过 #116 | 122 个可见控件里 112 个没有 accesskit 标签，VoiceOver 读不出来 |
| 8.2 | 发布包首次打开 | 见 #105、#115 | 签名、图标、文档类型 |

## 提的 issue

| issue | 级别 | 标题 | 对应测试 |
|---|---|---|---|
| #105 | **0.0.1-blocker** | macOS: CI 打出的 BAE.app 签名无效，下载后会被 Gatekeeper 当作“已损坏” | 手动（codesign / syspolicy_check） |
| #106 | bug | 时间轴改名：Enter 不能开始改名；改名框不全选原名 | `timeline::enter_on_a_selected_timeline_layer_starts_rename`、`timeline::rename_field_starts_with_the_whole_name_selected` |
| #107 | bug | 时间轴 Shift+单击是逐个加选，AE 是连续范围 | `timeline::selection_by_click_and_shift_click` |
| #108 | bug | 复制 / 拆分图层后仍选中原图层；连续拆分第二次静默失败 | `layers::duplicate_and_split_select_the_new_layer` |
| #109 | bug | 菜单：修剪缺 Cmd+Shift+X；5 个中文菜单名和 AE 26 不一致 | `menus::shortcuts_shown_in_menus_match_ae`、`menus::chinese_menus_use_ae_terms` |
| #110 | bug | 渲染队列：空路径写到当前目录；无扩展名；帧范围可超出合成 | `render_queue::empty_output_path_is_refused`、`…::output_without_extension_gets_one`、`…::frame_range_outside_the_comp` |
| #111 | bug | 打开损坏 / 不存在的工程时错误信息不清楚 | `acceptance_cli::export_json_names_a_broken_file`（提示全文见 `files_flow::broken_files_…` 的输出） |
| #112 | bug | 欢迎框（模态）打开时快捷键仍作用于工程 | `startup::shortcuts_behind_the_welcome_dialog` |
| #113 | bug | 重新打开工程后没有打开任何合成 | `files_flow::reopening_shows_the_comp_that_was_open` |
| #114 | bug | 同一帧里的多次 Cmd+Y / Cmd+N 生成同名图层 | `stress::key_presses_piled_up_in_one_frame` |
| #115 | bug | macOS 应用包：没有图标、没有文档类型 | 手动 |
| #116 | bug | 无障碍：大多数控件没有 accesskit 标签 | `startup::controls_have_accessibility_labels` |

修好后把对应测试的 `#[ignore]` 去掉就是复测。

## 本机无法自动验证的项，以及替代方式

| 项 | 为什么不能直接测 | 替代方式 |
|---|---|---|
| 系统输入法的真实组字（拼音候选框的位置、上屏） | 终端没有辅助功能权限，`ime_driver.swift` 发不了按键 | 在 egui 层注入和 egui-winit 相同的 `Ime::Preedit` / `Ime::Commit` 序列：上屏结果、接着组字、组字中按 J/K/P/空格不触发快捷键都验证了；候选框跟随光标由 `bae-ui` 现有的 `ime_follows_caret` / `ime_composition_in_text_fields` 测试覆盖。真实拼音仍需要发布前人工检查（#98 的清单第 1 项） |
| 原生对话框（NSSavePanel / NSOpenPanel / NSAlert）本身 | 不能点击 | 文件流程全部经过 `FileDialogs` 的脚本实现，覆盖每个“保存 / 不保存 / 取消”分支、取消对话框、出错提示；`shell/dialogs.rs` 的原生实现和系统自带的“是否替换”确认没有覆盖 |
| 原生菜单栏（NSMenu）上的点击和快捷键 | 不能发按键 | 用 `AppMenu::sync` 读的同一份数据检查标题、可用状态、快捷键、调用（`MenuItemSpec::is_enabled`、keymap 的 chord）；菜单抢先吃掉按键后是否只执行一次，以及 Cmd+Q，没法在本机验证 |
| 真实窗口截图 | 验收期间屏幕锁定，窗口被遮挡；带窗口的冒烟测试 120 帧全部走离屏渲染 | 用 `--headless --smoke-test` + `BAE_SMOKE_SCREENSHOT` 截图，走同一套 egui 渲染器；窗口标题从 `CGWindowList` 读到 |
| “在访达中显示”真的打开访达 | 不能打扰屏幕 | 用假 `open` 记录参数（见 5.7） |
| Gatekeeper 弹框 | 会在屏幕上弹框 | 用 `codesign --verify`、`spctl -a`、`syspolicy_check distribution` 的结果判断（#105） |
| 鼠标拖动（图层条、关键帧、查看器里拖动） | 本轮没做 | 查看器拖动由 `bae-ui-motion` 现有测试覆盖，其余留给人工检查 |

## 回归测试（随本报告的 PR）

- `apps/bae/src/acceptance/`（只在 macOS 上编译）：
  - `mod.rs`：没有窗口的 shell 和输入工具
  - `startup.rs`、`layers.rs`、`timeline.rs`、`files_flow.rs`、`render_queue.rs`、`menus.rs`、`stress.rs`
- `apps/bae/tests/acceptance_cli.rs`：真实可执行文件
- 新增依赖：`egui_kittest`（工作区里已经声明，0.36.2，default-features = false），只作为 `apps/bae` 的 dev-dependency，release 体积不变
- 唯一改到的非测试文件：`apps/bae/src/main.rs` 加了一行 `#[cfg(all(test, target_os = "macos"))] mod acceptance;`，测试模块只能这样接进 bin crate
- 没有 GPU 时，渲染队列的测试自动跳过

## 复测记录

### dev `8c0221d`（#120）：#105

CI 产物按用户下载的方式检查：`ditto` 解压，再加上 `com.apple.quarantine`。

- `codesign --verify --deep --strict`：valid on disk / satisfies its Designated Requirement
- `spctl`：只有普通的 `rejected`（ad-hoc 签名、没有公证，Gatekeeper 走“仍要打开”的流程，不是“已损坏”）
- `syspolicy_check`：只剩 ad-hoc 警告和缺公证票据
- 应用能正常运行

**#105 已关闭。**

### dev `763eaad`（#121）：#106 #107 #108 #109 #110 #112 #114 #117 #118

#121 去掉 ignore 的验收测试全部通过。另外从用户角度加了 19 个边界用例（其中 4 个因为还没修好标了 ignore），随复测 PR 提交：

| issue | 结果 | 补测的边界情况 |
|---|---|---|
| #106 | 通过，已关闭 | Enter 打开后全选原名，拼音组字直接替换；提交用的 Enter 不会再打开改名框；选中多个图层时改最上面的；焦点在按钮上时 Enter 不开始改名；双击打开时同样全选 |
| #107 | 通过，已关闭 | 范围向下、向上、从同一锚点缩小、换锚点、没有锚点时；Cmd+单击加选 / 减选。跨越隐藏消隐图层的情况不在 0.0.1 范围内 |
| #108 | 通过，已关闭 | 连续拆分 4 次，每次选中新段，5 段首尾相接，撤销 / 重做；同时复制多个图层，副本都被选中；撤销后选择里不留已删除的图层 |
| #109 | 通过，已关闭 | Cmd+Shift+X 实际把合成修剪成 2 秒的工作区域，撤销恢复；“修剪”一项按 glossary 保留“将合成修剪至工作区域”（AE 别处也用“工作区域”），测试期望值已按 glossary 更新 |
| #110 | **部分通过，保持打开** | 空路径 / 只有空格、缺扩展名、范围超出合成都修好了。还有三处：进度的总帧数用的是没限制的范围（显示 x / 121，实际渲 20 帧）；把文件夹当输出时文件写在文件夹旁边；`.png` 名字配 TIFF 格式时写出的是 TIFF 数据 |
| #112 | 通过，已关闭 | 模态框打开时，Cmd+N / Cmd+Y / 空格 / Enter / Delete / Cmd+S / Cmd+M 都不影响工程；`Workspace::modal_open()` 为真；Esc 关闭后恢复 |
| #114 | 通过，已关闭 | 已有改名为 “Solid 7” 的图层时，同一帧的纯色 + 调整图层得到 Solid 8–10、Adjustment Layer 1–2；同一帧两次 Cmd+N 得到 Comp 2、Comp 3 |
| #117 | 通过（数据层），已关闭 | 撤消 Cmd+Z、重做 Cmd+Shift+Z、键盘快捷键 Cmd+Alt+'、Window 面板项的快捷键都和 AE 一致。原生菜单的实际显示在本机没法目测，Windows 侧请确认 |
| #118 | 通过，已关闭 | 窗口 ▸ 渲染队列（Cmd+Alt+0）可以打开 / 关闭；面板关掉再打开后，队列里的项目还在；Window ▸ 项目能把被挡住的项目面板带到前面 |

新发现：**#124**。锁定开关不起作用：锁定的图层照样能被点选、范围选择、全选、改名、删除、排列。这是在测“范围选择跨越锁定图层”时发现的，#121 之前就存在。协调人已定为 0.0.1 要修，和 #110 剩下的三处一起交给了开发 agent。

**不在 0.0.1 范围内**：“隐藏消隐图层”（Hide Shy Layers）0.0.1 没有这个功能，所以“范围选择跨越隐藏的消隐图层”这项不测（协调人确认）。

### dev `d23a328`（#123）：#113 #111 #116

#123 去掉 ignore 的测试全部通过。又补了 8 个用例（其中 1 个对应还没修好的部分，标了 ignore）：

| issue | 结果 | 补测的边界情况 |
|---|---|---|
| #113 | 通过，已关闭 | 两个合成、不同时间、第二个是当前合成，`.bae` 和 `.baez` 重新打开后都恢复。视图变化不算未保存、不增加撤销步骤、关闭时不提示保存。`view` 记录过时或坏了（合成已删除、类型不对、时间超出合成或为负、没有记录的旧工程）时，打开第一个合成。修复前保存的 B1 工程打开后显示合成 b1（截图 `acceptance-0.0.1-shots/b1-reopened-zh-d23a328.png`，修复前的对比图是 `b1-reopened-zh.png`） |
| #111 | **部分通过，保持打开** | 中文提示 7 种情况都写明了文件和原因（不存在、空文件夹、.baez 损坏、更新版本保存的、缺工程层数据、工程层数据损坏、没有读取权限）；命令行错误带文件名。**还剩**：`export-json` / `render` 的错误前仍多一行带时间戳的 `ERROR bae_platform::cli` 日志。小问题：没有读取权限时原因是系统英文 |
| #116 | 通过，已关闭 | 121 个控件里 120 个有名字。工具、工作区、面板关闭和菜单按钮、合成标签、图层行、秒表、当前时间、工作区域、搜索、渲染都能按名字找到；accesskit Click 能切换工具（`toggled` 状态正确）、选中图层；中文标签正确，没有残留英文。建议（不阻塞）：同类控件重名，7 个秒表都叫“时间变化秒表”，图层开关不带图层名 |

release 体积：24,072,784 B（+287,248 B，来自 accesskit 适配器），macOS 22.96 MiB / 30 MiB，PASS。

### dev `c20770a`（#127）：#110 #124

#127 带上了 #125 的前两个测试提交，并去掉了 #110 和 #124 的 ignore，这些测试都通过。又补了 11 个用例：

| issue | 结果 | 补测的边界情况 |
|---|---|---|
| #110 | 通过，已关闭 | 中文文件夹（已有的、末尾带斜杠的新文件夹）里写合成名命名的文件，非法字符换成 `_`；PNG→TIFF→EXR→PNG→EXR 来回切换，名字不变、扩展名跟着变，文件头正确；手写的扩展名和格式不一致时面板有提示，写出的是格式的扩展名；起止帧输入 -25 / 99999 变成 0 / 299，结束不会早于开始 |
| #124 | **部分通过，保持打开** | 通过：用真正的锁定开关锁定和解锁、撤销和重做；锁定图层不能指定父级，但可以当父级，父级移动时锁定的子级跟着动；锁定图层的关键帧可以复制、不能粘贴进去；复制、拆分、排列、F9 都不作用于锁定图层；查看器点击穿过锁定图层。**还剩**：① MCP 的通用 `attr_set`（位置、名字）和 `prim_delete` 还能改、能删锁定图层；② 删除锁定子级的父级时什么都不发生、也没有提示；③ 锁定一个选中的图层后它仍是选中状态 |

为了算锁定子级的世界变换，`apps/bae` 的 dev-dependencies 加了 `bae-anim`（工作区内部 crate，已经在依赖树里，release 体积不变）。

### dev `7e44f42`（#122）：#115

用的是 CI 产物（run 44912303305），`ditto` 解压到带空格和中文的目录：

| 项 | 结果 |
|---|---|
| 图标 | `bae.icns`，16–1024 px 共 8 个尺寸，图案是自己设计的（`acceptance-0.0.1-shots/app-icon-256.png`）；加上隔离属性后签名仍然有效 |
| 文档类型 / UTI | .bae 是 `io.github.felixdad.bae.project`（被当作包），.baez 是 `io.github.felixdad.bae.package`；`lsregister -f` 之后 Launch Services 认得这两种类型，都由 BAE 打开 |
| 应用没启动时 `open` .bae | 应用启动，通过 file.open_project 打开（日志 `bae::files: opened`），标题 “BAE - 访达 打开.bae” |
| 应用在运行时 `open` .baez | 同一个进程切换过去，没有第二个实例；同时打开两个文件时只开第一个 |
| `open` 损坏的 .baez | 弹出写清原因的提示，当前工程不变，没有崩溃 |
| 恢复上次的合成 | 两个合成标签都在，当前是“第二 合成”，时间 1 秒（`acceptance-0.0.1-shots/view-restored-zh-7e44f42.png`） |

**#115 已关闭。** Windows 的双击打开要安装器写文件关联，按说明不在 0.0.1 范围内。

本机另一个工作区的开发构建也注册了同一个 bundle id，现在是默认应用，所以打开时用 `open -a` 指定了 CI 的应用；测完已经注销测试用的这份应用。

新开 **#131**（小问题）：`bae pack` 在 `--help` 里列出，但没有实现；`package_pack` 打包时丢掉 `project.json` 的 `view` 字段。

### dev `2c94655`（#130）：#124 收尾

#130 去掉 ignore 的 3 个测试通过（MCP 的通用 `attr_set` / `prim_delete`、删除锁定子图层的父级、锁定后取消选中）。又补了 6 个用例：

- **MCP 的每个入口**：`attr_set`、`attr_set_many`、`attr_clear`、`attr_set_expression`、`attr_set_at`、`prim_rename`、`prim_move`、`prim_create`（在锁定图层下建子 prim）、`prim_delete`、`layer_delete`、`layer_shift_time`、`layer_trim`、`layer_set_parent` 都被拒绝，消息里写明图层名（`layer "锁定 层" is locked; unlock it first`）。读取、移动别的图层越过它、复制它、改别的图层都可以。
- **批量命令**：`command_batch` 里只要有一步碰到锁定图层，整批都不生效；“解锁、改值、再锁定”放在一批里可以执行，撤销和重做都是一步。
- **跨工程层**：用户在 root 层锁定的图层，写自己层的 agent 不能改、不能删，但可以关掉锁定（和 AE 脚本一样）。
- **history_revert** 回到锁定之前的版本时，会把整个版本连同“未锁定”一起恢复，不会出现“仍然锁定但值被改了”的情况。agent 自己的 `history_undo` 和用户的撤销按历史回放，不受锁定限制（按设计）。
- **界面**：被拒绝的修改在状态栏有提示，中英文都有（“图层“Solid 1”已锁定，请先解锁。”），下一次修改后清除；被拒绝的修改不产生撤销步骤，工程也不变脏。删除锁定子图层的父级后，撤销会把父级和父子关系都恢复，重做会再删掉。

**#124 已关闭。** Windows 在 `c20770a` 上复测的界面部分也通过了。
