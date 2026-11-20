## Generate effects

Conventions for this part (apply to every item below unless the item says otherwise):

- Parameter tables are in AE Effect Controls order. "param matchName" follows AE's `<effect matchName>-NNNN` scheme; NNNN is the parameter's stable disk index and is **not** always the UI position (e.g. Fill). Values marked "obs." were observed in exported Lottie JSON (`ef[].mn`); others are derived from the scheme and are UNVERIFIED until checked in AE 2026 with `effect.property(i).matchName`.
- Group start/end markers (topics) consume indices; they are listed as `(group)` / `(group end)` rows.
- "Percent (0-1)" means AE stores a fraction 0..1 and displays it ×100 with "%" (Lottie exports the fraction, e.g. Fill Opacity `k: 1`). "Percent (0-100)" means AE stores the displayed number.
- bpc/GPU columns come from the docsforadobe first-party match-name table (`docs/matchnames/effects/firstparty.md`); "GPU —" means no Mercury GPU path is listed.
- Pixel-unit parameters scale with the downsample factor (see effects.md, Downsample-factor scaling of pixel parameters); point parameters are in layer space.
- Unless stated, generator effects render inside the layer's existing bounds and do **not** expand them (see effects.md, Output bounds expansion; Grow Bounds below).
- "Blending Mode" menus on Generate effects ("None" + the layer blend modes) composite the generated image over the effect input; "None" outputs the generated image only. Formulas for the modes: compositing-3d-color.md, Blend modes. UNVERIFIED: exact menu order after "None, Normal"; BAE stores the mode by stable name, not by index.

### AE-FX-600 · Fill (填充)

- **What it does:** Fills a chosen mask (or all masks, or the whole layer alpha) with a solid color.
- **Exact semantics:**
  - matchName `ADBE Fill`; category Generate; 32 bpc; GPU —. Lottie effect `ty: 21`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Fill Mask (填充蒙版) | `ADBE Fill-0001` (obs.) | Mask reference popup | None | None or any mask on this layer | — | Lottie exports as `ty: 10`, value = mask index (0 = None). |
  | 2 | All Masks (所有蒙版) | `ADBE Fill-0007` (obs.) | Checkbox | Off | — | — | When on, Fill Mask is ignored and the union of all masks (by their modes, see below) is filled. |
  | 3 | Color (颜色) | `ADBE Fill-0002` (obs.) | Color | (255, 0, 0) red (UNVERIFIED) | any | RGB | |
  | 4 | Invert (反转) | `ADBE Fill-0006` (obs.) | Checkbox | Off | — | — | Inverts the fill matte. |
  | 5 | Horizontal Feather (水平羽化) | `ADBE Fill-0003` (obs.) | Float slider | 0 | slider 0-50, valid 0-1000 (UNVERIFIED) | px | Box/Gaussian softening of the matte along x. |
  | 6 | Vertical Feather (垂直羽化) | `ADBE Fill-0004` (obs.) | Float slider | 0 | slider 0-50, valid 0-1000 (UNVERIFIED) | px | Along y. |
  | 7 | Opacity (不透明度) | `ADBE Fill-0005` (obs.) | Float slider | 100% | 0-100% | Percent (0-1) | Lottie `k: 1` = 100%. |

  ```
  // src = effect input (premultiplied), C = Color, o = Opacity (0..1)
  if AllMasks:        M = rasterize union of all masks on the layer (mask modes applied, mask feather/expansion/opacity applied)
  elif FillMask!=None: M = rasterize(FillMask path) with that mask's feather/expansion/opacity, ignoring its mode
  else:               M = src.a                     // no mask: colorize the layer's own alpha
  if Invert: M = 1 - M
  M = separable_blur(M, sigma_x(HFeather), sigma_y(VFeather))   // UNVERIFIED kernel; Gaussian with radius = feather px
  // UNVERIFIED: with a mask, the fill is NOT clipped to the layer alpha; it composites over src
  fill = premul(C, M)
  out  = lerp(src, fill over src, o)   // with no mask: out.rgb = C*src.a, out.a = src.a, then mixed by o
  ```
  - Edge cases: Fill Mask referencing a deleted mask resets to None. With no masks and None selected, every non-transparent pixel becomes Color with original alpha (the common "colorize text/shape" use).
- **Priority:** P0 (one of the most used effects; also exported by bodymovin).
- **Owner:** bae-effects (+ bae-render)
- **Notes:** Help: applying Stroke before Fill hides the inner half of the stroke; order matters (effect stack is top-down, effects.md, Effect stack). Masks used by Fill normally have mode None so they do not cut the layer (compositing-3d-color.md, Masks). lottie-web `SVGFillFilter` / Skottie `FillEffect` implement only Color+Opacity (Skottie indices 2 and 6), ignoring mask/feather. Implementation hint: one fragment pass `mix(src, vec4(C*M,M) over src, o)`; feather = two 1D blur passes on the mask texture reused from the mask rasterizer.

### AE-FX-601 · Gradient Ramp (梯度渐变)

- **What it does:** Replaces the layer with a two-color linear or radial gradient, optionally dithered and mixed with the original.
- **Exact semantics:**
  - matchName `ADBE Ramp`; category Generate; 32 bpc; GPU since AE 14.2 (2017). Lottie effect `ty: 5` (generic), params `ef[0..7]`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Start of Ramp (渐变起点) | `ADBE Ramp-0001` (obs.) | Point | (layerW/2, 0) | any | px, layer space | |
  | 2 | Start Color (起始颜色) | `ADBE Ramp-0002` (obs.) | Color | (0,0,0) black | any | RGB | |
  | 3 | End of Ramp (渐变终点) | `ADBE Ramp-0003` (obs.) | Point | (layerW/2, layerH) | any | px | |
  | 4 | End Color (结束颜色) | `ADBE Ramp-0004` (obs.) | Color | (255,255,255) white | any | RGB | |
  | 5 | Ramp Shape (渐变形状) | `ADBE Ramp-0005` (obs.) | Popup | Linear Ramp | 1 = Linear Ramp (线性渐变), 2 = Radial Ramp (径向渐变) | — | Skottie treats value 1 as linear, anything else as radial. |
  | 6 | Ramp Scatter (渐变散射) | `ADBE Ramp-0006` (obs.) | Float slider | 0 | slider 0-256? valid 0-512 (UNVERIFIED) | — | Dither amount. |
  | 7 | Blend With Original (与原始图像混合) | `ADBE Ramp-0007` (obs.) | Float slider | 0% | 0-100% | Percent (0-100) | |
  | 8 | Swap Colors (交换颜色) | `ADBE Ramp-0008` (obs.) | Button (no value) | — | — | — | Swaps Start/End Color values (one undoable edit); added AE CC 2015 (UNVERIFIED version). Lottie exports it as `ty: 6`. |

  ```
  // p = pixel centre in layer space, S = start, E = end
  d = E - S
  if Linear: t = dot(p - S, d) / dot(d, d)          // projection; constant along lines perpendicular to S->E
  if Radial: t = length(p - S) / length(d)           // S is centre, |E-S| is radius
  t = clamp(t, 0, 1)
  if Scatter > 0: t = clamp(t + (hash(p) - 0.5) * Scatter * k, 0, 1)   // UNVERIFIED: per-pixel uniform noise, k ≈ 1/256 per unit
  ramp = lerp(StartColor, EndColor, t)              // straight RGB in working space, alpha = 1 (both colors opaque)
  out  = lerp(ramp, src, Blend/100)                 // src premultiplied; result alpha = lerp(1, src.a, Blend)
  if |d| == 0: t = (Linear ? 0 : 1) for all p (UNVERIFIED) -> solid colour
  ```
  - The ramp fills the **entire layer rectangle** regardless of the layer's alpha (applying it to text fills the whole text-layer box; users then use Preserve Underlying Transparency or a matte, compositing-3d-color.md, Preserve transparency).
  - Interpolation happens in the project's working space; with "Linearize Working Space" on the ramp is linear-light (compositing-3d-color.md, Color management). 8/16 bpc quantise after interpolation (banding is what Ramp Scatter hides). 32 bpc: colors may exceed 1.0 and are not clamped.
- **Priority:** P0
- **Owner:** bae-effects
- **Notes:** Skottie `GradientEffect` implements shape/points/colors but not scatter/blend. Implementation hint: analytic per-pixel shader; dither with a blue-noise texture seeded per frame-independent pixel position (AE's scatter pattern is static over time — UNVERIFIED).

### AE-FX-602 · 4-Color Gradient (四色渐变)

- **What it does:** Generates a smooth gradient blended from four colored points.
- **Exact semantics:**
  - matchName `ADBE 4ColorGradient`; category Generate; 16 bpc; GPU —. Lottie `ty: 5`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | — | Positions & Colors (位置和颜色) | `ADBE 4ColorGradient-0011` (obs.) | (group) | — | — | — | Group start; its end marker is `-0012` (obs.). |
  | 1 | Point 1 (点 1) | `ADBE 4ColorGradient-0001` (obs.) | Point | (0.1·W, 0.1·H) (UNVERIFIED) | any | px | |
  | 2 | Color 1 (颜色 1) | `-0002` (obs.) | Color | (255,255,0) yellow (UNVERIFIED) | any | RGB | |
  | 3 | Point 2 (点 2) | `-0003` (obs.) | Point | (0.9·W, 0.1·H) (UNVERIFIED) | any | px | |
  | 4 | Color 2 (颜色 2) | `-0004` (obs.) | Color | (0,255,255) cyan (UNVERIFIED) | any | RGB | |
  | 5 | Point 3 (点 3) | `-0005` (obs.) | Point | (0.1·W, 0.9·H) (UNVERIFIED) | any | px | |
  | 6 | Color 3 (颜色 3) | `-0006` (obs.) | Color | (255,0,255) magenta (UNVERIFIED) | any | RGB | |
  | 7 | Point 4 (点 4) | `-0007` (obs.) | Point | (0.9·W, 0.9·H) (UNVERIFIED) | any | px | |
  | 8 | Color 4 (颜色 4) | `-0008` (obs.) | Color | (0,0,255) blue (UNVERIFIED) | any | RGB | |
  | 9 | Blend (混合) | `-0009` (obs.) | Float slider | 100 | slider 0-250? valid 1-? (UNVERIFIED) | — | Higher = softer transitions. |
  | 10 | Jitter (抖动) | `-0010` (obs.) | Float slider | 0 | 0-100 (UNVERIFIED) | Percent (0-100) | Noise to reduce banding, applied only where banding could occur. |
  | 11 | Opacity (不透明度) | `-0013` (obs.) | Float slider | 100% | 0-100% | Percent (0-100) | Fraction of layer opacity. |
  | 12 | Blending Mode (混合模式) | `-0014` (obs.) | Popup | None (UNVERIFIED) | None + blend modes | — | |

  ```
  // UNVERIFIED reconstruction (Adobe describes four solid-colour circles blended together).
  // Inverse-distance weighting with exponent controlled by Blend:
  for i in 1..4: d_i = max(length(p - P_i), eps)
  pw = 2 + (100 - Blend)/25          // UNVERIFIED mapping; larger Blend -> smaller exponent -> softer
  w_i = 1 / pow(d_i, pw)
  rgb = sum(w_i * C_i) / sum(w_i)
  if Jitter > 0: rgb += (hash(p) - 0.5) * Jitter * banding_mask(p) * k
  gen = vec4(rgb, 1) ; out = blend(BlendingMode, gen * Opacity, src)
  ```
  - At a control point the output equals that point's color exactly (the weighting is singular there).
- **Priority:** P1 (common background generator; exact AE blend curve must be matched by reference renders).
- **Owner:** bae-effects
- **Notes:** BAE decision: implement the IDW model above, then fit `pw(Blend)` against AE reference frames (golden tests) before M1; keep the Blend→falloff mapping in one function. Effect control points for Points 1-4 are drawn in the Composition viewer (effects.md, Point parameters).

### AE-FX-603 · Grid (网格)

- **What it does:** Draws a rectangular grid of lines in a color, or uses it as an alpha matte via Invert/Blending Mode.
- **Exact semantics:**
  - matchName `ADBE Grid`; category Generate; 8 bpc; GPU —. Lottie `ty: 5`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Anchor (锚点) | `ADBE Grid-0001` (obs.) | Point | layer centre (UNVERIFIED) | any | px | Origin of the pattern; moving it offsets the grid. |
  | 2 | Size From (大小依据) | `ADBE Grid-0002` (obs.) | Popup | Corner Point | 1 = Corner Point (边角点), 2 = Width Slider (宽度滑块), 3 = Width & Height Sliders (宽度和高度滑块) | — | |
  | 3 | Corner (边角) | `ADBE Grid-0003` (obs.) | Point | Anchor + (W/10, H/10)?? (UNVERIFIED) | any | px | Used only with Corner Point: cell = |Corner − Anchor| per axis. |
  | 4 | Width (宽度) | `ADBE Grid-0004` (obs.) | Float slider | 50? (UNVERIFIED) | slider 0-500? (UNVERIFIED) | px | Width Slider: cell = Width × Width. |
  | 5 | Height (高度) | `ADBE Grid-0005` (obs.) | Float slider | 50? (UNVERIFIED) | as Width | px | Only with Width & Height Sliders. |
  | 6 | Border (边界) | `ADBE Grid-0006` (obs.) | Float slider | 3? (UNVERIFIED) | slider 0-100? (UNVERIFIED) | px | Line thickness; 0 = no grid. |
  | — | Feather (羽化) | `ADBE Grid-0007` (obs.) | (group) | — | — | — | End marker `-0010` (obs.). |
  | 7 | Width (宽度) | `ADBE Grid-0008` (obs.) | Float slider | 0 | ≥ 0 | px | Horizontal softness of line edges. |
  | 8 | Height (高度) | `ADBE Grid-0009` (obs.) | Float slider | 0 | ≥ 0 | px | Vertical softness. |
  | 9 | Invert Grid (反转网格) | `ADBE Grid-0011` (obs.) | Checkbox | Off | — | — | Swaps transparent and opaque areas. |
  | 10 | Color (颜色) | `ADBE Grid-0012` (obs.) | Color | white | any | RGB | |
  | 11 | Opacity (不透明度) | `ADBE Grid-0013` (obs.) | Float slider | 100% | 0-100% | Percent (0-100) | |
  | 12 | Blending Mode (混合模式) | `ADBE Grid-0014` (obs.) | Popup | None | None + blend modes | — | |

  ```
  (cw, ch) = SizeFrom==Corner ? abs(Corner - Anchor) : SizeFrom==Width ? (Width, Width) : (Width, Height)
  q = p - Anchor
  fx = mod(q.x, cw); dx = min(fx, cw - fx)          // distance to nearest vertical line
  fy = mod(q.y, ch); dy = min(fy, ch - fy)
  // lines are centred on cell boundaries, total thickness Border (UNVERIFIED: centred vs inside the cell)
  ax = coverage(dx, Border/2, FeatherW) ; ay = coverage(dy, Border/2, FeatherH)
  m  = max(ax, ay) ; if InvertGrid: m = 1 - m
  gen = vec4(Color * m, m) * Opacity
  out = blend(BlendingMode, gen, src)
  coverage(d, half, f) = smoothstep(half + f/2 + 0.5, half - f/2 - 0.5, d)   // AA'd edge; UNVERIFIED profile
  ```
  - Help note: anti-aliasing of the borders can make the visible line thickness vary.
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Size From labels and Feather sub-params share names with Checkerboard. The second "Width/Height" pair under Feather is a different parameter (index 8/9) from the cell-size Width/Height (4/5); expressions must use `effect("Grid")(n)` indices or matchNames.

### AE-FX-604 · Checkerboard (棋盘)

- **What it does:** Draws a checkerboard in which alternate rectangles are colored and transparent.
- **Exact semantics:**
  - matchName `ADBE Checkerboard`; category Generate; 8 bpc; GPU —. Lottie `ty: 5`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Anchor (锚点) | `ADBE Checkerboard-0001` | Point | layer centre (UNVERIFIED) | any | px | |
  | 2 | Size From (大小依据) | `-0002` | Popup | Corner Point | Corner Point / Width Slider / Width & Height Sliders (as Grid) | — | |
  | 3 | Corner (边角) | `-0003` | Point | (UNVERIFIED) | any | px | |
  | 4 | Width (宽度) | `-0004` | Float slider | (UNVERIFIED) | (UNVERIFIED) | px | |
  | 5 | Height (高度) | `-0005` | Float slider | (UNVERIFIED) | (UNVERIFIED) | px | |
  | — | Feather (羽化) | `-0006` / end `-0009` | (group) | — | — | — | |
  | 6 | Width (宽度) | `-0007` | Float slider | 0 | ≥ 0 | px | Edge feather in x. |
  | 7 | Height (高度) | `-0008` | Float slider | 0 | ≥ 0 | px | Edge feather in y. |
  | 8 | Color (颜色) | `-0010` | Color | white | any | RGB | Colour of the non-transparent squares. |
  | 9 | Opacity (不透明度) | `-0011` | Float slider | 100% | 0-100% | Percent (0-100) | |
  | 10 | Blending Mode (混合模式) | `-0012` | Popup | None | None + blend modes | — | |

  ```
  (cw, ch) as Grid
  i = floor((p.x - Anchor.x)/cw) ; j = floor((p.y - Anchor.y)/ch)
  on = ((i + j) mod 2 == 0)                 // UNVERIFIED which parity holds the colour at the anchor cell
  m  = feathered(on, distance to cell edges, FeatherW, FeatherH)
  gen = vec4(Color*m, m) * Opacity ; out = blend(BlendingMode, gen, src)
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** All param matchNames here are derived (no export observed) — UNVERIFIED. Checkerboard has no Border and no Invert parameter.

### AE-FX-605 · Circle (圆形)

- **What it does:** Draws a solid disk or ring with optional feathering.
- **Exact semantics:**
  - matchName `ADBE Circle`; category Generate; 8 bpc; GPU —. Lottie `ty: 5`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Center (中心) | `ADBE Circle-0001` (obs.) | Point | layer centre | any | px | |
  | 2 | Radius (半径) | `-0002` (obs.) | Float slider | 75 (UNVERIFIED) | slider 0-? valid 0-30000 (UNVERIFIED) | px | |
  | 3 | Edge (边缘) | `-0003` (obs.) | Popup | None | 1 None (无), 2 Edge Radius (边缘半径), 3 Thickness (粗细), 4 Thickness * Radius (粗细 * 半径), 5 Thickness & Feather * Radius (粗细和羽化 * 半径) | — | |
  | 4 | Edge Radius / Thickness (边缘半径 / 粗细) | `-0004` (obs.) | Float slider | 10? (UNVERIFIED) | ≥ 0 | px or factor | Label changes with Edge; greyed when Edge = None. |
  | — | Feather (羽化) | `-0005` / end `-0008` (obs.) | (group) | — | — | — | |
  | 5 | Feather Outer Edge (羽化外侧边缘) | `-0006` (obs.) | Float slider | 0 | ≥ 0 | px | |
  | 6 | Feather Inner Edge (羽化内侧边缘) | `-0007` (obs.) | Float slider | 0 | ≥ 0 | px | Rings only. |
  | 7 | Invert Circle (反转圆形) | `-0009` (obs.) | Checkbox | Off | — | — | Inverts the matte. |
  | 8 | Color (颜色) | `-0010` (obs.) | Color | white | any | RGB | |
  | 9 | Opacity (不透明度) | `-0011` (obs.) | Float slider | 100% | 0-100% | Percent (0-100) | |
  | 10 | Blending Mode (混合模式) | `-0012` (obs.) | Popup | None | None + blend modes | — | |

  ```
  r = length(p - Center); R = Radius; E = EdgeParam
  switch Edge:
    None:                    rin = -inf ; rout = R
    Edge Radius:             rin = min(R, E) ; rout = max(R, E)   // ring between the two radii
    Thickness:               rin = R - E/2 ; rout = R + E/2       // UNVERIFIED: centred on R vs inside R
    Thickness*Radius:        rin = R - E*R/2 ; rout = R + E*R/2   // UNVERIFIED (E is a factor)
    Thickness&Feather*Radius:rin/rout as above; featherOut = FeatherOuter*R, featherIn = FeatherInner*R
  m_out = 1 - smoothstep(rout - fo/2, rout + fo/2, r)  (fo = feather outer, AA of 1px when 0)
  m_in  = smoothstep(rin - fi/2, rin + fi/2, r)
  m = m_out * m_in ; if Invert: m = 1 - m
  gen = vec4(Color*m, m) * Opacity ; out = blend(BlendingMode, gen, src)
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Exported Lottie (zh-CN AE) confirms the localized names 中心/半径/边缘/边缘半径/羽化外侧边缘/羽化内侧边缘/反转圆形. Implementation hint: analytic SDF shader, no textures.

### AE-FX-606 · Ellipse (椭圆)

- **What it does:** Draws an elliptical ring with a soft glow between an inside and an outside color.
- **Exact semantics:**
  - matchName `ADBE ELLIPSE`; category Generate; 32 bpc; GPU —. Lottie `ty: 5`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Center (中心) | `ADBE ELLIPSE-0001` | Point | layer centre | any | px | |
  | 2 | Width (宽度) | `-0002` | Float slider | 200 (UNVERIFIED) | ≥ 0 | px | Full ellipse width. |
  | 3 | Height (高度) | `-0003` | Float slider | 200 (UNVERIFIED) | ≥ 0 | px | |
  | 4 | Thickness (厚度) | `-0004` | Float slider | (UNVERIFIED) | ≥ 0 | px | Ring thickness (core = Inside Color). |
  | 5 | Softness (柔和度) | `-0005` | Float slider | (UNVERIFIED) | 0-100% | Percent | Width of the glow falloff toward Outside Color. |
  | 6 | Inside Color (内部颜色) | `-0006` | Color | white | any | RGB | Ring core colour. |
  | 7 | Outside Color (外部颜色) | `-0007` | Color | (UNVERIFIED; a saturated blue/red) | any | RGB | Glow colour. |
  | 8 | Composite On Original (在原始图像上合成) | `-0008` | Checkbox | Off | — | — | Off: ring on transparent; On: ring composited (Normal) over the input. |

  ```
  // UNVERIFIED reconstruction (Beam-like profile across the ring):
  d = signed distance from p to the ellipse outline (Center, Width/2, Height/2)
  core = 1 - smoothstep(Thickness/2 - 0.5, Thickness/2 + 0.5, |d|)
  glow = 1 - smoothstep(Thickness/2, Thickness/2 + Softness_px, |d|)
  rgb  = lerp(OutsideColor, InsideColor, core) ; a = max(core, glow)
  out  = CompositeOnOriginal ? (vec4(rgb*a, a) over src) : vec4(rgb*a, a)
  ```
- **Priority:** P2 (rarely used; Shape layer ellipse replaces it).
- **Owner:** bae-effects
- **Notes:** param matchNames derived (UNVERIFIED). Help gives no parameter descriptions; the inside/outside two-colour model is shared with Beam and Audio Spectrum/Waveform.

### AE-FX-607 · Beam (光束)

- **What it does:** Renders a moving laser-like beam segment between a start and end point with a core color and a glow color.
- **Exact semantics:**
  - matchName `ADBE Laser`; category Generate; 32 bpc; GPU —. Lottie `ty: 5`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Start Point (起始点) | `ADBE Laser-0001` | Point | (UNVERIFIED) | any | px | |
  | 2 | End Point (结束点) | `-0002` | Point | (UNVERIFIED) | any | px | |
  | 3 | Length (长度) | `-0003` | Float slider | 30% (UNVERIFIED) | 0-100% | Percent | Visible beam length as % of the path; reaches max when Time = 50% at Length 100%. |
  | 4 | Time (时间) | `-0004` | Float slider | 0% (UNVERIFIED) | 0-100% | Percent | Travel progress from start to end; animate it to "shoot". |
  | 5 | Start Thickness (起始厚度) | `-0005` | Float slider | 8 (UNVERIFIED) | 0-? | px | |
  | 6 | Ending Thickness (结束厚度) | `-0006` | Float slider | 8 (UNVERIFIED) | 0-? | px | |
  | 7 | Softness (柔和度) | `-0007` | Float slider | 50% (UNVERIFIED) | 0-100% | Percent | Glow falloff. |
  | 8 | Inside Color (内部颜色) | `-0008` | Color | white | any | RGB | |
  | 9 | Outside Color (外部颜色) | `-0009` | Color | (UNVERIFIED) | any | RGB | |
  | 10 | 3D Perspective (3D 透视) | `-0010` | Checkbox | On (UNVERIFIED) | — | — | Thickness varies with start/end thickness as if receding when Time is animated. |
  | 11 | Composite On Original (在原始图像上合成) | `-0011` | Checkbox | Off | — | — | |

  ```
  // UNVERIFIED reconstruction. u in [0,1] along Start->End.
  head = Time ; tail = Time - Length * f(Time)   // help: Length 100% gives max visible length at Time 50%
  visible segment = [clamp(tail,0,1), clamp(head,0,1)]
  thickness(u) = 3DPerspective ? perspective_lerp(StartT, EndT, u) : lerp(StartT, EndT, u)
  render capsule along the visible segment with core = Inside Color, glow (Softness) = Outside Color
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Help: looks best with motion blur on and shutter angle 360° (compositing-3d-color.md, Motion blur rendering). param matchNames derived (UNVERIFIED).

### AE-FX-608 · Lens Flare (镜头光晕)

- **What it does:** Simulates the flare of a bright light shining into one of three lens types.
- **Exact semantics:**
  - matchName `ADBE Lens Flare`; category Generate; 8 bpc; GPU —. Lottie `ty: 5`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Flare Center (光晕中心) | `ADBE Lens Flare-0001` | Point | (UNVERIFIED; upper-left area of the layer) | any | px | Optical axis is the layer centre; ghost elements line up along centre↔Flare Center. |
  | 2 | Flare Brightness (光晕亮度) | `-0002` | Float slider | 100% | 10-300% (UNVERIFIED) | Percent | |
  | 3 | Lens Type (镜头类型) | `-0003` | Popup | 50-300mm Zoom | 1 = 50-300mm Zoom (50-300 毫米变焦), 2 = 35mm Prime (35 毫米定焦), 3 = 105mm Prime (105 毫米定焦) | — | Each type is a fixed set of ghost/halo/streak elements. |
  | 4 | Blend With Original (与原始图像混合) | `-0004` | Float slider | 0% | 0-100% | Percent | |

  ```
  // Composite is additive ("screen-like") over the input; alpha unchanged where src opaque (UNVERIFIED)
  axis = LayerCentre - FlareCenter
  for each element e in LensTable[LensType]:          // element tables are not public: BAE authors its own
      pos = FlareCenter + axis * e.t ; draw e.shape(size*Brightness, colour, falloff) additively
  out = lerp(flare_add(src), src, Blend)
  ```
- **Priority:** P2 (AE-look fidelity requires Adobe's element tables, which are not documented).
- **Owner:** bae-effects
- **Notes:** BAE decision: ship three BAE-designed lens presets with the same names and parameter semantics; do not copy Adobe's flare artwork. 8 bpc only in AE, so values clip at 1.0 there; BAE may render float but must clamp when emulating.

### AE-FX-609 · Stroke (描边)

- **What it does:** Paints a brush stroke along one mask path or all mask paths, with animatable start/end for write-on reveals.
- **Exact semantics:**
  - matchName `ADBE Stroke`; category Generate; 8 bpc; GPU —. Lottie effect `ty: 22`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Path (路径) | `ADBE Stroke-0001` (obs.) | Mask reference popup | None | None or a mask | — | Lottie `ty: 10`. |
  | 2 | All Masks (所有蒙版) | `-0010` (obs.) | Checkbox | Off | — | — | Strokes every mask. |
  | 3 | Stroke Sequentially (顺序描边) | `-0011` (obs.) | Checkbox | On | — | — | With All Masks: Start/End run across masks one after another (in mask order) instead of on each mask simultaneously. |
  | 4 | Color (颜色) | `-0002` (obs.) | Color | white | any | RGB | |
  | 5 | Brush Size (画笔大小) | `-0003` (obs.) | Float slider | 2.0 | slider 0-50, valid 0-1000 (UNVERIFIED) | px | Diameter. |
  | 6 | Brush Hardness (画笔硬度) | `-0004` (obs.) | Float slider | 75% | 0-100% | Percent (0-1) | Lottie `k: 0.75`. |
  | 7 | Opacity (不透明度) | `-0005` (obs.) | Float slider | 100% | 0-100% | Percent (0-1) | |
  | 8 | Start (起始) | `-0008` (obs.) | Float slider | 0% | 0-100% | Percent (0-100) | |
  | 9 | End (结束) | `-0009` (obs.) | Float slider | 100% | 0-100% | Percent (0-100) | |
  | 10 | Spacing (间距) | `-0006` (obs.) | Float slider | 15% | 0-100% (UNVERIFIED) | Percent (0-100) | Distance between brush dabs as % of Brush Size. |
  | 11 | Paint Style (绘画样式) | `-0007` (obs.) | Popup | On Original Image | 1 On Original Image (在原始图像上), 2 On Transparent (在透明背景上), 3 Reveal Original Image (显示原始图像) | — | |

  ```
  paths = AllMasks ? all masks (in mask order) : [Path]
  L_total = sum(arc_length(path_i))
  // Start/End are fractions of each path (or of L_total when Sequentially)
  for each path segment [s0, s1] covered by [min(Start,End), max(Start,End)]:
     place round dabs every max(1, Spacing/100 * BrushSize) px along arc length
     dab profile: radius = BrushSize/2, hard core radius = Hardness * radius, linear/gaussian falloff to edge (UNVERIFIED profile)
  S = coverage (dabs combined with max, not additive — UNVERIFIED)
  On Original Image:     out = vec4(Color*S*Opacity, S*Opacity) over src
  On Transparent:        out = vec4(Color*S*Opacity, S*Opacity)
  Reveal Original Image: out = src * (S*Opacity)        // stroke acts as matte for the layer
  ```
  - Start > End is allowed (reverses the visible range). Mask modes are irrelevant to the stroke; masks with mode other than None still cut the layer.
- **Priority:** P1 (classic write-on trick; Lottie-exportable).
- **Owner:** bae-effects (+ bae-vector)
- **Notes:** lottie-web `SVGStrokeEffect` implements this (uses dasharray for Start/End, `Paint Style` 3 = reveal via mask). Open masks are stroked, closed masks include the closing segment. Output is clipped to the layer bounds (no bounds growth). Implementation hint: reuse the bae-vector path flattener and a dab-accumulation compute pass, or approximate with a Vello stroke when Spacing ≤ 25% and hardness = 100%.

### AE-FX-610 · Write-on (写入)

- **What it does:** Paints a stroke over time by leaving brush dabs along the animated Brush Position path, simulating hand-writing.
- **Exact semantics:**
  - matchName `ADBE Write-on`; category Generate; 8 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Brush Position (画笔位置) | `ADBE Write-on-0001` | Point | (UNVERIFIED) | any | px | Animate (or link by expression to a Motion Sketch layer). |
  | 2 | Color (颜色) | `-0002` | Color | (UNVERIFIED) | any | RGB | |
  | 3 | Brush Size (画笔大小) | `-0003` | Float slider | (UNVERIFIED) | (UNVERIFIED) | px | |
  | 4 | Brush Hardness (画笔硬度) | `-0004` | Float slider | (UNVERIFIED) | 0-100% | Percent | |
  | 5 | Brush Opacity (画笔不透明度) | `-0005` | Float slider | 100% | 0-100% | Percent | |
  | 6 | Stroke Length (secs) (描边长度(秒)) | `-0006` | Float slider | 0 = permanent (UNVERIFIED) | ≥ 0 | s | Each dab stays visible this long; 0 = forever. |
  | 7 | Brush Spacing (secs) (画笔间距(秒)) | `-0007` | Float slider | (UNVERIFIED) | > 0 | s | Time interval between dabs. |
  | 8 | Paint Time Properties (绘画时间属性) | `-0008` | Popup | None | None / Opacity / Color (UNVERIFIED) | — | Whether paint props are sampled per dab over time. |
  | 9 | Brush Time Properties (画笔时间属性) | `-0009` | Popup | None | None / Size / Hardness / Size & Hardness (UNVERIFIED) | — | |
  | 10 | Paint Style (绘画样式) | `-0010` | Popup | On Original Image | On Original Image (在原始图像上) / On Transparent (在透明背景上) / Reveal Original Image (显示原始图像) | — | |

  ```
  // History-dependent: render(t) = union of dabs at times tk = layerIn + k*Spacing, tk <= t
  for each tk: pos = BrushPosition(tk); if StrokeLength==0 or t - tk <= StrokeLength: draw dab(pos, size, hardness, colour)
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-anim)
- **Notes:** Output at time t depends on parameter values at all earlier dab times (a "time-sampling" effect); BAE must evaluate the Brush Position property at many times per frame (cache the sampled path). Shape layers with Trim Paths are the modern replacement (shapes.md, Trim Paths).

### AE-FX-611 · Scribble (涂抹)

- **What it does:** Fills or strokes mask regions with an animated hand-drawn zig-zag scribble line.
- **Exact semantics:**
  - matchName `ADBE Scribble Fill`; category Generate; 8 bpc; GPU —.
  - Parameters in AE order (defaults UNVERIFIED unless shown): Scribble (涂抹) {Single Mask, All Masks, All Masks Using Modes}; Mask (蒙版); Fill Type (填充类型) {Inside, Center Edge, Inside Edge, Outside Edge, Left Edge, Right Edge}; Edge Options (边缘选项) group: Edge Width (边缘宽度), End Cap (端点) {Round, Butt, Projecting}, Join (连接) {Round, Miter, Bevel}, Miter Limit (斜接限制); Start/End Apply To (起始/结束应用于) (UNVERIFIED placement); Color (颜色); Opacity (不透明度); Angle (角度); Stroke Width (描边宽度) (slider to 50); Stroke Options (描边选项) group: Curviness (弯曲度), Curviness Variation (弯曲度变化), Spacing (间距), Spacing Variation (间距变化), Path Overlap (路径重叠), Path Overlap Variation (路径重叠变化); Start (起始); End (结束); Fill Paths Sequentially (顺序填充路径); Wiggle Type (摆动类型) {Static, Jumpy, Smooth}; Wiggles/Second (摆动/秒); Random Seed (随机植入); Composite (合成) {On Original Image, On Transparent, Reveal Original Image}.
  ```
  generate parallel hatch lines at Angle across the fill region (Spacing ± variation),
  connect consecutive lines with curved turns (Curviness ± variation), overshoot by Path Overlap,
  re-randomise per Wiggle Type (Static: once; Jumpy: Wiggles/Second times per second; Smooth: interpolated),
  stroke with Stroke Width, trim by Start/End, composite per Composite.
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-vector)
- **Notes:** Chinese names of sub-parameters are best guesses（译名待核）. Deterministic per Random Seed.

### AE-FX-612 · Radio Waves (无线电波)

- **What it does:** Emits expanding wave outlines (polygon, image contour or mask shape) from a producer point.
- **Exact semantics:**
  - matchName `APC Radio Waves`; category Generate; 8 bpc; GPU —.
  - Parameters in AE order: Producer Point (产生点); Parameters Are Set At (参数设置为) {Birth, Each Frame}; Render Quality (渲染品质); Wave Type (波浪类型) {Polygon, Image Contours, Mask}; Polygon group: Sides (边), Curve Size (曲线大小), Curvyness (曲线弯曲度), Star (星形), Star Depth (星深度); Image Contour group: Source Layer, Source Center, Value Channel, Invert Input, Value Threshold, Pre-Blur, Tolerance, Contour; Mask (蒙版); Wave Motion group: Frequency (频率, waves/s), Expansion (扩展), Orientation (方向), Direction (方向), Velocity (速率), Spin (旋转), Lifespan (sec) (寿命(秒)), Reflection (反射); Stroke group: Profile (配置文件) {Square, Sine, …}, Color (颜色), Opacity (不透明度), Fade-in Time (淡入时间), Fade-out Time (淡出时间), Start Width (开始宽度), End Width (末端宽度). Defaults UNVERIFIED.
  ```
  waves born at t_k = k / Frequency; wave age a = t - t_k (alive while a < Lifespan)
  radius(a) = Expansion * a ; offset along Direction by Velocity * a ; rotation = Orientation + Spin * a
  Parameters Are Set At = Birth -> each wave keeps the parameter values sampled at t_k
  stroke width lerp(StartWidth, EndWidth, a/Lifespan); opacity fades in/out over Fade-in/Fade-out time
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Sub-parameter Chinese names（译名待核）. History-independent (each wave is a closed-form function of its birth time) — cheap to scrub.

### AE-FX-613 · Audio Spectrum (音频频谱)

- **What it does:** Draws the frequency spectrum of an audio layer as bars, lines or dots along a line or mask path.
- **Exact semantics:**
  - matchName `ADBE AudSpect`; category Generate; 32 bpc; GPU —.
  - Parameters in AE order: Audio Layer (音频层); Start Point (起始点), End Point (结束点) (used when Path = None); Path (路径); Use Polar Path (使用极坐标路径); Start Frequency (起始频率), End Frequency (结束频率) (Hz); Frequency Bands (频段); Maximum Height (最大高度); Audio Duration (音频持续时间) (ms); Audio Offset (音频偏移) (ms); Thickness (厚度); Softness (柔和度); Inside Color (内部颜色), Outside Color (外部颜色); Blend Overlapping Colors (混合叠加颜色); Hue Interpolation (色相插值); Dynamic Hue Phase (动态色相); Color Symmetry (颜色对称); Display Options (显示选项) {Digital, Analog Lines, Analog Dots}; Side Options (面选项) {Side A, Side B, Side A & B}; Duration Averaging (持续时间平均化); Composite On Original (在原始图像上合成). Defaults UNVERIFIED.
  ```
  window = audio samples of AudioLayer source in [t + Offset - Duration/2, t + Offset + Duration/2]
  spectrum = |FFT(window)| ; bands = log/linear bins between StartFreq and EndFreq (UNVERIFIED spacing)
  for band i: h_i = MaxHeight * magnitude_i ; draw at u_i = i/(Bands-1) along path, perpendicular, per Side Options
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-media)
- **Notes:** Help: uses the audio **source** footage, ignoring time remapping, effects, stretch and levels — precompose to include them. Masks with modes other than None clip the output. Sub-parameter Chinese names（译名待核）.

### AE-FX-614 · Audio Waveform (音频波形)

- **What it does:** Draws the audio waveform of an audio layer along a line or mask path.
- **Exact semantics:**
  - matchName `ADBE AudWave`; category Generate; 32 bpc; GPU —.
  - Parameters in AE order: Audio Layer (音频层); Start Point, End Point; Path (路径); Displayed Samples (显示的范例); Maximum Height (最大高度); Audio Duration (音频持续时间) (ms); Audio Offset (音频偏移) (ms); Thickness (厚度); Softness (柔和度); Inside Color, Outside Color (内部颜色/外部颜色); Waveform Options (波形选项) {Mono, Left, Right} (non-stereo layers play as Mono); Display Options (显示选项) {Digital, Analog Lines, Analog Dots}; Composite On Original (在原始图像上合成). Defaults UNVERIFIED.
  ```
  samples = DisplayedSamples points resampled from the source audio window around t + Offset (Duration long)
  y_i = MaxHeight * sample_i ; place along path/line ; draw per Display Options with Thickness/Softness
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-media)
- **Notes:** Same source-audio caveat as Audio Spectrum. Sub-parameter Chinese names（译名待核）.

### AE-FX-615 · Eyedropper Fill (吸管填充)

- **What it does:** Samples the average color in a circle of the layer and fills the layer with it.
- **Exact semantics:**
  - matchName `ADBE Eyedropper Fill`; category Generate; 8 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Sample Point (采样点) | `ADBE Eyedropper Fill-0001` | Point | layer centre (UNVERIFIED) | any | px | |
  | 2 | Sample Radius (采样半径) | `-0002` | Float slider | (UNVERIFIED) | ≥ 0 | px | |
  | 3 | Average Pixel Color (平均像素颜色) | `-0003` | Popup | (UNVERIFIED) | Skip Empty / All / All Premultiplied / Including Alpha | — | Skip Empty ignores transparent pixels; All includes their RGB; All Premultiplied averages premultiplied; Including Alpha also averages alpha. |
  | 4 | Maintain Original Alpha (保持原始 Alpha) | `-0004` | Checkbox | Off | — | — | On: original alpha stencils the sampled colour. |
  | 5 | Blend With Original (与原始图像混合) | `-0005` | Float slider | 0% | 0-100% | Percent | |
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Implementation hint: a single reduction pass (compute shader) over the sample disc, then a fill pass. Chinese parameter names（译名待核）.

### AE-FX-616 · Other Generate effects catalog (其他生成效果)

- **What it does:** Lists the remaining Generate-category effects that BAE reserves by matchName but does not specify in detail for M1.
- **Exact semantics:**

  | Effect (中文) | matchName | bpc | GPU | Summary |
  |---|---|---|---|---|
  | Advanced Lightning (高级闪电) | `ADBE Lightning 2` | 8 | — | Branching electric arcs between Origin and Direction (types Direction, Strike, Breaking, Bouncy, Omni, Anywhere, Vertical, Two-Way Strike); Conductivity State animates the path; Alpha Obstacle lets layer alpha deflect it. |
  | Cell Pattern (单元格图案) | `ADBE Cell Pattern` | 8 | — | Worley/Voronoi cellular noise (Bubbles, Crystals, Plates, Static Plates, Crystallize, Pillow, Mixed Crystals, Tubular, and HQ variants); Evolution with cycle, like Fractal Noise. |
  | Fractal (分形) | `ADBE Fractal` | 16 | — | Mandelbrot/Julia set renderer with color palettes and an in-viewer zoom tool. |
  | Paint Bucket (油漆桶) | `ADBE Paint Bucket` | 8 | — | Flood fill from a Fill Point by color similarity (Tolerance, Stroke options, Feather, Color, Opacity, Blending Mode). |
  | Vegas (勾画) | `APC Vegas` | 8 | — | Moving dashes ("chasing lights") along image contours or a mask (Segments, Length, Rotation, Width, Hardness, Start/Mid/End opacity). |
  | CC Glue Gun (CC Glue Gun) | `CC Glue Gun` | 32 | — | Glue-like strokes following a brush position. |
  | CC Light Burst 2.5 (CC Light Burst 2.5) | `CC Light Burst 2.5` | 32 | — | Radial light-ray streaks from a centre ("god rays" of the layer itself). |
  | CC Light Rays (CC Light Rays) | `CC Light Rays` | 32 | — | Light rays emanating from a point through the layer's bright areas. |
  | CC Light Sweep (CC Light Sweep) | `CC Light Sweep` | 32 | — | A moving linear highlight band (Center, Direction, Shape, Width, Sweep Intensity, Edge Intensity/Thickness, Light Color, Light Reception). |
  | CC Threads (CC Threads) | `CS Threads` | 32 | — | Woven-thread pattern overlay. Note matchName prefix `CS`. |
- **Priority:** P2
- **Owner:** bae-effects (+ bae-plugin)
- **Notes:** BAE decision: projects containing these load as "unsupported effect" placeholders that keep their parameter data and render pass-through (effects.md, Effect framework, and AE import), until implemented. The CC (Cycore) effects are third-party code bundled with AE; BAE must re-implement from behaviour, not code.

## Transition effects

Common rule (applies to AE-FX-617…621): every Transition effect except Iris Wipe has **Transition Completion (过渡完成)**, a Percent (0-100) float slider, default 0%, range 0-100%. 0% = layer untouched; 100% = layer fully transparent so underlying layers show. Transitions only multiply the layer's alpha (premultiplied RGB scaled with it); they never change colour and never expand bounds. Quality switch: at Draft, edges are pixel-snapped/not anti-aliased; at Best, sub-pixel and anti-aliased (compositing-3d-color.md, Quality and sampling).

### AE-FX-617 · Linear Wipe (线性擦除)

- **What it does:** Wipes the layer away along a straight edge moving in a chosen direction.
- **Exact semantics:**
  - matchName `ADBE Linear Wipe`; category Transition; 32 bpc (since CS6); GPU —. Lottie `ty: 5` (Skottie supports it).

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Transition Completion (过渡完成) | `ADBE Linear Wipe-0001` (obs.) | Float slider | 0% | 0-100% | Percent (0-100) | |
  | 2 | Wipe Angle (擦除角度) | `-0002` (obs.) | Angle | 0x+90.0° | any | degrees | Direction of travel; 90° travels left→right (left side disappears first); 0° travels bottom→top (UNVERIFIED for 0°, follows the clockwise-from-up convention). |
  | 3 | Feather (羽化) | `-0003` (obs.) | Float slider | 0 | slider 0-?, valid 0-32000? (UNVERIFIED) | px | Width of the soft edge. |

  ```
  // Skottie reconstruction (matches AE visually; exactness at the edge UNVERIFIED)
  c = clamp(Completion/100, 0, 1); f = max(Feather, 0)
  if c <= 0: return src ; if c >= 1: return 0
  θ = radians(90 - WipeAngle) ; v = (cos θ, -sin θ)            // unit vector in y-down layer space
  diag = (copysign(W, cos θ), copysign(H, sin θ))
  len  = |dot(diag, (cos θ, sin θ))|                           // extent of the layer along v
  G    = len + 2f
  s    = (dot(p - (W/2, H/2), v) + G/2) / G                     // 0 at the start side, 1 at the far side
  t0   = c * (len + f) / G
  m    = clamp((s - t0) / (f / G), 0, 1)        // f == 0 -> hard step at s = t0 (AA'd at Best quality)
  out  = src * m
  ```
  - The sweep range is padded by the feather so that the soft edge is entirely off the layer at 0% and 100%.
- **Priority:** P0 (most used transition; also used for reveals in motion graphics).
- **Owner:** bae-effects
- **Notes:** Help: Draft = not anti-aliased, Best = anti-aliased and smooth feather. Skottie `LinearWipeEffect.cpp` is the reference implementation above. Implementation hint: single analytic shader; no textures.

### AE-FX-618 · Radial Wipe (径向擦除)

- **What it does:** Reveals underlying layers with a wipe that sweeps around a centre point like a clock hand.
- **Exact semantics:**
  - matchName `ADBE Radial Wipe`; category Transition; 32 bpc (since CS6); GPU —. Lottie effect `ty: 26`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Transition Completion (过渡完成) | `ADBE Radial Wipe-0001` (obs.) | Float slider | 0% | 0-100% | Percent (0-100) | |
  | 2 | Start Angle (起始角度) | `-0002` (obs.) | Angle | 0x+0.0° | any | degrees | 0° = 12 o'clock (top). |
  | 3 | Wipe Center (擦除中心) | `-0003` (obs.) | Point | layer centre | any | px | |
  | 4 | Wipe (擦除) | `-0004` (obs.) | Popup | Clockwise | 1 Clockwise (顺时针), 2 Counterclockwise (逆时针), 3 Both (两者兼有) | — | Both: wipes symmetrically in both directions from Start Angle. |
  | 5 | Feather (羽化) | `-0005` (obs.) | Float slider | 0 | ≥ 0 (UNVERIFIED max) | px | Blurs the wipe edge. |

  ```
  c = Completion/100 ; if c <= 0 return src ; if c >= 1 return 0
  φ = clockwise angle of (p - WipeCenter) measured from straight up, in [0, 360)   // y-down space
  a = mod(φ - StartAngle, 360)
  Clockwise:        hidden = a < 360*c
  Counterclockwise: hidden = (360 - a) < 360*c        // i.e. a > 360*(1-c)
  Both:             hidden = min(a, 360 - a) < 180*c
  m = hidden ? 0 : 1 ; if Feather > 0: m = blur(m, Feather)   // UNVERIFIED: AE feathers angularly vs spatially
  out = src * m
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Skottie maps Wipe 1/2/3 to sweep offsets 0/−360/−180 (`wipeAlignment`), and leaves feather unimplemented. lottie-web: `ty: 26` is recognised as Radial Wipe but rendered only on the canvas/html renderers partially (UNVERIFIED).

### AE-FX-619 · Venetian Blinds (百叶窗)

- **What it does:** Reveals underlying layers through repeating strips that grow across each period.
- **Exact semantics:**
  - matchName `ADBE Venetian Blinds`; category Transition; 32 bpc; GPU —. Lottie `ty: 5` (Skottie supports it).

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Transition Completion (过渡完成) | `ADBE Venetian Blinds-0001` (obs.) | Float slider | 0% | 0-100% | Percent (0-100) | |
  | 2 | Direction (方向) | `-0002` (obs.) | Angle | 0x+0.0° | any | degrees | Orientation of the strip progression. |
  | 3 | Width (宽度) | `-0003` (obs.) | Float slider | 10 (UNVERIFIED) | slider 1-?, valid ≥ 1 (Skottie clamps to ≥ 1) | px | Strip period. |
  | 4 | Feather (羽化) | `-0004` (obs.) | Float slider | 0 | ≥ 0 | px | |

  ```
  c = Completion/100 ; P = max(1, Width) ; θ = radians(-Direction)
  u = fract(dot(p - (W/2, H/2), (cos θ, sin θ)) / P)     // position inside the strip period, 0..1 (UNVERIFIED phase origin)
  hidden = u < c                                         // each strip's hidden part grows from one side
  feather: soften the two transitions of each period over Feather px (Skottie: 3*Feather, min 0.5 px)
  out = src * (hidden ? 0 : 1)  (with soft ramps)
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Help: Draft animates strips with pixel precision, Best with sub-pixel precision. UNVERIFIED: whether Direction 0° yields horizontal or vertical strips in AE (Skottie varies the mask along (cos(−Direction), sin(−Direction)), giving vertical strips at 0°) — confirm with a reference render before M1.

### AE-FX-620 · Block Dissolve (块溶解)

- **What it does:** Makes the layer disappear in randomly ordered rectangular blocks.
- **Exact semantics:**
  - matchName `ADBE Block Dissolve`; category Transition; 16 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Transition Completion (过渡完成) | `ADBE Block Dissolve-0001` (obs.) | Float slider | 0% | 0-100% | Percent (0-100) | |
  | 2 | Block Width (块宽度) | `-0002` (obs.) | Float slider | 1.0 (UNVERIFIED) | slider 1-?, valid ≥ 1 (UNVERIFIED) | px | |
  | 3 | Block Height (块高度) | `-0003` (obs.) | Float slider | 1.0 (UNVERIFIED) | as width | px | |
  | 4 | Feather (羽化) | `-0004` (obs.) | Float slider | 0 | ≥ 0 | px | Softens block edges. |
  | 5 | Soft Edges (Best Quality) (柔化边缘(最佳品质)) | `-0005` (obs.) | Checkbox | On | — | — | Only affects Best quality: sub-pixel block placement with soft edges. |

  ```
  cell = floor(p / (BlockWidth, BlockHeight))             // grid anchored at layer origin (UNVERIFIED)
  r    = hash2(cell)                                      // fixed pseudo-random value in [0,1); no seed parameter
  m    = r >= c ? 1 : 0                                   // blocks vanish as Completion passes their r
  if Feather > 0 or (Best && SoftEdges): soften block borders (UNVERIFIED profile)
  out  = src * m
  ```
  - Randomness: the block order is a deterministic function of block position and size only (no Random Seed parameter; the pattern does not change over time). AE's PRNG is not documented, so pixel-exact order cannot match AE.
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** BAE decision: use a documented stable integer hash (e.g. PCG on (cell.x, cell.y)) so renders are deterministic across GPUs and versions; document that the order differs from AE. Help: Draft = pixel-precise sharp blocks; Best = sub-pixel soft blocks.

### AE-FX-621 · Gradient Wipe (渐变擦除)

- **What it does:** Makes pixels disappear in the order of the luminance of a gradient layer (dark first), with optional softness.
- **Exact semantics:**
  - matchName `ADBE Gradient Wipe`; category Transition; 16 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Transition Completion (过渡完成) | `ADBE Gradient Wipe-0001` (obs.) | Float slider | 0% | 0-100% | Percent (0-100) | |
  | 2 | Transition Softness (过渡柔和度) | `-0002` (obs.) | Float slider | 0% | 0-100% | Percent (0-100) | 0% = binary; > 0 = pixels pass through partial transparency. |
  | 3 | Gradient Layer (渐变图层) | `-0003` (obs.) | Layer | this layer (UNVERIFIED) | any layer in the comp | — | Still or moving. |
  | 4 | Gradient Placement (渐变位置) | `-0004` (obs.) | Popup | Stretch Gradient to Fit (UNVERIFIED) | 1 Tile Gradient (拼贴渐变), 2 Center Gradient (中心渐变), 3 Stretch Gradient to Fit (伸缩渐变以适合) | — | |
  | 5 | Invert Gradient (反转渐变) | `-0005` (obs.) | Checkbox | Off | — | — | Light pixels disappear first. |

  ```
  g = sample(GradientLayer source after its masks & effects, no transforms,  // effects.md, Layer parameter
             placement: Tile = wrap at layer origin; Center = centred, outside = black (UNVERIFIED); Stretch = scale to this layer's size)
  L = luminance(g.rgb)              // UNVERIFIED weights (Rec.601 0.299/0.587/0.114 assumed); g.a ignored (UNVERIFIED)
  if Invert: L = 1 - L
  c = Completion/100 ; s = Softness/100
  if s == 0: m = L > c ? 1 : 0      // at c=0 everything visible; at c=1 everything gone (L=1 pixels gone at c=1, UNVERIFIED tie rule)
  else:      m = clamp((L - (c*(1+s) - s)) / s, 0, 1)   // UNVERIFIED reconstruction: full range covered at c=0 and c=1
  out = src * m
  ```
  - A pure Gradient Ramp (black left, white right) as gradient layer reveals the underlying layers left to right as Completion increases.
- **Priority:** P1
- **Owner:** bae-effects (+ bae-render)
- **Notes:** The gradient layer must be in the same comp; its visibility switch does not matter (it is usually hidden). Requires rendering another layer as a texture input (render-graph dependency, effects.md, Layer parameter).

### AE-FX-622 · Iris Wipe (光圈擦除)

- **What it does:** Reveals underlying layers through a growing star/polygon-shaped iris; it has no Transition Completion.
- **Exact semantics:**
  - matchName `ADBE IRIS_WIPE`; category Transition; 32 bpc (since CS6); GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Iris Center (光圈中心) | `ADBE IRIS_WIPE-0001` (obs.) | Point | layer centre | any | px | |
  | 2 | Iris Points (光圈点) | `-0002` (obs.) | Float slider | 6 | 6-32 | count | Number of points of the iris polygon/star. |
  | 3 | Outer Radius (外径) | `-0003` (obs.) | Float slider | 0 | ≥ 0 | px | Animate to open the iris. |
  | 4 | Use Inner Radius (使用内径) | `-0004` (obs.) | Checkbox | Off | — | — | Off: regular polygon of Outer Radius; On: star alternating Outer/Inner radius. |
  | 5 | Inner Radius (内径) | `-0005` (obs.) | Float slider | 0 | ≥ 0 | px | |
  | 6 | Rotation (旋转) | `-0006` (obs.) | Angle | 0° | any | degrees | |
  | 7 | Feather (羽化) | `-0007` (obs.) | Float slider | 0 | ≥ 0 | px | |

  ```
  shape = polygon (or star when UseInnerRadius) with N = IrisPoints, radii Outer(/Inner), rotated by Rotation, centred at IrisCenter
  m = 1 - coverage(shape, feathered by Feather)     // the layer is hidden INSIDE the iris, revealing what is beneath
  out = src * m
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** UNVERIFIED: AE hides the inside of the iris (revealing the layer below); confirm direction with a reference render. Chinese parameter names（译名待核）.

### AE-FX-623 · Other Transition effects catalog (其他过渡效果)

- **What it does:** Lists the remaining Transition effects reserved by matchName for later implementation.
- **Exact semantics:**

  | Effect (中文) | matchName | bpc | GPU | Summary |
  |---|---|---|---|---|
  | Card Wipe (卡片擦除) | `APC CardWipeCam` | 8 | — | Splits the layer into a grid of cards that flip to reveal a Back Layer; Rows & Columns, Card Scale, Flip Axis/Direction/Order, Gradient Layer, Timing Randomness, Camera System (Camera Position / Corner Pins / Comp Camera), Lighting, Material, Position/Rotation Jitter. |
  | CC Glass Wipe (CC Glass Wipe) | `CC Glass Wipe` | 16 | — | Wipe driven by a gradient layer with glass-like refraction at the edge. |
  | CC Grid Wipe (CC Grid Wipe) | `CC Grid Wipe` | 32 | — | Grid of diamond/square cells wiping in. |
  | CC Image Wipe (CC Image Wipe) | `CC Image Wipe` | 16 | — | Gradient-wipe using another layer's channel with border softness. |
  | CC Jaws (CC Jaws) | `CC Jaws` | 32 | — | Sawtooth "jaws" edge wipe. |
  | CC Light Wipe (CC Light Wipe) | `CC Light Wipe` | 16 | — | Shape wipe (door/round/square) with a glowing light edge. |
  | CC Line Sweep (CC Line Sweep) | `CS LineSweep` | 32 | — | Sweeping diagonal lines wipe. |
  | CC Radial ScaleWipe (CC Radial ScaleWipe) | `CC Radial ScaleWipe` | 16 | — | Radial wipe that stretches pixels outward from the centre. |
  | CC Scale Wipe (CC Scale Wipe) | `CC Scale Wipe` | 32 | — | Directional pixel-stretch wipe. |
  | CC Twister (CC Twister) | `CC Twister` | 16 | — | Twists the layer around an axis to a Back Layer. |
  | CC WarpoMatic (CC WarpoMatic) | `CC WarpoMatic` | 16 | — | Warp-and-blend transition driven by layer brightness/contrast. |
- **Priority:** P2
- **Owner:** bae-effects (+ bae-plugin)
- **Notes:** Loaded as pass-through placeholders that keep parameters until implemented (see AE-FX-616 Notes).

## Matte effects

### AE-FX-624 · Simple Choker (简单阻塞工具)

- **What it does:** Shrinks (chokes) or grows (spreads) the edge of a layer's alpha matte in small sub-pixel increments.
- **Exact semantics:**
  - matchName `ADBE Simple Choker`; category Matte; 32 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | View (视图) | `ADBE Simple Choker-0001` (obs.) | Popup | Final Output | 1 Final Output (最终输出), 2 Matte (遮罩) | — | Matte = alpha shown as opaque grayscale (white = opaque). |
  | 2 | Choke Matte (阻塞遮罩) | `-0002` (obs.) | Float slider | 0.00 | slider -10…10, valid -100…100 (UNVERIFIED) | px | Positive chokes, negative spreads. |

  ```
  // UNVERIFIED reconstruction: signed-distance offset of the 0.5 alpha iso-contour
  d   = signed_distance_to_alpha_edge(src.a)       // >0 inside, <0 outside, computed with sub-pixel accuracy (e.g. jump flooding on the 0.5 contour)
  a'  = clamp(d - Choke + 0.5, 0, 1)               // shifts the edge by Choke px, 1-px anti-aliased ramp
  a'  = min(a', src.a) if Choke > 0 ; max(a', src.a) if Choke < 0   // keeps interior soft values (UNVERIFIED)
  rgb'= src.rgb_straight * a'                      // colour is unpremultiplied then re-premultiplied
  View == Matte: out = vec4(a', a', a', 1)
  ```
- **Priority:** P1 (standard post-key cleanup).
- **Owner:** bae-effects
- **Notes:** Spreading reveals colour from formerly transparent pixels; AE uses the straight colour of the nearest edge pixel (UNVERIFIED). Implementation hint: jump-flood distance field on GPU, O(log n) passes, independent of Choke size.

### AE-FX-625 · Matte Choker (遮罩阻塞工具)

- **What it does:** Fills holes and cleans edges in a matte by repeated blur-and-threshold stages that choke and spread.
- **Exact semantics:**
  - matchName `ADBE Matte Choker`; category Matte; 16 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Geometric Softness 1 (几何柔和度 1) | `ADBE Matte Choker-0001` (obs.) | Float slider | 4.00 (UNVERIFIED) | slider 0-10, valid 0-? (UNVERIFIED) | px | Blur radius of stage 1. |
  | 2 | Choke 1 (阻塞 1) | `-0002` (obs.) | Float slider | 75 (UNVERIFIED) | -127…127 (UNVERIFIED) | level | Positive chokes, negative spreads. |
  | 3 | Gray Level Softness 1 (灰色阶柔和度 1) | `-0003` (obs.) | Float slider | 10% (UNVERIFIED) | 0-100% | Percent (0-1) | Lottie shows fraction (e.g. 0.01). |
  | 4 | Geometric Softness 2 (几何柔和度 2) | `-0004` (obs.) | Float slider | 0.00 (UNVERIFIED) | as 1 | px | |
  | 5 | Choke 2 (阻塞 2) | `-0005` (obs.) | Float slider | 0 (UNVERIFIED) | as 1 | level | |
  | 6 | Gray Level Softness 2 (灰色阶柔和度 2) | `-0006` (obs.) | Float slider | 100% (UNVERIFIED) | 0-100% | Percent (0-1) | |
  | 7 | Iterations (迭代) | `-0007` (obs.) | Float slider (integer) | 1 | 1-100 (UNVERIFIED) | count | Repeats stage 1 + stage 2. |

  ```
  // UNVERIFIED reconstruction of Adobe's description (two identical stages, repeated)
  stage(a, gs, choke, gls):
      b = gaussian_blur(a, radius = gs)
      t = 0.5 + choke / 255                      // threshold moves up (choke) or down (spread)
      hard = b >= t ? 1 : 0                      // anti-aliased
      return lerp(hard, b, gls)                  // gls = 100%: plain alpha blur (help: use as alpha-channel blur)
  a = src.a
  repeat Iterations: a = stage(a, GS1, C1, GLS1); a = stage(a, GS2, C2, GLS2)
  out = vec4(src.rgb_straight * a, a)
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Help: stage two typically does the opposite of stage one; with Gray Level Softness 100% the effect is just an alpha blur.

### AE-FX-626 · Other Matte effects catalog (其他遮罩效果)

- **What it does:** Lists the AI/temporal matte-refinement effects BAE reserves but does not implement for M1.
- **Exact semantics:**

  | Effect (中文) | matchName | bpc | GPU | Summary |
  |---|---|---|---|---|
  | Refine Hard Matte (调整实边遮罩) | `ADBE RefineRBMatte` | 32 | — | Edge refinement for hard-edged mattes (Roto Brush family): Smooth, Feather, Contrast, Shift Edge, Chatter Reduction, Motion Blur, Decontaminate Edge Colors. |
  | Refine Soft Matte (调整柔和遮罩) | `ADBE RefineMatte2` | 32 | — | Like Refine Hard Matte but computes edge detail (hair) in a band: Calculate Edge Details (计算边缘细节), Additional Edge Radius (其他边缘半径), View Edge Region (查看边缘区域), Smooth (平滑), Feather (羽化), Contrast (对比度), Shift Edge (移动边缘), Chatter Reduction (震颤减少), Motion Blur group with More Motion Blur (更多运动模糊), Decontaminate Edge Colors (净化边缘颜色) and Decontamination group (净化). |
  | mocha shape (mocha shape) | `ISL MochaShapeImporter` | 32 | — | Imports Boris FX Mocha shape data as a matte (Blend mode, Invert, Render edge width, Render type, Fill color, Opacity). |
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** These depend on temporal edge analysis (Refine) or a third-party data format (mocha); loaded as pass-through placeholders keeping parameter data. Obsolete `ADBE RefineMatte` (Refine Matte) is listed under the Obsolete catalog. Roto Brush itself is out of this part's scope.

## Keying effects

### AE-FX-627 · Linear Color Key (线性颜色键)

- **What it does:** Makes pixels transparent according to their distance from a key color in RGB, hue or chroma, or (Keep Colors) restores such pixels.
- **Exact semantics:**
  - matchName `ADBE Linear Color Key2`; category Keying; 32 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Preview (预览) | `ADBE Linear Color Key2-0001` (obs.) | Custom UI (no value) | — | — | — | Two thumbnails (source, current view) and Key/+/− eyedroppers. |
  | 2 | View (视图) | `-0002` (obs.) | Popup | Final Output | 1 Final Output (最终输出), 2 Source Only (仅限源), 3 Matte Only (仅限遮罩) | — | Must be Final Output for rendering to apply the key. |
  | 3 | Key Color (主色) | `-0003` (obs.) | Color | (UNVERIFIED; blue 0,0,255 believed) | any | RGB | Eyedropper; Alt/Option-hover previews. |
  | 4 | Match Colors (匹配颜色) | `-0004` (obs.) | Popup | Using RGB | 1 Using RGB (使用 RGB), 2 Using Hue (使用色相), 3 Using Chroma (使用色度) | — | |
  | 5 | Matching Tolerance (匹配容差) | `-0005` (obs.) | Float slider | 10% (UNVERIFIED) | 0-100% | Percent (0-100) | 0 = whole image opaque, 100 = whole image transparent. |
  | 6 | Matching Softness (匹配柔和度) | `-0006` (obs.) | Float slider | 10% (UNVERIFIED) | 0-100% | Percent (0-100) | Values under 20% usually best (help). |
  | 7 | Key Operation (主要操作) | `-0007` (obs.) | Popup | Key Colors | 1 Key Colors (抠出颜色), 2 Keep Colors (保持颜色) | — | |

  ```
  // UNVERIFIED reconstruction; d in [0,1]
  using RGB:    d = max(|r-kr|, |g-kg|, |b-kb|)           // (UNVERIFIED: max-norm vs euclidean/√3)
  using Hue:    d = circular_distance(hue(px), hue(key)) / 0.5     // hue in [0,1)
  using Chroma: d = distance of (Cb,Cr) or (a,b) chroma vectors, normalised (UNVERIFIED)
  T = Tolerance/100 ; S = Softness/100
  k = d <= T ? 0 : d >= T + S ? 1 : (d - T) / S             // 0 = matches key
  Key Colors:  out.a = in.a * k
  Keep Colors: out.a = max(in.a, (1 - k) * orig.a)          // restores matching pixels removed by an earlier key (UNVERIFIED: orig = layer input before the effect stack)
  out.rgb = in.rgb_straight * out.a ; View Matte Only -> grayscale alpha, Source Only -> input unchanged
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Help workflow: apply a second instance set to Keep Colors to bring back e.g. light-blue clothing lost to a blue-screen key. Colour distances are evaluated in the working space (8/16 bpc: 0-1 normalised; 32 bpc: values > 1 clamp for matching, UNVERIFIED). Key Operation Chinese strings（译名待核）.

### AE-FX-628 · Color Range (颜色范围)

- **What it does:** Keys out a box of colors defined by min/max per component in Lab, YUV or RGB, chosen with eyedroppers.
- **Exact semantics:**
  - matchName `ADBE Color Range`; category Keying; 8 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Preview (预览) | `ADBE Color Range-0001` | Custom UI | — | — | — | Matte thumbnail with Key, + and − eyedroppers that set the Min/Max values. |
  | 2 | Fuzziness (模糊度) | `-0002` | Float slider | 0 (UNVERIFIED) | 0-100? (UNVERIFIED) | — | Softens the transition outside the box. |
  | 3 | Color Space (色彩空间) | `-0003` | Popup | Lab | 1 Lab, 2 YUV, 3 RGB | — | |
  | 4 | Min (L, Y, R) (最小值 (L, Y, R)) | `-0004` | Integer slider | 0 (UNVERIFIED) | 0-255 | level | First component. |
  | 5 | Max (L, Y, R) (最大值 (L, Y, R)) | `-0005` | Integer slider | 0? (UNVERIFIED: nothing keyed until a color is picked) | 0-255 | level | |
  | 6 | Min (a, U, G) (最小值 (a, U, G)) | `-0006` | Integer slider | 0 (UNVERIFIED) | 0-255 | level | Second component. |
  | 7 | Max (a, U, G) (最大值 (a, U, G)) | `-0007` | Integer slider | (UNVERIFIED) | 0-255 | level | |
  | 8 | Min (b, V, B) (最小值 (b, V, B)) | `-0008` | Integer slider | 0 (UNVERIFIED) | 0-255 | level | Third component. |
  | 9 | Max (b, V, B) (最大值 (b, V, B)) | `-0009` | Integer slider | (UNVERIFIED) | 0-255 | level | |

  ```
  c = to_space(px, ColorSpace) scaled so each component spans 0..255 (Lab: L 0..100 -> 0..255, a/b offset 128; YUV: Rec.601 with U,V offset 128) (UNVERIFIED scalings)
  e = max over components of distance outside [Min_i, Max_i] (0 inside the box)
  k = Fuzziness == 0 ? (e > 0 ? 1 : 0) : clamp(e / Fuzziness, 0, 1)   // UNVERIFIED falloff
  out.a = in.a * k
  ```
  - Eyedroppers: Key sets Min = Max = picked colour (± small range); "+" extends the box to include the picked colour; "−" shrinks the box to exclude it (UNVERIFIED exact update rules).
- **Priority:** P2 (legacy keyer; Keylight preferred per Adobe).
- **Owner:** bae-effects (+ bae-ui-motion)
- **Notes:** param matchNames derived (UNVERIFIED). Min/Max labels show all three space letters regardless of the chosen space.

### AE-FX-629 · Extract (提取)

- **What it does:** Keys out pixels whose value in a chosen channel lies outside a black–white point range, with soft tapers; shows a histogram.
- **Exact semantics:**
  - matchName `ADBE Extract`; category Keying; 16 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Histogram (直方图) | `ADBE Extract-0001` (obs.) | Custom UI | — | — | — | Histogram of Channel + draggable transparency control bar (length = Black/White Point, tapers = softness). |
  | 2 | Channel (通道) | `-0002` (obs.) | Popup | Luminance | 1 Luminance (明亮度), 2 Red (红色), 3 Green (绿色), 4 Blue (蓝色), 5 Alpha (Alpha) | — | |
  | 3 | Black Point (黑场) | `-0003` (obs.) | Integer slider | 0 | 0-255 | level | |
  | 4 | White Point (白场) | `-0004` (obs.) | Integer slider | 255 | 0-255 | level | |
  | 5 | Black Softness (黑色柔和度) | `-0005` (obs.) | Integer slider | 0 | 0-255 | level | |
  | 6 | White Softness (白色柔和度) | `-0006` (obs.) | Integer slider | 0 | 0-255 | level | |
  | 7 | Invert (反转) | `-0007` (obs.) | Checkbox | Off | — | — | |

  ```
  v = 255 * channel(px)       // Luminance weights UNVERIFIED (Rec.601 assumed); straight (unpremultiplied) values
  // kept range is [Black, White]; outside becomes transparent; softness tapers inward (UNVERIFIED direction)
  kL = Black Softness == 0 ? (v >= Black) : clamp((v - Black) / BlackSoftness + 1, 0, 1)    // ramp below Black (UNVERIFIED)
  kH = White Softness == 0 ? (v <= White) : clamp((White - v) / WhiteSoftness + 1, 0, 1)
  k = kL * kH ; if Invert: k = 1 - k
  out.a = in.a * k
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-ui-motion)
- **Notes:** Help: values above the White Point and below the Black Point become transparent; best for black or white backgrounds. Differs from Premiere's Extract. At 16 bpc sliders still use the 0-255 scale (UNVERIFIED).

### AE-FX-630 · Keylight (1.2) (Keylight 抠像效果)

- **What it does:** Bundled professional screen keyer (Foundry Keylight) that computes a matte from a screen colour, despills, and offers matte cleanup, garbage masks and colour correction.
- **Exact semantics:**
  - matchName `Keylight 906`; category Keying; 32 bpc; GPU —. Params observed in exports (`Keylight 906-0001`…`-0079`, groups included).
  - Top-level parameters in order (observed matchName suffix, default where observed): About (`-0001`); View (`-0002`, default Final Result = 11) {Source, Source Alpha, Corrected Source, Colour Correction Edges, Screen Matte, Inside Mask, Outside Mask, Combined Matte, Status, Intermediate Result, Final Result}; Unpremultiply Result (`-0003`, On); Screen Colour (`-0004`); Screen Gain (`-0005`, 100); Screen Balance (`-0006`, 50 — UNVERIFIED default, typical 50 for blue / 95 for green); Despill Bias (`-0007`); Alpha Bias (`-0008`); Lock Biases Together (`-0009`, On); Screen Pre-blur (`-0010`, 0).
  - Screen Matte group (`-0011`…`-0021`): Clip Black 0, Clip White 100, Clip Rollback 0, Screen Shrink/Grow 0, Screen Softness 0, Screen Despot Black 0, Screen Despot White 0, Replace Method {None, Source, Hard Colour, Soft Colour}, Replace Colour.
  - Inside Mask group (`-0022`…`-0029`): Inside Mask (mask ref), Inside Mask Softness, Invert, Replace Method, Replace Colour, Source Alpha {Ignore, Add To Inside Mask, Normal}.
  - Outside Mask group (`-0030`…`-0034`): Outside Mask, Outside Mask Softness, Invert.
  - Foreground Colour Correction group (`-0035`…`-0050`): Enable Colour Correction, Saturation 100, Contrast 0, Brightness 0, Colour Suppression {Suppress, Suppression Balance 50, Suppression Amount 100}, Colour Balancing {Hue, Sat, Colour Balance Wheel}.
  - Edge Colour Correction group (`-0051`…`-0069`): Enable Edge Colour Correction, Edge Hardness 50, Edge Softness 0, Edge Grow 0, then the same correction controls.
  - Source Crops group (`-0070`…`-0079`): X Method, Y Method {Colour, Repeat, Reflect, Wrap (UNVERIFIED order)}, Edge Colour, Edge Colour Alpha 100, Left 0, Right 100, Top 0, Bottom 100.

  ```
  // Published screen-difference principle (exact Keylight maths is proprietary; UNVERIFIED):
  // for a screen whose primary channel is P (green) with secondaries S1,S2 (red, blue):
  s  = Screen Balance/100
  diff_px  = P - (s*S1 + (1-s)*S2)          // per pixel (after Screen Pre-blur)
  diff_scr = same for Screen Colour
  a = 1 - Gain/100 * diff_px / diff_scr ; a = clip(a, Clip Black, Clip White) with rollback ; shrink/grow ; despot
  despill: P' = min(P, s*S1 + (1-s)*S2) biased by Despill Bias
  out = unpremultiply? (fg colour-corrected) : premultiplied composite-ready
  ```
- **Priority:** P1 (the standard green-screen tool in AE; BAE needs an equivalent keyer, not bit-exact Keylight).
- **Owner:** bae-effects
- **Notes:** BAE decision: implement a BAE keyer with the same parameter names/semantics and import mapping from `Keylight 906`; document that results are close, not identical. Keylight's UI is English-only in localized AE (UNVERIFIED for zh-CN 2026), hence no Chinese strings here（译名待核）. Help recommends the "Keylight + Key Cleaner + Advanced Spill Suppressor" animation preset.

### AE-FX-631 · Unmult (Unmult 效果)

- **What it does:** Removes a solid black (or white) background by deriving alpha from brightness relative to that background, optionally un-blending the background from semi-transparent colours.
- **Exact semantics:**
  - matchName UNVERIFIED (not in the docsforadobe table; added in a 2025 AE release, UNVERIFIED version); category Keying; 8/16/32 bpc (help).
  - Parameters (help order): Background Color (背景颜色) {Black, White}; Black Level (黑色阶) (when Black); White Level (白色阶) (when White); Softness (柔和度); Remove Color Matting (移除颜色遮罩); Clip HDR Results (剪切 HDR 结果). Defaults UNVERIFIED.

  ```
  // Black background, classic "unmultiply":
  m = max(r, g, b)                                    // straight values
  a = smoothstep(BlackLevel, BlackLevel + Softness, m)    // UNVERIFIED threshold/softness form; Softness 0 = hard cut
  if RemoveColorMatting: rgb = rgb / max(a, eps)      // un-blend black (white: rgb = 1 - (1 - rgb)/a)
  if ClipHDR: rgb = clamp(rgb, 0, 1)
  out = vec4(rgb * a, a * in.a)
  ```
- **Priority:** P1 (very common for stock fire/flare elements on black).
- **Owner:** bae-effects
- **Notes:** Chinese parameter names（译名待核）. Help: Black Level slider right = more dark areas transparent; White Level slider left = removes more bright colours.

### AE-FX-632 · Other Keying effects catalog (其他抠像效果)

- **What it does:** Lists the remaining keying effects reserved by matchName.
- **Exact semantics:**

  | Effect (中文) | matchName | bpc | GPU | Summary |
  |---|---|---|---|---|
  | Advanced Spill Suppressor (高级溢出抑制器) | `ADBE Spill2` | 32 | — | Removes screen-colour spill: Method {Standard, Ultra}, Key Color, Suppression, Ultra settings. |
  | CC Simple Wire Removal (CC Simple Wire Removal) | `CC Simple Wire Removal` | 32 | — | Removes a wire between two points by displacing/blurring neighbouring pixels. |
  | Color Difference Key (颜色差值键) | `ADBE Color Difference Key` | 16 | — | Two partial mattes (A from key colour, B from complementary) combined into an alpha; black/white/gamma per matte. |
  | Difference Matte (差值遮罩) | `ADBE Difference Matte2` | 16 | — | Keys pixels matching a Difference Layer (clean plate): Matching Tolerance, Matching Softness, Blur Before Difference. |
  | Inner/Outer Key (内部/外部键) | `ADBE ATG Extract` | 16 | — | Extracts a foreground using an inside and an outside mask path; Edge Thin/Feather/Threshold, Cleanup Foreground/Background. |
  | Key Cleaner (抠像清除器) | `ADBE KeyCleaner` | 32 | — | Restores edge detail lost by keying (Additional Edge Radius, Reduce Chatter, Alpha Contrast, Strength). |
  | Spill Suppressor (溢出抑制器) | `ADBE Spill Suppressor` | 32 | — | Obsolete category in AE 2026: Color To Suppress, Suppression. |
  | Color Key (颜色键) | `ADBE Color Key` | 16 | — | Obsolete category: Key Color, Color Tolerance, Edge Thin, Edge Feather. |
  | Luma Key (亮度键) | `ADBE Luma Key` | 16 | — | Obsolete category: Key Type {Key Out Brighter, Key Out Darker, Key Out Similar, Key Out Dissimilar}, Threshold, Tolerance, Edge Thin, Edge Feather (matchNames `ADBE Luma Key-0001…0005` obs.). |
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Spill Suppressor, Color Key and Luma Key live in the Obsolete category in current AE (docsforadobe table) but older templates use them often; BAE should implement Color Key and Luma Key early since they are trivial threshold keys.
