## Generate effects

Conventions for this part (apply to every item below unless the item says otherwise):

- Parameter tables are in AE Effect Controls order. "param matchName" follows AE's `<effect matchName>-NNNN` scheme; NNNN is the parameter's stable disk index and is **not** always the UI position (e.g. Fill). Values marked "obs." were observed in exported Lottie JSON (`ef[].mn`); others are derived from the scheme and are UNVERIFIED until checked in AE 2026 with `effect.property(i).matchName`.
- Group start/end markers (topics) consume indices; they are listed as `(group)` / `(group end)` rows.
- "Percent (0-1)" means AE stores a fraction 0..1 and displays it ×100 with "%" (Lottie exports the fraction, e.g. Fill Opacity `k: 1`). "Percent (0-100)" means AE stores the displayed number.
- bpc/GPU columns come from the docsforadobe first-party match-name table (`docs/matchnames/effects/firstparty.md`); "GPU —" means no Mercury GPU path is listed.
- Pixel-unit parameters scale with the downsample factor (see effects.md, Downsample-factor scaling of pixel parameters); point parameters are in layer space.
- Unless stated, generator effects render inside the layer's existing bounds and do **not** expand them (see effects.md, Output bounds expansion; Grow Bounds below).
- "Blending Mode" menus on Generate effects composite the generated image over the effect input; "None" outputs the generated image only. Formulas for the modes: compositing-3d-color.md, Blend modes. Exact menus (AE 26.0 popup table; stored value = 1-based option index, `(-` separators occupy an index):
  - Grid, Checkerboard, Circle: 1 None, 2 Normal, 3 Stencil Alpha, 4 sep, 5 Add, 6 Multiply, 7 Screen, 8 Overlay, 9 Soft Light, 10 Hard Light, 11 sep, 12 Color Dodge, 13 Color Burn, 14 sep, 15 Darken, 16 Lighten, 17 Difference, 18 Exclusion, 19 sep, 20 Hue, 21 Saturation, 22 Color, 23 Luminosity.
  - 4-Color Gradient: the same without Stencil Alpha (1 None, 2 Normal, 3 sep, 4 Add, 5 Multiply, … 18 sep, 19 Hue, 20 Saturation, 21 Color, 22 Luminosity).
  - Fractal Noise / Turbulent Noise: as 4-Color Gradient but without Color (… 19 Hue, 20 Saturation, 21 Luminosity).
  - "Stencil Alpha" here = generated alpha multiplies the input (input shows only where the pattern is). BAE stores the mode by stable name and maps the index per effect on import, because the indices differ between effects.
  - Source: AE 26.0x67 popup-option table (third-party ExtendScript enumeration, cached as research/fx-c/ae26-enums-fxc.txt). Defaults marked "(AE 26 dump)" in this part come from a third-party ExtendScript dump of AE 26.0x67 (`property.value` right after apply on a 320×240 layer at 30 fps, research/fx-c/ae26-dump-fxc.txt); layer-relative defaults are converted to W/H fractions.

### AE-FX-600 · Fill (填充)

- **What it does:** Fills a chosen mask (or all masks, or the whole layer alpha) with a solid color.
- **Exact semantics:**
  - matchName `ADBE Fill`; category Generate; 32 bpc; GPU —. Lottie effect `ty: 21`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Fill Mask (填充蒙版) | `ADBE Fill-0001` (obs.) | Mask reference popup | None | None or any mask on this layer | — | Lottie exports as `ty: 10`, value = mask index (0 = None). |
  | 2 | All Masks (所有蒙版) | `ADBE Fill-0007` (obs.) | Checkbox | Off | — | — | When on, Fill Mask is ignored and the union of all masks (by their modes, see below) is filled. |
  | 3 | Color (颜色) | `ADBE Fill-0002` (obs.) | Color | (255, 0, 0) red (AE 26 dump) | any | RGB | |
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
  | 1 | Start of Ramp (渐变起点) | `ADBE Ramp-0001` (obs.) | Point | (W/2, 0) (AE 26 dump) | any | px, layer space | |
  | 2 | Start Color (起始颜色) | `ADBE Ramp-0002` (obs.) | Color | (0,0,0) black | any | RGB | |
  | 3 | End of Ramp (渐变终点) | `ADBE Ramp-0003` (obs.) | Point | (W/2, H) (AE 26 dump) | any | px | |
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
  | 1 | Starting Point (起始点) | `ADBE Laser-0001` (AE 26 dump) | Point | (0.1·W, H/2) (AE 26 dump: [32,120] on 320×240) | any | px | |
  | 2 | Ending Point (结束点) | `-0002` | Point | (0.9·W, H/2) (AE 26 dump) | any | px | |
  | 3 | Length (长度) | `-0003` | Float slider | 25% (AE 26 dump, stored 0.25) | 0-100% | Percent (0-1) | Visible beam length as % of the path; help: at 100% the visible length is maximal when Time = 50%. |
  | 4 | Time (时间) | `-0004` | Float slider | 50% (AE 26 dump, stored 0.5) | 0-100% | Percent (0-1) | Travel progress from start to end; animate 0→100% to "shoot". |
  | 5 | Starting Thickness (起始厚度（译名待核）) | `-0005` | Float slider | 8 (AE 26 dump) | 0-? (UNVERIFIED) | px | |
  | 6 | Ending Thickness (结束厚度（译名待核）) | `-0006` | Float slider | 8 (AE 26 dump) | 0-? (UNVERIFIED) | px | |
  | 7 | Softness (柔和度) | `-0007` | Float slider | 50% (AE 26 dump, stored 0.5) | 0-100% | Percent (0-1) | Glow falloff. |
  | 8 | Inside Color (内部颜色) | `-0008` | Color | (255, 190, 250) pale pink (AE 26 dump RGBA [1, 0.745, 0.980, 0.725]; the stored 4th component is not used as opacity, UNVERIFIED) | any | RGB | Core colour. |
  | 9 | Outside Color (外部颜色) | `-0009` | Color | (255, 25, 25) red (AE 26 dump RGBA [1, 0.098, 0.098, 0.588]) | any | RGB | Glow colour. |
  | 10 | 3D Perspective (3D 透视) | `-0010` | Checkbox | On (AE 26 dump) | — | — | Thickness varies with start/end thickness as if receding when Time is animated. |
  | 11 | Composite On Original (在原始图像上合成) | `-0011` | Checkbox | Off (AE 26 dump) | — | — | |

  ```
  // UNVERIFIED reconstruction. u in [0,1] along Start->End.
  head = Time * (1 + Length) ; tail = head - Length   // fits the help statement: Length 100% -> whole path visible at Time 50%;
                                                       // defaults (25%, 50%) give a beam centred on the path
  visible segment = [clamp(tail,0,1), clamp(head,0,1)]
  thickness(u) = 3DPerspective ? perspective_lerp(StartT, EndT, u) : lerp(StartT, EndT, u)
  render capsule along the visible segment with core = Inside Color, glow (Softness) = Outside Color
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Help: looks best with motion blur on and shutter angle 360° (compositing-3d-color.md, Motion blur rendering). Parameter names and matchNames are from the AE 26.0 dump (the UI says "Starting/Ending Point" and "Starting/Ending Thickness").

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
  | 1 | Path (路径) | `ADBE Stroke-0001` (obs.) | Mask reference popup | None | None or a mask | — | Lottie `ty: 10`. Defaults of this table confirmed by the AE 26 dump. |
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
  | 8 | Paint Time Properties (绘画时间属性) | `-0008` | Popup | None | 1 None, 2 Opacity, 3 Color (AE 26 popup table) | — | Whether paint props are sampled per dab over time. |
  | 9 | Brush Time Properties (画笔时间属性) | `-0009` | Popup | None | 1 None, 2 Size, 3 Hardness, 4 Size & Hardness (AE 26 popup table) | — | |
  | 10 | Paint Style (绘画样式) | `-0010` | Popup | On Original Image | 1 On Original Image (在原始图像上), 2 On Transparent (在透明背景上), 3 Reveal Original Image (显示原始图像) | — | |

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
  - Parameters in AE order with AE 26 dump defaults and param matchNames (`ADBE Scribble Fill-NNNN`; the suffixes are not in UI order). Popup values are stored 1-based indices that count separators (AE 26 popup table).

  | # | Parameter (中文) | param matchName | Type | Default | Range / enum | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Scribble (涂写) | `-0064` | Popup | Single Mask (stored 3) | 1 None (hidden), 2 sep, 3 Single Mask, 4 All Masks, 5 All Masks Using Modes | — | |
  | 2 | Mask (蒙版) | `-0002` | Mask reference | None (0) | a mask on the layer | — | Used with Single Mask. |
  | 3 | Fill Type (填充类型) | `-0050` | Popup | Inside | 1 Inside, 2 Centered Edge, 3 Inside Edge, 4 Outside Edge, 5 Left Edge, 6 Right Edge | — | Left/Right Edge follow the mask's drawing direction. |
  | — | Edge Options (边缘选项) | `-0009` / end `-0021` | (group) | — | — | — | |
  | 4 | Edge Width (边缘宽度) | `-0052` | Float slider | 20 | ≥ 0 | px | Edge fill types only. |
  | 5 | End Cap (末端端点) | `-0056` | Popup | Round | 1 Round, 2 Butt, 3 Projecting | — | |
  | 6 | Join (连接) | `-0054` | Popup | Round | 1 Round, 2 Bevel, 3 Miter | — | |
  | 7 | Miter Limit (尖角限制) | `-0058` | Float slider | 4 | 1-500 (help) | × stroke width | |
  | 8 | Start/End Apply To (开始/末端应用于) | `-0068` | Popup | Scribble Result (stored 2) | 1 Mask Path, 2 Scribble Result | — | Inside the Edge Options group. |
  | 9 | Color (颜色) | `-0006` | Color | white | any | RGB | |
  | 10 | Opacity (不透明度) | `-0024` | Float slider | 100% (stored 1) | 0-100% | Percent (0-1) | |
  | 11 | Angle (角度) | `-0010` | Angle | 60° | any | degrees | Hatch direction. |
  | 12 | Stroke Width (描边宽度) | `-0008` | Float slider | 2 | 0.1-50 (help) | px | |
  | — | Stroke Options (描边选项) | `-0031` / end `-0045` | (group) | — | — | — | |
  | 13 | Curviness (曲度) | `-0012` | Float slider | 5% (stored 0.05) | 0-100% | Percent (0-1) | Help: 0-100 range of turn roundness. |
  | 14 | Curviness Variation (曲度变化) | `-0066` | Float slider | 1% (stored 0.01) | 0-100% | Percent (0-1) | |
  | 15 | Spacing (间距) | `-0060` | Float slider | 5 | ≥ 0 | px | Distance between hatch lines. |
  | 16 | Spacing Variation (间距变化) | `-0062` | Float slider | 1 | ≥ 0 | px | |
  | 17 | Path Overlap (路径重叠) | `-0038` | Float slider | 0 | negative = end inside, positive = overshoot outside | px | |
  | 18 | Path Overlap Variation (路径重叠变化（译名待核）) | `-0040` | Float slider | 5 | ≥ 0 | px | |
  | 19 | Start (开始) | `-0030` | Float slider | 0% | 0-100% | Percent (0-100) | |
  | 20 | End (结束) | `-0032` | Float slider | 100% | 0-100% | Percent (0-100) | |
  | 21 | Fill Paths Sequentially (顺序填充路径) | `-0028` | Checkbox | On | — | — | With All Masks: Start/End run over the total length. |
  | 22 | Wiggle Type (摆动类型) | `-0048` | Popup | Smooth (stored 3) | 1 Static (静态), 2 Jumpy (跳跃式), 3 Smooth (平滑式) | — | |
  | 23 | Wiggles/Second (摇摆/秒) | `-0044` | Float slider | 10 | ≥ 0 | per second | |
  | 24 | Random Seed (随机植入（译名待核）) | `-0046` | Integer slider | 1 | — | — | |
  | 25 | Composite (合成) | `-0026` | Popup | On Transparent (stored 2) | 1 On Original Image, 2 On Transparent, 3 Reveal Original Image | — | |
  ```
  generate parallel hatch lines at Angle across the fill region (Spacing ± variation),
  connect consecutive lines with curved turns (Curviness ± variation), overshoot by Path Overlap,
  re-randomise per Wiggle Type (Static: once; Jumpy: Wiggles/Second times per second; Smooth: interpolated),
  stroke with Stroke Width, trim by Start/End, composite per Composite.
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-vector)
- **Notes:** Chinese parameter names from the zh-CN Generate effects help page except where marked. Deterministic per Random Seed. Source: AE 26 dump and popup table (research/fx-c/ae26-dump-fxc.txt, ae26-enums-fxc.txt).

### AE-FX-612 · Radio Waves (无线电波)

- **What it does:** Emits expanding wave outlines (polygon, image contour or mask shape) from a producer point.
- **Exact semantics:**
  - matchName `APC Radio Waves`; category Generate; 8 bpc; GPU —.
  - Parameters in AE order with AE 26 dump defaults (param matchNames `APC Radio Waves-NNNN`, suffix in brackets; not in UI order; popups stored as 1-based index):
    - Producer Point (产生点) [`-0004`] = layer centre; Parameters Are Set At (参数设置为) [`-0006`] {1 Birth (生成), 2 Each Frame (每帧)} = Birth; Render Quality (渲染品质) [`-0064`] = 4 (oversampling factor, Best quality only); Wave Type (波形类型) [`-0002`] {1 Polygon (多边形), 2 Image Contours (图像等高线), 3 Mask (蒙版)} = Polygon.
    - Polygon group [`-0068`…`-0023`]: Sides (边) [`-0008`] = 64; Curve Size (曲线大小) [`-0010`] = 0; Curvyness (曲线弯曲度) [`-0012`] = 0; Star (星形) [`-0014`] = off; Star Depth (星深度) [`-0016`] = −0.5.
    - Image Contour group [`-0070`…`-0043`]: Source Layer (源图层) [`-0018`] = layer 1 (this layer, UNVERIFIED reading of the stored index); Source Center (源中心) [`-0020`] = layer centre; Value Channel (值通道) [`-0022`] {1 Intensity, 2 Red, 3 Green, 4 Blue, 5 Alpha, 6 Hue, 7 Lightness, 8 Saturation, 9 Value} = Alpha; Invert Input (反转输入) [`-0024`] = off; Value Threshold (值阈值) [`-0026`] = 127; Pre-Blur (预模糊) [`-0028`] = 0; Tolerance (容差) [`-0030`] = 0.5; Contour (等高线) [`-0032`] = 1.
    - Mask group [`-0072`…`-0049`]: Mask (蒙版) [`-0066`] = None.
    - Wave Motion group (波动) [`-0051`…`-0069`]: Frequency (频率) [`-0034`] = 1 (waves/s); Expansion (扩展) [`-0036`] = 5; Orientation (方向) [`-0038`] = 0°; Direction (方向) [`-0042`] = 90°; Velocity (速率) [`-0040`] = 0; Spin (旋转) [`-0044`] = 0; Lifespan (sec) (寿命（秒）) [`-0056`] = 10; Reflection (反射) [`-0062`] = off.
    - Stroke group (描边) [`-0071`…`-0087`]: Profile (配置文件) [`-0048`] {1 Square (正方形), 2 Triangle, 3 Sawtooth Out, 4 Sawtooth In, 5 Gaussian, 6 Bell, 7 Sine (正弦)} = Square; Color (颜色) [`-0046`] = (0, 0, 255) blue; Opacity (不透明度) [`-0054`] = 100% (stored 1); Fade-in Time (淡入时间) [`-0058`] = 0 s; Fade-out Time (淡出时间) [`-0060`] = 5 s; Start Width (开始宽度) [`-0050`] = 5; End Width (末端宽度) [`-0052`] = 5.
    - Help: Fade-in/Fade-out longer than Lifespan are shortened to Lifespan; if they overlap, the wave never reaches full opacity.
  ```
  waves born at t_k = k / Frequency; wave age a = t - t_k (alive while a < Lifespan)
  radius(a) = Expansion * a ; offset along Direction by Velocity * a ; rotation = Orientation + Spin * a
  Parameters Are Set At = Birth -> each wave keeps the parameter values sampled at t_k
  stroke width lerp(StartWidth, EndWidth, a/Lifespan); opacity fades in/out over Fade-in/Fade-out time
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Chinese names from the zh-CN help page; Profile values other than Square/Sine（译名待核）. Source: AE 26 dump and popup table. History-independent (each wave is a closed-form function of its birth time) — cheap to scrub.

### AE-FX-613 · Audio Spectrum (音频频谱)

- **What it does:** Draws the frequency spectrum of an audio layer as bars, lines or dots along a line or mask path.
- **Exact semantics:**
  - matchName `ADBE AudSpect`; category Generate; 32 bpc; GPU —.
  - Parameters in AE order: Audio Layer (音频层); Start Point (起始点), End Point (结束点) (used when Path = None); Path (路径); Use Polar Path (使用极坐标路径); Start Frequency (起始频率), End Frequency (结束频率) (Hz); Frequency Bands (频段); Maximum Height (最大高度); Audio Duration (音频持续时间) (ms); Audio Offset (音频偏移) (ms); Thickness (粗细); Softness (柔和度); Inside Color (内部颜色), Outside Color (外部颜色); Blend Overlapping Colors (混合叠加颜色); Hue Interpolation (色相插值); Dynamic Hue Phase (动态色相); Color Symmetry (颜色对称); Display Options (显示选项) {1 Digital, 2 Analog lines, 3 Analog dots} (`ADBE AudSpect-0020`); Side Options (面选项) {1 Side A, 2 Side B, 3 Side A & B} (`ADBE AudSpect-0021`); Duration Averaging (持续时间平均化); Composite On Original (在原始图像上合成). Defaults UNVERIFIED.
  ```
  window = audio samples of AudioLayer source in [t + Offset - Duration/2, t + Offset + Duration/2]
  spectrum = |FFT(window)| ; bands = log/linear bins between StartFreq and EndFreq (UNVERIFIED spacing)
  for band i: h_i = MaxHeight * magnitude_i ; draw at u_i = i/(Bands-1) along path, perpendicular, per Side Options
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-media)
- **Notes:** Help: uses the audio **source** footage, ignoring time remapping, effects, stretch and levels — precompose to include them. Masks with modes other than None clip the output. Chinese parameter names from the zh-CN Generate effects help page.

### AE-FX-614 · Audio Waveform (音频波形)

- **What it does:** Draws the audio waveform of an audio layer along a line or mask path.
- **Exact semantics:**
  - matchName `ADBE AudWave`; category Generate; 32 bpc; GPU —.
  - Parameters in AE order: Audio Layer (音频层); Start Point, End Point; Path (路径); Displayed Samples (显示的范例); Maximum Height (最大高度); Audio Duration (音频持续时间) (ms); Audio Offset (音频偏移) (ms); Thickness (粗细); Softness (柔和度); Inside Color, Outside Color (内部颜色/外部颜色); Waveform Options (波形选项) {1 Mono, 2 Left, 3 Right} (`ADBE AudWave-0014`; non-stereo layers play as Mono); Display Options (显示选项) {1 Digital, 2 Analog lines, 3 Analog dots} (`ADBE AudWave-0015`); Composite On Original (在原始图像上合成). Defaults UNVERIFIED.
  ```
  samples = DisplayedSamples points resampled from the source audio window around t + Offset (Duration long)
  y_i = MaxHeight * sample_i ; place along path/line ; draw per Display Options with Thickness/Softness
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-media)
- **Notes:** Same source-audio caveat as Audio Spectrum. Chinese parameter names from the zh-CN Generate effects help page.

### AE-FX-615 · Eyedropper Fill (吸管填充)

- **What it does:** Samples the average color in a circle of the layer and fills the layer with it.
- **Exact semantics:**
  - matchName `ADBE Eyedropper Fill`; category Generate; 8 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Sample Point (采样点) | `ADBE Eyedropper Fill-0001` | Point | layer centre (UNVERIFIED) | any | px | |
  | 2 | Sample Radius (采样半径) | `-0002` | Float slider | (UNVERIFIED) | ≥ 0 | px | |
  | 3 | Average Pixel Color (平均像素颜色) | `-0003` | Popup | (UNVERIFIED) | 1 Skip Empty (跳过空白), 2 All (全部), 3 All Premultiplied (全部预乘), 4 Including Alpha (包括 Alpha) (AE 26 popup table) | — | Skip Empty ignores transparent pixels; All includes their RGB; All Premultiplied averages premultiplied; Including Alpha also averages alpha. |
  | 4 | Maintain Original Alpha (保持原始 Alpha) | `-0004` | Checkbox | Off | — | — | On: original alpha stencils the sampled colour. |
  | 5 | Blend With Original (与原始图像混合) | `-0005` | Float slider | 0% | 0-100% | Percent | |
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Implementation hint: a single reduction pass (compute shader) over the sample disc, then a fill pass. Chinese parameter names from the zh-CN help page.

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
  | 2 | Wipe Angle (擦除角度) | `-0002` (obs.) | Angle | 0x+90.0° (AE 26 dump) | any | degrees | Direction of travel (help: at 90° the wipe travels left→right, the left side disappears first). Angles run clockwise from up, so 0° travels bottom→top and 180° top→bottom (Skottie reference implementation, which follows AE). |
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
  // sector maths matches Skottie RadialWipeEffect (sweep from StartAngle-90° in Skia's clockwise-from-+x convention)
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
  | 3 | Width (宽度) | `-0003` (obs.) | Float slider | 20 (AE 26 dump) | slider range UNVERIFIED; values < 1 behave as 1 (Skottie clamps) | px | Strip period (one transparent + one opaque part). |
  | 4 | Feather (羽化) | `-0004` (obs.) | Float slider | 0 | ≥ 0 | px | |

  ```
  c = Completion/100 ; P = max(1, Width) ; D = radians(Direction)
  u = fract(dot(p - (W/2, H/2), (cos D, sin D)) / P)     // y-down layer space; periods start at the layer centre (Skottie)
  hidden = u < c                                         // each strip's hidden part grows from one side
  feather: soften the two transitions of each period over Feather px (Skottie: 3*Feather, min 0.5 px)
  out = src * (hidden ? 0 : 1)  (with soft ramps)
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Help: Draft animates strips with pixel precision, Best with sub-pixel precision. Orientation per Skottie `VenetianBlindsEffect.cpp` (gradient vector (cos D, sin D) in y-down space): Direction 0° gives vertical strips whose transparent part grows left→right; 90° gives horizontal strips growing top→bottom. UNVERIFIED against an AE reference render (confirm before M1). The help page says 8/16 bpc; the docsforadobe table says 32 bpc (newer).

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
- **Notes:** Defaults confirmed by the AE 26 dump (Iris Points 6, radii 0, Rotation 0, Feather 0, centre = layer centre); help: 6-32 points, and when Outer Radius and/or Inner Radius is 0 the iris is not visible (layer unchanged). Help describes it as a radial transition that reveals the underlying layer, i.e. the inside of the iris is transparent. Chinese strings 外径 / 内径 / 使用内径 are from the zh-CN help page; 光圈中心 / 光圈点（译名待核）.

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
  - matchName `ADBE Unmult` (AE 26.0 effect dump; not in the docsforadobe table; added in a 2025 AE release, UNVERIFIED version); category Keying; 8/16/32 bpc (help).
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

## Noise & Grain effects

Effect > Noise & Grain（杂色和颗粒）. Fractal Noise and Turbulent Noise are generators that fill the layer bounds (they composite over the layer's own pixels through their Blending Mode); the rest are filters.

### AE-FX-633 · Fractal Noise (分形杂色)

- **What it does:** Generates animatable grayscale Perlin-style value noise from up to 20 summed octaves, used for clouds, smoke, textures, luma mattes and displacement maps, and composites it over the layer with its own Opacity and Blending Mode.
- **Exact semantics:**
  - matchName `ADBE Fractal Noise`; category Noise & Grain; 32 bpc; GPU since AE 14.2 (2017) (docsforadobe table). Does not expand bounds: fills the layer's existing rectangle (on a solid = the solid size; on an adjustment layer = the comp).

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Fractal Type (分形类型) | `ADBE Fractal Noise-0001` | Popup | Basic | see enum below | — | |
  | 2 | Noise Type (杂色类型（译名待核；帮助文作"噪点类型"）) | `-0002` | Popup | Soft Linear | 1 Block, 2 Linear, 3 Soft Linear, 4 Spline | — | Lottie value = 1-based index. |
  | 3 | Invert (反转) | `-0003` | Checkbox | off | — | — | |
  | 4 | Contrast (对比度) | `-0004` | Float slider | 100 | slider 50-400 (UNVERIFIED), valid 0-10000 (UNVERIFIED) | — | |
  | 5 | Brightness (亮度) | `-0005` | Float slider | 0 | slider -200..200 (UNVERIFIED), valid -10000..10000 (UNVERIFIED) | — | Units of 1/100 of full white. |
  | 6 | Overflow (溢出) | `-0006` | Popup | Allow HDR Results | 1 Clip (剪切), 2 Soft Clamp (柔和固定（译名待核；帮助文作"软限制"）), 3 Wrap Back (回绕（译名待核；帮助文作"回卷"）), 4 Allow HDR Results (允许 HDR 结果) | — | Default index 4 seen in exported Lottie files. |
  | 7 | Transform (变换) | `-0007` | Group start | — | — | — | |
  | 8 | Rotation (旋转) | `-0008` | Angle | 0x+0° | any | degrees | |
  | 9 | Uniform Scaling (统一缩放) | `-0009` | Checkbox | on | — | — | On: uses Scale; off: Scale Width / Scale Height. |
  | 10 | Scale (缩放) | `-0010` | Float slider | 100 | slider 20-600 (UNVERIFIED), valid 10-10000 (UNVERIFIED) | % | Disabled when Uniform Scaling off. |
  | 11 | Scale Width (缩放宽度) | `-0011` | Float slider | 100 | as Scale | % | Enabled only when Uniform Scaling off. |
  | 12 | Scale Height (缩放高度) | `-0012` | Float slider | 100 | as Scale | % | |
  | 13 | Offset Turbulence (偏移（湍流）) | `-0013` | Point (2D) | layer centre [w/2, h/2] | any | layer px | Default shown as 250,250 in exported 500×500 files. |
  | 14 | Perspective Offset (透视位移) | `-0031` | Checkbox | off | — | — | Note the out-of-order matchName (added later). |
  | 15 | (end Transform) | `-0014` | Group end | — | — | — | |
  | 16 | Complexity (复杂度) | `-0015` | Float slider | 6.0 | slider 1-20, valid 1-20 (UNVERIFIED) | octaves | Fractional values allowed. |
  | 17 | Sub Settings (子设置) | `-0016` | Group start | — | — | — | |
  | 18 | Sub Influence (%) (子影响（%）) | `-0017` | Float slider | 70 | slider 25-100 (UNVERIFIED), valid 0-10000 (UNVERIFIED) | % | |
  | 19 | Sub Scaling (子缩放) | `-0018` | Float slider | 56 | slider 25-100 (UNVERIFIED), valid 10-10000 (UNVERIFIED) | % | |
  | 20 | Sub Rotation (子旋转) | `-0019` | Angle | 0x+0° | any | degrees | |
  | 21 | Sub Offset (子位移) | `-0020` | Point (2D) | [0, 0] | any | px (UNVERIFIED: in octave space, see algorithm) | |
  | 22 | Center Subscale (中心辅助比例) | `-0021` | Checkbox | off | — | — | |
  | 23 | (end Sub Settings) | `-0022` | Group end | — | — | — | |
  | 24 | Evolution (演化) | `-0023` | Angle | 0x+0° | any | revolutions + degrees | Each revolution is a new state (not periodic in 360°). |
  | 25 | Evolution Options (演化选项) | `-0024` | Group start | — | — | — | |
  | 26 | Cycle Evolution (循环演化) | `-0025` | Checkbox | off | — | — | |
  | 27 | Cycle (in Revolutions) (循环（旋转次数）) | `-0026` | Integer slider | 1 | slider 1-? (UNVERIFIED), valid 1-1000 (UNVERIFIED) | revolutions | Enabled only when Cycle Evolution on. |
  | 28 | Random Seed (随机植入) | `-0027` | Integer slider | 0 | slider 0-1000 (UNVERIFIED), valid -10000..10000 (UNVERIFIED) | — | |
  | 29 | (end Evolution Options) | `-0028` | Group end | — | — | — | |
  | 30 | Opacity (不透明度) | `-0029` | Float slider | 100% | 0-100 | % | |
  | 31 | Blending Mode (混合模式) | `-0030` | Popup | Normal | see enum below | — | |

  - Fractal Type enum (stored 1-based index; `(-` separators occupy an index, from an AE 26.0 dump): 1 Basic (基本), 2 separator, 3 Turbulent Smooth (湍流平滑), 4 Turbulent Basic (湍流基本), 5 Turbulent Sharp (湍流锐化), 6 separator, 7 Dynamic (动态), 8 Dynamic Progressive (动态渐进), 9 Dynamic Twist (动态扭转), 10 separator, 11 Max (最大值), 12 Smeary (模糊), 13 Swirly (漩涡), 14 Rocky (岩石), 15 Cloudy (多云), 16 Terrain (地形), 17 Subscale (辅助比例), 18 Small Bumps (小凸起), 19 Strings (字符串), 20 Threads (线程)（enum 中文均译名待核）.
  - Blending Mode enum (stored index): 1 None, 2 Normal, 3 sep, 4 Add, 5 Multiply, 6 Screen, 7 Overlay, 8 Soft Light, 9 Hard Light, 10 sep, 11 Color Dodge, 12 Color Burn, 13 sep, 14 Darken, 15 Lighten, 16 Difference, 17 Exclusion, 18 sep, 19 Hue, 20 Saturation, 21 Luminosity. None = output noise only (layer alpha kept, UNVERIFIED: alpha = layer alpha); Hue / Saturation = noise value used as hue / saturation of the original (HSL), no change on grayscale originals; other modes per compositing-3d-color.md, Blend modes.

  ```
  // Best-known reconstruction (Skottie FractalNoiseEffect + help text). UNVERIFIED in exact constants.
  // p: layer-space pixel centre.
  GRID = 64 px at Scale 100                                   // UNVERIFIED base cell size (Skottie constant chosen to match AE visually)
  M  = translate(OffsetTurbulence) * scale(sx/100, sy/100) * rotate(Rotation) * scale(GRID)   // noise->layer
  q0 = inverse(M) * p                                         // octave-0 noise coordinates
  S  = translate(-SubOffset/100) * rotate(-SubRotation) * scale(100/SubScaling)               // per-octave step (cumulative)
  // Center Subscale off: sub-octaves scale about the noise origin (offset accumulates); on: about the same point as the
  //   previous octave, so octaves look stacked/duplicated. Perspective Offset on: octave k's offset is divided by its
  //   cumulative scale (parallax, deeper octaves move less) UNVERIFIED.
  // Evolution -> two coherent noise "planes" interpolated (smooth when Evolution animates):
  e      = radians(Evolution) * 0.25                         // UNVERIFIED rate (Skottie: 0.25 plane per radian)
  if CycleEvolution: P = round(2π*max(Cycle,1)*0.25); e = radians(Evolution) * P/(2π*max(Cycle,1)); else P = ∞
  seedOff = hash(RandomSeed)                                  // new pattern per seed; animating seed = popping
  plane0 = mod(floor(e), P) + seedOff ; plane1 = mod(floor(e)+1, P) + seedOff ; w = fract(e)
  base(q) = lerp(hash3(cell(q), plane0), hash3(cell(q), plane1), w)   // value noise in [0,1]
  filter(q) by Noise Type:
     Block       = base at floor(q) (nearest)
     Linear      = bilinear of the 4 lattice values around q-0.5
     Soft Linear = bilinear with smoothstep(t) weights
     Spline      = bicubic (Catmull-Rom UNVERIFIED) of 4x4 lattice values
  fractal(n) by Fractal Type:  Basic n ; Turbulent Basic 2|n-0.5| ; Turbulent Smooth (2|n-0.5|)^2 ;
     Turbulent Sharp sqrt(2|n-0.5|) ; others UNVERIFIED (Dynamic*: domain-warped octaves; Max: max over octaves;
     Smeary/Swirly/Rocky/Cloudy/Terrain/...: per-type remaps) -> BAE must calibrate against AE renders.
  oct = Complexity; amp = 1; n = 0; wsum = 0; q = q0
  loop:  wk = amp * min(oct, 1)          // fractional last octave is faded by the fractional part
         n += wk * fractal(filter(q)); wsum += wk
         if oct <= 1: break
         oct -= 1; amp *= SubInfluence/100; q = S * q
  v = n / wsum                                                // Sub Influence 0 == Complexity 1
  if Invert: v = 1 - v
  v = (v - 0.5) * Contrast/100 + 0.5 + Brightness/100         // UNVERIFIED exact form
  Overflow: Clip -> clamp(v,0,1); Soft Clamp -> smooth sigmoid into (0,1) (UNVERIFIED curve);
            Wrap Back -> triangle fold: t = mod(v,2); v = t<=1 ? t : 2-t ; Allow HDR -> unchanged (8/16 bpc still clip on store)
  noise = vec3(v) ; result = blend(Blending Mode, src=noise*Opacity, dst=layer)
  ```
  - Edge cases: Scale and Sub Scaling are clamped to their valid minimum (no division by 0); Complexity < 1 behaves as 1; at 8/16 bpc Allow HDR Results equals Clip after quantisation.
- **Priority:** P1 (the most used generator for textures/mattes; exact pattern match with AE is not achievable without calibration).
- **Owner:** bae-effects
- **Notes:** Lottie: generic effect `ty: 5`, `mn: "ADBE Fractal Noise"`, 31 parameter entries (`ix` 1-31) as in the table; Skottie renders it (Basic and the three Turbulent types, Block/Linear/Soft Linear; ignores Invert/Contrast/Brightness/Overflow/Perspective/Center Subscale/Opacity/Blending); lottie-web does not. Help gotcha: for a seamless loop the Evolution keyframes must cover exactly `Cycle` revolutions (or a divisor) and the last frame equals the first. Implementation hint: one WGSL fragment pass, loop count specialised by `ceil(Complexity)` buckets (1,2,3,4,8,20) as Skottie does; hash-based value noise avoids textures. BAE decision: ship Basic/Turbulent*/Max/Dynamic first; the remaining exotic types fall back to Basic with a warning until calibrated. Chinese parameter strings in the table are the zh-CN UI strings as commonly seen in AE tutorials (演化, 随机植入, 中心辅助比例, 透视位移, 子位移)（译名待核）; the zh-CN help page instead writes 演变, 随机种子, 居中子缩放, 噪点类型.

### AE-FX-634 · Noise (杂色)

- **What it does:** Adds random per-pixel variation to the image, either the same value to all channels (monochrome) or independent values per channel (color noise); a new pattern every frame.
- **Exact semantics:**
  - Current matchName `ADBE Noise2` (AE 26.0; the docsforadobe table still lists `ADBE Noise`, which AE 26.0 shows in the hidden _Obsolete category; version of the switch UNVERIFIED); category Noise & Grain; 32 bpc (help); GPU — (UNVERIFIED for `ADBE Noise2`).

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Amount of Noise (杂色数量) | `ADBE Noise-0001` / `ADBE Noise2-0001` | Float slider | 0.0% | 0-100 | % | |
  | 2 | Noise Type (杂色类型) | `-0002` | Checkbox "Use Color Noise" (使用杂色) | on | — | — | Lottie stores 1 = on. |
  | 3 | Clipping (剪切) | `-0003` | Checkbox "Clip Result Values" (剪切结果值) | on | — | — | No effect in 32 bpc projects (help). |

  ```
  // UNVERIFIED reconstruction; values straight (un-premultiplied), normalised 0..1
  a = Amount/100
  seed = hash(layerId, frameIndex)                 // changes every frame automatically (no Random Seed param)
  for each pixel: r = Color ? vec3(rnd(seed,p,0), rnd(seed,p,1), rnd(seed,p,2)) : vec3(rnd(seed,p,0))   // uniform [0,1)
                  c' = c + a * (r - 0.5) * 2        // UNVERIFIED amplitude: at 100% noise spans the full range
                  if Clipping and bpc < 32: c' = clamp(c', 0, 1)
                  else (unclipped, 8/16 bpc): out-of-range values wrap (UNVERIFIED) -> "more apparent noise" per help
  alpha unchanged
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Help describes Noise Type as "Use Color Noise adds random values to R, G, B individually". The random pattern is a function of time, so it renders identically on re-render (deterministic per frame). Lottie: not supported by lottie-web/Skottie (`ty: 5`, `mn: "ADBE Noise"` in exported files). BAE decision: use a stateless integer hash (PCG) of (pixel, frame, layer) for determinism across GPUs. Parameter Chinese strings from memory of the zh-CN UI（译名待核）; the zh-CN help page writes 噪点数量 / 噪点类型 / 裁切.

### AE-FX-635 · Median (中间值)

- **What it does:** Replaces each pixel by the per-channel median of the pixels in a square neighbourhood of the given Radius; removes salt-and-pepper noise at small radii and gives a painterly look at large radii.
- **Exact semantics:**
  - Two effects exist in AE 26.0: **Median** (中间值) `ADBE PS Median` (new; help: 8/16 bpc; parameter list beyond Radius UNVERIFIED) and **Median (Legacy)** (中值（旧版）) `ADBE Median` (docsforadobe: 16 bpc, GPU —). Older projects and presets use `ADBE Median`.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Radius (半径) | `ADBE Median-0001` | Integer slider | 0 (UNVERIFIED) | slider 0-100? valid 0-255 (UNVERIFIED) | px | 0 = no-op. |
  | 2 | Operate On Alpha Channel (在 Alpha 通道上运算) | `ADBE Median-0002` | Checkbox | off | — | — | On: alpha is median-filtered too; off: only colour (premultiplied? UNVERIFIED). |

  ```
  r = Radius (downsampled by preview resolution factor)
  for each channel c in (R,G,B[,A]): out.c(p) = median{ in.c(q) : |qx-px|<=r, |qy-py|<=r }   // square window, UNVERIFIED vs circular
  pixels outside the layer read as transparent black (UNVERIFIED: edge clamp)
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Not separable, cost O(r²) per pixel naively. Implementation hint: per-channel histogram median (Huang / constant-time median with column histograms) on the CPU for 8 bpc, or a GPU bitonic/selection on small radii and a 256-bin histogram search for larger radii; for 32 bpc quantise to 16-bit bins. BAE decision: implement one kernel and map both matchNames onto it.

### AE-FX-636 · Add Grain (添加颗粒)

- **What it does:** Synthesises film-like grain from parameters or film-stock presets, with tonal, colour and animation controls, optionally restricted by a colour/mask matte.
- **Exact semantics:**
  - matchName `VISINF Grain Implant`; category Noise & Grain; 16 bpc; GPU —.
  - Top-level parameters in order: Viewing Mode (查看模式) {Preview (预览) **default**, Blending Matte (混合遮罩), Final Output (最终输出)}; Preset (预设) (film stocks, e.g. Kodak 5248/5279/5287/5293/5298/SFX 200T, Eastman…, Fuji…; list UNVERIFIED); Preview Region (预览区域) group {Center, Width 200, Height 200, Show Box, Box Color}; Tweaking (微调) {Intensity, Size, Softness, Aspect Ratio, Channel Intensities {R,G,B}, Channel Size {R,G,B}} (values relative, 1.0 = neutral); Color (颜色) {Monochromatic, Saturation, Tint Amount, Tint Color}; Application (应用) {Blending Mode (Film/Normal/Multiply/Screen/... UNVERIFIED list), Shadows, Midtones, Highlights, Midpoint, Channel Balance {R,G,B Shadows/Midtones/Highlights}}; Animation (动画) {Animation Speed (default 1.0), Animate Smoothly, Random Seed}; Blend with Original (与原始图像混合) {Amount, Combine Match and Mask Using, Color Matching {...}, Masking Layer, Masking Mode, If Layer Sizes Differ}. Param matchNames `VISINF Grain Implant-NNNN` (numbering UNVERIFIED).
  - Gotcha: in Preview mode the effect renders grain only inside a 200×200 px box framed in white; the user must switch to Final Output before rendering (whether the render queue overrides this is UNVERIFIED; treat Preview as rendering only the box).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Animation Speed 0 freezes the grain; Random Seed changes the pattern on the same frame. Implementation hint: band-limited Gaussian noise generated per channel at Size, blurred by Softness, weighted by a tone curve from Shadows/Midtones/Highlights around Midpoint.

### AE-FX-637 · Turbulent Noise (湍流杂色)

- **What it does:** A faster, more modern variant of Fractal Noise whose small features move faster than large ones (turbulence), without cycle/loop controls.
- **Exact semantics:**
  - matchName `ADBE AIF Perlin Noise 3D`; category Noise & Grain; 32 bpc; GPU — (docsforadobe).
  - Parameters as Fractal Noise (AE-FX-633) with the same names, enums and defaults (Fractal Type, Noise Type, Invert, Contrast, Brightness, Overflow, Transform group incl. Perspective Offset, Complexity, Sub Settings {Sub Influence, Sub Scaling, Sub Rotation, Sub Offset}, Evolution, Opacity, Blending Mode), except: no Center Subscale; Evolution Options contains **Turbulence Factor** (湍流因子) and Random Seed instead of Cycle Evolution / Cycle. Param matchNames `ADBE AIF Perlin Noise 3D-NNNN` (exact numbering UNVERIFIED).
  - Turbulence Factor 0 = all octaves evolve at the same speed (Fractal-Noise-like); larger values make octave k evolve faster (UNVERIFIED model: octave k's evolution rate × (1/SubScaling)^(k·factor)); default UNVERIFIED (believed 0.5 or 1).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Help: the only reason to prefer Fractal Noise is looping (Cycle Evolution). Noise is genuinely 3D Perlin (gradient) noise sampled in (x, y, evolution) (UNVERIFIED but implied by the matchName), so it is smoother than Fractal Noise's value noise. BAE decision: share the octave loop with AE-FX-633 and switch the basis function.

### AE-FX-638 · Other Noise & Grain effects catalog (其他杂色和颗粒效果)

- **What it does:** Reserves the remaining Noise & Grain effects by matchName so projects round-trip.
- **Exact semantics:**

  | Effect (中文) | matchName | bpc | GPU | Summary |
  |---|---|---|---|---|
  | Dust & Scratches (蒙尘与划痕) | `ADBE Dust & Scratches` | 16 | — | Radius, Threshold: median-like replacement of pixels differing from neighbours by more than Threshold. |
  | Match Grain (匹配颗粒) | `VISINF Grain Duplication` | 16 | — | Samples grain from a Noise Source Layer and synthesises matching grain; Compensate for Existing Noise; shares Tweaking/Color/Application/Animation/Blend groups with Add Grain. |
  | Noise Alpha (杂色 Alpha) | `ADBE Noise Alpha2` (old `ADBE Noise Alpha`) | 8 | — | Noise {Uniform Random, Squared Random, Uniform Animation, Squared Animation}, Amount, Original Alpha {Add, Clamp, Scale, Edges}, Overflow {Clip, Wrap Back, Wrap}, Random Seed, Noise Options (Animation): Cycle Noise, Cycle (in Revolutions). |
  | Noise HLS (杂色 HLS) | `ADBE Noise HLS2` (old `ADBE Noise HLS`) | 8 | — | Noise {Uniform, Squared, Grain}, Hue, Lightness, Saturation, Grain Size, Noise Phase. |
  | Noise HLS Auto (杂色 HLS 自动) | `ADBE Noise HLS Auto2` (old `ADBE Noise HLS Auto`) | 8 | — | As Noise HLS but animates automatically with Noise Animation Speed. |
  | Remove Grain (移除颗粒) | `VISINF Grain Removal` | 16 | — | Sample-based denoiser: Noise Reduction Settings, Fine Tuning (Chroma Suppression, Texture, Noise Size Bias, Clean Solid Areas), Temporal Filtering, Unsharp Mask, Sampling, Blend with Original. |
  | Curl Noise (Curl Noise 效果) | UNVERIFIED (added after AE 26.0) | 32 (UNVERIFIED) | UNVERIFIED | Curl-noise flow-line generator: Input Noise {Internal, This Layer, Other Layer}, Curl Noise {Transform, Curl Generation (Evolution, Turbulence Speed 0-500 default 10, Swirl ±360, Density ±100, Smoothness, Vertical Bias), Flow Options (Sample Count 3-24, Sample Radius 0-200, Flow Softness, Edge Definition, Flow Falloff)}, Output {View, Contrast, Brightness, Clip HDR Results, Channel}. |
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Grain sampling effects (Match/Remove Grain) sample from the source layer ignoring earlier effects and masks (help). Chinese names of Curl Noise not localised on the zh-CN help page（译名待核）.

## Time effects

Effect > Time（时间）. These effects read their own layer (or another layer) at times other than the current time, so the render graph must be able to evaluate a layer's effect input at arbitrary layer times. Cross-reference: time remapping, frame blending and motion-blur sampling in transform-keyframes-expressions.md (Time remap) and compositing-3d-color.md (Motion blur & frame blending rendering).

### AE-FX-639 · Echo (残影)

- **What it does:** Combines the layer's image at the current time with copies from earlier or later times ("echoes"), weighted by a geometric decay and merged with a chosen operator; used for trails, streaks and smears.
- **Exact semantics:**
  - matchName `ADBE Echo`; category Time; 32 bpc; GPU —. Does not expand bounds.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Echo Time (seconds) (残影时间（秒）) | `ADBE Echo-0001` | Float slider | −1 comp frame duration (−0.033 at 30 fps; UNVERIFIED that it follows the comp frame rate) | slider −2..2 (UNVERIFIED), valid −30..30 (UNVERIFIED) | seconds | Negative = past frames, positive = future frames. |
  | 2 | Number Of Echoes (残影数量) | `-0002` | Integer slider | 1 | slider 0-30 (UNVERIFIED), valid 0-1000 (UNVERIFIED) | — | 0 = current frame only. |
  | 3 | Starting Intensity (起始强度) | `-0003` | Float slider | 1.00 | 0-1 | factor | Opacity of the first image (the current-time frame) in the sequence. |
  | 4 | Decay (衰减) | `-0004` | Float slider | 1.00 | 0-1 | ratio | Each echo's opacity = previous × Decay. |
  | 5 | Echo Operator (残影运算符) | `-0005` | Popup | Add | 1 Add (相加), 2 Maximum (最大值), 3 Minimum (最小值), 4 Screen (滤色), 5 Composite In Back (从后至前组合), 6 Composite In Front (从前至后组合), 7 Blend (混合) | — | Lottie value = index. |

  ```
  N = NumberOfEchoes ; dt = EchoTime
  for k in 0..N:
     t_k = t_layer + k*dt                          // layer-local time; k = 0 is the current frame
     I_k = effectInput(t_k)                        // premultiplied RGBA of the layer's source + masks + preceding effects at t_k (UNVERIFIED: masks included)
                                                   // outside the source's duration / before In point the input is transparent (UNVERIFIED for still sources: same image)
     w_k = StartingIntensity * Decay^k
  Add:                 O = Σ w_k I_k                               (clamped at 8/16 bpc; overloads to white)
  Maximum:             O = max_k (w_k I_k)  per channel             (UNVERIFIED: weighted before max)
  Minimum:             O = min_k (w_k I_k)  per channel             (UNVERIFIED)
  Screen:              O = 1 - Π_k (1 - w_k I_k)
  Composite In Back:   O = (w_N I_N) over ... over (w_1 I_1) over (w_0 I_0)    // current frame at the back
  Composite In Front:  O = (w_0 I_0) over (w_1 I_1) over ... over (w_N I_N)    // current frame in front
  Blend:               O = Σ w_k I_k / (N+1)                        (UNVERIFIED normalisation; help: "averages the echoes")
  // over(a, b) = a + b*(1 - a.alpha) on premultiplied values; w_k scales all four channels.
  ```
  - Echo Time is evaluated at the current time (animatable); frames are fetched at exact `t_k` (no frame rounding beyond the source's own frame lookup).
- **Priority:** P1
- **Owner:** bae-effects (+ bae-render)
- **Notes:** Help: Echo only echoes change inside the layer (footage motion, animated effects); motion from the layer's own Transform is not echoed, so users precompose. Lottie: exported as `ty: 5`, `mn: "ADBE Echo"`; not rendered by lottie-web/Skottie. Implementation hint: request N+1 upstream evaluations from the render graph (cache-friendly: neighbouring frames are usually cached during playback); accumulate in a 32-bit float target. Number of Echoes × upstream cost can explode, so BAE should cap concurrent fetches and reuse the frame cache.

### AE-FX-640 · Posterize Time (色调分离时间)

- **What it does:** Locks the layer's content to a lower (or different) frame rate by evaluating its source and preceding effects only at quantised times, producing a stepped/strobing look.
- **Exact semantics:**
  - matchName `ADBE Posterize Time`; category Time; 32 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Frame Rate (帧速率) | `ADBE Posterize Time-0001` | Float slider | 24.0 (UNVERIFIED) | slider 1-99 (UNVERIFIED), valid 0.1-999 (UNVERIFIED) | fps | Only Hold interpolation is allowed for its keyframes (help). |

  ```
  t_q = floor((t_layer - t0) * FrameRate + ε) / FrameRate + t0   // t0 = quantisation origin: comp time 0 (UNVERIFIED vs layer In point/layer time 0)
  output = effectInput(t_q)            // source, masks (UNVERIFIED) and every effect ABOVE Posterize Time in the stack are evaluated at t_q
  // Effects BELOW it still evaluate their own parameters at t (UNVERIFIED); layer Transform is never posterized (it is applied after effects).
  ```
  - Frame Rate ≥ comp frame rate: no visible change (each comp frame maps to itself).
- **Priority:** P1 (common for "on twos" / stop-motion looks).
- **Owner:** bae-effects (+ bae-render)
- **Notes:** Help: same result as precomposing and enabling the precomp's "Preserve frame rate when nested or in render queue" (see project-comp-layers.md, Composition Settings). The animate-with-Hold-only restriction is enforced by the property (interpolation type locked). Lottie: `ty: 5`, `mn: "ADBE Posterize Time"`; lottie-web ignores it. Expressions on the layer (`posterizeTime()` is a separate expression method, see transform-keyframes-expressions.md) are unrelated. Implementation hint: implemented as a time-mapping node in the render graph, so the frame cache can reuse the quantised frame.

### AE-FX-641 · Time Displacement (时间置换)

- **What it does:** Per pixel, replaces the layer's pixel with the same pixel from another time, chosen by the luminance of a displacement map layer (slit-scan effects).
- **Exact semantics:**
  - matchName `ADBE Time Displacement`; category Time; 16 bpc; GPU —. Masks on the same layer are ignored after applying (help).

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Time Displacement Layer (时间置换图层) | `ADBE Time Displacement-0001` | Layer | this layer (UNVERIFIED) | any layer in the comp or None | — | Map is read in its source form (no masks/effects/transforms; help). |
  | 2 | Max Displacement Time [sec] (最大移位时间 [秒]) | `-0002` | Float slider | 1.0 | slider 0-? / valid −? (UNVERIFIED; negative allowed believed) | seconds | |
  | 3 | Time Resolution [fps] (时间分辨率 [fps]) | `-0003` | Float slider | 60.0 | slider 1-? (UNVERIFIED) | fps | Number of distinct time slices per second. |
  | 4 | If Layer Sizes Differ: Stretch Map to Fit (如果图层大小不同：伸缩对应图以适合) | `-0004` | Checkbox | on (UNVERIFIED) | — | — | Off: map centred (help: "in the composition"). |

  ```
  L = luminance(map(p)) in 8-bit terms 0..255 (higher bpc: L = 255*luma)   // Rec.601 luma UNVERIFIED
  d = MaxDisplacementTime * (2 * (L - 128) / 256)                        // help formula: 0 -> -max, 128 -> 0, 255 -> +max*(127/128)
  d_q = round(d * TimeResolution) / TimeResolution                         // quantise to time slices (UNVERIFIED rounding)
  out(p) = effectInput(t + d_q)(p)                                         // outside the layer duration: transparent (UNVERIFIED)
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-render)
- **Notes:** Cost = number of distinct `d_q` values in the frame (each a full upstream evaluation). Implementation hint: bucket pixels by slice, fetch each slice frame once, gather in one pass. Lottie: no.

### AE-FX-642 · Other Time effects catalog (其他时间效果)

- **What it does:** Reserves the remaining Time-category effects by matchName so projects round-trip.
- **Exact semantics:**

  | Effect (中文) | matchName | bpc | GPU | Summary |
  |---|---|---|---|---|
  | CC Force Motion Blur (CC Force Motion Blur) | `CC Force Motion Blur` | 32 | — | Renders intermediate sub-frames of the layer and averages them to force motion blur (Motion Blur Samples, Override Shutter Angle, Shutter Angle, Native Motion Blur). |
  | CC Wide Time (CC Wide Time) | `CC Wide Time` | 32 | — | Blends a number of frames forward/backward (Forward Steps, Backward Steps, Native Motion Blur). |
  | CC Time Blend (CC Time Blend) | `CC Time Blend` | — | — | Accumulates frames over time with Transfer mode and Accumulation; moved to _Obsolete in AE 26.0. |
  | CC Time Blend FX (CC Time Blend FX) | `CC Time Blend FX` | — | — | Instance/Clear/Accumulate pair of CC Time Blend; moved to _Obsolete in AE 26.0. |
  | Pixel Motion Blur (像素运动模糊) | `ADBE OFMotionBlur` | 32 | — | Optical-flow motion blur: Shutter Control {Automatic, Manual}, Shutter Angle, Shutter Samples, Vector Detail. |
  | Time Difference (时差) | `ADBE Difference` | 8 | — | Colour difference between this layer and a Target layer at Time Offset: Contrast, Absolute Difference, Alpha Channel {Original, Target, Blend, Max, Full On, Lightness Of Result, Max Of Result, Alpha Difference, Alpha Difference Only}. |
  | Timewarp (时间扭曲) | `ADBE Timewarp` | 32 | — | Retiming with Method {Whole Frames, Frame Mix, Pixel Motion}, Adjust Time By {Speed (default 50), Source Frame}, Tuning (Vector Detail, Smoothing, Error Threshold, Block Size, Weighting), Motion Blur (Shutter Angle, Shutter Samples), Matte Layer, Warp Layer, Show, Source Crops. Masks on the layer are ignored. |
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Pixel Motion Blur and Timewarp Pixel Motion need an optical-flow estimator (shared with frame blending's Pixel Motion mode, see compositing-3d-color.md). Timewarp Speed 50% deliberately samples at ¼ and ¾ frame offsets rather than reusing original frames (help).

## Simulation effects

Effect > Simulation（模拟）. All are P2 for BAE M1. Particle effects are stateful simulations: the frame at time t depends on every earlier step, so they need deterministic, seekable stepping (see the Implementation hint in AE-FX-643).

### AE-FX-643 · CC Particle World (CC 粒子世界（译名待核）)

- **What it does:** A 3D particle emitter (bundled CycoreFX) that births particles from a producer volume, moves them under physics presets, and renders them as lines, sprites, spheres, polygons or textured cards, optionally through the comp camera.
- **Exact semantics:**
  - matchName `CC Particle World`; category Simulation; 16 bpc; GPU —. Output fills the layer (particles outside the layer bounds are clipped; users apply it to a comp-size solid).
  - Main parameter groups in UI order (param matchNames `CC Particle World-NNNN`; most defaults UNVERIFIED):
    - Grid & Guides (网格和参考线): Position, Radius, Motion Path, Motion Path Frames, Grid, Grid Position {Floor, Producer, World} (`-0056`), Grid Axis {X Axis, Y Axis, Z Axis} (`-0057`), Grid Subdivisions, Grid Size, Horizon, Axis Box.
    - Birth Rate (出生速率) 1.0; Longevity (sec) (寿命（秒）) 1.0.
    - Producer (发生器): Position X/Y/Z (0,0,0 = layer centre in CC units where 1.0 ≈ half layer width, UNVERIFIED), Radius X/Y/Z (0.025 believed).
    - Physics (物理学): Animation (`-0015`) {Explosive (default), Direction Axis, Cone Axis, ─, Viscouse, Twirl, Twirly, Vortex, Fire, ─, Jet Sideways, ─, Fractal Omni, Fractal Uni} (stored index counts separators); Velocity 1.0, Inherit Velocity %, Gravity 0.5, Resistance 0, Extra, Extra Angle, Direction Axis XYZ, Gravity Vector XYZ; Floor group: Floor Position {All, Above Floor, After Floor} (`-0064`), Floor Render {Normal, Reflected on Floor, Projected on Floor} (`-0065`), Floor Action {None, Ice, Glue, Bounce} (`-0066`).
    - Particle (粒子): Particle Type (`-0023`) {Line (default), Star, ─, Shaded Sphere, Faded Sphere, Darken&Faded Sphere, Bubble, ─, Motion Polygon, Motion Square, TriPolygon, QuadPolygon, Textured TriPolygon, Textured QuadPolygon, Tetrahedron, Cube, ─, Lens Convex, Lens Concave, Lens Fade, Lens Darken Fade, Lens Bubble, ─, Textured Square, Textured Disc, Textured Faded Disc}; Texture {Texture Layer, Scale Velocity, Texture Time {Birth, Current, From Start} (`-0047`)}; Rotation Speed, Initial Rotation, Rotation Axis {X, Y, Z, Any Axis} (`-0071`); Birth Size, Death Size, Size Variation, Max Opacity (75% believed); Color Map (`-0028`) {Custom, Birth to Death (default), Origin to Death, Birth to Origin, Origin Constant}, Birth Color, Death Color, Custom Color Map, Volume Shade, Transfer Mode (`-0039`) {Composite, Screen, Add, Black Matte}.
    - Extras (额外): Effect Camera, Depth Cue {None, Fade, Fog} (`-0082`), Composite With Original, Random Seed.
  ```
  // Simulation contract (BAE): state(t) = step^n(state(t_birth_start)), fixed internal step dt = 1/compFps (UNVERIFIED: CycoreFX sub-steps)
  per step: emit floor(BirthRate * k) new particles uniformly in the producer ellipsoid (k = UNVERIFIED rate constant per second)
            v0 = Velocity * dir(Animation preset) * random, gravity along Gravity Vector, drag by Resistance
            age += dt; kill when age > Longevity * (1 ± random variation)
  size(age) = lerp(BirthSize, DeathSize, age/life); color via Color Map; opacity = MaxOpacity (fades per type)
  render with the layer's/comp camera if Effect Camera uses AE camera, else the effect's internal camera
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Chinese group names（译名待核）; CycoreFX parameter names are not localised in zh-CN AE (UNVERIFIED). Implementation hint: deterministic per-particle seeding (particle id = emission step × slot) lets BAE recompute any frame by stepping from t=0 with a GPU compute pass and cache checkpoints every N frames for scrubbing. Lottie: none.

### AE-FX-644 · CC Particle Systems II (CC 粒子系统 II（译名待核）)

- **What it does:** A 2D particle emitter (CycoreFX) with a producer point, physics presets and per-particle size/opacity/colour over life.
- **Exact semantics:**
  - matchName `CC Particle Systems II`; category Simulation; 32 bpc; GPU —.
  - Groups in UI order: Birth Rate, Longevity (sec); Producer {Position, Radius X, Radius Y}; Physics {Animation (`-0009`) {Explosive (default), Fractal Explosive, Twirl, Twirly, Vortex, Fire, ─, Direction, Direction Normalized, ─, Jet Sideways}, Velocity, Inherit Velocity %, Gravity, Resistance, Direction, Extra}; Particle {Particle Type (`-0018`) {Line, Star, ─, Shaded Sphere, Faded Sphere (default UNVERIFIED), Shaded&Faded Sphere, Bubble, ─, Motion Polygon, TriPolygon, QuadPolygon, Cube, TetraHedron, Textured TriPolygon, Textured QuadPolygon, ─, Lens Convex, Lens Concave, Lens Fade, Lens Darken Fade, Lens Bubble}, Texture, Rotation Speed, Initial Rotation, Birth Size, Death Size, Size Variation, Opacity Map (`-0022`) {Constant, Fade Out Sharp, Fade Out, Fade In, Fade In and Out, Oscillate}, Max Opacity, Color Map (`-0024`) {Birth to Death, Origin to Death, Birth to Origin, Origin Constant}, Birth Color, Death Color, Transfer Mode (`-0028`) {Composite, Screen, Add, Black Matte}, Ink}; Random Seed.
  - Simulation contract as AE-FX-643 but in 2D layer pixels.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Shares the particle core with AE-FX-643 in BAE (2D = Z ignored).

### AE-FX-645 · Particle Playground (粒子运动场)

- **What it does:** Adobe's legacy particle system: generators (Cannon, Grid, Layer Exploder, Particle Exploder) create dots, text characters or layer-mapped sprites that move under Gravity, Repel, Wall and Property Mapper rules; the layer itself becomes invisible except for particles.
- **Exact semantics:**
  - matchName `ADBE Playgnd`; category Simulation; 8 bpc; GPU —.
  - Groups in UI order (with "Options…" dialog for text characters and Auto-Orient Rotation): Cannon (发射) {Position, Barrel Radius (negative = circular barrel, positive = square), Particles Per Second (0 = off; Cannon is the only generator on by default), Direction, Direction Random Spread (±half), Velocity (px/s), Velocity Random Spread, Color, Particle Radius (px)}; Grid (网格) {Position, Width, Height, Particles Across, Particles Down, Color, Particle Radius}; Layer Exploder (图层爆炸) {Explode Layer, Radius of New Particles, Velocity Dispersion}; Particle Exploder (粒子爆炸) {Radius of New Particles, Velocity Dispersion, Affects {...}}; Layer Map (图层映射) {Use Layer, Time Offset Type {Relative, Absolute, Relative Random, Absolute Random} (`ADBE Playgnd-0284`), Time Offset, Affects {...}}; Gravity (重力) {Force, Force Random Spread, Direction, Affects}; Repel (排斥) {Force, Force Radius, Repeller, Affects}; Wall (墙) {Boundary (mask), Affects}; Persistent Property Mapper and Ephemeral Property Mapper (永久属性映射器 / 短暂属性映射器) {Use Layer As Map, Affects, Map Red/Green/Blue To (properties such as Kinetic Friction, Mass, Scale, Speed, Opacity...), Min/Max, Operator (Set, Add, Difference, Subtract, Multiply, Min, Max)}.
  - Affects sub-group: Particles from {All, None, Cannon, Grid, Layer Exploder, Particle Exploder, and every "A or B (or C)" combination} (16 values, `ADBE Playgnd-0252`), Selection Map, Characters {None, Selection Text 1, Selection Text 2, Selection Text 3} (`-0254`), Older/Younger than, Age Feather.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Help: very slow; simulation always runs from the layer In point. Chinese group names from the zh-CN help page where shown (发射, 网格, 图层爆炸, 粒子爆炸, 图层映射, 重力, 排斥, 墙), others（译名待核）.

### AE-FX-646 · Other Simulation effects catalog (其他模拟效果)

- **What it does:** Reserves the remaining Simulation effects by matchName so projects round-trip.
- **Exact semantics:**

  | Effect (中文) | matchName | bpc | GPU | Summary |
  |---|---|---|---|---|
  | Card Dance (卡片动画) | `APC CardDanceCam` | 8 | — | Splits the layer into a grid of cards driven by Gradient Layers (Rows & Columns, Back Layer, Gradient Layer 1/2, Rotation Order, Transformation Order, X/Y/Z Position/Rotation/Scale sources, Camera System, Lighting, Material). |
  | Caustics (焦散) | `APC Caustics` | 8 | — | Renders a water surface's refraction/caustics over a Bottom layer (Bottom, Water {Water Surface, Wave Height, Smoothing, Water Depth, Refractive Index, Surface Color, Surface Opacity, Caustics Strength}, Sky, Lighting, Material). |
  | Foam (泡沫) | `APC Foam` | 8 | — | Bubble particle simulation (View, Producer, Bubbles, Physics, Zoom, Universe Size, Rendering, Flow Map, Random Seed). |
  | Shatter (碎片) | `APC Shatter` | 8 | — | Explodes the layer into extruded shapes (View, Render, Shape {Pattern, Custom Shatter Map, Repetitions, Direction, Origin, Extrusion Depth}, Force 1/2, Gradient, Physics, Textures, Camera System, Lighting, Material). |
  | Wave World (波形环境) | `APC Wave World` | 8 | — | 2D wave-propagation height-map generator (View, Wireframe Controls, Height Map Controls, Simulation, Ground, Producer 1/2). |
  | CC Ball Action (CC Ball Action) | `CC Ball Action` | 16 | — | Converts the layer into a grid of shaded 3D balls (Scatter, Rotation, Twist, Grid Spacing, Ball Size, Instability State). |
  | CC Bubbles (CC Bubbles) | `CC Bubbles` | 32 | — | Rising bubbles textured by the layer (Bubble Amount, Speed, Wobble, Bubble Size, Reflection Type, Shading Type). |
  | CC Drizzle (CC Drizzle) | `CC Drizzle` | 32 | — | Raindrop ripples on a surface (Drip Rate, Longevity, Rippling, Displacement, Ripple Height, Spreading, Light, Shading). |
  | CC Hair (CC Hair) | `CC Hair` | 16 | — | Grows hair strands over the layer (Length, Thickness, Weight, Constant Mass, Density, Hairfall Map, Hair Color, Light, Shading). |
  | CC Mr. Mercury (CC Mr. Mercury) | `CC Mr. Mercury` | 32 | — | Liquid-metal blob particles (Radius X/Y, Producer, Direction, Velocity, Birth Rate, Longevity, Gravity, Resistance, Extra, Animation, Blob Influence, Influence Map, Blob Birth/Death Size, Light, Shading). |
  | CC Pixel Polly (CC Pixel Polly) | `CC Pixel Polly` | 16 | — | Shatters the layer into polygons that fly apart (Force, Gravity, Spinning, Force Center, Direction Randomness, Speed Randomness, Grid Spacing, Object {Polygon, Textured Polygon, Square, Textured Square}, Enable Depth Sort, Start Time). |
  | CC Rainfall (CC Rainfall) | `CSRainfall` | 32 | — | Procedural rain streaks (Drops, Size, Scene Depth, Speed, Wind, Variation % (Wind), Spread, Color, Opacity, Background Reflection, Transfer Mode, Composite With Original, Extras). |
  | CC Scatterize (CC Scatterize) | `CC Scatterize` | 16 | — | Scatters the layer's pixels into a cloud (Scatter, Right/Left Twist, Transfer Mode). |
  | CC Snowfall (CC Snowfall) | `CSSnowfall` | 32 | — | Procedural snow (Flakes, Size, Variation % (Size), Scene Depth, Speed, Variation % (Speed), Wind, Spread, Wiggle, Color, Opacity, Background Illumination, Transfer Mode, Composite With Original, Extras). |
  | CC Star Burst (CC Star Burst) | `CC Star Burst` | 16 | — | Flies through a field of spheres textured by the layer (Scatter, Speed, Phase, Grid Spacing, Size, Blend w. Original). |
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** CC Rainfall and CC Snowfall are the most used in templates and are stateless functions of time (good first candidates). Card Dance, Caustics, Foam, Shatter and Wave World are 8 bpc legacy effects sharing the "Camera System / Lighting / Material" control groups.

## Audio effects

Effect > Audio（音频）. Audio effects modify the layer's audio stream (not pixels); they are evaluated by the audio render path used for preview and output (see ui-preview-render-io.md, Audio panel and output-module audio).

### AE-FX-647 · Audio effects catalog (音频效果)

- **What it does:** Reserves and summarises AE's built-in audio effects by matchName; all are applied to layers with audio and are rendered in preview/export audio.
- **Exact semantics:**

  | Effect (中文) | matchName | Parameters (help order) | Summary |
  |---|---|---|---|
  | Backwards (倒放) | `ADBE Aud Reverse` | Swap Channels | Plays the layer's audio from last frame to first; timeline frames unchanged. |
  | Bass & Treble (低音和高音) | `ADBE Aud BT` | Bass, Treble | Shelf boost/cut of low/high frequencies. |
  | Compressor (压缩器) | `ADBE Aud Compressor` | UNVERIFIED (threshold/ratio/attack/release family) | Dynamic-range compressor (added in a 2025 AE release, UNVERIFIED version). |
  | De-esser (消除齿音) | UNVERIFIED (not in the AE 26.0 dump) | UNVERIFIED | Sibilance reducer (help page lists it; newer than AE 26.0). |
  | Delay (延迟) | `ADBE Aud Delay` | Delay Time (ms), Delay Amount, Feedback, Dry Out, Wet Out | Echo: y[n] = dry·x[n] + wet·d[n], d[n] = amount·x[n−D] + feedback·d[n−D] (UNVERIFIED topology). |
  | Distortion (扭曲) | `ADBE Aud Distortion` | UNVERIFIED | Waveshaping distortion (newer effect). |
  | Flange & Chorus (变调与合声) | `ADBE Aud_Flange` | Voice Separation Time (ms), Voices, Modulation Rate, Modulation Depth, Voice Phase Change, Invert Phase, Stereo Voices, Dry Out, Wet Out | Modulated short delays. |
  | Gate (噪声门) | `ADBE Aud Gate` | UNVERIFIED | Noise gate (newer effect). |
  | High-Low Pass (高通/低通) | `ADBE Aud HiLo` | Filter Options {High Pass, Low Pass}, Cutoff Frequency (Hz), Dry Out, Wet Out | Cut filter (slope UNVERIFIED). |
  | Modulator (调制器) | `ADBE Aud Modulator` | Modulation Type {Sine, Triangle}, Modulation Rate (Hz), Modulation Depth, Amplitude Modulation | Vibrato + tremolo. |
  | Parametric EQ (参数均衡) | `ADBE Param EQ` | Frequency Response (graph), Band 1/2/3 Enabled, Frequency, Bandwidth, Boost/Cut | Three peaking bands (band 1 red, 2 green, 3 blue in the graph). |
  | Reverb (混响) | `ADBE Aud Reverb` | Reverb Time (ms), Diffusion, Decay, Brightness, Dry Out, Wet Out | Algorithmic room reverb. |
  | Stereo Mixer (立体声混合器) | `ADBE Aud Stereo Mixer` | Left Level, Right Level, Left Pan, Right Pan, Invert Phase | Per-channel level and pan. |
  | Tone (色调) | `ADBE Aud Tone` | Waveform Options {Sine, Triangle, Saw, Square, White Noise (UNVERIFIED order)}, Frequency 1-5 (Hz; 0 = off), Level | Synthesises up to five tones; works on any layer type except placeholders. |
- **Priority:** P2 (BAE M1 plays layer audio with level only; audio DSP is post-M1).
- **Owner:** bae-media
- **Notes:** The zh-CN help names Tone "色调" (identical to the Tint colour effect's term in some versions)（译名待核）. Audio effect bpc/GPU columns do not apply. Help tip: avoid clicks at the end of a Tone by keyframing Level to 0 on the last frame. Implementation hint: run audio effects in the audio graph at the project sample rate (48 kHz default) with parameters sampled per audio block (keyframes interpolated per block, UNVERIFIED AE block size).

## Text effects

Effect > Text（文本）. Legacy text generators; text layers (text.md) are the modern replacement, and help recommends the "Current Time Format" animation preset on a text layer instead.

### AE-FX-648 · Numbers (数字)

- **What it does:** Draws a number, counter, date, time, timecode or hexadecimal value (sequential, random or current date/time) onto the layer using a chosen font.
- **Exact semantics:**
  - matchName `ADBE Numbers2`; category Text; 8 bpc; GPU —. An "Options…" link opens the Font Style dialog (Font, Style, Direction {Horizontal, Vertical}, Alignment {Left, Center, Right}); the font is stored in the effect's arbitrary data (UNVERIFIED storage).

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Type (类型) | `ADBE Numbers2-0001` | Popup | Number | 1 Number (数字), 2 Number [Leading Zeros] (数字 [前导零]), 3 Timecode [30], 4 Timecode [25], 5 Timecode [24], 6 Time (时间), 7 Numerical Date (数字日期), 8 Short Date (短日期), 9 Long Date (长日期), 10 Hexadecimal (十六进制) | — | Leading Zeros = 5 integer digits. |
  | 2 | Random Values (随机值) | UNVERIFIED | Checkbox | off | — | — | Random within [0, Value]; Value 0 = full range. |
  | 3 | Value/Offset/Random Max (数值/位移/随机最大) | UNVERIFIED | Float slider | 0 | valid ±30000 for sequential offset (help: "maximum offset 30,000") | — | Meaning depends on Type (dates: days since 1 Jan 1995). |
  | 4 | Decimal Places (小数位数) | UNVERIFIED | Integer slider | 3 (UNVERIFIED) | 0-5 (UNVERIFIED) | — | |
  | 5 | Current Time/Date (当前时间/日期) | UNVERIFIED | Checkbox | off | — | — | Uses the wall clock at render time. |
  | 6 | Position (位置) | UNVERIFIED | Point | layer centre | — | layer px | |
  | 7 | Display Options (显示选项) | `ADBE Numbers2-0012` | Popup | Fill Only | 1 Fill Only, 2 Stroke Only, 3 Fill Over Stroke, 4 Stroke Over Fill | — | |
  | 8 | Fill Color (填充颜色) | UNVERIFIED | Color | red (UNVERIFIED) | — | — | |
  | 9 | Stroke Color (描边颜色) | UNVERIFIED | Color | white (UNVERIFIED) | — | — | |
  | 10 | Stroke Width (描边宽度) | UNVERIFIED | Float slider | 2.0 (UNVERIFIED) | — | px | |
  | 11 | Size (大小) | UNVERIFIED | Float slider | 36 (UNVERIFIED) | — | px | |
  | 12 | Tracking (跟踪) | UNVERIFIED | Integer slider | 0 | — | — | Average distance between characters. |
  | 13 | Proportional Spacing (比例间距) | UNVERIFIED | Checkbox | on (UNVERIFIED) | — | — | Off = monospaced digits. |
  | 14 | Composite On Original (在原始图像上合成) | UNVERIFIED | Checkbox | on (UNVERIFIED) | — | — | Off = original not visible. |

  ```
  Number:      text = format(V, DecimalPlaces)           // V = Value/Offset (animated) ; Random: V = rand(frame) * Max
  Timecode[N]: text = HH:MM:SS:FF of (layerTime + V) at N fps, non-drop
  Time:        HH:MM from V (or wall clock if Current Time/Date)
  Dates:       date = 1995-01-01 + V days (or today + V days with Current Time/Date); leap years honoured
  Hexadecimal: h = floor(V * 0x10000) (step 0x1 per 0.0000125 ≈ 1/80000 — help; note 1/65536 ≈ 0.0000153, UNVERIFIED which is exact)
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-text)
- **Notes:** Current Time/Date makes renders non-deterministic. BAE decision: render the string through bae-text layout so CJK fonts work; store the font as a normal effect parameter (font reference) rather than opaque data.

### AE-FX-649 · Timecode (时间码)

- **What it does:** Burns in a timecode, frame number or film feet+frames readout over a box on the layer.
- **Exact semantics:**
  - matchName `ADBE Timecode`; category Text; 8 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Display Format (显示格式) | `ADBE Timecode-0002` | Popup | SMPTE HH:MM:SS:FFF | 1 SMPTE HH:MM:SS:FFF (menu label as stored in AE 26.0; the frame field has as many digits as needed), 2 Frame numbers, 3 Feet + Frames (35mm), 4 Feet + Frames (16mm) | — | 35mm = 16 frames/foot, 16mm = 40 frames/foot. |
  | 2 | Time Source (时间源) | `ADBE Timecode-0009` | Popup | Layer Source (UNVERIFIED) | 1 Layer Source (图层源), 2 Composition (合成), 3 Custom (自定义) | — | Added in AE 22.x (UNVERIFIED version). |
  | 3 | Time Units (时间单位) | UNVERIFIED | Float slider | comp frame rate (UNVERIFIED) | — | fps | Custom only; display only. |
  | 4 | Drop Frame (丢帧) | UNVERIFIED | Checkbox | off | — | — | Custom only; meaningful at 29.97/59.94. |
  | 5 | Starting Frame (开始帧) | UNVERIFIED | Integer | 0 | — | frames | Custom only; number of the layer's first frame. |
  | 6 | Text Position (文本位置) | UNVERIFIED | Point | lower centre of layer (UNVERIFIED) | — | comp px (help) | |
  | 7 | Text Size (文字大小) | UNVERIFIED | Float slider | 48 (UNVERIFIED) | — | points | |
  | 8 | Text Color (文本颜色) | UNVERIFIED | Color | white | — | — | |
  | 9 | Show Box (显示方框) | UNVERIFIED | Checkbox | on | — | — | |
  | 10 | Box Color (方框颜色) | UNVERIFIED | Color | black | — | — | |
  | 11 | Opacity (不透明度) | UNVERIFIED | Float slider | 100% | 0-100 | % | Box opacity. |
  | 12 | Composite On Original (在原始图像上合成) | UNVERIFIED | Checkbox | on | — | — | Off: on transparent layer. |

  ```
  Layer Source: t = source footage timecode at the current source frame (embedded start TC + frame)
  Composition:  t = comp time incl. comp start timecode (see project-comp-layers.md, Composition Settings start timecode)
  Custom:       frame = StartingFrame + floor(layerTime * TimeUnits + ε); format with Drop Frame rules (SMPTE DF: skip frames 0,1 each minute except every 10th, scaled for 59.94)
  Feet+Frames:  feet = floor(frame / k), ff = frame mod k ; k = 16 (35mm) or 40 (16mm)
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-text)
- **Notes:** Help: does not modify embedded timecode metadata. Timecode formatting must share code with the app's time display (ui-preview-render-io.md).

## Utility effects

Effect > Utility（实用工具）. Colour-pipeline helpers; the colour maths they rely on (working space, linearisation, ICC transforms) is specified in compositing-3d-color.md, Color management.

### AE-FX-650 · Apply Color LUT (应用颜色 LUT)

- **What it does:** Transforms the layer's colours through a 1D/3D lookup table loaded from a file.
- **Exact semantics:**
  - matchName `ADBE Apply Color LUT2` (old `ADBE Apply Color LUT` in _Obsolete); category Utility; 32 bpc; GPU —.
  - Controls: a "Choose LUT…" button (selecting the file is prompted on apply). The LUT data is embedded in the effect so the project renders without the file (UNVERIFIED: embedded vs path only). Further parameters (e.g. interpolation) UNVERIFIED.
  - Supported formats (help): `.3dl` (incl. floating-point values, 3DMESH/Mesh keywords, ASSIMILATE SCRATCH files), IRIDAS `.cube` (incl. over-range), IRIDAS `.look`, cineSpace `.csp`. 3D LUTs must have all three axes the same size.

  ```
  // .cube semantics (Resolve/IRIDAS): LUT_3D_SIZE N, DOMAIN_MIN/MAX (default 0..1), data with R varying fastest
  in = straight (un-premultiplied) RGB of the layer, in the working space as-is (no implicit transform)
  if LUT_1D: per channel linear interpolation over N entries
  if LUT_3D: u = (in - domainMin) / (domainMax - domainMin) * (N-1); clamp u to [0, N-1]; trilinear interpolation of the 8 surrounding entries   // UNVERIFIED: AE trilinear vs tetrahedral
  .3dl: integer outputs scaled by max code value (e.g. 4095 for 12-bit, 1023 for 10-bit) unless floating point; input grid from the first line
  out = premultiply(lut(in), alpha); values outside the domain clamp at the edge
  ```
- **Priority:** P1 (very common for log footage and looks).
- **Owner:** bae-color (+ bae-effects)
- **Notes:** Help: AE normally colour-manages with ICC profiles, not LUTs. Implementation hint: upload the LUT as a 3D texture with hardware trilinear filtering (sample at texel centres: `(u + 0.5)/N`); 1D LUTs as 1D textures. BAE decision: also accept the same file formats in Interpret Footage for OCIO-subset workflows (see bae-color).

### AE-FX-651 · Color Profile Converter (颜色配置文件转换器)

- **What it does:** Converts the layer from an input ICC colour profile to an output profile with a rendering intent, independently of project colour management.
- **Exact semantics:**
  - matchName `ADBE ProfileToProfile`; category Utility; 32 bpc; GPU —. Profiles used are embedded in the project.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Input Profile (输入配置文件) | `ADBE ProfileToProfile-0001` | Popup | Project Working Space | 1 Project Working Space, 2 separator, then installed profiles alphabetically (Adobe RGB (1998), Apple RGB, ColorMatch RGB, image P3, ProPhoto RGB, sRGB IEC61966-2.1, … machine-dependent) | — | Stored index depends on the machine's profile list; AE also stores the profile itself (UNVERIFIED mechanism). |
  | 2 | Linearize Input Profile (线性化输入配置文件) | `-0002` (UNVERIFIED) | Checkbox | off | — | — | Uses the linear-gamma version of the profile. |
  | 3 | Output Profile (输出配置文件) | `-0003` | Popup | Project Working Space | as Input Profile | — | |
  | 4 | Linearize Output Profile (线性化输出配置文件) | `-0004` (UNVERIFIED) | Checkbox | off | — | — | |
  | 5 | Intent (方法) | `-0005` | Popup | Relative Colorimetric | 1 Perceptual (感性), 2 Saturation (饱和度), 3 Relative Colorimetric (相对色度), 4 Absolute Colorimetric (绝对色度) | — | Relative Colorimetric is AE's default intent everywhere. |
  | 6 | Use Black Point Compensation (使用黑场补偿) | `-0006` (UNVERIFIED) | Checkbox | on | — | — | |
  | 7 | Scene-ref. Profile Compensation (场景参考配置文件补偿) | `-0007` | Popup | Use Project Setting | 1 Use Project Setting (使用项目设置), 2 On (开), 3 Off (关) | — | |

  ```
  rgb_lin = profileToXYZ(InputProfile, linearized?)(straight rgb)        // ICC matrix/TRC or LUT-based
  rgb_out = XYZtoProfile(OutputProfile, linearized?)(chromaticAdapt(rgb_lin), intent, BPC)
  // 32 bpc: matrix/TRC profiles are applied unclamped (extended range) (UNVERIFIED for LUT profiles)
  ```
- **Priority:** P1
- **Owner:** bae-color (+ bae-effects)
- **Notes:** Help warns against mixing it with project colour management. Labels of parameters 2, 4, 6 and their Chinese strings（译名待核）.

### AE-FX-652 · Grow Bounds (范围扩散)

- **What it does:** Enlarges the layer's effect buffer by a number of pixels on every side so that the next effect can draw outside the original bounds (useful on collapsed / continuously rasterized layers and with 8/16-bpc effects that do not expand bounds themselves).
- **Exact semantics:**
  - matchName `ADBE GROW BOUNDS`; category Utility; 32 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Pixels (像素) | `ADBE GROW BOUNDS-0001` | Float slider (integer pixels) | 0 (UNVERIFIED) | slider 0-1000 (UNVERIFIED), valid 0-30000 (UNVERIFIED) | px | |

  ```
  out_rect = in_rect expanded by Pixels on left, top, right, bottom (scaled by preview downsample factor)
  out pixels = input pixels; new area = transparent black
  ```
- **Priority:** P1
- **Owner:** bae-render (+ bae-effects)
- **Notes:** Help: unnecessary for 32-bpc effects, which compensate automatically. Lottie: exported as `ty: 5`, `mn: "ADBE GROW BOUNDS"` (seen in exported files); players ignore it. BAE decision: BAE effects declare their own output bounds, so Grow Bounds is mostly a no-op except for layers whose buffer is the comp rectangle (collapse/continuous rasterize); it must still be honoured for imported projects.

### AE-FX-653 · Cineon Converter (Cineon 转换器)

- **What it does:** Converts between 10-bit log (Cineon/DPX printing density) and linear values with adjustable black/white points, gamma and highlight rolloff.
- **Exact semantics:**
  - matchName `ADBE Cineon Converter2` (old `ADBE Cineon Converter` in _Obsolete; old enum options appear for old projects); category Utility; 32 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Conversion Type (转换类型) | `ADBE Cineon Converter2-0001` | Popup | Log to Linear | 1 Linear to Log, 2 Log to Linear, 3 Log to Log | — | |
  | 2 | 10 Bit Black Point (10 位黑场) | `-0002` (UNVERIFIED) | Integer slider | 95 | 0-1023 | 10-bit code | |
  | 3 | Internal Black Point (内部黑场) | `-0003` (UNVERIFIED) | Float slider | 0 | 0-1 (UNVERIFIED) | — | |
  | 4 | 10 Bit White Point (10 位白场) | `-0004` (UNVERIFIED) | Integer slider | 685 | 0-1023 | 10-bit code | |
  | 5 | Internal White Point (内部白场) | `-0005` (UNVERIFIED) | Float slider | 1.0 | 0-1 (UNVERIFIED) | — | |
  | 6 | Gamma (灰度系数) | `-0006` (UNVERIFIED) | Float slider | 1.70 | 0.1-5 (UNVERIFIED) | — | |
  | 7 | Highlight Rolloff (高光滤除) | `-0007` (UNVERIFIED) | Integer slider | 20 (UNVERIFIED) | 0-100 (UNVERIFIED) | 10-bit codes | |

  ```
  // Standard Kodak conversion (UNVERIFIED how AE folds Gamma and Rolloff in)
  k = 0.002 / 0.6 * (Gamma/1.7)                      // density per code / negative gamma, scaled by display gamma
  offset = 10^((Black10 - White10) * k)
  Log->Lin: lin = (10^((code10 - White10) * k) - offset) / (1 - offset)
            lin = IntBlack + lin * (IntWhite - IntBlack)
            highlights within Rolloff codes below White10 are compressed with a soft knee (UNVERIFIED curve); 32 bpc keeps >1.0
  Lin->Log: inverse of the above ; Log->Log: remaps black/white points between two log encodings
  ```
- **Priority:** P2
- **Owner:** bae-color (+ bae-effects)
- **Notes:** Help: prefer colour-managed Cineon interpretation (see project-comp-layers.md, Interpret Footage, and compositing-3d-color.md, Color management).

### AE-FX-654 · HDR Compander (HDR 压缩扩展器)

- **What it does:** Compresses 32-bpc HDR values into 0-1 before low-dynamic-range effects and expands them back afterwards (used as a pair).
- **Exact semantics:**
  - matchName `ADBE Compander`; category Utility; 32 bpc; GPU —.

  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Mode (模式) | `ADBE Compander-0001` | Popup | Compress Range | 1 Compress Range (压缩范围), 2 Expand Range (扩展范围) | — | |
  | 2 | Gain (增益) | `-0002` (UNVERIFIED) | Float slider | 1.0 (UNVERIFIED) | slider 1-100 (UNVERIFIED) | — | Highest white value to represent. |
  | 3 | Gamma (灰度系数) | `-0003` (UNVERIFIED) | Float slider | 1.0 (UNVERIFIED) | 0.1-10 (UNVERIFIED) | — | Same values on both instances. |

  ```
  Compress: v' = (v / Gain)^(1/Gamma)      per colour channel, straight colour (UNVERIFIED exact form)
  Expand:   v  = Gain * v'^Gamma
  ```
- **Priority:** P2
- **Owner:** bae-color (+ bae-effects)
- **Notes:** Help: the "Compress-Expand Dynamic Range" animation preset links the two instances with an expression. Chinese parameter strings（译名待核）.

### AE-FX-655 · HDR Highlight Compression (HDR 高光压缩)

- **What it does:** Compresses HDR values so they fit (at Amount 100%) into 0-1, bringing highlight detail into the visible range.
- **Exact semantics:**
  - matchName `ADBE HDR ToneMap`; category Utility; 32 bpc; GPU —.
  - Single parameter Amount (数量) `ADBE HDR ToneMap-0001` (UNVERIFIED matchName), Float slider, default 100% (UNVERIFIED), 0-100 %.

  ```
  // UNVERIFIED reconstruction: global tone curve chosen so that max channel of the frame maps to 1.0 at Amount 100
  m = max over pixels of max(r,g,b)
  c = (m > 1) ? curve(c; m) : c          // e.g. c' = c / (1 + c*(1 - 1/m)) keeps 0..1 nearly unchanged and maps m -> 1
  out = lerp(c, c', Amount/100)
  ```
- **Priority:** P2
- **Owner:** bae-color (+ bae-effects)
- **Notes:** Frame-global analysis (needs the whole frame before output) — UNVERIFIED that AE analyses per frame rather than using a fixed curve.

### AE-FX-656 · CC Overbrights (CC 过亮显示（译名待核）)

- **What it does:** Visualises pixel values outside 0-1 in 32-bpc projects (shows which areas are over-range).
- **Exact semantics:**
  - matchName `CC Overbrights`; category Utility; 32 bpc; GPU —.
  - Channel (`CC Overbrights-0001`) {1 RGB Solid, 2 Lightness Solid, 3 Luminance Solid, 4 separator, 5 RGB Clip, 6 Lightness Clip, 7 Luminance Clip}; Clip Color (UNVERIFIED name/default).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Diagnostic only; BAE may offer the same as a viewer overlay instead. Heading Chinese name not localised（译名待核）.

## Obsolete and Immersive Video effects

### AE-FX-657 · Obsolete effects catalog (过时效果)

- **What it does:** Lists the effects AE still ships in the visible Obsolete category (过时) for compatibility with old projects, plus the rule for the hidden `_Obsolete` category.
- **Exact semantics:**

  | Effect (中文) | matchName | bpc | GPU | Summary / replacement |
  |---|---|---|---|---|
  | Basic 3D (基本 3D) | `ADBE Basic 3D` | 8 | — | Swivel, Tilt, Distance to Image, Specular Highlight, Preview: pseudo-3D plane rotation; replaced by 3D layers. |
  | Basic Text (基本文字) | `ADBE Basic Text2` | 8 | — | Draws text from a dialog (Fill and Stroke, Size, Tracking, Line Spacing, Composite On Original); replaced by text layers. |
  | Color Key (颜色键) | `ADBE Color Key` | 16 | — | See AE-FX-632. |
  | Fast Blur (Legacy) (快速模糊（旧版）) | `ADBE Fast Blur` | 32 | — | Blurriness, Blur Dimensions, Repeat Edge Pixels; see effects.md [fx-b] Blur & Sharpen for the current blur. |
  | Gaussian Blur (Legacy) (高斯模糊（旧版）) | `ADBE Gaussian Blur` | 32 | — | Blurriness, Blur Dimensions; replaced by `ADBE Gaussian Blur 2` (fx-b). |
  | Lightning (闪电) | `ADBE Lightning` | 8 | — | Old lightning generator; replaced by Advanced Lightning (AE-FX-616). |
  | Luma Key (亮度键) | `ADBE Luma Key` | 16 | — | See AE-FX-632. |
  | Path Text (路径文本) | `ADBE Path Text` | 8 | — | Text along a Bezier/circle/loop/line path with per-character jitter; replaced by text on a path (text.md). |
  | Reduce Interlace Flicker (减少交错闪烁) | `ADBE Reduce Interlace Flicker` | 32 | — | Softness: vertical blur to reduce interlace twitter. |
  | Spill Suppressor (溢出抑制器) | `ADBE Spill Suppressor` | 32 | — | See AE-FX-632. |
  | mocha shape (mocha shape) | `ISL MochaShapeImporter` | 32 | — | See AE-FX-626; the AE 26.0 dump lists it under Obsolete rather than Matte. |

  - Hidden `_Obsolete` category: older matchName versions (e.g. `ADBE Noise`, `ADBE Noise Alpha`, `ADBE Apply Color LUT`, `ADBE Cineon Converter`, `CC Time Blend`) never appear in the Effects menu but must load, render and keep their parameters when opened from old projects; scripting `app.effects` lists them with category `_Obsolete` (AE 26.0 dump).
- **Priority:** P2
- **Owner:** bae-effects (+ bae-plugin)
- **Notes:** BAE decision: when a newer equivalent exists, map the obsolete matchName onto the new BAE implementation with a parameter adapter, but keep the original matchName on save so round-tripping to AE is lossless. The effect framework rule for unknown matchNames (placeholder that keeps parameters) is in effects.md [fx-a]. Chinese names of Basic Text, Path Text and Reduce Interlace Flicker（译名待核）.

### AE-FX-658 · Immersive Video effects catalog (沉浸式视频效果)

- **What it does:** Lists the VR / 360° effects (Effect > Immersive Video) that operate on equirectangular footage.
- **Exact semantics:**

  | Effect (中文) | matchName | Summary |
  |---|---|---|
  | VR Blur (VR 模糊) | `Mettle SkyBox Blur` | Seam-aware blur on equirectangular images. |
  | VR Chromatic Aberrations (VR 色差) | `Mettle SkyBox Chromatic Aberrat` | Seam-aware chromatic aberration. |
  | VR Color Gradients (VR 颜色渐变) | `Mettle SkyBox Color Gradients` | Gradient generator with points on the sphere. |
  | VR Converter (VR 转换器) | `Mettle SkyBox Converter` | Converts between equirectangular, cube-map and other 360° layouts and rotates the sphere. |
  | VR De-Noise (VR 降噪) | `Mettle SkyBox Denoise` | Seam-aware denoise. |
  | VR Digital Glitch (VR 数字故障) | `Mettle SkyBox Digital Glitch` | Glitch effect for 360° footage. |
  | VR Fractal Noise (VR 分形杂色) | `Mettle SkyBox Fractal Noise` | Fractal noise mapped seamlessly onto the sphere. |
  | VR Glow (VR 发光) | `Mettle SkyBox Glow` | Seam-aware glow. |
  | VR Plane to Sphere (VR 平面到球面) | `Mettle SkyBox Project 2D` | Projects a flat layer onto the sphere. |
  | VR Rotate Sphere (VR 旋转球面) | `Mettle SkyBox Rotate Sphere` | Tilt/Pan/Roll of the equirectangular image. |
  | VR Sharpen (VR 锐化) | `Mettle SkyBox Sharpen` | Seam-aware sharpen. |
  | VR Sphere to Plane (VR 球面到平面) | `Mettle SkyBox Viewer` | Renders a flat camera view from the sphere. |
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** matchNames from the AE 26.0 effect dump (the docsforadobe first-party table has no Immersive Video section, so bpc/GPU values are UNVERIFIED; believed 32 bpc with GPU). All Chinese names（译名待核）. Implementation hint: implement once as "equirect-aware sampling" (wrap horizontally, pole-aware vertically) that the existing blur/glow/sharpen kernels can reuse.
