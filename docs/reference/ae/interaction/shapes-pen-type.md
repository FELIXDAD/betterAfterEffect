# After Effects 26 交互实测：形状工具、钢笔工具、文字工具

> 来源：Adobe After Effects 26.0x67 (`zh_CN`) | 平台：Windows 11 Pro x64 | 测试合成：`1920×1080`、30 fps、10 s、`zoom = 1.0` | 实测日期：2026-11-22
>
> 方法：通过 Windows 原生鼠标/键盘输入与 MSAA 工具栏读取在 AE 合成查看器中执行真实交互，随后立即通过 ExtendScript DOM 读回精确图层结构与数值（`0.0001 px` 精度），共实测 **61** 组用例。完整结构化数据见 [`shapes-pen-type.json`](shapes-pen-type.json)。

---

## 一、三种选中状态总决策表（开发实现核心）

| 工具 | 状态 1：未选中任何图层 | 状态 2：选中非形状图层（纯色/视频/图片/文字等） | 状态 3：选中形状图层（`ShapeLayer`） |
|---|---|---|---|
| **形状工具（`Q`）**<br>矩形 / 圆角矩形 / 椭圆 / 多边形 / 星形 | 在时间轴顶部（`index = 1`）新建 `ShapeLayer` 并单独选中；工具栏「创建形状/创建蒙版」切换按钮**置灰禁用** | 不新建图层；在当前选中图层上新建 `Mask`（`ADBE Mask Atom`，默认 `maskMode = ADD`）；工具栏隐藏填充/描边控件 | 工具栏「工具创建形状 / 工具创建蒙版」按钮**启用**：<br>• **工具创建形状（默认）**：在同一形状图层的 `Contents` **顶部（`index = 1`）** 插入新矢量组（如 `矩形 2` 排在 `矩形 1` 之上）<br>• **工具创建蒙版**：在该形状图层上新建 `Mask` |
| **钢笔工具（`G`）** | **第一次单击立即**在 `index = 1` 新建 `ShapeLayer` 及单顶点开放路径；后续点击/拖拽追加顶点 | 在当前选中图层上新建 `Mask`：<br>• **未闭合路径**：`maskMode = NONE`<br>• **闭合瞬间**：自动切换为 `maskMode = ADD` | • **工具创建形状（默认）**：在 `Contents` 顶部（`index = 1`）插入新矢量组 `形状 1`<br>• **工具创建蒙版**：在该形状图层上新建 `Mask` |
| **文字工具（`Ctrl+T`）** | 新建 `TextLayer` 插入到 `index = 1` 并单独选中 | **永不创建蒙版**；新建 `TextLayer` 并插入到**最上方选中图层的正上方**（如 2 个纯色选中 L2 → 插入到 `index = 2`，原 L2 顺移为 L3），仅选中新文字层 | **永不创建蒙版**；同左，新建 `TextLayer` 插入到所选形状图层正上方，仅选中新文字层 |

### 易踩坑点速览

1. **从中心绘制是 `Ctrl`（macOS `Cmd`），不是 `Alt`**：在矩形/椭圆工具拖拽时，`Ctrl` 从起点为中心向四周扩张（`Size = [2|dx|, 2|dy|]`）；`Alt` 在实测中不触发中心绘制，行为等同普通拖拽。
2. **形状工具平移量写在组变换 `Group.Position`，而不是路径 `Path.Position`**：默认首选项（`Shape Layer Center Anchor Point = false`）下，`Layer.Anchor = [0, 0]`、`Layer.Position = [comp.w/2, comp.h/2]`、`Path.Position = [0, 0]`，实际偏移存放在 `ADBE Vector Transform Group` → `ADBE Vector Position`。
3. **钢笔工具的 `Group.Position` 始终为 `[0, 0]`**：所有顶点直接写入图层局部坐标 `compPoint - Layer.Position`。
4. **开启「在新形状图层上居中放置锚点」后，形状工具与钢笔工具机制不同**：
   - **形状工具**：直接把 `Layer.Position` 设为形状中心、`Layer.Anchor = [0, 0]`、`Group.Position = [0, 0]`。
   - **钢笔工具**：顶点坐标仍相对合成中心不改写，而是把 `Layer.Anchor` 设为 `sourceRect` 中心、`Layer.Position` 设为对应的合成中心坐标（例如三角形 `A = [-310, -215]`、`P = [650, 325]`，视觉位置不变）。
5. **组内渲染顺序（自上而下）**：`ADBE Vectors Group` 内实测固定顺序为 `[1] 路径 → [2] 描边 → [3] 填充`，描边位于填充上方。
6. **段落文本框（文字工具拖拽）的 `Position` 是框中心而非左上角**：`Layer.Anchor = [0, 0]`、`Layer.Position = [(x1+x2)/2, (y1+y2)/2]`、`boxTextSize = [|x2-x1|, |y2-y1|]`、`boxTextPos = [-w/2, -h/2]`。
7. **钢笔开放蒙版默认 `MaskMode.NONE`**：只有闭合后才自动变成 `MaskMode.ADD`，且 AE 会把闭合顶点绕序规范化为逆时针（`Shape Direction = 1`）。

---

## 二、形状工具（`Q`）精确公式与行为

工具栏槽位可通过 `Alt+单击` 按固定顺序循环：`矩形工具 → 圆角矩形工具 → 椭圆工具 → 多边形工具 → 星形工具 → 矩形工具`。

### 2.1 新建形状图层默认属性（`Shape Layer Center Anchor Point = false`）

| 属性 | `matchName` | 实测默认值（1920×1080 合成） |
|---|---|---|
| 图层类型 / 名称 | `ADBE Vector Layer` | `ShapeLayer` / `形状图层 1`（EN: `Shape Layer 1`），标签色索引 `label = 8` |
| 图层锚点 / 位置 | `ADBE Anchor Point` / `ADBE Position` | `[0, 0, 0]` / `[960, 540, 0]`（合成中心） |
| 比例 / 旋转 / 不透明度 | `ADBE Scale` / `ADBE Rotate Z` / `ADBE Opacity` | `[100, 100, 100]` / `0` / `100` |
| 矢量组变换默认值 | `ADBE Vector Transform Group` | `Anchor=[0,0]`、`Scale=[100,100]`、`Skew=0`、`SkewAxis=0`、`Rotation=0`、`Opacity=100` |
| 组内元素顺序 | `ADBE Vectors Group` | `[1] 路径`、`[2] 描边`（继承上次工具栏设置）、`[3] 填充`（继承上次工具栏设置） |
| 形状方向 | `ADBE Vector Shape Direction` | `1`（即使从右下向左上反向拖拽仍为 `1`，`Size` 恒正） |

### 2.2 矩形 / 圆角矩形 / 椭圆拖拽公式（起点 `(x1, y1)` → 终点 `(x2, y2)`）

- **普通拖拽**（任意方向，包括 BR→TL 反向）：
  - `Path.Size = [|x2 - x1|, |y2 - y1|]`，`Path.Position = [0, 0]`
  - `Group.Position = [(x1 + x2)/2 - Layer.Pos.x + Layer.Anchor.x, (y1 + y2)/2 - Layer.Pos.y + Layer.Anchor.y]`
- **`Shift` 拖拽**（1:1 正方形/正圆）：
  - 边长 `s = max(|dx|, |dy|)`，以起点 `(x1, y1)` 为角点保持象限符号：`end = (x1 + sign(dx)*s, y1 + sign(dy)*s)`
  - 实测 `(400,200)→(800,500)`：`Size = [400, 400]`，`Group.Position = [-360, -140]`（对应合成中心 `(600, 400)`）。
- **`Ctrl`（Mac `Cmd`）拖拽**（从中心绘制）：
  - 以起点 `(x1, y1)` 为中心：`Size = [2|dx|, 2|dy|]`，`Group.Position = (x1, y1) - Layer.Pos + Layer.Anchor`
  - 实测 `(600,350)→(800,500)`：`Size = [400, 300]`，`Group.Position = [-360, -190]`。
- **`Alt` 拖拽**：不从中心绘制；实测 `(600,350)→(800,500)` 得到 `Size = [200, 150]`、`Group.Position = [-260, -115]`（与普通拖拽相同）。
- **`Shift + Ctrl` 拖拽**：从中心绘制正方形/正圆，`s = 2 * max(|dx|, |dy|)`；实测 `(600,350)→(800,500)` 得 `Size = [400, 400]`、`Group.Position = [-360, -190]`。
- **拖拽中按住 `Space` 移动 `(sdx, sdy)`**：保持 `Size` 不变，仅平移 `Group.Position += (sdx, sdy)`；实测 `(+100, +50)` 使 `gP` 从 `[-360, -190]` 变为 `[-260, -140]`。
- **单击不拖动**：**无操作（创建 0 个图层）**。
- **双击形状工具图标**：
  - 未选层或选中形状层（创建形状模式）：`Size = [comp.w, comp.h]`（`[1920, 1080]`），`Group.Position = [0, 0]`。
  - **`Shift` + 双击**：`Size = [min(w, h), min(w, h)]`（`[1080, 1080]`），`Group.Position = [0, 0]`。
  - 选中非形状图层（如 Solid）：创建铺满该图层本地尺寸的蒙版（矩形 `[0..w, 0..h]`；椭圆内切于 `[0..w, 0..h]`）。
- **圆角矩形默认值与拖拽中方向键**：
  - 矩形工具默认 `ADBE Vector Rect Roundness = 0`；圆角矩形工具默认 `Roundness = 20`。
  - 拖拽中按 `Up` / `Down`：每次增/减 `1 px`（实测 `Up × 3` 得 `23`）。
  - 拖拽中按 `Right`：直接设为最大圆角 `min(w, h) / 2`（400×300 → `150`）。
  - 拖拽中按 `Left`：直接归零为 `0`。
- **勾选「贝塞尔曲线路径（Bezier Path）」**（创建 `ADBE Vector Shape - Group` 而非参数路径，切线常量 `κ = 4*(√2-1)/3 ≈ 0.55228475`）：
  - **直角矩形（400×300）**：4 顶点逆时针（自右上角起）`[[200,-150], [-200,-150], [-200,150], [200,150]]`，切线全 `[0,0]`。
  - **圆角矩形（400×300, r=20）**：8 顶点逆时针，圆角切线长度 `r × κ = 11.0457`。
  - **椭圆（400×300, rx=200, ry=150）**：4 顶点逆时针（自顶部起）`[[0,-150], [-200,0], [0,150], [200,0]]`，切线 `rx×κ = 110.4569`、`ry×κ = 82.8427`。

### 2.3 多边形工具 / 星形工具公式（`ADBE Vector Shape - Star`）

- 多边形 `ADBE Vector Star Type = 2`，星形 `ADBE Vector Star Type = 1`；矢量组名称均为 `多边星形 1`（EN: `Polystar 1`）。
- **从起点 `(cx, cy)` 拖拽至 `(x2, y2)`**：
  - `Group.Position = (cx, cy) - Layer.Pos + Layer.Anchor`，`Star.Position = [0, 0]`。
  - `Outer Radius = hypot(dx, dy)`；`Inner Radius = 0.5 * Outer Radius`（默认比例 1:2）。
  - `Rotation = atan2(dx, -dy)`（角度制，以 12 点钟 `-Y` 方向为 `0°` 顺时针计量）：
    - 实测水平向右 `(200, 0)` → `Outer Radius = 200`、`Inner Radius = 100`、`Rotation = 90°`。
    - 实测右上 `(100, -150)` → `Outer Radius = 180.2776`、`Inner Radius = 90.1388`、`Rotation = 33.6901°`。
- **拖拽中方向键**：
  - `Up` / `Down`：边数/角数 `Points ± 1`（默认 `5`，且**跨次拖拽持久化**，实测 `Up × 2` 变为 `7` 后下一次拖拽沿用 `7`）。
  - `Right` / `Left`（星形）：`Outer Roundness ± 10%`（实测 `Right × 3` 得 `30`）。
- **双击多边形/星形工具图标**：`Group.Position = [0, 0]`、`Outer Radius = min(w, h)/2 = 540`、`Rotation = 0°`、`Points` **重置为 `5`**；星形 `Inner Radius = 270`（多边形 `Inner Radius = 50` 未使用）。

### 2.4 状态 2（非形状图层 → 蒙版）与状态 3（形状图层 → 组或蒙版）

- **蒙版坐标变换（精确验证）**：顶点存储在**目标图层的本地像素空间** $p_{\text{layer}} = M_{\text{layer}}^{-1} \cdot p_{\text{comp}}$。
  - 实测：`600×400` 纯色图层置于 `Anchor=[300,200]`、`Position=[1000,600]`、`Scale=[150%,100%]`，在合成空间拖拽 `(800,450)→(1100,650)`，落入图层本地 `x ∈ [166.6667, 366.6667]`、`y ∈ [50, 250]`，与逆矩阵公式误差为 `0.0000 px`。
- **形状图层上追加组（状态 3 默认）**：
  - 新组插入到 `Contents` **索引 1（最顶部）**，位于已有 `矩形 1` 之上。
  - 当形状图层已移动至 `Position = [1100, 600]`，拖拽 `(1000,400)→(1300,700)`（合成中心 `(1150, 550)`）时，`矩形 2` 的 `Group.Position = [1150-1100, 550-600] = [50, -50]`。
- **形状图层切换「工具创建蒙版」**：蒙版坐标位于形状图层局部空间 `(compX - Layer.Pos.x + Layer.Anchor.x, compY - Layer.Pos.y + Layer.Anchor.y)`（即以图层 `[960,540]` 为原点）。

---

## 三、钢笔工具（`G`）精确数值与行为

- **单次点击（空合成）**：立即新建 `形状图层 1` → `形状 1` → `路径 1`，含 1 个顶点 `closed = false`；**双击钢笔工具图标无操作（0 图层）**。
- **组变换**：`Group.Position` **恒为 `[0, 0]`**；顶点坐标直接等于 `compPoint - Layer.Pos`（默认 `Layer.Pos = [960, 540]`）。
- **贝塞尔拖拽手柄**：在顶点处按下并拖拽位移 `(dx, dy)`：
  - `outTangent = (+dx, +dy)`，`inTangent = (-dx, -dy)`（均为相对顶点的偏移量）。
  - 实测拖拽 `(+80, -60)` → `inTangent = [-80, 60]`、`outTangent = [80, -60]`。
- **拖拽中按住 `Alt` 断开切线**：冻结 `inTangent` 为按下 `Alt` 前的数值（实测 `[-80, 0]`），仅更新 `outTangent`（实测 `[+80, +48]`）。
- **闭合与绕序**：点击起始顶点闭合路径时，`closed` 变为 `true`，AE 自动按逆时针方向规范化顶点序列。
- **RotoBezier**：
  - 工具栏 `RotoBezier` 复选框位置随上下文栏动态变化（无选层或形状层时位于填充/描边右侧；选中非形状图层时填充/描边隐藏，复选框左移）。
  - 勾选后，在形状图层上自动生成平滑非零贝塞尔切线；在纯色图层上创建蒙版时设置 `mask.rotoBezier = true` 并生成相同自动切线。
- **纯色图层上的开放/闭合蒙版**：
  - 未闭合（3 点开放路径）：`maskMode = NONE`、`closed = false`。
  - 闭合后：`maskMode` 自动切换为 `ADD`、`closed = true`。

---

## 四、文字工具（`Ctrl+T`）精确数值与行为

- **永不创建蒙版**：无论当前未选层、选中纯色、选中形状层还是选中已有文字层，只要点击空白处或拖动文本框，一律新建 `TextLayer`（`ADBE Text Layer`，默认标签色 `label = 1`）。
- **图层插入位置与选中状态**：
  - 无选中图层：插入到 `index = 1`（合成顶部）。
  - 有选中图层：插入到**选中图层正上方**（实测 2 个纯色选中 L2 → 新文字层插入在 `index = 2`，原 L2 变为 `index = 3`）。
  - 创建后仅新文字层处于 `selected = true`，原选中图层取消选中。
- **点文本（单击 `(x, y)`）**：
  - `Layer.Anchor = [0, 0, 0]`，`Layer.Position = [x, y, 0]`（文本基线起点），`boxText = false`，图层名自动同步文本内容。
  - 点击后未输入任何字符直接提交（`Ctrl+Enter` / 切工具）：保留空文字图层，图层名为 `<空文本图层>`（EN: `<Empty Text Layer>`），`text = ""`，`sourceRect = {0, 0, 0, 0}`。
- **段落文本（拖拽框 `(x1, y1) → (x2, y2)`）**：
  - `Layer.Anchor = [0, 0, 0]`，`Layer.Position = [(x1+x2)/2, (y1+y2)/2, 0]`（文本框中心）。
  - `boxText = true`，`boxTextSize = [|x2 - x1|, |y2 - y1|]`，`boxTextPos = [-w/2, -h/2]`。
  - 实测 `(400,300)→(900,550)`：`Position = [650, 425, 0]`、`boxTextSize = [500, 250]`、`boxTextPos = [-250, -125]`。
- **双击文字工具图标**：在合成中心新建空点文本图层（`Position = [960, 540, 0]`、`Anchor = [0, 0, 0]`、`boxText = false`）。

---

## 五、全部实测用例明细（61 组）

| 编号 | 工具 | 选中状态 | 输入动作 | 实测结果摘要 |
|---|---|---|---|---|
| `sharp_rect_empty_tl_br` | shape | `no_layer_selected` | Rectangle drag (400,200)->(800,500) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] size=[400, 300],r=0] |
| `sharp_rect_empty_bezier` | shape | `no_layer_selected` | Rectangle with Bezier Path checked drag (400,200)->(800,500) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] bezier(closed=True,nv=4)] |
| `rounded_rect_empty_tl_br` | shape | `no_layer_selected` | Rounded Rectangle drag (400,200)->(800,500) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] size=[400, 300],r=20] |
| `rounded_rect_empty_br_tl` | shape | `no_layer_selected` | Rounded Rectangle reverse drag (800,500)->(400,200) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] size=[400, 300],r=20] |
| `rounded_rect_empty_shift` | shape | `no_layer_selected` | Rounded Rectangle Shift-drag (400,200)->(800,500) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -140] size=[400, 400],r=20] |
| `rounded_rect_empty_ctrl` | shape | `no_layer_selected` | Rounded Rectangle Ctrl-drag (600,350)->(800,500) from center | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] size=[400, 300],r=20] |
| `rounded_rect_empty_alt` | shape | `no_layer_selected` | Rounded Rectangle Alt-drag (600,350)->(800,500) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-260, -115] size=[200, 150],r=20] |
| `rounded_rect_empty_shift_ctrl` | shape | `no_layer_selected` | Rounded Rectangle Shift+Ctrl-drag (600,350)->(800,500) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] size=[400, 400],r=20] |
| `rounded_rect_empty_space_pan` | shape | `no_layer_selected` | Rounded Rectangle drag (400,200)->(800,500) + Space pan (+100,+50) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-260, -140] size=[400, 300],r=20] |
| `rounded_rect_empty_single_click` | shape | `no_layer_selected` | Rounded Rectangle single click (600,350) no drag | `0 图层`（无操作） |
| `rounded_rect_empty_double_click` | shape | `no_layer_selected` | Rounded Rectangle double-click tool icon | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[0, 0] size=[1920, 1080],r=20] |
| `rounded_rect_empty_shift_double_click` | shape | `no_layer_selected` | Rounded Rectangle Shift+double-click tool icon | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[0, 0] size=[1080, 1080],r=20] |
| `rounded_rect_empty_bezier` | shape | `no_layer_selected` | Rounded Rectangle with Bezier Path checked drag (400,200)->(800,500) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] bezier(closed=True,nv=8)] |
| `rounded_rect_empty_up3` | shape | `no_layer_selected` | Rounded Rectangle drag + Up arrow x3 mid-drag | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] size=[400, 300],r=23] |
| `rounded_rect_empty_right` | shape | `no_layer_selected` | Rounded Rectangle drag + Right arrow mid-drag (max roundness) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] size=[400, 300],r=150] |
| `rounded_rect_empty_left` | shape | `no_layer_selected` | Rounded Rectangle drag + Left arrow mid-drag (zero roundness) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] size=[400, 300],r=0] |
| `ellipse_empty` | shape | `no_layer_selected` | Ellipse drag (400,200)->(800,500) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`椭圆 1` gP=[-360, -190] size=[400, 300]] |
| `ellipse_empty_bezier` | shape | `no_layer_selected` | Ellipse with Bezier Path checked drag (400,200)->(800,500) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`椭圆 1` gP=[-360, -190] bezier(closed=True,nv=4)] |
| `ellipse_empty_double_click` | shape | `no_layer_selected` | Ellipse double-click tool icon | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`椭圆 1` gP=[0, 0] size=[1920, 1080]] |
| `polygon_empty_horiz` | shape | `no_layer_selected` | Polygon drag center (600,350)->(800,350) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`多边星形 1` gP=[-360, -190] type=2,pts=5,R=200,r=100,rot=90,outRnd=0] |
| `polygon_empty_diag` | shape | `no_layer_selected` | Polygon drag center (600,350)->(700,200) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`多边星形 1` gP=[-360, -190] type=2,pts=5,R=180.2776,r=90.1388,rot=33.6901,outRnd=0] |
| `polygon_empty_up2` | shape | `no_layer_selected` | Polygon drag center (600,350)->(800,350) + Up arrow x2 | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`多边星形 1` gP=[-360, -190] type=2,pts=7,R=200,r=100,rot=90,outRnd=0] |
| `polygon_empty_double_click` | shape | `no_layer_selected` | Polygon double-click tool icon | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`多边星形 1` gP=[0, 0] type=2,pts=5,R=540,r=50,rot=0,outRnd=0] |
| `star_empty_horiz` | shape | `no_layer_selected` | Star drag center (600,350)->(800,350) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`多边星形 1` gP=[-360, -190] type=1,pts=5,R=200,r=100,rot=90,outRnd=0] |
| `star_empty_up2` | shape | `no_layer_selected` | Star drag center (600,350)->(800,350) + Up arrow x2 | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`多边星形 1` gP=[-360, -190] type=1,pts=7,R=200,r=100,rot=90,outRnd=0] |
| `star_empty_right3` | shape | `no_layer_selected` | Star drag center (600,350)->(800,350) + Right arrow x3 | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`多边星形 1` gP=[-360, -190] type=1,pts=7,R=200,r=100,rot=90,outRnd=30] |
| `star_empty_double_click` | shape | `no_layer_selected` | Star double-click tool icon | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`多边星形 1` gP=[0, 0] type=1,pts=5,R=540,r=270,rot=0,outRnd=0] |
| `rect_two_solids_none_sel` | shape | `no_layer_selected` | Rectangle drag with 2 existing unselected solids | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] size=[400, 300],r=0] / L2 AVLayer `TopSolid` (sel=False, A=[960, 540], P=[960, 540]) / L3 AVLayer `BottomSolid` (sel=False, A=[960, 540], P=[960, 540]) |
| `rect_empty_center_anchor_on` | shape | `no_layer_selected` | Rectangle drag (400,200)->(800,500) with Shape Layer Center Anchor Point = true | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[600, 350]) [`矩形 1` gP=[0, 0] size=[400, 300],r=0] |
| `rect_solid_full_mask` | shape | `non_shape_selected` | Rectangle drag (400,200)->(800,500) on 1920x1080 Solid | L1 AVLayer `Solid 1` (sel=True, A=[960, 540], P=[960, 540]) [Mask: `蒙版 1` mode=ADD,roto=False,closed=True,nv=4] |
| `ellipse_solid_full_mask` | shape | `non_shape_selected` | Ellipse drag (400,200)->(800,500) on 1920x1080 Solid | L1 AVLayer `Solid 1` (sel=True, A=[960, 540], P=[960, 540]) [Mask: `蒙版 1` mode=ADD,roto=False,closed=True,nv=4] |
| `rect_solid_full_dblclick` | shape | `non_shape_selected` | Rectangle double-click tool icon on 1920x1080 Solid | L1 AVLayer `Solid 1` (sel=True, A=[960, 540], P=[960, 540]) [Mask: `蒙版 1` mode=ADD,roto=False,closed=True,nv=4] |
| `ellipse_solid_full_dblclick` | shape | `non_shape_selected` | Ellipse double-click tool icon on 1920x1080 Solid | L1 AVLayer `Solid 1` (sel=True, A=[960, 540], P=[960, 540]) [Mask: `蒙版 1` mode=ADD,roto=False,closed=True,nv=4] |
| `rect_solid_offset_scaled_mask` | shape | `non_shape_selected` | Rectangle drag (800,450)->(1100,650) on 600x400 Solid @ Pos[1000,600] Scale[150%,100%] | L1 AVLayer `OffsetSolid` (sel=True, A=[300, 200], P=[1000, 600]) [Mask: `蒙版 1` mode=ADD,roto=False,closed=True,nv=4] |
| `rect_two_solids_sel2` | shape | `non_shape_selected` | Rectangle drag (400,200)->(800,500) with Layer 2 selected among 2 solids | L1 AVLayer `TopSolid` (sel=False, A=[960, 540], P=[960, 540]) / L2 AVLayer `BottomSolid` (sel=True, A=[960, 540], P=[960, 540]) [Mask: `蒙版 1` mode=ADD,roto=False,closed=True,nv=4] |
| `rect_on_shape_add_group` | shape | `shape_selected` | Rectangle drag (1000,400)->(1300,700) on selected Shape Layer (Tool Creates Shape) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 2` gP=[190, 10] size=[300, 300],r=0; `矩形 1` gP=[-360, -190] size=[400, 300],r=0] |
| `rect_on_moved_shape_add_group` | shape | `shape_selected` | Rectangle drag (1000,400)->(1300,700) on selected Shape Layer moved to Pos[1100,600] | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[1100, 600]) [`矩形 2` gP=[50, -50] size=[300, 300],r=0; `矩形 1` gP=[-360, -190] size=[400, 300],r=0] |
| `rect_on_shape_dblclick` | shape | `shape_selected` | Rectangle double-click tool icon on selected Shape Layer (Tool Creates Shape) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 2` gP=[0, 0] size=[1920, 1080],r=0; `矩形 1` gP=[-360, -190] size=[400, 300],r=0] |
| `rect_on_shape_mask_mode` | shape | `shape_selected` | Rectangle drag (450,250)->(750,450) on selected Shape Layer with Tool Creates Mask | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] size=[400, 300],r=0] [Mask: `蒙版 1` mode=ADD,roto=False,closed=True,nv=4] |
| `pen_empty_single_click` | pen | `no_layer_selected` | Pen single click (500,350) in empty comp | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`形状 1` gP=[0, 0] bezier(closed=False,nv=1)] |
| `pen_empty_double_click` | pen | `no_layer_selected` | Pen double-click tool icon in empty comp | `0 图层`（无操作） |
| `pen_empty_open_linear` | pen | `no_layer_selected` | Pen open linear 3 clicks (400,300)->(700,200)->(900,450) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`形状 1` gP=[0, 0] bezier(closed=False,nv=3)] |
| `pen_empty_closed_linear` | pen | `no_layer_selected` | Pen closed linear triangle (400,300)->(700,200)->(900,450)->(400,300) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`形状 1` gP=[0, 0] bezier(closed=True,nv=3)] |
| `pen_empty_bezier_curve` | pen | `no_layer_selected` | Pen open Bezier 3 click-drags: (400,400)+(+80,-60), (800,400)+(+80,+60), (600,650)+(-80,0) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`形状 1` gP=[0, 0] bezier(closed=False,nv=3)] |
| `pen_empty_alt_break` | pen | `no_layer_selected` | Pen click (400,400), drag (700,350)+(+80,0) then Alt-drag outTangent to (+80,+48), click (950,500) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`形状 1` gP=[0, 0] bezier(closed=False,nv=3)] |
| `pen_empty_rotobezier` | pen | `no_layer_selected` | Pen RotoBezier closed triangle (400,300)->(700,200)->(900,450)->(400,300) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`形状 1` gP=[0, 0] bezier(closed=True,nv=3)] |
| `pen_empty_center_anchor_on` | pen | `no_layer_selected` | Pen closed triangle (400,300)->(700,200)->(900,450)->(400,300) with Shape Layer Center Anchor Point = true | L1 ShapeLayer `形状图层 1` (sel=True, A=[-310, -215], P=[650, 325]) [`形状 1` gP=[0, 0] bezier(closed=True,nv=3)] |
| `pen_solid_open_mask` | pen | `non_shape_selected` | Pen open linear 3 clicks (400,300)->(700,200)->(900,450) on Solid | L1 AVLayer `Solid 1` (sel=True, A=[960, 540], P=[960, 540]) [Mask: `蒙版 1` mode=NONE,roto=False,closed=False,nv=3] |
| `pen_solid_closed_bezier_mask` | pen | `non_shape_selected` | Pen closed Bezier 3 click-drags + close on Solid | L1 AVLayer `Solid 1` (sel=True, A=[960, 540], P=[960, 540]) [Mask: `蒙版 1` mode=ADD,roto=False,closed=True,nv=3] |
| `pen_solid_rotobezier_mask` | pen | `non_shape_selected` | Pen RotoBezier closed triangle on Solid | L1 AVLayer `Solid 1` (sel=True, A=[960, 540], P=[960, 540]) [Mask: `蒙版 1` mode=ADD,roto=True,closed=True,nv=3] |
| `pen_on_shape_add_group` | pen | `shape_selected` | Pen open linear 3 clicks (1000,300)->(1200,250)->(1300,450) on selected Shape Layer (Tool Creates Shape) | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`形状 1` gP=[0, 0] bezier(closed=False,nv=3); `矩形 1` gP=[-360, -190] size=[400, 300],r=0] |
| `pen_on_shape_mask_mode` | pen | `shape_selected` | Pen closed triangle (450,250)->(750,250)->(600,450) on selected Shape Layer with Tool Creates Mask | L1 ShapeLayer `形状图层 1` (sel=True, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] size=[400, 300],r=0] [Mask: `蒙版 1` mode=ADD,roto=False,closed=True,nv=3] |
| `type_empty_click` | type | `no_layer_selected` | Type single click (500,400) + type 'Hello' | L1 TextLayer `Hello` (sel=True, A=[0, 0], P=[500, 400]) [Point text='Hello'] |
| `type_empty_drag_box` | type | `no_layer_selected` | Type drag box (400,300)->(900,550) + type 'Hello World' | L1 TextLayer `Hello World` (sel=True, A=[0, 0], P=[650, 425]) [Paragraph box=[500, 250],boxPos=[-250, -125]] |
| `type_empty_no_text` | type | `no_layer_selected` | Type single click (500,400) + commit without typing | L1 TextLayer `<空文本图层>` (sel=True, A=[0, 0], P=[500, 400]) [Point text=''] |
| `type_empty_double_click` | type | `no_layer_selected` | Type double-click tool icon in empty comp | L1 TextLayer `<空文本图层>` (sel=True, A=[0, 0], P=[960, 540]) [Point text=''] |
| `type_two_solids_none_sel` | type | `no_layer_selected` | Type single click (500,400) + type 'TopNoneSel' with 2 unselected solids | L1 TextLayer `TopNoneSel` (sel=True, A=[0, 0], P=[500, 400]) [Point text='TopNoneSel'] / L2 AVLayer `TopSolid` (sel=False, A=[960, 540], P=[960, 540]) / L3 AVLayer `BottomSolid` (sel=False, A=[960, 540], P=[960, 540]) |
| `type_solid_selected` | type | `non_shape_selected` | Type single click (500,400) + type 'OnSolid' with Solid selected | L1 TextLayer `OnSolid` (sel=True, A=[0, 0], P=[500, 400]) [Point text='OnSolid'] / L2 AVLayer `Solid 1` (sel=False, A=[960, 540], P=[960, 540]) |
| `type_two_solids_sel2` | type | `non_shape_selected` | Type single click (500,400) + type 'AboveSel' with Layer 2 selected among 2 solids | L1 AVLayer `TopSolid` (sel=False, A=[960, 540], P=[960, 540]) / L2 TextLayer `AboveSel` (sel=True, A=[0, 0], P=[500, 400]) [Point text='AboveSel'] / L3 AVLayer `BottomSolid` (sel=False, A=[960, 540], P=[960, 540]) |
| `type_shape_selected` | type | `shape_selected` | Type single click (500,400) + type 'OnShape' with Shape Layer selected | L1 TextLayer `OnShape` (sel=True, A=[0, 0], P=[500, 400]) [Point text='OnShape'] / L2 ShapeLayer `形状图层 1` (sel=False, A=[0, 0], P=[960, 540]) [`矩形 1` gP=[-360, -190] size=[400, 300],r=0] |
| `type_existing_text_away` | type | `non_shape_selected` | Type single click (1000,600) + type 'Second' while Text Layer 'First' is selected | L1 TextLayer `Second` (sel=True, A=[0, 0], P=[1000, 600]) [Point text='Second'] / L2 TextLayer `First` (sel=False, A=[0, 0], P=[500, 400]) [Point text='First'] |
