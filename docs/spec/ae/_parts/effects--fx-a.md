# After Effects inventory: effects (AE-FX)

**Scope.** This file specifies the AE effect framework and every built-in effect BAE must provide. The framework covers the effect stack and its render order, enable switches, effect instance data, Compositing Options (Effect Opacity, Mask Reference), parameter types and how each one animates, parameter IDs and match names, bit-depth support (8/16/32 bpc), GPU flags, output-bound expansion, downsample scaling, layer parameters, Reset / Remove All, Animation Presets and legacy effect versions. For each built-in effect it gives the exact matchName, category, bpc and GPU support, every parameter in AE order (match name, type, default, range, units), an implementable algorithm, whether it expands bounds, and the Lottie effect `ty` where one exists. Sibling files: README.md, project-comp-layers.md, transform-keyframes-expressions.md, compositing-3d-color.md, shapes.md, text.md, effects.md, ui-preview-render-io.md. The item format, ID scheme, priorities and owners are defined in README.md.

**Conventions used in this file.**

- Colours are normalised: 0.0 = black, 1.0 = white in every bit depth (8 bpc 255, 16 bpc 32768 = 1.0; see compositing-3d-color.md, Pixel and alpha model). `R, G, B, A` are straight (unpremultiplied) channel values of the effect's input pixel; `R', G', B', A'` the output.
- `lerp(a, b, t) = a + (b - a) * t`; `clamp01(x) = min(max(x, 0), 1)`; `luma709(c) = 0.2126 R + 0.7152 G + 0.0722 B`; `luma601(c) = 0.299 R + 0.587 G + 0.114 B`.
- "bpc" and "GPU" values for each effect come from the docsforadobe first-party effect match-name table (columns "BPC" and "GPU": AE version in which GPU acceleration was added). An empty GPU column means CPU only.
- Parameter tables use the columns `| # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |`. `#` is the parameter's 1-based property index inside the effect group (`effect.property(#)`), which is also the numeric suffix of the parameter match name: AE gives every built-in effect parameter the match name `"<effect matchName>-NNNN"` (4 digits, 1-based, counted over the effect's parameter list including group start markers, see AE-FX-007). Where a parameter match name is written with "UNVERIFIED", the index was derived from the documented UI order and not checked against an AE project.
- Range is written "slider a…b; valid c…d". The slider range is what dragging in Effect Controls covers; the valid range is what typing or expressions can reach (values outside valid are clamped).
- "Expands bounds: yes/no" says whether the effect can draw outside the input layer rectangle (PF_OutFlag_I_EXPAND_BUFFER, AE-FX-017).

## Effect framework

### AE-FX-001 · Effect stack and effect render order (效果堆栈和渲染顺序)

- **What it does:** Every AV layer (footage, solid, comp, shape, text, adjustment, null) owns an ordered list of effect instances, the Effects group; effects process the layer's image one after another, top to bottom.
- **Exact semantics:**
  - Container: layer property group `"ADBE Effect Parade"` (display name "Effects", 效果). Effect instances are its children, index 1 = top. Camera, light and audio-only layers have no Effects group (audio layers get audio effects only; see effects.md, Audio effects).
  - Per-layer render order (full pipeline in compositing-3d-color.md, Render order): layer source (footage frame / rasterised vector / nested comp) → masks (mask paths, feather, expansion, modes; applied to the layer's alpha) → effects in stack order, index 1 first → layer styles → transform (with motion blur) → track matte → blend with layers below.
  ```
  img = render_source(layer, t)
  img = apply_masks(img, layer.masks, t)          // unless an effect references masks via Compositing Options
  for fx in layer.effects (index 1..n):
      if layer.fx_switch && fx.enabled:
          out = fx.render(img, params_at(fx, t), ctx)
          img = composite_options(img, out, fx, t)  // AE-FX-005/006
  img = apply_layer_styles(img)
  ```
  - Reordering an effect (drag in Effect Controls or Timeline) changes its index and therefore the result; there is no automatic sorting.
  - Each effect receives the output of the effect above as its input layer `param[0]` (SDK: "All effects automatically have at least 1 layer parameter, param[0], the layer to which they are applied").
  - Adding an effect inserts it at the bottom of the stack when nothing is selected; when an effect is selected in Effect Controls the new effect is inserted below it (UNVERIFIED: AE 2015+ behaviour; older versions always appended at the bottom).
- **Priority:** P0 — the basis of every effect.
- **Owner:** bae-render (+ bae-model, bae-effects)
- **Notes:** Scripting: `layer.property("ADBE Effect Parade")` (= `layer.Effects`, `layer.effect`), `addProperty(matchName)` appends an effect; `PropertyBase.moveTo(newIndex)` reorders. Expressions: `effect("Name")` or `effect(index)` (see transform-keyframes-expressions.md, effect object). Lottie: layer `ef` array, each entry `{ty, nm, mn, ix, en, np, ef:[values]}`. The bodymovin exporter assigns effect `ty` by matchName: 20 Tint, 21 Fill, 22 Stroke, 23 Tritone, 24 Pro Levels (Levels is converted to it), 25 Drop Shadow, 26 Radial Wipe, 27 Displacement Map, 28 Set Matte (`ADBE Set Matte3`), 29 Gaussian Blur (`ADBE Gaussian Blur 2`), 30 Twirl, 31 Mesh Warp, 32 Ripple, 33 Spherize, 34 Puppet (`ADBE FreePin3`), 35 Transform (`ADBE Geometry2`); every other effect (including expression controls) is exported as `ty: 5` (custom effect) with `mn` = its matchName. Bodymovin skips disabled effects unless "include hidden data" is set. lottie-web renders only a subset of these types and treats `ty: 5` effects as data for expressions. Source: bodymovin-extension `bundle/jsx/utils/effectsHelper.jsx`. Implementation hint: build each layer's effect chain as a linear sub-graph of the render graph; adjacent pixel-independent effects (PF_OutFlag_PIX_INDEPENDENT, e.g. Levels → Curves → Tint) can be fused into one WGSL pass.

### AE-FX-002 · Apply, duplicate, copy/paste and delete effects (应用、复制、粘贴和删除效果)

- **What it does:** Basic editing commands on effect instances: apply from Effects & Presets / Effect menu, duplicate, copy/paste between layers, delete, rename.
- **Exact semantics:**
  - Apply: drag from Effects & Presets onto a layer (Timeline, Composition viewer or Effect Controls), double-click in Effects & Presets with layers selected (applies to every selected layer), or choose from the Effect menu (category submenus). If no layer is selected, double-clicking an animation preset creates a new layer and applies the preset to it; double-clicking an effect does nothing (both documented in Adobe help, Effects and animation presets overview).
  - Effect > [last applied effect] (Ctrl+Alt+Shift+E / Cmd+Opt+Shift+E) re-applies the most recently applied effect to the selected layers.
  - Duplicate (Ctrl/Cmd+D) on a selected effect creates a copy directly below it with all keyframes, expressions and Compositing Options; the copy gets a numbered name ("Blur 2").
  - Paste of an effect onto a layer: if the target layer already has exactly one instance of that effect (same matchName) and the clipboard holds one instance, the paste replaces the existing instance's values instead of adding a new one; once a layer has two or more instances of that effect, paste adds a new instance (Adobe help, Effects and animation presets overview). Pasted keyframes start at the current time (keyframe paste rules: see transform-keyframes-expressions.md, copy/paste keys).
  - Delete: select effect, press Delete/Backspace; keyframes and expressions of that effect are removed with it. Expressions elsewhere that referenced the effect by name then error (see transform-keyframes-expressions.md, expression errors).
  - Rename: select effect, press Enter/Return, type the new name. The name must be unique among the layer's effects; AE appends " 2", " 3"... to automatically created duplicates. Default name = the effect's display name in the UI language.
  - Effect instance names default to the localised display name, so expression `effect("Slider Control")` written in an English AE fails in a localised AE unless the instance was renamed or the expression uses the match name / index (UNVERIFIED: AE's expression engine also accepts the English display name in localised versions; behaviour differs between versions).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** All of these are bae-model Commands (undoable, one history step each). "Undo Delete Effect" / "Undo Remove All Effects" restore keyframes too. Effects & Presets panel and Effect Controls UI: see ui-preview-render-io.md, Effect Controls and Effects & Presets panels.

### AE-FX-003 · Effect switch and layer Effects switch (效果开关和图层效果开关)

- **What it does:** Each effect instance has an on/off switch; each layer has an "fx" switch that disables all of its effects at once.
- **Exact semantics:**
  - Per-effect switch (Effect Controls / Timeline, left of the effect name; drawn as "fx" icon, an eyeball icon in the latest AE's modernised Effect Controls per Adobe help, UNVERIFIED version): when off, the effect is skipped entirely (its input passes through unchanged), its parameters still evaluate and its keyframes are kept. Scripting: `PropertyBase.enabled` on the effect group; Lottie `en: 0|1` (bodymovin omits disabled effects by default, so exported files normally contain only `en: 1`).
  - Layer Effects switch (Switches column, "fx", 效果): when off, every effect of that layer is skipped for preview and render. Scripting: `AVLayer.effectsActive` (boolean). Not animatable.
  - Render Settings > Effects (Current Settings / All On / All Off) overrides the switches at render time (see ui-preview-render-io.md, Render Queue).
  - Expression Controls effects never change pixels; expressions keep reading their values when the effect switch is off (UNVERIFIED: AE returns the parameter value of a disabled effect; widely relied on in rigs, not documented).
  - Neither switch is keyframeable; to fade an effect in time animate Effect Opacity (AE-FX-005) or effect parameters.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render, bae-ui-motion)
- **Notes:** Disabling an effect on an adjustment layer leaves the adjustment layer acting as an empty adjustment (no change below). BAE decision: a disabled effect must be removed from the render-graph node chain (not rendered as identity) so it costs nothing.

### AE-FX-004 · Effect instance data (效果实例数据)

- **What it does:** Defines what is persisted for each effect instance in a layer's effect stack.
- **Exact semantics:**
  | Field | Type | Notes |
  |---|---|---|
  | matchName | string | Locale-independent identifier of the effect type, e.g. `"ADBE Easy Levels2"`. Scripting `PropertyBase.matchName`. Lottie `mn`. |
  | name | string | Instance name, user-editable; default = localised display name (`PropertyBase.name`, Lottie `nm`). |
  | enabled | bool | AE-FX-003. |
  | version | (major, minor, bug, stage, build) | The effect plug-in version that created the instance (SDK `PF_VERSION`, stored in the project). AE compares it on load and may run the effect's parameter upgrade path. |
  | parameters | ordered list | One property per parameter, keyed by parameter ID (AE-FX-007), each with static value or keyframes and an optional expression. |
  | compositing options | group | AE-FX-005, AE-FX-006. |
  | sequence data | opaque bytes | Per-instance private state of the effect (flattened sequence data); built-in BAE effects normally keep none. |
  | label | label index (0 = none, 1..16) | Colour label of the effect instance, shown as a swatch in Effect Controls and optionally tinting the effect row (panel menu: Show Effect Label Swatch, Use Label Color for Effect Background). New in the latest AE release documented by Adobe help (UNVERIFIED version, AE 26.x); same label set as layer labels (project-comp-layers.md, labels). |
  - Effects have no in/out times of their own; an effect is active for the whole layer duration.
  - Scripting: `effect.isEffect === true`, `effect.propertyIndex`, `effect.canSetEnabled`. `app.effects` lists installed effects with `displayName`, `matchName`, `category`.
- **Priority:** P0
- **Owner:** bae-model (+ bae-plugin)
- **Notes:** BAE decision: store `matchName` verbatim for AE effects so AE projects and Lottie round-trip; BAE-native effects use reverse-DNS style IDs and may map to an AE matchName. Unknown matchNames are kept as placeholders (AE-FX-026). Cross-ref ARCHITECTURE §14 (plugin manifest/registry) and §15 (AE plug-in compatibility, sequence data slot).

### AE-FX-005 · Compositing Options: Effect Opacity (合成选项：效果不透明度)

- **What it does:** Every effect (AE CC 2014 13.0 and later) has a Compositing Options group whose Effect Opacity blends the effect's output with its input, like a per-effect "Blend With Original".
- **Exact semantics:**
  - Group `"ADBE Effect Built In Params"` (合成选项, Compositing Options), appended after the effect's own parameters. Child `"ADBE Effect Mask Opacity"` = Effect Opacity (效果不透明度): percent, default 100%, valid 0…100%, keyframeable, expression-capable.
  ```
  // in = input of this effect, fx = effect output, o = effectOpacity/100, m = mask coverage (AE-FX-006; 1 when no references)
  w = o * m
  out = lerp(in, fx, w)       // per channel including alpha, computed on premultiplied values
  ```
  - UNVERIFIED: the blend is done on premultiplied colour (equivalent to a "normal" crossfade of the two images); for effects that change alpha (e.g. Set Matte) this yields alpha = lerp(in.a, fx.a, w).
  - At o = 100% and no mask references the effect output is used unchanged and no extra pass is needed. At o = 0 the effect is effectively bypassed (but still rendered by AE).
  - Expression Controls effects have no Compositing Options (they produce no pixels). Audio effects have none.
- **Priority:** P1 — used regularly for quick effect fades; trivial to implement.
- **Owner:** bae-render (+ bae-model, bae-effects)
- **Notes:** Scripting path: `effect.property("ADBE Effect Built In Params").property("ADBE Effect Mask Opacity")` (confirmed by an Adobe community answer, 2025). Many older effects additionally have their own "Blend With Original" parameter (与原始图像混合), which is applied inside the effect before Compositing Options. Implementation hint: fold `lerp` into the effect's own shader as a uniform `mix` when the effect is a single pass; skip it when w == 1.

### AE-FX-006 · Compositing Options: Mask References (合成选项：蒙版引用)

- **What it does:** Restricts an effect to the area of one or more masks on the same layer, without those masks cutting the layer's alpha.
- **Exact semantics:**
  - Under Compositing Options, "+" adds a mask reference, "−" removes the last one. Each reference has:
    | Parameter (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|
    | Mask Reference N (蒙版引用 N) | popup | the first mask of the layer when the reference is added with "+" | None (无), then every mask on this layer by name (Mask 1, Mask 2, ...) | — | Dynamic list ("dynamic stream" per Adobe help); renaming or adding a mask updates the menu. |
    | Inverted (反转) | checkbox | off | — | — | UNVERIFIED: per-reference invert checkbox next to the menu. |
  - Coverage: each referenced mask is rasterised with its own path, feather (incl. per-vertex feather), expansion, opacity and mode; references are combined in their order using the masks' modes (Add, Subtract, Intersect, Lighten, Darken, Difference; None contributes nothing), exactly as layer masks combine (compositing-3d-color.md, Masks). Result `m(x,y) ∈ [0,1]`, used in AE-FX-005: `out = lerp(in, fx, effectOpacity * m)`.
  - A mask that is referenced by any effect's Compositing Options does not cut the layer's alpha (UNVERIFIED: AE treats referenced masks as effect-only when their mode is None; with any other mode the mask also applies to the layer — users set the mask mode to None to use it purely as an effect mask). Adobe help: "An effect mask cannot be used to modify the alpha channel of a layer."
  - Mask Opacity scales the effect intensity inside that mask. Only masks on the same layer can be referenced; references to a deleted mask revert to None.
  - Version: Compositing Options (Effect Opacity and Mask Reference) added in AE CC 2014 (13.0, June 2014). Source: https://helpx.adobe.com/after-effects/using/mask-reference.html and CC 2014 release coverage.
- **Priority:** P1
- **Owner:** bae-render (+ bae-model, bae-vector)
- **Notes:** UNVERIFIED match names: references group `"ADBE Effect Mask Parade"`, each reference `"ADBE Effect Mask"` with `"ADBE Effect Path Stream Ref"` (menu) and `"ADBE Effect Mask Invert"` (checkbox). Scripting cannot set the referenced mask directly in AE (known limitation). Implementation hint: reuse the mask coverage rasteriser of the layer-mask pass and feed it as a texture into the per-effect mix shader.

### AE-FX-007 · Parameter identity: indices, disk IDs and parameter match names (参数标识：索引、磁盘 ID 和匹配名称)

- **What it does:** Defines how each effect parameter is identified in projects, scripts, expressions and plug-ins.
- **Exact semantics:**
  - Index: parameters are numbered 1..n in UI order (`effect.property(1)` = first visible parameter). The SDK numbers them 0..n with param[0] = input layer; SDK index k ↔ property index k.
  - Parameter groups (topics) are flattened: scripting lists both the group start and the group end as ordinary properties of the effect (value type NO_VALUE; the AE 26 dump of Shadow/Highlight shows `-0006` and `-0015` both named "More Options"), so each consumes one property index and one disk ID, and the parameters inside continue the effect-level numbering. Bodymovin exports them as `ty: 6` no-value entries (e.g. Pro Levels `-0003` RGB start, `-0009` RGB end).
  - Parameter match name: `"<effect matchName>-NNNN"`, NNNN = 4-digit zero-padded disk ID, e.g. Slider Control's slider = `"ADBE Slider Control-0001"`, Levels (Individual Controls) Red Gamma = `"ADBE Pro Levels2-0013"` (confirmed by bodymovin's hard-coded Pro Levels export table). For built-in effects the disk ID equals the 1-based index at the time the effect was designed; later-added parameters get new IDs appended even if they are displayed earlier (so disk ID ≠ display index for some effects, e.g. parameters added in newer versions). Property index and disk ID can also differ when a disk ID is hidden or shown out of order: Curves lists `-0002` (Channel) before `-0001` (Curves); Hue/Saturation starts at `-0002` (no visible `-0001`). Expressions and scripts that address by match name survive localisation; by display name they do not.
  - Disk ID (SDK `PF_ParamDef.uu.id`) is the stable key under which AE stores the parameter value in the project; a plug-in can reorder parameters in the UI while keeping disk IDs (SDK "Changing parameter orders").
  - Display name = localised UI string (e.g. "Gamma" / "灰度系数"); `PropertyBase.name` returns it.
- **Priority:** P0 — required for AE project import, Lottie (`mn`) and expressions.
- **Owner:** bae-model (+ bae-plugin, bae-anim)
- **Notes:** Lottie effect values carry `mn` (parameter match name), `nm`, `ix` (= index) and `ty` (value type, AE-FX-008..013). BAE decision: every BAE effect parameter has a permanent numeric ID (= AE disk ID for AE-compatible effects) and a match name derived from it; renaming or reordering parameters never changes the ID. See ARCHITECTURE §15 ("参数有稳定的数字 ID（对应 AE 的 disk ID）").

### AE-FX-008 · Numeric parameters: slider, float slider and angle (数值参数：滑块、浮点滑块和角度)

- **What it does:** One-dimensional numeric effect parameters shown as an underlined scrubbable value with an optional slider.
- **Exact semantics:**
  | Type | SDK | Value | Animation | Notes |
  |---|---|---|---|---|
  | Slider (integer, legacy) | `PF_Param_SLIDER` | long | linear/bezier/hold | "No longer used"; values rounded to integers. |
  | Fixed slider | `PF_Param_FIX_SLIDER` | 16.16 fixed | linear/bezier/hold | Deprecated; precision = 1/65536. |
  | Float slider | `PF_Param_FLOAT_SLIDER` | double | linear/bezier/hold | `valid_min/valid_max`, `slider_min/slider_max`, `precision` (decimal places shown), display flags (percent, pixel), optional exponential curve tolerance. |
  | Angle | `PF_Param_ANGLE` | degrees (16.16 fixed; float in expressions) | linear/bezier/hold | Dial UI shown as `Nx +D°` (revolutions + degrees); multiple revolutions allowed (value may exceed 360 or be negative). |
  - Slider range vs valid range: dragging the slider covers [slider_min, slider_max]; typing/scrubbing the number accepts [valid_min, valid_max]; expression results are clamped to the valid range (UNVERIFIED: AE clamps expression results to valid range for effect sliders; it does for most).
  - Right-click > Edit Value... opens a dialog where the user can change the slider range for that instance (stored per instance) within the valid range.
  - Units flags: pixels (value scaled by downsample factor at render time, AE-FX-018), percent (displayed with "%", stored as the displayed number, e.g. 50 not 0.5), degrees.
  - Graph editor and velocity work like any 1D property (transform-keyframes-expressions.md, Keyframes and interpolation).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-ui-motion)
- **Notes:** Lottie docs: value `ty: 0` (Slider) and `ty: 1` (Angle), value in `v` (animatable property with `a`, `k`). The bodymovin exporter, however, writes every one-dimensional interpolatable parameter, angles included, as `ty: 0`; importers must accept both. Expression `effect("X")(n).value` returns a Number.

### AE-FX-009 · Checkbox and popup parameters (复选框和弹出菜单参数)

- **What it does:** Discrete parameters: on/off checkbox and enumerated popup menu.
- **Exact semantics:**
  - Checkbox (`PF_Param_CHECKBOX`): value 0/1. Both expressions and scripting see a one-dimensional Number (`PropertyValueType.OneD`, value 0 or 1), not a boolean. `PF_ParamFlag_CANNOT_INTERP` is forced on: keyframes are always Hold. UNVERIFIED: an expression result is treated as on when non-zero (vs. a ≥ 0.5 threshold).
  - Popup (`PF_Param_POPUP`): value = 1-based item index (A_long). Items are fixed at effect definition ("Entry1|Entry2|..."; "(-" draws a separator, which still consumes an index). Keyframes are Hold only. Expression results are rounded to the nearest integer and clamped to [1, n] (UNVERIFIED rounding).
  - Lottie docs: checkbox `ty: 4`, popup/dropdown `ty: 7`; values stored as numbers (popup 1-based). Bodymovin classifies any one-dimensional parameter that cannot interpolate linearly as `ty: 7`, so AE checkboxes are exported as `ty: 7` with value 0/1; importers must accept both.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Popup menu text is localised; projects store only the index, so BAE must keep AE's item order exactly as given in each effect item below. Dropdown Menu Control (AE-FX-035) is the only popup whose items the user edits.

### AE-FX-010 · Color parameter (颜色参数)

- **What it does:** RGB colour chosen with the colour picker or eyedropper.
- **Exact semantics:**
  - `PF_Param_COLOR`: stored as RGB (alpha unused; AE always reports alpha = 1). Plug-ins read 8-bit `PF_Pixel` or float values through `PF_ColorParamSuite1`.
  - Expressions/scripting see a 4-D array `[r, g, b, a]` in 0…1 (a = 1). Float colours may exceed 1.0 when set by expression in 32 bpc projects (UNVERIFIED: the picker itself caps at 1.0 unless HDR picker; AE 2022+ allows values > 1 in 32 bpc).
  - Interpolates linearly per channel in the stored (non-linear, working-space) RGB values; no hue-space interpolation. Supports linear/bezier/hold (1D-style temporal ease shared by all channels).
  - Eyedropper samples the composition viewer at full precision of the comp (in 8 bpc projects 1/255 steps).
- **Priority:** P0
- **Owner:** bae-model (+ bae-color, bae-ui-motion)
- **Notes:** Lottie `ty: 2`, value `[r,g,b,a]` in 0…1. With a linearised working space, AE converts colour parameters from the picker's display space to working space (see compositing-3d-color.md, Color management).

### AE-FX-011 · Point and 3D Point parameters and effect control points (点和三维点参数及效果控制点)

- **What it does:** 2D (x, y) or 3D (x, y, z) positions, shown in Effect Controls and as draggable effect control points in the Composition/Layer viewer.
- **Exact semantics:**
  - `PF_Param_POINT`: value in pixels in the effect's input layer space, origin top-left, y down. Default is specified by the effect as a percentage of layer size (e.g. (50%, 50%) = layer centre) and converted to pixels when the effect is applied.
  - `PF_Param_POINT_3D` (AE CS5.5 / 10.5+): (x, y, z) pixels; default similarly relative to layer size (z in pixels).
  - Coordinate space: layer space for normal layers; comp space for continuously-rasterised layers and collapsed precomps (shape and text layers always comp space) (Adobe help, Effect control points).
  - Origin offset: if an upstream effect expanded the buffer, AE adds the offset so the point still refers to the same layer pixel (SDK "Point Parameter Origin").
  - Animation: both dimensions animate together. UNVERIFIED: effect points are spatial properties like Position (spatial bezier/auto-bezier handles, motion path in the Layer panel). Separate Dimensions is not available on effect points.
  - Downsampling: pixel values are multiplied by the downsample factor before rendering (AE-FX-018).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-ui-motion)
- **Notes:** Lottie `ty: 3` (Point); bodymovin exports 3D points as `ty: 3` with a 3-element value (its type fallback branch). Expressions: `effect("X")("Point")` returns [x, y] in layer space; use `toComp()` to convert.

### AE-FX-012 · Layer parameter (图层参数)

- **What it does:** A popup that selects another layer of the same comp as an input image (control layer / layer map), with a second menu choosing which stage of that layer to use.
- **Exact semantics:**
  - `PF_Param_LAYER`: value = layer reference (stored by layer ID; menu shows "None" (无) then layers as "N. Name"). Choosing the layer itself is allowed for most effects (the effect then receives the layer's own source stage, not its own output).
  - Second menu (added in AE CC 2017.1 / 14.2, April 2017): `Source` (源) / `Masks` (蒙版) / `Effects & Masks` (效果和蒙版)（译名待核）. Source = the layer's source frame, masks and effects ignored; Masks = after its masks; Effects & Masks = after its masks and effects. Default = Source. Transforms are never included (precompose to include them). The Adobe help sentence "The compound effect ignores effects, masks, and transformations of a control layer" describes the default (Source) and pre-14.2 behaviour. Source: https://blog.adobe.com/en/publish/2017/05/02/after-effects-cc-april-2017-in-depth-new-features
  - Time: the referenced layer is sampled at the same comp time as the current frame, mapped through that layer's own time stretch/remap (UNVERIFIED: AE checks out the layer at the comp time, converted to the layer's own time).
  - Size: the control layer arrives at its own pixel size; effects with "Stretch Map To Fit"-type options scale it to the destination size, otherwise it is centred on the destination layer.
  - The control layer's video switch does not matter (hidden layers are usable; Adobe help recommends turning it off). Track-matte layers can be referenced. Layer parameters take Hold keyframes only (no interpolation between layers). UNVERIFIED: whether layer parameters (including Layer Control) accept expressions; BAE should allow an expression returning a layer.
  - Scripting value type `PropertyValueType.LAYER_INDEX`: `property.value` = layer index (0 = None). UNVERIFIED: scripting cannot read or set the Source / Masks / Effects & Masks menu.
- **Priority:** P0 — needed by Set Matte, Displacement Map, Layer Control and many compound effects.
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 10` (Effect Value Layer), value = layer `ind`. Implementation hint: the render graph adds a dependency edge to the referenced layer's stage node (source / after masks / after effects), which must not create cycles; a cycle (A references B whose effects reference A) renders the referenced layer as empty. Cross-ref ARCHITECTURE §15 (layer checkout at other times).

### AE-FX-013 · Group, button, no-value, arbitrary-data and path parameters (参数组、按钮、无值、任意数据和路径参数)

- **What it does:** Structural and special parameter types.
- **Exact semantics:**
  | Type | SDK | Value / behaviour | Animatable |
  |---|---|---|---|
  | Group (topic) | `PF_Param_GROUP_START` / `_END` | Twirl-down group in Effect Controls and Timeline; can nest; may start collapsed (`PF_ParamFlag_START_COLLAPSED`). | n/a |
  | Button | `PF_Param_BUTTON` (CS5.5+) | Push button; triggers a callback; no value. | no |
  | No value / label | Scripting `PropertyValueType.NO_VALUE`; SDK param with `PF_PUI_ECW_SEPARATOR` / custom UI | Placeholder rows (labels, group start/end markers). | no |
  | Arbitrary data | `PF_Param_ARBITRARY_DATA` | Opaque blob with an ID and default (scripting `PropertyValueType.CUSTOM_VALUE`, value not readable); interpolation provided by the effect (e.g. Curves' curve data, Levels' histogram, Hue/Saturation's Channel Range). Effect supplies copy/compare/interpolate/flatten callbacks. | if the effect implements interpolation (UNVERIFIED for Curves) |
  | Path | `PF_Param_PATH` | Reference to a mask on the same layer (scripting `PropertyValueType.MASK_INDEX`; index of mask, "None" = 0). The effect reads the mask's path at render time. | no (the referenced mask animates) |
  - Parameter flags BAE must model (SDK `PF_ParamFlag_*`, `PF_PUI_*`): CANNOT_TIME_VARY (no keyframes), CANNOT_INTERP (hold only), SUPERVISE (UI callbacks), START_COLLAPSED, COLLAPSE_TWIRLY, INVISIBLE, DISABLED, NO_ECW_UI (hidden in Effect Controls), STD_CONTROL_ONLY.
- **Priority:** P0 for groups and no-value rows; P1 for path; P2 for button and arbitrary data (internal use by built-in effects such as Curves).
- **Owner:** bae-model (+ bae-plugin, bae-ui-motion)
- **Notes:** Lottie docs: Ignored value `ty: 6`, Effect No Value has no `ty`. Bodymovin actually writes no-value parameters (labels, group start/end markers) as `ty: 6` with `v: 0`, arbitrary data as an empty object (`ty: 9` reserved), mask references as `ty: 11`, layer references as `ty: 10`. Cross-ref ARCHITECTURE §15 (BAE parameter types must be a superset of AE's).

### AE-FX-014 · Animating effect parameters (效果参数动画)

- **What it does:** Effect parameters are ordinary animatable properties: stopwatch, keyframes, expressions.
- **Exact semantics:**
  - Every parameter with a value and without CANNOT_TIME_VARY shows a stopwatch in Effect Controls and Timeline; keyframing, interpolation, Graph Editor, expressions and pick-whip work exactly as for layer properties (transform-keyframes-expressions.md).
  - Interpolation by type: float/angle/colour/point/3D point → linear, bezier, auto-bezier, continuous, hold (point also spatial); checkbox, popup, layer, path → hold only; arbitrary data → effect-defined.
  - Effect parameters are evaluated at the layer's local time after time stretch/remap (the effect sees the same time as the layer's other properties).
  - Keyboard: `E` reveals effects in the Timeline; `EE` reveals expressions; `U` animated properties.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Expressions reference effect parameters as `effect("Name")("Param name")`, `effect("Name")(index)` or by match name `effect("Name")("ADBE Slider Control-0001")` (see transform-keyframes-expressions.md, effect object).

### AE-FX-015 · Effect bit-depth support and conversion (效果位深度支持和转换)

- **What it does:** Each effect declares the maximum bit depth it processes (8, 16 or 32 bpc); AE converts pixels when the project depth is higher.
- **Exact semantics:**
  - Flags: 8 bpc only (no flag); 16 bpc = `PF_OutFlag_DEEP_COLOR_AWARE`; 32 bpc float = `PF_OutFlag2_FLOAT_COLOR_AWARE` (+ SmartFX). The Effects & Presets panel badges each effect with 8/16/32 and can filter to effects supporting the project depth.
  - Effect depth < project depth: the input is converted down (32 → 16 or 8: values clamped to [0, 1] and quantised; 16 → 8: quantised), the effect runs, its output is converted up (exact). Overbright values above 1.0 are lost at that point. Effect Controls shows a warning triangle next to the effect name ("this effect only supports 8 bpc").
  - Effect depth ≥ project depth: runs at project depth.
  - Per-effect bpc values for built-in effects are given in each effect item (from the docsforadobe match-name table).
- **Priority:** P0 — needed to match AE numbers in 8/16/32 projects.
- **Owner:** bae-render (+ bae-effects, bae-color)
- **Notes:** BAE decision: all BAE effects run in float on the GPU. To match AE in 16/32 bpc projects, a node whose AE counterpart is 8 or 16 bpc applies the same clamp+quantise on its input and output (configurable "AE fidelity" switch; off = keep float). In 8 bpc projects every node output is clamped and quantised to 1/255 anyway (compositing-3d-color.md, Emulating 8/16 bpc clamping).

### AE-FX-016 · GPU acceleration flag (GPU 加速标志)

- **What it does:** Marks effects that AE can render on the GPU (Mercury GPU Acceleration).
- **Exact semantics:**
  - Flag `PF_OutFlag2_SUPPORTS_GPU_RENDER_F32` (SDK, AE 16.0+ for third-party). The effect is badged GPU in Effects & Presets; "Show GPU Capable Effects Only" filters by it. GPU rendering runs at 32-bit float regardless of project depth (output then converted to project depth).
  - When Project Settings > Video Rendering and Effects = Mercury Software Only, all effects render on the CPU; results should match within rounding.
  - The AE version that first GPU-accelerated each built-in effect is listed per effect below (e.g. Invert 14.1, Levels 14.2, Lumetri 13.8).
- **Priority:** P2 — informational for AE fidelity; BAE effects are GPU-first anyway.
- **Owner:** bae-effects (+ bae-plugin)
- **Notes:** BAE decision: every built-in BAE effect is implemented in WGSL and runs on the GPU; the AE GPU column is only used to decide which AE effects have a "GPU-path" numeric behaviour (float, unclamped) worth matching. Cross-ref ARCHITECTURE §14 (plug-ins declare GPU support).

### AE-FX-017 · Output bounds expansion and Repeat Edge Pixels (输出边界扩展和重复边缘像素)

- **What it does:** Some effects draw outside the input layer's rectangle (blurs, glows, drop shadow, distortions) and enlarge the layer buffer; others are clipped to the layer bounds.
- **Exact semantics:**
  - Effects with `PF_OutFlag_I_EXPAND_BUFFER` (or SmartFX result rect larger than the request) may return a larger output rect; the layer's bounds grow accordingly for the rest of the effect stack, layer styles and transform. AE limits a layer buffer to 30,000 × 30,000 pixels (UNVERIFIED limit applies to effect buffers too).
  - Effects without the flag output exactly the input rect; content outside is clipped (Adobe help: use Grow Bounds (增长边界) to extend legacy effects).
  - Effects that sample outside the input (blurs, distortions) treat outside pixels as transparent black, unless a "Repeat Edge Pixels" (重复边缘像素) checkbox is on: then outside samples clamp to the nearest edge pixel (clamp-to-edge addressing) and the output is NOT expanded (the result stays the layer size with hard edges).
  ```
  sample(img, x, y):
      if repeat_edge: return img[clamp(x, 0, w-1), clamp(y, 0, h-1)]
      else: return (x,y) inside ? img[x,y] : transparent_black
  out_rect = repeat_edge ? in_rect : in_rect.expand(effect_radius)
  ```
  - Each effect item states "Expands bounds: yes/no".
  - Adjustment layers and collapsed/continuously rasterised layers render effects at comp size, so expansion is limited to the comp rect (see compositing-3d-color.md, Adjustment layers; Collapse transformations).
- **Priority:** P0 — blur/glow fidelity depends on it.
- **Owner:** bae-render (+ bae-effects)
- **Notes:** Implementation hint: every BAE effect node declares `fn output_rect(input_rect, params) -> Rect` and `fn input_roi(output_roi, params) -> Rect` (ARCHITECTURE §14 ROI negotiation); the render graph allocates textures from those rects. Pixel-independent colour effects (this part's Color Correction and most Channel effects) never expand bounds and need ROI = output ROI.

### AE-FX-018 · Downsample-factor scaling of pixel parameters (像素参数的降采样缩放)

- **What it does:** When previewing at reduced resolution (Half, Third, Quarter, Custom) or with proxies, pixel-unit parameters are scaled so the result looks like a scaled-down full-resolution render.
- **Exact semantics:**
  - Render request carries `downsample_x`, `downsample_y` (rational, e.g. 1/2). Effects multiply every pixel-distance parameter (blur radius, offsets, point positions, stroke widths) by the factor in the corresponding axis: "a blur of 4 pixels should be interpreted as a blur of 2 pixels if the downsample factor is 1/2" (SDK PF_InData).
  - Point parameters are delivered already scaled by AE (UNVERIFIED: AE scales point param values by downsample before the effect sees them; effects scale scalar pixel params themselves).
  - Non-square pixel aspect ratio: effects receive `pixel_aspect_ratio` and must scale horizontal distances so circles stay round in square-pixel display (SDK Pixel aspect ratio).
  - Colour-only effects (this part) are unaffected, except where they compute statistics over the image (Auto Levels/Contrast/Color, Equalize, Color Stabilizer), which then see a lower-resolution image and may give slightly different results.
- **Priority:** P0
- **Owner:** bae-effects (+ bae-render)
- **Notes:** See compositing-3d-color.md, Preview resolution. BAE decision: parameters tagged `unit = pixels` in the BAE parameter schema are scaled by the host before the shader runs, so individual WGSL shaders never see downsample factors.

### AE-FX-019 · Effects on adjustment layers (调整图层上的效果)

- **What it does:** An effect on an adjustment layer processes the composite of all layers below it within the adjustment layer's area and time.
- **Exact semantics:** Specified in compositing-3d-color.md, Adjustment layers (input = composite below, cropped to the adjustment layer's bounds and masks; result blended with Opacity and blend mode; adjustment layer's own source pixels are ignored). Effect-level rules that apply here: the effect's input layer `param[0]` is that composite, at comp size; point parameters are in the adjustment layer's layer space; Compositing Options (AE-FX-005/006) apply before the adjustment layer's own opacity.
- **Priority:** P0
- **Owner:** bae-render (+ bae-effects)
- **Notes:** —

### AE-FX-020 · Layer and time inputs of effects (效果的图层和时间输入)

- **What it does:** Defines which images and times an effect may read besides its own input at the current time.
- **Exact semantics:**
  - Layer parameters (AE-FX-012) read another layer's Source / Masks / Effects & Masks stage at the current comp time (in that layer's time).
  - Effects with `PF_OutFlag_WIDE_TIME_INPUT` (e.g. Echo, Posterize Time, Time Displacement, Color Stabilizer, Pixel Motion Blur) check out their input or parameters at other times; AE then renders the input layer (after masks and effects above this effect) at those times. The input is the same stage of the same layer at time t' (UNVERIFIED: AE renders the layer's own effects above at t' too).
  - Effects with `PF_OutFlag_I_USE_SHUTTER_ANGLE` (e.g. Transform effect, CC Force Motion Blur) read the comp shutter angle/phase.
  - Effects that use the comp camera and lights (`PF_OutFlag2_I_USE_3D_CAMERA`, `_I_USE_3D_LIGHTS`; e.g. Card Dance, Card Wipe, Shatter, CC Sphere) read the active camera/lights at the current time; the layer stays 2D (no intersections or shadows with 3D layers).
- **Priority:** P1
- **Owner:** bae-render (+ bae-anim, bae-effects)
- **Notes:** Implementation hint: an effect node declares its extra dependencies (`(layer, stage, time)` tuples) during graph construction, so the render scheduler can render and cache them first; caching must key on (layer, stage, time). Cross-ref ARCHITECTURE §15 ("效果可以声明对其他图层、其他时间的依赖").

### AE-FX-021 · Reset effect (重置效果)

- **What it does:** Resets all parameters of one effect instance to their defaults.
- **Exact semantics:**
  - Effect Controls: "Reset" link at the top of the effect; in the latest AE (modernised Effect Controls, UNVERIFIED version) a Reset Effect icon. Removes all keyframes of the effect's parameters and sets static defaults. UNVERIFIED: expressions stay attached.
  - UNVERIFIED: Compositing Options are reset too (Effect Opacity 100%, mask references removed).
  - Point defaults are recomputed from the current layer size (e.g. layer centre).
  - Right-click a single parameter > Reset resets only that parameter.
  - One undo step.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Expression Controls: Reset sets the Dropdown Menu Control value to 1 but keeps the edited menu items (UNVERIFIED).

### AE-FX-022 · Effect > Remove All (效果 > 全部移除)

- **What it does:** Deletes every effect from the selected layers.
- **Exact semantics:** Menu Effect > Remove All, shortcut Ctrl+Shift+E (Win) / Cmd+Shift+E (Mac). Applies to all selected layers (selected in Timeline or Composition panel); deletes all effect instances including Expression Controls, with their keyframes and expressions. One undo step named "Remove All Effects". Expressions on other properties that referenced removed effects then error.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** —

### AE-FX-023 · Animation Presets (.ffx) (动画预设)

- **What it does:** Saves and applies a reusable set of effects, properties, keyframes and expressions as a `.ffx` file.
- **Exact semantics:**
  - Save: select effects and/or properties (any layer properties, including transform, text animators, masks), Animation > Save Animation Preset or Effects & Presets panel menu; saved to the user Presets folder by default.
  - Apply: drag/double-click from Effects & Presets, Animation > Apply Animation Preset..., Animation > Recent Animation Presets, or Ctrl+Alt+Shift+F / Cmd+Opt+Shift+F (apply most recent preset). Applied at the current time (keyframe times are relative to the first keyframe). Effects in the preset are added to the layer (with the paste-replace rule of AE-FX-002 for single instances, UNVERIFIED); non-effect properties overwrite existing values.
  - Effect Controls per-effect "Animation Presets" menu (shown when "Show Animation Presets" is enabled) applies a preset's settings to the existing effect instance.
  - Presets folders scanned at startup and when Effects & Presets is shown; folders named in parentheses are ignored. "Refresh List" rescans.
  - A preset cannot be disabled/removed as a unit after application.
  - `.ffx` is a binary RIFX container (big-endian RIFF, as in .aep) (UNVERIFIED structure).
- **Priority:** P1 — commonly used, but BAE can first ship its own preset format.
- **Owner:** bae-model (+ bae-motion, bae-ui-motion)
- **Notes:** BAE decision: BAE presets are a stored subtree of the project data model (JSON/USD-style fragment); importing AE `.ffx` is P2 and depends on the .aep reader. Bundled AE presets are Adobe content and must not be copied.

### AE-FX-024 · Legacy and obsolete effect versions (旧版和过时效果)

- **What it does:** When AE replaces an effect with a new implementation, the old one keeps its matchName and stays loadable (in the Obsolete category or hidden) so old projects render identically.
- **Exact semantics:**
  - New versions get a new matchName (usually with a numeric suffix): e.g. `ADBE Easy Levels` → `ADBE Easy Levels2` (Levels), `ADBE Pro Levels` → `ADBE Pro Levels2`, `ADBE Brightness & Contrast` → `ADBE Brightness & Contrast 2`, `ADBE Color Balance` → `ADBE Color Balance 2`, `ADBE Exposure` → `ADBE Exposure2`, `ADBE Photo Filter` → `ADBE PhotoFilterPS`, `ADBE Set Matte2` → `ADBE Set Matte3`, `ADBE Gaussian Blur` → `ADBE Gaussian Blur 2` (Gaussian Blur (Legacy) stays in Obsolete), `ADBE Fast Blur` = Fast Blur (Legacy).
  - Obsolete category effects (still applicable from the Effect > Obsolete menu): Basic 3D, Basic Text, Color Key, Fast Blur (Legacy), Gaussian Blur (Legacy), Lightning, Luma Key, Path Text, Reduce Interlace Flicker, Spill Suppressor (docsforadobe table, "Obsolete").
  - Hidden obsolete effects (load only from old projects; docsforadobe "_Obsolete"): Alpha Levels, Apply Color LUT (v1; current `ADBE Apply Color LUT2`), Box Blur, Cineon Converter (v1; current `ADBE Cineon Converter2`), old Color Balance / Exposure / Levels / Photo Filter / Set Matte / Brightness & Contrast, Three-Way Color Corrector, Threshold, Unsharp Mask, Photoshop layer-style effects (`ADBE PSL *`), etc. (docsforadobe "_Obsolete").
  - An effect can also keep its matchName and change behaviour behind a parameter (e.g. Brightness & Contrast "Use Legacy (supports HDR)"; see each effect).
- **Priority:** P1 for the legacy versions of effects specified in this file (old projects and templates use them); P2 for the rest.
- **Owner:** bae-effects (+ bae-model)
- **Notes:** BAE decision: map old matchNames to the same BAE implementation with a `legacy` mode flag when the maths differs, otherwise alias them. Each effect item below names its legacy matchName where one exists.

### AE-FX-025 · Effect categories and the Effect menu (效果类别和效果菜单)

- **What it does:** Built-in effects are grouped into categories that form the Effect menu submenus and the Effects & Presets tree.
- **Exact semantics:** Categories (AE 2025, English / zh-CN): 3D Channel (3D 通道), Audio (音频), Blur & Sharpen (模糊和锐化), Channel (通道), CINEMA 4D, Color Correction (颜色校正), Distort (扭曲), Expression Controls (表达式控制), Generate (生成), Immersive Video (沉浸式视频), Keying (抠像), Matte (遮罩), Noise & Grain (杂色和颗粒), Obsolete (过时), Perspective (透视), Simulation (模拟), Stylize (风格化), Text (文本), Time (时间), Transition (过渡), Utility (实用工具). Within a category effects are sorted alphabetically by localised display name. Third-party effects add categories by their PiPL category string. Scripting `app.effects[i].category` gives the category string ("" for hidden effects).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-plugin)
- **Notes:** UNVERIFIED zh-CN strings for Immersive Video and Utility (译名待核). The Effects & Presets panel itself is specified in ui-preview-render-io.md.

### AE-FX-026 · Missing effects (缺失的效果)

- **What it does:** When a project references an effect whose plug-in is not installed, AE keeps the instance and its parameter data and shows it as missing.
- **Exact semantics:** On open, AE reports missing effects (dialog listing them; File > Dependencies > Find Missing Effects selects layers using them). The effect stays in the stack with its name in red/italic UNVERIFIED and a "missing" message in Effect Controls; it renders as pass-through (input unchanged). Parameter values and keyframes are preserved and saved back.
- **Priority:** P1
- **Owner:** bae-model (+ bae-plugin, bae-render)
- **Notes:** BAE decision (ARCHITECTURE §15): resolve an unknown matchName in order BAE native implementation → installed BAE plug-in → AE-plug-in compatibility layer → "missing effect" placeholder that keeps all parameter data and round-trips to AE unchanged.

## Expression Controls

### AE-FX-027 · Expression Controls common behaviour (表达式控制效果的共同行为)

- **What it does:** The Expression Controls category holds effects that render nothing and only expose one animatable parameter each, so expressions on other properties can read a keyframable, user-editable value (rig controls).
- **Exact semantics:**
  - Category `Expression Controls` (表达式控制). All eight are 32 bpc, CPU, no GPU column (they do no pixel work): 3D Point Control `ADBE Point3D Control`, Angle Control `ADBE Angle Control`, Checkbox Control `ADBE Checkbox Control`, Color Control `ADBE Color Control`, Dropdown Menu Control `ADBE Dropdown Control`, Layer Control `ADBE Layer Control`, Point Control `ADBE Point Control`, Slider Control `ADBE Slider Control`.
  - Render: output = input, bit-exact, no bounds change. They still occupy a slot in the effect stack (AE-FX-001) and carry the standard Compositing Options group (`ADBE Effect Built In Params`), which has no visible effect because output = input.
  - Each has exactly one parameter with match name `"<effect matchName>-0001"`; typical expression access: `effect("Slider Control")("Slider")`, `effect("Slider Control")(1)` or `effect("Slider Control")("ADBE Slider Control-0001")`.
  - Users normally rename the instance (Return/Enter on the selected effect); expressions referencing the old name are updated automatically by AE's rename propagation (see transform-keyframes-expressions.md, expression evaluation and name tracking).
  - Right-click > Edit Value... changes the slider range shown in Effect Controls for numeric controls (stored per instance); it does not change the valid range (AE-FX-008).
  - Expression controls are the usual target of Essential Graphics "Add Property" (see ui-preview-render-io.md, Essential Graphics).
  - Lottie/bodymovin exports them as an effect with `ty: 5` (Custom Effect), `mn` = effect matchName, `nm` = instance name, `ef` = one effect value. Lottie docs value types: 0 slider, 1 angle, 2 color, 3 point, 4 checkbox, 7 dropdown, 10 layer. What bodymovin actually writes (type chosen from the AE property value type): Slider → 0, Angle → 0, Color → 2, Point → 3, 3D Point → 3 (3-element value), Checkbox → 7, Dropdown Menu → 7, Layer → 10. Importers must accept both numbering schemes (bodymovin-extension `effectsHelper.jsx`).
- **Priority:** P0 — expression rigs and templates depend on them.
- **Owner:** bae-model (+ bae-anim, bae-ui-motion)
- **Notes:** Implementation hint: the render graph should skip these nodes entirely (identity), so they cost nothing at render time. The animation preset "Animated Shape Control" in AE's bundled presets is a pseudo-effect (custom expression-control effect defined in an .ffx); pseudo-effects are a P2 topic (AE-FX-023).

### AE-FX-028 · Slider Control (滑块控制)

- **What it does:** Exposes one floating-point number.
- **Exact semantics:** matchName `ADBE Slider Control`; 32 bpc; GPU —; expands bounds: no; Lottie effect `ty: 5`, value `ty: 0`.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Slider (滑块) | `ADBE Slider Control-0001` | float slider | 0 | slider 0…100; valid −1,000,000…1,000,000 (the 1,000,000 cap is confirmed by Adobe community reports; the symmetric −1,000,000 floor is UNVERIFIED) | none | 2 decimals displayed; Edit Value can widen the slider range up to the valid range. |
  ```
  render(in) = in
  value(t)   = evaluated property value (keyframes / expression), clamped to valid range
  ```
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Expressions read a Number. AE scripting cannot read or set the slider range of an instance (community-reported; no API). Source for the 1,000,000 cap: https://community.adobe.com/t5/after-effects-discussions/slider-only-goes-up-to-1-000-000/m-p/13465738 BAE decision: store the per-instance slider range in the model so Essential Graphics and the UI can show it.

### AE-FX-029 · Angle Control (角度控制)

- **What it does:** Exposes one angle in degrees with the rotation dial UI.
- **Exact semantics:** matchName `ADBE Angle Control`; 32 bpc; GPU —; expands bounds: no; Lottie value `ty: 1` per Lottie docs (bodymovin writes `ty: 0`, AE-FX-027).
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Angle (角度) | `ADBE Angle Control-0001` | angle | 0 | unbounded revolutions (displayed `Nx+D°`) | degrees | Value is a plain Number in degrees in expressions (e.g. 450 = 1x+90°). |
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Interpolates numerically (450 → 0 turns back 450°, no shortest-path wrapping).

### AE-FX-030 · Checkbox Control (复选框控制)

- **What it does:** Exposes one on/off switch.
- **Exact semantics:** matchName `ADBE Checkbox Control`; 32 bpc; GPU —; expands bounds: no; Lottie value `ty: 4` per Lottie docs (bodymovin writes `ty: 7`, AE-FX-027).
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Checkbox (复选框) | `ADBE Checkbox Control-0001` | checkbox | off (0) | 0 / 1 | — | Keyframes are Hold only. Expressions read Number 0 or 1 (truthy in `if`). |
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** An expression on the checkbox itself may return any number; non-zero is shown as checked (AE-FX-009, UNVERIFIED threshold).

### AE-FX-031 · Color Control (颜色控制)

- **What it does:** Exposes one RGB colour.
- **Exact semantics:** matchName `ADBE Color Control`; 32 bpc; GPU —; expands bounds: no; Lottie value `ty: 2`.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Color (颜色) | `ADBE Color Control-0001` | color | [1, 0, 0, 1] (pure red) (UNVERIFIED default) | 0…1 per channel (may exceed 1 via expression in 32 bpc) | — | Expressions read `[r,g,b,a]`, a = 1. |
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-color)
- **Notes:** Interpolation per AE-FX-010 (linear per channel in stored RGB). Common use: `effect("Color Control")("Color")` on a Fill effect colour or shape fill colour.

### AE-FX-032 · Point Control (点控制)

- **What it does:** Exposes one 2D point with an on-screen control point in the Composition viewer when the effect is selected.
- **Exact semantics:** matchName `ADBE Point Control`; 32 bpc; GPU —; expands bounds: no; Lottie value `ty: 3`.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Point (点) | `ADBE Point Control-0001` | point (2D) | UNVERIFIED: [0, 0] in layer coordinates (some builds place it at the layer centre) | unbounded | pixels, layer space | Spatial property: has motion path with spatial bezier handles (transform-keyframes-expressions.md, spatial interpolation). |
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-ui-motion)
- **Notes:** The value is in the layer's own coordinate space (like all effect point parameters, AE-FX-011); rig expressions often convert it with `toComp()`. Downsampling does not change the stored value.

### AE-FX-033 · 3D Point Control (3D 点控制)

- **What it does:** Exposes one 3D point (x, y, z).
- **Exact semantics:** matchName `ADBE Point3D Control`; 32 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | 3D Point (3D 点)（译名待核） | `ADBE Point3D Control-0001` | 3D point | UNVERIFIED: [layer width/2, layer height/2, 0] | unbounded | pixels | Spatial 3D property; viewer control point drawn only in 2D projection of the layer (UNVERIFIED). |
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-ui-motion)
- **Notes:** UNVERIFIED version of introduction (the SDK's `PF_Param_POINT_3D` dates from CS5.5 / AE 10.5). The Lottie docs have no 3D effect-value type; bodymovin exports this control as `ty: 3` with a 3-element value, and BAE does the same.

### AE-FX-034 · Layer Control (图层控制)

- **What it does:** Exposes one layer reference, which expressions use as a layer object.
- **Exact semantics:** matchName `ADBE Layer Control`; 32 bpc; GPU —; expands bounds: no; Lottie value `ty: 10` (value = referenced layer index).
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Layer (图层) | `ADBE Layer Control-0001` | layer | None (UNVERIFIED) | None or any layer of the same comp (including itself) | — | Not keyframable to arbitrary values: Hold keys only (layer params cannot interpolate). |
  - `effect("Layer Control")("Layer")` returns a Layer object, so `.transform.position`, `.effect(...)`, `.sourceRectAtTime()` etc. work on it (see transform-keyframes-expressions.md, Layer object). When the parameter is None the expression gets an error / null (UNVERIFIED exact behaviour).
  - The reference is stored by layer identity (survives reordering and renaming), not by index.
  - No pixels are read, so the "Source / Masks / Effects & Masks" choice of AE-FX-012 has no effect here (UNVERIFIED whether AE shows that menu on Layer Control).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** UNVERIFIED version of introduction (believed to be one of the original expression controls). Deleting the referenced layer resets the parameter to None. Scripting value = layer index, 0 = None (AE-FX-012).

### AE-FX-035 · Dropdown Menu Control (下拉菜单控件)

- **What it does:** Exposes a popup menu whose item strings the user defines; the value is the 1-based index of the chosen item.
- **Exact semantics:** matchName `ADBE Dropdown Control`; 32 bpc; GPU —; expands bounds: no; Lottie value `ty: 7`. Added in AE 17.0.1 (2020).
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Menu (菜单) | `ADBE Dropdown Control-0001` | popup (user items) | 1 | 1…n (n = number of items, separators count) | — | Default items "Item 1", "Item 2", "Item 3" (UNVERIFIED exact strings). Hold keyframes only. |
  - **Editing items:** an "Edit..." button beside the menu in Effect Controls opens a dialog where items are added (+), removed (−), renamed and reordered (UNVERIFIED reorder). Scripting: `menuProp.isDropdownEffect` (17.0.1) is true for this property only; `menuProp.setPropertyParameters(["A","B","(-","C"])` (17.0.1) replaces all items and returns the updated Menu property (the old object may be invalidated); `menuProp.propertyParameters` (26.0) returns the item strings.
  - Item string rules (setPropertyParameters): strings only; no empty strings; no duplicates; no `\` character; `"(-"` creates a separator line that still consumes an index.
  - Changing the item list keeps the stored index (an index above the new count is clamped to n, UNVERIFIED).
  - Expressions read the index as a Number (1-based); item text is not available to expressions (UNVERIFIED for AE 26 — scripting got `propertyParameters` in 26.0, expressions have no equivalent).
- **Priority:** P0 — required by modern templates/MOGRTs.
- **Owner:** bae-model (+ bae-anim, bae-ui-motion)
- **Notes:** Menu item strings are stored in the project in the system code page (scripting docs warn that non-ASCII item strings set from a script running under a different system language become illegible). BAE decision: store items as UTF-8; import AE projects by decoding with the project's code page.

## Color Correction (core effects)

Shared notes for this section: all colour-correction effects operate on straight (unpremultiplied) RGB and leave alpha unchanged unless the effect has an Alpha channel option; none expands bounds; all accept Compositing Options (AE-FX-005). In tables `#` is the 4-digit suffix of the parameter match name (= disk ID); for effects with parameter groups, a group start and its (invisible) group end each consume one ID, so `#` can be larger than the visible property index (verified for Exposure and Levels (Individual Controls) against Lottie/Skottie index tables). Parameter defaults marked "(AE 26 dump)" come from a third-party ExtendScript dump of AE 26.0x67 (`effect.property(i).value` right after apply) cached in the research folder.

### AE-FX-036 · Tint (色调)

- **What it does:** Maps each pixel's luminance onto a two-colour ramp from Map Black To to Map White To, mixed with the original by Amount to Tint.
- **Exact semantics:** matchName `ADBE Tint`; category Color Correction; 32 bpc; GPU since AE 14.1 (2017); expands bounds: no; Lottie effect `ty: 20`.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Map Black To (将黑色映射到) | `ADBE Tint-0001` | color | black [0,0,0] (AE 26 dump) | — | — | |
  | 2 | Map White To (将白色映射到) | `ADBE Tint-0002` | color | white [1,1,1] (AE 26 dump) | — | — | |
  | 3 | Amount to Tint (着色数量) | `ADBE Tint-0003` | float slider | 100 (AE 26 dump) | slider 0…100; valid 0…100 | % | |
  | 4 | Swap Colors (交换颜色) | `ADBE Tint-0004` | button (no value) | — | — | — | Dump reports an unnamed group/no-value item at 0004; UI shows the Swap Colors button, which exchanges params 1 and 2 as one undoable edit (sets static values; with keyframes it swaps the current-time values, UNVERIFIED). |
  ```
  L      = luma(R, G, B)            // UNVERIFIED weights: Skottie uses Rec.709 (0.2126, 0.7152, 0.0722);
                                    // lottie-web uses (1/3, 1/3, 1/3)
  T      = black + (white - black) * L        // per channel; L not clamped in 32 bpc
  out.rgb = lerp(in.rgb, T, amount / 100)
  out.a   = in.a
  ```
- **Priority:** P0 — one of the most-used colour effects; supported by Lottie.
- **Owner:** bae-effects
- **Notes:** In 32 bpc, L > 1 extrapolates beyond Map White To (no clamp, UNVERIFIED). Implementation hint: the whole effect is a 3×4 colour matrix (Skottie folds luminance and ramp into one matrix), so it can be fused with neighbouring matrix effects. Lottie `ef` order: [Map Black To (ty 2), Map White To (ty 2), Amount (ty 0)].

### AE-FX-037 · Tritone (三色调)

- **What it does:** Maps luminance onto a three-colour ramp Shadows → Midtones → Highlights, then blends with the original.
- **Exact semantics:** matchName `ADBE Tritone`; 32 bpc; GPU —; expands bounds: no; Lottie effect `ty: 23`.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Highlights (高光) | `ADBE Tritone-0001` | color | [1,1,1] | — | — | |
  | 2 | Midtones (中间调) | `ADBE Tritone-0002` | color | [127,100,70]/255 = [0.49804, 0.39216, 0.27451] (AE 26 dump) | — | — | |
  | 3 | Shadows (阴影) | `ADBE Tritone-0003` | color | [0,0,0] | — | — | |
  | 4 | Blend With Original (与原始图像混合) | `ADBE Tritone-0004` | float slider | 0 | 0…100 | % | 100 = no visible effect. |
  ```
  L = luma(R, G, B)                       // same weights question as Tint (UNVERIFIED)
  if L <= 0.5: T = lerp(shadows,  midtones,   L / 0.5)
  else:        T = lerp(midtones, highlights, (L - 0.5) / 0.5)
  out.rgb = lerp(T, in.rgb, blend / 100)
  out.a   = in.a
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Note the parameter order (Highlights first) differs from the ramp order. Skottie builds a 256-entry 3-colour gradient table (8-bit precision); BAE must evaluate analytically for 16/32 bpc.

### AE-FX-038 · Levels (色阶)

- **What it does:** Remaps input range [Input Black, Input White] to [Output Black, Output White] with a gamma curve, for RGB together or for a single channel (R, G, B or Alpha).
- **Exact semantics:** matchName `ADBE Easy Levels2`; 32 bpc; GPU since AE 14.2 (2017); expands bounds: no (Lottie: no own effect type; Bodymovin converts it to Pro Levels `ty: 24` by switching the Channel menu and reading params 3–7 per channel; Skottie also renders `mn: "ADBE Easy Levels2"` directly).
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Channel (通道) | `ADBE Easy Levels2-0001` | popup | 1 = RGB | RGB / Red / Green / Blue / Alpha | — | Selects which channel's five values the controls show and edit. |
  | 2 | Histogram (直方图) | `ADBE Easy Levels2-0002` | arbitrary data | — | — | — | UI only (histogram of the effect input at the current time). Each of RGB/Red/Green/Blue/Alpha keeps its own five values; params 3–7 show and edit the set of the channel selected in Channel (Bodymovin reads all five sets this way). Where the non-selected sets are stored is internal (UNVERIFIED: in this blob). |
  | 3 | Input Black (输入黑色) | `ADBE Easy Levels2-0003` | float slider | 0.0 | displayed 0…255 (8 bpc), 0…32768 (16 bpc), 0.0…1.0 (32 bpc); valid beyond in 32 bpc (UNVERIFIED limits) | level | Stored normalised (1.0 = white); scripting returns normalised values (AE 26 dump: Input White = 1). |
  | 4 | Input White (输入白色) | `ADBE Easy Levels2-0004` | float slider | 1.0 (255 in 8 bpc display) | as above | level | |
  | 5 | Gamma (灰度系数) | `ADBE Easy Levels2-0005` | float slider | 1.0 | UNVERIFIED: slider 0.1…10 | exponent | Shown as the middle triangle. |
  | 6 | Output Black (输出黑色) | `ADBE Easy Levels2-0006` | float slider | 0.0 | as Input Black | level | |
  | 7 | Output White (输出白色) | `ADBE Easy Levels2-0007` | float slider | 1.0 | as Input Black | level | |
  | 8 | Clip To Output Black (剪切以输出黑色) | `ADBE Easy Levels2-0008` | popup | 3 = Off for 32 bpc Color | On / Off / Off for 32 bpc Color | — | |
  | 9 | Clip To Output White (剪切以输出白色) | `ADBE Easy Levels2-0009` | popup | 3 = Off for 32 bpc Color | On / Off / Off for 32 bpc Color | — | |
  ```
  map(x; ib, iw, g, ob, ow, clipB, clipW):
      t = (x - ib) / (iw - ib)            // iw == ib: treat as a step at ib (Skottie nudges by epsilon)
      // clip active = On, or Off for 32 bpc Color while the project is 8/16 bpc
      if clipB active: t = max(t, 0)       // below Input Black -> exactly Output Black (per help)
      if clipW active: t = min(t, 1)       // above Input White -> exactly Output White (per help)
      t = sign(t) * |t| ^ (1 / g)          // g > 1 brightens; t < 0 only reachable with clip off:
                                           // UNVERIFIED (mirrored here; Skottie clamps t at 0)
      y = ob + (ow - ob) * t
      return y
  per pixel (straight colour):
      R1 = map(R; red values)   G1 = map(G; green values)   B1 = map(B; blue values)   // per-channel first
      R' = map(R1; rgb values)  G' = map(G1; rgb values)    B' = map(B1; rgb values)   // then RGB composite
      A' = map(A; alpha values)
  ```
  - Inversion (Input White < Input Black, or Output White < Output Black) is allowed and inverts the ramp.
  - In 8/16 bpc results are clamped to 0…1 by the buffer regardless of the clip settings.
- **Priority:** P0
- **Owner:** bae-effects
- **Notes:** Legacy matchName `ADBE Easy Levels` (pre-CS6? UNVERIFIED version; hidden obsolete) used 8-bit-style values. The order per-channel → composite follows lottie-web/Skottie's Pro Levels implementations (lottie-web applies the R/G/B/A component-transfer before the composite one); AE's own order is UNVERIFIED. The Histogram's "auto" buttons do not exist in Levels (see Auto Levels, AE-FX-049). Implementation hint: build a 1D LUT per channel (4096 entries or analytic in-shader for 32 bpc). Source: https://helpx.adobe.com/after-effects/using/color-correction-effects.html (clip semantics); https://github.com/bodymovin/bodymovin-extension/blob/master/bundle/jsx/utils/effectsHelper.jsx (`handleEasyLevels`).

### AE-FX-039 · Levels (Individual Controls) (色阶（单独控件）)

- **What it does:** Same maths as Levels, but every channel's five values are separate keyframable properties grouped per channel.
- **Exact semantics:** matchName `ADBE Pro Levels2`; 32 bpc; GPU since AE 14.2; expands bounds: no; Lottie effect `ty: 24` (Pro Levels).
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Channel (通道) | `ADBE Pro Levels2-0001` | popup | 1 = RGB | RGB / Red / Green / Blue / Alpha | — | Only selects the histogram shown. |
  | 2 | Histogram (直方图) | `ADBE Pro Levels2-0002` | arbitrary data | — | — | — | UI only. |
  | 3 | RGB (RGB) | `ADBE Pro Levels2-0003` | group start | — | — | — | |
  | 4–8 | Input Black, Input White, Gamma, Output Black, Output White (输入黑色、输入白色、灰度系数、输出黑色、输出白色) | `-0004`…`-0008` | float slider ×5 | 0, 1, 1, 0, 1 | as Levels | level | Composite RGB values. |
  | 9 | (group end) | `-0009` | — | — | — | — | |
  | 10–16 | Red group: Red Input Black … Red Output White (红色输入黑色 … 红色输出白色) | `-0010` start, `-0011`…`-0015`, `-0016` end | float ×5 | 0, 1, 1, 0, 1 | | level | Red Gamma = `ADBE Pro Levels2-0013`. |
  | 17–23 | Green group | `-0017` start, `-0018`…`-0022`, `-0023` end | float ×5 | 0, 1, 1, 0, 1 | | level | |
  | 24–30 | Blue group | `-0024` start, `-0025`…`-0029`, `-0030` end | float ×5 | 0, 1, 1, 0, 1 | | level | |
  | 31–37 | Alpha group | `-0031` start, `-0032`…`-0036`, `-0037` end | float ×5 | 0, 1, 1, 0, 1 | | level | |
  | 38 | Clip To Output Black (剪切以输出黑色) | `ADBE Pro Levels2-0038` | popup | 3 = Off for 32 bpc Color (UNVERIFIED default) | On / Off / Off for 32 bpc Color | — | |
  | 39 | Clip To Output White (剪切以输出白色) | `ADBE Pro Levels2-0039` | popup | 3 = Off for 32 bpc Color (UNVERIFIED default) | same | — | |
  - Algorithm: identical to Levels (AE-FX-038) using the per-group values.
- **Priority:** P1 — used by Lottie exports and templates that animate single channels.
- **Owner:** bae-effects (+ bae-model)
- **Notes:** Lottie `ef` array indices are disk ID − 1 (Skottie: RGB In Black = index 3, Clip To Output Black = 37). Lottie clip popup value 1 = clip (Skottie treats 2/3 as no clip). Legacy `ADBE Pro Levels` is hidden obsolete. The group-ID scheme here (start + end each consume an ID) is the verified example for AE-FX-007.

### AE-FX-040 · Curves (曲线)

- **What it does:** Remaps tones through user-drawn transfer curves for RGB (composite) and each of Red, Green, Blue and Alpha.
- **Exact semantics:** matchName `ADBE CurvesCustom` (AE CC 2019 / 16.0 rewrite, UNVERIFIED version); 32 bpc; GPU —; expands bounds: no; no Lottie equivalent.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 2 | Channel (通道) | `ADBE CurvesCustom-0002` | popup | 1 = RGB | RGB / Red / Green / Blue / Alpha | — | Shown first in the UI although its disk ID is 2. |
  | 1 | Curves (曲线) | `ADBE CurvesCustom-0001` | arbitrary data | identity line for all five curves | — | — | Holds the five curves; the whole blob is one keyframable property (interpolates point-wise only when point counts match, otherwise Hold, UNVERIFIED). |
  - Curve model: each curve is a list of control points (x, y) in 0…1 with the endpoints (0,0) and (1,1) present by default; Bezier mode draws a smooth curve through the points (monotone cubic spline in x, Photoshop-style natural cubic spline, UNVERIFIED exact spline); Pencil mode stores a 256-entry free-hand map. Buttons: Bezier/Pencil tool, Open (load `.acv` spline or `.amp` arbitrary map), Save, Smooth, Reset (current channel), Auto (automatic curve from a reference database; P2), graph size buttons.
  ```
  per pixel (straight colour):
      R1 = curve_R(R); G1 = curve_G(G); B1 = curve_B(B)          // per-channel curves
      R' = curve_RGB(R1); G' = curve_RGB(G1); B' = curve_RGB(B1)  // composite curve (order UNVERIFIED)
      A' = curve_A(A)
  curve(x): for x in [0,1] evaluate the spline, clamp the result to [0,1];
            for x outside [0,1] (32 bpc) extend linearly from the end segment (UNVERIFIED)
  ```
  - Display scale: graph labels 0–255 (8 bpc) or 0–32768 (16 bpc); 32 bpc shows 0.0–1.0.
- **Priority:** P0 — core grading tool.
- **Owner:** bae-effects (+ bae-ui-motion)
- **Notes:** Older projects use the obsolete matchName `ADBE Curves` (8/16 bpc, curve stored differently, UNVERIFIED). Expressions cannot read the curve data (arbitrary data has no expression value). BAE decision: store curves as explicit point lists per channel plus mode (bezier/pencil); Implementation hint: bake each curve into a 1D LUT texture (≥1024 entries, float) per frame, with analytic extension outside [0,1].

### AE-FX-041 · Hue/Saturation (色相/饱和度)

- **What it does:** Rotates hue and scales saturation and lightness, globally (Master) or for one of six hue ranges, or Colorizes the image with a single hue.
- **Exact semantics:** matchName `ADBE HUE SATURATION`; 32 bpc; GPU since AE 14.1; expands bounds: no; no Lottie effect type (Skottie supports the Master values when exported as a custom effect).
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 2 | Channel Control (通道控制) | `ADBE HUE SATURATION-0002` | popup | 1 = Master | Master (全图) / Reds / Yellows / Greens / Cyans / Blues / Magentas | — | Disk ID 0001 is not exposed to scripting (AE 26 dump). |
  | 3 | Channel Range (通道范围) | `ADBE HUE SATURATION-0003` | arbitrary data | default ranges (below) | — | — | Two colour bars plus range/falloff handles for the selected hue range. |
  | 4 | Master Hue (主色相) | `ADBE HUE SATURATION-0004` | angle | 0 | −180…+180 | degrees | Positive = clockwise on the wheel. Label becomes "Red Hue" etc. for a hue range (UNVERIFIED storage of per-range values). |
  | 5 | Master Saturation (主饱和度) | `ADBE HUE SATURATION-0005` | float slider | 0 | −100…+100 | — | |
  | 6 | Master Lightness (主亮度) | `ADBE HUE SATURATION-0006` | float slider | 0 | −100…+100 | — | |
  | 7 | Colorize (彩色化)（译名待核） | `ADBE HUE SATURATION-0007` | checkbox | off | — | — | zh help text uses 着色. |
  | 8 | Colorize Hue (着色色相) | `ADBE HUE SATURATION-0008` | angle | 0 | 0…360 (UNVERIFIED) | degrees | Only active when Colorize is on. |
  | 9 | Colorize Saturation (着色饱和度) | `ADBE HUE SATURATION-0009` | float slider | 25 | 0…100 | — | |
  | 10 | Colorize Lightness (着色亮度) | `ADBE HUE SATURATION-0010` | float slider | 0 | −100…+100 | — | |
  - Default hue ranges (Photoshop-compatible; 30° core with 30° falloff each side, per AE help): Reds core 345°…15° (falloff 315°…345°, 15°…45°), Yellows 45…75 (15…45, 75…105), Greens 105…135 (75…105, 135…165), Cyans 165…195 (135…165, 195…225), Blues 225…255 (195…225, 255…285), Magentas 285…315 (255…285, 315…345) (exact numbers UNVERIFIED for AE; they are Photoshop's defaults).
  ```
  // Master adjustments (Skottie's reverse-engineered AE model; per-range weighting UNVERIFIED)
  // 1. Lightness, component-wise toward 0 or 1:
  l = lightness / 100
  c = (l < 0) ? c * (1 + l) : c * (1 - l) + l
  // 2. Saturation, scale around the chroma mid-point, clamped so no channel leaves [0,1]:
  s = saturation / 100
  scale = (s < 0) ? 1 + s : min(1 / (1 - s), 126)
  mn = min(R,G,B); mx = max(R,G,B); mid = (mn + mx) / 2; ch = max(mx - mn, 1e-4)
  scale = min(scale, min(mid, 1 - mid) / ch * 2)
  c = mid + (c - mid) * scale
  // 3. Hue: rotate HSL hue by hue/360
  // Skottie composes these so that lightness is applied first, then saturation, then hue.
  // Hue range (Reds..Magentas): weight w(h_in) = 1 inside core, linear ramp across falloff,
  //   0 outside; out = lerp(in, adjusted(in), w)   (UNVERIFIED)
  // Colorize: out = HSL(colorizeHue, colorizeSat/100, L') where L' = HSL lightness of the
  //   input adjusted by Colorize Lightness as in step 1 (UNVERIFIED)
  ```
- **Priority:** P0 — core effect, GPU-accelerated in AE.
- **Owner:** bae-effects (+ bae-ui-motion)
- **Notes:** "Use legacy" does not exist for this effect. 32 bpc: values above 1.0 are processed without clamping in the hue/lightness steps (UNVERIFIED). Implementation hint: single fragment shader; precompute range weights from the six range definitions.

### AE-FX-042 · Brightness & Contrast (亮度和对比度)

- **What it does:** Adjusts overall brightness and contrast with a tone-curve model (default) or the legacy linear model.
- **Exact semantics:** matchName `ADBE Brightness & Contrast 2`; 32 bpc; GPU since AE 14.1; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Brightness (亮度) | `ADBE Brightness & Contrast 2-0001` | float slider | 0 | effective −150…150 (legacy mode clamps to −100…100, per Skottie); UNVERIFIED: AE slider/valid limits | — | |
  | 2 | Contrast (对比度) | `ADBE Brightness & Contrast 2-0002` | float slider | 0 | effective −50…100 (values below −50 behave as −50 in modern mode, per Skottie's measurements; legacy uses −100…100); UNVERIFIED: AE slider/valid limits (assumed −100…100) | — | |
  | 3 | Use Legacy (supports HDR) (使用旧版（支持 HDR）) | `ADBE Brightness & Contrast 2-0003` | checkbox | off | — | — | Legacy mode is the only mode that does not clamp 32 bpc values. |
  ```
  // Modern mode (Skottie's curve fit of AE; approximate):
  b = clamp(brightness, -150, 150) / 150
  c = clamp(contrast,   -50, 100) / 100
  x = 1 - (1 - x) ^ (2 ^ (1.8 * b))            // brightness, per channel, x in [0,1]
  x = x + (-c / (3π)) * sin(2π x)              // contrast (Skottie applies brightness, then contrast)
  // Legacy mode (exact linear model per Skottie):
  b = clamp(brightness, -100, 100) / 255
  c = clamp(contrast,   -100, 100) / 100
  S = (c > 0) ? 1 / max(1 - c, ε) : 1 + c
  x' = S * x + 0.5 * (1 - S) + b * max(S, 1)
  ```
- **Priority:** P0
- **Owner:** bae-effects
- **Notes:** The modern curve is Photoshop's non-linear Brightness/Contrast; the formulas above are fits (UNVERIFIED exactness), so BAE needs reference renders to tune. Obsolete matchName `ADBE Brightness & Contrast` (pre-CS6 linear version) maps to the legacy model.

### AE-FX-043 · Exposure (曝光度)

- **What it does:** Simulates a camera exposure change in f-stops in linear light, plus offset and gamma, for all channels or per channel.
- **Exact semantics:** matchName `ADBE Exposure2`; 32 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Channels (通道) | `ADBE Exposure2-0001` | popup | 1 = Master | Master (主音轨, sic in zh help) / Individual Channels (单个通道) | — | |
  | 2 | Master (主) | `-0002` start / `-0006` end | group | — | — | — | |
  | 3 | Exposure (曝光度) | `ADBE Exposure2-0003` | float slider | 0 | slider −5…5; valid −20…20 (UNVERIFIED) | f-stops | |
  | 4 | Offset (偏移) | `ADBE Exposure2-0004` | float slider | 0 | slider −0.5…0.5; valid −2…2 (UNVERIFIED) | — | |
  | 5 | Gamma Correction (灰度系数校正) | `ADBE Exposure2-0005` | float slider | 1.0 | slider 0.1…5 (UNVERIFIED); valid 0.0001…9.99 (UNVERIFIED) | — | |
  | 7–11 | Red: Red Exposure, Red Offset, Red Gamma Correction | `-0007` start, `-0008`…`-0010`, `-0011` end | group | 0, 0, 1 | as Master | | Active only in Individual Channels. |
  | 12–16 | Green: … | `-0012`…`-0016` | group | 0, 0, 1 | | | |
  | 17–21 | Blue: … | `-0017`…`-0021` | group | 0, 0, 1 | | | |
  | 22 | Bypass Linear Light Conversion (不使用线性光转换) | `ADBE Exposure2-0022` | checkbox | off | — | — | |
  ```
  for each colour channel c (alpha untouched):
      (e, o, g) = Master values, or the channel's own values in Individual Channels mode
      x = bypass ? c : to_linear(c)        // to_linear: working-space EOTF; identity when the
                                           // project is linearized (UNVERIFIED: sRGB curve when
                                           // no working space is set)
      x = x * 2^e + o
      x = sign(x) * |x|^(1/g)              // negative values mirrored around zero (per help)
      c' = bypass ? x : from_linear(x)
  ```
- **Priority:** P1 — standard HDR/linear grading tool.
- **Owner:** bae-effects (+ bae-color)
- **Notes:** Obsolete `ADBE Exposure` (CS3-era) lacked Bypass. The zh-CN help translates Master as 主音轨 (an audio-panel string reused by mistake); verify the UI string (主 / 主要) — （译名待核）. Cross-ref compositing-3d-color.md, Color management (linearize working space).

### AE-FX-044 · Color Balance (颜色平衡)

- **What it does:** Shifts the red/green/blue balance separately in shadows, midtones and highlights (Photoshop Color Balance).
- **Exact semantics:** matchName `ADBE Color Balance 2`; 16 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Shadow Red Balance (阴影红色平衡) | `ADBE Color Balance 2-0001` | float slider | 0 | −100…100 | — | |
  | 2 | Shadow Green Balance (阴影绿色平衡) | `-0002` | float slider | 0 | −100…100 | — | |
  | 3 | Shadow Blue Balance (阴影蓝色平衡) | `-0003` | float slider | 0 | −100…100 | — | |
  | 4 | Midtone Red Balance (中间调红色平衡) | `-0004` | float slider | 0 | −100…100 | — | |
  | 5 | Midtone Green Balance (中间调绿色平衡) | `-0005` | float slider | 0 | −100…100 | — | |
  | 6 | Midtone Blue Balance (中间调蓝色平衡) | `-0006` | float slider | 0 | −100…100 | — | |
  | 7 | Highlight Red Balance (高光红色平衡) | `-0007` | float slider | 0 | −100…100 | — | |
  | 8 | Highlight Green Balance (高光绿色平衡) | `-0008` | float slider | 0 | −100…100 | — | |
  | 9 | Highlight Blue Balance (高光蓝色平衡) | `-0009` | float slider | 0 | −100…100 | — | |
  | 10 | Preserve Luminosity (保持发光度) | `-0010` | checkbox | off | — | — | |
  - Parameter list and IDs are UNVERIFIED (from the AE UI order; not in the AE 26 dump); zh parameter names （译名待核）.
  ```
  // GIMP/Photoshop-style model (UNVERIFIED against AE):
  L = HSL lightness of the input
  a = 0.25; b = 0.333; s = 0.7
  wS = clamp01((L - b) / -a + 0.5) * s
  wM = clamp01((L - b) / a + 0.5) * clamp01((L + b - 1) / -a + 0.5) * s
  wH = clamp01((L + b - 1) / a + 0.5) * s
  c' = clamp01(c + wS * shadow_c/100 + wM * mid_c/100 + wH * high_c/100)   // c ∈ {R,G,B}
  if preserveLuminosity: replace HSL lightness of c' by L
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Obsolete `ADBE Color Balance` (older version) hidden. In 32 bpc projects this 16-bpc effect clips (AE-FX-015).

### AE-FX-045 · Black & White (黑白)

- **What it does:** Converts to greyscale with per-hue weights (Photoshop Black & White), optionally tinting the result.
- **Exact semantics:** matchName `ADBE Black&White`; 16 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Reds (红色) | `ADBE Black&White-0001` | float slider | 40 | −200…300 (UNVERIFIED; Photoshop range) | % | |
  | 2 | Yellows (黄色) | `-0002` | float slider | 60 | same | % | |
  | 3 | Greens (绿色) | `-0003` | float slider | 40 | same | % | |
  | 4 | Cyans (青色) | `-0004` | float slider | 60 | same | % | |
  | 5 | Blues (蓝色) | `-0005` | float slider | 20 | same | % | |
  | 6 | Magentas (洋红) | `-0006` | float slider | 80 | same | % | |
  | 7 | Tint (色调) | `-0007` | checkbox | off | — | — | |
  | 8 | Tint Color (色调颜色) | `-0008` | color | [225, 211, 179]/255 = [0.88235, 0.82745, 0.70196] | — | — | |
  - Defaults from the AE 26 dump; zh parameter names （译名待核）.
  ```
  // hue-sextant decomposition (Skottie, matching Photoshop):
  m  = min(R,G,B); dr = R-m; dg = G-m; db = B-m
  wy = min(dr,dg); wc = min(dg,db); wm = min(db,dr)        // secondaries
  wr = dr-wy-wm;   wg = dg-wy-wc;   wb = db-wc-wm          // primaries
  Y  = m + (Reds*wr + Yellows*wy + Greens*wg + Cyans*wc + Blues*wb + Magentas*wm) / 100
  if !tint: out.rgb = (Y, Y, Y)
  else:     out.rgb = tint_with(TintColor, Y)   // keeps Y as luminosity; UNVERIFIED formula
                                                // (Photoshop: colour with Tint's hue/sat at lightness Y)
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Check: pure red (1,0,0) → 0.40; yellow (1,1,0) → 0.60; grey stays grey (all weights apply to 0).

### AE-FX-046 · Vibrance (自然饱和度)

- **What it does:** Increases or decreases saturation, affecting low-saturation colours more and protecting skin tones (Vibrance), plus a uniform Saturation control.
- **Exact semantics:** matchName `ADBE Vibrance`; 16 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Vibrance (自然饱和度) | `ADBE Vibrance-0001` | float slider | 0 | −100…100 | — | IDs UNVERIFIED. |
  | 2 | Saturation (饱和度) | `ADBE Vibrance-0002` | float slider | 0 | −100…100 | — | |
  ```
  // Approximation (Photoshop algorithm is unpublished; UNVERIFIED):
  s = HSV saturation of input; h = hue
  skin = smooth weight that is 1 for hues in magenta…orange (≈ 300°…40°), else 0
  k_v  = vibrance/100 * (1 - s) * (1 - 0.5 * skin)
  c    = mid + (c - mid) * (1 + k_v)          // mid = luma(c)
  c    = mid + (c - mid) * (1 + saturation/100)
  clamp so no channel leaves [0,1]
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Needs reference renders from AE to tune; BAE decision: ship the approximation flagged "approximate" until tuned.

### AE-FX-047 · Photo Filter (照片滤镜)

- **What it does:** Simulates a coloured lens filter: tints the image with a preset or custom colour at a given density, optionally preserving luminosity.
- **Exact semantics:** matchName `ADBE PhotoFilterPS`; 32 bpc (since CS6); GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Filter (滤镜) | `ADBE PhotoFilterPS-0001` | popup | 1 = Warming Filter (85) | list below (22 entries incl. separator) | — | Choosing a preset sets Color. |
  | 2 | Color (颜色) | `ADBE PhotoFilterPS-0002` | color | [236, 138, 0]/255 (Warming 85) | — | — | Used when Filter = Custom; for presets the preset colour is used (UNVERIFIED whether Color is overwritten or ignored). |
  | 3 | Density (密度) | `ADBE PhotoFilterPS-0003` | float slider | 25 | 0…100 | % | |
  | 4 | Preserve Luminosity (保持发光度) | `ADBE PhotoFilterPS-0004` | checkbox | on | — | — | |
  - Filter menu (index: name (zh) = sRGB colour; colours are Photoshop's preset values, UNVERIFIED for AE): 1 Warming Filter (85) (暖色滤镜 (85)) = #EC8A00; 2 Warming Filter (LBA) = #FA9600; 3 Warming Filter (81) (暖色滤镜 (81)) = #EBB113; 4 Cooling Filter (80) (冷色滤镜 (80)) = #006DFF; 5 Cooling Filter (LBB) = #005DFF; 6 Cooling Filter (82) (冷色滤镜 (82)) = #00B5FF; 7 Red = #EA1A1A; 8 Orange = #F38417; 9 Yellow = #F9E31C; 10 Green = #19C919; 11 Cyan = #1DCBEA; 12 Blue = #1D35EA; 13 Violet = #9B1DEA; 14 Magenta = #E318E3; 15 Sepia = #AC7A33; 16 Deep Red = #FF0000; 17 Deep Blue = #0022CD; 18 Deep Emerald = #008C00; 19 Deep Yellow = #FFD500; 20 Underwater = #00C2B1; 21 separator `(-`; 22 Custom. (Warming 85 = [0.92549, 0.54118, 0] matches the AE 26 dump default of the obsolete version.)
  ```
  // Approximation (UNVERIFIED):
  f   = filter colour; d = density / 100
  t   = c * f                                  // multiply-like filter
  out = lerp(c, t, d)
  if preserveLuminosity: out = out * luma(c) / max(luma(out), ε)   // restore original luminance
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Parameter IDs 0001–0004 are taken from the obsolete version's AE 26 dump (Filter, Color, Density, Preserve Luminosity); for `ADBE PhotoFilterPS` only the menu was dumped (UNVERIFIED: same IDs). Obsolete `ADBE Photo Filter` (8/16 bpc) has a 20-entry menu without the LBA/LBB entries (1 Warming (85), 2 Warming (81), 3 Cooling (80), 4 Cooling (82), 5 Red … 18 Underwater, 19 separator, 20 Custom), so its indices differ from `ADBE PhotoFilterPS`; importers must remap. Imported Photoshop Photo Filter adjustment layers become this effect.

### AE-FX-048 · Channel Mixer (通道混合器)

- **What it does:** Rebuilds each output channel as a weighted sum of the input R, G, B channels plus a constant.
- **Exact semantics:** matchName `ADBE CHANNEL MIXER`; 32 bpc (docsforadobe table; the help page says 8/16); GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Red-Red (红色-红色) | `ADBE CHANNEL MIXER-0001` | float slider | 100 | −200…200 (UNVERIFIED) | % | |
  | 2 | Red-Green (红色-绿色) | `-0002` | float slider | 0 | same | % | |
  | 3 | Red-Blue (红色-蓝色) | `-0003` | float slider | 0 | same | % | |
  | 4 | Red-Const (红色-常量) | `-0004` | float slider | 0 | same | % | |
  | 5 | Green-Red (绿色-红色) | `-0005` | float slider | 0 | same | % | |
  | 6 | Green-Green (绿色-绿色) | `-0006` | float slider | 100 | same | % | |
  | 7 | Green-Blue (绿色-蓝色) | `-0007` | float slider | 0 | same | % | |
  | 8 | Green-Const (绿色-常量) | `-0008` | float slider | 0 | same | % | |
  | 9 | Blue-Red (蓝色-红色) | `-0009` | float slider | 0 | same | % | |
  | 10 | Blue-Green (蓝色-绿色) | `-0010` | float slider | 0 | same | % | |
  | 11 | Blue-Blue (蓝色-蓝色) | `-0011` | float slider | 100 | same | % | |
  | 12 | Blue-Const (蓝色-常量) | `-0012` | float slider | 0 | same | % | |
  | 13 | Monochrome (单色) | `-0013` | checkbox | off | — | — | |
  - IDs follow UI order (UNVERIFIED).
  ```
  R' = (RR*R + RG*G + RB*B + RC) / 100
  G' = (GR*R + GG*G + GB*B + GC) / 100
  B' = (BR*R + BG*G + BB*B + BC) / 100
  if monochrome: G' = B' = R'
  clamp to [0,1] in 8/16 bpc; unclamped in 32 bpc
  ```
- **Priority:** P1
- **Owner:** bae-effects
- **Notes:** Implementation hint: a 3×4 colour matrix; fuse with adjacent matrix effects (Tint, legacy B&C).

## Color Correction (catalog, P1/P2)

These effects are needed so AE projects open and render, but are rarely used in new work. Each item gives the identity, parameter list and a reference algorithm; exact defaults not confirmed by the AE 26 dump or the enum table are marked UNVERIFIED. Parameter zh names in this section are （译名待核） unless they appear in the zh-CN help page.

### AE-FX-049 · Auto Color, Auto Contrast and Auto Levels (自动颜色、自动对比度和自动色阶)

- **What it does:** Analyse each frame's histogram and stretch it so the darkest/lightest pixels map to black/white (Auto Contrast: overall contrast; Auto Levels: each channel separately; Auto Color: contrast and colour from shadows/midtones/highlights, with optional neutral-midtone snap), with temporal smoothing.
- **Exact semantics:** matchNames `ADBE AutoColor`, `ADBE AutoContrast`, `ADBE AutoLevels`; all 16 bpc; GPU —; expands bounds: no.
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Temporal Smoothing (seconds) (瞬时平滑（秒）) | float slider | 0 | 0…UNVERIFIED (slider 0…10) | s | 0 = each frame analysed alone. |
  | Scene Detect (场景检测) | checkbox | off | — | — | Ignores frames past a scene cut when smoothing. |
  | Black Clip (修剪黑色) | float slider | 0.10 (per help) | 0…10 (UNVERIFIED) | % | Fraction of darkest pixels ignored; help recommends 0…1 %. |
  | White Clip (修剪白色) | float slider | 0.10 (per help) | 0…10 (UNVERIFIED) | % | |
  | Snap Neutral Midtones (对齐中性中间调) | checkbox | off | — | — | Auto Color only. |
  | Blend With Original (与原始图像混合) | float slider | 0 | 0…100 | % | |
  ```
  for frames in [t - smooth/2, t + smooth/2] (cut at scene changes if Scene Detect): accumulate histograms
  lo = value at Black Clip percentile, hi = value at (100 - White Clip) percentile
     (Auto Contrast: one map from luminance, applied equally to R,G,B; Auto Levels: per channel
      (may add/remove casts); Auto Color: help says channels are not adjusted individually, so it
      does not add or remove casts except through Snap Neutral Midtones — exact algorithm UNVERIFIED)
  out = (in - lo) / (hi - lo)
  Auto Color + Snap Neutral Midtones: find near-neutral average colour, set per-channel gamma so it becomes grey
  out = lerp(out, in, blend/100)
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** These read multiple frames of the input (temporal smoothing) → need the layer-time sampling API of AE-FX-020; non-deterministic caching otherwise. Histogram analysis per frame is a GPU reduction.

### AE-FX-050 · Broadcast Colors (广播颜色)

- **What it does:** Limits composite-video signal amplitude (IRE) for NTSC or PAL by reducing luminance or saturation, or keys out unsafe/safe pixels.
- **Exact semantics:** matchName `ADBE Broadcast Colors`; 8 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Broadcast Locale (广播区域设置) | `ADBE Broadcast Colors-0001` | popup | 1 = NTSC | NTSC / PAL | — | |
  | 2 | How to Make Color Safe (确保颜色安全的方式) | `-0002` | popup | 1 = Reduce Luminance | Reduce Luminance / Reduce Saturation / (separator) / Key Out Unsafe / Key Out Safe | — | Separator consumes index 3. |
  | 3 | Maximum Signal Amplitude (IRE) (最大信号振幅 (IRE)) | `-0003` | float slider | 110 | 90…120 (UNVERIFIED) | IRE | |
  ```
  Y = luma601(c); C = chroma amplitude of the (I,Q) or (U,V) pair (NTSC: YIQ, PAL: YUV)
  amplitude_IRE = (Y + C) * 100 (+ 7.5 setup for NTSC, UNVERIFIED)
  if amplitude_IRE > max:
     Reduce Luminance:  scale Y down until the limit is met
     Reduce Saturation: scale C down until the limit is met
     Key Out Unsafe:    alpha = 0 ; Key Out Safe: alpha = 0 for pixels within the limit
  ```
- **Priority:** P2 — legacy broadcast QC; Video Limiter (AE-FX-069) replaces it.
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-051 · Change Color and Change to Color (更改颜色和更改为颜色)

- **What it does:** Change Color shifts hue/lightness/saturation of pixels near a chosen colour; Change to Color replaces a chosen colour (From) with another (To) using HLS tolerances.
- **Exact semantics:**
  - Change Color — matchName `ADBE Change Color`; 16 bpc; GPU —.
    | # | Parameter (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|---|
    | 1 | View (视图) | popup | 1 = Corrected Layer | Corrected Layer / Color Correction Mask | — | Mask view shows the match weight as grey. |
    | 2 | Hue Transform (色相变换) | float slider | 0 | −180…180 (UNVERIFIED) | degrees | |
    | 3 | Lightness Transform (亮度变换) | float slider | 0 | −100…100 | — | |
    | 4 | Saturation Transform (饱和度变换) | float slider | 0 | −100…100 | — | |
    | 5 | Color To Change (要更改的颜色) | color | UNVERIFIED (red [1,0,0]) | — | — | |
    | 6 | Matching Tolerance (匹配容差) | float slider | 15 (UNVERIFIED) | 0…100 | % | |
    | 7 | Matching Softness (匹配柔和度) | float slider | 0 | 0…100 | % | |
    | 8 | Match Colors (匹配颜色) | popup | 2 = Using Hue (UNVERIFIED default) | Using RGB / Using Hue / Using Chroma | — | |
    | 9 | Invert Color Correction Mask (反转颜色校正蒙版) | checkbox | off | — | — | |
  - Change to Color — matchName `ADBE Change To Color`; 16 bpc; GPU —.
    | # | Parameter (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|---|
    | 1 | From (自) | color | UNVERIFIED | — | — | |
    | 2 | To (至) | color | UNVERIFIED | — | — | |
    | 3 | Change (更改) | popup | 1 = Hue (UNVERIFIED default) | Hue / Hue & Lightness / Hue & Saturation / Hue, Lightness & Saturation | — | |
    | 4 | Change By (更改方式) | popup | 1 = Setting To Color | Setting To Color / Transforming To Color | — | |
    | 5 | Tolerance (容差) group: Hue, Lightness, Saturation | float ×3 | 5, 50, 50 (UNVERIFIED) | 0…100 | % | |
    | — | Softness (柔和度) | float slider | 50 (UNVERIFIED) | 0…100 | % | |
    | — | View Correction Matte (查看校正遮罩) | checkbox | off | — | — | |
  ```
  w = match weight of pixel vs key colour:
      distance in RGB, hue, or chroma (UV) space depending on Match Colors / Change;
      w = 1 inside tolerance, ramps to 0 over softness, else 0; invert if requested
  Change Color:   out = HLS(h + hueT*w, l + lightT*w, s + satT*w)
  Change to Color: Setting: hue (and L/S as selected) set to To's values, weighted by w;
                   Transforming: shifted by (To - From), weighted by w
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Parameter match-name suffixes follow the `#` column (Change Color enum IDs 0001 View and 0008 Match Colors, and Change to Color 0003 Change and 0004 Change By, are confirmed by the AE 26 enum table); later Change to Color IDs depend on group IDs and are UNVERIFIED.

### AE-FX-052 · Color Balance (HLS) (颜色平衡 (HLS))

- **What it does:** Legacy global hue rotation plus lightness and saturation offsets (kept for old projects).
- **Exact semantics:** matchName `ADBE Color Balance (HLS)`; 16 bpc; GPU —.
  | # | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|
  | 1 | Hue (色相) | angle | 0 | unbounded | degrees | |
  | 2 | Lightness (亮度) | float slider | 0 | −100…100 | — | |
  | 3 | Saturation (饱和度) | float slider | 0 | −100…100 | — | −100 = greyscale. |
  - Algorithm: same as Hue/Saturation Master (AE-FX-041) with these three values (UNVERIFIED exact identity).
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** AE help says it exists only for compatibility with older projects.

### AE-FX-053 · Color Link (颜色链接)

- **What it does:** Fills the layer with a single colour computed from statistics (average, median, brightest...) of a source layer, blended with the original by a blend mode.
- **Exact semantics:** matchName `ADBE Color Link`; 8 bpc; GPU —.
  | # | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|
  | 1 | Source Layer (源图层) | layer | None (= this layer with masks and effects) | — | — | A named layer is read without masks/effects. |
  | 2 | Sample (示例) | popup | 1 = Average | Average / Median / Brightest / Darkest / Max RGB / Min RGB / (sep) / Average Alpha / Median Alpha / Max Alpha / Min Alpha | — | |
  | 3 | Clip (剪切) | float slider | 0 (UNVERIFIED) | 0…100 | % | Ignore extreme pixels. |
  | 4 | Stencil Original Alpha (模板原始 Alpha) | checkbox | off | — | — | |
  | 5 | Blending Mode (混合模式) | popup | 1 = Normal | Normal / (sep) / Add / Multiply / Screen / Overlay / Soft Light / Hard Light / (sep) / Color Dodge / Color Burn / (sep) / Darken / Lighten / Difference / Exclusion / (sep) / Hue / Saturation / Color / Luminosity | — | Formulas per compositing-3d-color.md, Blend modes. |
  | 6 | Opacity (不透明度) | float slider | 100 | 0…100 | % | ID order of 5/6 UNVERIFIED (enum table puts Blending Mode at 0005). |
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Requires a whole-frame reduction of the source layer per frame (GPU reduction).

### AE-FX-054 · Color Stabilizer (颜色稳定器)

- **What it does:** Removes flicker by sampling 1–3 points on a pivot frame and adjusting other frames so those sample colours stay constant.
- **Exact semantics:** matchName `ADBE Deflicker`; 16 bpc; GPU —.
  | # | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|
  | — | Set Frame (设置帧) | button | — | — | — | Stores the current time as pivot frame. |
  | 1 | Stabilize (稳定) | popup | 1 = Brightness | Brightness / Levels / Curves | — | 1, 2 or 3 sample points. |
  | — | Black Point (黑场) | point | UNVERIFIED | — | px | |
  | — | Mid Point (中点) | point | UNVERIFIED | — | px | Curves only. |
  | — | White Point (白场) | point | UNVERIFIED | — | px | Levels / Curves. |
  | — | Sample Size (样本大小) | float slider | 5 (UNVERIFIED) | — | px | Radius of the sampled area. |
  - Algorithm: average colour in a disc of Sample Size at each point on the pivot frame and on the current frame; build a per-channel offset (Brightness), linear map (Levels) or 3-point curve (Curves) that maps current samples to pivot samples; apply to all pixels.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Reads another time of its own input (pivot frame) — AE-FX-020.

### AE-FX-055 · Colorama (色光)

- **What it does:** Converts a chosen pixel attribute (phase) through a cyclic colour palette (output cycle) with many modifiers; used for gradient maps and psychedelic cycling.
- **Exact semantics:** matchName `APC Colorama`; 16 bpc; GPU —; expands bounds: no.
  - Groups and key parameters (enum lists from the AE 26 enum table):
    | Group | Parameter (中文) | Type | Default | Values |
    |---|---|---|---|---|
    | Input Phase (输入相位) | Get Phase From (获取相位，自) `-0002` | popup | 1 = Intensity | Intensity / Red / Green / Blue / Hue / Lightness / Saturation / Value / Alpha / Zero |
    | | Add Phase (添加相位) | layer | None | |
    | | Add Phase From `-0006` | popup | 1 = Intensity | same list |
    | | Add Mode (添加模式) | popup | UNVERIFIED | Wrap / Clamp / Average / Screen (`-0040`? UNVERIFIED placement) |
    | | Phase Shift (相移) | angle | 0 | |
    | Output Cycle (输出循环) | Use Preset Palette (使用预设调板) `-0010` | popup | 1 = [none] | [none], Alpha Ramp, Hue Cycle, Negative, Ramp Red/Green/Blue/Grey, RGB, Saturation Ramp, Solarize Red/Green/Blue/Grey, (sep), Carribean, Clay, Copper, Deep Ocean, Earthenware, Fire, Fire And Smoke, Golden 1, Golden 2, Granite, Horizon, Leather, Moldy, Mossy, Old Glory, Rusty, Sepia 1, Sepia 2, … (full list in the enum table) |
    | | Output Cycle (输出循环) | arbitrary data (colour wheel with up to 64 triangles, UNVERIFIED count) | default Hue Cycle wheel (UNVERIFIED) | |
    | | Cycle Repetitions (循环重复次数) | float | 1 | |
    | | Interpolate Palette (插值调板) | checkbox | on | |
    | Modify (修改) | Modify `-0016` | popup | 1 = All | All / Red / Green / Blue / RG / GB / RB / Hue / Lightness / Saturation / HL / LS / HS / None |
    | | Modify Alpha (修改 Alpha) | checkbox | on (UNVERIFIED) | |
    | | Change Empty Pixels (更改空像素) | checkbox | on (UNVERIFIED) | |
    | Pixel Selection (像素选区) | Matching Color, Matching Tolerance, Matching Softness, Matching Mode `-0026` | color / float / float / popup | Off | Off / RGB / Hue / Chroma |
    | Masking (蒙版) | Mask Layer, Masking Mode `-0030`, Composite Over Layer, Blend With Original | layer / popup / checkbox / float | Off | Off / Intensity / Alpha / Inverted Intensity / Inverted Alpha |
  ```
  phase = attribute(in, getPhaseFrom) (+ attribute(addPhaseLayer, addPhaseFrom) via add mode)
  p     = fract(phase * cycleRepetitions + phaseShift/360)
  col   = palette(p)      // interpolated between wheel triangles if Interpolate Palette
  apply col to the channels selected by Modify, weighted by pixel-selection and masking weights
  out   = lerp(result, in, blend/100)
  ```
- **Priority:** P2 — complex, but widely used in older templates; implement Get Phase From + preset palettes + Modify first.
- **Owner:** bae-effects (+ bae-ui-motion)
- **Notes:** Palette definitions of the presets are Adobe data; BAE must recreate them by measurement (UNVERIFIED colours). Parameter IDs other than those in the enum table are UNVERIFIED.

### AE-FX-056 · Equalize (色调均化)

- **What it does:** Redistributes pixel values so brightness (or each RGB channel) is spread more evenly over the full range, like Photoshop's Equalize; transparent pixels are excluded from the histogram.
- **Exact semantics:** matchName `ADBE Equalize`; 8 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Equalize (色调均化) | `ADBE Equalize-0001` | popup | UNVERIFIED (2 = Brightness) | RGB / Brightness / Photoshop Style (AE 26 enum table) | — | |
  | 2 | Amount to Equalize (色调均化量) | `ADBE Equalize-0002` | float slider | UNVERIFIED (50) | 0…100 | % | 100 = as even as possible. |
  ```
  hist over pixels with A > 0 (8-bit bins):
    RGB:            one histogram per channel; map each channel through its own CDF
    Brightness:     histogram of brightness (max(R,G,B), UNVERIFIED) ; scale RGB so brightness follows the CDF
    Photoshop Style: histogram of luminance; same CDF applied to R,G,B (PS Equalize, UNVERIFIED exact variant)
  eq(v) = CDF(v)                        // normalised 0..1
  out   = lerp(v, eq(v), amount/100)
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Whole-frame histogram → GPU reduction pass then a LUT pass. zh name from the zh-CN help heading "色调均化效果".

### AE-FX-057 · Gamma/Pedestal/Gain (灰度系数/基值/增益)

- **What it does:** Per-channel response curve: a black stretch for low values, then gamma, pedestal (output minimum) and gain (output maximum) separately for R, G and B.
- **Exact semantics:** matchName `ADBE Gamma/Pedestal/Gain2`; 8 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|
  | 1 | Black Stretch (黑色伸缩) | float slider | 1 (UNVERIFIED) | 1…4 (UNVERIFIED) | — | Remaps low values of all channels; larger brightens darks. |
  | 2 | Red Gamma (红色灰度系数) | float slider | 1 | 0.1…10 (UNVERIFIED; slider 0.5…2) | — | |
  | 3 | Red Pedestal (红色基值) | float slider | 0 | −1…1 (UNVERIFIED) | — | 0 = off. |
  | 4 | Red Gain (红色增益) | float slider | 1 | 0…2 (UNVERIFIED) | — | 1 = completely on. |
  | 5–7 | Green Gamma / Pedestal / Gain (绿色…) | float | 1 / 0 / 1 | as red | — | |
  | 8–10 | Blue Gamma / Pedestal / Gain (蓝色…) | float | 1 / 0 / 1 | as red | — | |
  ```
  v  = blackStretch(c)       // UNVERIFIED: v = 1 - (1 - c)^? ; identity at 1.0
  v  = pow(v, 1 / gamma_ch)   // UNVERIFIED: AE help calls gamma "an exponent"; direction measured to be confirmed
  out = pedestal_ch + v * (gain_ch - pedestal_ch)
  ```
- **Priority:** P2 — legacy.
- **Owner:** bae-effects
- **Notes:** Parameter IDs assumed sequential (no groups) — UNVERIFIED. Build as a 3×1D LUT.

### AE-FX-058 · Leave Color (保留颜色)

- **What it does:** Desaturates everything except colours similar to Color To Leave.
- **Exact semantics:** matchName `ADBE Leave Color`; 8 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Amount to Decolor (脱色量) | `ADBE Leave Color-0001` | float slider | 0 (UNVERIFIED) | 0…100 | % | 100 = non-matching areas fully grey. |
  | 2 | Color To Leave (要保留的颜色) | `-0002` | color | UNVERIFIED (red) | — | — | |
  | 3 | Tolerance (容差) | `-0003` | float slider | UNVERIFIED (15) | 0…100 | % | 0 = only exact match kept; 100 = no change. |
  | 4 | Edge Softness (边缘柔和度) | `-0004` | float slider | UNVERIFIED (0) | 0…100 | % | |
  | 5 | Match Colors (匹配颜色) | `-0005` | popup | 1 = Using RGB | Using RGB / Using Hue (AE 26 enum table) | — | Help text says "Using HSB"; the menu string is "Using Hue". |
  ```
  d = Using RGB ? |rgb - key| / sqrt(3) : hueDistance(rgb, key) / 0.5
  w_keep = d <= tol ? 1 : (d <= tol + soft ? 1 - (d - tol)/soft : 0)    // UNVERIFIED ramp shape
  grey = luma(rgb) (UNVERIFIED weights; 601 assumed)
  out  = lerp(rgb, grey, (amount/100) * (1 - w_keep))
  ```
- **Priority:** P2 — Hue/Saturation and Lumetri HSL cover the common use.
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-059 · Lumetri Color: Basic Correction and Creative (Lumetri 颜色：基本校正和创意)

- **What it does:** The commonly used part of Lumetri Color: input LUT, white balance, tone (exposure, contrast, highlights, shadows, whites, blacks), saturation, and Creative look LUT with adjustments.
- **Exact semantics:** matchName `ADBE Lumetri`; 32 bpc; GPU since 13.8; expands bounds: no. Groups in UI order (parameter IDs are not sequential; the AE 26 enum table confirms `-0005` = Basic Correction Input LUT and `-0025` = Creative Look, both "None / [Custom] / Browse..." plus installed LUTs):
  | Group | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|
  | Basic Correction (基本校正) | Input LUT (输入 LUT) | popup + file | None | None / [Custom] / Browse… / bundled LUTs | — | Applied first. |
  | White Balance (白平衡) | WB Selector (白平衡选择器) | color (eyedropper) | — | — | — | Picks a neutral; sets Temperature/Tint. |
  | | Temperature (色温) | float slider | 0 | −100…100 | — | |
  | | Tint (色彩) | float slider | 0 | −100…100 | — | |
  | Tone (色调) | Exposure (曝光) | float slider | 0 | −5…5 | stops (UNVERIFIED) | |
  | | Contrast (对比度) | float slider | 0 | −100…100 | — | |
  | | Highlights (高光) | float slider | 0 | −100…100 | — | |
  | | Shadows (阴影) | float slider | 0 | −100…100 | — | |
  | | Whites (白色) | float slider | 0 | −100…100 | — | |
  | | Blacks (黑色) | float slider | 0 | −100…100 | — | |
  | | Reset / Auto (重置 / 自动) | button | — | — | — | Auto analyses the frame. |
  | | Saturation (饱和度) | float slider | 100 | 0…200 | — | |
  | Creative (创意) | Look (Look) | popup + file | None | None / [Custom] / Browse… / bundled looks (SL …) | — | |
  | | Intensity (强度) | float slider | 100 | 0…200 | % | |
  | Adjustments (调整) | Faded Film (淡化胶片) | float slider | 0 | 0…100 | — | |
  | | Sharpen (锐化) | float slider | 0 | −100…100 | — | Spatial; reads neighbours. |
  | | Vibrance (自然饱和度) | float slider | 0 | −100…100 | — | |
  | | Saturation (饱和度) | float slider | 100 | 0…200 | — | |
  | | Shadow Tint / Highlight Tint (阴影色彩 / 高光色彩) | colour wheel (arbitrary data) | neutral | — | — | |
  | | Tint Balance (色彩平衡) | float slider | 0 | −100…100 | — | |
  - Ranges and zh strings follow Premiere Pro's Lumetri panel — UNVERIFIED for AE; zh names （译名待核）.
  - Order of operations (UNVERIFIED, from Premiere documentation): Input LUT → White balance → Tone → Saturation → Creative Look × Intensity → Creative adjustments → Curves → Color Wheels → HSL Secondary → Vignette (AE-FX-060).
  - Algorithms are proprietary. BAE reference: Temperature/Tint = von Kries scale in linear light; Exposure = ×2^stops in linear light; Contrast = S-curve pivot 0.5 (log-ish); Highlights/Shadows/Whites/Blacks = smooth tone-zone curves; Saturation = lerp to luma709 (UNVERIFIED all).
- **Priority:** P1 — very common grading tool; exact parity requires measurement against AE renders.
- **Owner:** bae-effects (+ bae-color, bae-ui-motion)
- **Notes:** LUT files: .cube / .look / .3dl formats (bae-media parses; bae-color applies 3D LUT via a GPU texture). Bundled Adobe looks/LUTs cannot be redistributed; BAE decision: ship own looks and list missing Adobe ones as "missing LUT" (shows None with warning). Lumetri state also includes HDR options (HDR White, HDR Range) in newer versions (UNVERIFIED for AE).

### AE-FX-060 · Lumetri Color: Curves, Color Wheels, HSL Secondary and Vignette (Lumetri 颜色：曲线、色轮、HSL 辅助和晕影)

- **What it does:** The advanced Lumetri groups: RGB curves and Hue/Saturation curves, three-way colour wheels, HSL secondary keying with its own correction, and a vignette.
- **Exact semantics:** same effect as AE-FX-059 (`ADBE Lumetri`).
  - Curves (曲线): RGB Curves (master + R/G/B spline, arbitrary data); Hue Saturation Curves: Hue vs Sat, Hue vs Hue, Hue vs Luma, Luma vs Sat, Sat vs Sat (each a periodic or linear spline, arbitrary data). Default: identity.
  - Color Wheels (色轮): Shadows / Midtones / Highlights wheels (offset colour) with lightness sliders (default 0, −100…100 UNVERIFIED).
  - HSL Secondary (HSL 辅助): Key — Set color / Add color / Remove color eyedroppers, H, S, L range sliders (each min/max + feather), Color/Gray toggle (enum `-0075`: Color/Gray / Color/Black / White/Black per AE 26 enum table, i.e. the mask display mode); Refine — Denoise, Blur; Correction — colour wheel, Temperature, Tint, Contrast, Sharpen, Saturation.
  - Vignette (晕影): Amount 0 (−5…5), Midpoint 50 (0…100), Roundness 0 (−100…100), Feather 50 (0…100) (UNVERIFIED ranges).
  - Newer AE versions add input/output colour-space popups to Lumetri (enum `-0130`: None / ACEScct / Apple Log/Rec. 2020 / ARRI LogC3/Wide Gamut3 / … / Rec. 709 / sRGB) and technical LUT lists (`-0127`, `-0128`) — AE 26 enum table; semantics UNVERIFIED.
  ```
  curves:  c = rgbCurve_master(c); c.ch = rgbCurve_ch(c.ch)
           h,s,l = hsl(c); s *= 2*HueVsSat(h); h += HueVsHue(h)-0.5; l *= ...; s *= 2*LumaVsSat(l); s *= 2*SatVsSat(s)
  wheels:  lift/gamma/gain style: c += shadowOffset*wS(l) + midOffset*wM(l) + highOffset*wH(l)   // UNVERIFIED weights
  hsl sec: w = rangeWeight(h)*rangeWeight(s)*rangeWeight(l) -> denoise/blur(w) -> apply correction weighted by w
  vignette: d = ellipticalDistance(uv, roundness); f = smoothstep(midpoint, midpoint+feather, d); c *= 2^(amount*f) (UNVERIFIED)
  ```
- **Priority:** P2 — advanced grading; Curves (AE-FX-040) and Hue/Saturation (AE-FX-041) cover basic needs.
- **Owner:** bae-effects (+ bae-ui-motion)
- **Notes:** The curve and wheel editors need custom Effect Controls widgets (bae-ui-motion).

### AE-FX-061 · OCIO transform effects (OCIO 变换效果)

- **What it does:** Five OpenColorIO-based effects that convert a layer between colour spaces or apply OCIO looks, LUT/CDL files, or display/view transforms, using the project's OCIO configuration.
- **Exact semantics:** All 32 bpc (UNVERIFIED), Color Correction category, available when the project uses OCIO colour management (AE 24.x+, UNVERIFIED version).
  | Effect (中文) | matchName | Key parameters |
  |---|---|---|
  | OCIO Color Space Transform (OCIO 色彩空间转换) | `ADBE OCIO Color Space Transform` | Input Color Space, Output Color Space (lists from config) |
  | OCIO File Transform (OCIO 文件转换) | `ADBE OCIO FILE Transform` | LUT/CDL file, Interpolation, Inverse (UNVERIFIED) |
  | OCIO Display Transform (OCIO 显示变换) | `ADBE OCIO Display Transform` | Input Color Space, Display Device, View Transform, Inverse Transform, Information |
  | OCIO Look Transform (OCIO 外观变换) | `ADBE OCIO Look Transform` | Look, Input/Output space, Inverse (UNVERIFIED) |
  | OCIO CDL Transform (OCIO CDL 变换) | `ADBE OCIO CDL Transform` | Slope, Offset, Power (RGB), Saturation, Direction (UNVERIFIED) |
  - CDL formula (ASC CDL v1.2): `out = clamp(in*slope + offset)^power`, then saturation about Rec.709 luma: `luma + sat*(out - luma)`.
  - Dynamic lists are rebuilt from the config; if the project's config changes and a chosen name disappears, the Information section shows an error and the effect passes input through (UNVERIFIED).
- **Priority:** P2 — follows BAE's OCIO-subset colour management.
- **Owner:** bae-color (+ bae-effects)
- **Notes:** zh names from the zh-CN help headings. Matchnames from an AE 26.0 effect enumeration (third-party audit dump). See compositing-3d-color.md, Color management for the OCIO subset BAE supports. BAE decision: no OpenColorIO runtime (AGENTS.md); implement the transforms BAE's built-in config defines natively, CDL and .cube files natively.

### AE-FX-062 · PS Arbitrary Map (PS 任意映射)

- **What it does:** Legacy: applies a Photoshop arbitrary-map (.amp / lookup) curve to the layer or individual channels, with a cyclic phase shift.
- **Exact semantics:** matchName `ADBE PS Arbitrary Map`; 8 bpc in the match-name table (help says 8 and 16 bpc); GPU —; expands bounds: no.
  | # | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|
  | — | Options… (选项) | button | — | — | — | Loads an .amp file into arbitrary data (256-entry tables for master/R/G/B/A). |
  | 1 | Phase (相位) | float slider | 0 | UNVERIFIED | — | Shifts the map horizontally, cyclically. |
  | 2 | Apply Phase Map To Alpha (将相位图应用到 Alpha) | checkbox | off | — | — | Uses the map's alpha table, else identity. |
  ```
  idx = fract(v + phase / 256 (UNVERIFIED unit)) * 255
  out = table_ch[idx]   (default table: identity)
  ```
- **Priority:** P2 — compatibility only; AE says use Curves for new work.
- **Owner:** bae-effects (+ bae-media)
- **Notes:** —

### AE-FX-063 · Selective Color (可选颜色)

- **What it does:** Photoshop-style selective colour: adds or removes cyan, magenta, yellow and black within each of nine colour classes (Reds, Yellows, Greens, Cyans, Blues, Magentas, Whites, Neutrals, Blacks).
- **Exact semantics:** matchName `ADBE SelectiveColor`; 16 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Method (方法) | `ADBE SelectiveColor-0001` | popup | UNVERIFIED (1 = Relative) | Relative / Absolute | — | |
  | 2 | Colors (颜色) | `-0002` | popup | 1 = Reds | Reds / Yellows / Greens / Cyans / Blues / Magentas / Whites / Neutrals / Blacks | — | Selects which class the 4 sliders below edit (UI only). |
  | 3–6 | Cyan / Magenta / Yellow / Black (青色 / 洋红色 / 黄色 / 黑色) | — | float slider | 0 | −100…100 | % | Mirror of the Details entry for the selected class. |
  | — | Details (细节) group: Reds Cyan … Blacks Black | — | float slider ×36 | 0 | −100…100 | % | The stored/animated values (9 classes × CMYK). |
  ```
  mx,md,mn = max, mid, min of (R,G,B)
  weights (Photoshop-compatible reimplementation, UNVERIFIED vs AE):
    Reds/Greens/Blues:     w = mx - md   if that channel is the max
    Cyans/Magentas/Yellows: w = md - mn  if the complementary channel is the min
    Whites:   w = mn > 0.5 ? (mn - 0.5) * 2 : 0
    Blacks:   w = mx < 0.5 ? (0.5 - mx) * 2 : 0
    Neutrals: w = 1 - (|mx - 0.5| + |mn - 0.5|)
  for each class k with w_k > 0, for channel ch in R,G,B (ink = C for R, M for G, Y for B):
    a = clamp(ink_k + black_k + ink_k*black_k?, -1, 1)              // UNVERIFIED combination
    Relative: delta = -a * (a < 0 ? (1 - c) : c) ... ; Absolute: delta = -a        // UNVERIFIED
    c += w_k * delta
  ```
- **Priority:** P2 — mainly for Photoshop adjustment-layer fidelity.
- **Owner:** bae-effects
- **Notes:** AE help: the class weight is proportional to closeness to the class colour (50% magenta gets half the Magentas correction plus half the Whites correction). Relative cannot affect pure white. Imported PSD Selective Color adjustment layers map to this effect (see project-comp-layers.md, Import).

### AE-FX-064 · Shadow/Highlight (阴影/高光)

- **What it does:** Brightens shadows and darkens highlights based on the local neighbourhood (not a global curve), with optional automatic amounts and temporal smoothing.
- **Exact semantics:** matchName `ADBE ShadowHighlight`; 16 bpc; GPU —; expands bounds: no. Defaults from the AE 26 dump.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Auto Amounts (自动数量) | `ADBE ShadowHighlight-0001` | checkbox | on (1) | — | — | Ignores 2/3 and enables 4. |
  | 2 | Shadow Amount (阴影数量) | `-0002` | float slider | 50 | 0…100 | % | |
  | 3 | Highlight Amount (高光数量) | `-0003` | float slider | 0 | 0…100 | % | |
  | 4 | Temporal Smoothing (seconds) (瞬时平滑（秒）) | `-0004` | float slider | 0 | 0…UNVERIFIED | s | Only with Auto Amounts. |
  | 5 | Scene Detect (场景检测) | `-0005` | checkbox | off | — | — | |
  | 6 | More Options (更多选项) | `-0006` | group start | — | — | — | Group end is `-0015`. |
  | 7 | Shadow Tonal Width (阴影色调宽度) | `-0007` | float slider | 50 | 0…100 | % | |
  | 8 | Shadow Radius (阴影半径) | `-0008` | float slider | 30 | 0…UNVERIFIED | px | Scaled by downsample factor (AE-FX-018). |
  | 9 | Highlight Tonal Width (高光色调宽度) | `-0009` | float slider | 50 | 0…100 | % | |
  | 10 | Highlight Radius (高光半径) | `-0010` | float slider | 30 | 0…UNVERIFIED | px | |
  | 11 | Color Correction (颜色校正) | `-0011` | float slider | 20 | 0…100 (UNVERIFIED) | % | Saturation of adjusted areas. |
  | 12 | Midtone Contrast (中间调对比度) | `-0012` | float slider | 0 | −100…100 | — | |
  | 13 | Black Clip (修剪黑色) | `-0013` | float slider | 0.01 | 0…50 (UNVERIFIED) | % | AE 26 dump stores 0.01; help text says 0.1 % — UNVERIFIED which is displayed. |
  | 14 | White Clip (修剪白色) | `-0014` | float slider | 0.01 | 0…50 (UNVERIFIED) | % | |
  | 16 | Blend With Original (与原始图像混合) | `-0016` | float slider | 0 | 0…100 | % | |
  ```
  L    = luminance(in); Lb_s = gaussianBlur(L, shadowRadius); Lb_h = gaussianBlur(L, highlightRadius)
  ws   = smoothstep(shadowTonalWidth_as_level, 0, Lb_s)      // 1 in deep shadow, 0 above tonal width (UNVERIFIED)
  wh   = smoothstep(1 - highlightTonalWidth_as_level, 1, Lb_h)
  if Auto Amounts: shadowAmt, highlightAmt = from frame histogram (temporally smoothed)   // UNVERIFIED
  out  = in lifted by shadowAmt*ws, lowered by highlightAmt*wh (gamma-style, UNVERIFIED);
         saturation scaled by colorCorrection in adjusted areas; midtone contrast S-curve;
         final black/white clip stretch like Auto Levels (AE-FX-049)
  out  = lerp(out, in, blend/100)
  ```
- **Priority:** P2 — algorithm is proprietary (Photoshop Shadows/Highlights); needs measurement.
- **Owner:** bae-effects
- **Notes:** zh parameter names （译名待核）. Two separable blurs of luminance → implement on GPU with the shared blur kernel of Gaussian Blur (effects.md, Blur & Sharpen).

### AE-FX-065 · CC Color Neutralizer (CC 颜色中和器（译名待核）)

- **What it does:** Removes colour casts by mapping chosen shadow, midtone and highlight "unbalance" colours to neutral (or to chosen balance colours), per region.
- **Exact semantics:** matchName `CS Color Neutralizer`; 32 bpc; GPU —; expands bounds: no.
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Shadows Unbalance (阴影不平衡（译名待核）) | color | black (UNVERIFIED) | — | — | Colour that should become Shadows Balance. |
  | Shadows Balance | color | black (UNVERIFIED) | — | — | |
  | Midtones Unbalance / Midtones Balance | color | mid grey (UNVERIFIED) | — | — | |
  | Highlights Unbalance / Highlights Balance | color | white (UNVERIFIED) | — | — | |
  | Pinning (固定（译名待核）) | float slider | UNVERIFIED | — | % | Keeps extreme black/white from moving. |
  | Blend w. Original | float slider | 0 (UNVERIFIED) | 0…100 | % | |
  - Parameter list UNVERIFIED (from Cycore documentation memory). Reference algorithm: per channel, a 3-point curve through (unbalance_s→balance_s), (unbalance_m→balance_m), (unbalance_h→balance_h), pinned at 0 and 1 by Pinning.
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Cycore (CC) effects ship with AE; display names are not translated in zh-CN AE. In AE 26 the match name is `CS Color Neutralizer` (prefix CS, not CC).

### AE-FX-066 · CC Color Offset (CC 颜色偏移（译名待核）)

- **What it does:** Rotates each of R, G and B through the value range by a phase angle, with Wrap, Solarize or Polarize overflow.
- **Exact semantics:** matchName `CC Color Offset`; 16 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Red Phase (红色相位（译名待核）) | `CC Color Offset-0001` | angle | 0 | unbounded | degrees | |
  | 2 | Green Phase | `-0002` | angle | 0 | unbounded | degrees | |
  | 3 | Blue Phase | `-0003` | angle | 0 | unbounded | degrees | |
  | 4 | Overflow (溢出（译名待核）) | `-0004` | popup | 1 = Wrap (UNVERIFIED default) | Wrap / Solarize / Polarize (AE 26 enum table) | — | |
  ```
  x = c + phase_ch / 360
  Wrap:     out = fract(x)
  Solarize: out = 1 - |2*fract(x/2) - 1|  -> triangle wave (UNVERIFIED)
  Polarize: UNVERIFIED (reflects at 1 then clamps)
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** —

### AE-FX-067 · CC Kernel (CC 内核（译名待核）)

- **What it does:** 3×3 convolution of each pixel with its neighbours (blur, sharpen, emboss looks) despite being filed under Color Correction.
- **Exact semantics:** matchName `CS Kernel`; 32 bpc; GPU —; expands bounds: no (UNVERIFIED).
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | 9 kernel weights (row-major, top-left … bottom-right) | float slider ×9 | identity: centre 1, others 0 (UNVERIFIED) | UNVERIFIED | — | |
  | Divisor / normalisation (UNVERIFIED) | float | UNVERIFIED | — | — | |
  | Absolute Value (UNVERIFIED) | checkbox | off | — | — | |
  | Blend w. Original | float slider | 0 | 0…100 | % | |
  ```
  out(x,y) = sum_{i,j in -1..1} k[i][j] * in(x+i, y+j) / divisor   (premultiplied, edges clamped; UNVERIFIED)
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Parameter list UNVERIFIED. AE 26 has no other first-party 3×3 convolution effect (neither the docsforadobe table nor an AE 26.0 effect enumeration lists a "Convolution Kernel"), so CC Kernel is the only one.

### AE-FX-068 · CC Toner (CC 调色（译名待核）)

- **What it does:** Maps luminance to a 2-, 3- or 5-colour gradient (Duotone, Tritone, Pentone) or a solid tone, blended with the original.
- **Exact semantics:** matchName `CC Toner`; 32 bpc; GPU —; expands bounds: no. Lottie: supported by Skottie (`CCTonerEffect`), parameter order below follows its index table.
  | # | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|
  | 1 | Tones (色调（译名待核）) | popup | UNVERIFIED (2 = Tritone) | Duotone / Tritone / Pentone / (separator) / Solid (AE 26 enum table) | — | Values 1,2,3,5 (separator = 4). AE 26 enum table lists this popup under disk ID `-0005`, conflicting with Skottie's index 0 — ID mapping UNVERIFIED. |
  | 2 | Highlights (高光) | color | white (UNVERIFIED) | — | — | |
  | 3 | Brights (亮部（译名待核）) | color | UNVERIFIED | — | — | Pentone only. |
  | 4 | Midtones (中间调) | color | UNVERIFIED | — | — | |
  | 5 | Darktones (暗部（译名待核）) | color | UNVERIFIED | — | — | Pentone only. |
  | 6 | Shadows (阴影) | color | black (UNVERIFIED) | — | — | |
  | 7 | Blend w. Original (与原始图像混合) | float slider | 0 | 0…100 | % | |
  ```
  stops (5 evenly spaced at 0, .25, .5, .75, 1):
    Duotone: S, lerp(S,H,.25), lerp(S,H,.5), lerp(S,H,.75), H
    Tritone: S, lerp(S,M,.5), M, lerp(M,H,.5), H
    Pentone: S, D, M, B, H
    Solid:   M ×5
  l   = luminance(in)                       // Skottie: GradientColorFilter luminance (Rec.709 weights, UNVERIFIED for AE)
  out = lerp(in, gradient(l), (100 - blend)/100); alpha unchanged
  ```
- **Priority:** P2 — but Lottie-relevant (Skottie supports it).
- **Owner:** bae-effects
- **Notes:** Matches Skottie `CCTonerEffect.cpp`. Lottie effect `ty` is the generic custom effect (5) with matchName `CC Toner` (UNVERIFIED).

### AE-FX-069 · Video Limiter (视频限幅器)

- **What it does:** Clips luma and chroma of the signal to broadcast-legal ranges in the project working space (100 IRE = 1.0 in 32 bpc = 255 in 8 bpc); the modern, GPU-accelerated replacement for Broadcast Colors.
- **Exact semantics:** matchName `ADBE DigitalVideoLimiter` (AE 26 effect enumeration; not in the docsforadobe table); bpc: 32 (UNVERIFIED); GPU: yes per AE help (version UNVERIFIED); expands bounds: no.
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Clip Level (剪切级别（译名待核）) `-0001` | float slider (the AE 26 enum table lists no popup at `-0001`) | UNVERIFIED | UNVERIFIED | IRE | Maximum legal level (UNVERIFIED name; Premiere Pro's Video Limiter uses this name). |
  | Compression Before Clipping (剪切前压缩（译名待核）) `ADBE DigitalVideoLimiter-0002` | popup | UNVERIFIED (1 = None) | None / 3% / 5% / 10% / 20% (AE 26 enum table) | — | Soft knee that starts this far below the clip level. |
  | Further controls (e.g. clip method / reduction axis) | — | — | — | — | UNVERIFIED whether AE has any; needs a parameter dump. |
  ```
  Y,Cb,Cr = rgbToYCbCr709(c)  (working space)
  Y  = softClip(Y, minIRE/100, maxIRE/100, knee)      // Luma or Smart Limit
  Cr,Cb scaled so |chroma| stays within chroma limit  // Chroma or Smart Limit
  out = yCbCrToRgb(...)
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** zh name from the zh-CN help heading "视频限幅器效果" (the same page also calls the menu item 视频限制器). Parameter list needs a fetch of AE's Effect Controls; whole item is weakly sourced.

## Channel

Shared notes for this section: enum lists and parameter-ID positions of popups come from a generated table of popup options for AE 26.0x67 built-in effects (third-party ExtendScript enumeration, cached in the research folder) — referred to below as "the AE 26 enum table"; option N (1-based, separators `(-` included) is the stored value. Effects that take a second layer read it through the layer-parameter rules of AE-FX-012 and AE-FX-020: the other layer's pixels in its own layer space (source, or after masks / masks and effects per the layer-param menu), without its transform, at the same comp time, centred on or stretched to the effect layer. Unless stated, channel effects work on straight colour and do not expand bounds. zh effect names come from the zh-CN Channel effects help page.

### AE-FX-070 · Invert (反转)

- **What it does:** Inverts the colour information of the layer in RGB, HLS or YIQ space, all three components or a single one, or inverts alpha.
- **Exact semantics:** matchName `ADBE Invert`; 32 bpc; GPU since 14.1; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Channel (通道) | `ADBE Invert-0001` | popup | 1 = RGB | see enum below | — | |
  | 2 | Blend With Original (与原始图像混合) | `ADBE Invert-0002` | float slider | 0 | slider 0…100; valid 0…100 | % | |
  - Channel enum (AE 26 enum table, value = stored number): 1 RGB, 2 Red, 3 Green, 4 Blue, (5 separator), 6 HLS, 7 Hue, 8 Lightness, 9 Saturation, (10 separator), 11 YIQ, 12 Luminance, 13 In Phase Chrominance, 14 Quadrature Chrominance, (15 separator), 16 Alpha.
  ```
  RGB space:  R' = 1 - R (Red/Green/Blue: only that channel; RGB: all three)
  HLS space:  (h,l,s) = rgbToHls(R,G,B), h in 0..1
              Hue: h' = fract(h + 0.5)  (UNVERIFIED; Skottie uses h' = 0.5 - h)
              Lightness: l' = 1 - l ; Saturation: s' = 1 - s ; HLS: all three ; back to RGB
  YIQ space:  [Y,I,Q] = M_yiq * [R,G,B], M_yiq = [[0.299,0.587,0.114],[0.5959,-0.2746,-0.3213],[0.2115,-0.5227,0.3112]]
              Luminance: Y' = 1 - Y ; In Phase: I' = -I ; Quadrature: Q' = -Q ; YIQ: all three ; back via inverse matrix
  Alpha:      A' = 1 - A  (colour unchanged)
  out = lerp(result, in, blend/100)
  ```
  - 32 bpc: `1 - x` is applied without clamping, so super-white (x > 1) becomes negative (UNVERIFIED; AE generally does not clamp in float).
  - Inverting alpha of a layer whose transparent pixels have undefined colour reveals that colour (usually black in straight space).
- **Priority:** P0 — one of the most used basic effects; trivial colour matrix.
- **Owner:** bae-effects
- **Notes:** Lottie: no dedicated effect `ty`; exported as a generic effect (`ty: 5`) with `mn: "ADBE Invert"`; Skottie implements it (`InvertEffect.cpp`, same enum values and YIQ matrices), lottie-web ignores it. Implementation hint: every mode except HLS is a 4×5 colour matrix; fuse with neighbouring matrix effects.

### AE-FX-071 · Set Matte (设置遮罩)

- **What it does:** Replaces (or multiplies) the layer's alpha with a channel taken from another layer, so any layer anywhere in the stack can act as a traveling matte for one or many layers.
- **Exact semantics:** matchName `ADBE Set Matte3` (the CS6+ 32-bpc version); 32 bpc; GPU yes per help (version not in table); expands bounds: no. Defaults from the AE 26 dump.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Take Matte From Layer (从图层获取遮罩) | `ADBE Set Matte3-0001` | layer | this layer (index of self, dump value 1 on a 1-layer comp) | — | — | With Source / Masks / Effects & Masks menu (AE-FX-012). |
  | 2 | Use For Matte (用于遮罩) | `-0002` | popup | 4 = Alpha Channel | 1 Red Channel / 2 Green Channel / 3 Blue Channel / 4 Alpha Channel / 5 Luminance / 6 Hue / 7 Lightness / 8 Saturation / 9 Full / 10 Off | — | Lottie uses the same numbers. |
  | 3 | Invert Matte (反转遮罩) | `-0003` | checkbox | off | — | — | |
  | 4 | Stretch Matte to Fit (伸缩遮罩以适合) | `-0004` | checkbox | on | — | — | AE 26 scripting name of this property is "If Layer Sizes Differ" (UNVERIFIED whether the UI is still a checkbox). Off = matte layer centred. |
  | 5 | Composite Matte with Original (将遮罩与原始图像合成) | `-0005` | checkbox | on | — | — | On: multiply with existing alpha; off: replace it. |
  | 6 | Premultiply Matte Layer (预乘遮罩图层) | `-0006` | checkbox | on | — | — | |
  ```
  M = layerParam(1) rendered in its own layer space at the current time (no transform), positioned:
      stretch ? scaled to the effect layer's width/height : centred (pixels outside M = transparent)
  rgb_m = premultiplyMatteLayer ? M.rgb * M.a : M.rgb          // straight -> premultiplied (UNVERIFIED exact)
  v = switch useForMatte: Red: rgb_m.r | Green | Blue | Alpha: M.a | Luminance: luma(rgb_m) (weights UNVERIFIED)
      | Hue/Lightness/Saturation: from rgbToHls(rgb_m) | Full: 1 | Off: 0
  if invert: v = 1 - v
  A' = composite ? A * v : v ;  RGB unchanged
  ```
- **Priority:** P1 — legacy but very common in older templates and Lottie (`ty: 28`).
- **Owner:** bae-effects (+ bae-render)
- **Notes:** AE help says it exists for compatibility and suggests track mattes (see compositing-3d-color.md, Track mattes). Effects/masks of the matte layer are ignored unless the layer-param menu says otherwise, which is the classic gotcha. Lottie: effect `ty: 28` (Matte3); `ef[0]` Layer (layer index), `ef[1]` Channel, `ef[2]` Invert, `ef[3]` Stretch To Fit, `ef[4]` Show Mask (= Composite Matte with Original), `ef[5]` Premultiply Mask; lottie-web implements it only as an alpha mask (`SVGMatte3Effect`).

### AE-FX-072 · Shift Channels (转换通道)

- **What it does:** Rebuilds each output channel (alpha, red, green, blue) from a chosen source channel or derived value of the same pixel.
- **Exact semantics:** matchName `ADBE Shift Channels`; 32 bpc; GPU yes per help (version not in table); expands bounds: no (but Take Alpha From = Full On makes transparent areas of the layer rect opaque).
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Take Alpha From (从获取 Alpha（译名待核）) | `ADBE Shift Channels-0001` | popup | 1 = Alpha | enum below | — | |
  | 2 | Take Red From (从获取红色（译名待核）) | `-0002` | popup | 2 = Red | enum below | — | |
  | 3 | Take Green From (从获取绿色（译名待核）) | `-0003` | popup | 3 = Green | enum below | — | |
  | 4 | Take Blue From (从获取蓝色（译名待核）) | `-0004` | popup | 4 = Blue | enum below | — | |
  - Enum (AE 26 enum table, all four popups): 1 Alpha, 2 Red, 3 Green, 4 Blue, 5 Luminance, 6 Hue, 7 Lightness, 8 Saturation, 9 Full On, 10 Full Off.
  ```
  src(k) = Alpha: A | Red: R | Green: G | Blue: B | Luminance: luma(R,G,B) (Rec.709 in Skottie; AE weights UNVERIFIED)
         | Hue/Lightness/Saturation: rgbToHls(R,G,B) component | Full On: 1 | Full Off: 0
  A' = src(takeAlpha); R' = src(takeRed); G' = src(takeGreen); B' = src(takeBlue)
  ```
- **Priority:** P1 — common for channel tricks and alpha fixes; Skottie supports it.
- **Owner:** bae-effects
- **Notes:** The help zh heading is 转换通道效果 and the shared label 通道获取源 ("Take Channel From"); the individual parameter strings are （译名待核）. Skottie leaves Hue/Lightness/Saturation as TODO (outputs 0). Lottie: generic effect with `mn: "ADBE Shift Channels"`; `ef` order Alpha, Red, Green, Blue.

### AE-FX-073 · Set Channels (设置通道)

- **What it does:** Builds R, G, B and A of the layer from a chosen channel of up to four (possibly different) source layers.
- **Exact semantics:** matchName `ADBE Set Channels`; 16 bpc in the table (help says 8/16/32 bpc and GPU); expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Source Layer 1 (源图层 1) | `ADBE Set Channels-0001` | layer | None (= this layer) | — | — | |
  | 2 | Set Red To Source 1's (将源 1 设置为红色（译名待核）) | `-0002` | popup | 1 = Red | 1 Red / 2 Green / 3 Blue / 4 Alpha / 5 Luminance / 6 Hue / 7 Lightness / 8 Saturation / 9 Full / 10 Off | — | |
  | 3 | Source Layer 2 (源图层 2) | `-0003` | layer | None | — | — | |
  | 4 | Set Green To Source 2's | `-0004` | popup | 2 = Green | same | — | |
  | 5 | Source Layer 3 (源图层 3) | `-0005` | layer | None | — | — | |
  | 6 | Set Blue To Source 3's | `-0006` | popup | 3 = Blue | same | — | |
  | 7 | Source Layer 4 (源图层 4) | `-0007` | layer | None | — | — | |
  | 8 | Set Alpha To Source 4's | `-0008` | popup | 4 = Alpha | same | — | |
  | 9 | If Layer Sizes Differ: Stretch Layers to Fit (如果图层大小不同：伸缩图层以适合) | `-0009` | checkbox | on (UNVERIFIED) | — | — | Off = centred. |
  ```
  S_k = sourceLayer_k is None ? input : layerParam(k) (centred or stretched)
  R' = chan(S_1, setRed); G' = chan(S_2, setGreen); B' = chan(S_3, setBlue); A' = chan(S_4, setAlpha)
  chan(): as Shift Channels src() with Full = 1, Off = 0 (enum order differs: Red first)
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Popup defaults (1,2,3,4) give an identity result; IDs are confirmed by the AE 26 enum table (popups at 0002/0004/0006/0008).

### AE-FX-074 · Minimax (最小值/最大值)

- **What it does:** Replaces each channel of a pixel with the minimum or maximum of that channel within a radius (morphological erode/dilate), used to shrink or grow mattes.
- **Exact semantics:** matchName `ADBE Minimax`; 16 bpc in the table (help: 8/16/32 bpc, GPU-accelerated); expands bounds: no (UNVERIFIED — Maximum on alpha cannot grow past the layer rectangle).
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Operation (操作) | `ADBE Minimax-0001` | popup | 2 = Maximum (UNVERIFIED) | 1 Minimum / 2 Maximum / 3 Minimum Then Maximum / 4 Maximum Then Minimum | — | |
  | 2 | Radius (半径) | `-0002` | float slider | 0 | slider 0…100 (UNVERIFIED); valid 0…UNVERIFIED | px | Scaled by downsample factor (AE-FX-018). |
  | 3 | Channel (通道) | `-0003` | popup | 1 = Color | 1 Color / 2 Alpha and Color / 3 Red / 4 Green / 5 Blue / 6 Alpha | — | |
  | 4 | Direction (方向) | `-0004` | popup | 1 = Horizontal & Vertical | 1 Horizontal & Vertical / 2 Just Horizontal / 3 Just Vertical | — | |
  | 5 | Don't Shrink Edges (不要收缩边缘（译名待核）) | `-0005` | checkbox | off | — | — | UNVERIFIED version (CC 2015+). Treats outside-of-layer as not shrinking. |
  ```
  r = round(radius / downsample)          // integer pixel radius (UNVERIFIED rounding)
  op = min or max
  H&V:  out = op over the (2r+1)x(2r+1) square (separable: horizontal pass then vertical pass)   // square vs disc UNVERIFIED
  Just Horizontal / Just Vertical: one 1D pass
  "then" variants: second pass with the other op and the same r (opening / closing)
  applied to the selected channels only (Color = R,G,B; Alpha and Color = R,G,B,A)
  outside the layer: transparent black (so Minimum erodes from the layer edge unless Don't Shrink Edges)
  ```
  - Example from AE help: a white solid area on black, Minimum, radius 1 → shrinks by 1 px on each side.
  - Layer Quality does not affect Minimax.
- **Priority:** P1 — standard matte choke/spread tool.
- **Owner:** bae-effects
- **Notes:** Implementation hint: van Herk/Gil-Werman separable min/max is O(1) per pixel for any radius; operate on premultiplied or straight values consistently (AE: UNVERIFIED which).

### AE-FX-075 · Remove Color Matting (移除颜色遮罩)

- **What it does:** Removes colour fringes from semi-transparent pixels whose colour was premultiplied against a known background colour (formerly called "Unmultiply").
- **Exact semantics:** matchName `ADBE Remove Color Matting`; 32 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Background Color (背景颜色) | `ADBE Remove Color Matting-0001` | color | black [0,0,0] (UNVERIFIED) | — | — | Colour the edges were matted against. |
  | 2 | Clip HDR Results (剪切 HDR 结果（译名待核）) | `-0002` | checkbox | on | — | — | 32 bpc only matters; off allows values outside 0…1. |
  ```
  if A > 0:  C' = (C - (1 - A) * bg) / A          // C straight-stored but actually matted with bg
             if clipHDR: C' = clamp01(C')
  else:      C' = C                                // alpha 0: colour irrelevant
  A' = A
  ```
- **Priority:** P1 — standard fix after keying or for footage matted on a colour.
- **Owner:** bae-effects
- **Notes:** Not the same as Knoll Unmult-style effects that create alpha from brightness (AE help). Interpret Footage "Premultiplied – Matted With Color" does the same at import (see project-comp-layers.md, Interpret Footage).

### AE-FX-076 · Channel Combiner (通道合成器)

- **What it does:** Extracts a value (channel, HLS/YUV component, min/max RGB) from the layer or a second layer and writes it into chosen channels, or converts the whole pixel between colour models.
- **Exact semantics:** matchName `ADBE Channel Combiner`; 8 bpc in the table (help says 32 bpc and GPU-accelerated — conflict, UNVERIFIED); expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Use 2nd Layer (使用第二个图层) | `ADBE Channel Combiner-0001` | checkbox | off | — | — | |
  | 2 | Source Layer (源图层) | `-0002` | layer | None | — | — | Only with Use 2nd Layer. |
  | — | (IDs 0003–0004 hidden/unused, UNVERIFIED) | | | | | | |
  | 5 | From (自) | `-0005` | popup | UNVERIFIED (7 = Red?) | 1 RGB to HLS / 2 HLS to RGB / 3 RGB to YUV / 4 YUV to RGB / 5 Straight to Premultiplied / (6) / 7 Red / 8 Green / 9 Blue / 10 Alpha / (11) / 12 Hue / 13 Lightness / 14 Luminance / 15 Saturation / 16 Saturation Multiplied / (17) / 18 Min RGB / 19 Max RGB | — | Options 1–5 are whole-pixel conversions; To is ignored. |
  | 6 | To (至) | `-0006` | popup | UNVERIFIED | 1 Red / 2 Green / 3 Blue / 4 Alpha / (5) / 6 Hue / 7 Lightness / 8 Saturation / (9) / 10 Red only / 11 Green only / 12 Blue only / 13 Alpha only / (14) / 15 Hue only / 16 Lightness only / 17 Saturation only | — | |
  | 7 | Invert (反转) | `-0007` | checkbox | off | — | — | |
  | 8 | Solid Alpha (纯色 Alpha) | `-0008` | checkbox | off | — | — | A' = 1 everywhere. |
  ```
  S = use2nd ? layerParam(2) : input
  From 1..5: out = convert(S)   e.g. RGB to HLS writes (h,l,s) into (R,G,B); Straight to Premultiplied: rgb *= a
  else v = Red|Green|Blue|Alpha of S | Hue|Lightness|Saturation (HLS) | Luminance (UNVERIFIED weights)
         | Saturation Multiplied = s * (1 - |2l - 1|) (UNVERIFIED: "s times distance to black or white") | Min RGB | Max RGB
       if invert: v = 1 - v
       To Red/Green/Blue/Alpha: replace that channel of the input, keep others
       To Hue/Lightness/Saturation: replace that HLS component of the input
       "X only": Red only -> (v,0,0); Alpha only -> rgb = 1, A = v; Hue only -> hls(v, 0.5, 1);
                 Lightness only -> hls(any, v, 0); Saturation only -> hls(0, 0.5, v)
  if solidAlpha: A' = 1
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Help tip: From = channel, To = Lightness only shows any channel as greyscale. The meaning of Invert (input vs output) per help is "inverts the output channel values".

### AE-FX-077 · Compound Arithmetic (复合运算)

- **What it does:** Legacy: combines the layer with a second layer using an arithmetic or logical operator on 8-bit values, with clip/wrap/scale overflow.
- **Exact semantics:** matchName `ADBE Compound Arithmetic`; 8 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Second Source Layer (第二个源图层) | `ADBE Compound Arithmetic-0001` | layer | None | — | — | |
  | 2 | Operator (运算符) | `-0002` | popup | 1 = Copy (UNVERIFIED default) | 1 Copy / 2 Add / 3 Subtract / 4 Multiply / 5 Difference / 6 And / 7 Or / 8 Xor / 9 Lighter / 10 Darker / 11 Minimum / 12 Maximum / 13 Screen / 14 Overlay / 15 Hard Light | — | |
  | 3 | Operate On Channels (在通道上运算) | `-0003` | popup | 1 = RGB | 1 RGB / 2 ARGB / 3 Alpha | — | |
  | 4 | Overflow Behavior (溢出特性) | `-0004` | popup | 1 = Clip | 1 Clip / 2 Wrap / 3 Scale | — | |
  | 5 | Stretch Second Source to Fit (伸缩第二个源以适合) | `-0005` | checkbox | on | — | — | Off: second layer at its own size, aligned top-left. |
  | 6 | Blend With Original (与原始图像混合) | `-0006` | float slider | 0 | 0…100 | % | |
  ```
  a = this pixel, b = second-layer pixel, both as integers 0..255 per channel
  r = Copy: b | Add: a+b | Subtract: a-b | Multiply: a*b/255 | Difference: |a-b| | And/Or/Xor: bitwise
      | Lighter: max? Darker: min? (UNVERIFIED difference to Minimum/Maximum) | Screen: 255-(255-a)(255-b)/255
      | Overlay / Hard Light: standard formulas (compositing-3d-color.md, Blend modes)
  Clip: clamp 0..255 | Wrap: r mod 256 (258 -> 2, -3 -> 253) | Scale: remap [min r, max r] over the frame to 0..255
  out = lerp(r, a, blend/100)
  ```
- **Priority:** P2 — compatibility only (help recommends blend modes).
- **Owner:** bae-effects
- **Notes:** Scale needs a whole-frame min/max reduction.

### AE-FX-078 · Calculations (计算)

- **What it does:** Blends a chosen channel of the layer with a chosen channel of a second layer using a blending mode and opacity.
- **Exact semantics:** matchName `ADBE Calculations`; 16 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Input (输入) | `ADBE Calculations-0001` | group | — | — | — | |
  | 2 | Input Channel (输入通道) | `-0002` | popup | 1 = RGBA | 1 RGBA / 2 Gray / 3 Red / 4 Green / 5 Blue / 6 Alpha | — | Gray = luminance into R,G,B. |
  | 3 | Invert Input (反转输入) | `-0003` | checkbox | off | — | — | Applied before channel extraction. |
  | 5 | Second Layer (第二个图层) group | `-0005` | group | — | — | — | 0004 = Input group end. |
  | 6 | Second Layer (第二个图层) | `-0006` | layer | None | — | — | |
  | 7 | Second Layer Channel (第二个图层通道) | `-0007` | popup | 1 = RGBA | same list as Input Channel | — | |
  | 8 | Second Layer Opacity (第二个图层不透明度) | `-0008` | float slider | 100 | 0…100 | % | 0 = no influence. |
  | 9 | Invert Second Layer (反转第二个图层) | `-0009` | checkbox | off | — | — | |
  | 10 | Stretch Second Layer to Fit (伸缩第二个图层以适合) | `-0010` | checkbox | on (UNVERIFIED) | — | — | Off = centred. |
  | 12 | Blending Mode (混合模式) | `-0012` | popup | 1 = Normal | Normal / Copy / (sep) / Darken / Multiply / Color Burn / Classic Color Burn / (sep) / Add / Lighten / Screen / Color Dodge / Classic Color Dodge / (sep) / Overlay / Soft Light / Hard Light / Linear Light / Vivid Light / Pin Light / (sep) / Difference / Classic Difference / Exclusion / (sep) / Hue / Saturation / Color / Luminosity / (sep) / Stencil Alpha / Stencil Luma / Silhouette Alpha / Silhouette Luma / (sep) / Alpha Add / Luminescent Add | — | Formulas: compositing-3d-color.md, Blend modes. |
  | 13 | Preserve Transparency (保持透明度) | `-0013` | checkbox | off | — | — | Keeps the original alpha. |
  ```
  a = extract(invertInput ? 1 - in : in, inputChannel)      // Gray/Red/... broadcast into RGB (alpha kept? UNVERIFIED)
  b = extract(invertSecond ? 1 - L2 : L2, secondChannel); b.alpha *= secondOpacity/100
  out = blend(mode, src = b, dst = a)
  if preserveTransparency: out.a = in.a
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-render)
- **Notes:** Group IDs inferred from the popup IDs 0002/0007/0012 in the AE 26 enum table. Implementation hint: reuse the compositor's blend-mode shader library.

### AE-FX-079 · Arithmetic (算术)

- **What it does:** Applies one simple arithmetic or bitwise operation between each of R, G, B and a per-channel constant.
- **Exact semantics:** matchName `ADBE Arithmetic`; 8 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Operator (运算符) | `ADBE Arithmetic-0001` | popup | UNVERIFIED (4 = Add) | 1 And / 2 Or / 3 Xor / 4 Add / 5 Subtract / 6 Difference / 7 Min / 8 Max / 9 Block Above / 10 Block Below / 11 Slice / 12 Multiply / 13 Screen | — | |
  | 2 | Red Value (红色值（译名待核）) | `-0002` | slider | 0 | 0…255 | 8-bit level | |
  | 3 | Green Value (绿色值（译名待核）) | `-0003` | slider | 0 | 0…255 | | |
  | 4 | Blue Value (蓝色值（译名待核）) | `-0004` | slider | 0 | 0…255 | | |
  | 5 | Clip Result Values (剪切结果值) | `-0005` | checkbox | on | — | — | Off: results wrap (mod 256). |
  ```
  per channel, p = pixel 0..255, v = value:
  And/Or/Xor: bitwise | Add: p+v | Subtract: p-v | Difference: |p-v| | Min: min | Max: max
  Block Above: p > v ? 0 : p | Block Below: p < v ? 0 : p
  Slice: this channel = p > v ? 255 : 0, the other colour channels set to 255 (per help)  (UNVERIFIED interplay)
  Multiply: p*v/255 | Screen: 255 - (255-p)(255-v)/255
  clip ? clamp(0,255) : wrap mod 256
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Defined on 8-bit integers; in 16/32-bpc projects BAE converts as in AE-FX-015.

### AE-FX-080 · Blend (混合)

- **What it does:** Animatable blend of the layer with another layer using one of five modes (Crossfade, Color Only, Tint Only, Darken Only, Lighten Only).
- **Exact semantics:** matchName `ADBE Blend`; 16 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Blend With Layer (与图层混合) | `ADBE Blend-0001` | layer | None (= this layer) | — | — | |
  | 2 | Mode (模式) | `-0002` | popup | 1 = Crossfade | 1 Crossfade / 2 Color Only / 3 Tint Only / 4 Darken Only / 5 Lighten Only | — | |
  | 3 | Blend With Original (与原始图像混合) | `-0003` | float slider | 0 | 0…100 | % | 100 = original only. |
  | 4 | If Layer Sizes Differ (如果图层大小不同) | `-0004` | popup | 1 = Center | 1 Center / 2 Stretch to Fit | — | Layer Quality affects scaling only for Stretch. |
  ```
  b = layerParam(1) placed per (4)
  Crossfade:    r = b (including alpha; a proper crossfade with transparency)
  Color Only:   r = HLS(hue(b), sat(b), light(a))       // UNVERIFIED exact colour model
  Tint Only:    as Color Only but only where sat(a) > 0 (UNVERIFIED weighting)
  Darken Only:  r.rgb = min(a.rgb, b.rgb) ; Lighten Only: max
  out = lerp(r, a, blendWithOriginal/100)
  ```
- **Priority:** P2
- **Owner:** bae-effects
- **Notes:** Help: unlike layer blending modes, these are animatable via Blend With Original; to hide the control layer turn off its Video switch.

### AE-FX-081 · Solid Composite (纯色合成)

- **What it does:** Composites the layer over a solid colour (behind it) with separate opacities and a blending mode, without creating a solid layer.
- **Exact semantics:** matchName `ADBE Solid Composite`; 32 bpc; GPU —; expands bounds: no (fills only the layer rectangle).
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Source Opacity (源不透明度) | `ADBE Solid Composite-0001` | float slider | 100 | 0…100 | % | |
  | 2 | Color (颜色) | `-0002` | color | white [1,1,1] (UNVERIFIED) | — | — | |
  | 3 | Opacity (不透明度) | `-0003` | float slider | 100 | 0…100 | % | Solid opacity. |
  | 4 | Blending Mode (混合模式) | `-0004` | popup | 1 = Normal | Normal / (sep) / Add / Multiply / Screen / Overlay / Soft Light / Hard Light / (sep) / Color Dodge / Color Burn / (sep) / Darken / Lighten / Difference / Exclusion / (sep) / Hue / Saturation / Color / Luminosity | — | |
  ```
  dst = solid(color, alpha = opacity/100) over the layer rect
  src = input with alpha *= sourceOpacity/100
  out = composite(src over dst with blendingMode)   // formulas: compositing-3d-color.md, Blend modes
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-render)
- **Notes:** —

### AE-FX-082 · CC Composite (CC 合成（译名待核）)

- **What it does:** Re-composites the layer's original (pre-effect) image with the current effect-stack result using a blend mode — typically applied after other effects to bring back the original on top or behind.
- **Exact semantics:** matchName `CC Composite` per the docsforadobe table and the AE 26 enum table key (an AE 26.0 effect enumeration lists `CS Composite` — UNVERIFIED which one current AE writes; BAE must accept both); 16 bpc; GPU —; expands bounds: no.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Opacity (不透明度) | `CC Composite-0001` | float slider | 100 | 0…100 | % | |
  | 2 | Composite Original (合成原始项目（译名待核）) | `-0002` | popup | UNVERIFIED (2 = Behind?) | Copy / Behind / In front / (sep) / Darken / Multiply / Linear Burn / Color Burn / Classic Color Burn / (sep) / Add / Lighten / Screen / Linear Dodge / Color Dodge / Classic Color Dodge / (sep) / Overlay / Soft Light / Hard Light / Linear Light / Vivid Light / Pin Light / Hard Mix / (sep) / Difference / Classic Difference / Exclusion / (sep) / Hue / Saturation / Color / Luminosity / (sep) / Stencil Alpha / Stencil Luma / Silhouette Alpha / Silhouette Luma / (sep) / Add Alpha / Luminescent Premul | — | |
  | 3 | RGB Only (仅 RGB（译名待核）) | `-0003` | checkbox | off | — | — | Keep the current alpha. |
  ```
  orig = layer pixels before the first effect (after masks; UNVERIFIED)
  cur  = output of the effects above CC Composite
  Copy: r = orig ; Behind: r = cur over orig ; In front: r = orig over cur ;
  other modes: r = blend(mode, src = orig, dst = cur)
  r = lerp(cur, r, opacity/100) ; if RGBOnly: r.a = cur.a
  ```
- **Priority:** P2
- **Owner:** bae-effects (+ bae-render)
- **Notes:** Needs access to the pre-effects input inside the stack (BAE effect graph must keep it alive, AE-FX-001).
