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
- **Notes:** Scripting: `layer.property("ADBE Effect Parade")` (= `layer.Effects`, `layer.effect`), `addProperty(matchName)` appends an effect; `PropertyBase.moveTo(newIndex)` reorders. Expressions: `effect("Name")` or `effect(index)` (see transform-keyframes-expressions.md, effect object). Lottie: layer `ef` array, each entry `{ty, nm, mn, ix, en, ef:[values]}`; lottie-web only renders a few effect types (Tint 20, Fill 21, Stroke 22, Tritone 23, Pro Levels 24, Drop Shadow 25, Set Matte 28, Gaussian Blur 29, ...) and treats others (including `ty: 5` custom/expression-control effects) as data only. Implementation hint: build each layer's effect chain as a linear sub-graph of the render graph; adjacent pixel-independent effects (PF_OutFlag_PIX_INDEPENDENT, e.g. Levels → Curves → Tint) can be fused into one WGSL pass.

### AE-FX-002 · Apply, duplicate, copy/paste and delete effects (应用、复制、粘贴和删除效果)

- **What it does:** Basic editing commands on effect instances: apply from Effects & Presets / Effect menu, duplicate, copy/paste between layers, delete, rename.
- **Exact semantics:**
  - Apply: drag from Effects & Presets onto a layer (Timeline, Composition viewer or Effect Controls), double-click in Effects & Presets with layers selected (applies to every selected layer), or choose from the Effect menu (category submenus). If no layer is selected, double-clicking an animation preset creates a new layer; double-clicking an effect does nothing (UNVERIFIED for effects; documented for presets).
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
  - Per-effect switch (Effect Controls / Timeline, left of the effect name; drawn as "fx" icon, an eyeball icon in the AE 2025+ modernised Effect Controls): when off, the effect is skipped entirely (its input passes through unchanged), its parameters still evaluate and its keyframes are kept. Scripting: `PropertyBase.enabled` on the effect group; Lottie `en: 0|1`.
  - Layer Effects switch (Switches column, "fx", 效果): when off, every effect of that layer is skipped for preview and render. Scripting: `AVLayer.effectsActive` (boolean). Not animatable.
  - Render Settings > Effects (Current Settings / All On / All Off) overrides the switches at render time (see ui-preview-render-io.md, Render Queue).
  - Expression Controls effects are never rendered regardless of the switch, but their values still evaluate for expressions even when the effect switch is off (UNVERIFIED: AE returns the parameter value from a disabled effect; it does).
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
  | label / selection | UI only | Not persisted per effect except selection state. |
  - Effects have no in/out times of their own; an effect is active for the whole layer duration.
  - Scripting: `effect.isEffect === true`, `effect.propertyIndex`, `effect.canSetEnabled`. `app.effects` lists installed effects with `displayName`, `matchName`, `category`.
- **Priority:** P0
- **Owner:** bae-model (+ bae-plugin)
- **Notes:** BAE decision: store `matchName` verbatim for AE effects so AE projects and Lottie round-trip; BAE-native effects use reverse-DNS style IDs and may map to an AE matchName. Unknown matchNames are kept as placeholders (AE-FX-026). Cross-ref ARCHITECTURE §14 (plugin manifest/registry) and §15 (AE plug-in compatibility, sequence data slot).

### AE-FX-005 · Compositing Options: Effect Opacity (合成选项：效果不透明度)

- **What it does:** Every effect (AE CS6 11.0 and later) has a Compositing Options group whose Effect Opacity blends the effect's output with its input, like a per-effect "Blend With Original".
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
    | Mask Reference N (蒙版引用 N) | popup | None (无) | None, then every mask on this layer by name (Mask 1, Mask 2, ...) | — | Dynamic list; renaming a mask updates the menu. |
    | Inverted (反转) | checkbox | off | — | — | UNVERIFIED: per-reference invert checkbox next to the menu. |
  - Coverage: each referenced mask is rasterised with its own path, feather (incl. per-vertex feather), expansion, opacity and mode; references are combined in their order using the masks' modes (Add, Subtract, Intersect, Lighten, Darken, Difference; None contributes nothing), exactly as layer masks combine (compositing-3d-color.md, Masks). Result `m(x,y) ∈ [0,1]`, used in AE-FX-005: `out = lerp(in, fx, effectOpacity * m)`.
  - A mask that is referenced by any effect's Compositing Options does not cut the layer's alpha (UNVERIFIED: AE treats referenced masks as effect-only when their mode is None; with any other mode the mask also applies to the layer — users set the mask mode to None to use it purely as an effect mask). Adobe help: "An effect mask cannot be used to modify the alpha channel of a layer."
  - Mask Opacity scales the effect intensity inside that mask. Only masks on the same layer can be referenced; references to a deleted mask revert to None.
  - Version: Compositing Options and Mask Reference added in AE CS6 (11.0, 2012).
- **Priority:** P1
- **Owner:** bae-render (+ bae-model, bae-vector)
- **Notes:** UNVERIFIED match names: references group `"ADBE Effect Mask Parade"`, each reference `"ADBE Effect Mask"` with `"ADBE Effect Path Stream Ref"` (menu) and `"ADBE Effect Mask Invert"` (checkbox). Scripting cannot set the referenced mask directly in AE (known limitation). Implementation hint: reuse the mask coverage rasteriser of the layer-mask pass and feed it as a texture into the per-effect mix shader.

### AE-FX-007 · Parameter identity: indices, disk IDs and parameter match names (参数标识：索引、磁盘 ID 和匹配名称)

- **What it does:** Defines how each effect parameter is identified in projects, scripts, expressions and plug-ins.
- **Exact semantics:**
  - Index: parameters are numbered 1..n in UI order (`effect.property(1)` = first visible parameter). The SDK numbers them 0..n with param[0] = input layer; SDK index k ↔ property index k.
  - Group start markers (topics) occupy an index; the matching group end does not appear as a property (UNVERIFIED: scripting sees the topic as a PropertyGroup; its children continue the effect-level numbering in match names).
  - Parameter match name: `"<effect matchName>-NNNN"`, NNNN = 4-digit zero-padded disk ID, e.g. Slider Control's slider = `"ADBE Slider Control-0001"`, Levels (Individual Controls) Red Gamma = `"ADBE Pro Levels2-0013"` (UNVERIFIED index). For built-in effects the disk ID equals the 1-based index at the time the effect was designed; later-added parameters get new IDs appended even if they are displayed earlier (so disk ID ≠ display index for some effects, e.g. parameters added in newer versions). Expressions and scripts that address by match name survive localisation; by display name they do not.
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
- **Notes:** Lottie value `ty: 0` (Slider) and `ty: 1` (Angle), value in `v` (animatable property with `a`, `k`). Expression `effect("X")(n).value` returns a Number.

### AE-FX-009 · Checkbox and popup parameters (复选框和弹出菜单参数)

- **What it does:** Discrete parameters: on/off checkbox and enumerated popup menu.
- **Exact semantics:**
  - Checkbox (`PF_Param_CHECKBOX`): value 0/1 (expressions return `0`/`1`; scripting returns `true/false`... UNVERIFIED: scripting `value` returns 1/0 for effect checkboxes). `PF_ParamFlag_CANNOT_INTERP` is forced on: keyframes are always Hold. Expression results are thresholded: non-zero → on (UNVERIFIED rounding rule: AE treats value ≥ 0.5 as on? report: any non-zero is on).
  - Popup (`PF_Param_POPUP`): value = 1-based item index (A_long). Items are fixed at effect definition ("Entry1|Entry2|..."; "(-" draws a separator, which still consumes an index). Keyframes are Hold only. Expression results are rounded to the nearest integer and clamped to [1, n] (UNVERIFIED rounding).
  - Lottie: checkbox `ty: 4`, popup/dropdown `ty: 7`; values stored as numbers (popup 1-based).
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
  - Animation: both dimensions animate together; point parameters are spatial (motion path shown in the viewer with spatial bezier/auto-bezier handles) (UNVERIFIED: effect points use spatial interpolation like Position; they draw motion paths in the Layer panel). Separate Dimensions is not available on effect points.
  - Downsampling: pixel values are multiplied by the downsample factor before rendering (AE-FX-018).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-ui-motion)
- **Notes:** Lottie `ty: 3` (Point; 3D points are exported as 3-element point values UNVERIFIED). Expressions: `effect("X")("Point")` returns [x, y] in layer space; use `toComp()` to convert.

### AE-FX-012 · Layer parameter (图层参数)

- **What it does:** A popup that selects another layer of the same comp as an input image (control layer / layer map), with a second menu choosing which stage of that layer to use.
- **Exact semantics:**
  - `PF_Param_LAYER`: value = layer reference (stored by layer ID; menu shows "None" (无) then layers as "N. Name"). Choosing the layer itself is allowed for most effects (the effect then receives the layer's own source stage, not its own output).
  - Second menu (AE CC 2015 13.5+ UNVERIFIED version): `Source` (源) / `Masks` (蒙版) / `Effects & Masks` (效果和蒙版). Source = layer's source footage/frame without masks, effects or transforms; Masks = source with its masks applied; Effects & Masks = after masks and effects. Default = Source. Transforms are never included (Adobe help: "The compound effect ignores effects, masks, and transformations of a control layer" — precompose to include transforms).
  - Time: the referenced layer is sampled at the same comp time as the current frame, mapped through that layer's own time stretch/remap (UNVERIFIED: AE checks out the layer at the comp time, converted to the layer's own time).
  - Size: the control layer arrives at its own pixel size; effects with "Stretch Map To Fit"-type options scale it to the destination size, otherwise it is centred on the destination layer.
  - The control layer's video switch does not matter (hidden layers are usable). Track-matte layers can be referenced. Not animatable (no keyframes; UNVERIFIED: layer params cannot be keyframed but can take an expression in AE 2019+? — Layer Control accepts expressions? no: layer params are not expression-capable except Layer Control? keep: not keyframeable, not expression-capable).
  - Scripting cannot read or set the Source/Masks/Effects & Masks menu (known AE limitation); value via `property.value` = layer index (0 = None).
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
  | No value / label | Lottie `Effect No Value`; SDK param with `PF_PUI_ECW_SEPARATOR` / custom UI | Placeholder rows (e.g. histogram rows of Levels, labels). | no |
  | Arbitrary data | `PF_Param_ARBITRARY_DATA` | Opaque blob with an ID and default; interpolation provided by the effect (e.g. Curves' curve data, Levels' histogram). Effect supplies copy/compare/interpolate/flatten callbacks. | if the effect implements interpolation (Curves: yes, UNVERIFIED) |
  | Path | `PF_Param_PATH` | Reference to a mask on the same layer (index of mask; "None" = 0). The effect reads the mask's path at render time. | no (the referenced mask animates) |
  - Parameter flags BAE must model (SDK `PF_ParamFlag_*`, `PF_PUI_*`): CANNOT_TIME_VARY (no keyframes), CANNOT_INTERP (hold only), SUPERVISE (UI callbacks), START_COLLAPSED, COLLAPSE_TWIRLY, INVISIBLE, DISABLED, NO_ECW_UI (hidden in Effect Controls), STD_CONTROL_ONLY.
- **Priority:** P0 for groups and no-value rows; P1 for path; P2 for button and arbitrary data (internal use by built-in effects such as Curves).
- **Owner:** bae-model (+ bae-plugin, bae-ui-motion)
- **Notes:** Lottie: Ignored value `ty: 6`, No Value has no `ty`; groups appear as `ty: 5` custom-effect children (UNVERIFIED). Cross-ref ARCHITECTURE §15 (BAE parameter types must be a superset of AE's).

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
  - Effect Controls: "Reset" link (AE ≤ 24) / Reset Effect icon (AE 25.x modernised Effect Controls) at the top of the effect. Removes all keyframes of the effect's parameters and sets static defaults; expressions are UNVERIFIED (AE keeps expressions? — Reset removes keyframes and sets values; expressions remain attached UNVERIFIED).
  - Also resets Compositing Options (Effect Opacity 100%, mask references removed? UNVERIFIED).
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
  - Hidden obsolete effects (load only from old projects): Alpha Levels, Apply Color LUT, Box Blur, Cineon Converter, old Color Balance / Exposure / Levels / Photo Filter / Set Matte / Brightness & Contrast, Three-Way Color Corrector, Threshold, Unsharp Mask, Photoshop layer-style effects (`ADBE PSL *`), etc. (docsforadobe "_Obsolete").
  - An effect can also keep its matchName and change behaviour behind a parameter (e.g. Brightness & Contrast "Use Legacy", Hue/Saturation "Use legacy"? no; see each effect).
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
  - Lottie/bodymovin exports them as an effect with `ty: 5` (Custom Effect), `mn` = effect matchName, `nm` = instance name, `ef` = one effect value whose `ty` is 0 slider, 1 angle, 2 color, 3 point, 4 checkbox, 7 dropdown, 10 layer (3D Point is exported as ty 3 with a 3-element value, UNVERIFIED).
- **Priority:** P0 — expression rigs and templates depend on them.
- **Owner:** bae-model (+ bae-anim, bae-ui-motion)
- **Notes:** Implementation hint: the render graph should skip these nodes entirely (identity), so they cost nothing at render time. The animation preset "Animated Shape Control" in AE's bundled presets is a pseudo-effect (custom expression-control effect defined in an .ffx); pseudo-effects are a P2 topic (AE-FX-023).

### AE-FX-028 · Slider Control (滑块控制)

- **What it does:** Exposes one floating-point number.
- **Exact semantics:** matchName `ADBE Slider Control`; 32 bpc; GPU —; expands bounds: no; Lottie effect `ty: 5`, value `ty: 0`.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Slider (滑块) | `ADBE Slider Control-0001` | float slider | 0 | slider 0…100; valid −1,000,000…1,000,000 (UNVERIFIED valid range) | none | 2 decimals displayed; Edit Value can widen the slider range up to the valid range. |
  ```
  render(in) = in
  value(t)   = evaluated property value (keyframes / expression), clamped to valid range
  ```
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Expressions read a Number. AE scripting cannot read or set the slider range of an instance (community-reported; no API). BAE decision: store the per-instance slider range in the model so Essential Graphics and the UI can show it.

### AE-FX-029 · Angle Control (角度控制)

- **What it does:** Exposes one angle in degrees with the rotation dial UI.
- **Exact semantics:** matchName `ADBE Angle Control`; 32 bpc; GPU —; expands bounds: no; Lottie value `ty: 1`.
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Angle (角度) | `ADBE Angle Control-0001` | angle | 0 | unbounded revolutions (displayed `Nx+D°`) | degrees | Value is a plain Number in degrees in expressions (e.g. 450 = 1x+90°). |
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Interpolates numerically (450 → 0 turns back 450°, no shortest-path wrapping).

### AE-FX-030 · Checkbox Control (复选框控制)

- **What it does:** Exposes one on/off switch.
- **Exact semantics:** matchName `ADBE Checkbox Control`; 32 bpc; GPU —; expands bounds: no; Lottie value `ty: 4`.
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
- **Notes:** Introduced in AE CS5.5? UNVERIFIED version (3D Point parameter type `PF_Param_POINT_3D` was added to the SDK in CS5.5 / AE 10.5). Lottie export of this control is not in the Lottie docs effect-value list; BAE exports it as `ty: 3` with three components (BAE decision).

### AE-FX-034 · Layer Control (图层控制)

- **What it does:** Exposes one layer reference, which expressions use as a layer object.
- **Exact semantics:** matchName `ADBE Layer Control`; 32 bpc; GPU —; expands bounds: no; Lottie value `ty: 10` (value = referenced layer index).
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Layer (图层) | `ADBE Layer Control-0001` | layer | None (UNVERIFIED) | None or any layer of the same comp (including itself) | — | Not keyframable to arbitrary values: Hold keys only (layer params cannot interpolate). |
  - `effect("Layer Control")("Layer")` returns a Layer object, so `.transform.position`, `.effect(...)`, `.sourceRectAtTime()` etc. work on it (see transform-keyframes-expressions.md, Layer object). When the parameter is None the expression gets an error / null (UNVERIFIED exact behaviour).
  - The reference is stored by layer identity (survives reordering and renaming), not by index.
  - Unlike pixel effects' layer parameters there is no "Source / Masks / Effects & Masks" choice (no pixels are read).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Layer Control was added in AE CS5.5? UNVERIFIED version. Deleting the referenced layer resets the parameter to None.

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
- **Exact semantics:** matchName `ADBE Easy Levels2`; 32 bpc; GPU since AE 14.2 (2017); expands bounds: no (Lottie: not supported directly; exporters convert to Pro Levels `ty: 24`, UNVERIFIED).
  | # | Parameter (中文) | param matchName | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|---|---|
  | 1 | Channel (通道) | `ADBE Easy Levels2-0001` | popup | 1 = RGB | RGB / Red / Green / Blue / Alpha | — | Selects which channel's five values the controls show and edit. |
  | 2 | Histogram (直方图) | `ADBE Easy Levels2-0002` | arbitrary data | — | — | — | UI only (histogram of the effect input at the current time); holds per-channel level values (UNVERIFIED). |
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
      t = max(t, 0) ^ (1 / g)              // UNVERIFIED for t < 0 with clipping off (Skottie clamps t at 0)
      y = ob + (ow - ob) * t
      if clipB is On, or (Off for 32 bpc and project bpc != 32): y = max(y, min(ob, ow))
      if clipW is On, or (Off for 32 bpc and project bpc != 32): y = min(y, max(ob, ow))
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
- **Notes:** Legacy matchName `ADBE Easy Levels` (pre-CS6? UNVERIFIED version; hidden obsolete) used 8-bit-style values. The order per-channel → composite follows lottie-web/Skottie's Pro Levels implementations (lottie-web applies the R/G/B/A component-transfer before the composite one); AE's own order is UNVERIFIED. The Histogram's "auto" buttons do not exist in Levels (see Auto Levels, AE-FX-049). Implementation hint: build a 1D LUT per channel (4096 entries or analytic in-shader for 32 bpc).

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
  | 1 | Brightness (亮度) | `ADBE Brightness & Contrast 2-0001` | float slider | 0 | −150…150 (legacy mode uses −100…100) | — | |
  | 2 | Contrast (对比度) | `ADBE Brightness & Contrast 2-0002` | float slider | 0 | −50…100 effective (slider −100…100, negative values below −50 behave as −50, per Skottie) | — | |
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
- **Notes:** Obsolete `ADBE Photo Filter` (8/16 bpc) has a 20-entry menu without the LBA/LBB entries (1 Warming (85), 2 Warming (81), 3 Cooling (80), 4 Cooling (82), 5 Red … 18 Underwater, 19 separator, 20 Custom), so its indices differ from `ADBE PhotoFilterPS`; importers must remap. Imported Photoshop Photo Filter adjustment layers become this effect.

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

- **What it does:** Analyse each frame's histogram and stretch it so the darkest/lightest pixels map to black/white (Auto Contrast: luminance only; Auto Levels: per channel; Auto Color: per channel plus optional neutral-midtone snap), with temporal smoothing.
- **Exact semantics:** matchNames `ADBE AutoColor`, `ADBE AutoContrast`, `ADBE AutoLevels`; all 16 bpc; GPU —; expands bounds: no.
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Temporal Smoothing (seconds) (时间滤波（秒）) | float slider | 0 | 0…UNVERIFIED (slider 0…10) | s | 0 = each frame analysed alone. |
  | Scene Detect (场景检测) | checkbox | off | — | — | Ignores frames past a scene cut when smoothing. |
  | Black Clip (黑色剪切) | float slider | 0.10 | 0…10 (UNVERIFIED) | % | Fraction of darkest pixels ignored. |
  | White Clip (白色剪切) | float slider | 0.10 | 0…10 (UNVERIFIED) | % | |
  | Snap Neutral Midtones (对齐中性中间调) | checkbox | off | — | — | Auto Color only. |
  | Blend With Original (与原始图像混合) | float slider | 0 | 0…100 | % | |
  ```
  for frames in [t - smooth/2, t + smooth/2] (cut at scene changes if Scene Detect): accumulate histograms
  lo = value at Black Clip percentile, hi = value at (100 - White Clip) percentile
     (Auto Contrast: of luminance, applied equally to R,G,B; Auto Levels / Auto Color: per channel)
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
- **Priority:** P2 — legacy broadcast QC; Video Limiter (AE-FX-068) replaces it.
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
