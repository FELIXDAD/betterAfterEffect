# After Effects 26 实机待核问题汇总（14 个开发 PR）

> **采集环境**：Windows 11 Pro x64（3840×2160 @ 150% DPI），Adobe After Effects 26.0（`26.0x67`，`zh_CN`）。
> **采集方式**：ExtendScript 实时回读 + Win32 `WM_COMMAND`/键鼠实测 + Win32 `MN_GETHMENU` 弹出菜单直读 + MSAA 模态框控件采集 + 既有三批交互实测（#190/#191/#192）与 #176 / `menus.json` / `preferences.json`。
> **统计**：覆盖 **14 个 PR**、**55 条待核项**（其中发现 **20 处与真 AE 26 的差异**，已逐条评论回对应 PR）。完整结构化数据见 [`verifications.json`](verifications.json)。

## 重要差异速览（建议优先修正）

| PR | 负责人 | 待核项 | 真 AE 26 实测行为 |
|---:|:---:|---|---|
| #168 | A | 圆角矩形 ↑↓ 步长、多边形/星形圆度步长、星形默认内径 | 圆角矩形拖拽中 Up/Down 每次 ±1 px（Right=min(w,h)/2，Left=0），与当前实现一致；星形默认内径 = 0.5 × 外径，与当前实现一致；但星形拖拽中 Right/Left 调整外圆度步长为 ±10%（实测 Right×3 → 30%），而非 5%。Up/Down 调边数/角数 ±1（默认 5 且跨次拖拽持久化，双击工具图标重置为 5）。 |
| #168 | A | 蒙版颜色循环规则与起始色 | 首选项 Appearance 默认开启“循环遮罩颜色”（Cycle Mask Label Colors）及“使用对比色”（Use Contrasting Color For Mask Path Enabled, choices=5）。实测连续建 6 个蒙版的 RGB 顺序为 #B3C7B3(淡绿) → #E8920D(橙) → #F46DD6(紫红) → #B53838(红) → #E4D84C(黄) → #E7C19E(桃红)，不是固定从黄色开始。 |
| #168 | A | 选中多个图层时绘制形状工具画在哪个图层上 | 实测同时选中 2 个纯色图层（TopSolid + BottomSolid）拖矩形工具：AE 不在任一图层上建蒙版，而是直接新建一个形状图层（形状图层 1）插在最上方选中图层之上（L1）！ |
| #171 | A | RotoBezier 的精确切线公式与张力 | 实测 RotoBezier 闭合三角形 3 个顶点（与 Catmull-Rom 1/6 弦长不同）：方向 u = normalize(v_{i+1} - v_{i-1})；长度分别等于各自相邻边长的精确 1/3，即 inTangent = -(1/3) * |v_i - v_{i-1}| * u，outTangent = +(1/3) * |v_{i+1} - v_i| * u（实测 6 条切线比值均为 0.3333）。 |
| #181 | A | 段落文本图层的锚点与位置 | 实测拖出 (400,300)→(900,550) 段落框（500×250）：Layer Anchor=[0,0,0]，但 Layer Position = [650, 425, 0]（拖出框的几何中心，不是左上角！），而 TextDocument.boxTextPos = [-250, -125]（相对 Position 的左上角偏移 [-w/2, -h/2]）。当前实现将 Position 设为框左上角与 AE 不同。 |
| #181 | A | 查看器鼠标滚轮每格缩放倍率 | 实测滚轮每格并非 1.15 倍：向上滚放大乘以 9/8 = 1.125（1.0 → 1.125 → 1.265625），向下滚缩小乘以 7/8 = 0.875（1.0 → 0.875 → 0.765625）。 |
| #167 | E | 上次的合成设置是仅本次运行记住还是跨重启记住 | 跨重启记住！AE 26 将上次的合成设置（尺寸、帧率、锁定长宽比、开始时间码等）持久化写入偏好文件 [Composition Pref Section]，冷启动后 Ctrl+N 直接恢复上次会话的设置（本机实测冷启打开仍为上次 960×540 @ 60fps）。 |
| #175 | E | 改纯色尺寸时锚点的处理 | AE 并非仅在锚点居中时移到新中心，而是对所有锚点按新旧尺寸等比缩放：`ax' = ax * (newW / oldW)`、`ay' = ay * (newH / oldH)`（Position 不变）。实测 400×300 → 600×500：居中锚点 [200,150] → [300,250]；偏心锚点 [50,50] → [75, 83.3333]。 |
| #179 | E | 摄像机设置里“单位”控制哪些字段 | 单位控制全部 5 个长度字段（缩放 zoom、胶片大小 filmSize、焦距 focalLength、对焦距离 focusDistance、光圈 aperture），而非仅管胶片大小和焦长！实测新建摄像机对话框默认单位为毫米，对话框显示 zoom=1254.32、filmSize=36.00、focalLength=66.67、focusDistance=1254.32、aperture=8.93、fStop=7.5，而建出图层内部按 72 ppi 存像素：Zoom=3555.5556 px（=1254.32×72/25.4）、Aperture=25.30934 px（=8.93×72/25.4）。 |
| #179 | E | 灯光默认类型、默认参数与中文命名格式 | 默认类型为聚光（SPOT），Intensity=100、Color=[1,1,1,1]、ConeAngle=90°、ConeFeather=50%、Falloff=无(1)、FalloffStart=500、FalloffDistance=500、CastsShadows=关、ShadowDarkness=100%、ShadowDiffusion=0、Position=[cx+80, cy-80, -666.66667]。注意中文默认名为 `聚光 1`（不带“灯”字！当前写“聚光灯 1”有细微差异）。 |
| #183 | E | 单个图层与多个图层时的默认名称及中文格式 | 单个图层 `MySolid` 时默认名称为 `MySolid 合成 1`（中文为 `<图层名> 合成 1`，不是 `Comp 1`）；选中多个图层时默认名称为 `预合成 1`。 |
| #183 | E | “移动”后跨合成父级与轨道遮罩的处理 | 跨合成父级（父层留在外部）：AE 会断开父级（insideParent=null），但**烘焙并保留子层的世界/合成画面位置**，不让图层跳位（实测父在 [300,200]、子本地 [100,50] → 预合成内部子层位置为 [300,150]）。当前实现直接断开保留局部值会导致跳位，建议修正！轨道遮罩（仅移动填充层、遮罩层留在外部）：轨道遮罩直接断开清空（insideHasMatte=false、outerHasMatte=false）。 |
| #183 | E | “保留”时内部图层的开始时间、新合成尺寸/时长与所在文件夹 | 实测对 startTime=1, in=2, out=6 的 400×300 纯色执行“保留”：新合成尺寸 = 400×300（源尺寸）、时长 = 10s（原合成时长）、所在文件夹 = 原合成所在文件夹（SubFolder）；但**内部图层的 startTime=0, inPoint=0, outPoint=10**（从 0 开始！），原图层的 startTime=1 / in=2 / out=6 留在外层预合成图层上。当前实现将内部图层 startTime 设为原图层 startTime 会造成双倍偏移，请修正！ |
| #174 | C | 关键帧速度对话框：多维属性行标签与“连续”复选框文案 | 区分空间属性与非空间多维属性：(1) Position 等空间属性合并为单行 `速度:` / `影响:`（不拆维度）；(2) Scale 等非空间多维属性上方有 `锁定维度` 复选框（默认勾选），行标签为 `维度: X`、`维度: Y`（而非 `维度 1/2`）。底部复选框精确中文为 `连续（锁定传出到传入）`（EN: Continuous (Lock Outgoing to Incoming)）。 |
| #174 | C | Ctrl/Cmd 单击已选中的关键帧：只改单击的还是改全部选中的 | 改**全部选中的关键帧**！实测全选 3 个线性关键帧 [30f,90f,150f]，Ctrl 单击中间帧 90f，全部 3 个关键帧同时切换为 BEZIER/BEZIER。当前实现“只改单击的”与 AE 不同，建议调整。 |
| #174 | C | Alt 缩放时间穿过未选中关键帧时是否覆盖 | 穿过时**不覆盖/不删除**未选中关键帧！实测 4 帧 [1s,2s,4s,5s] 选中前 3 帧，Alt+拖 4s→6s 跨越未选中的 5s：结果为 [30f(sel), 80f(sel), 150f(未选中原值完好), 180f(sel)]。此外须注意 Alt 必须在 mouse-down 前按住才进入等比缩放模式，拖拽后再按 Alt 仍为刚性平移（见 PR #192）。 |
| #185 | C | 滑动编辑：Alt 拖图层条能否滑动；滑动时图层标记是否跟着源走 | (1) **Alt 拖图层条主体不能滑动编辑**，它仍整体移动图层条（实测 in=3..7,st=0 → in=4..8,st=1）；需用向后平移工具（Y）拖图层条（或拖源素材淡色条）才执行滑动（实测 in=3..7 不动、st=0→1）。(2) 滑动时**图层标记跟着源走**（保持图层时间，合成时间同步平移：4.0s → 5.0s），与当前实现一致！ |
| #172 | B | 文件 ▸ 导入 下“从库…”与“消失点 (.vpe)…”的准确中文名 | AE 26.0 zh_CN 精确名称分别为：`从 Libraries 中...`（cmd=9764867，非“从库…”）和 `Vanishing Point (.vpe)...`（cmd=9764869，zh_CN 下保持英文，非“消失点 (.vpe)…”）。 |
| #172 | B | 图层 ▸ 轨道遮罩 ▸ 以上方/下方图层作为遮罩 的精确中文名 | AE 26.0 zh_CN 菜单路径为 `图层(L) ▸ 跟踪遮罩(A) ▸ 使用上方图层遮罩` / `使用下方图层遮罩`（子项各 4 个：`Alpha 遮罩`、`Alpha 反转遮罩`、`亮度遮罩`、`亮度反转遮罩`；顶部第一项为 `没有轨道遮罩`）。 |
| #172 | B | 视图 ▸ 拆分并新建锁定的查看器 的精确中文名 | AE 26.0 zh_CN 精确名称为 `视图(V) ▸ 与新锁定的查看器分开`（cmd=589825, Ctrl+Alt+Shift+N），非“拆分并新建锁定的查看器”。 |

## A（工具 / 查看器）

### PR #168 — Shape tools (shapes, groups, masks)

关联文档：`docs/reference/ae/interaction/shapes-pen-type.md (PR #190)`、`docs/reference/ae/preferences.json`

1. **圆角矩形 ↑↓ 步长、多边形/星形圆度步长、星形默认内径** — ⚠️ 与 AE 26 有差异
   - 圆角矩形拖拽中 Up/Down 每次 ±1 px（Right=min(w,h)/2，Left=0），与当前实现一致；星形默认内径 = 0.5 × 外径，与当前实现一致；但星形拖拽中 Right/Left 调整外圆度步长为 ±10%（实测 Right×3 → 30%），而非 5%。Up/Down 调边数/角数 ±1（默认 5 且跨次拖拽持久化，双击工具图标重置为 5）。
   - *依据*：`PR #190 shapes-pen-type.json: rrect_empty_up_x3=23, star_empty_right_x3=30, star_empty_drag outer=200 inner=100`
2. **新形状图层的变换（锚点/位置/组位置）及“在新形状图层上居中放置锚点”首选项** — ✅ 与 AE 26 一致
   - 首选项关闭时：Layer Anchor=[0,0,0]、Layer Position=合成中心 [960,540,0]、Group.Position = 拖出框中心 - 合成中心；首选项开启（Main Pref Section v2 / Shape Layer Center Anchor Point = true）时：Layer Anchor 移至形状中心，Layer Position 同步补偿保持画面不动。
   - *依据*：`PR #190 shapes-pen-type.md §2.1-2.2 & rect_empty_center_anchor_on`
3. **蒙版颜色循环规则与起始色** — ⚠️ 与 AE 26 有差异
   - 首选项 Appearance 默认开启“循环遮罩颜色”（Cycle Mask Label Colors）及“使用对比色”（Use Contrasting Color For Mask Path Enabled, choices=5）。实测连续建 6 个蒙版的 RGB 顺序为 #B3C7B3(淡绿) → #E8920D(橙) → #F46DD6(紫红) → #B53838(红) → #E4D84C(黄) → #E7C19E(桃红)，不是固定从黄色开始。
   - *依据*：`verify_A.json: p168_mask_color_cycle & preferences.json [Mask Colors]`
4. **选中多个图层时绘制形状工具画在哪个图层上** — ⚠️ 与 AE 26 有差异
   - 实测同时选中 2 个纯色图层（TopSolid + BottomSolid）拖矩形工具：AE 不在任一图层上建蒙版，而是直接新建一个形状图层（形状图层 1）插在最上方选中图层之上（L1）！
   - *依据*：`verify_A.json: p168_rect_two_solids_both_sel (nL=3, L1=形状图层 1)`
5. **选中空对象 / 摄像机 / 灯光时拖拽形状工具的行为** — 🟡 部分一致 / 合理差异
   - 选中摄像机或灯光时：新建形状图层放在其上方（L1），与当前实现一致。选中空对象（Null）时：因为 AE 的 Null 底层是 100×100 AVLayer，AE 会在 Null 上建蒙版（nL 仍为 1）；BAE 的 Null 无纯色实体，保持新建形状图层是合理差异。
   - *依据*：`verify_A.json: p168_rect_on_null (nL=1, Mask 1), p168_rect_on_camera (nL=2), p168_rect_on_light (nL=2)`
6. **选中形状组时新组加在组内还是 Contents 顶部** — ✅ 与 AE 26 一致
   - 仅选中形状层内的 ExistingGroup 后拖椭圆工具：新组“椭圆 1”添加在根 Contents 索引 1（与 ExistingGroup 同级并在其上方），不嵌套进 ExistingGroup。
   - *依据*：`verify_A.json: p168_ellipse_with_group_selected`

### PR #171 — Pen tools (open/closed paths, vertices, RotoBezier)

关联文档：`docs/reference/ae/interaction/shapes-pen-type.md (PR #190)`

1. **Shift 单击的角度约束步长（45° 还是 15°）** — ✅ 与 AE 26 一致
   - 实测为 45° 步长（不是 15°）。从 (400,400) Shift 单击 (600,355)（-12.7°）吸附到 0°（[-360,-140]），再 Shift 单击 (800,270)（-33°）吸附到 -45°（[-160,-340]）。当前实现 45° 正确。
   - *依据*：`verify_A.json: p171_pen_shift_angle_step vertices=[[-560,-140],[-360,-140],[-160,-340]]`
2. **RotoBezier 的精确切线公式与张力** — ⚠️ 与 AE 26 有差异
   - 实测 RotoBezier 闭合三角形 3 个顶点（与 Catmull-Rom 1/6 弦长不同）：方向 u = normalize(v_{i+1} - v_{i-1})；长度分别等于各自相邻边长的精确 1/3，即 inTangent = -(1/3) * |v_i - v_{i-1}| * u，outTangent = +(1/3) * |v_{i+1} - v_i| * u（实测 6 条切线比值均为 0.3333）。
   - *依据*：`PR #190 shapes-pen-type.json: pen_empty_rotobezier (in/dprev=0.3333, out/dnext=0.3333 on all 3 vertices)`
3. **转换顶点工具：单击角点 vs 拖拽角点** — ℹ️ AE 26 实测基准
   - 在 AE 中转换“顶点”工具单击角点不会自动拉出手柄（仍为 [0,0]），需按住拖动拉出对称手柄；单击已有手柄的平滑点则将双侧切线归零恢复为角点。另外钢笔工具单选/悬停仅作用于当前选中图层的路径。
   - *依据*：`verify_A.json: p171_convert_vertex_click & PR #190 pen_empty_bezier_curve / pen_empty_alt_break`

### PR #177 — Selection / Rotation / Pan Behind / Snapping / Nudge

关联文档：`docs/reference/ae/interaction/transform-tools.md (PR #191)`

1. **手柄缩放是否以锚点为中心；多选时其他图层如何缩放** — ✅ 与 AE 26 一致
   - 单选实测以图层自身锚点为中心缩放（偏心锚点 [0,0] 实测确认，见 PR #191）。多选两图层（LeftBox@[600,540] + RightBox@[1200,540]）拖动 LeftBox 右下角手柄：每个选中图层各自绕自己的锚点应用相同缩放比例（两者均变为 Scale=[200,200]，Position 各自保持 [600,540] 与 [1200,540] 不变）。
   - *依据*：`PR #191 sel_scale_offcenter_anchor & verify_A2.json: p177_multi_select_handle_scale`
2. **吸附规则（点对点 vs 按轴）、距离阈值、目标点种类** — ℹ️ AE 26 实测基准
   - 详见 PR #191 transform-tools.md：支持按轴独立吸附（X/Y 分别吸附）与点对点吸附，目标涵盖合成中心/边界与其他图层的中心/边界/角点；100% 缩放（150% DPI）下像素阈值约 9–12 物理像素；按住 Ctrl(Cmd) 拖拽可反转当前“对齐”开关。
   - *依据*：`PR #191 transform-tools.json (snap_center / snap_edge / snap_threshold cases)`
3. **微移步长与 3D 图层的方向键微移方向** — ✅ 与 AE 26 一致
   - 2D 方向键按当前缩放下 1 屏幕像素（Shift=10px，100%=1px、200%=0.5px、50%=2px，见 PR #191）。3D 图层绕 Y 轴旋转 45° 后按 Right/Up：Position 从 [960,540,0] 变为 [961,540,0] → [961,539,0]，沿合成/视图屏幕 X/Y 轴移动（Z 不变），不沿图层本地旋转轴。
   - *依据*：`PR #191 transform-tools.md & verify_A2.json: p177_3d_nudge_right / p177_3d_nudge_right_then_up`
4. **旋转工具（W）在空白处拖动是否旋转选中图层** — ✅ 与 AE 26 一致
   - 不旋转！实测选中 RotBox 后用旋转工具（W）在空白画布 (100,100)→(250,200) 拖动，Rotation 保持 0°。必须从选中图层上开始拖拽，与当前实现一致。
   - *依据*：`verify_A2.json: p177_rotate_drag_empty_canvas (rot=0)`
5. **向后平移（Y）工具拖动图层内部是否“内容在锚点后平移”** — ✅ 与 AE 26 一致
   - 已实测确认（见 PR #191）：拖锚点时 Anchor+=delta 且 Position+=delta（画面静止）；在图层内拖蒙版/内容时锚点与蒙版保持原地，图层内容相对平移。
   - *依据*：`PR #191 transform-tools.json: anchor_drag_move / anchor_pan_behind_masked`

### PR #181 — Type tool (point/paragraph text, editing) & viewer wheel zoom

关联文档：`docs/reference/ae/interaction/shapes-pen-type.md (PR #190)`

1. **新文字默认字体与字号** — ℹ️ AE 26 实测基准
   - 继承字符面板当前设置（实测本机 zh_CN 继承 MicrosoftYaHei-Bold / 200px，自动行距 leading = 1.2 × fontSize）。
   - *依据*：`PR #190 shapes-pen-type.json: type_empty_click textDocument`
2. **段落文本图层的锚点与位置** — ⚠️ 与 AE 26 有差异
   - 实测拖出 (400,300)→(900,550) 段落框（500×250）：Layer Anchor=[0,0,0]，但 Layer Position = [650, 425, 0]（拖出框的几何中心，不是左上角！），而 TextDocument.boxTextPos = [-250, -125]（相对 Position 的左上角偏移 [-w/2, -h/2]）。当前实现将 Position 设为框左上角与 AE 不同。
   - *依据*：`PR #190 shapes-pen-type.json: type_empty_drag_box (P=[650,425], box=[500,250], boxPos=[-250,-125])`
3. **编辑已有文字时，在空白处点击是否新建文字图层** — ℹ️ AE 26 实测基准
   - 区分两种状态：(1) 仅选中文字图层但未进入文本编辑态时，用文字工具点空白处会立刻在点击处新建第二个文字图层（PR #190 type_existing_text_away）；(2) 已进入文本编辑态时，点合成空白处不会新建图层（仍停留在当前文字层编辑态，需 Esc / 切工具 / Ctrl+Enter 提交）。
   - *依据*：`PR #190 type_existing_text_away & verify_E.json: p181_type_click_far_away (nL=1)`
4. **把文字删光后提交：删除图层还是保留空文本图层** — ✅ 与 AE 26 一致
   - 保留空文本图层！实测 Ctrl+A → Backspace → 切回 V 提交，图层保留且名称变为 `<空文本图层>`（text=''）。单击未输入直接提交同样保留 `<空文本图层>`（PR #190 type_empty_no_text）。与当前实现一致。
   - *依据*：`verify_A2.json: p181_type_delete_all_commit & PR #190 type_empty_no_text`
5. **查看器鼠标滚轮每格缩放倍率** — ⚠️ 与 AE 26 有差异
   - 实测滚轮每格并非 1.15 倍：向上滚放大乘以 9/8 = 1.125（1.0 → 1.125 → 1.265625），向下滚缩小乘以 7/8 = 0.875（1.0 → 0.875 → 0.765625）。
   - *依据*：`verify_A2.json: p181_wheel_zoom_up1/up2/down1/down2`

## E（对话框 / 导入）

### PR #167 — Composition Settings (Ctrl+N / Ctrl+K)

关联文档：`docs/reference/ae/preferences.json`

1. **新建合成预设列表的完整顺序与分隔线** — ℹ️ AE 26 实测基准
   - AE 26.0 预设列表共 29 项（含 4 条分隔线 '-'），完整顺序见 verifications.json 的 compPresets 字段（含 HD 1080p 24/25/29.97、UHD 4K/8K、社交媒体纵/横/方 30fps、HDV/DVCPRO HD、Cineon/胶片 2K/4K）。
   - *依据*：`docs/reference/ae/preferences.json [Composition Preset Names Section v11]`
2. **上次的合成设置是仅本次运行记住还是跨重启记住** — ⚠️ 与 AE 26 有差异
   - 跨重启记住！AE 26 将上次的合成设置（尺寸、帧率、锁定长宽比、开始时间码等）持久化写入偏好文件 [Composition Pref Section]，冷启动后 Ctrl+N 直接恢复上次会话的设置（本机实测冷启打开仍为上次 960×540 @ 60fps）。
   - *依据*：`verify_E.json: p167_new_comp_dialog & preferences.json [Composition Pref Section]`
3. **项目面板有焦点且选中另一个合成时，Ctrl+K 编辑哪个合成** — ✅ 与 AE 26 一致
   - 编辑项目面板中选中的合成！实测 TestComp (1920×1080, 10s, 30fps) 在时间轴打开，焦点切到项目面板并选中 00_OtherComp (1280×720, 5s, 24fps) 后按 Ctrl+K，弹出的是 00_OtherComp 的合成设置。
   - *依据*：`verify_E.json: p167_ctrl_k_project_focused (1280x720, 0:00:05:00, 基础 24)`
4. **改合成尺寸时锁定图层是否跟随移动** — ✅ 与 AE 26 一致
   - 跟随移动！实测将 1920×1080 合成改为 1000×1080（中心锚点），LockedLayer（locked=true）与 FreeLayer 的 Position 均从 [960,540] 变为 [500,540]。与当前实现一致。
   - *依据*：`verify_CE.json: p167_comp_resize_locked_via_dialog`
5. **“是 0:00:10:00 基础 30”这行中文措辞** — ✅ 与 AE 26 一致
   - AE 26 实测精确中文措辞为 `是 0:00:05:00  基础 24`（“基础”前有两个空格；像素分辨率提示为 `1280 x 720，3.5 MB（每 8bpc 帧）`）。
   - *依据*：`verify_E.json: p167_new_comp_dialog & p167_ctrl_k_project_focused`

### PR #175 — Solid Settings (Ctrl+Y / Ctrl+Shift+Y)

关联文档：`docs/reference/ae/interaction/layers-timeline-dnd.md (PR #192)`、`docs/reference/ae/preferences.json`

1. **纯色中文默认命名格式与持久化字段** — ✅ 与 AE 26 一致
   - 中文格式实测为 `<颜色名> 纯色 <N>`（如 `红色 纯色 1`，颜色名与“纯色”之间有空格），与当前格式“深灰色 纯色 1”一致。跨重启持久化的字段见 preferences.json [Last Solid Section Float (CS6)]：Color R/G/B、Width、Height、Pix Aspect、Parent Comp Width/Height/Pix Aspect。
   - *依据*：`PR #192 new_layer_solid & preferences.json [Last Solid Section Float (CS6)]`
2. **仅 1 个图层使用时，“影响使用此纯色的所有图层”是置灰还是隐藏** — ✅ 与 AE 26 一致
   - 保留显示但勾选并置灰（单层使用：`影响使用此纯色的所有图层` checked+disabled，下方提示 `此更改将影响 1 个图层。`，按钮为 `确定`）。当多个图层共用同一纯色时：复选框默认未勾选且可用，提示 `将为此图层创建新纯色。`，确认按钮文字动态变为 `新建`。
   - *依据*：`verify_CE.json: p175_solid_settings_single_clean & verify_E.json: p175_solid_settings_shared_use`
3. **改纯色尺寸时锚点的处理** — ⚠️ 与 AE 26 有差异
   - AE 并非仅在锚点居中时移到新中心，而是对所有锚点按新旧尺寸等比缩放：`ax' = ax * (newW / oldW)`、`ay' = ay * (newH / oldH)`（Position 不变）。实测 400×300 → 600×500：居中锚点 [200,150] → [300,250]；偏心锚点 [50,50] → [75, 83.3333]。
   - *依据*：`verify_E.json: p175_solid_resize_anchor`
4. **英寸和毫米换算是否为 72 ppi** — ✅ 与 AE 26 一致
   - 确认为 72 ppi（1 inch = 72 px，1 mm = 72/25.4 ≈ 2.834646 px；在摄像机设置实测 3555.5556 px / 1254.32 mm = 2.834648 亦严格吻合 72 ppi）。
   - *依据*：`PR #192 new_layer_camera (zoom=3555.5556px vs dialog 1254.32mm)`

### PR #179 — Camera Settings and Light Settings

关联文档：`docs/reference/ae/interaction/layers-timeline-dnd.md (PR #192)`

1. **摄像机设置里“单位”控制哪些字段** — ⚠️ 与 AE 26 有差异
   - 单位控制全部 5 个长度字段（缩放 zoom、胶片大小 filmSize、焦距 focalLength、对焦距离 focusDistance、光圈 aperture），而非仅管胶片大小和焦长！实测新建摄像机对话框默认单位为毫米，对话框显示 zoom=1254.32、filmSize=36.00、focalLength=66.67、focusDistance=1254.32、aperture=8.93、fStop=7.5，而建出图层内部按 72 ppi 存像素：Zoom=3555.5556 px（=1254.32×72/25.4）、Aperture=25.30934 px（=8.93×72/25.4）。
   - *依据*：`PR #192 layers-timeline-dnd.json: new_layer_camera`
2. **灯光默认类型、默认参数与中文命名格式** — ⚠️ 与 AE 26 有差异
   - 默认类型为聚光（SPOT），Intensity=100、Color=[1,1,1,1]、ConeAngle=90°、ConeFeather=50%、Falloff=无(1)、FalloffStart=500、FalloffDistance=500、CastsShadows=关、ShadowDarkness=100%、ShadowDiffusion=0、Position=[cx+80, cy-80, -666.66667]。注意中文默认名为 `聚光 1`（不带“灯”字！当前写“聚光灯 1”有细微差异）。
   - *依据*：`PR #192 layers-timeline-dnd.json: new_layer_light`
3. **灯光设置里“预览”及衰减/阴影控件状态** — ℹ️ AE 26 实测基准
   - 新建灯光对话框中“预览”默认置灰不可勾选（st=0x0）；半径(falloffStart)、衰减距离(falloffDistance)、阴影深度(shadowDarkness)、阴影扩散(shadowDiffusion)在默认 衰减=无 / 投影=关 时仍显示在布局中（默认数值 500 / 500 / 100 / 0）并置灰禁用。
   - *依据*：`verify_last.json: p179_light_dialog_uia`

### PR #183 — Pre-compose dialog and layer.precompose (Ctrl+Shift+C)

1. **单个图层与多个图层时的默认名称及中文格式** — ⚠️ 与 AE 26 有差异
   - 单个图层 `MySolid` 时默认名称为 `MySolid 合成 1`（中文为 `<图层名> 合成 1`，不是 `Comp 1`）；选中多个图层时默认名称为 `预合成 1`。
   - *依据*：`verify_CE.json: p183_precomp_dialog_single_real & p183_precomp_dialog_multi_real`
2. **单层可保留时默认选中项及复选框默认值** — ✅ 与 AE 26 一致
   - 单层可保留时：默认选中 `保留“<合成名>”中的所有属性`；`将合成持续时间调整为所选图层的时间范围` 为未勾选且置灰；`打开新合成` 为未勾选。多选时自动切为 `将所有属性移动到新合成`，`保留` 置灰且其说明文字引用最上方选中图层名（实测引用 `FirstLayer`）。
   - *依据*：`verify_CE.json: p183_precomp_dialog_single_real & p183_precomp_dialog_multi_real`
3. **“移动”后跨合成父级与轨道遮罩的处理** — ⚠️ 与 AE 26 有差异
   - 跨合成父级（父层留在外部）：AE 会断开父级（insideParent=null），但**烘焙并保留子层的世界/合成画面位置**，不让图层跳位（实测父在 [300,200]、子本地 [100,50] → 预合成内部子层位置为 [300,150]）。当前实现直接断开保留局部值会导致跳位，建议修正！轨道遮罩（仅移动填充层、遮罩层留在外部）：轨道遮罩直接断开清空（insideHasMatte=false、outerHasMatte=false）。
   - *依据*：`verify_E.json: p183_precomp_behavior.moveWithParentOutside & verify_last.json: p183_precomp_track_matte`
4. **“保留”时内部图层的开始时间、新合成尺寸/时长与所在文件夹** — ⚠️ 与 AE 26 有差异
   - 实测对 startTime=1, in=2, out=6 的 400×300 纯色执行“保留”：新合成尺寸 = 400×300（源尺寸）、时长 = 10s（原合成时长）、所在文件夹 = 原合成所在文件夹（SubFolder）；但**内部图层的 startTime=0, inPoint=0, outPoint=10**（从 0 开始！），原图层的 startTime=1 / in=2 / out=6 留在外层预合成图层上。当前实现将内部图层 startTime 设为原图层 startTime 会造成双倍偏移，请修正！
   - *依据*：`verify_E.json: p183_precomp_behavior.leaveAttrs`

### PR #187 — File > Import (Ctrl+I) and Project panel drag-and-drop

关联文档：`docs/reference/ae/ui/preferences-dialog.md (PR #176)`、`docs/reference/ae/interaction/layers-timeline-dnd.md (PR #192)`

1. **静止图片默认时长与序列帧率的首选项默认值** — ℹ️ AE 26 实测基准
   - 见 PR #176 preferences-dialog.md「导入」页：静止素材默认时长为“合成的长度”（可选指定时长），序列素材默认帧速率为 30 帧/秒（30 fps），与 #187 当前默认 30 fps 一致；接 bae-prefs 后可与首选项联动。
   - *依据*：`PR #176 docs/reference/ae/ui/preferences-dialog.md (import page)`
2. **项目素材拖到时间轴 / 合成查看器的位置与多选规则** — ℹ️ AE 26 实测基准
   - 已在 PR #192 实测完备：拖到合成视口落点即 Position（多选均落该点并按列表顺序堆叠且全选）；拖到时间轴始终居中合成中心 [cx,cy]，支持空区/层间/底部插入，多选按列表顺序插入且全选。供后续 P1 实现直接对照。
   - *依据*：`PR #192 layers-timeline-dnd.md §5`

## C（时间轴）

### PR #174 — Timeline context menus, Keyframe Interpolation/Velocity, Alt time-scaling

关联文档：`docs/reference/ae/interaction/layers-timeline-dnd.md (PR #192)`

1. **图层右键菜单完整项与顺序** — ℹ️ AE 26 实测基准
   - 实测常规图层右键顶层顺序（123 含子项）：蒙版 ▸ | 品质 ▸、开关 ▸、变换 ▸、时间 ▸、帧混合 ▸、3D 图层、参考线图层、环境图层、标记 ▸ | 跟踪和稳定 ▸（子菜单内含 4 项：跟踪摄像机、变形稳定器 VFX、跟踪运动、跟踪蒙版）| 打开 ▸、显示 ▸ | 预合成... | 反向选择、选择子项、重命名、添加到基本图形、裁切合成至选定图层边界、场景编辑检测...。注：常规纯色图层右键菜单中无 排列 ▸、跟踪遮罩 ▸、保持透明度、摄像机 ▸。
   - *依据*：`verify_CE.json: p174_layer_context_menu`
2. **关键帧右键菜单完整项、是否有“标签 ▸”以及“选择跟随关键帧”中文** — ✅ 与 AE 26 一致
   - 有 `标签(&L) ▸`（含 选择标签组、无、16 色、编辑标签颜色...）及 `选择关键帧标签组 ▸`！顺序：当前值、编辑值...、转到关键帧时间 | 标签 ▸、选择关键帧标签组 ▸ | 选择相同关键帧、选择前面的关键帧、`选择跟随关键帧`（精确中文确认！）| 切换定格关键帧、关键帧插值...、漂浮穿梭时间、关键帧速度... | 关键帧辅助 ▸。
   - *依据*：`verify_CE.json: p174_keyframe_context_menu`
3. **关键帧插值对话框：下拉框下是否有说明文字，“临时插值”中文** — ✅ 与 AE 26 一致
   - AE 26 精确中文标签确为 `临时插值:`、`空间插值:`、`浮动:`；每个下拉框下方确有 1–2 行简短说明文字（“浮动:”下方有 2 行）。
   - *依据*：`verify_CE.json: p174_kf_interpolation_dialog`
4. **关键帧速度对话框：多维属性行标签与“连续”复选框文案** — ⚠️ 与 AE 26 有差异
   - 区分空间属性与非空间多维属性：(1) Position 等空间属性合并为单行 `速度:` / `影响:`（不拆维度）；(2) Scale 等非空间多维属性上方有 `锁定维度` 复选框（默认勾选），行标签为 `维度: X`、`维度: Y`（而非 `维度 1/2`）。底部复选框精确中文为 `连续（锁定传出到传入）`（EN: Continuous (Lock Outgoing to Incoming)）。
   - *依据*：`verify_CE.json: p174_kf_velocity_dialog & verify_last.json: p174_kf_velocity_scale_2d`
5. **Ctrl/Cmd 单击已选中的关键帧：只改单击的还是改全部选中的** — ⚠️ 与 AE 26 有差异
   - 改**全部选中的关键帧**！实测全选 3 个线性关键帧 [30f,90f,150f]，Ctrl 单击中间帧 90f，全部 3 个关键帧同时切换为 BEZIER/BEZIER。当前实现“只改单击的”与 AE 不同，建议调整。
   - *依据*：`verify_CE.json: p174_ctrl_click_with_3_selected`
6. **Alt 缩放时间穿过未选中关键帧时是否覆盖** — ⚠️ 与 AE 26 有差异
   - 穿过时**不覆盖/不删除**未选中关键帧！实测 4 帧 [1s,2s,4s,5s] 选中前 3 帧，Alt+拖 4s→6s 跨越未选中的 5s：结果为 [30f(sel), 80f(sel), 150f(未选中原值完好), 180f(sel)]。此外须注意 Alt 必须在 mouse-down 前按住才进入等比缩放模式，拖拽后再按 Alt 仍为刚性平移（见 PR #192）。
   - *依据*：`verify_CE.json: p174_alt_scale_across_unselected & PR #192 kf_alt_drag_before_down/after_down`

### PR #185 — Comp/Layer markers & Layer bar slip edit

关联文档：`docs/reference/ae/interaction/layers-timeline-dnd.md (PR #192)`

1. **滑动编辑：Alt 拖图层条能否滑动；滑动时图层标记是否跟着源走** — ⚠️ 与 AE 26 有差异
   - (1) **Alt 拖图层条主体不能滑动编辑**，它仍整体移动图层条（实测 in=3..7,st=0 → in=4..8,st=1）；需用向后平移工具（Y）拖图层条（或拖源素材淡色条）才执行滑动（实测 in=3..7 不动、st=0→1）。(2) 滑动时**图层标记跟着源走**（保持图层时间，合成时间同步平移：4.0s → 5.0s），与当前实现一致！
   - *依据*：`verify_CE.json: p185_slip_alt_drag_bar & p185_slip_pan_behind_Y_with_marker`
2. **标记右键菜单完整项（AE 26 是否有“标签 ▸”）** — ✅ 与 AE 26 一致
   - 合成标记右键菜单共 10 项，**无 `标签 ▸` 子菜单**（标签在 `设置...` 对话框中修改）。完整顺序：`设置...` | `从源更新标记`、`转到标记时间`、`删除此标记`、`删除所有标记` | `锁定标记` | `启用受保护区域`。
   - *依据*：`verify_last.json: p185_comp_marker_context_menu`
3. **Alt+*（通过对话框添加标记）点取消时是否留下标记** — ✅ 与 AE 26 一致
   - 不留下标记！实测 Alt + Numpad * 弹出 `合成标记` 对话框后按 Esc 取消，compMarkers 为空（0 个）。确认为点“确定”才添加，与当前实现一致。
   - *依据*：`verify_CE.json: p185_comp_marker_dialog_via_alt_star & p185_after_alt_star_cancel`
4. **AE 26 标记对话框标题与字段名** — ℹ️ AE 26 实测基准
   - AE 26 精简了旧版 Flash 提示点/章节/Web 链接面板：(1) `合成标记` 对话框含 `时间:`、`持续时间:`、`注释:`、`标签:`、`受保护区域`、`确定`、`取消`；(2) `图层标记` 对话框含 `时间:`、`持续时间:`、`注释:`、`标签:`、单选 `注释标记`（默认）/ `分段标记`、`确定`、`取消`。另外提醒 PR #192 实测的右端 1 帧包含规则：`N`、`]`、`Alt+]` 均对齐到 `playhead + 1/fps`。
   - *依据*：`verify_CE.json: p185_comp_marker_dialog_via_alt_star & verify_last.json: p185_layer_marker_dialog`

## B（菜单 / 快捷键）

### PR #172 — AE full menu bar & Chinese labels

关联文档：`docs/reference/ae/menus.json`

1. **动画 ▸ Track in Boris FX Mocha 位置与名称** — ✅ 与 AE 26 一致
   - AE 26.0 `menus.json` 中位于 `动画(A)` 第 1 项，cmd=851969，zh_CN 下名称仍为英文 `Track in Boris FX Mocha`。
   - *依据*：`docs/reference/ae/menus.json (cmd 851969)`
2. **文件 ▸ 导入 下“从库…”与“消失点 (.vpe)…”的准确中文名** — ⚠️ 与 AE 26 有差异
   - AE 26.0 zh_CN 精确名称分别为：`从 Libraries 中...`（cmd=9764867，非“从库…”）和 `Vanishing Point (.vpe)...`（cmd=9764869，zh_CN 下保持英文，非“消失点 (.vpe)…”）。
   - *依据*：`docs/reference/ae/menus.json (cmd 9764867, 9764869)`
3. **合成 ▸ 响应式设计 - 时间 下三项精确中文名** — ℹ️ AE 26 实测基准
   - 精确顺序与名称：`创建开场`（2097153）、`创建结尾`（2097154）、`通过工作区域创建受保护区域`（2097155）。
   - *依据*：`docs/reference/ae/menus.json (cmd 2097153-2097155)`
4. **动画 ▸ 关键帧辅助 里四个空对象相关项的精确中文名** — ℹ️ AE 26 实测基准
   - 精确名称分别为：`位置点的空控制器`（11075587）、`沿路径进行空值追踪`（11075593）、`空值跟随路径点`（11075594）、`路径点的空控制器`（11075598）。
   - *依据*：`docs/reference/ae/menus.json (cmd 11075587, 11075593, 11075594, 11075598)`
5. **图层 ▸ 轨道遮罩 ▸ 以上方/下方图层作为遮罩 的精确中文名** — ⚠️ 与 AE 26 有差异
   - AE 26.0 zh_CN 菜单路径为 `图层(L) ▸ 跟踪遮罩(A) ▸ 使用上方图层遮罩` / `使用下方图层遮罩`（子项各 4 个：`Alpha 遮罩`、`Alpha 反转遮罩`、`亮度遮罩`、`亮度反转遮罩`；顶部第一项为 `没有轨道遮罩`）。
   - *依据*：`docs/reference/ae/menus.json (cmd 16384001-16384006, 23658497-23724036)`
6. **视图 ▸ 拆分并新建锁定的查看器 的精确中文名** — ⚠️ 与 AE 26 有差异
   - AE 26.0 zh_CN 精确名称为 `视图(V) ▸ 与新锁定的查看器分开`（cmd=589825, Ctrl+Alt+Shift+N），非“拆分并新建锁定的查看器”。
   - *依据*：`docs/reference/ae/menus.json (cmd 589825)`

### PR #182 — AE default shortcuts keymap

关联文档：`docs/reference/ae/shortcuts.json`、`docs/reference/ae/menus.json`、`docs/reference/ae/interaction/layers-timeline-dnd.md (PR #192)`

1. **无未决待核项；与实机快捷键行为对照确认** — ✅ 与 AE 26 一致
   - PR #182 描述中无待核条目。已交叉核对所有在三批实测中使用的 Windows 默认快捷键（V/H/Z/W/Y/Q/G/Ctrl+T、Ctrl+Y/Ctrl+Alt+Shift+T/Y/C/L、Ctrl+D/Ctrl+Shift+D、B/N/Ctrl+Shift+X/[/]/Alt+[/Alt+]、F9/Shift+F9/Ctrl+Shift+F9/Ctrl+Alt+H、Alt+Left/Right/Shift、Ctrl+Alt+K/Ctrl+Shift+K、Ctrl+Shift+C、Ctrl+N/K、Alt+Numpad*），与 AE 26.0 `shortcuts.json` 及 `menus.json` 完全一致。
   - *依据*：`docs/reference/ae/shortcuts.json & PR #190/#191/#192 live tests`

### PR #186 — Layer > Switches, 3D Layer, Quality, Edit > Label

关联文档：`docs/reference/ae/menus.json`

1. **无未决待核项；开关/品质/标签菜单结构与实机核对** — ✅ 与 AE 26 一致
   - PR #186 描述中无待核条目。实测图层右键菜单与顶栏 `图层 ▸ 开关` / `品质` / `3D 图层` 与 PR #186 所接条目完全一致；同时确认 `开关` 子菜单在图层右键中的准确中文顺序为：隐藏其他视频、显示所有视频、解锁所有图层(Ctrl+Shift+L) | 启用表达式、禁用表达式 | 消隐、锁定(Ctrl+L)、音频、视频(Ctrl+Alt+Shift+V)、单独（注：顶栏/右键均称“单独”而非“独奏”）、效果、折叠、运动模糊、调整图层。
   - *依据*：`verify_CE.json: p174_layer_context_menu & docs/reference/ae/menus.json`

## 附录：AE 26 实测菜单与预设列表

### A. 合成预设顺序（`Composition Preset Names Section v11`）

1. `HD  •  1920x1080 • 24 fps`
2. `HD  •  1920x1080 • 25 fps`
3. `HD  •  1920x1080 • 29.97 fps`
4. `-`
5. `UHD 4K)  •  3840x2160 • 23.976 fps`
6. `UHD 4K)  •  3840x2160 • 25 fps`
7. `UHD 4K)  •  3840x2160 • 29.97 fps`
8. `UHD (8K)  •  7680x4320 • 23.976 fps`
9. `-`
10. `社交媒体纵向 HD  •  1080x1920 • 30 fps`
11. `社交媒体横向 HD  •  1920x1080 • 30 fps`
12. `社交媒体纵向  •  720x1280 • 30 fps`
13. `社交媒体横向  •  1280x720 • 30 fps`
14. `社交媒体方形  •  1080x1080 • 30 fps`
15. `-`
16. `HDV/HDTV  •  1280x720 • 25 fps`
17. `HDV/HDTV  •  1280x720 • 29.97 fps`
18. `HDV  •  1440x1080 (1.33) • 25 fps`
19. `HDV  •  1440x1080 (1.33) • 29.97 fps`
20. `DVCPRO HD  •  960x720 (1.33) • 23.976 fps`
21. `DVCPRO HD  •  960x720 (1.33) • 25 fps`
22. `DVCPRO HD  •  960x720 (1.33) • 29.97 fps`
23. `DVCPRO HD  •  1440x1080 (1.33) • 25 fps`
24. `DVCPRO HD  •  1280x1080 (1.5) • 29.97 fps`
25. `-`
26. `Cineon 1/2  •  1828x1332 • 24 fps`
27. `Cineon 完整  •  3656x2664 • 24 fps`
28. `胶片 (2K)  •  2048x1556 • 24 fps`
29. `胶片 (4K)  •  4096x3112 • 24 fps`

### B. 关键帧右键菜单（`zh_CN` 实测完整项）

```text
960.0、540.0
编辑值...
转到关键帧时间
---
标签(&L) >
  选择标签组
  ---
  无 [checked]
  ---
  红色
  黄色
  浅绿色
  粉色
  淡紫色
  桃红色
  海泡沫
  蓝色
  绿色
  紫色
  橙色
  棕色
  紫红色
  青色
  砂岩
  深绿色
  ---
  编辑标签颜色...
选择关键帧标签组 >
  在选定的图层上
  在所有图层上
  ---
  选定图层上的可见关键帧
  所有图层上的可见关键帧
---
选择相同关键帧
选择前面的关键帧
选择跟随关键帧
---
切换定格关键帧
关键帧插值...
漂浮穿梭时间
关键帧速度...
---
```

> 注：末尾分隔线下方为动态挂载的 `关键帧辅助 ▸` 子菜单（项同顶栏 `动画(A) ▸ 关键帧辅助(K)`）。

### C. 合成标记右键菜单（`zh_CN` 实测完整项）

```text
设置...
---
从源更新标记 [disabled]
转到标记时间
删除此标记
删除所有标记 [disabled]
---
锁定标记
---
启用受保护区域
```

### D. 图层右键菜单（常规纯色图层 `zh_CN` 实测完整树）

```text
蒙版 >
  新建蒙版	Ctrl+Shift+N
  ---
  蒙版形状...	Ctrl+Shift+M [disabled]
  蒙版羽化...	Ctrl+Shift+F [disabled]
  蒙版不透明度... [disabled]
  蒙版扩展... [disabled]
  ---
  重置蒙版 [disabled]
  移除蒙版 [disabled]
  移除所有蒙版 [disabled]
  ---
  反转	Ctrl+Shift+I [disabled]
  已锁定 [disabled]
  运动模糊 [disabled] >
    与图层相同 [disabled]
    开 [disabled]
    关 [disabled]
  羽化衰减 [disabled] >
    平滑 [disabled]
    线性 [disabled]
  ---
  解锁所有蒙版 [disabled]
  锁定其他蒙版 [disabled]
  隐藏锁定的蒙版 [disabled]
---
品质 >
  最佳	Ctrl+U [checked]
  草图	Ctrl+Shift+U
  线框	Ctrl+Alt+Shift+U
  ---
  双线性	Alt+B [checked]
  双立方	Alt+Shift+B
开关 >
  隐藏其他视频 [disabled]
  显示所有视频
  解锁所有图层	Ctrl+Shift+L
  ---
  启用表达式
  禁用表达式
  ---
  消隐
  锁定	Ctrl+L
  音频 [disabled]
  视频	Ctrl+Alt+Shift+V [checked]
  单独
  效果 [disabled]
  折叠
  运动模糊
  调整图层
变换 >
  重置(&R)
  ---
  锚点...
  位置...	Ctrl+Shift+P
  缩放...
  方向...	Ctrl+Alt+Shift+R [disabled]
  旋转...	Ctrl+Shift+R
  不透明度...	Ctrl+Shift+O
  ---
  水平翻转
  垂直翻转
  视点居中	Ctrl+Home
  在图层内容中居中放置锚点	Ctrl+Alt+Home
  ---
  适合复合	Ctrl+Alt+F
  适合复合宽度	Ctrl+Alt+Shift+H
  适合复合高度	Ctrl+Alt+Shift+G
  ---
  自动定向...	Ctrl+Alt+O
时间 >
  启用时间重映射	Ctrl+Alt+T [disabled]
  时间反向图层	Ctrl+Alt+R
  时间伸缩(&C)...
  冻结帧 [disabled]
  在最后一帧上冻结 [disabled]
  将视频对齐到数据 [disabled]
帧混合 >
  关 [checked]
  帧混合
  像素运动
3D 图层
参考线图层
环境图层 [disabled]
标记 >
  添加标记(&R)	Numpad *
  从源更新标记 [disabled]
  锁定标记
  删除所有标记 [disabled]
---
跟踪和稳定 >
  跟踪摄像机 [disabled]
  变形稳定器 VFX [disabled]
  跟踪运动 [disabled]
  跟踪蒙版 [disabled]
---
打开 >
  打开效果控件
  打开图层
  打开图层源
  编辑原稿... [disabled]
显示 >
  在资源管理器中显示 [disabled]
  在项目中显示图层源
  在项目流程图中显示图层
  在项目中显示合成
  显示表达式错误
---
预合成...
---
反向选择
选择子项
重命名
添加到基本图形 [disabled]
裁切合成至选定图层边界
场景编辑检测... [disabled]
```

