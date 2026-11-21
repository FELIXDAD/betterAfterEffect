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

