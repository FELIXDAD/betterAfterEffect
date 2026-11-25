# After Effects inventory: compositing, 3D and color (AE-CMP)

**Scope.** This file specifies how After Effects turns evaluated layers into pixels: the pixel and alpha model and bit-depth clamping, per-layer and per-comp render order, every blend mode with exact formulas, opacity and Preserve Underlying Transparency, track mattes, masks, adjustment layers, Collapse Transformations / Continuously Rasterize, nested-comp resolution and frame rate, quality / sampling / subpixel positioning, layer bounds, motion-blur and frame-blend rendering, the Classic 3D renderer (depth sort, intersections, 2D/3D grouping), cameras, lights, material options, color management, and preview resolution / region of interest / fast previews / transparency grid / channel views. Sibling files: README.md, project-comp-layers.md, transform-keyframes-expressions.md, compositing-3d-color.md, shapes.md, text.md, effects.md, ui-preview-render-io.md. The item format, ID scheme, priorities and owners are defined in README.md.

**Conventions used in this file.** Colour values are written normalised: 0.0 = black / transparent, 1.0 = white / opaque, whatever the project bit depth. `Cb`, `ab` = backdrop (composite of the layers below) colour and alpha; `Cs`, `as` = source (the layer being composited) colour and alpha; colours are *straight* (unmultiplied) unless the name ends in `p` (`cbp = Cb*ab`, `csp = Cs*as`). `Lum(C) = 0.30*R + 0.59*G + 0.11*B` unless stated otherwise. `clamp01(x) = min(max(x, 0), 1)`.

## Pixel and alpha model

### AE-CMP-001 · Internal pixel format and straight alpha (像素格式和直接 Alpha（译名待核）)

- **What it does:** Defines how AE stores the pixels of every layer buffer, effect buffer and comp buffer: four channels A, R, G, B with colour stored *straight* (not multiplied by alpha).
- **Exact semantics:**
  - Channel order in AE memory is ARGB (alpha first). 8 bpc: `PF_Pixel8 {u8 alpha, red, green, blue}`; 16 bpc: `PF_Pixel16 {u16 alpha, red, green, blue}`; 32 bpc: `PF_PixelFloat {f32 alpha, red, green, blue}` (plug-in SDK, Color space conversion / PF_EffectWorld).
  - Colour in layer and effect buffers is straight (unmultiplied): a pixel with alpha 0 may carry arbitrary RGB, and that RGB is preserved through the pipeline when an effect asks for it (SDK: SmartFX `preserve_rgb_of_zero_alpha`, `PF_OutFlag2_REVEALS_ZERO_ALPHA` — effects such as Set Channels can make zero-alpha pixels visible again, which is only possible with straight storage).
  - Footage with a premultiplied alpha is unmultiplied on import according to Interpret Footage (Premultiplied with colour X): `C = (Cp - (1 - a) * X) / a` for `a > 0`; see project-comp-layers.md, Interpret Footage. Output modules re-premultiply on export when Channels = RGB+Alpha and Color = Premultiplied (with black or the comp background colour); see ui-preview-render-io.md, Render Queue.
  - All compositing formulas in this file are stated on straight colour; the generic compositing equation (AE-CMP-012) gives the premultiplied form used for implementation.
  - Edge case: unmultiplying a pixel with `a == 0` yields RGB = 0 unless the RGB was preserved; implementations must never divide by zero.
- **Priority:** P0 — every compositing formula depends on it.
- **Owner:** bae-render (+ bae-color, bae-effects)
- **Notes:** UNVERIFIED: the AE plug-in SDK guide does not state in one sentence that "AE pixels are straight"; the straight-alpha conclusion is derived from `preserve_rgb_of_zero_alpha`, `PF_OutFlag2_REVEALS_ZERO_ALPHA`, `AEGP_CopyBits_HQ_Straight` vs `_Premul`, and long-standing third-party knowledge (AE internally works unmultiplied). Implementation hint: BAE should store GPU working buffers premultiplied (better filtering, cheaper Normal blend, correct bilinear sampling) and unmultiply only inside shaders that need straight colour (non-Normal blend modes, effects declared as straight-input, `sampleImage()`); the visible result must equal AE's straight pipeline. BAE decision: zero-alpha RGB preservation is needed only for the few effects that reveal zero alpha (Set Channels, Channel Combiner, Shift Channels, Remove Color Matting inputs); such effects receive a side "straight RGB" buffer only when flagged, not by default.

### AE-CMP-002 · 8 bits per channel pixels (8 bpc（每通道 8 位）)

- **What it does:** In an 8-bpc project every buffer stores integer channel values 0-255 and every operation result is clamped and rounded to that range.
- **Exact semantics:**
  - Code value `v8 ∈ {0..255}`; normalised `x = v8 / 255`. White = 255.
  - After every pixel-producing step (footage decode, effect output, mask application, layer transform resample, blend with the layers below, comp output) the result is clamped to [0, 255] and rounded to integer. Overbright (>1.0) and negative values cannot exist.
  - Rounding: UNVERIFIED: round-half-up (`floor(x*255 + 0.5)`) for stored results; AE's internal fixed-point maths (e.g. 8-bit multiply `(a*b + 128) / 255` style) can differ by ±1 code value from float maths.
  - 8-bpc effects in a 16- or 32-bpc project: the effect's input is converted down to 8 bpc (clamped and quantised) and its output converted back; the effect icon in Effect Controls shows a warning in higher-depth projects. See effects.md, effect framework.
  - Conversion to 8-bpc output when the project is 16 or 32 bpc applies dithering at render/export time, not in previews (Adobe help, Color basics).
- **Priority:** P0 — default project depth.
- **Owner:** bae-render (+ bae-color)
- **Notes:** Project bit depth itself (Project Settings > Color > Bit Depth, Alt/Option-click the bpc button in the Project panel) is specified in compositing-3d-color.md, Color management. Scripting: `app.project.bitsPerChannel` (8, 16, 32). Lottie has no bit-depth concept (players render 8 bpc).

### AE-CMP-003 · 16 bits per channel pixels (16 bpc（每通道 16 位）)

- **What it does:** In a 16-bpc project buffers store integer channel values 0-32768 ("15+1 bit") with the same clamping points as 8 bpc but much finer quantisation.
- **Exact semantics:**
  - Code value `v16 ∈ {0..32768}`; normalised `x = v16 / 32768`; white = 32768 (not 65535). (Adobe help, Color basics; SDK `PF_MAX_CHAN16` = 32768, slider display "0-32768".)
  - Clamping points identical to 8 bpc (AE-CMP-002): every step's result clamped to [0, 32768] and quantised.
  - Conversion 8 → 16: `v16 = round(v8 * 32768 / 255)`; 16 → 8: `v8 = round(v16 * 255 / 32768)` (UNVERIFIED exact rounding).
  - Files with 16-bit channels (0-65535) are rescaled on import: `v16 = round(vfile * 32768 / 65535)` (UNVERIFIED rounding).
- **Priority:** P1 — common for gradients/banding avoidance; BAE gets it almost for free from float buffers.
- **Owner:** bae-render (+ bae-color)
- **Notes:** The extra code value means 16-bpc midpoint grey = 16384. Info panel can display values as 0-1, 0-255, 0-32768 or percent; see ui-preview-render-io.md, Info panel.

### AE-CMP-004 · 32 bits per channel floating-point pixels (32 bpc 浮点（每通道 32 位）)

- **What it does:** In a 32-bpc project buffers store IEEE-754 single floats; values above 1.0 (overbright / over-range) and below 0.0 are kept through layers, effects, blend modes and nested comps.
- **Exact semantics:**
  - `x ∈ (-∞, +∞)` per colour channel, white = 1.0. Alpha is stored as float too; UNVERIFIED: AE keeps alpha within [0, 1] at compositing steps (opacity, masks and mattes multiply alpha by values in [0, 1]), but individual 32-bpc effects may output alpha outside [0, 1].
  - No clamp is applied between steps; clamping happens only where an operation is defined that way (e.g. some blend modes clamp, see AE-CMP-012 onward), at conversion to 8/16 bpc effects, at display (viewer shows values clipped to the display range unless the HDR/exposure tools are used) and at output to an integer format.
  - Blend modes, glows and blurs can look different from 8/16 bpc because inputs are not clamped (Adobe help, Color basics: "some blending modes behave differently in 32-bpc mode").
  - NaN / Inf: UNVERIFIED: AE does not sanitise NaN produced by effects; they propagate.
- **Priority:** P0 — BAE's float-only GPU pipeline makes this the natural native mode, and HDR is a core BAE goal.
- **Owner:** bae-render (+ bae-color)
- **Notes:** Linear-light blending (Blend Colors Using 1.0 Gamma / Linearize Working Space) is specified in compositing-3d-color.md, Color management; it is independent of bit depth but most useful at 32 bpc. Implementation hint: GPU half floats (RGBA16F) have max 65504 and ~3 decimal digits of precision; this is enough for display but differs from AE 32 bpc for very large or tiny values — offer RGBA32F as the "32 bpc" mode and RGBA16F as the default.

### AE-CMP-005 · Emulating 8/16 bpc clamping in BAE float buffers (在浮点缓冲中模拟 8/16 bpc 截断（译名待核）)

- **What it does:** Makes BAE's float GPU pipeline produce the same numbers as AE when the project is set to 8 or 16 bpc.
- **Exact semantics:**
  - BAE working buffers are RGBA16F by default or RGBA32F (ARCHITECTURE 5). When the project depth is 8 or 16 bpc, every place listed in AE-CMP-002 as a clamping point applies `x = clamp01(x)` to all four channels (straight colour) before the result is stored.
  - Optional quantisation (fidelity mode): `x = round(x * N) / N` with `N = 255` (8 bpc) or `N = 32768` (16 bpc), applied at the same points. Default: clamp only, no quantisation (UNVERIFIED that quantisation differences are visible enough to matter; they cause ±1 code value differences).
  - Clamping is applied to straight colour; when buffers are premultiplied, clamp as `cp = min(max(cp, 0), a)` and `a = clamp01(a)`, which is equivalent.
  - Clamping points (minimum set): footage decode output; each effect output; output of mask/matte/opacity stage; each blend with the layers below; nested comp output; the final comp output.
- **Priority:** P0 — without it 8-bpc projects (the AE default) render differently, e.g. Add/Screen/Color Dodge chains and Levels after a Glow.
- **Owner:** bae-render (+ bae-color, bae-effects)
- **Notes:** BAE decision: clamping is a pipeline mode, not a separate buffer format, so 8/16/32 bpc share one shader set with a `clamp_mode` uniform. Implementation hint: fuse the clamp into the blend shader's final store; effects compiled with a `CLAMP` specialization constant.

## Comp render order

### AE-CMP-006 · Composition stacking and layer participation (合成中的图层堆积顺序（译名待核）)

- **What it does:** A comp frame is built by rendering its layers from the bottom of the Timeline stack to the top, each layer compositing onto the result of the layers below it.
- **Exact semantics:**
  ```
  render_comp(comp, t):
    acc = transparent (RGBA 0,0,0,0), size = comp size * resolution factor
    for layer in comp.layers ordered from highest index (bottom) to index 1 (top):
      if !participates(layer, t): continue
      if layer is adjustment layer:  acc = apply_adjustment(layer, acc, t)   # AE-CMP-049 Adjustment layers
      elif layer is 3D and the renderer groups it: consecutive 3D layers form one 3D group,
           rendered together (depth sorted / intersected) and composited as a unit (see compositing-3d-color.md, Classic 3D renderer)
      else: acc = composite(layer_pixels(layer, t), acc, layer.blend_mode, layer.preserve_transparency)
    return acc            # comp background colour is NOT part of acc (AE-CMP-010)
  participates(layer, t) =
      layer.video_enabled (eye switch)                       # matte layers: see Track mattes
      and layer.in_point <= t < layer.out_point
      and (no layer in comp is soloed, or layer.solo)
      and layer has visual content (not audio-only, not camera/light/null-without-source)
      and not (layer.guide_layer and this render is a nested-comp render or a final render)
  ```
  - Layer index 1 is the top of the stack. A layer whose in/out excludes `t` contributes nothing (also no adjustment, no matte).
  - Solo affects only the comp being viewed/rendered at that level; the soloed state of layers in a nested comp applies inside that nested comp. UNVERIFIED: solo in a nested comp is honoured when rendering the outer comp.
  - Shy switch and "hide shy layers" never affect rendering.
  - Guide layers render in the comp's own viewer, but are excluded from final render output and from the nested-comp render when the comp is used as a layer in another comp (Adobe help, Creating layers: guide layers). Render Settings has "Guide Layers: Current Settings / All Off" (see ui-preview-render-io.md, Render Queue).
  - Nested comps (precomp layers) are rendered before they are composited (AE renders the nested comp, at the time mapped through the precomp layer's time stretch/remap, then treats it as footage). See AE-CMP-054 Nested comps.
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Stacking order editing, layer switches and solo semantics as data are in project-comp-layers.md, Layer switches / Stacking. Lottie: `layers[0]` is the top layer, players render from the last array element to the first; layers with `hd: true` are hidden; `ip`/`op` are in frames. Implementation hint: build the per-frame render graph from the EvaluatedComp once, skipping non-participating layers before any GPU work; group consecutive 2D layers with Normal mode and no matte into one batched draw pass.

### AE-CMP-007 · Per-layer render pipeline for raster layers (栅格图层的渲染顺序（译名待核）)

- **What it does:** Fixes the order in which a non-vector (footage, solid, precomp without collapse) layer's own operations are applied before it is composited.
- **Exact semantics:**
  ```
  layer_pixels(layer, t):                      # all in layer (source) pixel space until step 4
    1. src  = source frame at layer-local time (time stretch / time remap / frame blending,
              Interpret Footage, proxy); precomp = nested comp render (AE-CMP-054)
    2. src  = apply masks top-to-bottom (AE-CMP-038 Masks) -> alpha multiplied by mask result
    3. src  = apply effects top-to-bottom (Effect Controls order), each effect sees the previous
              effect's output; effects may enlarge the buffer (Grow Bounds etc.)
    4. img  = transform into comp space: anchor point, position, scale, rotation / orientation,
              opacity (alpha *= opacity/100), with motion-blur sub-samples if MB is on
              (AE-CMP-064 Motion blur); resampling per Quality/Sampling (AE-CMP-059..061)
    5. img  = layer styles (drop shadow, strokes, ... ; see project-comp-layers.md, Layer styles)
    6. img  = track matte: img.alpha *= matte value (AE-CMP-034 Track mattes)
    7. acc  = blend img onto acc with blend mode and Preserve Underlying Transparency
  ```
  - Adobe help (Precomposing > Render order): "Within each raster layer, elements are applied in the following order: masks, effects, transformations, and layer styles." Masks within the Masks group and effects within the Effects group are processed top to bottom.
  - Steps 6-7: UNVERIFIED exact position of the track matte relative to layer styles; observed AE behaviour is that a track matte also cuts the layer's styles (e.g. a drop shadow outside the matte disappears), i.e. matte after styles.
  - Opacity is part of the Transform group but acts as an alpha multiplier at step 4; it is applied before the blend mode (AE-CMP-031 Opacity).
  - What the Layer viewer shows is the result before step 4 (its View menu can show the source, the masks stage, or any effect stage).
  - The Transform effect (Distort) lets the user insert an extra transform among effects; it does not move the layer's own Transform step.
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim, bae-effects)
- **Notes:** Some effects ignore the layer's masks (they read the unmasked source, e.g. effects that take a "layer" parameter read that layer's source or masks-and-effects per their own setting); Adobe help recommends precomposing in that case. Lottie: masks (`masksProperties`) are applied before effects (`ef`) in lottie-web; layer styles (`sy`) are mostly unsupported by players. Implementation hint: steps 1-3 run at layer resolution in a layer-sized texture; step 4 is one resampling draw into a comp-sized (or ROI-sized) target, fused with opacity, matte and blend when no layer style is present.

### AE-CMP-008 · Per-layer render pipeline for continuously rasterized layers (连续栅格化图层的渲染顺序（译名待核）)

- **What it does:** For vector layers that are continuously rasterized (shape, text, and AI/PDF/EPS footage with the switch on), the transform is applied before effects so vectors are rasterised sharp at their final size.
- **Exact semantics:**
  ```
  layer_pixels_cr(layer, t):
    1. vector content (shape contents / text / vector file) evaluated in layer space
    2. masks top-to-bottom (mask paths are in layer space; they move with the layer)
    3. rasterise vectors + masks directly into comp space using the full layer transform
       (with motion-blur sub-samples) and opacity; anti-aliased in Best quality
    4. effects top-to-bottom, operating on the comp-space raster
    5. layer styles, 6. track matte, 7. blend (as AE-CMP-007)
  ```
  - Adobe help (Layers > Continuous rasterization): default order is "masks, followed by transformations, and then effects" for continuously rasterized layers versus "masks, effects, transformations" for other layers, so effect results can differ (e.g. a blur radius is not scaled by layer scale; directional parameters do not rotate with the layer).
  - UNVERIFIED: effect parameters that are points in layer space (e.g. a Point Control used by an effect) are mapped through the layer transform so that they still land on the same layer-space location.
  - Shape and text layers are always continuously rasterized; vector footage layers are only when the switch is on (otherwise they are rasterised once at 100% of their document size, then treated as raster layers).
- **Priority:** P0 — shape and text layers are always on this path.
- **Owner:** bae-render (+ bae-vector, bae-text, bae-effects)
- **Notes:** Full Collapse Transformations / Continuously Rasterize rules (when effects force a rasterisation, buffer bounds, 3D interaction) are in this file, section Collapse Transformations and Continuous Rasterization. Implementation hint: Vello renders the layer's vector scene with the final layer-to-comp matrix straight into an ROI-sized texture; effects then run on that texture.

### AE-CMP-009 · Layer viewer render stage selection (图层面板"视图"渲染阶段（译名待核）)

- **What it does:** The Layer viewer can display the layer at any stage of its pre-transform pipeline: Masks, or after any effect; this is the same pipeline as AE-CMP-007 truncated.
- **Exact semantics:**
  - View menu entries in order: Masks, then one entry per effect in stack order (UNVERIFIED exact first entry naming; when "Render" is unchecked the viewer shows the untouched source with mask outlines).
  - Selecting an effect renders steps 1-3 of AE-CMP-007 up to and including that effect; transforms, styles, mattes and blend are never applied in the Layer viewer.
- **Priority:** P1
- **Owner:** bae-render (+ bae-ui-motion)
- **Notes:** The SDK (`AEGP_RenderOptionsSuite`, "what's new" notes) exposes the same choice for third-party renders ("render the layer from its source, from its masks, or from its effects"). Viewer UI is in ui-preview-render-io.md, Layer viewer.

### AE-CMP-010 · Composition background colour in rendering (合成背景颜色)

- **What it does:** The comp Background Color is shown behind the comp in the Composition viewer but is not a layer: it is not blended with, not seen by adjustment layers or blend modes, and becomes transparent when the comp is nested or rendered with alpha.
- **Exact semantics:**
  - The comp is always composited over transparent black (AE-CMP-006). Display: viewer draws `out = comp_over(bg)` = `Cp + (1 - a) * bg` unless the transparency grid is on (see compositing-3d-color.md, transparency grid).
  - Nesting: the nested comp's background colour is discarded (transparent); the containing comp's background is used (Adobe help, Composition settings).
  - Output: with Channels = RGB+Alpha the background is transparent. With Channels = RGB, UNVERIFIED: the frame is matted against the comp background colour (best understanding of "RGB" output; to get black use a black background or a solid).
  - Blend modes on the bottom-most layer therefore blend against transparency, not the background colour: e.g. a Multiply layer alone in a comp looks like Normal.
- **Priority:** P0
- **Owner:** bae-render (+ bae-model)
- **Notes:** The Background Color setting (default black `[0,0,0]`) is persisted in Composition Settings, see project-comp-layers.md, Composition Settings; scripting `CompItem.bgColor`. Lottie has no comp background (players draw transparent). Gotcha: users often expect blend modes to interact with the background colour — they do not; the standard workaround is a solid layer.

## Blend modes

### AE-CMP-011 · Blending mode menu, data and enums (混合模式)

- **What it does:** Every AV layer has exactly one blending mode (default Normal) chosen from a fixed 38-entry menu in eight separator-delimited groups; the mode is not animatable.
- **Exact semantics:** Menu order (group separators shown as `—`), with official zh-CN UI names, ExtendScript `BlendingMode` enum, and Lottie layer `bm` value:

  | # | Mode (中文) | Group | ExtendScript `BlendingMode.` | Lottie `bm` |
  |---|---|---|---|---|
  | 1 | Normal (正常) | Normal | `NORMAL` | 0 |
  | 2 | Dissolve (溶解) | Normal | `DISSOLVE` | — |
  | 3 | Dancing Dissolve (动态抖动溶解) | Normal | `DANCING_DISSOLVE` | — |
  | — | | | | |
  | 4 | Darken (变暗) | Subtractive | `DARKEN` | 4 |
  | 5 | Multiply (相乘) | Subtractive | `MULTIPLY` | 1 |
  | 6 | Color Burn (颜色加深) | Subtractive | `COLOR_BURN` | 7 |
  | 7 | Classic Color Burn (经典颜色加深) | Subtractive | `CLASSIC_COLOR_BURN` | — |
  | 8 | Linear Burn (线性加深) | Subtractive | `LINEAR_BURN` | — |
  | 9 | Darker Color (深色) | Subtractive | `DARKER_COLOR` | — |
  | — | | | | |
  | 10 | Add (相加) | Additive | `ADD` | 16 |
  | 11 | Lighten (变亮) | Additive | `LIGHTEN` | 5 |
  | 12 | Screen (滤色) | Additive | `SCREEN` | 2 |
  | 13 | Color Dodge (颜色减淡) | Additive | `COLOR_DODGE` | 6 |
  | 14 | Classic Color Dodge (经典颜色减淡) | Additive | `CLASSIC_COLOR_DODGE` | — |
  | 15 | Linear Dodge (线性减淡) | Additive | `LINEAR_DODGE` | — |
  | 16 | Lighter Color (浅色) | Additive | `LIGHTER_COLOR` | — |
  | — | | | | |
  | 17 | Overlay (叠加) | Complex | `OVERLAY` | 3 |
  | 18 | Soft Light (柔光) | Complex | `SOFT_LIGHT` | 9 |
  | 19 | Hard Light (强光) | Complex | `HARD_LIGHT` | 8 |
  | 20 | Linear Light (线性光) | Complex | `LINEAR_LIGHT` | — |
  | 21 | Vivid Light (亮光) | Complex | `VIVID_LIGHT` | — |
  | 22 | Pin Light (点光) | Complex | `PIN_LIGHT` | — |
  | 23 | Hard Mix (实色混合) | Complex | `HARD_MIX` | 17 |
  | — | | | | |
  | 24 | Difference (差值) | Difference | `DIFFERENCE` | 10 |
  | 25 | Classic Difference (经典差值) | Difference | `CLASSIC_DIFFERENCE` | — |
  | 26 | Exclusion (排除) | Difference | `EXCLUSION` | 11 |
  | 27 | Subtract (相减) | Difference | `SUBTRACT` | — |
  | 28 | Divide (相除) | Difference | `DIVIDE` | — |
  | — | | | | |
  | 29 | Hue (色相) | HSL | `HUE` | 12 |
  | 30 | Saturation (饱和度) | HSL | `SATURATION` | 13 |
  | 31 | Color (颜色) | HSL | `COLOR` | 14 |
  | 32 | Luminosity (明度) | HSL | `LUMINOSITY` | 15 |
  | — | | | | |
  | 33 | Stencil Alpha (模板 Alpha) | Matte | `STENCIL_ALPHA` | — |
  | 34 | Stencil Luma (模板亮度) | Matte | `STENCIL_LUMA` | — |
  | 35 | Silhouette Alpha (轮廓 Alpha) | Matte | `SILHOUETE_ALPHA` (sic, one T) | — |
  | 36 | Silhouette Luma (轮廓亮度) | Matte | `SILHOUETTE_LUMA` | — |
  | — | | | | |
  | 37 | Alpha Add (Alpha 添加) | Utility | `ALPHA_ADD` | — |
  | 38 | Luminescent Premul (冷光预乘) | Utility | `LUMINESCENT_PREMUL` | — |

  - Scripting attribute `AVLayer.blendingMode` (read/write). The mode is a layer attribute, not a property: no keyframes, no expressions (Adobe help suggests splitting the layer, or the Compound Arithmetic effect, to change it over time).
  - The same mode list (subsets) is reused by paint strokes, layer styles, some effects (e.g. Blend, Set Matte "composite" options) and shape-layer Blend Mode; those are specified with their features.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render, bae-ui-motion)
- **Notes:** zh-CN names are from the zh-CN help category lists (Darker Color appears as 深色 in the category list and 较深的颜色 in a heading; Lighter Color 浅色 / 较浅的颜色; Luminosity 明度 / 发光度; Hard Mix 实色混合) — UNVERIFIED which form the AE 2026 zh-CN menu uses（译名待核）. Lottie: `bm` 0-17 per the Lottie spec; lottie-web implements only 0-15 (CSS `mix-blend-mode` names, `player/js/utils/blendModes.js`), so Add (16) and Hard Mix (17) are dropped there; Skottie implements all 18 (`modules/skottie/src/BlendModes.cpp`: 16 → `SkBlendMode::kPlus`, 17 → a runtime Hard Mix blender); Lottie has no Dissolve, Classic*, Linear Burn/Dodge/Light, Vivid/Pin Light, Darker/Lighter Color, Subtract, Divide, Stencil/Silhouette, Alpha Add, Luminescent Premul — BAE Lottie export must flatten or warn (see ui-preview-render-io.md, Lottie export). Keyboard: Shift+- / Shift+= cycle modes of selected layers (ui-preview-render-io.md, shortcuts).

### AE-CMP-012 · Generic blend compositing equation (混合模式合成公式（译名待核）)

- **What it does:** Defines how a separable or non-separable blend function `B(Cb, Cs)` is combined with source and backdrop alpha, so that each mode item only has to specify `B`.
- **Exact semantics:**
  - Adobe help states the AE modes are "identical to blending modes in Adobe Photoshop" and refers to the PDF Reference 1.7 §7.2.4 for the algorithms. The PDF/W3C model, written premultiplied:
  ```
  as' = as                                   # source alpha after masks, effects, opacity, matte
  ao  = as' + ab * (1 - as')                 # union alpha (all modes in this item)
  cop = (1 - ab) * csp + (1 - as') * cbp + as' * ab * B(Cb, Cs)
  Co  = (ao > 0) ? cop / ao : 0              # straight result colour
  # equivalently (straight):  Cmix = (1 - ab) * Cs + ab * B(Cb, Cs);  then Normal "over" with Cmix
  ```
  - Normal is `B(Cb, Cs) = Cs`, giving ordinary Porter-Duff source-over.
  - `B` is evaluated on straight colour of both inputs, per channel for separable modes; non-separable modes (Darker/Lighter Color, Hue, Saturation, Color, Luminosity) take whole RGB triples.
  - Bit-depth behaviour:
    - 8/16 bpc: `Cb`, `Cs` are already in [0, 1]; `B` results are clamped to [0, 1]; then the output is stored with clamping (AE-CMP-005).
    - 32 bpc: inputs may be < 0 or > 1; `B` is evaluated unclamped where the formula allows (Add, Subtract, Divide, Multiply, Screen, Linear modes produce over-range / negative values); modes that are only meaningful in [0, 1] (Color Burn/Dodge, Vivid/Pin Light, Hard Mix, Overlay, Soft Light, HSL modes) — UNVERIFIED: AE evaluates the same formula without clamping inputs, so results for over-range inputs follow the formula literally (e.g. Screen with `Cs = 2` gives `Cb + 2 - 2*Cb`).
  - Colour space: blending happens in the project working space's encoded values, or in linear light when "Blend Colors Using 1.0 Gamma" / "Linearize Working Space" is on (see compositing-3d-color.md, Color management).
  - Modes that change alpha differently (Dissolve, Stencil/Silhouette, Alpha Add, Luminescent Premul) override this equation; see their items.
  - Edge cases: `ao == 0` → output transparent black; `ab == 0` → result equals the source (no blend visible); `as' == 0` → backdrop unchanged.
- **Priority:** P0
- **Owner:** bae-render (+ bae-color)
- **Notes:** Source: https://helpx.adobe.com/after-effects/using/blending-modes-layer-styles.html ("identical to blending modes in Adobe Photoshop"; "see section 7.2.4 of version 1.7 of the PDF reference"). UNVERIFIED: that AE's implementation matches the PDF/W3C alpha equation exactly for every mode (especially the Classic modes, which predate it, and Add / Luminescent Premul). Behaviour visible in AE consistent with it: a non-Normal layer over transparency looks like Normal. Implementation hint: one WGSL "blend" fragment shader with a `mode` switch reading premultiplied inputs, unmultiplying with `max(a, 1e-6)` guard, applying `B`, recombining with the equation above; for Normal use fixed-function hardware blending (`One, OneMinusSrcAlpha`) and batch consecutive Normal layers. Adobe help lists eight modes (Screen, Multiply, Add, Overlay, Hard Light, Classic Difference, Lighten, Darken) as GPU-accelerated at 32 bpc when the layer has motion blur — irrelevant to results, but indicates AE's own GPU path.

### AE-CMP-013 · Normal blend mode (正常)

- **What it does:** Composites the source over the backdrop with ordinary alpha compositing; the default mode.
- **Exact semantics:**
  ```
  B(Cb, Cs) = Cs
  ao  = as + ab*(1 - as)
  cop = csp + cbp*(1 - as)
  ```
  - With `as == 1` the backdrop is fully replaced; with opacity < 100% the backdrop shows through proportionally.
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** Lottie `bm: 0` (or `bm` absent). ExtendScript `BlendingMode.NORMAL`.

### AE-CMP-014 · Dissolve and Dancing Dissolve (溶解和动态抖动溶解)

- **What it does:** Replaces partial transparency with a random per-pixel choice: each pixel is either the fully opaque source colour or the untouched backdrop, with probability equal to the source alpha; Dancing Dissolve re-randomises every frame.
- **Exact semantics:**
  ```
  r = rand01(seed, pixel_x, pixel_y [, frame_time for Dancing Dissolve])   # uniform in [0,1)
  if r < as':   ao = 1; Co = Cs                                  # pixel fully opaque source
  else:         ao = ab;  Co = Cb                                # pixel untouched
  ```
  - `as'` includes opacity, masks/feather, effects and track matte, so a soft mask edge becomes a speckled edge.
  - Dissolve: noise pattern is constant over time for a given layer (changes only if the layer moves, because the pattern is in comp pixel space — UNVERIFIED whether it is in comp space or layer space). Dancing Dissolve: pattern changes on each rendered frame.
  - Dissolve and Dancing Dissolve do not work on 3D layers (Adobe help: "Dissolve and Dancing Dissolve do not work on 3D layers"); UNVERIFIED: on a 3D layer they then behave like Normal.
  - Seeding: UNVERIFIED (AE's hash is not documented); BAE needs a deterministic hash of (layer id, x, y[, frame]) so renders are reproducible.
- **Priority:** P2 — rarely used.
- **Owner:** bae-render
- **Notes:** Not in Lottie. At preview resolution < Full the dither grain is per downsampled pixel (UNVERIFIED). Implementation hint: integer hash (e.g. PCG) of pixel coordinates in the shader; no texture needed.

### AE-CMP-015 · Darken, Multiply and Linear Burn (变暗、相乘和线性加深)

- **What it does:** Simple per-channel darkening modes.
- **Exact semantics:**
  ```
  Darken:      B = min(Cb, Cs)
  Multiply:    B = Cb * Cs                 # 8 bpc: (b*s)/255 ; 16 bpc: (b*s)/32768
  Linear Burn: B = Cb + Cs - 1             # 8/16 bpc: clamped at 0
  ```
  - 32 bpc: Multiply and Darken are unclamped (over-range × over-range grows); Linear Burn may go negative (UNVERIFIED whether AE clamps Linear Burn at 0 in 32 bpc).
  - White source is neutral for Multiply and Linear Burn; black source gives black.
- **Priority:** P0 — Multiply is among the most used modes; Darken and Linear Burn ride along.
- **Owner:** bae-render
- **Notes:** Lottie: Multiply `bm: 1`, Darken `bm: 4`; Linear Burn has no Lottie value.

### AE-CMP-016 · Color Burn and Classic Color Burn (颜色加深和经典颜色加深)

- **What it does:** Darkens the backdrop by increasing contrast according to the source.
- **Exact semantics:**
  ```
  Color Burn (PDF/W3C):
    if Cb >= 1:      B = 1
    elif Cs <= 0:    B = 0
    else:            B = 1 - min(1, (1 - Cb) / Cs)
  Classic Color Burn (AE 5.0 and earlier):
    UNVERIFIED: B = 1 - (1 - Cb) / Cs clamped to [0, 1], computed on premultiplied colour
    (without the PDF/W3C alpha handling), with Cs == 0 -> 0
  ```
  - Pure white source leaves the backdrop unchanged.
  - 32 bpc: UNVERIFIED handling of `Cb > 1` or negative values; the formula above yields 1 for `Cb >= 1`.
- **Priority:** P1 (Classic variant P2-level importance, kept in the same item because it shares the code path).
- **Owner:** bae-render
- **Notes:** Lottie Color Burn `bm: 7`. Adobe help: Classic Color Burn is the AE 5.0-and-earlier Color Burn, kept for compatibility with old projects. BAE decision: until verified against AE renders, Classic variants may map to the modern formula with a golden-image test flagged as known difference.

### AE-CMP-017 · Darker Color and Lighter Color (深色和浅色)

- **What it does:** Non-separable modes that pick the whole source or whole backdrop colour, whichever is darker (or lighter).
- **Exact semantics:**
  ```
  sum(C) = C.r + C.g + C.b            # Photoshop: "compares the total of all channel values"
  Darker Color:  B = (sum(Cs) < sum(Cb)) ? Cs : Cb
  Lighter Color: B = (sum(Cs) > sum(Cb)) ? Cs : Cb
  ```
  - UNVERIFIED: AE compares channel sums (as documented for Photoshop) rather than luminosity; tie-break (equal sums) keeps the backdrop (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-render
- **Notes:** Not in Lottie. Unlike Darken/Lighten no new colours are created.

### AE-CMP-018 · Add and Linear Dodge (相加和线性减淡)

- **What it does:** Adds source and backdrop channel values; the result is never darker than either input.
- **Exact semantics:**
  ```
  Add:          B = Cb + Cs            # 8/16 bpc clamped to 1; 32 bpc unclamped (overbright)
  Linear Dodge: B = Cb + Cs            # Photoshop "Linear Dodge (Add)"
  ```
  - UNVERIFIED: the AE difference between Add and Linear Dodge. Best understanding: Add is AE's legacy additive mode (added to premultiplied colour so that an edge's partial alpha contributes `Cs*as` light without darkening, i.e. `cop = cbp + csp`, `ao = as + ab*(1-as)`), while Linear Dodge follows the PDF/W3C equation (AE-CMP-012) and clamps at 1 even at 32 bpc. Needs golden tests against AE.
  - Black source leaves the backdrop unchanged.
- **Priority:** P0 — Add is the standard mode for glows, light leaks, particles.
- **Owner:** bae-render
- **Notes:** Lottie Add `bm: 16` (not implemented by lottie-web; Skottie maps it to `SkBlendMode::kPlus`). Add is one of the eight modes AE accelerates on the GPU at 32 bpc.

### AE-CMP-019 · Lighten and Screen (变亮和滤色)

- **What it does:** Per-channel lightening modes.
- **Exact semantics:**
  ```
  Lighten: B = max(Cb, Cs)
  Screen:  B = 1 - (1 - Cb) * (1 - Cs)  = Cb + Cs - Cb*Cs
  ```
  - 32 bpc: Screen with an input > 1 can produce results lower than `max(Cb, Cs)` or negative-derived artefacts (e.g. `Cb = 2, Cs = 0.5` → `2 + 0.5 - 1.0 = 1.5`, darker than the backdrop), because the formula is used literally (UNVERIFIED; this is the well-known "Screen breaks in 32 bpc" behaviour that leads users to Add in HDR).
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** Lottie Screen `bm: 2`, Lighten `bm: 5`.

### AE-CMP-020 · Color Dodge and Classic Color Dodge (颜色减淡和经典颜色减淡)

- **What it does:** Brightens the backdrop by decreasing contrast according to the source.
- **Exact semantics:**
  ```
  Color Dodge (PDF/W3C):
    if Cb <= 0:      B = 0
    elif Cs >= 1:    B = 1
    else:            B = min(1, Cb / (1 - Cs))
  Classic Color Dodge (AE 5.0 and earlier):
    UNVERIFIED: B = Cb / (1 - Cs) clamped to [0, 1], on premultiplied colour, Cs == 1 -> 1
  ```
  - Black source leaves the backdrop unchanged.
  - 32 bpc: UNVERIFIED whether the `min(1, ...)` clamp is applied (likely not for over-range `Cb`).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Lottie Color Dodge `bm: 6`. Classic variant: see AE-CMP-016 Notes.

### AE-CMP-021 · Overlay and Hard Light (叠加和强光)

- **What it does:** Multiply-or-screen modes: Overlay switches on the backdrop value, Hard Light on the source value (Hard Light = Overlay with inputs swapped).
- **Exact semantics:**
  ```
  HardLight(Cb, Cs) = (Cs <= 0.5) ? Cb * 2*Cs                      # multiply
                                   : 1 - (1 - Cb) * (1 - (2*Cs - 1)) # screen(Cb, 2Cs-1)
  Overlay(Cb, Cs)   = HardLight(Cs, Cb)                             # switch on Cb <= 0.5
  ```
  - A 50% grey (0.5) source is neutral for both modes (result = `Cb`); a 50% grey backdrop under Overlay yields the source colour (`Overlay(0.5, Cs) = Cs`).
  - 32 bpc: formula used literally for over-range values (UNVERIFIED).
- **Priority:** P0 — Overlay is a staple; Hard Light shares the code.
- **Owner:** bae-render
- **Notes:** Adobe help's Hard Light text says the switch depends on "the underlying color"; the Photoshop/PDF definition (and the same help page's first sentence, "depending on the original source color") switch on the source — BAE follows the PDF/W3C definition above; UNVERIFIED against AE renders. Lottie Overlay `bm: 3`, Hard Light `bm: 8`. Both are GPU-accelerated in AE 32 bpc.

### AE-CMP-022 · Soft Light (柔光)

- **What it does:** A gentle burn/dodge of the backdrop driven by the source; never produces pure black or white from non-pure inputs.
- **Exact semantics:**
  ```
  # PDF 1.7 / W3C definition:
  if Cs <= 0.5:  B = Cb - (1 - 2*Cs) * Cb * (1 - Cb)
  else:          B = Cb + (2*Cs - 1) * (D(Cb) - Cb)
  D(x) = (x <= 0.25) ? ((16*x - 12)*x + 4)*x : sqrt(x)
  ```
  - UNVERIFIED: AE may use the Photoshop variant where `D(x) = sqrt(x)` for all x (differs slightly for dark backdrops); verify with a golden test (backdrop ramp × source ramp).
  - 32 bpc: `sqrt` of negative backdrop is undefined → treat `Cb < 0` as 0 for `D` (UNVERIFIED AE behaviour).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Lottie `bm: 9` (lottie-web uses CSS `soft-light`, which is the W3C formula).

### AE-CMP-023 · Linear Light, Vivid Light, Pin Light and Hard Mix (线性光、亮光、点光和实色混合)

- **What it does:** Photoshop "light" family: burn or dodge the backdrop depending on whether the source is above or below 50% grey; Hard Mix posterises the Vivid Light result to 0 or 1 per channel.
- **Exact semantics:**
  ```
  LinearLight: B = Cb + 2*Cs - 1                                # 8/16 bpc clamp [0,1]
  VividLight:  B = (Cs <= 0.5) ? ColorBurn(Cb, 2*Cs)           # AE-CMP-016 modern formula
                               : ColorDodge(Cb, 2*Cs - 1)       # AE-CMP-020 modern formula
  PinLight:    B = (Cs <= 0.5) ? min(Cb, 2*Cs) : max(Cb, 2*Cs - 1)
  HardMix:     B = (Cb + Cs >= 1) ? 1 : 0                       # per channel
  ```
  - Hard Mix threshold equality: in 8 bpc, `b + s >= 255` → 255 (UNVERIFIED tie handling; Photoshop's documented rule is "sum ≥ 255 → 255").
  - Hard Mix ignores source opacity in Photoshop's "fill" sense; in AE opacity still mixes via AE-CMP-012 (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Adobe help describes these as switching on "the underlying color"; Photoshop and the PDF-style definitions switch on the source (blend) colour — BAE uses the source switch above, UNVERIFIED against AE. Lottie: only Hard Mix exists (`bm: 17`, not implemented by lottie-web). Divisions inside Vivid Light: `2*Cs == 0` → 0 (burn), `2*Cs - 1 == 1` → 1 (dodge) with the guards of AE-CMP-016/020.

### AE-CMP-024 · Difference, Classic Difference and Exclusion (差值、经典差值和排除)

- **What it does:** Per-channel difference modes; white source inverts the backdrop, black leaves it unchanged.
- **Exact semantics:**
  ```
  Difference:         B = |Cb - Cs|
  Exclusion:          B = Cb + Cs - 2*Cb*Cs
  Classic Difference: UNVERIFIED: |cbp - csp| on premultiplied colour with alpha
                      handled the pre-AE-6 way (result alpha = union), giving different
                      edges on partially transparent pixels than Difference
  ```
  - 32 bpc: Difference of over-range values is unclamped (e.g. `|3 - 1| = 2`). Adobe help: in Difference "painting with white inverts the backdrop" — at 32 bpc that is only true for backdrop ≤ 1.
- **Priority:** P0 — Difference is used daily for alignment checks.
- **Owner:** bae-render
- **Notes:** Lottie Difference `bm: 10`, Exclusion `bm: 11`; Classic Difference has no Lottie value. Classic Difference is GPU-accelerated in AE 32 bpc (Adobe help).

### AE-CMP-025 · Subtract and Divide (相减和相除)

- **What it does:** Arithmetic modes: backdrop minus source; backdrop divided by source.
- **Exact semantics:**
  ```
  Subtract: B = Cb - Cs                    # 8/16 bpc: clamp at 0; 32 bpc: may be < 0 (Adobe help)
  Divide:   B = (Cs > 0) ? Cb / Cs : (Cb > 0 ? 1 : 0)   # 8/16 bpc: clamp at 1; 32 bpc: may be > 1
  ```
  - Divide by zero: UNVERIFIED. Photoshop yields white for `Cs = 0` (unless `Cb = 0`); at 32 bpc AE may return a large or infinite value — BAE must not emit Inf/NaN; BAE decision: return `Cb / max(Cs, 1e-6)` capped at 65504 in 32 bpc (UNVERIFIED vs AE) and the Photoshop rule at 8/16 bpc.
  - White source is neutral for Divide; black source is neutral for Subtract.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Not in Lottie.

### AE-CMP-026 · Hue, Saturation, Color and Luminosity (色相、饱和度、颜色和明度)

- **What it does:** Non-separable modes that combine the hue, saturation and luminosity of source and backdrop.
- **Exact semantics:**
  ```
  Lum(C)     = 0.30*C.r + 0.59*C.g + 0.11*C.b
  Sat(C)     = max(C.r, C.g, C.b) - min(C.r, C.g, C.b)
  ClipColor(C): l = Lum(C); n = min(C); x = max(C)
                if n < 0: C = l + (C - l) * l / (l - n)
                if x > 1: C = l + (C - l) * (1 - l) / (x - l)
                return C
  SetLum(C, l): d = l - Lum(C); return ClipColor(C + d)
  SetSat(C, s): (Cmax, Cmid, Cmin) = sort channels
                if Cmax > Cmin: Cmid = (Cmid - Cmin) * s / (Cmax - Cmin); Cmax = s
                else: Cmid = Cmax = 0
                Cmin = 0; return C (channels in original positions)
  Hue:        B = SetLum(SetSat(Cs, Sat(Cb)), Lum(Cb))
  Saturation: B = SetLum(SetSat(Cb, Sat(Cs)), Lum(Cb))
  Color:      B = SetLum(Cs, Lum(Cb))
  Luminosity: B = SetLum(Cb, Lum(Cs))
  ```
  - Luma weights 0.30 / 0.59 / 0.11 per PDF 1.7 and Photoshop; UNVERIFIED that AE uses exactly these (not Rec.709 0.2126/0.7152/0.0722).
  - 32 bpc: `ClipColor` forces results into [0, 1] in the PDF model; UNVERIFIED whether AE skips `ClipColor` for 32 bpc (if it does, overbright luminosity survives Color/Luminosity modes).
- **Priority:** P1 — Color and Luminosity are commonly used for tinting/grading.
- **Owner:** bae-render (+ bae-color)
- **Notes:** Lottie `bm` 12 Hue, 13 Saturation, 14 Color, 15 Luminosity (lottie-web: CSS `hue`/`saturation`/`color`/`luminosity`, which are this W3C definition). Implementation hint: all four share one shader function set; avoid branches by sorting with `min/max`.

### AE-CMP-027 · Stencil Alpha and Stencil Luma (模板 Alpha 和模板亮度)

- **What it does:** The stencil layer is not drawn; instead it multiplies the alpha of the entire composite of all layers below it in the same comp, revealing them only where the stencil is opaque (or bright).
- **Exact semantics:**
  ```
  Stencil Alpha:  m = as'                       # stencil alpha incl. opacity, masks, effects
  Stencil Luma:   m = Lum601(Cs) * as'          # UNVERIFIED weights and whether as' multiplies
  acc.alpha = acc.alpha * m      # acc straight colour unchanged; premultiplied: acc *= m (all 4 channels)
  # nothing of the stencil layer's colour is added
  ```
  - Affects every layer below it in the same comp (the accumulated buffer), including layers whose own blend mode is Normal; layers above the stencil are composited normally afterwards.
  - To limit the effect to a subset of layers, those layers must be precomposed (Adobe help).
  - Outside the stencil layer's bounds `m = 0` (the composite below becomes transparent there).
  - Preserve Underlying Transparency and track mattes on the stencil layer itself modify `as'` normally (UNVERIFIED).
  - In 8/16 bpc `m` is in [0, 1]; in 32 bpc a luma > 1 is clamped to 1 (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** Luma for "Luma" modes: UNVERIFIED — AE track-matte luma is commonly reported as Rec.601 (0.299, 0.587, 0.114); assume the same here. Not in Lottie. Interaction with 3D: a stencil layer in a 3D group — UNVERIFIED (stencil/silhouette modes on 3D layers act on the composite rendered so far in depth order; see compositing-3d-color.md, Classic 3D renderer). Implementation hint: fixed-function blend `src=Zero, dst=SrcAlpha` (after writing `m` to the shader's alpha) on premultiplied buffers.

### AE-CMP-028 · Silhouette Alpha and Silhouette Luma (轮廓 Alpha 和轮廓亮度)

- **What it does:** The inverse of Stencil: the silhouette layer is not drawn and cuts a hole through all layers below it where it is opaque (or bright).
- **Exact semantics:**
  ```
  Silhouette Alpha: m = 1 - as'
  Silhouette Luma:  m = 1 - Lum601(Cs) * as'     # white -> fully transparent hole; black -> no change
  acc.alpha = acc.alpha * m        (premultiplied: acc *= m)
  ```
  - Outside the silhouette layer's bounds `m = 1` (no change).
  - Same scope rules as Stencil (AE-CMP-027): all layers below in the same comp.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** ExtendScript enum is misspelled `BlendingMode.SILHOUETE_ALPHA`. Adobe help: "Painting with pure white creates 0% opacity" (Silhouette Luma). Not in Lottie.

### AE-CMP-029 · Alpha Add (Alpha 添加)

- **What it does:** Composites colour like Normal but adds the alphas instead of combining them multiplicatively, so complementary soft edges (e.g. two layers with inverted feathered mattes, or abutting anti-aliased 3D faces) join without a seam.
- **Exact semantics:**
  ```
  ao  = min(1, ab + as')                     # instead of as' + ab*(1 - as')
  cop = csp + cbp * (1 - as')                # UNVERIFIED: colour as Normal
  Co  = (ao > 0) ? cop / ao : 0
  ```
  - Example from Adobe help: two 50% alpha pixels give 75% with Normal, 100% with Alpha Add.
  - UNVERIFIED: exact colour formula; alternative plausible form `cop = csp + cbp` (pure additive premultiplied) clamped by `ao`. Verify with two complementary gradient-alpha solids of different colours.
- **Priority:** P2
- **Owner:** bae-render
- **Notes:** Not in Lottie. Alpha Add affects the alpha of the source result (Adobe help: "The Alpha Add blending mode affects the alpha channel of the source layer").

### AE-CMP-030 · Luminescent Premul (冷光预乘)

- **What it does:** Treats the source as premultiplied light: colour values that exceed the source alpha are not clipped but added to the composite, for compositing lens flares and light effects rendered with premultiplied alpha.
- **Exact semantics:**
  ```
  # UNVERIFIED best understanding:
  csp_lum = Cs (straight colour interpreted as if already premultiplied, i.e. not multiplied by as')
  cop = csp_lum + cbp * (1 - as')
  ao  = as' + ab * (1 - as')
  Co  = (ao > 0) ? cop / ao : 0               # may exceed 1; clamped at 8/16 bpc
  ```
  - Adobe help recommends interpreting such premultiplied footage as Straight when using this mode, which is consistent with the formula: the footage's stored (premultiplied) RGB is used directly as additive light, so colour in pixels with alpha < colour value is not lost.
- **Priority:** P2
- **Owner:** bae-render
- **Notes:** Not in Lottie. Golden test: lens-flare footage with premultiplied black edges, interpreted Straight, over a mid-grey solid.

## Opacity and Preserve Underlying Transparency

### AE-CMP-031 · Layer opacity application (图层不透明度的应用（译名待核）)

- **What it does:** The layer's Opacity transform property scales the layer's alpha after masks, effects and transform, before the track matte and blend mode.
- **Exact semantics:**
  ```
  as' = clamp01(as_after_transform * opacity / 100)     # opacity in percent, 0..100
  ```
  - Opacity range 0-100 % (values set by expression outside the range are clamped, UNVERIFIED for > 100).
  - Opacity multiplies alpha only (straight colour unchanged); with premultiplied buffers multiply all four channels.
  - Opacity 0 % layers are still evaluated (expressions, effects referencing them) but contribute nothing; BAE may skip rasterising them unless they are a track matte, a stencil/silhouette source or referenced by an effect layer parameter (UNVERIFIED whether an effect that samples another layer sees that layer's opacity: AE effect layer parameters ignore the referenced layer's transforms and opacity by default — see effects.md, effect framework).
  - Opacity of a track-matte layer scales the matte value (AE-CMP-034); opacity of a stencil/silhouette layer scales `m` (AE-CMP-027/028); opacity of an adjustment layer scales the effect mix (AE-CMP-049 Adjustment layers).
  - Opacity is applied per motion-blur sub-sample (it can be animated within the shutter).
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** The Opacity property itself (keyframes, `ks.o` in Lottie, matchName `ADBE Opacity`) is defined in transform-keyframes-expressions.md, Transform properties. Implementation hint: fold opacity into the final resample/blend shader as a uniform; never allocate a buffer for it.

### AE-CMP-032 · Preserve Underlying Transparency (保留底层透明度)

- **What it does:** With the "T" switch on, the layer is visible only where the composite of the layers below is opaque, and it never changes that composite's alpha (like a Photoshop clipping mask).
- **Exact semantics:**
  ```
  # source-atop with blend mode; ab, cbp from the accumulated composite below
  ao  = ab                                   # alpha of the result = alpha below, unchanged
  cop = as' * ab * B(Cb, Cs) + (1 - as') * cbp
  Co  = (ao > 0) ? cop / ao : 0
  ```
  - "Underlying" means the full accumulated composite of all participating layers below in the same comp (not just the next layer).
  - Over fully transparent areas the layer contributes nothing; partially transparent underlying pixels receive the layer weighted by `ab`.
  - Combined with a track matte: the matte first reduces `as'` (AE-CMP-034), then the atop composite applies.
  - Ignored on adjustment layers (UNVERIFIED) and has no meaning on the bottom layer (result would be empty).
  - UNVERIFIED: exact formula; AE help describes the behaviour qualitatively only ("the layer gets its transparency from the composite of the layers below").
- **Priority:** P1
- **Owner:** bae-render (+ bae-model)
- **Notes:** Switch lives in the Modes column ("T"); scripting `AVLayer.preserveTransparency` (Boolean). No Lottie equivalent (BAE Lottie export: approximate with an alpha track matte only when the underlying composite is a single layer, else warn). Implementation hint: fixed-function blend for Normal mode: `color: src=DstAlpha, dst=OneMinusSrcAlpha; alpha: src=Zero, dst=One` on premultiplied buffers.

## Track mattes

### AE-CMP-033 · Track matte assignment: legacy and AE 23.0 models (轨道遮罩)

- **What it does:** A layer can take its transparency from another layer (the matte) in the same comp; since AE 23.0 (2023) any layer can be chosen as the matte, earlier only the layer directly above.
- **Exact semantics:**
  - **AE 23.0+ model (current):** each AV layer has `trackMatteLayer` (any other AV layer in the same comp, or none) plus two toggles: Alpha / Luma and Normal / Inverted. Chosen via the Track Matte column (in the Modes column area) with a layer list dropdown or pick whip. Many layers may share one matte layer. When a layer is first picked as a matte its Video switch is turned off automatically, but the user may turn it back on and the matte then also renders normally.
  - **Legacy model (AE ≤ 22.x):** the matte is always the layer immediately above (index − 1); choosing a Track Matte mode turns off that layer's Video switch; reordering layers changes or breaks the pairing; one matte serves one layer.
  - Opening a legacy project maps each matted layer to `trackMatteLayer = layer above` with the same mode.
  - Modes (UI order; scripting `TrackMatteType`; Lottie `tt`):

  | Mode (中文) | ExtendScript | Lottie `tt` | Opaque where |
  |---|---|---|---|
  | No Track Matte (没有轨道遮罩（译名待核）) | `NO_TRACK_MATTE` | absent / 0 | — |
  | Alpha Matte (Alpha 遮罩) | `ALPHA` | 1 | matte alpha = 100 % |
  | Alpha Inverted Matte (Alpha 反转遮罩) | `ALPHA_INVERTED` | 2 | matte alpha = 0 % |
  | Luma Matte (亮度遮罩) | `LUMA` | 3 | matte luminance = 100 % |
  | Luma Inverted Matte (亮度反转遮罩) | `LUMA_INVERTED` | 4 | matte luminance = 0 % |

  - Constraints: a layer cannot be its own matte; cycles (A mattes B, B mattes A) are refused (UNVERIFIED exact UI behaviour). Camera, light and audio-only layers cannot be mattes (no pixels). UNVERIFIED: whether adjustment layers can be picked as mattes (if allowed, only their source alpha would count).
- **Priority:** P0
- **Owner:** bae-model (+ bae-render, bae-ui-motion, bae-motion)
- **Notes:** Scripting (23.0+): `AVLayer.setTrackMatte(layer, TrackMatteType)`, `AVLayer.removeTrackMatte()` (keeps the type), `AVLayer.trackMatteLayer` (read-only), `hasTrackMatte`, `isTrackMatte`; `trackMatteType` is the legacy read/write API. Lottie: matted layer has `tt` (1-4), matte layer has `td: 1`, and `tp` (matte parent layer index) — when `tp` is absent the matte is the previous layer in the array (the layer above), which is how bodymovin exported pre-23 projects. Parenting the matte to the matted layer makes a matte that moves with it (Adobe help tip). Source: https://helpx.adobe.com/after-effects/using/track-mattes-and-traveling-mattes.html ("Any layer in your composition, no matter where it sits in the layer stack, can be used as a Track Matte source"; multiple layers may reference one matte; the matte's visibility is switched off by default when chosen and can be turned back on).

### AE-CMP-034 · Track matte compositing math (轨道遮罩合成计算（译名待核）)

- **What it does:** Multiplies the matted layer's alpha by a per-pixel matte value derived from the matte layer's rendered image in comp space.
- **Exact semantics:**
  ```
  M = render matte layer at comp resolution (its own masks, effects, transform, opacity,
      layer styles, motion blur, and its own track matte if any), as straight (Cm, am)
      — the matte layer's blend mode and Preserve Transparency are ignored (UNVERIFIED)
  v = Alpha:          am
      Alpha Inverted: 1 - am
      Luma:           Luma(Cm) * am              # transparent matte areas count as black
      Luma Inverted:  1 - Luma(Cm) * am          # transparent matte areas become opaque
  as' = as' * clamp01(v)                         # matted layer, after its opacity
  Luma(C) = 0.299*R + 0.587*G + 0.114*B          # UNVERIFIED weights (Rec.601 assumed)
  ```
  - Order: applied after the matted layer's transform, opacity and layer styles, before its blend mode and Preserve Underlying Transparency (AE-CMP-007).
  - Both layers are evaluated at the same comp time; each uses its own in/out points. Outside the matte layer's in/out range (or when the matte is otherwise empty) `M` is transparent: Alpha/Luma mattes hide the layer completely, inverted mattes show it fully (UNVERIFIED).
  - Pixels outside the matte layer's bounds are transparent (`am = 0`).
  - The matte layer's Video switch does not affect its use as a matte; Solo state of the matte is irrelevant (UNVERIFIED).
  - 32 bpc: luma > 1 is clamped to 1 for the matte value (UNVERIFIED); 8/16 bpc already in range.
  - Linear blending setting: UNVERIFIED whether luma is computed on encoded or linearised values when "Blend Colors Using 1.0 Gamma" is on.
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** lottie-web implements Alpha/Luma via SVG masks with `mask-type` alpha/luminance (CSS luminance coefficients 0.2125/0.7154/0.0721 in linearRGB), so lottie-web luma mattes do not match AE exactly. Implementation hint: render the matte layer once per frame into an R16F/R8 "matte value" texture at ROI size, shared by all layers that use it; fuse the multiply into the matted layer's resample/blend shader.

### AE-CMP-035 · Nested, shared and 3D track mattes (嵌套、共享和 3D 轨道遮罩（译名待核）)

- **What it does:** Covers mattes that themselves have mattes, one matte used by several layers, and mattes or matted layers that are 3D.
- **Exact semantics:**
  - Nested mattes: a matte layer may itself have a track matte; it is resolved recursively (matte of matte applied to the matte's alpha before the matte value is derived). UNVERIFIED depth limit; cycles must be rejected at assignment.
  - Shared mattes (23.0+): the matte is rendered once per frame and reused; each user may choose a different mode (Alpha / Luma / inverted).
  - 3D matte layer: the matte is rendered through the comp's active camera (and lights, if it accepts lights) into a flat 2D comp-space image, then used as in AE-CMP-034.
  - 3D matted layer: the matte value multiplies the matted layer's rendered (camera-projected) image in comp space; the matted layer still takes part in its 3D group's depth sort / intersection (UNVERIFIED: AE renders a matted 3D layer as its own 2D image, breaking intersections with other 3D layers, similar to layers with blend modes in the Classic 3D renderer; see compositing-3d-color.md, Classic 3D renderer).
  - Interplay with blend modes: the matted layer's blend mode applies after the matte (matte reduces `as'`, then `B` and AE-CMP-012).
  - Matte layer on an adjustment layer: an adjustment layer can have a track matte, limiting where its effects apply (see Adjustment layers section).
  - Collapsed precomp as matte or matted layer: the precomp is flattened for the matte step (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render (+ bae-model)
- **Notes:** Legacy model limitation: since a matte had to be directly above, sharing a matte required duplicates or precomps; BAE implements only the 23.0+ model internally and converts legacy pairs on import.

## Masks

### AE-CMP-036 · Mask data model and mask path (蒙版和蒙版路径)

- **What it does:** Each AV layer owns an ordered list of masks; each mask is a Bezier path in the layer's own coordinate space plus mode, inversion, feather, opacity and expansion properties. Closed masks modify the layer's alpha; open masks only serve as path inputs for effects.
- **Exact semantics:**
  - Property tree (top to bottom in the Timeline under Masks (蒙版), each mask named "Mask 1", "Mask 2", ...):

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Mask Path (蒙版路径) | Bezier path (shape) | set when drawn | — | layer pixels | closed or open; animatable; matchName `ADBE Mask Shape` |
  | Mask Feather (蒙版羽化) | 2D [x, y] | [0, 0] | 0 … 32,000 by scrubbing (Adobe help); UNVERIFIED max for typed values | pixels | constrain-proportions link; matchName `ADBE Mask Feather` |
  | Mask Opacity (蒙版不透明度) | scalar | 100 | 0 … 100 | % | matchName `ADBE Mask Opacity` |
  | Mask Expansion (蒙版扩展) | scalar | 0 | −32,000 … 32,000 (scrub range) | pixels | matchName `ADBE Mask Offset` |

  - Attributes (not animatable): mode (default Add; AE-CMP-038), Inverted (反转, default off), Mask Feather Falloff (Smooth default), Motion Blur (Same As Layer default), RotoBezier (off), colour, locked, name.
  - Coordinates: vertices are in layer space (the source's pixel grid, origin top-left of the layer), so masks move, scale and rotate with the layer; to move a mask independently the user animates the path (or uses a separate layer as a track matte).
  - Closed vs open: a path with `closed = true` affects alpha (unless mode None). An open path never affects alpha and has no mode; effects such as Stroke, Path Text, Audio Waveform, Audio Spectrum and Vegas accept open or closed masks; Fill, Smear, Reshape, Particle Playground and Inner/Outer Key need closed masks (Adobe help).
  - Empty mask list, or all masks None/open → layer alpha unchanged.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render, bae-vector)
- **Notes:** Scripting: `layer.mask` (`ADBE Mask Parade`) → `MaskPropertyGroup` (`ADBE Mask Atom`) with `maskMode`, `inverted`, `rotoBezier`, `maskMotionBlur`, `maskFeatherFalloff`, `locked`, `color`. Expression access: `mask("Mask 1").maskPath / maskFeather / maskOpacity / maskExpansion / invert` (see transform-keyframes-expressions.md, Expressions mask object). Lottie: layer `hasMask`, `masksProperties[]` with `mode` (see AE-CMP-038), `inv` (bool), `pt` (animated Bezier), `o` (opacity %), `x` (expansion px), `nm`; the official Lottie schema (lottie-docs, Mask) lists only `mode`, `o`, `pt`, `x`, `inv` (+ `nm`); feather is a de-facto extension: Skottie reads a 2D `f` (`modules/skottie/src/Layer.cpp`), lottie-web ignores it. Bezier path model and editing: shapes.md, Bezier model; drawing tools: ui-preview-render-io.md, tools.

### AE-CMP-037 · Mask display attributes: colour, lock, name (蒙版颜色、锁定和名称（译名待核）)

- **What it does:** Per-mask UI data that does not affect rendering: outline colour in viewers and Timeline, lock (prevents selection/editing), and name.
- **Exact semantics:**
  - Colour: RGB in [0, 1]; default yellow for every new mask unless the Appearance preference "Cycle Mask Colors" is on (then it cycles through label colours); "Use Contrasting Color For Mask Path" picks a colour that contrasts with the pixels where drawing starts and differs from the previous mask.
  - Locked: boolean; locked masks cannot be selected or edited in viewers; still render.
  - Name: string, default "Mask N" (N = index at creation), renamable, unique not required (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `MaskPropertyGroup.color` (array of 3 floats), `.locked`. Preferences: ui-preview-render-io.md, Preferences (Appearance).

### AE-CMP-038 · Mask modes and accumulation (蒙版模式)

- **What it does:** Combines the coverage of all closed masks on a layer, top to bottom, into one alpha multiplier using per-mask modes.
- **Exact semantics:**
  - Modes in menu order (zh-CN; ExtendScript `MaskMode.`; Lottie `mode`; drawing shortcut key): None (无; `NONE`; `"n"`; N), Add (相加; `ADD`; `"a"`; A), Subtract (相减; `SUBTRACT`; `"s"`; S), Intersect (相交; `INTERSECT`; `"i"`; I), Lighten (变亮; `LIGHTEN`; `"l"`; L), Darken (变暗; `DARKEN`; `"d"`; D), Difference (差值; `DIFFERENCE`; `"f"`; F). Default Add. Not animatable.
  ```
  # m_i = per-mask value in [0,1] = Inverted? / feather / expansion / opacity applied (AE-CMP-039..043)
  active = [mask for mask in masks (top to bottom) if mask.closed and mask.mode != None]
  if active is empty: M = 1
  else:
    first = active[0]
    acc = (first.mode in {Subtract, Intersect, Darken}) ? 1 : 0      # Darken: UNVERIFIED
    for mask in active:
      m = mask_value(mask)
      Add:        acc = acc + m - acc*m            # union (source-over); UNVERIFIED vs min(1, acc+m)
      Subtract:   acc = acc * (1 - m)
      Intersect:  acc = acc * m
      Lighten:    acc = max(acc, m)
      Darken:     acc = min(acc, m)
      Difference: acc = acc + m - 2*acc*m          # XOR-like; UNVERIFIED vs |acc - m|
    M = acc
  layer.alpha = layer.alpha * M                     # masks stage of AE-CMP-007
  ```
  - With a single Subtract mask the layer is visible everywhere except inside the mask (because the start value is 1); a single Intersect mask behaves like Add.
  - Mask modes operate only between masks of the same layer; they never touch other layers.
  - The first mask "interacts with the alpha channel of the layer" (Adobe help): `M` multiplies the source alpha, it does not replace it, so a mask cannot make transparent source pixels opaque.
  - Drawing/editing shortcuts that set the mode of the closed mask being drawn or modified (Adobe help, Mask modes): N None, A Add, S Subtract, I Intersect, L Lighten, D Darken, F Difference.
  - Coverage outside the layer's bounds does not exist (masks are evaluated inside the layer frame only).
- **Priority:** P0
- **Owner:** bae-render (+ bae-model, bae-vector)
- **Notes:** Scripting enum order as documented: `MaskMode.NONE, ADD, SUBTRACT, INTERSECT, LIGHTEN, DARKEN, DIFFERENCE`. Source: https://helpx.adobe.com/after-effects/using/alpha-channels-masks-mattes.html (Mask modes). Adobe help's Intersect text says non-overlapping areas become "complete opacity"; actual AE behaviour (and lottie-web/Skottie) is complete transparency outside the intersection — the help text is wrong. Reference implementations: Skottie maps Add→SrcOver, Intersect→SrcIn, Subtract→DstOut (first Subtract = inverted geometry), Difference→Xor and does not support Lighten/Darken; lottie-web starts from a full white rect when the first mask is Subtract or Intersect. Golden tests needed for the partial-coverage formulas (overlap of two 50 % opacity masks in each mode). Implementation hint: rasterise each mask's coverage into an R16F texture at layer resolution (or ROI), accumulate in one compute pass over all masks, multiply into alpha in the same pass.

### AE-CMP-039 · Mask Inverted (反转)

- **What it does:** Swaps inside and outside of one mask before it is combined with the other masks.
- **Exact semantics:**
  ```
  cov = antialiased coverage of the (expanded, feathered) closed path in [0,1]
  m   = (inverted ? 1 - cov : cov) * maskOpacity/100      # UNVERIFIED: opacity after inversion
  ```
  - Inverted applies to the expanded and feathered coverage (so the feather ramp is mirrored as well).
  - Shortcut Ctrl/Cmd+Shift+I inverts selected masks (see ui-preview-render-io.md, shortcuts).
- **Priority:** P0
- **Owner:** bae-render (+ bae-model)
- **Notes:** Scripting `MaskPropertyGroup.inverted`; expression `mask(...).invert`; Lottie `inv`. lottie-web implements inversion by adding a comp-sized rectangle to the path (even-odd), Skottie by inverse fill type — both equivalent for coverage.

### AE-CMP-040 · Mask Opacity (蒙版不透明度)

- **What it does:** Scales the influence of one closed mask on the layer's alpha.
- **Exact semantics:**
  - `m = coverage' * opacity / 100`, where `coverage'` is after expansion, feather and inversion (AE-CMP-039).
  - Mask Opacity 100 % → interior fully opaque; outside of a (non-inverted) mask is always transparent regardless of opacity (Adobe help).
  - Range 0-100 %; 0 % makes an Add mask contribute nothing (and a lone Add mask with 0 % hides the layer completely).
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** Lottie mask `o` (0-100). Shortcut TT shows Mask Opacity (ui shortcuts).

### AE-CMP-041 · Mask Feather (蒙版羽化)

- **What it does:** Softens the mask edge with a Gaussian-like falloff, separately in x and y, straddling the path half inside and half outside.
- **Exact semantics:**
  - Value `[fx, fy]` in layer pixels; both ≥ 0. Feather width `f` is centred on the (expanded) edge: e.g. f = 25 → ~12.5 px inside and ~12.5 px outside (Adobe help).
  - Profile: opacity varies "according to a Gaussian distribution", so the influence extends slightly beyond the nominal width (Adobe help). Best implementation: blur the hard (anti-aliased) coverage with a separable Gaussian, `sigma_x = k * fx`, `sigma_y = k * fy`. UNVERIFIED `k` for AE: Skottie uses `k = 0.38` (`kFeatherToSigma` in `modules/skottie/src/Layer.cpp`, its empirical match to AE); a pure "99 % within f/2" Gaussian would be `k ≈ 0.19`. BAE default: `k = 0.38` until golden-tested.
  - Feathering is computed only within the layer's dimensions: coverage outside the layer frame is not available, so a feather that reaches the layer edge is cut abruptly (Adobe help). The blur uses transparent (decal) borders, not edge clamping.
  - Feather is in layer space, so it scales with the layer's Scale when the layer is transformed afterwards (raster pipeline); downsampled previews scale it by the resolution factor (AE-CMP-062 Resolution downsampling).
  - 0 feather → hard anti-aliased edge (Best quality) or aliased (Draft, UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-render
- **Notes:** Shortcut F reveals Mask Feather. Lottie: not in the official mask schema; Skottie reads `f`. Implementation hint: separable two-pass Gaussian on the R16F coverage texture with kernel radius `3*sigma`; for large feathers downsample (mip) first — the result is smooth, so a 1/4-resolution blur and bilinear upsample is visually identical. Feather Falloff and variable-width feathers modify this profile (AE-CMP-042, AE-CMP-043).

### AE-CMP-042 · Mask Feather Falloff (羽化衰减)

- **What it does:** Chooses the shape of the feather ramp: Smooth (default) or Linear.
- **Exact semantics:**
  - Values in menu order (Layer > Mask > Feather Falloff / 图层 > 蒙版 > 羽化衰减; scripting `MaskPropertyGroup.maskFeatherFalloff`, `MaskFeatherFalloff.`): Smooth (平滑, `FFO_SMOOTH`, default), Linear (线性, `FFO_LINEAR`).
  - Smooth: Gaussian / smoothstep-like ramp (AE-CMP-041). Linear: alpha changes linearly with distance across the feather band from the inner to the outer feather boundary. UNVERIFIED exact Smooth curve when variable-width feather points exist (likely smoothstep over the band rather than a true Gaussian blur).
  - Per-mask attribute, not animatable.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Introduced with variable-width mask feather in AE CS6 (11.0). Implementation hint: when Falloff = Linear or feather points exist, compute coverage from a signed-distance field of the path (distance to inner/outer boundary) instead of blurring.

### AE-CMP-043 · Variable-width mask feather points (可变宽度蒙版羽化)

- **What it does:** Feather points placed along a closed mask path with the Mask Feather tool define a feather band whose width varies along the path, inside or outside.
- **Exact semantics:**
  - Each feather point: position along the path (segment index + parameter), radius (signed: outward positive, inward negative — UNVERIFIED sign), tension (curvature of the boundary through the point, 0-100 %, UNVERIFIED range), corner angle, and Hold flag (constant radius up to the next feather point).
  - Feather points define the outer and inner feather boundaries; if no feather point is inside the mask, the inner boundary is the mask path itself (Adobe help). The feather ramps from the inner to the outer boundary (shape per Feather Falloff).
  - Feather points are stored with the Mask Path keyframes; counts are kept constant across keyframes when the preference "Preserve Constant Vertex and Feather Point Count when Editing Masks" is on (Preferences > Composition in current AE; General in older versions).
  - Combined with the global Mask Feather value: UNVERIFIED (assume the global Gaussian feather is applied on top).
- **Priority:** P2
- **Owner:** bae-render (+ bae-model, bae-ui-motion)
- **Notes:** SDK: `AEGP_MaskFeatherSuite` (feather points with `radiusF`, negative allowed for inner feathers, `AEGP_MaskFeatherType_INNER/OUTER`, interpolation, tension, corner angle). Not in Lottie. Tool UI in ui-preview-render-io.md, tools (Mask Feather tool, G toggles Pen/Mask Feather).

### AE-CMP-044 · Mask Expansion (蒙版扩展)

- **What it does:** Grows (positive) or shrinks (negative) the area a mask influences, as an offset of its coverage in pixels, without changing the mask path.
- **Exact semantics:**
  - `e` in layer pixels; `e > 0` expands outward, `e < 0` contracts; scrub range −32,000 … 32,000 (Adobe help; earlier versions scrubbed only to ±1,000, larger values had to be typed).
  - Applied before feather: the feather is centred on the expanded edge.
  - Offset geometry: UNVERIFIED — expansion behaves like a morphological dilation/erosion of the filled path (rounded outer corners on expansion, sharp inner corners), not a miter offset of the path.
  - Inverted masks: expansion is applied to the path's interior first, then inverted (UNVERIFIED).
  - Large negative values may remove the mask area completely (coverage 0 → for Add mode the layer becomes invisible).
- **Priority:** P0
- **Owner:** bae-render (+ bae-vector)
- **Notes:** Lottie `x` (pixels); lottie-web emulates positive expansion with a stroke of width `2x` and negative with an SVG `feMorphology erode` of radius `-x`. Implementation hint: compute from a signed distance field: `cov = smoothstep(-0.5, 0.5, -sd(p) + e)`; one SDF serves expansion, feather and linear falloff.

### AE-CMP-045 · Mask rasterisation and anti-aliasing (蒙版栅格化和消除锯齿（译名待核）)

- **What it does:** Converts each closed mask path into a per-pixel coverage value.
- **Exact semantics:**
  - Coverage is the area fraction of the pixel inside the path (anti-aliased) in Best quality; in Draft quality UNVERIFIED (AE still anti-aliases masks in Draft but with lower precision).
  - Fill rule for self-intersecting mask paths: UNVERIFIED (nonzero winding assumed — Skottie and lottie-web use nonzero for masks).
  - Coverage is computed in layer space at the layer's render resolution (source pixels × preview resolution factor); for continuously rasterized and collapsed layers it is computed in comp space with the layer transform applied (AE-CMP-008), so mask edges stay sharp when scaled up.
  - Paths are flattened adaptively so that the error is below ~0.1 px at render resolution (implementation requirement, not AE-documented).
- **Priority:** P0
- **Owner:** bae-render (+ bae-vector)
- **Notes:** Implementation hint: use Vello's path coverage (or kurbo flattening + analytic coverage compute shader) into an R16F mask atlas; share the code path with shape-layer fills.

### AE-CMP-046 · Mask motion blur (蒙版运动模糊)

- **What it does:** Controls whether a mask's own path animation is motion-blurred.
- **Exact semantics:**
  - Values (scripting `MaskMotionBlur`): Same As Layer (与图层相同, `SAME_AS_LAYER`, default), On (打开（译名待核）, `ON`), Off (关闭（译名待核）, `OFF`).
  - Same As Layer: the mask is evaluated at each motion-blur sub-sample time when the layer's Motion Blur switch and the comp's Motion Blur enable are on.
  - On: mask path is sampled at the sub-sample times even if the layer's Motion Blur switch is off (Adobe help: "regardless of the setting of the Enable Motion Blur switch for the layer"); the comp-level Enable Motion Blur / Render Settings motion blur still gate it (UNVERIFIED for the comp-level gate).
  - Off: mask evaluated at the frame time only, while the layer's transform is still blurred.
  - Sub-sample times and weighting come from the comp's Shutter Angle / Phase / Samples Per Frame (see transform-keyframes-expressions.md, motion-blur sampling; AE-CMP-064 Motion blur rendering).
- **Priority:** P1
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Layer > Mask > Motion Blur menu; scripting `MaskPropertyGroup.maskMotionBlur`. Source: https://helpx.adobe.com/after-effects/using/assorted-animation-tools.html (Motion blur). Implementation hint: for raster layers the masked source is rendered per sub-sample only when the mask path actually changes within the shutter interval; otherwise reuse one masked source for all sub-samples.

### AE-CMP-047 · RotoBezier masks (RotoBezier 蒙版（译名待核）)

- **What it does:** A mask whose curve tangents are computed automatically from vertex positions and per-vertex tension, instead of manual Bezier handles.
- **Exact semantics:**
  - Boolean per mask (`MaskPropertyGroup.rotoBezier`); converting toggles between manual Bezier and RotoBezier (Layer > Mask and Shape Path > RotoBezier).
  - Each vertex has a tension (adjusted with the Convert Vertex tool); the renderer receives ordinary cubic Beziers computed from vertices + tensions. UNVERIFIED: exact tangent formula (Catmull-Rom-like with tension scaling).
  - Rendering of a RotoBezier mask is identical to rendering the derived Bezier path.
- **Priority:** P2
- **Owner:** bae-vector (+ bae-model)
- **Notes:** Also exists for shape-layer paths; shapes.md, Bezier model. Lottie exports the derived Bezier.

### AE-CMP-048 · Masks on special layer types (特殊图层上的蒙版（译名待核）)

- **What it does:** Defines what masks do on adjustment, text, shape, precomp and 3D layers.
- **Exact semantics:**
  - Adjustment layer: the mask result limits where the adjustment layer's effects are applied (mix factor, see Adjustment layers section).
  - Text and shape layers: masks clip the rendered vector content; mask paths are in layer space; evaluated in the continuously rasterized order (masks → transform → effects, AE-CMP-008). Text-on-path and shape effects can use the masks as path input.
  - Solid layers: a mask on a solid is the standard way to draw a soft shape. Solids are raster layers, so the mask is rasterised at the solid's pixel size and its edge softens when the solid is scaled above 100 % (UNVERIFIED: AE may render solids resolution-independently).
  - Precomp layers: masks apply to the nested comp's rendered frame; with Collapse Transformations on, a closed mask forces the nested layers to be flattened first (Collapse Transformations section).
  - 3D layers: masks are in the layer's own plane and are projected with it.
  - Camera, light and audio-only layers cannot have masks (no pixels). Null objects: UNVERIFIED whether AE allows masks on them; they would have no visible effect (a null is never rendered).
- **Priority:** P0
- **Owner:** bae-render (+ bae-text, bae-vector)
- **Notes:** Effects that read a mask path (Stroke, Fill, Path Text, Vegas...) are specified in effects.md; text "Path Options" in text.md.

## Adjustment layers

### AE-CMP-049 · Adjustment layer rendering (调整图层)

- **What it does:** An adjustment layer applies its effect stack to the composite of all layers below it in the same comp; its own alpha (source, masks, transform, opacity, track matte) defines where and how strongly the effects apply.
- **Exact semantics:**
  ```
  apply_adjustment(L, acc, t):                 # acc = premultiplied composite of layers below
    if !L.video_enabled or no enabled effects: return acc        # no visible result
    E = run L.effects top-to-bottom on acc (comp-sized input, comp coordinates)
    A = alpha of L's own layer pipeline:  source alpha (AE-created adjustment = opaque comp-size solid)
        * masks (AE-CMP-038) -> transform (incl. motion blur) -> * opacity/100 -> * track matte value
    if L.blend_mode == Normal:
        out = lerp(acc, E, A)                  # premultiplied, all four channels
    else:
        out = lerp(acc, blend(src = E, dst = acc, L.blend_mode), A)   # AE-CMP-012; UNVERIFIED
    return out
  ```
  - Effects can change alpha: e.g. Linear Wipe on an adjustment layer makes the layers below transparent where it wipes (follows from the lerp).
  - An adjustment layer at the bottom of the stack (nothing below) has no visible result; layers above it are unaffected.
  - Opacity acts as an effect-strength control (`A` scales the mix); Preserve Underlying Transparency on an adjustment layer: UNVERIFIED (no visible meaning).
  - Effects' spatial parameters (points, layer-space coordinates) are interpreted in the adjustment layer's layer space, which for an untransformed comp-sized adjustment solid equals comp space (UNVERIFIED for transformed adjustment layers).
  - Any AV layer can be switched to an adjustment layer (Adjustment Layer switch); its pixel colours are then ignored and only its alpha is used as `A`. Switching it off makes it a normal layer again.
  - Layer > New > Adjustment Layer (Ctrl+Alt+Y / Cmd+Option+Y) creates a white comp-size solid named "Adjustment Layer N" with the switch on (solid stored in the Solids folder).
  - Blend modes on adjustment layers are honoured (classic "glow" trick: adjustment layer with a blur set to Screen/Add).
- **Priority:** P0
- **Owner:** bae-render (+ bae-effects, bae-model)
- **Notes:** Scripting `AVLayer.adjustmentLayer` (Boolean). Not representable in Lottie (BAE export: precompose the layers below and apply the effects to the precomp, or warn). Implementation hint: when `A` is a full-frame constant 1 (common case), skip the lerp and pass the effect output through; the ROI of `E` can be limited to the bounding box of `A`.

### AE-CMP-050 · Adjustment layers with 3D, precomps and mattes (调整图层与 3D、预合成和遮罩的交互（译名待核）)

- **What it does:** Specifies the boundary cases of adjustment layers.
- **Exact semantics:**
  - 3D adjustment layer: its footprint `A` is the layer's projected (camera-transformed) alpha, but the effects still process the 2D composite of everything below it in stacking order. UNVERIFIED: whether 3D layers in front of the adjustment layer in depth but below it in stacking order are adjusted (stacking order wins in AE's Classic 3D: adjustment layers act on the stacking-order composite). An adjustment layer splits 3D layers above and below it into separate 3D groups (no intersections or shadows across it) — see compositing-3d-color.md, Classic 3D renderer.
  - Inside a precomp: it affects only layers below it inside that precomp. If that precomp layer has Collapse Transformations on, the adjustment layer forces the nested layers to be flattened and cropped (Adobe help: an adjustment layer "will force the flattening and cropping that collapsing transformations would usually prevent").
  - Track matte on an adjustment layer: limits the adjustment to the matte (multiplies `A`). Using an adjustment layer as a matte source for another layer: UNVERIFIED (its source alpha is used).
  - Masks on an adjustment layer: the standard way to restrict effects to a region (Adobe help); animated masks may follow subjects.
  - Motion blur on an adjustment layer blurs only its footprint `A`, not the content below (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** —

## Collapse Transformations and Continuous Rasterization

### AE-CMP-051 · Collapse Transformations for precomp layers (折叠变换)

- **What it does:** With the switch on, a precomp layer is not flattened into a comp-sized image: its nested layers are transformed straight into the containing comp (transforms concatenated), so resolution is preserved, content outside the nested comp frame is kept, nested 3D layers join the outer 3D scene, and nested blend modes act on the outer comp's layers.
- **Exact semantics:**
  ```
  render_collapsed(P, acc, t):                  # P = precomp layer with Collapse on
    if the nested comp contains an (active) adjustment layer:
        # Adobe help: the adjustment layer "will force the flattening and cropping that
        # collapsing transformations would usually prevent" -> behave as not collapsed
        return composite(nested_frame(P, t) transformed by P (AE-CMP-054), acc, ...)
    if P has (a closed mask with mode != None) or (an enabled effect) or (a layer style):
        # forced rasterisation ("partial collapse"; Adobe help: nested layers "are first
        # rendered on their own, then masks and effects are applied")
        img = render nested layers with transforms = outer(P) ∘ nested(layer), into a
              comp-space buffer of the OUTER comp size (UNVERIFIED: not cropped to the
              nested frame; transforms still concatenated, so resolution is preserved)
        img = apply P's masks (mapped to comp space), effects, layer styles
        acc = composite(img, acc, P.blend_mode, P.preserve_transparency)
        # nested blend modes affect only nested layers; 3D layers cannot intersect
        # or shadow across this boundary
    else:
        for each nested layer N bottom-to-top:
            N.transform = outer_transform(P) ∘ N.transform            # concatenated
            N.opacity  *= P.opacity/100                                # UNVERIFIED: per layer
            composite N directly onto acc with N's own blend mode      # nested modes pass through
        nested 3D layers are depth-sorted/intersected with the outer 3D group and use the
        outer comp's active camera and lights (nested cameras/lights ignored, UNVERIFIED)
  ```
  - Nested comp background colour is never rendered (always transparent).
  - The precomp layer's own blend mode: in the full-collapse case the nested layers keep their own modes and P's mode is ignored (UNVERIFIED; in practice AE greys out / ignores P's mode when collapsed without masks/effects).
  - Without the switch: the nested comp is rendered to an image of the nested comp's size (cropped at its frame), using the nested comp's own camera (or default camera) for its 3D layers, and treated as a 2D or 3D footage layer.
  - Quality switch and motion blur of P propagate to the nested layers as per "Switches Affect Nested Comps" (AE-CMP-057).
  - Collapse Transformations is also the Continuously Rasterize switch position for vector layers (AE-CMP-052) — same switch column, different icon/meaning.
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Scripting `AVLayer.collapseTransformation` (Boolean; for vector layers it is the Continuously Rasterize state). Adobe help (Precomposing > Collapsing transformations) is the source of the 3D, cropping, adjustment-layer and forced-rasterisation rules. Lottie: precomp layers are always "collapsed" in players (no cropping by `w`/`h` unless the player clips; lottie-web clips precomps to `w`×`h`) — UNVERIFIED cross-player. Cost: full collapse avoids an intermediate buffer (cheaper and sharper) but prevents caching the nested comp as one image. Implementation hint: represent collapsed precomps in the render graph as an inlined sub-graph with a pushed transform; forced rasterisation inserts an offscreen node in comp space.

### AE-CMP-052 · Continuously Rasterize for vector layers (连续栅格化)

- **What it does:** For vector footage layers (Illustrator AI, PDF, EPS) the switch makes AE re-rasterise the vectors every frame at their transformed size so they stay sharp at any scale; shape and text layers are always continuously rasterized.
- **Exact semantics:**
  - Off (default for vector footage): the vector file is rasterised once at its document size (100 %) into a bitmap, then follows the raster pipeline (AE-CMP-007); scaling above 100 % blurs/pixelates.
  - On: render order becomes masks → transformations → effects (AE-CMP-008); the vector is rasterised in comp space with the full layer transform at the current preview resolution.
  - Effects on a continuously rasterized layer receive a comp-space raster, so their results differ from the switch-off case (e.g. a Drop Shadow's distance is in comp pixels and does not scale with the layer).
  - Best quality anti-aliases vector edges whether or not continuous rasterization is on (Adobe help, Layers > Continuous rasterization); Draft renders without anti-aliasing (Adobe help, Layer image quality: Draft "displays and renders a layer without anti-aliasing and subpixel positioning").
  - Shape and text layers: switch shown but always on; scripting state UNVERIFIED for these layer types.
  - Cost: re-rasterisation each frame (slower but sharper).
- **Priority:** P0 for shape/text (always on); P1 for AI/PDF/EPS footage switch.
- **Owner:** bae-render (+ bae-vector, bae-media)
- **Notes:** BAE decision: vector footage import (AI/PDF/EPS) is a bae-media concern (see project-comp-layers.md, Import); BAE converts it to a Vello scene so both switch states can be honoured. Implementation hint: when off, cache the 100 % raster once per source; when on, render Vello each frame into the ROI.

### AE-CMP-053 · Collapsed or continuously rasterized layers: bounds, 3D and nested-switch effects (折叠/连续栅格化图层的边界与 3D 交互（译名待核）)

- **What it does:** Summarises the side effects users rely on when the switch is on.
- **Exact semantics:**
  - Bounds: collapsed/continuously rasterized content is not cropped to the nested comp frame or vector artboard; layer bounds become the transformed content bounds, clipped only by the outer comp's render region (AE-CMP-063 Layer bounds).
  - 3D: the precomp layer must itself have its 3D switch on for its collapsed nested 3D layers to join the outer 3D scene (outer camera, lights, shadows, intersections). UNVERIFIED: with the precomp layer 2D, AE renders the nested 3D layers through the outer comp's camera but composites them as a closed 2D group at the precomp layer's stacking position.
  - The 3D switch of a continuously rasterized vector layer works as for any layer.
  - Motion blur, quality and frame blending switches on a collapsed layer: propagate into the nested layers (AE-CMP-057 Switches Affect Nested Comps).
  - Track matte with a collapsed precomp (either side): forces flattening (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** —

## Nested compositions

### AE-CMP-054 · Nested composition rendering (嵌套合成的渲染（译名待核）)

- **What it does:** A precomp layer renders its source comp at the mapped time into an image the size of the nested comp, then treats it like footage in the outer comp (unless collapsed, AE-CMP-051).
- **Exact semantics:**
  ```
  nested_frame(P, t_outer):
    t_n = layer_time(P, t_outer)            # start time, stretch, time remap
                                            # (see transform-keyframes-expressions.md, Time remapping)
    if nested.preserve_frame_rate: t_n = floor_to_frame(t_n, nested.frame_rate)   # AE-CMP-056
    res = nested.preserve_resolution ? nested.own_resolution : outer render resolution factor (AE-CMP-055)
    img = render_comp(nested, t_n) at nested width × height × res     # AE-CMP-006
    # background transparent; content outside the nested frame is cropped
  ```
  - Nested comps are rendered before the outer layers that use them (Adobe help, render order).
  - Image size = nested comp frame size (scaled by the resolution factor); layer bounds of the precomp layer = that rectangle (AE-CMP-063 Layer bounds).
  - Nested comp's pixel aspect ratio: the image is treated as footage with the nested comp's PAR and resampled to the outer comp's PAR (UNVERIFIED exact rule; see project-comp-layers.md, Composition Settings).
  - Nested comp's 3D layers: rendered with the nested comp's active camera (or the default camera) into a flat image, unless collapsed.
  - Guide layers inside the nested comp are excluded (AE-CMP-006).
  - Recursion: nesting depth is unbounded; a comp cannot contain itself directly or indirectly (AE refuses the operation).
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Lottie: precomp layer `ty: 0`, `refId` → `assets[].layers`, `w`/`h` (clip size), `tm` (time remap, seconds); `sr` stretch, `st` start time. Implementation hint: a nested comp is a sub-graph rendered into a cached texture keyed by (comp id, nested time, resolution, switch-propagation state, content hash) — see AE-CMP-058.

### AE-CMP-055 · Preserve Resolution When Nested (嵌套时保留分辨率)

- **What it does:** By default a nested comp renders at the containing comp's resolution factor (Full, Half, Third, Quarter, Custom); with this Composition Settings > Advanced option it keeps its own resolution setting.
- **Exact semantics:**
  - Off (default): nested resolution factor := factor of the comp being rendered/previewed (propagates recursively down the nesting chain).
  - On: the nested comp renders at its own Resolution setting (e.g. a comp set to Quarter stays Quarter even when the outer comp is Full); the result is upsampled into the outer comp.
  - Not inherited by new comps (Adobe help: new comps do not inherit Preserve frame rate / resolution settings from the previous Composition Settings).
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Scripting `CompItem.preserveNestedResolution`. Resolution factor semantics: AE-CMP-062 (Resolution downsampling) and compositing-3d-color.md, Preview resolution.

### AE-CMP-056 · Preserve Frame Rate When Nested or in Render Queue (嵌套时或在渲染队列中保留帧速率)

- **What it does:** Makes a nested comp (or a comp rendered in the Render Queue with a different frame rate) sample its animation only at its own frame times, instead of at the outer comp's times.
- **Exact semantics:**
  - Off (default): the nested comp is evaluated at the exact nested time `t_n` derived from the outer frame (animation is resampled at the outer frame rate; e.g. a 12 fps comp nested in 24 fps shows 24 distinct positions per second).
  - On: `t_n` is quantised down to the nested comp's frame grid: `t_n' = floor(t_n * fps_n + eps) / fps_n` (UNVERIFIED rounding: floor vs nearest); same when the comp is rendered with a Render Settings frame-rate override.
  - Footage frames inside the nested comp are always chosen by the footage's own frame rate regardless of this switch.
  - Recommended by Adobe instead of Posterize Time for intentionally choppy animation.
- **Priority:** P1
- **Owner:** bae-anim (+ bae-render, bae-model)
- **Notes:** Scripting `CompItem.preserveNestedFrameRate`. SDK: with PFR on, effects' `current_time` is restricted to multiples of the time step. Implementation hint: in flicks the quantisation is exact integer division (705,600,000 / fps for integer rates; NTSC rates use the comp's flick frame duration).

### AE-CMP-057 · Switches Affect Nested Comps (开关影响嵌套合成)

- **What it does:** A preference that makes the Quality, Effects (fx), Frame Blending and Motion Blur switches of a precomp layer propagate recursively into the layers of the nested comp.
- **Exact semantics:**
  - Preference location: Preferences > Composition in current AE (Adobe help, Precomposing and Preferences pages: "Quality, effects, frame blending, and motion blur switches affect nested compositions recursively"); older help pages place it under General / Appearance. Default on (UNVERIFIED: default not stated by Adobe; long-standing AE default is checked).
  - When on, for every layer N rendered inside a nested comp reached through precomp layer P (recursively):
    - Quality: `N.quality = min(N.quality, P.quality)` in order Wireframe < Draft < Best (UNVERIFIED: P = Draft forces nested layers to Draft; P = Best leaves them as set).
    - Effects switch: P's fx switch off disables effects on nested layers (the preference text lists "effects" among the propagated switches; exact rule UNVERIFIED: assumed `N.effects_on = N.effects_on && P.effects_on`).
    - Motion Blur: nested comps obey the containing comp's Enable Motion Blur comp switch (Adobe help, Motion blur: "nested compositions obey the setting for the compositions in which they're contained"); nested layers blur only if their own MB switch is on and P's MB switch is on (UNVERIFIED for the layer-switch AND); shutter settings come from the outer (rendering) comp (UNVERIFIED).
    - Frame Blending: frame blending cannot be applied to a precomp layer itself (Adobe help, Frame blending), so propagation concerns the comp-level Enable Frame Blending switch: nested footage layers frame-blend only if the containing (rendering) comp's switch is on (UNVERIFIED by analogy with motion blur).
  - When off, nested layers use only their own switches and the nested comp's own comp switches.
  - Draft 3D and comp-level switches: UNVERIFIED propagation.
- **Priority:** P1
- **Owner:** bae-render (+ bae-model)
- **Notes:** Preferences dialog: ui-preview-render-io.md, Preferences. Golden test: nested comp with a fast-moving MB-enabled layer, toggle the precomp layer's MB switch with the preference on/off.

### AE-CMP-058 · Nested comp frame caching (嵌套合成帧缓存（译名待核）)

- **What it does:** A nested comp frame that is needed several times for the same outer frame (or several outer frames mapping to the same nested time) is rendered once and reused.
- **Exact semantics:**
  - Cache key: (nested comp id, nested time `t_n` in flicks, resolution factor, propagated switch state (quality, MB, frame blending, fx), downstream ROI, project bit depth/colour settings, content version of the nested sub-tree).
  - Reuse cases: the same precomp used by several layers at the same time; held frames (time remap holds, PFR quantisation, stretch < 1 producing repeated nested frames); motion-blur sub-samples of the precomp layer's own transform (the nested content is rendered once when only the outer transform moves within the shutter — UNVERIFIED that AE does this; AE re-renders nested comps per sub-sample when the nested content itself animates).
  - Invalidation: any change inside the nested sub-tree (via StageEvents) invalidates entries with that comp id.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** BAE decision: cache keys per ARCHITECTURE 5 are (Prim, time, parameter hash); the nested comp's parameter hash includes the propagated switches. Implementation hint: GPU texture cache with LRU eviction under a VRAM budget; RAM-preview cache sits on top (see compositing-3d-color.md, Preview).

## Quality and sampling

### AE-CMP-059 · Layer quality: Best, Draft, Wireframe (图层品质：最佳、草图、线框)

- **What it does:** A per-layer switch that trades rendering precision for speed, both in previews and final renders.
- **Exact semantics:**

  | Value (中文) | ExtendScript `LayerQuality.` | Rendering |
  |---|---|---|
  | Best (最佳) | `BEST` (default) | subpixel positioning, anti-aliasing, 3D shading, effects at full precision (`PF_Quality_HI`); sampling per AE-CMP-060 |
  | Draft (草图) | `DRAFT` | no anti-aliasing, no subpixel positioning (positions snapped to whole pixels), nearest-neighbour resampling, effects told `PF_Quality_LO` and may compute approximately |
  | Wireframe (线框) | `WIREFRAME` | layer drawn only as its bounding-box outline; contents not rendered |

  - Clicking the Quality switch cycles Best (Bilinear) → Draft → Best (Bicubic) (Adobe help); Layer > Quality offers all options including Wireframe.
  - Duplicated or split layers keep the setting. Quality also affects the precision of the layer when used as a control layer for compound effects and in motion tracking.
  - Draft nearest-neighbour: UNVERIFIED exact rounding (pixel centre lands on `floor(x + 0.5)`).
  - Wireframe in final render: UNVERIFIED (AE renders the box outline in the layer's label colour? Best understanding: wireframe layers render nothing in final output).
  - Propagation into nested comps: AE-CMP-057.
- **Priority:** P0 (Best and Draft); Wireframe P2.
- **Owner:** bae-render (+ bae-model, bae-effects)
- **Notes:** Scripting `AVLayer.quality`. Render Settings can override all layers' quality (Current Settings / Best / Draft / Wireframe; ui-preview-render-io.md, Render Queue). Lottie has no quality concept. Implementation hint: Draft = `textureSample` with a nearest sampler and integer-snapped translation; effect shaders get a `quality` uniform so they can drop sample counts.

### AE-CMP-060 · Sampling: Bilinear and Bicubic (双线性和双立方采样)

- **What it does:** For Best-quality layers, chooses the resampling filter used for the layer transform (scaling, rotation, subpixel translation) and the Transform effect.
- **Exact semantics:**
  - Values (scripting `AVLayer.samplingQuality`, `LayerSamplingQuality.`): Bilinear (双线性, `BILINEAR`, default), Bicubic (双立方, `BICUBIC`). Only meaningful when quality = Best. Shortcuts Alt/Option+B (Best/Bilinear), Alt/Option+Shift+B (Best/Bicubic).
  - Bilinear: standard 2×2 tap interpolation on premultiplied (alpha-weighted) colour; for downscaling AE uses area sampling / mip-like prefiltering (UNVERIFIED: AE Best quality downscales with a box-area filter rather than plain bilinear, avoiding aliasing).
  - Bicubic: 4×4 tap cubic; kernel UNVERIFIED (Keys a = −0.5 assumed); can ring/overshoot at hard edges (Adobe help), helps more for upscaling; results clamped to [0, 1] at 8/16 bpc, and overshoot may produce < 0 or > 1 in 32 bpc (UNVERIFIED).
  - Not used by (Adobe help): textures in the Ray-traced 3D renderer (always bilinear); transformations inside effects (bilinear unless the effect implements its own method). UNVERIFIED for the Advanced 3D renderer.
  - Samples outside the layer are transparent (zero alpha), not edge-clamped.
- **Priority:** P1
- **Owner:** bae-render
- **Notes:** UNVERIFIED version of introduction (a CC-era addition, around AE CC 2014.1-2015, 13.1-13.5). Source: https://helpx.adobe.com/after-effects/using/layers.html (Bilinear and bicubic sampling; shortcuts Alt/Option+B and Alt/Option+Shift+B; Quality switch cycles Best (bilinear) → Draft → Best (bicubic)). Implementation hint: bilinear = hardware sampler on premultiplied textures with mipmaps for strong minification (trilinear when scale < 0.5); bicubic = 4-tap optimised B-spline/Catmull-Rom shader using bilinear fetches.

### AE-CMP-061 · Subpixel positioning and precision (子像素定位)

- **What it does:** Layer positions and other spatial values are fractional; in Best quality a layer placed between pixel boundaries is resampled (slight softening), in Draft it snaps to whole pixels.
- **Exact semantics:**
  - All spatial values (Position, Anchor Point, effect points) are computed to a precision of 1/65,536 pixel (Adobe help).
  - Best: the transform matrix is used unrounded; a fractional offset produces interpolation blur similar to anti-aliasing.
  - Draft: translation is rounded to whole pixels (UNVERIFIED rounding rule), giving visible stepping in slow moves.
  - A layer at integer position, 100 % scale, 0° rotation and matching PAR is copied pixel-exactly in Best quality (no filtering), which users rely on for crisp UI/graphics.
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Implementation hint: detect identity-up-to-integer-translation transforms and use a copy/blit path (exactness and speed); keep layer matrices in f64 on the CPU and convert to f32 only after subtracting the ROI origin to avoid precision loss on large comps.

### AE-CMP-062 · Resolution downsampling factor (分辨率/降采样因子（译名待核）)

- **What it does:** When a comp is previewed or rendered at reduced resolution (Half, Third, Quarter, Custom), every layer and effect is rendered with a downsample factor so the image looks like a scaled-down full-resolution render.
- **Exact semantics:**
  - Factor `(dx, dy)` with `dx, dy ∈ {1, 2, 3, 4, custom}`; render buffers are `ceil(width / dx) × ceil(height / dy)` (UNVERIFIED rounding).
  - All pixel-distance parameters are scaled by `1/dx`, `1/dy`: effect sliders measured in pixels (blur radius, glow radius, stroke width...), mask feather and expansion, layer-style sizes, motion-blur distances (automatic via transforms), shape stroke widths and text sizes (automatic via transforms). Effects receive `downsample_x/y` (SDK `PF_InData`) and must scale scalar pixel parameters themselves; point parameters and layer-parameter dimensions are pre-scaled by AE.
  - Non-pixel parameters (angles, percentages, colours, counts, random seeds) are not scaled.
  - Effects that cannot match full resolution exactly (e.g. integer-radius kernels, noise patterns) may look different at reduced resolution; this is accepted AE behaviour.
  - Preview resolution UI (Composition panel menu, Adaptive Resolution, Auto) is in compositing-3d-color.md, Preview resolution.
- **Priority:** P0 — without consistent downsampling, half-resolution previews look wrong.
- **Owner:** bae-render (+ bae-effects, bae-vector, bae-text)
- **Notes:** Scripting `CompItem.resolutionFactor` ([x, y]). BAE decision: effects declare which parameters are pixel distances in their schema so the host can pre-scale them; plug-ins via the AE-compat layer still receive `downsample_x/y` (ARCHITECTURE 15).

## Layer bounds and buffers

### AE-CMP-063 · Layer pixel bounds, comp-edge clipping and buffer limits (图层边界与合成边缘裁剪（译名待核）)

- **What it does:** Defines the extent of each layer's pixel buffer through the pipeline and where content is cropped.
- **Exact semantics:**
  - Source bounds: footage frame size; solids/adjustment solids their size; text and shape layers the bounds of their rendered content (plus stroke/effects); precomp layers the nested comp frame (AE-CMP-054) unless collapsed.
  - Masks never enlarge bounds (they only reduce alpha); feather is computed only inside the layer frame (AE-CMP-041).
  - Effects see the whole layer, including parts outside the comp frame (a blur near the comp edge uses pixels outside it). Effects may expand the buffer (Drop Shadow, Glow, blurs without "Repeat Edge Pixels", Grow Bounds), so the effect output can extend beyond the source frame; effects that do not expand clip their output to the input buffer (a classic AE artefact: a blur on a layer cut off at the layer edge). See effects.md, effect framework (output extent) and Grow Bounds (Utility).
  - After the transform, everything is clipped to the comp frame (plus the render ROI). Content outside a nested (non-collapsed) comp's frame is cropped when that comp is used as a layer.
  - Maximum comp and layer size 30,000 × 30,000 px (SDK: effect output buffers are clamped to 30,000 × 30,000).
  - Empty bounds (zero-area content, fully transparent after masks) → layer skipped downstream, but still evaluated for expressions.
- **Priority:** P0
- **Owner:** bae-render (+ bae-effects)
- **Notes:** SDK `extent_hint` = smallest rectangle containing all non-zero-alpha pixels; BAE keeps an equivalent per-buffer "content rect" to shrink work. Implementation hint: every render-graph node propagates (input rect → output rect) and (requested ROI → needed input ROI) like AE SmartFX PreRender, so off-screen work is skipped.

## Motion blur and frame blending rendering

### AE-CMP-064 · Motion blur rendering (运动模糊)

- **What it does:** For layers with the Motion Blur switch on (and the comp's Enable Motion Blur switch on for previews, or Render Settings motion blur on for output), the layer is rendered at several sub-frame times within the shutter interval and the results are averaged.
- **Exact semantics:**
  ```
  if comp.mb_enabled and layer.mb_switch (with nested-switch rules AE-CMP-057):
      times = motion_blur_sample_times(layer, t)       # shutter angle/phase, samples per frame,
                                                       # adaptive limit: see transform-keyframes-
                                                       # expressions.md, motion-blur sampling
      img = (1/N) * Σ_i  render_layer_at(layer, times[i])        # equal (box) weights, premultiplied
  else:
      img = render_layer_at(layer, t)
  ```
  - What is re-evaluated per sub-sample: the layer transform (incl. parent chain, camera for 3D), opacity, masks set to Same As Layer/On (AE-CMP-046), shape and text content of continuously rasterized layers (their path animation is blurred — shape layers always use Samples Per Frame), and nested comps when the precomp layer is collapsed or nested layers have MB (each nested layer blurs according to its own switch).
  - Not re-evaluated: effects on raster layers (rendered once at frame time, then transformed per sample) unless the effect supports shutter-angle motion blur itself (SDK `PF_OutFlag_I_USE_SHUTTER_ANGLE`; e.g. Transform effect, CC Force Motion Blur family); motion inside footage (Adobe help: the MB switches cannot blur motion that already exists within a layer; use frame blending or effects such as Pixel Motion Blur instead).
  - Averaging is in premultiplied space so edges blur correctly; 8/16 bpc clamping applies after averaging (UNVERIFIED).
  - 2D layers: number of samples adapts to the layer's motion between Samples Per Frame (minimum; default 16) and Adaptive Sample Limit (maximum; default 128); 3D layers and shape layers always use Samples Per Frame (Adobe help, Composition Settings > Advanced). Defaults and ranges are specified in project-comp-layers.md, Composition Settings.
  - Track mattes: the matte layer is blurred according to its own MB switch; the matted layer's blurred image is then multiplied by the (separately blurred) matte value (UNVERIFIED: AE applies the matte per sample vs after averaging).
  - Adjustment layers: blur only their footprint (AE-CMP-050).
  - 3D layers: each sample renders the layer through the camera at the sample time; camera motion itself blurs every 3D layer with MB on.
- **Priority:** P0
- **Owner:** bae-render (+ bae-anim)
- **Notes:** Scripting `AVLayer.motionBlur`, `CompItem.motionBlur`, `CompItem.shutterAngle` (0-720), `shutterPhase` (−360…360), `motionBlurSamplesPerFrame`, `motionBlurAdaptiveSampleLimit`. Lottie has no motion blur (some players offer a global MB option). Cost: up to N× the per-layer transform/resample cost. Implementation hint: for raster layers render the pre-transform image once and accumulate N transformed draws of the same texture into an RGBA16F/32F accumulator with additive blending and weight 1/N (one pass, no extra buffers); for rigid 2D transforms an analytic line-integral sampler (k taps along the motion vector per pixel) is cheaper and visually equivalent; only re-rasterise per sample when content (masks, shapes, text, nested comps) changes within the shutter.

### AE-CMP-065 · Frame blending rendering (帧混合)

- **What it does:** For footage layers (video or image sequences) whose source frames do not line up with the comp's frames (time stretch, time remap, conformed footage), creates intermediate frames by blending source frames instead of repeating them. Not applicable to precomp layers or single stills (Adobe help: "You can't apply frame blending to a precomposition layer"; apply it to the footage layers inside the nested comp instead).
- **Exact semantics:**
  - Layer switch values (scripting `AVLayer.frameBlendingType`, `FrameBlendingType.`): Off (`NO_FRAME_BLEND`), Frame Mix (帧混合, `FRAME_MIX`), Pixel Motion (像素运动, `PIXEL_MOTION`). Only applied when the comp's Enable Frame Blending switch is on (previews) / Render Settings frame blending is on (output).
  ```
  Frame Mix (measured in AE 2026, 9 cases, weights fit within 0.02):
    s = source time for this comp frame × source fps     # fractional source frames
    v = source frames one comp frame spans               # |remap(t+T/2) − remap(t−T/2)|·fps,
                                                         # or T·100/|stretch|·fps
    h = 0.74 if v <= 1 else 0.93·v                       # tent half-width
    w(k) = tent_cdf(k + 0.5 − s, h) − tent_cdf(k − 0.5 − s, h)
    img = Σ w(k) · frame(clamp(k, 0, last))              # premultiplied, Σ w = 1
    # frame-aligned s still mixes about 89 / 5.5 / 5.5 %
  Pixel Motion:
    img = optical-flow interpolation between frame(f0) and frame(f0+1) at phase w
          (motion vectors estimated per pixel; AE's algorithm is proprietary)
  ```
  - Frame blending applies before masks/effects (it produces the layer's source frame).
  - Quality interaction: a frame-blended layer in Draft quality always uses Frame Mix, even if Pixel Motion is selected (Adobe help).
  - Gate: the comp's Enable Frame Blending switch disables frame blending for all layers in that comp regardless of layer switches (Adobe help); Render Settings can enable it for output.
- **Priority:** P1 (Frame Mix); Pixel Motion P2.
- **Owner:** bae-render (+ bae-media, bae-anim)
- **Notes:** Source: https://helpx.adobe.com/after-effects/using/time-stretching-time-remapping.html (Frame blending). zh-CN: menu 图层 > 帧混合 > 帧混合 / 像素运动. Which source frames and weights are used is defined in transform-keyframes-expressions.md, frame blending; this item covers only the pixel combination. Cost: Frame Mix decodes two frames per output frame (cache-friendly); Pixel Motion needs a flow pass (GPU compute, e.g. pyramidal Lucas-Kanade/DIS) — BAE decision: Pixel Motion may ship later behind Frame Mix. **BAE (2026-11-25):** Frame Mix is implemented as above (`bae-render` `frame_blend.rs`; one source node per frame summed with Alpha Add; at most 64 frames). Pixel Motion renders as Frame Mix with a warning until the optical-flow pass (shared with Pixel Motion Blur and Timewarp) lands.

## Other compositing behaviour

### AE-CMP-066 · Effects switch and per-effect enable in rendering (效果开关和单个效果启用（译名待核）)

- **What it does:** The layer's Effects (fx) switch bypasses the layer's whole effect stack; each effect's own enable checkbox bypasses that effect only.
- **Exact semantics:**
  - Layer fx switch off: step 3 of AE-CMP-007 (and step 4 of AE-CMP-008) is skipped entirely; masks, transform, styles, matte and blend still apply. For an adjustment layer it disables the adjustment (result = composite below, unchanged).
  - Effect enable off: that effect is removed from the chain; the next effect receives the previous effect's output.
  - Disabled effects are not evaluated (no rendering cost); their expressions are still evaluated when referenced elsewhere (UNVERIFIED).
  - Render Settings "Effects: Current Settings / All On / All Off" overrides for output (see ui-preview-render-io.md, Render Queue).
  - Nested propagation of the fx switch: AE-CMP-057.
- **Priority:** P0
- **Owner:** bae-render (+ bae-model)
- **Notes:** Scripting `AVLayer.effectsActive`; per effect `PropertyBase.enabled`. Lottie: effects carry `en` (enabled) flags; lottie-web supports only a few effect types.

### AE-CMP-067 · Pixel aspect ratio conversion during compositing (合成时的像素长宽比转换（译名待核）)

- **What it does:** Layers whose source pixel aspect ratio (PAR) differs from the comp's PAR are horizontally rescaled as part of the layer transform so they look correct in the comp.
- **Exact semantics:**
  ```
  par_scale_x = source.PAR / comp.PAR          # applied in layer space before Scale/Rotation
  M_layer = Translate(Position) * Rotate * Scale * Scale(par_scale_x, 1) * Translate(-AnchorPoint)
  ```
  - Anchor Point and mask coordinates stay in source pixels (unscaled); the comp viewer can additionally apply "Pixel Aspect Ratio Correction" for display only (ui).
  - Nested comps use the nested comp's PAR as their source PAR (AE-CMP-054).
  - The conversion is a real resample of raster layers (filtered per Quality/Sampling); continuously rasterized and collapsed layers include it in their transform without an extra resample.
  - Effects on a layer operate in source pixels (non-square); AE adjusts effect point parameters for PAR (SDK pixel-aspect-ratio notes: effects receive `pixel_aspect_ratio` and must compensate for circular kernels).
- **Priority:** P1 — matters for anamorphic/DV footage; square-pixel projects are unaffected.
- **Owner:** bae-render (+ bae-anim, bae-media)
- **Notes:** PAR values come from Interpret Footage and Composition Settings (project-comp-layers.md). UNVERIFIED: exact position of the PAR scale in the matrix chain (assumed innermost, i.e. applied to source pixels first). Lottie assumes square pixels.

