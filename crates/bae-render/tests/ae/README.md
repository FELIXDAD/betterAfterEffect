# 与 After Effects 对照的用例

| 文件 | 用途 |
|---|---|
| `cases.json` | 用例定义（两边共用）：合成尺寸、图层（纯色 / 素材 / 预合成）、位置、缩放、旋转、不透明度、混合模式、轨道遮罩、保留基础透明度；`tolerance` 是允许的每通道最大差（默认 1，即取整误差）。 |
| `render_cases.jsx` | 在 AE 里搭出全部用例（8 bpc、不线性混合、无色彩管理、图层质量"最佳"），用 `saveFrameToPng` 输出第 0 帧（预乘 RGBA）。 |
| `cases.rs` | 生成两边共用的素材（灰阶渐变、彩色渐变、带软边 alpha 的纹理），用 bae-model 命令搭同样的工程，逐像素比较。 |
| `golden/` | AE 26.2.1 的输出，`tests/ae_golden.rs` 在 CI 里用它比较，不需要 AE。软件 / 虚拟适配器（CI 的 Windows runner 用 WARP）上每个用例的容差加 2（纹理滤波精度略低），真 GPU 上保持原容差；测试输出里打印适配器和实际容差。 |

重新生成 golden（改了用例或换了 AE 版本时）：

```sh
cargo run -p bae-render --example ae_compare -- prepare      # 素材 → /private/tmp/bae-04-ae/assets
tools/ae/aectl run crates/bae-render/tests/ae/render_cases.jsx # AE → /private/tmp/bae-04-ae/ae
cargo run -p bae-render --example ae_compare -- compare      # BAE 渲染 + 对比表
cp /private/tmp/bae-04-ae/ae/*.png crates/bae-render/tests/ae/golden/
```

注意：`saveFrameToPng` 是异步的，脚本里每次调用后等文件出现再继续；连续调用不等待时 AE 曾经卡死在激活事件里。ExtendScript 的 `catch` 变量不要叫 `e`（和 aectl 的包装冲突）。

## 结论（AE 26.2.1，2026-11-20）

- **正常、相乘、滤色、叠加**：在 256×256 的 B(Cb, Cs) 全表上与 AE 最大差 1（取整），符合 PDF/W3C 的通用合成公式（AE-CMP-012），叠加按底色 ≤ 0.5 分支。
- **相加**：AE 是预乘颜色直接相加 `cop = cbp + csp`，alpha 取并集（8 bpc 再截断），**不是** W3C 公式；源图层半透明时两者差 14/255。已按 AE 实现，确认了规格 AE-CMP-018 里"UNVERIFIED"的推测。
- **轨道遮罩**：Alpha / Alpha 反转 / 亮度 / 亮度反转一致；亮度用 Rec.601 权重（0.299, 0.587, 0.114），作用在预乘颜色上（透明处算黑）。
- **保留基础透明度**：与 AE-CMP-032 的 source-atop 公式一致。
- **采样**：图层的栅格按"边缘过渡宽度 = max(1 个目标像素, 1 个源像素)"处理：原尺寸、旋转、缩小时是像素框滤波（与 AE 完全一致，差 0）；放大时等于双线性插值且栅格外透明（纯色和预合成放大 200%/250% 的边缘与 AE 一致）。缩小的纹理在像素覆盖范围内做框滤波超采样；软边纹理缩到 40% 时仍有最大 10 的差（2048 个像素里 43 个差 > 1），AE 的缩小滤波核还没完全对上。

## 蒙版用例（2026-11-21，待 AE 出 golden）

`cases.json` 里 `mask_` 开头的 8 个用例标了 `"pending_golden": true`：这台机器没有 AE，golden 还没生成，`ae_golden` 测试遇到没有 golden 的待定用例会跳过并打印。**需要在装了 AE 的机器上跑**上面"重新生成 golden"的四步（`render_cases.jsx` 已经会按用例里的 `masks` 建蒙版：`ADBE Mask Atom` 的路径、模式、反转、`ADBE Mask Opacity`、`ADBE Mask Feather`、`ADBE Mask Offset`），把 `mask_*.png` 拷进 `golden/`，再删掉这几个用例的 `pending_golden`。用 `ae_compare -- compare` 的表格对照下面的待核项，有差异就改 `crates/bae-render/src/mask.rs`。

| 用例 | 核对什么（docs/spec/ae/compositing-3d-color.md） |
|---|---|
| `mask_modes` | 每种模式单独用（50%）时的起始值：相减 / 交集 / 变暗从 1 开始；在一个 60% 相加蒙版下面再叠 50% 的各模式：相加是 `a + m − a·m` 还是 `min(1, a + m)`，差值是 `a + m − 2am` 还是 `|a − m|`（AE-CMP-038） |
| `mask_invert_opacity` | 蒙版不透明度在反转之后乘（反转 40% → 外面 40%，里面 0），三个蒙版组合（AE-CMP-039/040） |
| `mask_feather` | 羽化 0 的半像素边缘覆盖率；羽化 10 / 40 的宽度（σ = 0.38 × 羽化，来自 Skottie 的经验值）；X/Y 不同的羽化（AE-CMP-041） |
| `mask_feather_large` | 200 px 羽化的完整剖面，直接拟合 σ 和羽化值的比例 |
| `mask_feather_frame` | 蒙版比图层大时，羽化在图层边界按"外面是透明"处理（图层自身边缘变软） |
| `mask_expansion` | 扩大时外角是圆的，收缩时内角是尖的；星形收缩（AE-CMP-044） |
| `mask_fill_rule` | 自相交路径（五角星）中间填不填：填 = 非零环绕，空 = 奇偶（AE-CMP-045） |
| `mask_scaled_matte` | 放大 250% 的纯色上的蒙版边缘（BAE 按屏幕分辨率栅格化，边缘是锐的；AE 的纯色是栅格图层，可能是先栅格化再放大）；带羽化蒙版的图层当轨道遮罩 |


## 图层混合模式（AE 26.2.1，2026-11-24）

新增的 33 个 `blend_` 用例检查小图层的内外边界，在 80% 不透明度的底层上以 65% 不透明度叠加源层。两种溶解改用 100% 不透明度，以便与 AE 逐像素核对确定性端点；半透明像素的随机选择、跨帧及 ROI 一致性由 GPU/CPU 测试覆盖。

`grid_` 用例读取 `cases.json` 的 `hsl_pairs`（底色、源色）。每行一对 RGB，每四列依次使用 25/50/75/100% 的底层不透明度，组内四列按同一顺序改变源层不透明度。四种模式共 384 格，8 bpc、每通道满值 255；Metal 上色相、饱和度、颜色、明度最大差为 2、3、1、2。单独的半透明样本中，色相和饱和度的最大差为 0 和 1。[饱和度校准跟踪 #257](https://github.com/FELIXDAD/betterAfterEffects/issues/257)

## 3D 图层与运动模糊（AE 26.2.1，2026-11-24）

`threed_` 和 `mb_` 开头的 17 个用例（`cases.json` 新增的键：`three_d`、三维 `pos`、`rot3`（X/Y/Z 旋转）、`orient`（方向）、摄像机图层 `camera: {type, zoom}` + `poi`、`mb`（图层运动模糊开关）、`keys`（`pos` / `rot` 的线性关键帧，按帧）、合成的 `motion_blur: {angle, phase, samples}`、渲染帧 `frame`）。只重渲这些用例：

```sh
tools/ae/aectl eval 'var BAE_AE_ONLY="threed_,mb_"; $.evalFile("<仓库>/crates/bae-render/tests/ae/render_cases.jsx")'
```

| 用例 | 最大差 | 说明 |
|---|---:|---|
| `threed_flat_z0` | 0 | 默认摄像机下 z = 0 的 3D 图层与 2D 逐像素相同（AE-CMP-400/403） |
| `threed_depth`、`threed_group_split` | 1 | 组内按深度、不按图层顺序；2D 图层把组隔开后按图层顺序（AE-CMP-404/406） |
| `threed_rot_y`、`threed_rot_orient` | 2、4 | 透视投影、X/Y/Z 旋转和方向的顺序（`Or · Rot`，AE-ANI-014） |
| `threed_camera_two_node`、`threed_camera_one_node` | 2 | 双节点（瞄准目标点）和单节点摄像机，素材的透视采样 |
| `threed_intersect` | 4 | 相交图层沿交线切开 |
| `threed_precomp` | 16 | 3D 预合成与其他 3D 图层一起按深度排序；剩下的差在预合成内部白块的边缘 |
| `threed_floor` | 25 | 只在地面两条斜边上：极度透视压缩的斜边，AE 的过渡比我们窄（总覆盖率相同，内部像素一致） |
| `mb_translate`、`mb_angle_phase`、`mb_footage` | 1 | 2D 平移、360° / 0° 快门、素材 |
| `mb_3d` | 3 | 3D 图层沿 z 运动（固定每帧样本数） |
| `mb_rotate` | 5 | 2D 旋转（自适应样本数按角点的弧长计） |
| `mb_camera` | 15 | 双节点摄像机绕目标点移动：图层几乎不动，差在上下边缘不到 0.1 像素的位置 |
| `mb_off` | 0 | 合成开、图层关：不模糊 |

实测得到、已经按此实现的结论（`docs/spec/` 里原来标 UNVERIFIED 的几处）：

- **采样时间**（AE-ANI-103）：`t_k = t_open + dur · k / N`，第一个样本在快门打开时，最后一个在关闭前 `dur/N`；不是 `(k + ½)/N`。`mb_translate` 的拖尾两端各差 1/16 的台阶只有这样才对得上（误差 0–1）。改在 `bae_anim::MotionBlur::sample_times`。
- **2D 图层的自适应样本数**（AE-ANI-104）：`N = clamp(floor(d / step) + 1, 每帧样本数, 自适应采样上限)`，`d` 是整个快门内角点的最大位移（像素），位移小于 32 像素时 `step = 1.3`，32 像素起 `step = 1.9`。用 30×30 纯色、每帧样本数 2、上限 256、位移 3–300 像素的 30 个合成逐一拟合（每个都只有一个 N 误差为 0–1），与图层大小、方向无关，上限生效。代码在 `motion::adaptive_samples`。
- **相交的平局**（AE-CMP-404/405）：两个图层在视线方向上相差不到 1 像素时，图层顺序靠上的在前。45° 相交的切口因此向下面那个图层一侧移 1 像素（`three_d::TIE_DEPTH`）。
- **3D 预合成**（AE-CMP-406）：未折叠的 3D 预合成**不**隔开 3D 组，和其他 3D 图层一起按深度排序（规格把它列为隔断，实测不是）。
- **边缘**：采样纹理时不读内容以外的纹素（池里的纹理比内容大），四条边都只由覆盖率过渡决定；之前右边、下边会混进透明纹素，3D 预合成上差到 65。2D 的已有用例不受影响。

## 灯光（AE 26.2.1，2026-11-25）

`light_` 开头的 8 个用例（`cases.json` 新增：灯光图层 `light: {type, intensity, color, cone, feather, falloff, radius, dist}` + `pos` / `poi` / `adjustment`，图层的 `mat: {amb, diff, spec, shin, metal, accepts}`）。

| 用例 | 最大差 | 说明 |
|---|---:|---|
| `light_point` | 1 | 点光 + 20% 环境光，默认材质（环境 100、漫射 50、镜面 50、反光度 5、金属 100） |
| `light_spot` | 1 | 聚光（锥形 50°、羽化 30%、平滑衰减）、彩色灯光、转过的图层、镜面 70 / 反光度 30 / 金属 50 |
| `light_parallel` | 2 | 平行光 + 彩色环境光，两个转向不同的图层 |
| `light_footage` | 1 | 素材纹理上的点光（反向平方限制衰减） |
| `light_accepts`、`light_flat` | 1 | 「接受灯光」关、2D 图层：不打光 |
| `light_scope` | 1 | 打开「调整图层」开关的灯光只照下面的图层；上面的图层**不打光**（不是变暗） |
| `light_mb` | 4 | 打光的图层加运动模糊 |

光照模型先用单独的标定合成（200×200 的 3D 纯色正对默认摄像机，每个合成只变一项）在 AE 里逐项拟合，全部对上（多数误差 0–1）后再写着色器：

- 环境光：`C · 环境系数 · Σ 环境光`；漫射：Lambert `max(0, N·L)`，与像素中心的光线方向逐点一致（误差 0–1）。法线朝向观察者（从背面看也照亮朝向摄像机的一面，灯光在背后时全黑）。
- 镜面：Blinn-Phong（半角向量 `normalize(L + V)`，`V` 指向摄像机），指数就是「镜面反光度」的百分数（5 % → 5，100 % → 100）；只在 `N·L > 0` 时有。AE 的峰值在高反光度时比纯 Blinn-Phong 低一点（100 % 时 249 而不是 255，约 `1 − 0.00024·n`），差 ≤ 3，没有加这项修正。
- 「金属质感」把高光颜色从白色混到图层颜色；灯光颜色和强度（%）乘在每一项上；负强度相减，结果不小于 0；8 bpc 截断到 1。
- 聚光：`smoothstep(cos 外角, cos 内角, cos θ)`，外角 = 锥形角度 / 2，内角 = 外角 × (1 − 羽化 %)；是在**余弦**上的 smoothstep（在角度上做差到 86）。羽化 0 是硬边。
- 衰减：「平滑」是 `1 − smoothstep(半径, 半径 + 衰减距离, d)`（线性斜坡差 9），「反向平方限制」是半径外 `(半径 / d)²`（误差 0）。

另外修了 #288 的一个 3D 问题（`light_parallel` 暴露出来）：BSP 切开图层时，两块在切口处各自做抗锯齿，叠起来是 0.5 over 0.5 = 0.75，切口变成半透明的缝。现在先画的那块覆盖越过切口 1 像素，只有后画的那块做抗锯齿；而且屏幕上不重叠的图层不再互相切（只在重叠的图层簇里排序）。`tests/three_d.rs` 的 `cuts_leave_no_see_through_seams` 去掉修正时会失败。

正交视图（正面、左侧……）在 AE 里只能在界面上切换，`saveFrameToPng` 只渲活动摄像机，所以没有 AE 对照；用 GPU 测试按几何检查（`orthographic_views_draw_without_perspective`）。

## 阴影（AE 26.2.1，2026-11-25）

`shadow_` 开头的 7 个用例（`cases.json` 新增：灯光 `shadows` / `dark` / `diff`，材质 `casts` / `accepts_shadows` / `trans`）。

| 用例 | 最大差 | 说明 |
|---|---:|---|
| `shadow_point` | 12 | 点光投射纯色方块的硬阴影；差都在阴影边缘一个像素上 |
| `shadow_spot_soft` | 16 | 聚光、扩散 15、深度 80、40% 透光的彩色投射图层（转过 20°）：半影形状 |
| `shadow_parallel` | 33 | 平行光、两个不同深度的投射图层；差在两块阴影相接的边角（AE 阴影图的边更软、略有偏移） |
| `shadow_footage` | 13 | 带 alpha 的素材投射（形状跟着 alpha 走） |
| `shadow_switches` | 19 | 投射图层「投影」关 / 开，另一盏不投影的灯 |
| `shadow_receiver_off` | 1 | 接受图层「接受阴影」关 |
| `shadow_two_lights` | 6 | 两盏投影灯、深度 100 / 60 |

先用 18 个只变一项的标定合成（200×200 接受图层、前面 60 像素的投射方块、左上方的灯光）确定模型：

- 几何：阴影就是从灯光沿光线投到接受平面上的投射图层（平行光沿灯光方向），边的位置与解析投影一致。
- 阴影深度 50% → 剩一半光（误差 0）；不透明度 50% 的投射图层挡一半；透光率：阴影颜色 = 透光率 × 投射图层**自身**（不打光的）颜色（50% 红、100% 蓝都精确）。
- 「阴影扩散」：边缘是线性过渡（方框滤波），宽度 = 2 · 扩散 · 投射到接受的距离 / 灯光到投射的距离——等于一个半边长为扩散值的方形面光源（扩散 10、40 的拟合误差 4 和 1.7）。
- 没有扩散时 AE 的边也有约 2 像素的过渡（阴影图分辨率 = 合成尺寸时的过滤）：取 1 个接受像素的方框（`SHADOW_SOFTNESS`，各用例最大差最小）；AE 的阴影图过滤和灯光视锥有关，所以边缘像素还有 10–20 的差（平行光的交角处 33）。
- 灯光、投射、接受任一开关关掉都没有阴影；2D 图层隔开的 3D 组之间没有阴影；阴影同时去掉漫射和镜面，不影响环境光。

## 时间重映射、时间伸缩、冻结帧（AE 26.2.1，2026-11-25）

`time_` 开头的 6 个用例，素材是脚本生成的 20 帧 PNG 序列（`seq_0000.png` …，每帧颜色不同、竖条每帧右移 3 像素），在 AE 里按 10 fps 解释，放进 30 fps 的合成（`cases.json` 新增：`sequence` / `seq_rate` / `seq_frames`，图层 `stretch`、`start_time`、`remap`（按合成帧打的时间重映射关键帧，值是秒）、`remap_hold`）。

| 用例 | 最大差 | 说明 |
|---|---:|---|
| `time_seq_plain` | 0 | 10 fps 序列在 30 fps 合成里：第 7 帧显示源第 2 帧（向下取整） |
| `time_stretch_200` | 0 | 伸缩 200% |
| `time_reverse` | 0 | 伸缩 −100%（反向） |
| `time_remap_keys` | 0 | 时间重映射线性关键帧倒放 |
| `time_remap_hold` | 0 | 单个定格关键帧（冻结） |
| `time_precomp_stretch` | 0 | 伸缩 50% 的预合成（内部按映射后的时间求值） |

命令和 AE 菜单命令的对照（在 AE 里执行菜单命令后读回关键帧）：

- 启用时间重映射：两个线性关键帧，源开头 0→0、源结尾 时长→时长（与 `layer.time_remap` 一致）。
- 冻结帧：一个关键帧放在当前时间，值是此刻显示的源时间（伸缩 50% 的图层在 0.5 s 处值为 1.0），**入和出都是定格**；不改伸缩和出点。
- 在最后一帧上冻结：0→0 线性，最后一个关键帧在「源时长 − **一个合成帧**」（不是源帧；2 s 的 10 fps 素材在 30 fps 合成里是 1.9667 s），入线性、出定格；出点延长到合成结尾。
