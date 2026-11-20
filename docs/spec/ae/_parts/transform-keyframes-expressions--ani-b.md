## Expression engine and evaluation model

### AE-ANI-400 · Expression engine selection (表达式引擎)

- **What it does:** Selects which language runtime evaluates every expression in the project: the JavaScript engine (ECMAScript 2018) or the Legacy ExtendScript engine (ECMAScript 3).
- **Exact semantics:**
  - Project-level setting, File > Project Settings > Expressions tab (项目设置 > 表达式（译名待核）).

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Expressions Engine (表达式引擎) | enum | JavaScript | JavaScript, Legacy ExtendScript | — | scripting `app.project.expressionEngine`: `"javascript-1.0"` / `"extendscript"` |

  - Enum menu order: `JavaScript` (JavaScript), `Legacy ExtendScript` (旧版 ExtendScript).
  - New projects default to JavaScript since AE 16.0 (CC 2019, Oct 2018), the version that added the setting. Projects saved in earlier AE versions (no engine field) open with Legacy ExtendScript selected (documented: "Projects saved in previous versions of After Effects default to Legacy ExtendScript"); a project saved by 16.0+ keeps its saved engine.
  - Switching the engine re-evaluates every expression in the project; no expression text is rewritten.
  - The JavaScript engine is documented as up to 5x faster than Legacy at render time and as having higher floating-point precision. On Windows AE uses V8 for the JavaScript engine.
- **Priority:** P0 (the engine choice defines the whole expression language)
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Source: https://helpx.adobe.com/after-effects/using/expression-language-reference.html (Expressions: JavaScript Engine); scripting attribute added in AE 16.0. Lottie/bodymovin stores expressions as raw strings on a property (`x` key) and evaluates them with the browser's JS engine, with no engine flag. BAE decision: BAE implements only the JavaScript-engine dialect on QuickJS (ES2020+, a superset of ES2018). The persisted engine field is still read and written so AE round-trips keep it; a project marked Legacy is evaluated with the JS engine plus the compatibility shims in AE-ANI-401 to AE-ANI-403, and the UI flags it (see ui-preview-render-io.md, Project Settings).

### AE-ANI-401 · JavaScript-engine syntax rules vs Legacy ExtendScript (JavaScript 与旧版 ExtendScript 的语法差异)

- **What it does:** Defines which Legacy-ExtendScript expression syntax fails or behaves differently under the JavaScript engine; BAE must reproduce the JavaScript-engine results.
- **Exact semantics:**
  - The expression's result is the completion value of the last evaluated statement (like `eval`), not a `return` value. A top-level `return` is not required (UNVERIFIED whether a top-level `return` is accepted by AE's JS engine; lottie-web wraps the code so `$bm_rt` assignment is used instead).
  - Differences (JS engine behaviour on the left is what BAE implements):

  | Construct | Legacy ExtendScript | JavaScript engine |
  |---|---|---|
  | Expression ending in an `if` with no `else`, e.g. `var x = 50; if (time > 1) { x = 100 }` | evaluates to the last non-empty statement value (50 or 100) | error "Undefined value used in expression (could be an out of range array subscript?)" when the branch is not taken |
  | `if (c) 100 else 50;` on one line without braces | evaluates | syntax error ("Unexpected token else"); line break before `else`, braces or ternary required |
  | Expression ending with a function declaration (function called above it) | evaluates (hoisting, completion = the call) | error "Object of type found where a Number, Array, or Property is needed"; the last statement must produce a value. Note: plain ES2015+ `eval` semantics would return the call's value, so BAE must detect "last top-level statement is a function declaration" and raise this error explicitly |
  | `this` as shorthand for `thisLayer`, e.g. `this(5)(2)` | allowed | `this` is the global object; error "this is not a function" |
  | `text.sourceText[0]` (character index) | first character | needs `text.sourceText.value[0]` |
  | Property linked from a Source Text expression without `.value` | string "[object Property]" | the property's value |
  | snake_case aliases (`this_comp`, `value_at_time()`…) | allowed | not defined (ReferenceError) |
  | `.jsxbin` binary code via `eval()` | allowed | not supported |
  | `$.` object | full | only `$.build`, `$.engineName` (JS engine only) and `$.global`; everything else (`$.version`, `$.os`, `$.fileName`, `$.evalFile()`, `$.getenv()`…) is listed as unsupported |
  | `reflect.properties`, `reflect.methods`, `toSource()` | allowed | not supported |
  | ES5–ES2018 syntax (`let`, `const`, arrow functions, template literals, default params, spread, `Array.prototype.includes`, `Object.entries`, `padStart`…) | syntax error | allowed |

  - `posterizeTime(0)` freezes the value at comp time 0 in both engines since AE 16.0 (see AE-ANI-410).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Source: AE help "Syntax differences between expression engines" (legacy-and-extend-script-engine). BAE decision: QuickJS is ES2020+, so it accepts everything the AE JS engine accepts; BAE must not be *more* lenient on the three if/else cases only where that changes the numeric result. Recommended: emulate the AE JS-engine errors (an expression whose completion value is `undefined` is an error, AE-ANI-411) rather than silently using `undefined`.

### AE-ANI-402 · Legacy compatibility aliases (旧版兼容别名)

- **What it does:** Deprecated snake_case names and `this`-as-layer shorthand that only the Legacy engine understood.
- **Exact semantics:**
  - Attribute aliases (Legacy → camelCase): `this_comp`→`thisComp`, `this_layer`→`thisLayer`, `this_property`→`thisProperty`, `color_depth`→`colorDepth`, `has_parent`→`hasParent`, `in_point`→`inPoint`, `out_point`→`outPoint`, `start_time`→`startTime`, `has_video`→`hasVideo`, `has_audio`→`hasAudio`, `audio_active`→`audioActive`, `anchor_point`→`anchorPoint`, `audio_levels`→`audioLevels`, `time_remap`→`timeRemap`, `casts_shadows`→`castsShadows`, `light_transmission`→`lightTransmission`, `accepts_shadows`→`acceptsShadows`, `accepts_lights`→`acceptsLights`, `frame_duration`→`frameDuration`, `shutter_angle`→`shutterAngle`, `shutter_phase`→`shutterPhase`, `num_layers`→`numLayers`, `pixel_aspect`→`pixelAspect`, `point_of_interest`→`pointOfInterest`, `depth_of_field`→`depthOfField`, `focus_distance`→`focusDistance`, `blur_level`→`blurLevel`, `cone_angle`→`coneAngle`, `cone_feather`→`coneFeather`, `shadow_darkness`→`shadowDarkness`, `shadow_diffusion`→`shadowDiffusion`, `active_camera`→`activeCamera`.
  - Method aliases: `value_at_time`, `velocity_at_time`, `speed_at_time`, `nearest_key`, `posterize_time`, `look_at`, `seed_random`, `gauss_random`, `ease_in`, `ease_out`, `rgb_to_hsl`, `hsl_to_rgb`, `degrees_to_radians`, `radians_to_degrees`, `from_comp_to_surface`, `to_comp_vec`, `from_comp_vec`, `to_world_vec`, `from_world_vec`, `to_comp`, `from_comp`, `to_world`, `from_world`, `temporal_wiggle`, `loop_in_duration`, `loop_out_duration`, `loop_in`, `loop_out` → same-named camelCase methods.
  - Under the AE JavaScript engine none of these exist.
- **Priority:** P2 (only matters for pre-2018 projects marked Legacy)
- **Owner:** bae-anim
- **Notes:** lottie-web defines a few of these aliases (`loop_in`, `loop_out`, `radians_to_degrees`; note its `degrees_to_radians` is wired to the wrong function). BAE decision: define the aliases only when the project's engine field is Legacy ExtendScript (AE-ANI-400), so JS-engine projects keep AE's ReferenceError behaviour.

### AE-ANI-403 · Overloaded array arithmetic operators (数组算术运算符)

- **What it does:** Lets expressions use `+ - * /` directly on arrays such as `position + [100, 0]` or `value * 2`, which plain JavaScript would turn into string concatenation or NaN.
- **Exact semantics:**
  - AE's JavaScript engine pre-processes expression source before evaluation and rewrites arithmetic operators to helper calls; Legacy ExtendScript supported them natively via operator overloading. The pre-processing is NOT applied to code inside `.jsx` expression libraries or to strings passed to `eval()`; there `position + [100,100]` "will not evaluate" and `add()/sub()/mul()/div()` must be used.
  - Semantics (lottie-web `$bm_sum/$bm_sub/$bm_mul/$bm_div/$bm_neg`, used as the algorithm reference; AE matches for the common cases):
  ```
  a + b:
    number|bool (+) number|bool     -> numeric add
    either is string                -> string concatenation
    array + number                  -> copy of array with [0] += number   (only the first element!)
    number + array                  -> copy of array with [0] = number + array[0]
    array + array                   -> elementwise; length = max(len); missing element of the shorter
                                       array: result element = the element that exists (i.e. treated as 0)
  a - b: numeric strings are parseInt()-ed first; array - number subtracts from [0] only;
         number - array -> copy of array with [0] = number - array[0];
         array - array elementwise; where only one operand has element i, lottie-web copies that
         element unchanged (so [1] - [1,5] = [0,5], not [0,-5]; UNVERIFIED whether AE negates)
  a * b: number*number; array*number or number*array -> every element multiplied
         array*array -> UNVERIFIED (lottie-web returns 0; AE errors or returns NaN)
  a / b: number/number; array/number -> every element divided; number/array -> number / each element
  -a   : array -> every element negated
  a % b: plain numeric modulo (strings parsed as integers)
  ```
  - Comparison operators (`==`, `<`) on arrays are NOT overloaded: `[1,2] == [1,2]` is `false` (reference comparison).
- **Priority:** P0 (pick-whip expressions like `value + [10,0]` depend on it)
- **Owner:** bae-anim
- **Notes:** UNVERIFIED: AE's exact rule for array + scalar (first element only) is taken from lottie-web, which replicates AE; Dan Ebberts' material agrees. Implementation hint: do the rewrite once at compile time with a small JS parser pass (e.g. SWC/oxc-style AST rewrite done in Rust before handing source to QuickJS, or a QuickJS-side transform) mapping binary `+ - * /` and unary `-` to `$bm_sum` etc.; fast-path both operands numeric inline so scalar expressions do not pay a call. Do not rewrite inside string/template literals or regexes. Keep the rewrite off for `.jsx` libraries and `eval()` to match AE.

### AE-ANI-404 · Expression on a property and the enable switch (属性表达式与启用表达式开关)

- **What it does:** Any animatable property can carry one expression string plus an enable flag; when enabled, the expression result replaces the property's keyframed/static value at every time.
- **Exact semantics:**
  - Persisted per property: `expression` (UTF-16/UTF-8 string, may be empty), `expressionEnabled` (bool, default `true` when an expression is added). Scripting: `Property.expression`, `Property.expressionEnabled`, `Property.canSetExpression`, `Property.expressionError` (read-only string, empty when no error).
  - Effective value at time t:
  ```
  if prop.expression != "" and prop.expressionEnabled and canSetExpression:
      post = evaluate(prop, t)          // AE-ANI-405..409
      on error: post = pre(t)           // AE-ANI-411
  else:
      post = pre(t)                     // keyframes or static value (see keyframes items in this file)
  ```
  - Disabling the switch (启用表达式开关 shows a slash) keeps the text; the property uses its pre-expression value. An empty expression string is the same as no expression.
  - Properties that cannot take expressions: non-animatable properties (`canSetExpression == false`), e.g. property groups, layer switches, blend mode (UNVERIFIED list; per-property flag from the property tree, see project-comp-layers.md, Property tree/value types).
  - Keyframes and an expression may coexist; keyframes still define the pre-expression value.
  - Scripting set semantics (documented): assigning a valid string to `Property.expression` sets `expressionEnabled = true`; assigning a string that fails to evaluate raises a script error and sets `expressionEnabled = false`; assigning `""` sets `expressionEnabled = false` without error. `expressionEnabled` can only be set true when `canSetExpression` is true and the expression is valid.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Lottie: animatable property `{ a, k, x }` where `x` is the expression string; no enable flag (an exported disabled expression is simply not written, UNVERIFIED). UI (Alt-click stopwatch, expression field, pick whip, Graph Editor post-expression graph) is specified in ui-preview-render-io.md, Timeline. "Convert Expression to Keyframes" is in this file's keyframe-assistant items (ani-a). All edits go through a `SetExpression { prop, text, enabled }` Command in bae-model.

### AE-ANI-405 · Per-frame stateless evaluation (逐帧无状态求值)

- **What it does:** Each expression is evaluated independently for each property, each time sample; nothing computed at one frame is visible at another.
- **Exact semantics:**
  - Inputs to one evaluation: the property (`thisProperty`), its layer (`thisLayer`), its comp (`thisComp`), the project, and `time` = the comp time in seconds of the containing comp (float; not layer time, not affected by the layer's own Time Stretch or Time Remap of this layer). Time-remap/time-stretch of a *nested* comp maps the outer time to the inner comp's `time` before inner expressions run.
  - Global JavaScript state (variables, `globalThis` properties, closures) does not persist between evaluations, between properties, or between frames. An expression can reproduce history only by sampling `valueAtTime()` of earlier times (and so costs O(n) per frame).
  - Evaluation order of different properties is unspecified; the result must not depend on it.
  - The value is computed for every sample AE requests: every frame, every motion-blur sub-frame (time = sub-frame time, see ani-a Motion blur sampling), every frame-blending source frame, every audio sample block for audio properties (UNVERIFIED granularity), and any `valueAtTime()` call from another expression.
  - Time precision: `time` is a double of the requested time; in BAE `time = flicks / 705_600_000.0`.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** BAE decision: run each expression as a compiled function in a reusable QuickJS context, resetting per-evaluation globals (`time`, `value`, `thisProperty`…) before each call and freezing/clearing user-visible global writes so state cannot leak (e.g. evaluate inside a fresh function scope; detect and discard writes to `globalThis` between calls). Implementation hint: evaluation is embarrassingly parallel per (property, time) pair; use one QuickJS runtime per worker thread.

### AE-ANI-406 · Pre-expression and post-expression value (表达式前值和表达式后值)

- **What it does:** Distinguishes a property's value before its own expression runs (keyframes/static) from the value after it.
- **Exact semantics:**
  - Inside an expression, `value` (= `thisProperty.value` in the current expression's own scope) is the pre-expression value at `time`.
  - `thisProperty.valueAtTime(t)` from inside the property's own expression returns the pre-expression value at t (it cannot recurse into itself). UNVERIFIED: AE returns pre-expression value for self-references; lottie-web does the same.
  - `key(i).value`, `wiggle()`, `loopIn/Out()`, `smooth()`, `temporalWiggle()`, `velocity`, `speed` on the property itself use pre-expression values.
  - Reading ANY OTHER property (`thisComp.layer("A").position`, `effect("Slider Control")("Slider")`) returns that property's post-expression value at the requested time (AE-ANI-408).
  - UI: pre-expression values display in blue, post-expression values in red (see ui-preview-render-io.md, Timeline).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Scripting: `Property.value` / `valueAtTime(t, preExpression)` takes an explicit `preExpression` bool; the expression language has no such parameter. Lottie: same model (`value` is the keyframed value; `$bm_rt` the result).

### AE-ANI-407 · Expression result coercion (表达式结果类型转换)

- **What it does:** Converts whatever the expression returns into the property's value type and dimension.
- **Exact semantics:**
  ```
  r = completion value of the expression
  if r is a Property object        -> r = r.value                        (JS engine; Legacy only for numeric targets)
  if r is undefined                 -> error "Undefined value used in expression (could be an out of range array subscript?)"
  target kind:
    1D number (Rotation, Opacity, Slider…):
       number/boolean -> Number(r)            (true -> 1, false -> 0)
       array          -> error "Expression result must be of dimension 1, not N" (UNVERIFIED wording)
       string         -> error "Object of type String found where a Number, Array, or Property is needed" (UNVERIFIED; numeric strings may be coerced)
    N-D array (Position, Scale, Anchor, Point, Color):
       array len == N -> use
       array len >  N -> UNVERIFIED: extra elements ignored (linking a 2D layer's Position to a 3D layer's Position works)
       array len <  N -> error "Expression result must be of dimension N, not M"
       number         -> error (same message with M = 1)
    Color: 4 elements [r, g, b, a] in 0..1; 8/16 bpc projects clamp each element to [0,1]; 32 bpc keeps values outside 0..1
    Angle properties: degrees, unbounded (720 = two revolutions)
    Source Text: any value -> string (numbers via JS ToString, arrays joined with ","), or a TextDocument/TextStyle result (AE-ANI-471, AE-ANI-473)
    Path (mask/shape path): a Path object from createPath()/points() or another path property, else error
    Checkbox: number, non-zero = on (stored 0/1)
    Dropdown Menu: number rounded to an existing 1-based index (UNVERIFIED rounding; out of range -> error)
    Layer-reference (Layer Control): a Layer object or layer index (UNVERIFIED)
  NaN / ±Infinity elements -> UNVERIFIED: AE substitutes 0 / the property's limit; lottie-web passes them through. BAE: treat NaN as an error (AE-ANI-411); clamp ±Infinity to the property's range.
  then clamp to the property's hard range (e.g. Opacity 0..100; effect parameter hard min/max)
  ```
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** UNVERIFIED claims are marked inline; the exact error strings must be confirmed in AE 26. Integer-valued properties (e.g. Number of Copies in Repeater is float in AE; Posterize "Level" etc.) round per the property's own definition (see effects.md, each effect). Implementation hint: produce a typed `Value` enum on the Rust side straight from the QuickJS result (`f64` / small fixed arrays) without JSON round-tripping.

### AE-ANI-408 · Referencing other properties and sampling at other times (引用其他属性与跨时间取值)

- **What it does:** Expressions read other properties, layers, comps and footage, at the current time or any other time.
- **Exact semantics:**
  - `otherProp` (implicit `.value`) = `otherProp.valueAtTime(time)`: post-expression value at the current comp time of the *expression's* comp. If the other property is in another comp reached through `comp("Name")`, the same numeric `time` is used in that comp's timeline (no time remap applied).
  - Through `thisComp.layer("Precomp").source.layer(1)…`, the precomp's time is the remapped time (layer start time, stretch and Time Remap of the precomp layer applied, see AE-ANI-450 Layer.sourceTime). When the caller passes an explicit time to `valueAtTime(t)`, remapping is ignored and `t` is used directly in the target's comp.
  - Layer-space transform methods, `sourceRectAtTime`, `sampleImage`, `audioLevels` of other layers likewise evaluate the dependency graph at the requested time.
  - Recursion: evaluating another property's expression is a nested evaluation with its own `time`, `value`, `thisLayer`, `thisProperty`.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Cross-comp access by name (`comp("Name")`) evaluates the target comp even if it is not used in the render tree. Implementation hint: memoise each (property, time) result per render job (AE-ANI-412) because cross references fan out quickly (e.g. 100 layers each reading `thisComp.layer(index-1)` is O(n) with memoisation, O(n²) or worse without).

### AE-ANI-409 · Circular references (循环引用)

- **What it does:** Detects expressions whose evaluation depends on their own result.
- **Exact semantics:**
  - A cycle exists when, during evaluation of (P, t), a nested evaluation requests post-expression (P, t) again (directly or via other properties). Reading the *pre*-expression value of P (via `value` inside P) is not a cycle.
  - Reading (P, t') for t' ≠ t is not a cycle by itself, but unbounded recursion (e.g. `valueAtTime(time - thisComp.frameDuration)` on its own post-expression value via another layer) must terminate.
  - AE behaviour: UNVERIFIED. AE reports an error on the property (message along the lines of "circular dependency" / the expression's own value used) and uses the pre-expression value for the inner request; lottie-web has no detection.
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** BAE decision: keep an evaluation stack of (property id, time); a re-entrant request returns the property's pre-expression value at that time and marks every property on the cycle with error "Circular expression reference: A → B → A" (AE-ANI-411). Cap nested evaluation depth (e.g. 256) and treat overflow as an error, so a pathological `valueAtTime` chain cannot hang a render.

### AE-ANI-410 · posterizeTime(updatesPerSecond) (posterizeTime 降低更新频率（译名待核）)

- **What it does:** Makes the rest of the expression see a time quantised to a lower update rate, so the property changes only N times per second.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | updatesPerSecond (每秒更新次数（译名待核）) | Number | required | ≥ 0 | 1/s | 0 freezes at time 0 (AE 16.0+) |

  ```
  posterizeTime(fps):
    if fps == 0: t' = 0                       // AE 16.0 (2018)+; before 16.0 behaviour undefined
    else:        t' = floor(time * fps) / fps  // UNVERIFIED: floor of (time - 0)·fps; lottie-web uses floor(time*fps)/fps
    set the expression-global `time` = t'; every later time-dependent call that uses the default time
    (value, valueAtTime defaults, wiggle, random seeding, layer attributes) sees t'
    returns fps (Number)
  ```
  - Statements before the call see the original `time`. The pre-expression `value` is re-sampled at t' (UNVERIFIED for AE; lottie-web re-reads `value` at the posterized time).
  - AE 17.0 (Jan 2020) release note: "Any expression using posterizeTime() now calculates only once for the entire comp, not on every frame"; read as once per distinct posterized time (the literal wording cannot hold for `posterizeTime(1); random()`), see AE-ANI-412.
  - Documented idiom: `posterizeTime(1 / framesToTime(n))` holds each value for n comp frames.
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Lottie-web: `posterizeTime(framesPerSecond)` sets `time = Math.floor(time*fps)/fps` and recomputes `value`. Comp-relative: quantisation is relative to comp time 0, not the layer in-point (UNVERIFIED for AE).

### AE-ANI-411 · Expression errors and fallback value (表达式错误)

- **What it does:** When an expression throws or produces an invalid result, AE keeps rendering with a fallback value and reports the error on the property and in a banner.
- **Exact semantics:**
  - Error sources: syntax error (reported with line number), runtime exception, undefined/invalid result type, wrong dimension (AE-ANI-407), reference to a missing layer/effect/property ("Layer named 'X' is missing" style messages), circular reference (AE-ANI-409).
  - Value used while erroring: the property's pre-expression value at that time (UNVERIFIED: AE uses the pre-expression value; it does not use a last-good value).
  - Version behaviour:
    - AE ≤ 13.2: the first error showed a modal dialog and AE automatically disabled the expression (enable switch turned off).
    - AE 13.5 (CC 2015)+: the expression stays enabled and is re-evaluated every time; a warning banner (警告横幅) at the bottom of the Composition and Layer panels shows the one-line error text, with previous/next arrows for multiple errors, a Reveal Expression button and expand/collapse; a yellow warning icon next to the expression opens the full message. The banner disappears when the expression is fixed or disabled.
  - Preference: "Show Warning Banner When Project Contains Expression Errors" (Scripting & Expressions on macOS / General on Windows; default on).
  - Timeline context menu "Reveal Expression Errors" (显示表达式错误) reveals erroring properties of selected layers.
  - Scripting: `Property.expressionError` returns the message string ("" if none).
  - Renaming a layer/effect/property makes AE try to rewrite name references in expressions; failed rewrites surface as errors (see ui/project rename items; bae-motion rename Command must update references).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-ui-motion)
- **Notes:** BAE requirement: every error is attached to the property (persistent `ExpressionError { prop, time_first_seen, line, column, message }` in the EvaluatedComp side-channel, never in the saved document), deduplicated per property, and cleared when a later evaluation of that property succeeds. Headless render (aerender-style) logs the errors and continues. Error messages are i18n strings with the JS exception text appended unchanged.

### AE-ANI-412 · Expression caching and dependency tracking (表达式缓存与依赖跟踪)

- **What it does:** Avoids re-evaluating expressions whose inputs did not change, while keeping results identical to full per-frame evaluation.
- **Exact semantics:** (AE behaviour)
  - AE 17.0 (2020)+ detects expressions whose result does not change over the comp and evaluates them once; expressions using `posterizeTime()` evaluate once per posterized interval. AE's detection rules are undocumented.
  - Results must be bit-identical to evaluating every (property, time) from scratch.
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** BAE decision (algorithm): on each evaluation record (a) every property/attribute read with the time it was read at, (b) whether `time`, `random`/`noise`/`wiggle` (time-seeded), `Date`, `sampleImage`, `audioLevels`, `sourceRectAtTime` or footage data were used. Cache key = (property id, time, document revision of every dependency). An expression that read nothing time-dependent (no `time`, no time-varying inputs) is cached for all times; one that called `posterizeTime(f)` is cached per `floor(time*f)` bucket. Invalidate by document revision from bae-model change notifications. `Math.random()` and `Date.now()` mark the result uncacheable and non-deterministic (render warnings). Expose cache hit stats in the profiler.

## Global objects and attributes

### AE-ANI-413 · Global time attribute (time 当前时间（译名待核）)

- **What it does:** `time` is the composition time, in seconds, at which the expression is being evaluated.
- **Exact semantics:**
  - Type Number (double). Equals the requested time in the comp that contains the layer (comp time 0 = comp start; `thisComp.displayStartTime` is NOT added).
  - Not snapped to frames: during motion-blur sub-sampling or frame blending it is a fractional-frame time; `valueAtTime()` callers can make it any value including negative or beyond the comp duration.
  - `posterizeTime()` reassigns it for the rest of the expression (AE-ANI-410).
  - Default argument of every `…AtTime`/`t` parameter in the language is this `time`.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** BAE: `time = flicks as f64 / 705_600_000.0`; convert back with round-to-nearest flick when the expression passes a time to `valueAtTime` (keyframe lookup in flicks). Lottie-web: `time = renderedFrame / frameRate` (integer frames only).

### AE-ANI-414 · Root objects and implicit scope (根对象与隐式作用域)

- **What it does:** Provides `thisComp`, `thisLayer`, `thisProperty` and lets expressions call property and layer members without a prefix.
- **Exact semantics:**

  | Name | Type | Meaning |
  |---|---|---|
  | `thisComp` | Comp | comp containing the expression's layer |
  | `thisLayer` | Layer / Camera / Light | layer containing the property |
  | `thisProperty` | Property | the property carrying the expression |
  | `value` | Number / Array / String / Path / TextDocument | pre-expression value (AE-ANI-406) |

  - Name resolution for an unqualified identifier (AE help: the default object is the property on which the expression is written, followed by the layer containing it; source: https://helpx.adobe.com/after-effects/using/expression-language.html):
  ```
  resolve(id):
    1. local JS variables / functions declared in the expression
    2. members of thisProperty   (wiggle, loopOut, valueAtTime, key, numKeys, velocity, speed, smooth, nearestKey, propertyGroup, …)
    3. members of thisLayer      (position, scale, transform, effect, mask, content, text, index, name, inPoint, toComp, sourceRectAtTime, …)
    4. global expression functions (comp, footage, time, colorDepth, linear, ease, random, timeToFrames, add, …)
    5. JavaScript globals (Math, Date, JSON, Array, …)
  ```
  - Ambiguous names: `name`, `active`, `enabled` resolve via step 2 first (UNVERIFIED: AE returns the layer's name for bare `name`; lottie-web defines bare `name` as the layer name). Implementers must test bare `name` in AE before shipping.
  - Inside `.jsx` libraries and `eval()` strings, steps 2 and 3 are NOT available: members must be prefixed with `thisLayer.` or `thisProperty.` (e.g. `thisLayer.time`, `thisProperty.wiggle()`, `thisLayer.add()`, `thisLayer.timeToFrames()`); see AE-ANI-466.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Implementation hint: compile each expression as `function (thisProperty, thisLayer, thisComp, time, value) { with-like scope }`. QuickJS supports `with` in sloppy mode; faster: statically collect free identifiers at compile time and bind only those that exist on the property/layer as local `const`s (getter thunks for lazily computed members). This also makes dependency tracking (AE-ANI-412) cheaper.

### AE-ANI-415 · comp(name) and footage(name) lookup (按名称获取合成和素材)

- **What it does:** Global functions that return a Comp or Footage object from the project by item name.
- **Exact semantics:**

  | Method | Parameter (中文) | Type | Default | Returns |
  |---|---|---|---|---|
  | `comp(name)` | name (名称) | String | required | Comp |
  | `footage(name)` | name (名称) | String | required | Footage |

  - Match is exact and case-sensitive on the project item name; with duplicates, the first match in project order is used (UNVERIFIED order: AE project-tree order).
  - `comp()` matches only composition items; `footage()` matches footage items (files, solids, placeholders; UNVERIFIED whether solids are found) including data files (.json/.mgjson/.csv/.tsv/.txt) and `.jsx` libraries.
  - Missing name → expression error (e.g. "Couldn't find composition named 'X'", UNVERIFIED wording).
  - Comps obtained via `comp()` evaluate at the same numeric `time` (AE-ANI-408).
- **Priority:** P1
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Renaming a project item does not rewrite `comp("…")` strings (UNVERIFIED; layer/effect renames are rewritten). Lottie-web supports `comp(name)` only for comps inside the exported animation.

### AE-ANI-416 · colorDepth and thisProject (颜色深度与当前项目)

- **What it does:** Exposes project-level settings to expressions.
- **Exact semantics:**

  | Attribute | Type | Values | Since |
  |---|---|---|---|
  | `colorDepth` | Number | 8, 16 or 32 (project bits per channel) | long-standing (first version UNVERIFIED) |
  | `thisProject.bitsPerChannel` | Number | 8, 16, 32 (same as scripting `app.project.bitsPerChannel`) | AE 15.1 (Apr 2018) |
  | `thisProject.fullPath` | String | absolute platform path incl. file name; `""` if never saved | AE 15.1 |
  | `thisProject.linearBlending` | Boolean | state of "Blend Colors Using 1.0 Gamma" | AE 15.1 |

  - Changing the project setting invalidates every expression that read these (AE-ANI-412).
- **Priority:** P1 (`colorDepth`), P2 (`thisProject`)
- **Owner:** bae-anim (+ bae-color)
- **Notes:** Color management settings are specified in compositing-3d-color.md, Color management. BAE: `fullPath` returns the `.bae`/`.baez` path; it is privacy-sensitive in shared projects, so collaborative sessions may return the path as seen by the evaluating machine.

## Time conversion methods

### AE-ANI-417 · timeToFrames and framesToTime (时间与帧的转换)

- **What it does:** Converts seconds to an integer frame count and frames back to seconds.
- **Exact semantics:**

  | Method / Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | `timeToFrames(t, fps, isDuration)` → Number | | | | | |
  | t (时间) | Number | `time + thisComp.displayStartTime` | any | s | |
  | fps (帧速率) | Number | `1.0 / thisComp.frameDuration` | > 0 | frames/s | |
  | isDuration (是否为持续时间) | Boolean | `false` | — | — | rounding mode |
  | `framesToTime(frames, fps)` → Number | | | | | |
  | frames (帧数) | Number | required | any, need not be integer | frames | |
  | fps (帧速率) | Number | `1.0 / thisComp.frameDuration` | > 0 | frames/s | |

  ```
  timeToFrames(t, fps, isDuration):
     x = t * fps
     if !isDuration: return floor(x)                 // toward −∞ (absolute time)
     else:           return sign(x) * ceil(abs(x))   // away from zero (duration)
  framesToTime(frames, fps) = frames / fps           // no rounding
  ```
  - Note the default `t` includes `displayStartTime`, so on a comp whose Start Timecode is 00:00:10:00 at 30 fps, `timeToFrames()` at comp time 0 returns 300.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** UNVERIFIED: AE's floating-point tolerance. Pure `floor(t*fps)` misrounds e.g. t = 0.7, fps = 30 (`20.999999999999996` → 20); AE is believed to round to the nearest frame when within a tiny epsilon. BAE decision: compute in flicks when `t` is a flick-exact time and fps is a standard rate (exact rational), otherwise use `floor(x + 1e-9)` / `ceil(abs(x) - 1e-9)`. Lottie-web: `timeToFrames(t = time, fps = frameRate)` returns the unrounded `t * fps` and ignores `displayStartTime` and `isDuration` (not AE-exact); `framesToTime(fr, fps)` = `fr / fps`.

### AE-ANI-418 · Timecode string conversions (时间码字符串转换)

- **What it does:** Formats a time as timecode, NTSC timecode, feet+frames, or the project's current time display format.
- **Exact semantics:**

  | Method | Parameters (中文) in order, with defaults | Returns |
  |---|---|---|
  | `timeToTimecode(t, timecodeBase, isDuration)` | t (时间) = `time + thisComp.displayStartTime`; timecodeBase (时间码基准) = 30; isDuration = false | String |
  | `timeToNTSCTimecode(t, ntscDropFrame, isDuration)` | t = `time + thisComp.displayStartTime`; ntscDropFrame (NTSC 丢帧) = false; isDuration = false | String |
  | `timeToFeetAndFrames(t, fps, framesPerFoot, isDuration)` | t = `time + thisComp.displayStartTime`; fps = `1.0 / thisComp.frameDuration`; framesPerFoot (每英尺帧数) = 16 (35mm); isDuration = false | String |
  | `timeToCurrentFormat(t, fps, isDuration, ntscDropFrame)` | t = `time + thisComp.displayStartTime`; fps = `1.0 / thisComp.frameDuration`; isDuration = false; ntscDropFrame = `thisComp.ntscDropFrame` (argument added AE 10.5 CS5.5) | String |

  ```
  // frame count first, using the rounding of AE-ANI-417
  timeToTimecode(t, base=30, dur=false):
      n = round_mode(t * base, dur);  neg = n < 0;  n = abs(n)
      ff = n mod base; s = n div base; ss = s mod 60; mm = (s div 60) mod 60; hh = s div 3600
      return (neg ? "-" : "") + pad2(hh) ":" pad2(mm) ":" pad2(ss) ":" pad2(ff)
      // pad width of ff grows with base digits (e.g. base 100 -> 3 digits)  UNVERIFIED
  timeToNTSCTimecode(t, drop=false, dur=false):
      n = round_mode(t * 30000/1001, dur)            // NTSC frame count; UNVERIFIED that AE uses 29.97 exactly
      if !drop: format n with base 30 as above, separators ":"
      else:     // SMPTE drop-frame: skip labels 00 and 01 at each minute except every 10th minute
          D = n div 17982; M = n mod 17982
          n_label = n + 18*D + 2*max(0, (M - 2) div 1798)
          format n_label with base 30; separator before frames is ";" (UNVERIFIED: AE uses ";" for all three separators)
  timeToFeetAndFrames(t, fps, fpf=16, dur=false):
      n = round_mode(t * fps, dur); feet = n div fpf; fr = n mod fpf
      return feet + "+" + pad2(fr)                    // e.g. "12+08"; UNVERIFIED padding of feet (AE UI shows "0000+00")
  timeToCurrentFormat(...): dispatch on Project Settings > Time Display Style:
      Timecode  -> timecode at the comp's timecode base (drop-frame per ntscDropFrame)
      Frames    -> decimal frame number (+ project "Start numbering frames at" offset 0 or 1, UNVERIFIED)
      Feet + Frames -> as timeToFeetAndFrames with the project film setting (16 or 40 frames per foot)
  ```
  - Negative times produce a leading "-" on the absolute value (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Typical use: Source Text `timeToCurrentFormat()` for burn-in timecode. Project time-display settings are specified in project-comp-layers.md, Project settings. Implementation hint: share the formatter with the UI time display (bae-ui-motion) so expression output and the timeline read the same.

## Vector math and other math

### AE-ANI-419 · add, sub, mul, div vector functions (矢量加减乘除)

- **What it does:** Global functions for component-wise vector arithmetic that work in every context (including `.jsx` libraries and `eval()` where operator overloading is unavailable).
- **Exact semantics:**

  | Method | Parameters (中文) | Returns |
  |---|---|---|
  | `add(vec1, vec2)` | vec1 (矢量 1), vec2 (矢量 2): Array or Number | Array (or Number if both numbers) |
  | `sub(vec1, vec2)` | vec1, vec2: Array or Number | Array / Number |
  | `mul(vec, amount)` | vec (矢量): Array or Number; amount (数量): Number | Array / Number |
  | `div(vec, amount)` | vec: Array or Number; amount: Number | Array / Number |

  ```
  // dimension leniency (documented): result has the dimension of the LONGER input,
  // missing elements of the shorter input are treated as 0
  add([10, 20], [1, 2, 3]) == [11, 22, 3]
  sub(a, b)[i] = (a[i] ?? 0) - (b[i] ?? 0)
  mul(v, k)[i] = v[i] * k ;  div(v, k)[i] = v[i] / k      // k == 0 -> ±Infinity / NaN per IEEE
  numbers where arrays are expected: the reference documents Array parameters only; accepting a Number
  (treated as a 1-element vector) is UNVERIFIED for AE (lottie-web accepts numbers)
  ```
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Differs from the overloaded `+` operator in AE-ANI-403 only for array+scalar (operator touches element 0; `add([1,2], 5)` is UNVERIFIED, likely `[6, 2]` treating 5 as `[5]`). The help page states these functions need the `thisLayer.` prefix inside `.jsx` libraries / `eval()`. Implementation hint: implement natively in Rust and register as QuickJS C functions operating on `Float64Array`-like fast paths for 2/3/4-element arrays.

### AE-ANI-420 · clamp(value, limit1, limit2) (钳制)

- **What it does:** Constrains a number or each component of an array between two limits.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | value (值) | Number or Array | required | |
  | limit1 (限制 1) | Number or Array | required | lower limit (order-independent) |
  | limit2 (限制 2) | Number or Array | required | upper limit |

  ```
  lo = min(limit1, limit2); hi = max(limit1, limit2)        // per component
  clamp(v)[i] = min(max(v[i], lo[i]), hi[i])
  scalar limit with array value -> broadcast to every component (UNVERIFIED for AE)
  array limit shorter than value -> missing limit components leave the value unclamped (UNVERIFIED)
  ```
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** lottie-web swaps limits when `min > max` and supports numbers only. NaN value returns NaN.

### AE-ANI-421 · dot, cross, length, normalize (点积、叉积、长度与归一化)

- **What it does:** Basic vector geometry.
- **Exact semantics:**

  | Method | Parameters (中文) | Returns |
  |---|---|---|
  | `dot(vec1, vec2)` | vec1, vec2 (矢量): Array | Number = Σ vec1[i]·vec2[i] over max length, missing = 0 |
  | `cross(vec1, vec2)` | vec1, vec2: Array of 2 or 3 | Array (documented as "2- or 3-dimensional"): `[y1z2−z1y2, z1x2−x1z2, x1y2−y1x2]` with missing z = 0; UNVERIFIED: for two 2D inputs AE returns a 2-element array, presumably the first two components of that product (= [0, 0]), versus the full 3-element vector |
  | `length(vec)` | vec (矢量): Array or Number | Number = √(Σ vec[i]²); a Number argument returns `abs(vec)` |
  | `length(point1, point2)` | point1 (点 1), point2 (点 2): Array | Number = `length(sub(point1, point2))` (distance) |
  | `normalize(vec)` | vec: Array | Array = `div(vec, length(vec))`; zero vector → NaN elements (UNVERIFIED; lottie-web gives NaN) |
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** lottie-web `length(a, b)` uses `min(len)` elements, AE's documented leniency implies max length with zero padding; BAE follows AE (max length). Typical use: `length(position, pointOfInterest)` on a camera's Focus Distance.

### AE-ANI-422 · lookAt(fromPoint, atPoint) (lookAt 朝向)

- **What it does:** Returns Orientation angles (degrees) that make a layer's +Z axis point from `fromPoint` toward `atPoint`.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Units | Notes |
  |---|---|---|---|---|
  | fromPoint (起点) | Array[3] | required | px, world space | location of the layer to orient |
  | atPoint (目标点) | Array[3] | required | px, world space | point to look at |

  ```
  d = atPoint - fromPoint
  xRot = -atan2(d.y, d.z)                          // degrees
  yRot =  atan2(d.x, sqrt(d.y*d.y + d.z*d.z))      // degrees
  return [xRot, yRot, 0]                           // for the Orientation property [x, y, z]
  ```
  - d = 0 → `[-0, 0, 0]` (atan2(0,0) = 0).
  - The result assumes AE's rotation order for Orientation (X then Y then Z applied as specified in this file's transform-matrix items, ani-a); AE's coordinate system is Y-down, Z into the screen.
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Algorithm from lottie-web `lookAt` (algorithm reference; AE's own code is undocumented, UNVERIFIED that AE produces identical angles including the sign convention). Docs tip: on cameras turn auto-orient off (Auto-Orient Towards Point of Interest would otherwise add its own rotation; see ani-a Auto-orient).

### AE-ANI-423 · degreesToRadians, radiansToDegrees and JavaScript Math (角度弧度转换与 Math 对象)

- **What it does:** Angle unit conversion plus the standard JavaScript `Math` object.
- **Exact semantics:**
  - `degreesToRadians(degrees)` = `degrees * π / 180`; `radiansToDegrees(radians)` = `radians * 180 / π`. Numbers only (arrays UNVERIFIED: not supported).
  - `Math` is the standard ECMAScript object (`Math.sin`, `Math.atan2`, `Math.PI`, `Math.hypot`, …). AE angle properties use degrees while `Math` trig uses radians.
  - `Math.random()` is NOT seeded: it is non-deterministic per evaluation (unlike `random()`, AE-ANI-424).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** lottie-web bug: its `degrees_to_radians` alias points at `radiansToDegrees`; BAE must not copy it. BAE decision: `Math.random()` stays non-deterministic for AE parity, but the evaluator flags the expression as uncacheable (AE-ANI-412) and the render log warns once per property because network/multi-machine renders will differ.

## Random numbers and noise

### AE-ANI-424 · Random seed model and seedRandom(offset, timeless) (随机种子与 seedRandom)

- **What it does:** Makes `random()`, `gaussRandom()` and `wiggle()` deterministic: the same layer, property, time and offset always produce the same numbers.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Notes |
  |---|---|---|---|---|
  | offset (偏移) | Number | required | any | also controls the initial phase of `wiggle()` |
  | timeless (与时间无关) | Boolean | `false` | — | `true` = current time is not part of the seed |

  ```
  at the start of every evaluation:
      seed_state = Seed(layerUniqueId, propertyIdentity, time, offset = 0, timeless = false)
  seedRandom(offset, timeless = false):          // returns undefined
      seed_state = Seed(layerUniqueId, propertyIdentity, timeless ? <none> : time, offset)
  each random()/gaussRandom() call draws the NEXT number of the sequence from seed_state,
  so successive calls in one evaluation differ, and the k-th call at the same
  (layer, property, time, offset) is always the same number
  ```
  - `layerUniqueId` is the layer's persistent ID (not its index; reordering layers does not change results). AE CS6/CC (11.0/12.0) changed `random()` to decorrelate layers with nearby IDs; `wiggle()` was not affected.
  - Duplicated layers get new IDs and therefore different random sequences.
  - The JavaScript engine was fixed in AE 16.1 to produce the same random numbers as the Legacy engine.
  - AE's hash and generator are undocumented: UNVERIFIED; exact AE random values cannot be reproduced from public sources.
- **Priority:** P0 (determinism across frames, previews and render machines is mandatory)
- **Owner:** bae-anim
- **Notes:** BAE decision: `Seed = SplitMix64(hash64(layer.uuid) ⊕ hash64(property path matchNames + index) ⊕ hash64(timeless ? 0 : time_in_flicks) ⊕ hash64(f64 bits of offset))`; generator = PCG32 or xoshiro128** seeded from it; `random()` uses the top 53 bits → [0,1). Document in the project format that BAE's sequences differ from AE's (an imported AE project with `random()` will look different but behave the same statistically). Lottie-web seeds `BMMath.seedrandom(randSeed + seed)` with a per-load random base, so lottie-web is NOT deterministic across loads: do not copy.

### AE-ANI-425 · random() overloads (随机数 random)

- **What it does:** Uniformly distributed random numbers or arrays, from the seeded sequence.
- **Exact semantics:**

  | Overload | Parameters (中文) | Default | Returns |
  |---|---|---|---|
  | `random()` | — | — | Number in [0, 1) |
  | `random(maxValOrArray)` | maxValOrArray (最大值或数组): Number/Array | 1 | Number in [0, max) or Array (same dimension, each component in [0, max[i])) |
  | `random(minValOrArray, maxValOrArray)` | minValOrArray (最小值或数组), maxValOrArray | required | Number in [min, max) or Array with the dimension of the LONGER argument; the shorter array is padded with 0 |

  ```
  number: min + u * (max - min)            // u = next uniform in [0,1); max < min allowed (range reversed)
  array : r[i] = min[i] + u_i * (max[i] - min[i])
          u_i: an independent draw per component (UNVERIFIED for AE; lottie-web reuses ONE draw for all components,
               which would make random([100,100]) lie on the diagonal; AE results visibly do not)
  ```
  - Example: `random([100, 200], [300, 400])` → x in 100–300, y in 200–400.
  - Upper bound inclusive/exclusive: UNVERIFIED (practically irrelevant; BAE uses exclusive).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Typical idiom `seedRandom(index, true); random(50, 100)` gives a per-layer constant. `Math.random()` is unrelated (AE-ANI-423).

### AE-ANI-426 · gaussRandom() overloads (高斯随机数 gaussRandom)

- **What it does:** Normally distributed random numbers where about 90% of results fall in the given range.
- **Exact semantics:**

  | Overload | Parameters (中文) | Default | Returns |
  |---|---|---|---|
  | `gaussRandom(maxValOrArray)` | maxValOrArray (最大值或数组) | 1 | Number/Array, ~90% in [0, max] |
  | `gaussRandom(minValOrArray, maxValOrArray)` | minValOrArray, maxValOrArray | required | Number/Array with the dimension of the longer argument, ~90% in [min, max] per component |

  ```
  mean  = (min + max) / 2
  sigma = (max - min) / 2 / 1.6448536      // 1.6449 = z for a two-sided 90% interval  (derived from the documented 90%; UNVERIFIED exact AE constant)
  r     = mean + sigma * N(0,1)            // unclamped: ~10% lies outside [min, max]
  N(0,1) from the seeded uniform sequence via Box–Muller (BAE choice; AE's transform UNVERIFIED)
  ```
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Same seeding as AE-ANI-424. Arrays: independent normal draw per component (UNVERIFIED for AE).

### AE-ANI-427 · noise(valOrArray) (柏林噪声 noise)

- **What it does:** Smooth deterministic Perlin noise in −1…1 of a 1D, 2D or 3D input; unlike `random()` it is not seeded by layer or time.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Notes |
  |---|---|---|---|---|
  | valOrArray (值或数组) | Number or Array[2|3] | required | any | coordinates in noise space |

  - Returns Number in [−1, 1]. Same input → same output everywhere (no layer/property/time seeding; `seedRandom` has no effect, UNVERIFIED).
  - Nearby inputs give nearby outputs (continuous, C1). Lattice period 1 (classic Perlin): integer inputs return 0 (UNVERIFIED for AE).
  - Example `rotation + 360*noise(time)`.
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** AE's permutation table and gradient set are undocumented (UNVERIFIED). BAE decision: Ken Perlin's 2002 "improved noise" with the reference permutation, 1D/2D inputs evaluated as 3D with missing coordinates = 0, output scaled to [−1, 1]. lottie-web does not implement `noise()`. Shares no code path with `wiggle()` (AE-ANI-435), which uses its own fractal noise.

## Interpolation methods

### AE-ANI-428 · linear() (线性插值 linear)

- **What it does:** Maps a driver value from an input range to an output range linearly, clamped at both ends.
- **Exact semantics:**

  | Overload | Parameters (中文) | Returns |
  |---|---|---|
  | `linear(t, tMin, tMax, value1, value2)` | t (驱动值), tMin (最小驱动值), tMax (最大驱动值): Number; value1 (起始值), value2 (结束值): Number or Array | Number / Array |
  | `linear(t, value1, value2)` | t: Number; value1, value2: Number or Array | same, with tMin = 0, tMax = 1 |

  ```
  if tMax < tMin: swap(tMin, tMax)            // lottie-web; values NOT swapped, so linear(100, 100, 0, 0, 10) = 10
                                              // UNVERIFIED for AE (alternative: true reversed mapping returning 0)
  if t <= tMin: return value1
  if t >= tMax: return value2
  u = (t - tMin) / (tMax - tMin)              // tMin == tMax is caught by the two returns above
  return value1 + (value2 - value1) * u       // per component for arrays
  arrays of different length: result has the length of value1 (lottie-web); AE UNVERIFIED (likely longer, zero-padded)
  ```
  - Overload detection: 3 arguments → second form; 5 arguments → first form. 4 arguments: UNVERIFIED for AE; lottie-web treats any call with `value1` or `value2` undefined as the 3-argument form (`linear(t, a, b, c)` = `linear(t, a, b)`).
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Typical uses: `linear(time, 0, 6, 20, 80)` (ramp), `linear(value, 0, 100, 20, 80)` (range remap). The `t <= tMin` / `t >= tMax` order means values are returned exactly (bit-identical) outside the range.

### AE-ANI-429 · ease(), easeIn(), easeOut() (缓动插值 ease、easeIn、easeOut)

- **What it does:** Like `linear()` but with zero velocity at both ends (`ease`), only at the tMin end (`easeIn`), or only at the tMax end (`easeOut`).
- **Exact semantics:**
  - Overloads identical to `linear` for each function: `f(t, tMin, tMax, value1, value2)` and `f(t, value1, value2)` (tMin = 0, tMax = 1).
  ```
  u = (t - tMin) / (tMax - tMin)      // first overload; second overload: u = t
  u = clamp(u, 0, 1)
  m = curve(u)
  return value1 + (value2 - value1) * m           // per component
  curve = cubic-bezier easing (x1, y1, x2, y2), solved for y at x = u:
     ease    : (0.33,  0,     0.667, 1)      // tangent 0 at both ends
     easeIn  : (0.333, 0,     0.833, 0.833)  // tangent 0 at tMin, linear (slope 1) at tMax
     easeOut : (0.167, 0.167, 0.667, 1)      // linear at tMin, tangent 0 at tMax
  ```
  - Note the naming: AE's `easeIn` eases at the *start* (tMin side) — the opposite of the keyframe "Easy Ease In" wording, which eases into the incoming keyframe.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** The bezier constants are lottie-web's (`BezierFactory.getBezierEasing`), chosen to match AE; AE's exact curve is undocumented — UNVERIFIED (it may be a cubic Hermite/smoothstep-like curve; values differ by < 1% between candidates). Verify by sampling AE at u = 0.1…0.9 before M1. lottie-web clamps u but, for tMax == tMin, divides by zero (NaN); BAE: `tMax == tMin` → return value1 if t <= tMin else value2 (same boundary rule as `linear`, whose documented contract is value1 for t <= tMin and value2 for t >= tMax). Implementation hint: precompute a 256-entry lookup + one Newton step like the keyframe evaluator (see ani-a temporal ease).

## Color conversion methods

### AE-ANI-430 · rgbToHsl and hslToRgb (RGB 与 HSL 转换)

- **What it does:** Converts a 4-element RGBA color array to HSLA and back; all components normalised 0–1.
- **Exact semantics:**

  | Method | Parameter (中文) | Type | Returns |
  |---|---|---|---|
  | `rgbToHsl(rgbaArray)` | rgbaArray (RGBA 数组) | Array[4], 0–1 | Array[4] `[h, s, l, a]`, h in [0,1) (fraction of 360°) |
  | `hslToRgb(hslaArray)` | hslaArray (HSLA 数组) | Array[4], 0–1 | Array[4] `[r, g, b, a]` |

  ```
  rgbToHsl([r,g,b,a]):
     mx = max(r,g,b); mn = min(r,g,b); l = (mx+mn)/2
     if mx == mn: h = s = 0
     else: d = mx-mn
           s = l > 0.5 ? d/(2-mx-mn) : d/(mx+mn)
           h = mx==r ? (g-b)/d + (g<b ? 6 : 0) : mx==g ? (b-r)/d + 2 : (r-g)/d + 4
           h /= 6
     return [h, s, l, a]                 // alpha passed through
  hslToRgb([h,s,l,a]):
     if s == 0: r = g = b = l
     else: q = l < 0.5 ? l*(1+s) : l+s-l*s;  p = 2*l - q
           r = hue2rgb(p,q,h+1/3); g = hue2rgb(p,q,h); b = hue2rgb(p,q,h-1/3)
     hue2rgb(p,q,t): t<0 -> t+=1; t>1 -> t-=1
           t<1/6: p+(q-p)*6t ; t<1/2: q ; t<2/3: p+(q-p)*(2/3-t)*6 ; else p
  ```
  - Values outside 0–1 (32 bpc) pass through the formulas unclamped (UNVERIFIED for AE; h outside [0,1] wraps only by one period in `hue2rgb`).
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Formulas from lottie-web (standard HSL); AE's are undocumented but results match for in-gamut colors (UNVERIFIED at edge cases). Docs example `rgbToHsl(effect("Change Color")("Color To Change"))` (the docs page has a typo `rgbToHsl.effect`). Conversions are in the project's working space numbers, no color management applied (see compositing-3d-color.md, Color management).

### AE-ANI-431 · hexToRgb(hexString) (十六进制转 RGB)

- **What it does:** Parses a hex color string into an RGBA array 0–1. Added in AE 16.0 (2018).
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | hexString (十六进制字符串) | String | required | 6 digits (RGB) or 8 digits (RGBA); optional leading `#`, `0x`, `0X` ignored; characters beyond 8 digits ignored |

  ```
  s = strip one leading "#" | "0x" | "0X"; s = s[0..8]
  r = int(s[0..2],16)/255; g = int(s[2..4],16)/255; b = int(s[4..6],16)/255
  a = len(s) >= 8 ? int(s[6..8],16)/255 : 1.0
  return [r, g, b, a]
  ```
  - Case-insensitive A–F. Invalid characters / fewer than 6 digits: UNVERIFIED (BAE: expression error). 3-digit shorthand ("#F0F") is not documented (UNVERIFIED; BAE: error).
  - Examples returning `[1, 0, 1, 1]`: `"FF00FF"`, `"#FF00FF"`, `"0xFF00FF"`, `"0XFF00FFFF"`.
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Mostly used with data-driven JSON/CSV (AE-ANI-464). No `rgbToHex` exists in AE.

## Property object: values, velocity, wiggle, smoothing

### AE-ANI-432 · value and valueAtTime(t) (属性值与 valueAtTime)

- **What it does:** Returns a property's value at the current time or at any comp time.
- **Exact semantics:**

  | Member | Parameter (中文) | Type | Default | Returns |
  |---|---|---|---|---|
  | `prop.value` | — | — | — | value at `time` |
  | `prop.valueAtTime(t)` | t (时间) | Number, comp seconds | required | value at t |

  - On the expression's own property: pre-expression value (AE-ANI-406). On any other property: post-expression value (AE-ANI-408).
  - `t` may be fractional, negative, or past the comp/layer end: keyframed values hold the first/last keyframe value outside the keyed range; expressions are evaluated at t as-is (no clamping to the layer's in/out points).
  - Time-remapped layers: `t` is the comp time of the property's own comp; the layer's Time Remap/Stretch is applied by the evaluator, not by the caller (see ani-a, Time remap).
  - Returns Number, Array, String (Source Text: string in JS engine via implicit `.value`), Path object (AE-ANI-444) or TextDocument.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Docs example `valueAtTime(random(4))` (random keyframe value each frame). Implementation hint: `valueAtTime` with a time outside the current frame defeats per-frame caches; memoise (property, flick time) in the job-wide cache (AE-ANI-412). Lottie-web: `valueAtTime(t)` reads keyframes only (pre-expression) for every property.

### AE-ANI-433 · velocity and velocityAtTime(t) (速度矢量 velocity)

- **What it does:** Temporal derivative of a property's value, in property units per second, with the property's dimension.
- **Exact semantics:**

  | Member | Parameter (中文) | Type | Default | Returns |
  |---|---|---|---|---|
  | `prop.velocity` | — | — | — | Number/Array = `velocityAtTime(time)` |
  | `prop.velocityAtTime(t)` | t (时间) | Number, comp seconds | required | Number/Array |

  ```
  keyframed property, own expression (pre-expression):
      v(t) = d/dt value(t), analytic from the keyframe curve (temporal ease, spatial bezier for Position):
      spatial: v(t) = path tangent dP/ds * ds/dt  (vector, px/s)
      at exactly a keyframe time: UNVERIFIED which side; BAE uses the outgoing segment (incoming at the last key)
      hold interpolation or outside the keyed range: 0 (all components)
  static (unkeyed) property: 0
  other property with an expression: UNVERIFIED (AE uses keyframe velocity of that property, ignoring its expression);
      BAE decision: numeric backward difference of post-expression values, (v(t) - v(t - h)) / h, h = 1 ms (lottie-web uses h = 0.001 s)
  ```
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** The keyframe velocity math (influence/speed per keyframe) is specified in this file's ease-model items (ani-a); share that code. Lottie-web: backward difference with Δ = −0.001 s on keyframe values.

### AE-ANI-434 · speed and speedAtTime(t) (速率 speed)

- **What it does:** Scalar, non-negative magnitude of the property's rate of change; documented for spatial properties (Position, Anchor Point, effect points).
- **Exact semantics:**

  | Member | Parameter (中文) | Type | Default | Returns |
  |---|---|---|---|---|
  | `prop.speed` | — | — | — | Number = `speedAtTime(time)` |
  | `prop.speedAtTime(t)` | t (时间) | Number, comp seconds | required | Number ≥ 0 |

  ```
  speedAtTime(t) = length(velocityAtTime(t))          // px/s for spatial properties
  1D property: UNVERIFIED (AE returns abs(velocity) or 0); BAE returns abs(velocityAtTime(t))
  ```
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Equals the Speed graph in the Graph Editor (see ani-a, Graph editor). Lottie-web returns `100 * |Δv|` with Δ = 0.01 s for arrays and 0 for scalars (not AE-exact).

### AE-ANI-435 · wiggle(freq, amp, octaves, amp_mult, t) (摇摆 wiggle)

- **What it does:** Adds smooth, deterministic fractal noise to the property's pre-expression value.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | freq (频率) | Number | required | ≥ 0 | wiggles per second | |
  | amp (振幅) | Number | required | any (negative = phase-inverted) | property units | |
  | octaves (倍频) | Number | 1 | ≥ 0; non-integer UNVERIFIED | — | number of noise layers summed |
  | amp_mult (振幅倍数) | Number | 0.5 | any (typ. 0–1) | — | amplitude factor per octave |
  | t (时间) | Number | `time` | any | comp seconds | base time; value is sampled AND noised at t |

  ```
  wiggle(freq, amp, octaves=1, amp_mult=0.5, t=time):
      base = thisProperty.valueAtTime(t)          // pre-expression
      for each component i of base (independent noise per dimension):
          n = 0; f = freq; a = amp
          for k in 0 .. octaves-1:
              n += a * N_i(f * t + phase(seed, i, k))     // N: smooth 1D noise in ~[-1, 1]
              f *= 2; a *= amp_mult
          out[i] = base[i] + n
      seed = (layer unique id, property identity, seedRandom offset) — NOT time (AE-ANI-424)
  ```
  - Example from the docs: `position.wiggle(5, 20, 3, 0.5)` → 5 wiggles/s of ~20 px, plus 10/s of ~10 px and 20/s of ~5 px (so frequency doubles per octave).
  - Each dimension is independent: to wiggle uniformly use `w = wiggle(5,10); [w[0], w[0]]`.
  - `freq = 0` → constant offset; `amp = 0` → `base`.
  - Works on any numeric/array property incl. colors (all 4 components, including alpha).
  - Not affected by the CS6 `random()` change; changing `seedRandom(offset)` before calling `wiggle` changes its pattern.
  - Distribution: "average size of about amp" per docs; exact AE noise basis, amplitude normalisation and per-octave phase are undocumented (UNVERIFIED).
- **Priority:** P0 (most used expression in AE)
- **Owner:** bae-anim
- **Notes:** BAE decision: N_i = 1D gradient (Perlin) noise with a per-(seed, component, octave) permutation offset, normalised so that its standard deviation ≈ amp·0.5 and |N| ≤ 1 (UNVERIFIED match to AE's visual amplitude; validate against AE renders before M1). Results must be continuous in t (C1) and identical on every machine. Lottie-web's `wiggle` is a placeholder (frequency hard-coded to 5, `Math.random`-based, not smooth) — do not use as a reference. Looping wiggle idiom (Dan Ebberts): crossfade `wiggle(f,a,o,m,t)` with `wiggle(f,a,o,m,t-loopDur)`.

### AE-ANI-436 · temporalWiggle(freq, amp, octaves, amp_mult, t) (时间摇摆 temporalWiggle)

- **What it does:** Samples the property at a wiggled *time* instead of adding noise to the value, so the output always stays on the property's own animation curve.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Units | Notes |
  |---|---|---|---|---|
  | freq (频率) | Number | required | wiggles per second | |
  | amp (振幅) | Number | required | UNVERIFIED: seconds of time offset | the reference says "in units of the property to which it is applied" (text copied from wiggle); the documented example `scale.temporalWiggle(5, 0.2)` only makes sense as a time amount |
  | octaves (倍频) | Number | 1 | — | |
  | amp_mult (振幅倍数) | Number | 0.5 | — | |
  | t (时间) | Number | `time` | comp seconds | base time |

  ```
  temporalWiggle(...) = thisProperty.valueAtTime(t + W(t))     // W = the scalar fractal noise of AE-ANI-435 (one channel)
  ```
  - Meaningful only on animated properties; on a static property it returns the static value.
- **Priority:** P2
- **Owner:** bae-anim
- **Notes:** Shares the noise function with `wiggle()`. Not implemented by lottie-web.

### AE-ANI-437 · smooth(width, samples, t) (平滑 smooth)

- **What it does:** Box-filters the property's value over a time window around t.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | width (宽度) | Number | 0.2 | ≥ 0 | seconds (total window) | |
  | samples (采样数) | Number | 5 | integer ≥ 1 | — | odd values include t itself |
  | t (时间) | Number | `time` | any | comp seconds | centre of the window |

  ```
  smooth(width=0.2, samples=5, t=time):
      n = floor(samples)
      if n <= 1: return valueAtTime(t)
      t0 = t - width/2; step = width / (n - 1)
      return (Σ_{i=0}^{n-1} valueAtTime(t0 + i*step)) / n     // per component; own property -> pre-expression values
  ```
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Window placement (centred, endpoints included) follows lottie-web, whose default width is 0.4 (differs from AE's documented 0.2; BAE uses 0.2). UNVERIFIED: AE's exact sample positions. Implementation hint: each sample is a `valueAtTime` → reuse the per-job cache.

## Property object: loops and keyframe access

### AE-ANI-438 · loopOut() and loopOutDuration() (循环输出 loopOut)

- **What it does:** After the last keyframe, repeats or extrapolates a segment of the keyframed animation until the layer's out point (and beyond).
- **Exact semantics:**

  | Method / Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | `loopOut(type, numKeyframes)` | | | | | |
  | type (类型) | String | `"cycle"` | `"cycle"`, `"pingpong"`, `"offset"`, `"continue"` | — | case-insensitive in lottie-web; AE UNVERIFIED |
  | numKeyframes (关键帧数) | Number | 0 | 0…numKeys−1 | keyframes | segment = last `numKeyframes`+1 keyframes; 0 or too large → all keyframes |
  | `loopOutDuration(type, duration)` | | | | | |
  | type (类型) | String | `"cycle"` | as above | — | |
  | duration (持续时间) | Number | 0 | ≥ 0 | comp seconds | segment = [lastKey − duration, lastKey]; 0 → [layer inPoint, lastKey] |

  ```
  K = keyframes (comp times), n = numKeys, e = K[n].time
  if n < 2 (and not duration form) or t <= e: return value(t)          // pre-expression, no looping
  segment start s:
      loopOut:          m = (numKeyframes <= 0 || numKeyframes > n-1) ? n-1 : floor(numKeyframes)
                        s = K[n - m].time
      loopOutDuration:  s = duration > 0 ? e - duration : layer.inPoint   // too-large duration clipped to the maximum (UNVERIFIED: clip to e - layer.inPoint)
  D = e - s; if D <= 0: return value(e)                                  // "too small -> constant loop"
  p = floor((t - s) / D); φ = (t - s) - p*D                              // φ in [0, D)
  cycle:    v(s + φ)
  pingpong: p odd ? v(e - φ) : v(s + φ)
  offset:   v(s + φ) + p * (v(e) - v(s))                                 // per component
  continue: v(e) + vel_in(e) * (t - e)        // vel_in = incoming keyframe velocity at the last key; numKeyframes/duration ignored
  v(x) = pre-expression keyframed value at comp time x
  ```
  - At t = e + p·D exactly, cycle returns v(s) (first value of the segment).
  - Documented exceptions (properties not expressible as simple numbers in the Timeline): Source Text, path shape properties (mask/shape path) and the Levels effect Histogram. Documented clipping: keyframe counts/durations that are too large are clipped to the maximum; too small gives a constant loop. BAE decision: may additionally support `cycle`/`pingpong` on paths with equal vertex count (an extension, AE errors or does not loop).
  - With 0 keyframes returns `value`; with 1 keyframe returns the constant value.
  - Spatial properties loop along the spatial path (sampling `v` handles that); `offset` adds the vector delta.
- **Priority:** P0 (`loopOut()` is one of the most common expressions)
- **Owner:** bae-anim
- **Notes:** Algorithms checked against lottie-web `loopOut` (ExpressionPropertyDecorator.js). Lottie-web deviations BAE must NOT copy: its `continue` uses a numerical slope over 0.001 s scaled inconsistently for arrays (/0.0005); its `loopOutDuration` computes `|lastKey − fps·duration|` (bug). AE applies looping only to keyframe times; the layer's out point does not stop the loop (values continue past it).

### AE-ANI-439 · loopIn() and loopInDuration() (循环输入 loopIn)

- **What it does:** Before the first keyframe, repeats or extrapolates a segment that starts at the first keyframe (from the layer's in point onwards).
- **Exact semantics:**

  | Method / Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | `loopIn(type, numKeyframes)` | | | | | segment = keyframe 1 … keyframe `numKeyframes`+1; 0 or too large → all |
  | type (类型) | String | `"cycle"` | `"cycle"`, `"pingpong"`, `"offset"`, `"continue"` | — | |
  | numKeyframes (关键帧数) | Number | 0 | 0…numKeys−1 | keyframes | e.g. `loopIn("cycle", 3)` loops keys 1–4 |
  | `loopInDuration(type, duration)` | | | | | |
  | duration (持续时间) | Number | 0 | ≥ 0 | comp seconds | segment = [firstKey, firstKey + duration]; 0 → [firstKey, layer outPoint] |

  ```
  s = K[1].time
  if n < 2 or t >= s: return value(t)
  e = loopIn: K[1 + m].time (m as in loopOut) ; loopInDuration: duration > 0 ? s + duration : layer.outPoint
  D = e - s; if D <= 0: return value(s)
  p = floor((s - t) / D); φ = (s - t) - p*D                 // φ in [0, D)
  cycle:    φ == 0 ? v(s) : v(e - φ)
  pingpong: p even ? v(s + φ) : v(e - φ)                    // mirrored first
  offset:   v(e - φ) - (p + 1) * (v(e) - v(s))   (φ == 0: v(s) - p*(v(e)-v(s)))
  continue: v(s) - vel_out(s) * (s - t)                     // outgoing velocity at the first key
  ```
  - Combined idiom from the docs: `loopIn("continue") + loopOut("continue") - value`.
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Mirrors AE-ANI-438; lottie-web reference has an extra off-by-`firstKeyFrame` term in its loopIn `cycle` branch (`cycleDuration - (… + firstKeyFrame)`), a bug when the first key is not at 0. Boundary handling at φ = 0 is BAE's continuity choice (UNVERIFIED for AE).

### AE-ANI-440 · numKeys, key(index) and the Key object (关键帧数与 Key 对象)

- **What it does:** Read-only access to a property's keyframes from expressions.
- **Exact semantics:**

  | Member | Parameter (中文) | Type | Returns |
  |---|---|---|---|
  | `prop.numKeys` | — | — | Number of keyframes (markers for a marker property) |
  | `prop.key(index)` | index (索引) | Number, 1-based | Key (MarkerKey on marker properties) |
  | `Key.value` | — | — | keyframe value (property type) |
  | `Key.time` | — | — | keyframe time, comp seconds |
  | `Key.index` | — | — | 1-based index |

  - `key(i)` with i < 1 or i > numKeys → error (UNVERIFIED message, e.g. "index out of range"); non-integer i: UNVERIFIED (BAE: floor).
  - Key does not expose interpolation type, ease or spatial tangents.
  - Separated dimensions: `position.numKeys` reflects the separated X/Y/Z properties' own counts (the unified Position has none, UNVERIFIED) — see ani-a, Separate dimensions.
  - A Key object read in an expression is a snapshot; `Key.value` arithmetic follows AE-ANI-403.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Typical: `key(numKeys).time`, `value - key(1).value`. Lottie-web: `key(i)` returns `{time, value, index}` with time in seconds.

### AE-ANI-441 · nearestKey(t), previousKey(t), nextKey(t) (最近关键帧、上一个与下一个关键帧)

- **What it does:** Finds a Key (or MarkerKey) relative to a time.
- **Exact semantics:**

  | Method | Parameter (中文) | Type | Returns | Since |
  |---|---|---|---|---|
  | `nearestKey(t)` | t (时间) | Number, comp seconds | key with minimal \|key.time − t\|; tie → earlier key (UNVERIFIED) | long-standing |
  | `previousKey(t)` | t | Number | last key with time ≤ t; if none, the FIRST key | AE 26.0 (2026), JS engine only |
  | `nextKey(t)` | t | Number | first key with time > t; if none, the LAST key | AE 26.0, JS engine only |

  - Property with 0 keys → error (UNVERIFIED; BAE: error "property has no keyframes").
  - Work on marker properties (`marker.nearestKey(time)`), returning MarkerKey (AE-ANI-461, AE-ANI-462).
- **Priority:** P1 (`nearestKey`), P2 (`previousKey`/`nextKey`)
- **Owner:** bae-anim
- **Notes:** Docs example (AE 26): `valueAtTime(linear(time - marker.previousKey(time).time, 0, dur, key(1).time, key(numKeys).time))` retriggers an animation at each marker. Implementation hint: binary search over sorted key times in flicks.

### AE-ANI-442 · Property identity and hierarchy: name, propertyIndex, propertyGroup(), numProperties (属性名称、索引与属性组)

- **What it does:** Lets expressions discover where a property sits in the layer's property tree, for name-independent rigs.
- **Exact semantics:**

  | Member | Parameter (中文) | Type | Default | Returns |
  |---|---|---|---|---|
  | `prop.name` / `group.name` | — | — | — | String: display name (user-renamed name if any) |
  | `prop.propertyIndex` / `group.propertyIndex` | — | — | — | Number: 1-based index inside the parent group |
  | `prop.propertyGroup(countUp)` | countUp (向上层级数) | Number | 1 | PropertyGroup `countUp` levels up |
  | `group.numProperties` | — | — | — | Number of direct children (not recursive) |

  - `propertyGroup(1)` of a brush stroke's Rotation → the stroke's Transform group; `propertyGroup(2)` → the Brush group. Climbing past the layer's top-level groups returns the Layer (UNVERIFIED; BAE: return the Layer object, error beyond it).
  - Indexes cover masks, effects, text animators, selectors, shape groups and contents, trackers and track points.
  - `thisLayer("ADBE Effect Parade").numProperties` = number of effects on the layer.
- **Priority:** P1
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Typical use: duplicated shape groups reading `thisProperty.propertyGroup(3).propertyIndex` for a per-copy offset. The property tree itself is in project-comp-layers.md, Property tree/value types.

### AE-ANI-443 · Property and group access by name, index and matchName (按名称、索引与匹配名访问属性)

- **What it does:** Calling a layer or property group like a function returns a child property/group by display name, match name or 1-based index.
- **Exact semantics:**
  ```
  layerOrGroup(nameOrIndex) -> Property | PropertyGroup
      String: first child whose display name == s, else whose matchName == s     // case-sensitive (UNVERIFIED precedence)
      Number: child at 1-based index (propertyIndex)
  examples:
      thisLayer("ADBE Transform Group")("ADBE Position")      // == transform.position
      thisLayer("Effects")("Gaussian Blur")("Blurriness")      // localized display names work but break in other languages
      effect(1)(2)                                            // second parameter of first effect
      this(5)(2)                                              // Legacy only; JS engine: thisLayer(5)(2)
  missing child -> error naming the missing property
  ```
  - Display names are localized; match names (e.g. `ADBE Transform Group`, `ADBE Position`, `ADBE Effect Parade`, `ADBE Mask Parade`, `ADBE Root Vectors Group`, `ADBE Text Properties`) are language-independent. The pick whip writes compact English attribute forms (`transform.position`, `effect("Name")("Param")`) in AE 2026, UNVERIFIED whether localized builds write match names.
  - Top-level groups reachable on a layer: Masks, Effects, Transform, Layer Styles, Geometry Options, Material Options, Audio, Time Remap, Motion Trackers, Text, Contents, Essential Properties (see project-comp-layers.md, Property tree).
  - Layer Styles are reached the same way; the pick whip writes attribute chains such as `layerStyle.dropShadow.opacity` (UNVERIFIED spellings; match names `ADBE Layer Styles`, `dropShadow/enabled`… per the scripting match-name list).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** BAE decision: every BAE property carries an AE-compatible matchName (from docs/matchnames on docsforadobe; cached at research/common/dfa-matchnames-effects-firstparty.md) so AE expressions using match names resolve. Name lookup must be O(1) via a per-group hash map built at load.

## Path property methods

### AE-ANI-444 · Path values: points(), inTangents(), outTangents(), isClosed() (路径点与切线读取)

- **What it does:** Reads the vertices and bezier handles of a mask path, a shape-layer Path item or a brush stroke path. Added in AE 15.0 (2017).
- **Exact semantics:**

  | Method | Parameter (中文) | Type | Default | Returns |
  |---|---|---|---|---|
  | `path.points(t)` | t (时间) | Number, comp seconds | `time` | Array of `[x, y]`, rounded to 4 decimals |
  | `path.inTangents(t)` | t | Number | `time` | Array of `[x, y]` offsets relative to their vertex, rounded to 4 decimals |
  | `path.outTangents(t)` | t | Number | `time` | same, outgoing handles |
  | `path.isClosed()` | — | — | — | Boolean |
  | `path.name` | — | — | — | String |

  - Coordinate space: mask paths → layer space (origin top-left of the layer); shape paths → the shape group's space (relative to the group's Transform Anchor Point, i.e. the path's own coordinates); brush strokes → relative to the stroke start (first point = [0,0]).
  - "Rounded to the fourth decimal place": `round(x * 10000) / 10000` on every coordinate.
  - A path property's `value` in an expression is a Path object; returning a Path object (from another path property, or `createPath`) from a path property's expression sets the path (e.g. `thisComp.layer("Shape").content("Shape 1").content("Path 1").path` on a Mask Path). Arithmetic on Path objects is not supported.
  - Accessors: `mask("Mask 1").maskPath` (the reference's own examples also use `mask("Mask 1").path`), `content("Shape 1").content("Path 1").path`, brush `effect("Paint")("Brush 1")("Path")` (UNVERIFIED access string).
- **Priority:** P1
- **Owner:** bae-anim (+ bae-vector)
- **Notes:** Parametric shapes (Rectangle/Ellipse/Polystar) have no path property in AE, so these methods are unavailable on them (UNVERIFIED). BAE shape path model is in shapes.md, Bezier model. Lottie-web: `points()`, `inTangents()`, `outTangents()`, `isClosed()` on shape paths (Lottie `ks.k` with `v`, `i`, `o`, `c`), tangent arrays as offsets.

### AE-ANI-445 · pointOnPath(), tangentOnPath(), normalOnPath() (路径上的点、切线与法线)

- **What it does:** Samples a point, unit tangent or unit normal at an arc-length percentage along a path.
- **Exact semantics:**

  | Method | Parameters (中文) | Default | Returns |
  |---|---|---|---|
  | `pointOnPath(percentage, t)` | percentage (百分比): Number 0–1; t (时间): Number | 0.5, `time` | `[x, y]` |
  | `tangentOnPath(percentage, t)` | same | 0.5, `time` | unit vector `[x, y]` (length exactly 1) |
  | `normalOnPath(percentage, t)` | same | 0.5, `time` | unit vector `[x, y]` (length exactly 1) |

  ```
  L = total arc length (closed paths include the closing segment)
  s = percentage * L
  find segment k and local bezier parameter u where cumulative arc length == s (arc-length parameterised, uniform speed)
  pointOnPath  = B_k(u)
  tangentOnPath = normalize(B_k'(u))                // direction of travel
  normalOnPath  = [-tangent.y, tangent.x]            // tangent rotated +90° in y-down layer space (lottie-web; AE sign UNVERIFIED)
  closed path: percentage 0 and 1 return the same point
  percentage outside [0,1]: UNVERIFIED (lottie-web returns the end point; BAE: clamp)
  zero-length path: point = first vertex, tangent/normal = [0,0] (lottie-web)
  ```
  - Arc-length percentages do not align with vertices (a three-point path's middle vertex is generally not at 0.5); open vs closed paths with the same points give different results.
  - `tangentOnPath`/`normalOnPath` return offsets of length exactly 1 from the sampled point; documented: they differ from `outTangents()` even where a user vertex lies at that percentage (those are raw, non-normalised handles). The incoming tangent is `-tangentOnPath()`.
- **Priority:** P1
- **Owner:** bae-anim (+ bae-vector)
- **Notes:** Lottie-web approximates derivatives by sampling `pointOnPath(p + 0.001)`; BAE decision: analytic derivative of the cubic with kurbo's arc-length inversion (accuracy 1e-4 px), which differs from lottie-web only below visible precision. Typical use: attach a layer to a path, `toComp(pointOnPath(linear(time,0,5,0,1)))`.

### AE-ANI-446 · createPath(points, inTangents, outTangents, is_closed) (创建路径 createPath)

- **What it does:** Builds a Path value from arrays so an expression can generate or modify mask/shape paths.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Notes |
  |---|---|---|---|---|
  | points (点) | Array of `[x, y]` | `[[0,0],[100,0],[100,100],[0,100]]` | length ≥ 1 | required unless called with no arguments |
  | inTangents (入切线) | Array of `[x, y]` | `[]` | length 0 or exactly `points.length` | offsets relative to each vertex; `[]` → all `[0,0]` (corner points) |
  | outTangents (出切线) | Array of `[x, y]` | `[]` | as inTangents | |
  | is_closed (是否闭合) | Boolean | `true` | — | AE 16.1 fixed the menu inserting `is_closed` vs `isClosed` (parameter name only) |

  - Called as a method of a path property (`thisProperty.createPath(...)` / `myMask.createPath(...)`); a global `createPath` also resolves via implicit scope on path properties.
  - Documented: a non-empty tangent array "must be exactly the same" length as `points`; AE's reaction to a mismatch (error vs fill) is UNVERIFIED (lottie-web silently fills missing with [0,0]; BAE: error).
  - Result is valid only as the result of a path property (or argument to further path calls); returning it from a numeric property is an error.
  - Examples: `p.createPath(p.points())` straightens all curves; `p.createPath(p.points(), p.inTangents(), p.outTangents(), false)` opens a closed path.
- **Priority:** P1
- **Owner:** bae-anim (+ bae-vector)
- **Notes:** Lottie-web `createPath(points, inTangents, outTangents, closed)` matches. Path interpolation/vertex-count rules for keyframed paths are in shapes.md, Bezier model.

## Layer object

### AE-ANI-447 · Layer general attributes (图层常规属性)

- **What it does:** Read-only layer facts usable in expressions (also available unqualified on the expression's own layer).
- **Exact semantics:**

  | Attribute | Type | Meaning |
  |---|---|---|
  | `name` | String | layer name; the source name if the layer has no custom name |
  | `index` | Number | 1-based stacking index in the comp (top = 1) |
  | `width`, `height` | Number (px) | source comp/footage/solid size; text and shape layers return the comp size (UNVERIFIED); null 100×100 (UNVERIFIED); camera/light: UNVERIFIED (BAE: comp size) |
  | `inPoint`, `outPoint` | Number (comp s) | layer In/Out; reversed (negative stretch) layers have `inPoint > outPoint` |
  | `startTime` | Number (comp s) | layer start time (may be > inPoint) |
  | `parent` | Layer/Camera/Light | parent layer; error if none |
  | `hasParent` | Boolean | true if a parent is set (safe to test before using `parent`) |
  | `enabled` | Boolean | Video switch (eyeball) on |
  | `active` | Boolean | Video switch on AND `inPoint <= time < outPoint` (UNVERIFIED: out point exclusive) |
  | `audioActive` | Boolean | Audio switch on AND time within In/Out |
  | `hasVideo`, `hasAudio` | Boolean | layer has a video / audio component |

  - All time attributes are in comp seconds of the layer's comp; `inPoint/outPoint/startTime` reflect the current layer timing (see project-comp-layers.md, Layer attributes & timing).
  - `active` depends on the evaluated time (not solo state; UNVERIFIED whether solo of another layer affects it — BAE: no).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Lottie-web exposes `index`, `name`, `inPoint`, `outPoint`, `width`, `height`, `hasParent`, `parent`, `active` (`ip`/`op`/`ind`/`nm`/`parent` keys). Typical: `if (hasParent) parent.index`.

### AE-ANI-448 · Layer transform shortcuts (图层变换属性快捷访问)

- **What it does:** Direct attribute access to a layer's Transform properties (the post-expression values of those properties).
- **Exact semantics:**

  | Attribute | Type | Meaning |
  |---|---|---|
  | `transform` | PropertyGroup | the Transform group (`ADBE Transform Group`); `transform.position` etc. |
  | `anchorPoint` | Array 2/3 | layer space |
  | `position` | Array 2/3 | in parent space if parented, else comp/world space; separated dimensions still return the combined array |
  | `xPosition`, `yPosition`, `zPosition` | Number | separated components (`transform.xPosition`; UNVERIFIED as bare layer attributes) |
  | `scale` | Array 2/3 | percent |
  | `rotation` | Number | degrees; on 3D layers = Z Rotation |
  | `opacity` | Number | percent 0–100 |
  | `orientation` | Array 3 | degrees, 3D only |
  | `rotationX`, `rotationY`, `rotationZ` | Number | degrees, 3D only |
  | Camera/Light `pointOfInterest` | Array 3 | see AE-ANI-457/AE-ANI-458 |

  - Accessing a 3D-only attribute on a 2D layer → error (UNVERIFIED; e.g. `orientation` on a 2D layer).
  - These are Property objects (support `.valueAtTime()`, `.velocity`, `.wiggle()`…), coerced to values where a value is needed.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Transform math (matrix order, auto-orient) is in this file's ani-a items. Lottie: `transform` interface maps `ks.a`, `ks.p` (or `ks.px/py/pz` when split), `ks.s`, `ks.r` (`ks.rz`), `ks.o`, `ks.or`, `ks.rx`, `ks.ry`.

### AE-ANI-449 · Layer 3D material attributes (图层 3D 材质属性)

- **What it does:** Read Material Options of a 3D layer as numbers.
- **Exact semantics:**

  | Attribute | Type | Values |
  |---|---|---|
  | `castsShadows` | Number | 0 off, 1 on, 2 Only |
  | `acceptsShadows` | Number | 0 off, 1 on, 2 Only |
  | `acceptsLights` | Number ("Boolean Number") | 0 off, 1 on |
  | `lightTransmission` | Number | percent |
  | `ambient`, `diffuse`, `specular`, `shininess`, `metal` | Number | percent (specular intensity = `specular`) |

  - Booleans are returned as numbers ("Boolean Number"), so `castsShadows == true` is true only for 1.
- **Priority:** P2
- **Owner:** bae-anim
- **Notes:** Material Options semantics are in compositing-3d-color.md, Material options. Legacy snake_case aliases in AE-ANI-402.

### AE-ANI-450 · Layer.source and sourceTime(t) (图层源与源时间)

- **What it does:** `source` returns the layer's source Comp or Footage object with time mapped into the source; `sourceTime(t)` returns the source time shown at comp time t.
- **Exact semantics:**

  | Member | Parameter (中文) | Type | Default | Returns |
  |---|---|---|---|---|
  | `source` | — | — | — | Comp (precomp layers) or Footage (footage, solids UNVERIFIED); error for text/shape/null/camera/light |
  | `sourceTime(t)` | t (时间) | Number, comp seconds | `time` | Number, source seconds (AE CS5.5 / 10.5+) |

  ```
  sourceTime(t):
      if timeRemapEnabled: return timeRemap.valueAtTime(t)          // post-expression Time Remap
      return (t - startTime) * 100 / stretchPercent                  // stretch < 0 = reversed
  objects reached through `source` (e.g. source.layer(1).position) evaluate at sourceTime(time)
  (with the precomp's own displayStartTime considered, UNVERIFIED), unlike comp("Name") which uses `time` unchanged (AE-ANI-408)
  ```
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Stretch/time-remap math is in ani-a, Time remap. Frame-rate conformance of footage (Interpret Footage) is applied by the media layer when decoding, not by `sourceTime` (UNVERIFIED for footage with conformed frame rate: sourceTime returns conformed-time seconds).

### AE-ANI-451 · sourceRectAtTime(t, includeExtents) (源矩形 sourceRectAtTime)

- **What it does:** Returns the bounding box of the layer's content in layer space, typically to size backgrounds behind text or to center anchors. Added in AE 13.2 (2014); paragraph extents in AE 15.1.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | t (时间) | Number | `time` | comp seconds (documented). For precomp/footage layers the source is sampled at the source time that comp time maps to (UNVERIFIED) |
  | includeExtents (包括范围（译名待核）) | Boolean | `false` | documented to apply only to shape layers ("increases the size of the layer bounds as necessary", e.g. stroke widths) and paragraph text layers (returns the paragraph box, AE 15.1+); ignored for other layer types |

  - Returns a plain object `{top, left, width, height}` (layer-space pixels; `top`/`left` may be negative).
  - Per layer type:
    - Footage, solid, precomp, placeholder: `{top: 0, left: 0, width: source width, height: source height}`.
    - Null: `{0, 0, 100, 100}` (UNVERIFIED).
    - Point text: tight bounds of the laid-out glyphs (ink bounds, so "a" and "g" give different heights) relative to the layer origin at the first baseline; includes text animator offsets (UNVERIFIED) and per-character 3D is flattened (UNVERIFIED). Stroke width not included (UNVERIFIED).
    - Paragraph text: glyph bounds; with `includeExtents = true` the paragraph text box rectangle.
    - Shape layer: union of the geometric bounds of all visible shape contents after group transforms, in layer space; with `includeExtents = true` grown by stroke half-widths/miters (UNVERIFIED exact miter handling). Empty shape layer: `{0,0,0,0}` (UNVERIFIED).
    - Camera / light: error (UNVERIFIED).
  - Effects, masks and layer transforms are NOT included.
- **Priority:** P0 (ubiquitous in templates: auto-sizing text boxes)
- **Owner:** bae-anim (+ bae-text, bae-vector)
- **Notes:** Text layout bounds are defined in text.md (layout, bounds); shape bounds in shapes.md. Lottie-web's text `sourceRectAtTime` returns the SVG bbox of the text. Implementation hint: this forces text layout/shape flattening during expression evaluation; bae-text and bae-vector must expose CPU-side bounds queries that do not touch the GPU, cached per (layer, time).

### AE-ANI-452 · sampleImage(point, radius, postEffect, t) (采样图像 sampleImage)

- **What it does:** Returns the average color of a rectangle of a layer's rendered pixels, letting expressions react to image content.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | point (点) | Array[2] | required | any | layer-space px | `[0,0]` = centre of the upper-left pixel |
  | radius (半径) | Array[2] | `[0.5, 0.5]` | ≥ 0 | px | half-width/half-height of the sampled rectangle; default = one pixel |
  | postEffect (效果后) | Boolean | `true` | — | — | true: after masks and effects of that layer; false: source before masks/effects |
  | t (时间) | Number | `time` | any | comp s | |

  ```
  rect = [point - radius, point + radius] in layer pixel space (pixel centres at integer coords)
  for each pixel p overlapping rect, weight w_p = coverage area of p by rect (UNVERIFIED: area-weighted vs centre-inside)
  A   = Σ w_p * a_p / Σ w_p
  RGB = Σ w_p * a_p * rgb_p / Σ (w_p * a_p)       // "alpha-weighted": un-premultiplied average; 0 if A == 0
  return [R, G, B, A]                              // 0..1 (32 bpc may exceed); working-space values
  ```
  - Layer transforms, motion blur and the layer's blend mode are NOT applied; pixels outside the layer are transparent black.
  - Example `thisComp.layer(1).sampleImage([100,100], [2,1.5])` samples a 4×3 px rectangle.
  - Since AE 13.2 it no longer disables multiprocessing; AE 16.1 fixed wrong results when the value was read by a ScriptUI panel.
- **Priority:** P1
- **Owner:** bae-anim (+ bae-render)
- **Notes:** UNVERIFIED: sampled at full resolution regardless of preview resolution. Implementation hint: requires a GPU render of the sampled layer at time t and a readback; batch all `sampleImage` requests of one frame into one compute pass with one readback, and evaluate dependent expressions after it (two-phase evaluation). Mark dependent properties as render-dependent in the cache key (AE-ANI-412).

### AE-ANI-453 · audioLevels and timeRemap attributes (音频电平与时间重映射属性)

- **What it does:** Exposes the Audio Levels and Time Remap properties of a layer.
- **Exact semantics:**

  | Attribute | Type | Meaning |
  |---|---|---|
  | `audioLevels` | Array[2] (dB) | value of the layer's Audio > Audio Levels property `[left, right]` (keyframable gain), NOT the loudness of the audio signal |
  | `timeRemap` | Number (s) | value of Time Remap when enabled; error if Time Remap is not enabled (UNVERIFIED) |

  - To drive animation from loudness use Animation > Keyframe Assistant > Convert Audio to Keyframes and reference its slider (see ani-a keyframe assistants; effects.md Expression Controls).
- **Priority:** P2
- **Owner:** bae-anim
- **Notes:** Time Remap semantics in ani-a, Time remap.

## Layer space transforms

### AE-ANI-454 · toComp, fromComp, toWorld, fromWorld (图层空间点变换)

- **What it does:** Converts points between a layer's own space, world space (3D comp space independent of the view) and composition space (what the active camera sees).
- **Exact semantics:**

  | Method | Parameters (中文) | Default | Returns |
  |---|---|---|---|
  | `toComp(point, t)` | point (点): Array 2/3; t (时间): Number | t = `time` | Array 2/3 |
  | `fromComp(point, t)` | same | `time` | Array 2/3 |
  | `toWorld(point, t)` | same | `time` | Array 2/3 |
  | `fromWorld(point, t)` | same | `time` | Array 2/3 |

  ```
  M(t) = layer-to-world 4x4 matrix of the layer at comp time t, including the whole parent chain
         (anchor -> scale -> rotation/orientation -> position, then parent's M; auto-orient included;
          exact order in this file's ani-a transform-matrix item). 2D layers: z terms = 0.
  p3 = [p.x, p.y, p.z ?? 0, 1]
  toWorld(p)   = (M(t) · p3).xyz           // returned with the input's dimension (2D in -> 2D out, UNVERIFIED)
  fromWorld(p) = (M(t)^-1 · p3).xyz
  toComp(p):
      if the layer is 2D, or the comp has no 3D layers: = toWorld(p)
      else: w = toWorld(p); c = active camera (or the comp's default camera)
            q = View(c,t) · w                             // camera space
            comp.xy = [W/2, H/2] + zoom(c) * q.xy / q.z  (perspective; W,H = comp size; UNVERIFIED that the
                      comp-centre offset is used rather than the camera's own position for one-node cameras)
            return [comp.x, comp.y, q.z?]                 // third component: UNVERIFIED (camera-space depth)
  fromComp(p): inverse of toComp; for 3D layers the comp point is un-projected to an
      unspecified depth, so "the result may have a nonzero z" (UNVERIFIED exact rule; BAE: un-project onto the world
      plane z = 0, then fromWorld)
  t: time at which M, the parent chain and the camera are evaluated
  ```
  - Parent and layer transforms are post-expression values, so these calls create dependencies on every transform property in the chain (AE-ANI-412).
  - Typical: `myLayer.toComp(myLayer.anchorPoint)` = the layer's anchor in comp space; `fromComp(thisComp.layer(2).position)`.
- **Priority:** P0
- **Owner:** bae-anim
- **Notes:** Lottie-web implements `toComp`, `fromComp`, `toWorld`, `fromWorld` with its 2D/3D transform matrices but ignores cameras (TransformInterface/LayerInterface). The default camera when 3D layers exist without a camera layer is specified in compositing-3d-color.md, Cameras. Implementation hint: reuse the EvaluatedComp matrices computed for rendering; when `t` is not the current frame, compute the chain on demand with the same code.

### AE-ANI-455 · toCompVec, fromCompVec, toWorldVec, fromWorldVec (图层空间矢量变换)

- **What it does:** Transforms direction vectors (differences of points) between layer, world and comp space, ignoring translation.
- **Exact semantics:**

  | Method | Parameters (中文) | Default | Returns |
  |---|---|---|---|
  | `toCompVec(vec, t)` | vec (矢量): Array 2/3; t (时间) | `time` | Array 2/3 |
  | `fromCompVec(vec, t)` | same | `time` | Array 2/3 |
  | `toWorldVec(vec, t)` | same | `time` | Array 2/3 |
  | `fromWorldVec(vec, t)` | same | `time` | Array 2/3 |

  ```
  toWorldVec(v)   = Lin(M(t)) · v            // upper-left 3x3 incl. scale, rotation, parent chain; no translation
  fromWorldVec(v) = Lin(M(t))^-1 · v
  toCompVec / fromCompVec: = the World variants for 2D layers / no camera;
      3D with camera: UNVERIFIED (BAE: Jacobian of the projection at the layer's anchor point)
  ```
  - Example: `toCompVec([1, 0])` = direction of the layer's x axis in comp space (used to derive a layer's on-screen angle via `Math.atan2`).
  - Docs example mistakenly uses `toWorld(sub(p1, p2))` where `toWorldVec` is meant.
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Vectors are not normalised by these methods.

### AE-ANI-456 · fromCompToSurface(point, t) (从合成到图层表面 fromCompToSurface)

- **What it does:** Finds where a comp-space (screen) point lands on a 3D layer's surface as seen from the active camera, returning layer coordinates — e.g. to drive an effect point on a 3D layer.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | point (点) | Array 2/3 | required | comp space; z ignored (UNVERIFIED) |
  | t (时间) | Number | `time` | |

  ```
  ray = camera centre -> the comp-space pixel `point` (through the active/default camera at t)
  plane = the layer's z = 0 plane in world space (M(t))
  hit = intersect(ray, plane)
  return fromWorld(hit).xy            // Array[2]
  ray parallel to plane / no 3D: UNVERIFIED (BAE: return fromComp(point).xy)
  ```
  - For 3D layers only; on 2D layers it equals `fromComp(point)` (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-anim
- **Notes:** Typical rig: a 2D null's position fed into `fromCompToSurface` on a 3D layer for Lens Flare centre.

## Camera and light attributes

### AE-ANI-457 · Camera layer attributes (摄像机属性)

- **What it does:** Read camera-specific values; a Camera is a Layer without source/effect/mask/width/height/anchorPoint/scale/opacity/audioLevels/timeRemap/material attributes.
- **Exact semantics:**

  | Attribute (also `cameraOption.<name>`) | Type | Units / values |
  |---|---|---|
  | `pointOfInterest` | Array[3] | world space px |
  | `zoom` | Number | px |
  | `depthOfField` | Number | 1 on / 0 off |
  | `focusDistance` | Number | px |
  | `aperture` | Number | px |
  | `blurLevel` | Number | percent |
  | `irisShape` | Number | 1–10, menu index; value 2 is the menu divider (AE 12.1+) |
  | `irisRotation` | Number | degrees |
  | `irisRoundness` | Number | percent |
  | `irisAspectRatio` | Number | 1–100 |
  | `irisDiffractionFringe` | Number | 1–100 |
  | `highlightGain` | Number | 1–100 |
  | `highlightThreshold` | Number | 0–100 (8 bpc), 0–32768 (16 bpc), 0–1.0 (32 bpc) |
  | `highlightSaturation` | Number | 1–100 |
  | `active` | Boolean | camera is the comp's active camera at `time`: Video switch on, time within In/Out, and topmost such camera |

  - `position`, `orientation`, `rotationX/Y/Z`, `rotation` available as on 3D layers. `thisComp.activeCamera` returns the active camera (AE-ANI-459).
  - Docs example (constant on-screen size): `cam = thisComp.activeCamera; scale * length(sub(position, cam.position)) / cam.zoom`.
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Camera semantics (zoom/FOV, DOF) are in compositing-3d-color.md, Cameras. The camera options group match name is `ADBE Camera Options Group`. UNVERIFIED: the active-camera rule's handling of solo and shy states.

### AE-ANI-458 · Light layer attributes (灯光属性)

- **What it does:** Read light-specific values; a Light is a Layer with the same exclusions as a Camera.
- **Exact semantics:**

  | Attribute (also `lightOption.<name>`) | Type | Units / values |
  |---|---|---|
  | `pointOfInterest` | Array[3] | world space px |
  | `intensity` | Number | percent |
  | `color` | Array[4] | RGBA 0–1 |
  | `coneAngle` | Number | degrees (Spot) |
  | `coneFeather` | Number | percent (Spot) |
  | `falloff` | Number | menu index of Falloff (None / Smooth / Inverse Square Clamped; 1-based, UNVERIFIED) |
  | `radius` | Number | px |
  | `falloffDistance` | Number | px |
  | `castsShadows` | Number | 1 on / 0 off |
  | `shadowDarkness` | Number | percent |
  | `shadowDiffusion` | Number | px |

  - Attributes not applicable to the light type (e.g. `coneAngle` on a Point light) return the stored value (UNVERIFIED; BAE: return the stored value, no error).
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Light semantics in compositing-3d-color.md, Lights. Light options group match name `ADBE Light Options Group`.

## Comp object

### AE-ANI-459 · Comp attributes (合成属性)

- **What it does:** Read composition settings from `thisComp`, `comp("Name")` or a precomp layer's `source`.
- **Exact semantics:**

  | Attribute | Type | Meaning |
  |---|---|---|
  | `name` | String | comp name |
  | `width`, `height` | Number | px |
  | `duration` | Number | seconds |
  | `frameDuration` | Number | seconds per frame = 1 / frame rate (e.g. 1001/30000 for 29.97) |
  | `displayStartTime` | Number | Start Timecode in seconds (e.g. 3600 for 01:00:00:00) |
  | `ntscDropFrame` | Boolean | timecode uses drop-frame (AE 10.5+) |
  | `pixelAspect` | Number | pixel aspect ratio (1 for square) |
  | `shutterAngle` | Number | degrees (motion blur) |
  | `shutterPhase` | Number | degrees |
  | `bgColor` | Array[4] | background color RGBA 0–1 (alpha 1, UNVERIFIED) |
  | `numLayers` | Number | number of layers |
  | `activeCamera` | Camera | camera the comp renders through at the current time (topmost active camera layer); not necessarily the Composition panel's view. No camera layer: error (UNVERIFIED) |
  | `marker` | Marker property | comp markers (AE-ANI-461) |

  - Values come from Composition Settings (see project-comp-layers.md, Composition Settings); shutter values from the Advanced tab.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Lottie-web CompInterface exposes `layer()`, `numLayers`, `frameDuration`, `displayStartTime`, `width`, `height`, `name` (keys `w`, `h`, `fr`, `nm`). BAE: `frameDuration` computed from the comp's rational frame rate, not rounded.

### AE-ANI-460 · Comp.layer() and layerByComment() (按索引、名称或相对位置获取图层)

- **What it does:** Returns a Layer (or Camera/Light) object of a comp.
- **Exact semantics:**

  | Overload | Parameters (中文) | Returns |
  |---|---|---|
  | `layer(index)` | index (索引): Number, 1-based Timeline order | Layer |
  | `layer(name)` | name (名称): String; layer name, or source name if the layer has no custom name; duplicates → topmost | Layer |
  | `layer(otherLayer, relIndex)` | otherLayer (其他图层): Layer; relIndex (相对索引): Number, + = below, − = above | Layer at `otherLayer.index + relIndex` |
  | `layerByComment(comment)` | comment (注释): String; case-sensitive substring match on the layer Comment column, topmost match | Layer (AE version UNVERIFIED, documented by docsforadobe) |

  - Out-of-range index / missing name → error (e.g. "Layer named 'X' is missing" style, UNVERIFIED wording).
  - `thisComp.layer(index - 1)` / `thisComp.layer(thisLayer, -1)` = layer directly above (common in chained rigs).
  - Name match is exact and case-sensitive.
  - Renaming a layer: AE rewrites `layer("Old")` references in expressions of the project (see AE-ANI-411).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-motion)
- **Notes:** BAE: layer lookup by name must be O(1) per comp (hash map, first-in-stack wins). Renaming references is a bae-motion `RenameLayer` Command side effect that rewrites expression strings (string literal match only).

## Markers in expressions

### AE-ANI-461 · Marker property access: marker.key(), nearestKey(), numKeys (标记属性访问)

- **What it does:** Reads layer markers (`thisLayer.marker`) and composition markers (`thisComp.marker`).
- **Exact semantics:**

  | Member | Parameters (中文) | Returns |
  |---|---|---|
  | `marker.numKeys` | — | Number of markers |
  | `marker.key(index)` | index (索引): Number, 1-based in time order (not the marker's numbered name) | MarkerKey |
  | `marker.key(name)` | name (名称): String = the marker's Comment; duplicates → earliest in time | MarkerKey |
  | `marker.nearestKey(t)` | t (时间): Number | MarkerKey nearest to t |
  | `marker.previousKey(t)`, `marker.nextKey(t)` | t | MarkerKey (AE 26.0, AE-ANI-441) |

  - Composition markers can no longer be accessed by marker number as in old AE; use `marker.key("1")` (default comp-marker names are numbers). AE 17.1.2 fixed name lookup in the JS engine.
  - `MarkerKey.time` is in comp seconds (for layer markers: the marker's comp time, which moves with the layer; UNVERIFIED on time-stretched layers).
  - No markers → `numKeys` = 0; `key()`/`nearestKey()` → error.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Marker data model in project-comp-layers.md, Comp markers / Layer markers. Lottie: comp markers in `markers` (`tm`, `cm`, `dr`); lottie-web exposes `thisComp.marker` partially.

### AE-ANI-462 · MarkerKey attributes (MarkerKey 属性)

- **What it does:** Fields of a marker returned by marker access methods.
- **Exact semantics:**

  | Attribute | Type | Source field in the marker dialog |
  |---|---|---|
  | `time` | Number (s) | marker time |
  | `index` | Number | 1-based index in time order |
  | `duration` | Number (s) | Duration |
  | `comment` | String | Comment |
  | `chapter` | String | Chapter |
  | `url` | String | URL |
  | `frameTarget` | String | Frame Target |
  | `cuePointName` | String | cue point Name |
  | `eventCuePoint` | Boolean | true = Event, false = Navigation |
  | `parameters` | Object (String → String) | Parameter Name / Value pairs; `m.parameters["background color"]` |
  | `protectedRegion` | Boolean | comp marker "Protected Region" (AE 16.0+); also true on the nested-comp layer's mirrored markers; otherwise n/a for layer markers |

  - `MarkerKey.value` returns the comment string (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Typical use: trigger animations at markers — `n = marker.nearestKey(time); if (time < n.time) n = …; t = time - n.time; …`.

## Footage object and data-driven expressions

### AE-ANI-463 · Footage attributes (素材属性)

- **What it does:** Reads properties of a footage project item obtained by `footage("name")` or a footage layer's `source`.
- **Exact semantics:**

  | Attribute | Type | Meaning |
  |---|---|---|
  | `name` | String | name in the Project panel |
  | `width`, `height` | Number | px (after Interpret Footage? UNVERIFIED: source pixel size) |
  | `duration` | Number | seconds (0 for stills, UNVERIFIED) |
  | `frameDuration` | Number | seconds per frame (conformed frame rate per Interpret Footage, UNVERIFIED) |
  | `ntscDropFrame` | Boolean | drop-frame timecode (AE 10.5+) |
  | `pixelAspect` | Number | pixel aspect ratio |

  - Footage obtained via `layer.source` evaluates time-based members at the layer's source time (AE-ANI-450).
- **Priority:** P1
- **Owner:** bae-anim (+ bae-media)
- **Notes:** Interpret Footage settings are specified in project-comp-layers.md, Interpret Footage.

### AE-ANI-464 · JSON, CSV and TSV data access: sourceText, sourceData, dataValue() (数据文件访问)

- **What it does:** Lets expressions read imported data files (JSON, CSV, TSV/TXT, MGJSON) to drive animation. Added in AE 15.0 (2017); CSV/TSV support later (UNVERIFIED version: AE 16.x/17.x).
- **Exact semantics:**

  | Member | Parameters (中文) | Returns |
  |---|---|---|
  | `footage("f.json").sourceText` | — | String: the whole file contents (read-only) |
  | `footage("f.json").sourceData` | — | the parsed JSON value (object/array) — documented as "an array of sourceData objects" (read-only) |
  | `footage("f").dataValue(dataPath)` | dataPath (数据路径): Array of 0-based indices | value of a static or dynamic data stream |

  ```
  JSON: sourceData = JSON.parse(sourceText)   // AE 15.1+ uses a strict JSON parser (functions/comments rejected at import)
        e.g. footage("sample.json").sourceData[0].Color ; eval(footage("sample.json").sourceText) also works
  CSV/TSV: dataValue([row, column])           // help: "Rows and columns are zero-indexed", footage("sample.tsv").dataValue([15,1]) = row 16, column 2
        UNVERIFIED: whether index 0 is the header row and the argument order ([row, col] in the help text vs
        [column, row] reported by users); values are Numbers when numeric, else Strings
  MGJSON: dataValue(path) where path indexes the dataOutline hierarchy, e.g. [0] first child, [1, 0] first child of the second group
        dynamic stream: value at the current time (sample interpolation UNVERIFIED: nearest previous sample)
  ```
  - Adding a data footage item to a comp creates a layer whose only group is "Data" (数据), with one property per data stream, each carrying an auto-generated expression that reads the file (see project-comp-layers.md, layer types).
  - Changing the file on disk and reloading updates all dependents.
- **Priority:** P2
- **Owner:** bae-anim (+ bae-media)
- **Notes:** The docs examples `dataValue([1][0])` / `dataKeyCount([1][0])` are a JavaScript typo: `[1][0]` evaluates to the number 1; the intended path is `[1, 0]`. BAE implements paths as flat index arrays and also accepts a bare number as a one-element path. BAE decision: parse data files once in bae-media (serde_json / csv crate) and hand QuickJS a frozen JS value shared across evaluations; never re-parse per frame.

### AE-ANI-465 · MGJSON dynamic data: dataKeyCount(), dataKeyTimes(), dataKeyValues() (MGJSON 动态数据流)

- **What it does:** Reads the time samples of a dynamic data stream in a Motion Graphics JSON (.mgjson) file.
- **Exact semantics:**

  | Method | Parameters (中文) | Default | Returns |
  |---|---|---|---|
  | `dataKeyCount(dataPath)` | dataPath (数据路径): Array | required | Number of samples |
  | `dataKeyTimes(dataPath, t0, t1)` | dataPath; t0 (开始时间): Number s; t1 (结束时间): Number s | t0 = stream start, t1 = stream end (`samplesTemporalExtent`) | Array of sample times (s) within [t0, t1] |
  | `dataKeyValues(dataPath, t0, t1)` | same | same | Array of sample values within [t0, t1] |

  - Static streams: count 1 (UNVERIFIED). Interval inclusivity of [t0, t1]: UNVERIFIED (BAE: inclusive).
- **Priority:** P2
- **Owner:** bae-anim (+ bae-media)
- **Notes:** MGJSON schema (dataOutline, dataDynamicSamples, samplesTemporalExtent) is published by Adobe; import is specified in ui-preview-render-io.md, import formats.

### AE-ANI-466 · .jsx expression function libraries and eval() (表达式函数库与 eval)

- **What it does:** Shares functions and constants across expressions by importing a `.jsx` file as footage and reading it through `footage("lib.jsx").sourceData`. Added in AE 15.1 (2018).
- **Exact semantics:**
  ```
  lib.jsx content: a single JavaScript object literal, e.g.
      { "add2": function (a, b) { return thisLayer.add(a, b); }, "speed": 5 }
  usage: footage("lib.jsx").sourceData.add2([1,2], [3,4])
  rules (JS engine):
    - the file is evaluated once per project load/reload, not per frame (AE detail UNVERIFIED)
    - no operator pre-processing (AE-ANI-403): array math must use add/sub/mul/div
    - no implicit scope (AE-ANI-414): prefix native members with thisLayer./thisProperty.
      (thisLayer.time, thisProperty.value, thisLayer.timeToFrames(), thisProperty.wiggle(), thisLayer.comp(), thisLayer.footage(), …)
    - thisLayer/thisProperty inside library functions refer to the CALLING expression's layer/property
  eval(string) inside an expression: same restrictions as .jsx code
  ```
  - AE 15.0 allowed function libraries in `.json` files; AE 15.1's strict JSON parser rejects them, so they must be renamed to `.jsx` (Replace Footage keeps references).
  - Performance note from Adobe: library code skips pre-processing and can be faster than the same code inline.
- **Priority:** P2
- **Owner:** bae-anim (+ bae-media)
- **Notes:** BAE decision: compile each library once per runtime into a frozen object; bind `thisLayer`/`thisProperty`/`thisComp` as per-call dynamic globals so library functions see the caller's context. `.jsx` imports are collected by bae-package (project packaging).

## Effects, masks, shape contents and text animators

### AE-ANI-467 · Effect access: effect(), param(), active (效果访问)

- **What it does:** Reaches an effect on a layer and its parameters.
- **Exact semantics:**

  | Member | Parameters (中文) | Returns |
  |---|---|---|
  | `layer.effect(name)` | name (名称): String; current name in Effect Controls (default or user-renamed); duplicates → topmost | Effect (PropertyGroup) |
  | `layer.effect(index)` | index (索引): Number, 1-based top-down in Effect Controls | Effect |
  | `effect(...).param(name|index)` | parameter display name or 1-based index | Property |
  | `effect(...)(name|index)` | shorthand of `param()` (also accepts match names, AE-ANI-443) | Property |
  | `effect(...).active` | — | Boolean: effect switch (fx) on |
  | `effect(...).name` / `.numProperties` | — | as PropertyGroup (AE-ANI-442) |

  - Effect point parameters (e.g. Bulge Center) are always in layer space — use `toComp()`/`fromComp()` to convert.
  - Parameter indexes count every parameter row including group headers and hidden/legacy parameters (UNVERIFIED; e.g. Bulge "Bulge Height" is `param(4)` in the docs example), so per-effect index tables must match AE's matchName order (`ADBE Bulge-0004`).
  - Renaming an effect: AE rewrites references (AE 16.1 fixed wrongly renaming parameter references that share the effect's name).
  - Missing effect/parameter → error.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-effects)
- **Notes:** Effect parameter definitions and match names per effect are in effects.md. Lottie: `ef` array, parameter interface `effect("Name")("Param")` and `effect(i)(j)` supported by lottie-web (`ef[].ef[].v`).

### AE-ANI-468 · Mask access and Mask attributes (蒙版访问与蒙版属性)

- **What it does:** Reaches a layer mask and its properties.
- **Exact semantics:**

  | Member | Type | Meaning |
  |---|---|---|
  | `layer.mask(name)` | Mask | by name (default "Mask 1" or user name); duplicates → topmost |
  | `layer.mask(index)` | Mask | 1-based top-down in the Timeline |
  | `mask.maskPath` | Path property | AE-ANI-444; linkable to other path properties, not numerically editable except via path methods |
  | `mask.maskOpacity` | Number | percent |
  | `mask.maskFeather` | Array[2] | px (x, y) |
  | `mask.maskExpansion` | Number | px |
  | `mask.invert` | Boolean | Inverted checkbox |

  - Mask mode (Add/Subtract…) is not readable from expressions (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** Mask rendering is in compositing-3d-color.md, Masks. Lottie: `masksProperties[]` (`pt`, `o`, `x` expansion, `inv`); lottie-web supports `mask("name").maskPath` (`maskPath` only).

### AE-ANI-469 · Shape layer content access: content() (形状图层内容访问)

- **What it does:** Reaches shape groups, paths, fills, strokes and operators of a shape layer.
- **Exact semantics:**
  ```
  layer.content(nameOrIndex)                 // top-level item of Contents (ADBE Root Vectors Group), name or 1-based index
  group.content(nameOrIndex)                 // item inside a Group's Contents
  group.transform.position / .scale / .rotation / .opacity / .anchorPoint / .skew / .skewAxis
  pathItem.path                              // Path property (AE-ANI-444..446)
  rect.size / .position / .roundness ; ellipse.size ; star.points / .outerRadius …
  fill.color / .opacity ; stroke.color / .strokeWidth / .opacity / .dash(…) ; repeater.copies …
  examples: content("Shape 1").content("Path 1").path ; content("Group 1").transform.rotation
  ```
  - Names are the item names shown in the Timeline ("Rectangle 1", "Fill 1", user names); duplicates → first (topmost) (UNVERIFIED).
  - Attribute names follow the camelCase of the English property name (e.g. "Stroke Width" → `strokeWidth`); match names (e.g. `ADBE Vector Group`, `ADBE Vector Shape`) also work via call syntax.
- **Priority:** P0 (shape-layer rigs depend on it)
- **Owner:** bae-anim (+ bae-vector)
- **Notes:** Shape item definitions and their properties are in shapes.md. Lottie: `shapes[]`/`it[]` with `ty` gr/sh/rc/el/sr/fl/st/tr…; lottie-web's ShapeInterface exposes `content()`, `path`, `transform`, `color`, `opacity`, `strokeWidth`, `size`, `position`, `roundness`.

### AE-ANI-470 · Text layer property access: text, animators, selectors, path and more options (文本属性访问)

- **What it does:** Reaches the properties of a text layer's Text group.
- **Exact semantics:**

  | Access | Returns |
  |---|---|
  | `text` (on a text layer) | the Text group (`ADBE Text Properties`) |
  | `text.sourceText` | Source Text property (AE-ANI-471) |
  | `text.pathOption` | Path Options group: `.path` (mask index menu), `.reversePath`, `.perpendicularToPath`, `.forceAlignment`, `.firstMargin`, `.lastMargin` (UNVERIFIED attribute spellings) |
  | `text.moreOption` | More Options group: `.anchorPointGrouping`, `.groupingAlignment`, `.fillAndStroke`, `.interCharacterBlending` (UNVERIFIED spellings) |
  | `text.animator(nameOrIndex)` | a Text Animator group |
  | `text.animator(n).property.<prop>` | animator properties, e.g. `.position`, `.scale`, `.opacity`, `.fillColor`, `.tracking`; AE 26.0: `.fontAxisWght`, `.fontAxisWdth`, `.fontAxisSlnt`, `.fontAxisItal`, `.fontAxisOpsz` and custom `fontAxis<TAG>` (variable fonts) |
  | `text.animator(n).selector(nameOrIndex)` | a Range / Wiggly / Expression selector group; e.g. `.start`, `.end`, `.offset`, `.amount` |

  - Expression Selector variables `textIndex`, `textTotal`, `selectorValue` and the Amount expression are specified in text.md (Expression selector) — not repeated here.
- **Priority:** P1
- **Owner:** bae-anim (+ bae-text)
- **Notes:** Text animator and selector semantics are in text.md. Lottie: text data `t.d` (document keyframes), `t.a[]` animators with `s` selector and `a` properties; lottie-web exposes `text.sourceText` and the expression selector.

## Text: Source Text and text styles

### AE-ANI-471 · Source Text value in expressions (源文本表达式取值)

- **What it does:** Defines what `text.sourceText` reads as and what a Source Text expression may return.
- **Exact semantics:**
  - Reading: `text.sourceText` / `value` on Source Text is the text string at `time` (JS engine: a String-like value; `text.sourceText.value` is always a String; character access needs `.value[0]` in the JS engine, AE-ANI-401). `text.sourceText.length` / string methods: UNVERIFIED on the property object, always valid on `.value`.
  - Line breaks are `"\r"` in AE text (UNVERIFIED for `\n` input: both are accepted when returned; AE stores carriage returns).
  - Read-only flags (AE 25.0): `isPointText`, `isParagraphText`, `isHorizontalText`, `isVerticalText` (Booleans).
  - Writing (expression result on Source Text):
  ```
  String / Number / Array / Boolean -> ToString(result) becomes the text;
        the character style of character 0 of the pre-expression text at `time` applies to ALL characters
        (per-character styling of the keyframed text is lost), paragraph settings of paragraph 0 apply to all (UNVERIFIED)
  TextStyle object (AE 17.0+) -> text = the style's text if it carries one (from setText/replaceText or a style read from a
        Source Text property), else the pre-expression text (UNVERIFIED for createStyle() without setText); styling = the style object
  Property object -> its value (JS engine), "[object Property]" in Legacy
  undefined -> error
  ```
  - Source Text keyframes are Hold-only; `valueAtTime` returns the text at that time.
- **Priority:** P0 (text expressions such as counters and timecode are basic AE usage)
- **Owner:** bae-anim (+ bae-text)
- **Notes:** TextDocument data model and source text keyframes are in text.md. Lottie: Source Text is `t.d.k[]` keyframes of `{s: TextDocument, t}`; lottie-web's Source Text expression returns a string applied with the keyframe's style.

### AE-ANI-472 · TextStyle read attributes (文本样式读取属性)

- **What it does:** Reads character and paragraph style values through `text.sourceText.style` (AE 17.0) or `getStyleAt()`.
- **Exact semantics:**
  - Access: `text.sourceText.style` ≡ `thisProperty.style` (on Source Text) ≡ `style` (bare, on Source Text) ≡ `text.sourceText.getStyleAt(0, 0)` per the docs (note the documented equivalence uses time 0, UNVERIFIED vs current time). Other layers: `thisComp.layer("X").text.sourceText.style`.
  - Character attributes (AE 17.0 unless noted):

  | Attribute | Type | Values |
  |---|---|---|
  | `font` | String | PostScript/"system" font name (e.g. `"ArialMT"`) |
  | `fontSize` | Number | px |
  | `isFauxBold`, `isFauxItalic`, `isAllCaps`, `isSmallCaps` | Boolean | |
  | `tracking` | Number | 1/1000 em |
  | `leading` | Number | px (meaningful when `isAutoLeading` false) |
  | `isAutoLeading` | Boolean | |
  | `baselineShift` | Number | px |
  | `applyFill`, `applyStroke` | Boolean | |
  | `fillColor`, `strokeColor` | Array[3] | RGB 0–1 |
  | `strokeWidth` | Number | px |
  | `horizontalScaling`, `verticalScaling` (AE 25.0) | Number | scale factor (1 = 100%, UNVERIFIED unit) |
  | `baselineDirection` (25.0) | String | `"default"`, `"rotated"`, `"tate-chuu-yoko"` |
  | `baselineOption` (25.0) | String | `"default"`, `"subscript"`, `"superscript"` |
  | `digitSet` (25.0) | String | `"default"`, `"hindidigits"` |
  | `isLigature` (25.0) | Boolean | |
  | `kerning` (25.0) | Number, read-only | manual kerning value; 0 unless kerning type is manual |
  | `kerningType` (25.0) | String, read-only | `"manual"`, `"metrics"`, `"optical"` |
  | `lineJoin` (25.0) | String | `"bevel"`, `"miter"`, `"round"` |
  | `tsume` (25.0) | Number | 0–1 |

  - Paragraph attributes (AE 25.0; value of the first paragraph):

  | Attribute | Type | Values |
  |---|---|---|
  | `direction` | String | `"left-to-right"`, `"right-to-left"` |
  | `justification` | String | `"alignLeft"`, `"alignCenter"`, `"alignRight"`, `"justifyLastLeft"`, `"justifyLastCenter"`, `"justifyLastRight"`, `"justifyFull"` (left/right swap under RTL) |
  | `isEveryLineComposer` | Boolean | Every-line vs Single-line composer |
  | `isHangingRoman` | Boolean | Roman hanging punctuation |
  | `firstLineIndent`, `leftMargin`, `rightMargin`, `spaceBefore`, `spaceAfter` | Number | px |
  | `leadingType` | String | `"top-to-top"`, `"bottom-to-bottom"` |
- **Priority:** P1
- **Owner:** bae-anim (+ bae-text)
- **Notes:** Character/Paragraph panel semantics are in text.md. BAE font names: BAE must resolve AE PostScript names to its font database (bae-text); unknown fonts fall back and raise a missing-font warning, not an expression error (UNVERIFIED AE behaviour: AE substitutes a fallback font).

### AE-ANI-473 · TextStyle setters, chaining, setText(), createStyle(), getStyleAt() (文本样式设置方法)

- **What it does:** Builds a styled text result in an expression: every setter returns a new TextStyle so calls chain, and returning the TextStyle from Source Text applies it.
- **Exact semantics:**

  | Method | Parameters (中文) | Since |
  |---|---|---|
  | `text.sourceText.getStyleAt(charIndex, t)` | charIndex (字符索引): Number, 0-based (spaces and line breaks count); t (时间): Number, default `time` | 17.0 |
  | `text.sourceText.createStyle()` | — returns an empty TextStyle | 17.0 |
  | `style.setText(value)` | value (文本): String | 17.0 |
  | `setFont(value)` | String (PostScript name; insert via the expression menu Text > Font… dialog) | 17.0 |
  | `setFontSize(value)` | Number px | 17.0 |
  | `setFauxBold(value)`, `setFauxItalic(value)`, `setAllCaps(value)`, `setSmallCaps(value)` | Boolean | 17.0 |
  | `setTracking(value)` | Number | 17.0 |
  | `setLeading(value)` | Number px (visible only when auto leading is off) | 17.0 |
  | `setAutoLeading(value)` | Boolean | 17.0 |
  | `setBaselineShift(value)` | Number px | 17.0 |
  | `setApplyFill(value)`, `setApplyStroke(value)` | Boolean | 17.0 |
  | `setFillColor(value)`, `setStrokeColor(value)` | Array [R, G, B] 0–1 (fill visible only with applyFill; stroke needs applyStroke and width > 0) | 17.0 |
  | `setStrokeWidth(value)` | Number px | 17.0 |
  | `setHorizontalScaling(value)`, `setVerticalScaling(value)` | Number | 25.0 |
  | `setBaselineDirection(value)`, `setBaselineOption(value)`, `setDigitSet(value)`, `setLigature(value)`, `setLineJoin(value)`, `setTsume(value)` | enum strings / Boolean / Number as in AE-ANI-472 | 25.0 |
  | `setKerningType(value)` | `"metrics"` or `"optical"` (`"manual"` invalid) | 25.0 |
  | `setKerning(value, characterIndex)` | value: Number; characterIndex (字符索引): Number — sets manual kerning before that character | 25.0 |

  ```
  every setter: returns a NEW TextStyle (immutable chaining); does not modify the property directly
  style = text.sourceText.style            // carries the pre-expression text and styles at `time`
  style.setFont("Times New Roman").setFontSize(42).setText("New Text")   // result of a Source Text expression
  combine another layer's text with its style at time: s = other.getStyleAt(0, time); s.setText(other.value)
  ```
  - Setter values out of range: clamped to the Character panel ranges (UNVERIFIED; BAE: clamp, e.g. font size 0.1–1296 px per text.md).
  - Fonts referenced only inside expressions are not recorded as project fonts (not synced / not listed in Resolve Fonts).
- **Priority:** P1
- **Owner:** bae-anim (+ bae-text)
- **Notes:** Implementation hint: represent TextStyle as an immutable Rust-side value (text + run list) exposed to QuickJS as an opaque class; chaining allocates only small run edits. BAE decision: font lookup happens at layout time in bae-text, keyed by PostScript name.

### AE-ANI-474 · Per-character styling and replaceText() (逐字符样式与 replaceText)

- **What it does:** Applies style setters to a character range and replaces substrings while keeping styles (AE 25.0, October 2024).
- **Exact semantics:**
  ```
  setter(value, startIndex = 0, numOfCharacters = <to end of text>)
      // available on: setFont, setFontSize, setFauxBold, setFauxItalic, setAllCaps, setSmallCaps, setTracking,
      //   setLeading, setAutoLeading, setBaselineShift, setApplyFill, setFillColor, setApplyStroke, setStrokeColor,
      //   setStrokeWidth, setHorizontalScaling, setVerticalScaling, setBaselineDirection, setBaselineOption,
      //   setDigitSet, setLigature, setLineJoin, setTsume, setKerningType
      // indexes are 0-based UTF-16 code units of the style's text (UNVERIFIED: code units vs grapheme clusters);
      // spaces and line breaks count as characters
  replaceText(value, startIndex = 0, numOfCharacters = <to end>)
      // replaces text[startIndex, startIndex+num) with value; new characters inherit the style of the replaced range's
      // first character (UNVERIFIED); returns TextStyle
  example: "Old Text" -> style.replaceText("New", 0, 4) -> "NewText"
  later calls override earlier ones on overlapping ranges, e.g.
      style.setFont("Montserrat-Light").setFont("Gigantic", 0, 6).setFont("Gigantic", 10, 6)
  ```
  - Range beyond the text end is clipped (UNVERIFIED); startIndex < 0: error (UNVERIFIED).
  - Text reflows (line breaks, paragraph layout) after per-character changes exactly as with Character-panel substring styling.
- **Priority:** P2
- **Owner:** bae-anim (+ bae-text)
- **Notes:** BAE text runs use byte offsets internally; the JS API must convert from UTF-16 indexes.

### AE-ANI-475 · Paragraph style setters (段落样式设置方法)

- **What it does:** Sets paragraph-level attributes for all paragraphs from an expression (AE 25.0).
- **Exact semantics:**

  | Method | Parameter (中文) | Type / values |
  |---|---|---|
  | `setDirection(value)` | value (方向) | `"left-to-right"` / `"right-to-left"` |
  | `setJustification(value)` | value (对齐) | enum of AE-ANI-472 |
  | `setEveryLineComposer(value)` | value | Boolean |
  | `setHangingRoman(value)` | value | Boolean |
  | `setFirstLineIndent(value)`, `setLeftMargin(value)`, `setRightMargin(value)`, `setSpaceBefore(value)`, `setSpaceAfter(value)` | value | Number px |
  | `setLeadingType(value)` | value | `"top-to-top"` / `"bottom-to-bottom"` |

  - No range arguments: applies to every paragraph (UNVERIFIED).
  - Must be called AFTER `setText()` when both are used in a chain (setText resets paragraph attributes).
- **Priority:** P2
- **Owner:** bae-anim (+ bae-text)
- **Notes:** Paragraph semantics in text.md (Paragraph panel).

## Expression controls as inputs

### AE-ANI-476 · Reading Expression Control effects (读取表达式控制)

- **What it does:** Defines the value each Expression Control returns when read from an expression; the controls themselves (parameters, ranges, UI) are specified in effects.md, Expression Controls.
- **Exact semantics:**

  | Control (中文) | Typical access | Value read |
  |---|---|---|
  | Slider Control (滑块控制) | `effect("Slider Control")("Slider")` | Number (no clamping beyond the control's hard range) |
  | Angle Control (角度控制) | `effect("Angle Control")("Angle")` | Number, degrees (revolutions × 360 + degrees) |
  | Checkbox Control (复选框控制) | `effect("Checkbox Control")("Checkbox")` | Number 1 (on) / 0 (off); truthy in `if` |
  | Color Control (颜色控制) | `effect("Color Control")("Color")` | Array[4] RGBA 0–1 (alpha 1) |
  | Point Control (点控制) | `effect("Point Control")("Point")` | Array[2], layer space of the layer carrying the effect |
  | 3D Point Control (3D 点控制) | `effect("3D Point Control")("3D Point")` | Array[3], layer space |
  | Layer Control (图层控制) | `effect("Layer Control")("Layer")` | a Layer object (the chosen layer in the same comp); "None" → error when its members are used (UNVERIFIED: returns null) |
  | Dropdown Menu Control (下拉菜单控制) | `effect("Dropdown Menu Control")("Menu")` | Number: 1-based index of the selected item (AE 17.0+) |

  - Controls are ordinary animatable properties: they can be keyframed, can carry their own expressions, and are read post-expression (AE-ANI-408).
  - Point/3D Point values are layer space of their own layer; convert with `toComp()` of that layer when used on another layer.
  - Layer Control references survive layer renames and reordering (stored by layer ID).
- **Priority:** P0 (rigs and Essential Graphics templates depend on them)
- **Owner:** bae-anim (+ bae-effects)
- **Notes:** Lottie: expression controls export as effects `ty` 0 (slider), 1 (angle), 2 (color), 3 (point), 4 (checkbox), 7 (dropdown), 10 (layer) with lottie-web support for reading them (UNVERIFIED type numbers; see effects.md). Essential Graphics exposure is in ui-preview-render-io.md, Essential Graphics.

### AE-ANI-477 · Dropdown menu strings: items, text, textAtTime() (下拉菜单文本)

- **What it does:** Reads the item strings of any dropdown-menu property (Dropdown Menu Control and built-in effect/layer menus). AE 26.0 (January 2026), JavaScript engine only.
- **Exact semantics:**

  | Member | Parameter (中文) | Returns |
  |---|---|---|
  | `menuProp.value` | — | Number, 1-based selected index (all versions) |
  | `menuProp.items` | — | Array of Strings, all menu entries in order |
  | `menuProp.text` | — | String of the selected entry at `time` |
  | `menuProp.textAtTime(t)` | t (时间): Number | String of the selected entry at t |

  - Works on built-in menus via match names, e.g. `effect("ADBE Fractal Noise")("ADBE Fractal Noise-0001").text` = current Fractal Type name.
  - Strings are localized for built-in menus (Adobe recommends using the index for logic). Layer Control menus are not supported (use the returned layer's `.name`).
  - Divider entries: UNVERIFIED whether included in `items`.
- **Priority:** P2
- **Owner:** bae-anim (+ bae-effects)
- **Notes:** BAE: built-in menu strings come from the i18n tables of each effect; `items` returns the strings of the current UI language, like AE.

## Runtime environment

### AE-ANI-478 · JavaScript standard library and sandbox (JavaScript 标准库与沙箱)

- **What it does:** Defines which JavaScript facilities an expression can use beyond the AE-specific objects.
- **Exact semantics:**
  - Available: all ECMAScript 2018 built-ins (`Math`, `JSON`, `Array`, `String`, `Number`, `Object`, `RegExp`, `Date`, `Map`, `Set`, typed arrays, `Promise` objects without an event loop), `eval()` (with the restrictions of AE-ANI-466), `$.build`, `$.engineName`, `$.global` (AE-ANI-401).
  - Not available: `require`/`import`, file system, network, timers (`setTimeout`), `console` output (UNVERIFIED: AE has no console for expressions), DOM/browser objects, ExtendScript `File`/`Folder`/`app` scripting DOM (expressions cannot call the scripting API).
  - `Date` returns wall-clock time of the evaluating machine (non-deterministic); `Date.toLocaleString()` works (AE 16.1.2 fixed a crash).
  - Expressions cannot modify any project data; all object attributes are read-only.
  - Infinite loops: AE hangs/needs to be interrupted (UNVERIFIED; no documented timeout).
- **Priority:** P1
- **Owner:** bae-anim
- **Notes:** BAE decision: QuickJS context with only ECMAScript built-ins, no `std`/`os` modules, `console.log` routed to the expression error/info panel (BAE extension, harmless); per-evaluation interrupt handler with a time budget (e.g. 50 ms default, configurable) that turns runaway loops into an expression error instead of a hang; memory limit per runtime. `Date`/`Math.random` usage marks the expression non-cacheable (AE-ANI-412).

### AE-ANI-479 · Essential Properties access (基本属性访问（译名待核）)

- **What it does:** Reads the Essential Properties (formerly Master Properties, AE 16.1+ / 2019) exposed by a precomp layer, so expressions in the parent comp can use per-instance overrides.
- **Exact semantics:**
  - Access through the precomp layer's Essential Properties group, e.g. `thisComp.layer("Precomp 1").essentialProperty("Title Color")` (UNVERIFIED method name; call syntax `layer("ADBE Layer Overrides")("Title Color")` also resolves, match name UNVERIFIED).
  - Value = the override value on that layer instance if set, else the value of the source property inside the precomp (post-expression) at the mapped time (UNVERIFIED).
  - AE 15.1.2 fixed wrong evaluation when several master properties share a name.
- **Priority:** P2
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Essential Graphics / Essential Properties UI and data are specified in ui-preview-render-io.md (Essential Graphics) and project-comp-layers.md (layer attributes). Verify the exact access syntax in AE 26 with the pick whip before implementation.
