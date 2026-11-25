# BAE Plugin Architecture

| | |
|---|---|
| Status | **Design v0.1**, reconciled with the merged `bae-model` (02) and `bae-render` (04) interfaces; implemented parts are marked *(implemented)*. Normative for 02, 04, 07 and 20. |
| Owner | 19 `core/plugins` |
| ABI | `sdk/c/bae_plugin.h`, **ABI 1.0** (stable since M1; the 0.1 pre-release is still accepted, §14.5) |
| Rust | `crates/bae-plugin-sdk` (shared contract + author SDK), `crates/bae-plugin` (host) |
| Related | `docs/ARCHITECTURE.md` §2, §14, §15, §16; `docs/spec/aex-compat.md` (20) |

The key words MUST, MUST NOT, SHOULD, MAY are used as in RFC 2119. "Host" is the BAE
application (or `bae --headless`, or `bae-plugin-runner` in out-of-process mode).

---

## Contents

- §0 Summary
- §1 Goals, non-goals, principles
- §2 Terminology
- §3 Plugin kinds
- §4 Implementation forms
- §5 Package layout and manifest
- §6 Discovery, installation, versioning, dependencies
- §7 Runtime lifecycle (load / unload)
- §8 Effect lifecycle
- §9 After Effects mapping
- §10 Render-graph contract (for 04)
- §11 Declarative WGSL effects
- §12 Native GPU interop
- §13 Threading model
- §14 C ABI: suites, vtables, compatibility policy
- §15 Out-of-process mode
- §16 Errors, crashes, diagnostics
- §17 Script plugins
- §18 Other contribution kinds
- §19 Built-in features through the plugin path (dogfooding)
- §20 Missing plugins and placeholders
- §21 Trust, permissions, sandboxing
- §22 Performance budget and acceptance tests
- §23 Requests to other workstreams (what 01, 02, 04 and others must provide)
- §24 Open questions and decision log

---

## 0. Summary

- One extension model for built-in and third-party code. Built-in effects, importers,
  commands and panels register through the same registries, descriptors and render
  nodes as plugins. There is no second-class path.
- Three plugin forms:
  1. **Declarative WGSL package** – manifest + WGSL + parameter table. No native code.
     Executed by the same generic shader node as built-in WGSL effects: zero extra cost.
  2. **Native plugin** – a `.dylib`/`.dll` exposing one C symbol, versioned *suites*
     (host services) and *vtables* (plugin callbacks), capability negotiation, optional
     raw Metal/D3D12/Vulkan interop, optional out-of-process isolation.
  3. **Script plugin** – JavaScript on QuickJS (shared with the expression engine),
     AE-like object model, mutates the project only through commands, sandboxed by
     declared permissions. Never runs per frame.
- The effect lifecycle maps 1:1 onto After Effects' `EffectMain` command selectors so the
  future `bae-aex-host` (20) is a translation layer, not a second effect system.
- The render-graph contract (ROI in → result rect out, dependencies on other layers and
  times, threading, bit depths, GPU vs CPU, pixel-layout conversion, downsample/PAR/time)
  is defined here and implemented by 04.
- Performance rules: a declarative plugin costs < 2 % over the equivalent built-in
  (target: 0, same code path); a native call costs nanoseconds; CPU round trips and
  out-of-process isolation are paid only by the node that needs them.

---

## 1. Goals, non-goals, principles

### 1.1 Goals

1. Everything listed in ARCHITECTURE §14 is extensible: effects/generators/transitions,
   shape operators, text selectors (later), importers/exporters/codecs, commands,
   panels/workspaces, expression function libraries, automation scripts.
2. Built-in features use the plugin interfaces (dogfooding, §19).
3. Plugins load and unload at runtime without restarting and without leaking GPU
   resources (§7).
4. A native plugin compiled against an older SDK keeps running on a newer host (§14.6).
5. The interfaces leave room for running AE effect plugins later (§9, ARCHITECTURE §15).

### 1.2 Non-goals (this design)

- Marketplace, signing, payment (later). Plugin licensing policy (undecided, §24).
- Running `.aex`/`.plugin` AE binaries (20, P3).
- WASM plugins: evaluated later as a sandboxed CPU form; the contracts below are
  form-agnostic so a WASM form can be added as a fourth implementation.

### 1.3 Principles

- **P1 Zero cost when unused.** No plugin installed → no extra work on any hot path.
  Registry lookups happen when the render graph is built, never per pixel/per frame.
- **P2 Pay only where used.** CPU round trips, native GPU synchronization, out-of-process
  IPC and permission checks cost only the node/call that uses them.
- **P3 Same path.** A declarative effect and a built-in WGSL effect are the *same node
  type* with different data. A built-in Rust effect and a native plugin effect implement
  the *same trait* (`bae_render::Effect`, owned by 04); the native one adds an FFI hop.
- **P4 Commands only.** Plugins never mutate the document directly. Edits are command
  invocations or ops recorded into the current transaction (ARCHITECTURE §2, principle 1).
- **P5 Declarative first.** Anything describable as data (effect metadata, params,
  passes, ROI, workspaces, keymaps) is data in the manifest, so it can be listed,
  validated and cached without loading code.
- **P6 Explicit contracts.** Every callback states its thread, the lifetime of every
  pointer, what it may mutate and its AE equivalent.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| Plugin / package | A directory (or `.baeplugin` zip) with `bae-plugin.toml` at its root. One plugin id, one version. |
| Contribution | One extension a plugin provides, e.g. one effect, one command. A plugin has 1..N. |
| Kind | The contribution type: `effect`, `shape_operator`, `importer`, … (§3). |
| Form / implementation | How a contribution is implemented: `wgsl`, `native`, `script`, or (built-ins only) `rust`. |
| Full id | `<plugin-id>/<contribution-id>`, e.g. `com.example.glow/glow`. Stored in documents. |
| Registry | A host table of contributions of one kind (effects, commands, …). Owned by 01/02/03/04/05/08/11. |
| Owner | Who registered an entry: `Builtin(module)` or `Plugin(plugin-id)`. Used for bulk unregistration. |
| Global | Per-contribution, per-process plugin object (`void* global`; AE "global data"). |
| Edit instance | Per effect-instance object on the model thread, mutable (AE "sequence data", UI copy). |
| Render instance | Immutable per effect-instance object used by render threads (AE MFR flattened copy). |
| State blob | Serialized instance state stored in the document (AE flattened sequence data). |
| Suite | Host-provided table of C functions acquired by `(name, version)`; frozen once released. |
| VTable | Plugin-provided table of callbacks; extensible by `struct_size`. |
| Render pixel space | Full-resolution layer pixel coordinates × downsample factor; origin = layer top-left, y down. |

Naming rule (ARCHITECTURE §3): internal Rust modules registered at startup are
*app modules* (`AppModule`); user-facing extensions are *plugins*. Code never mixes the two.

---

## 3. Plugin kinds

| Kind (manifest table) | Forms | Registry target (owner WS) | Runs on | Native vtable |
|---|---|---|---|---|
| `[[effect]]` class `filter` / `generator` / `transition` | wgsl, native, rust | effect registry + effect schema registry (04 + 02) | render threads (render side), model thread (edit side) | `BaeEffectVTable1` |
| `[[shape_operator]]` | native, rust | shape-operator registry (05) | evaluation threads | `BaeShapeOperatorVTable1` |
| `[[text_selector]]` | reserved | text-selector registry (05) | — | reserved (`BAE_KIND_TEXT_SELECTOR`) |
| `[[importer]]` | native, script, rust | importer registry (01 AppBuilder) | model thread (via command) | `BaeImporterVTable1` |
| `[[exporter]]` | native, script, rust | exporter registry (01) | worker thread, read-only stage | `BaeExporterVTable1` |
| `[[codec]]` roles `decode` / `encode` | native, rust | codec registry (08) | media threads | `BaeDecoderVTable1` / `BaeEncoderVTable1` |
| `[[command]]` | native, script, rust | `CommandRegistry` (02) → menus, shortcuts, MCP (17) | model thread | `BaeCommandVTable1` |
| `[[panel]]` | script (declarative UI); native reserved | panel registry (11) | UI thread | reserved (`BAE_KIND_PANEL`) |
| `[[workspace]]` | data only | workspace registry (11) | UI thread | — |
| `[[keymap]]` | data only | keymap registry (11) | UI thread | — |
| `[[expression_library]]` | script (JS module), native | expression function registry (03) | expression threads | `BaeExpressionLibraryVTable1` |
| `[[automation]]` | script | registered as a command `script.<plugin-id>.<id>` (02) | script thread | — |

Notes:

- *Generators* are effects whose source input is optional; their default output extent is
  the layer bounds (AE behaviour). *Transitions* take inputs `from` (= self) and `to`, and
  have one FLOAT param flagged `transition_progress` (0..1); in the motion workspace they
  behave like AE transition effects (e.g. Linear Wipe), in the edit workspace (14) the
  sequence drives progress.
- `keymap` is an addition to the ARCHITECTURE §14 list: keymaps are pure data (needed for
  the Premiere / Resolve / FCP presets of 14) and cost nothing to support.
- A kind is registered only into its registry. A plugin may mix kinds and forms (e.g. a
  native effect plus a script panel), but has at most one native library and one script
  entry module.

---

## 4. Implementation forms

### 4.1 Responsibilities

| Responsibility | Declarative WGSL | Native (C ABI) | Script (JS) | Built-in Rust |
|---|---|---|---|---|
| Metadata (name, category, class, flags) | manifest | manifest (listing) + `global_setup` (authoritative flags) | manifest | descriptor in code |
| Parameters | manifest `[[effect.param]]` | `define_params` callback | manifest | `define_params` |
| ROI / result rect | manifest expressions (§11.7) | `pre_render` | — | `pre_render` |
| Pixels | WGSL passes compiled by the host | `render_cpu` and/or `render_gpu` (compute suite or native GPU) | never | same as native, via Rust traits |
| Instance state | none (stateless) | `instance_*` callbacks | — | `Effect::Instance` |
| Param-change logic | none | `param_changed`, `update_params_ui` | — | same callbacks |
| Edits to the project | — | via `bae.param.edit`, `bae.command` | via the script API (commands) | via the same Rust traits |
| Sandboxing | inherent: WGSL is memory-safe; no host API | none in-process (trusted code); OS process isolation optional | QuickJS + permissions | n/a |
| Hot reload | yes (file watch, dev mode) | reload = unload + load new version | yes | no |

### 4.2 Which form to choose (guidance for authors and for 07)

1. Pure GPU image math with a fixed pass structure → **declarative**.
2. Data-dependent pass structure (pyramids, iterative solvers), CPU algorithms, existing
   C/C++ code, codecs, importers → **native**. Prefer `bae.gpu.compute` (portable,
   host-encoded, no synchronization cost) over `bae.gpu.native`.
3. Automation, batch edits, small panels, expression helpers → **script**.

### 4.3 One contract, three projections

The render-graph node interface is **`bae_render::Effect`** (04, merged in PR #16):
`def()` (the `bae_model::EffectDef`), `output_bounds`, `input_roi`,
`layer_dependencies`, `mask_dependencies` and
`render(EncodeContext, EffectParams, input, output_roi)`.

Inputs beyond the effect's own input *(implemented, #286)*: `layer_dependencies` returns
`LayerDependency { layer, time, stage }` — a layer of the same comp (also the effect's
own layer, e.g. Linear Color Key's "Keep Colors"), at a comp time (other times: previous /
next frames), at `LayerStage::Source` / `Masks` / `EffectsAndMasks` (AE's layer popup);
the image is in that layer's space (no transform, opacity, visibility or track matte).
`mask_dependencies` returns `MaskSelect::One(mask prim)` / `All` of the effect's own
layer, delivered as a layer-space coverage image (alpha). `render` reads them with
`EncodeContext::effect_layer(&dep)` / `effect_mask(&select)`. Other times come from
`graph::CompFrames` (`eval::EvaluatorFrames`, passed by `Renderer::render_frame_with`;
the preview and export pass one); frames that use them are not frame-cached.
Parameter *definitions* are `bae_model::EffectDef` / `EffectRegistry` (02, merged in
PR #11). Every form ends up as these two objects:

```
bae_model::EffectDef         definition: params (AE superset), flags; registered in EffectRegistry
bae_render::Effect           implementation; registered in bae_render::EffectLibrary
```

- **Built-in Rust effects** (07) implement `bae_render::Effect` directly.
- **Declarative effects**: `bae-plugin` turns the manifest into an `EffectDef`
  (`bae_plugin::effect_def`, *(implemented)*) and a `DeclarativeEffect` that implements
  `bae_render::Effect` with the generic shader node (§11). The render graph MAY downcast
  to inspect the WGSL description for optimizations (fusion, §10.9).
- **Native plugins**: the plugin implements the C vtable (`bae_plugin.h`) directly, or
  later a Rust author trait in `bae-plugin-sdk` projected onto the vtable by
  `export_plugin!`; the host's `NativeEffect` adapter (in `bae-plugin`) implements
  `bae_render::Effect` on top of the vtable. Host services go the other way: the host
  exposes them to native plugins as the `bae.render` / `bae.gpu.*` suites.
- The render graph never needs to know which form it is running.

The richer protocol of §8 and §10 (pre-render checkouts, other times, threading
classes, CPU nodes, instance state) is the target; `bae_render::Effect` grows towards
it with default methods as 04 implements the executor (requests in §23.3).

---

## 5. Package layout and manifest

### 5.1 Package layout

```
com.example.vignette/            package root (directory name is informational)
  bae-plugin.toml                manifest (required)
  shaders/vignette.wgsl          declarative shaders
  bin/macos-aarch64/libx.dylib   native libraries, one per platform key
  bin/windows-x86_64/x.dll
  scripts/main.js                script entry module
  schemas/*.json                 command argument JSON Schemas
  locales/zh-CN.toml             translations (en is the manifest default)
  icons/*.svg                    optional icons (SVG only, own artwork)
  README.md, LICENSE
```

All paths in the manifest are relative to the package root, use `/`, MUST NOT be absolute,
MUST NOT contain `..` segments and MUST resolve inside the package.

`.baeplugin` = a zip of the package root (manifest at the archive root, or inside exactly
one top-level directory). Media/binaries may be stored uncompressed. Max 256 MiB by default.

### 5.2 Identifiers

| Identifier | Rule | Example |
|---|---|---|
| Plugin id | reverse-DNS, ≥ 2 dot-separated segments, each `[a-z0-9]([a-z0-9-]*[a-z0-9])?`, total ≤ 128 bytes. `bae.*` reserved for first-party packages. | `com.example.vignette` |
| Contribution id | `[a-z][a-z0-9_-]*`, ≤ 64 bytes, unique within the plugin and kind | `vignette` |
| Full id | `<plugin-id>/<contribution-id>` | `com.example.vignette/vignette` |
| Param key | `[a-z][a-z0-9_]*`, ≤ 64 bytes, unique per effect, not starting with `bae_` | `amount` |
| Param id | integer 1..=2³¹−1, unique per effect, never reused | `3` |
| Platform key | `<os>-<arch>`: `macos-aarch64`, `macos-x86_64`, `windows-x86_64`, `windows-aarch64`, `linux-x86_64`, `linux-aarch64` (headless/CI); `ios-aarch64`, `android-aarch64` reserved | |

Plugin ids and contribution ids are case-sensitive ASCII and are what documents store;
they MUST NOT change across versions. Renames use `replaces` (§5.3.1).

### 5.3 Manifest schema (`bae-plugin.toml`, `schema_version = 1`)

Unknown keys are an **error** in tables defined here (catches typos) except inside
`[x]` / `[x.*]` (reserved for vendor extensions, ignored by the host).

#### 5.3.1 Top level and `[plugin]`

```toml
schema_version = 1                       # required; manifest format version

[plugin]
id = "com.example.vignette"              # required, §5.2
version = "1.2.0"                        # required, semver 2.0
api = "0.1"                              # required: minimum BAE plugin API (major.minor), §6.6
name = "Vignette"                        # required, default (English) display name
description = "Darkens the edges of a layer."
authors = ["Example Studio <dev@example.com>"]
license = "Apache-2.0"                   # SPDX expression, informational
homepage = "https://example.com/vignette"
host = ">=0.3, <2"                       # optional semver req on the BAE app version
platforms = ["macos-aarch64", "windows-x86_64"]  # optional restriction; default: all that the forms allow
replaces = ["com.example.old-vignette"]  # optional: ids this plugin supersedes (doc migration)
icon = "icons/vignette.svg"              # optional
```

`api` is the single *BAE plugin API version* covering the C ABI (`BAE_ABI_VERSION`), the
declarative shader interface (§11) and the script API (§17). It is the minimum the plugin
needs; the host accepts it iff same major and host minor ≥ plugin minor (§6.6).

#### 5.3.2 `[native]`

```toml
[native]
entry = "bae_plugin_entry"               # optional; the default is the only value allowed in API 0.x/1.x
isolation = "in-process"                 # "in-process" (default) | "out-of-process" | "prefer-out-of-process"
[native.libraries]                       # required if any contribution has implementation = "native"
macos-aarch64 = "bin/macos-aarch64/libvignette.dylib"
windows-x86_64 = "bin/windows-x86_64/vignette.dll"
```

A native plugin is available on exactly the platform keys listed in `[native.libraries]`
(intersected with `plugin.platforms` if given). `prefer-out-of-process` means: run isolated
unless the user switched the plugin to in-process in the Plugin Manager.

#### 5.3.3 `[script]`

```toml
[script]
entry = "scripts/main.js"                # ES module; required if any contribution uses implementation = "script"
memory_limit_mb = 64                     # optional, default 64, max 1024
```

#### 5.3.4 `[permissions]`

```toml
[permissions]
project = "write"                        # "none" | "read" (default for scripts) | "write"
filesystem = ["read:project", "write:plugin-data", "read:user-picked"]
network = ["https://api.example.com"]    # origins; https only (http://localhost allowed)
clipboard = "none"                       # "none" | "read" | "write" | "read-write"
process = false                          # spawn child processes
gpu_native = false                       # use bae.gpu.native (native plugins)
render = false                           # request rendered frames (scripts/commands)
```

Filesystem scopes: `project` (the project directory), `plugin-data` (always granted),
`plugin-cache` (always granted), `user-picked` (files the user picks in a host dialog;
capability-style handles), `assets` (project asset files, read only), `any` (dangerous).
Semantics and enforcement: §17.4 and §21.

#### 5.3.5 `[dependencies]`

```toml
[dependencies]
"com.example.shader-lib" = "^1.2"        # semver requirement (cargo syntax; bare "1.2" = "^1.2")
"bae.std-shaders" = { version = "^0.1", optional = true }
```

Dependencies give access to another plugin's exported resources: WGSL include modules
(§11.10), JS modules (`import "plugin:com.example.lib/x.js"`), and plugin-provided suites
(`BaePluginExports.get_suite`). Rules: §6.5.

#### 5.3.6 `[[effect]]`

```toml
[[effect]]
id = "vignette"                          # required
name = "Vignette"                        # required
category = "Stylize"                     # Effects & Presets folder (well-known English key, translated by host)
class = "filter"                         # "filter" (default) | "generator" | "transition"
implementation = "wgsl"                  # required: "wgsl" | "native"
keywords = ["edge", "darken"]            # search keywords (translations in locales)
ae_match_names = []                      # AE matchNames this effect can stand in for on AE import (15/20)
flags = ["pixel-independent"]            # see table below
threading = "concurrent"                 # native only; informational listing value, authoritative in global_setup
devices = ["gpu"]                        # native only: ["cpu"], ["gpu"], ["cpu","gpu"]

# ---- declarative only ----
shader = "shaders/vignette.wgsl"         # required for wgsl
includes = []                            # optional WGSL files concatenated before `shader` (§11.10)
features = []                            # optional WGSL features: ["f16"] (§11.2)
style = "pixel"                          # "pixel" | "compute" (§11.4); default: "compute" iff [[effect.pass]] exists
entry = "vignette"                       # pixel style: name of the pixel function; default = id with '-' → '_'
identity_when = "params.amount == 0"     # optional expression (§11.7): node is skipped when true
[effect.roi]                             # optional (§11.6)
extent = "input"                         # "input" (default for filter/transition) | "layer" (default for generator) | "infinite"
read = "0"                               # expression, full-res layer px, all sides; or read_left/read_right/read_top/read_bottom
grows = false                            # result rect grows by the mirrored read footprint

[[effect.param]]                         # declarative: authoritative; native: optional listing copy (must match)
# ... §5.3.7

[[effect.texture]]                       # compute style only: intermediates (§11.5)
name = "blur_h"                          # [a-z][a-z0-9_]*, ≤ 32 bytes; bound as bae_tex_<name>
format = "working"                       # "working" (default) | "rgba16f" | "rgba32f" | "r32f" | "rg32f" | "rgba8unorm" (portable storage formats)
rect = "output"                          # "output" (default) | "input": which rect it covers
scale = 1.0                              # reserved: API 0.1 accepts only 1.0 (reduced-resolution intermediates later)

[[effect.pass]]                          # compute style only: executed in order (§11.5)
entry = "blur_horizontal"                # @compute entry point in `shader`
reads = ["input"]                        # "input", "to" (transitions), layer-param keys, texture names
writes = "blur_h"                        # "output" or a texture name

[[effect.migration]]                     # optional, any implementation (§6.5)
from = "1"                               # semver requirement on the version that wrote the data
rename = { radius_px = "radius" }        # old param key → new key
drop = ["legacy_mode"]
defaults = { quality = 2 }               # values for params that did not exist
```

Pixel style (`style = "pixel"`) MUST NOT have `[[effect.texture]]` or `[[effect.pass]]`;
compute style MUST have ≥ 1 pass and the last pass MUST write `output`. A texture MUST be
written by an earlier pass before it is read. `entry` is invalid for compute style.

Effect `flags` (manifest spelling → C flag):

| Manifest | C | Meaning |
|---|---|---|
| `pixel-independent` | `PIXEL_INDEPENDENT` | output pixel depends only on the same input pixel |
| `expands-buffer` | `EXPANDS_BUFFER` | result may exceed input extent (implied by `grows = true`) |
| `shrinks-buffer` | `SHRINKS_BUFFER` | result may be smaller than input |
| `non-param-vary` | `NON_PARAM_VARY` | output changes over time at constant params (noise seeded by time) |
| `wide-time-input` | `WIDE_TIME_INPUT` | reads inputs or params at other times |
| `uses-shutter` | `USES_SHUTTER` | reads shutter angle/phase |
| `uses-camera` / `uses-lights` | `USES_CAMERA` / `USES_LIGHTS` | depends on the comp camera/lights |
| `non-deterministic` | `NON_DETERMINISTIC` | never cache |
| `in-place` | `IN_PLACE` | (native CPU) output may alias input |

#### 5.3.7 `[[effect.param]]`

Common keys: `id` (required), `key` (required), `type` (required), `name` (required),
`group` (key of a `group` param defined earlier), `flags` (list), `hidden`, `disabled`
(shorthands for flags). Type-specific keys:

| `type` | Keys (defaults) | Stored value | WGSL field (§11.3) |
|---|---|---|---|
| `float` | `default`(0), `range`([-1e6,1e6]), `slider`(=range), `precision`(2), `display`("number"/"percent"/"pixels"/"degrees"/"seconds"/"frames") | f64 | `f32` |
| `int` | `default`, `range`, `slider`, `display` | i32 | `i32` |
| `angle` | `default` (degrees) | f64 degrees | `f32` degrees |
| `checkbox` | `default`(false), `label` | bool | `u32` (0/1) |
| `color` | `default` [r,g,b,a] (0..1), `alpha`(false) | f32×4 straight, working space | `vec4<f32>` |
| `point` | `default` [x,y], `relative`(true: fraction of layer) | f64×2 layer px | `vec2<f32>` render px |
| `point3d` | `default` [x,y,z], `relative` | f64×3 | `vec3<f32>` |
| `popup` | `default` (1-based index, default 1), `choices = ["Circle", "Fit Layer"]` (append only) | i64 1-based index (AE) | `u32` |
| `layer` | `default` ("none"/"self") | layer ref | texture `bae_tex_<key>` + rect |
| `group` | `end_id` (required: id of the group end, AE counts it as a param), `collapsed`(false); members set `group = "<key>"` and follow it contiguously | — | — |
| `button` | `label` | — | — |
| `string` | reserved (API 0.1 has no string param kind in `bae_model`, R02-8) | UTF-8 | — |
| `asset` | reserved (R02-8) | asset ref | — |
| `arbitrary` | — (native only) | bytes | — |
| `path` | — | mask ref | — (native only) |

Param `flags` (manifest spelling): `hidden`, `disabled`, `supervise`, `not-animatable`,
`cannot-time-vary`, `start-collapsed`, `ui-only`, `no-expressions`, `transition-progress`.
The first six map to `bae_model::ParamFlags`; the last three are kept by the plugin host.
Mapping to `bae_model::ParamKind` *(implemented, `bae_plugin::effect_def`)*: float →
`Slider`, int → `Slider { integer }`, angle, checkbox, color, point, point3d, popup →
`Popup { options }`, layer, group → `GroupStart`…`GroupEnd`, button, arbitrary →
`ArbitraryData`, path. Relative point defaults depend on the layer size and are resolved
by the effect implementation, not stored in the `EffectDef` (R02-9). Numeric
`default` MUST lie in `range`; `slider` MUST lie within `range`.

#### 5.3.8 Other contribution tables

```toml
[[shape_operator]]
id = "zigzag-lite"; name = "Zig Zag Lite"; implementation = "native"

[[importer]]
id = "lottie"; name = "Lottie JSON"; implementation = "native"   # or "script"
extensions = [".json", ".lottie"]; mime = ["application/json"]; entry = "importLottie"  # script: exported function

[[exporter]]
id = "svg-frames"; name = "SVG frames"; implementation = "script"; extensions = [".svg"]; entry = "exportSvg"

[[codec]]
id = "ffmpeg"; name = "FFmpeg"; implementation = "native"; roles = ["decode", "encode"]
extensions = [".mkv", ".webm"]; priority = 10   # higher wins among codecs claiming an extension

[[command]]
id = "align-to-grid"; name = "Align Layers to Grid"; implementation = "script"; entry = "alignToGrid"
schema = "schemas/align-to-grid.json"    # JSON Schema of the arguments (required)
menu = "Layer/Align"                     # optional menu path (well-known English keys)
shortcut = { motion = "Ctrl+Alt+G" }     # optional per-workspace default
description = "Snap selected layers to an N-pixel grid."   # also the MCP tool description

[[automation]]                           # AE File > Scripts. Not [[script]]: [script] is the runtime table
id = "batch-rename"; name = "Batch Rename"; entry = "batchRename"; menu = "File/Scripts"

[[panel]]
id = "notes"; name = "Shot Notes"; implementation = "script"; entry = "notesPanel"

[[workspace]]
id = "colorist"; name = "Colorist"; layout = "workspaces/colorist.toml"

[[keymap]]
id = "premiere"; name = "Premiere Pro style"; file = "keymaps/premiere.toml"; workspace = "edit"

[[expression_library]]
id = "easing"; namespace = "easing"; implementation = "script"; entry = "scripts/easing.js"
```

Commands get the registry id `plugin.<plugin-id>.<command-id>` (dots in the plugin id are
kept), which is also their MCP tool name; automation scripts get `script.<plugin-id>.<id>`.

#### 5.3.9 `[exports]`

```toml
[exports]                                # resources dependents may use (§5.3.5)
wgsl = ["shaders/noise.wgsl"]            # includable as "plugin:<this-id>/shaders/noise.wgsl" (§11.10)
js = ["scripts/lib.js"]                  # importable as "plugin:<this-id>/scripts/lib.js" (§17.1)
```

Native plugins export suites at runtime instead (`BaePluginExports.get_suite`, §14.4).

#### 5.3.10 Localization (`locales/<bcp47>.toml`)

```toml
[plugin]
name = "暗角"
description = "使图层边缘变暗。"

[effect.vignette]
name = "暗角"
keywords = ["边缘", "压暗"]
[effect.vignette.params]
amount = "数量"
center = "中心"
[effect.vignette.choices.shape]          # popup key → 1-based index → label
1 = "圆形"
2 = "椭圆"

[command.align-to-grid]
name = "对齐到网格"
description = "把选中的图层吸附到 N 像素网格。"
```

Every contribution kind uses `[<table>.<id>]` with `name`, `description` and `keywords`
(e.g. `[command.align-to-grid]`, `[script.report-off-grid]`); effects and shape operators
add `params` and `choices`. Categories and menu paths use well-known English keys (`Stylize`, `Blur & Sharpen`, …)
that the host translates with Adobe's official Chinese terms (`docs/glossary.md`, 11).
Missing translations fall back to the manifest strings. Native plugins may also pass
`l10n_key` in `BaeParamDef` to look up `[effect.<id>.params]`.

### 5.4 Validation

`bae plugin check <dir|file>` and the loader run the same validation (in `bae-plugin`):
ids/keys syntax and uniqueness; versions parse; `api` major supported; paths safe and
existing; every `implementation = "native"` contribution has a library for at least one
platform; `[native]`/`[script]` present iff used; param ids/keys unique and
> 0; defaults within ranges; groups defined before use; declarative effects: shader
exists, passes/textures consistent (§11.5), expressions parse and reference only defined
numeric params (§11.7), and the shader compiles against the generated prelude (naga).
Errors carry `file:line:column` (TOML spans; WGSL lines mapped back past the prelude).

---

## 6. Discovery, installation, versioning, dependencies

### 6.1 Plugin directories

| Scope | macOS | Windows | Trust |
|---|---|---|---|
| Bundled (first-party) | `BAE.app/Contents/Resources/plugins/` | `<install>\plugins\` | trusted, read-only |
| System | `/Library/Application Support/BAE/Plugins/` | `%ProgramData%\BAE\Plugins\` | trusted (admin installed) |
| User | `~/Library/Application Support/BAE/Plugins/` | `%APPDATA%\BAE\Plugins\` | trusted after install consent |
| Project | `<Project>.bae/plugins/` | same | **untrusted** until the user trusts the project (§21) |

Bundled/system/user paths come from `bae-platform` (01); the project path is resolved by
19 from the open project. Extra directories can be added with `BAE_PLUGIN_PATH`
(`:`/`;`-separated, development) and `--plugin-dir`. `--safe-mode` loads bundled plugins only.

Installed layout inside a scope: `<scope>/<plugin-id>/<version>/` (the package root).
Several versions of a plugin MAY be installed side by side; at most one is **active** per
process (§6.4).

### 6.2 Scanning (fast startup)

1. At startup the host enumerates directories and reads **only manifests** (no library is
   opened, no shader compiled). Parsed manifests are cached in
   `<cache>/plugins/scan-cache.bin` keyed by (path, size, mtime) of the manifest.
2. Contributions are listed in registries as *lazy entries* from manifest data (names,
   categories, keywords, flags) so the Effects & Presets panel, menus and MCP tool lists
   work without loading code.
3. Code is loaded on first use (§7.2): an effect is applied, a project using it is
   opened, a command runs. Native `define_params` results are cached in the scan cache
   (keyed by library hash + plugin version) so later sessions show parameters without
   loading the library.
4. Budget: ≤ 0.5 ms per cached plugin, ≤ 50 ms total for 100 plugins (§22).

### 6.3 Install / uninstall / enable / disable

All four are app-level commands in `CommandRegistry` (so Agents and scripts can call
them, subject to permissions): `plugin.install {source, scope}`, `plugin.uninstall {id,
version?, scope}`, `plugin.set_enabled {id, enabled}`, `plugin.list`, and the CLI
`bae plugin install|uninstall|enable|disable|list|check|pack`.

Install: open the `.baeplugin` (zip) or directory → validate manifest (§5.4) → show
the consent dialog (permissions, native code, platforms) unless `--yes` → extract into a
temp dir in the target scope → atomic rename to `<scope>/<id>/<version>/` → rescan →
activate if it wins resolution (§6.4), hot-loading if code is needed by open projects.

Uninstall: if active, unload (§7.4) → remove the version directory (on Windows, a loaded
DLL may still be mapped; the directory is renamed to `.trash/` and deleted on next start).
Effects in open projects become placeholders (§20); no document data is changed.

Disable: like uninstall but files stay. Enabled state lives in the user settings, keyed
by plugin id (all versions).

### 6.4 Which version is active

Candidates for a plugin id are all installed, enabled, platform-compatible,
API-compatible versions in trusted scopes (plus the project scope if trusted). Selection:

1. If the open project's plugin lock (§6.7) names a version and a candidate with the same
   **major** exists, prefer the highest such candidate.
2. Otherwise pick the highest version; ties are broken by scope: project > user > system
   > bundled (the most specific scope wins, so a project can ship a fixed copy).
3. All dependency requirements of active plugins must be satisfied (§6.5); otherwise the
   resolver backtracks to the next-lower candidate, and finally reports a conflict.

Only one version per id is active per process because natively loaded code and registry
entries are keyed by id. Opening a project that needs a different major than the active
one shows a warning; its effects of that plugin become placeholders until resolved.

### 6.5 Plugin versions and data compatibility

Plugin `version` is semver; it carries a data-compatibility promise:

- Patch and minor releases MUST read every document written by older versions of the
  same major: param ids are never reused or retyped, removed params become hidden (the
  host keeps their stored values), popup choices are only appended (the document stores
  the 1-based index, as AE does), state blobs stay readable.
- A major bump MAY break this. The plugin then SHOULD provide migration: declarative
  `[[effect.migration]]` tables (`from = "1"`, `rename = {old_id = new_id}`, `drop = [...]`,
  `defaults = {...}`) or native `message(selector = "bae.migrate")` receiving the old
  values + state and returning new ones (§8.3.4).
- `replaces = [...]` lets a renamed plugin adopt documents of an older id.

Dependency resolution: requirements use cargo semver syntax; the resolver is a simple
backtracking resolver over (id → candidate versions) with at most one active version
per id; cycles are an error; optional dependencies are satisfied if present and
otherwise ignored; a missing required dependency leaves the plugin `Unresolved` (listed
with an explanation, never loaded).

### 6.6 API version negotiation

There is one plugin API version `MAJOR.MINOR` (C ABI = `BAE_ABI_VERSION`, declarative
interface, script API). Rules:

1. **Manifest check (no code loaded):** plugin `api = "A.a"`; host API `H.h`. Accept iff
   `A == H` and `a ≤ h`, or `A` is an older stable major served by a compatibility shim
   whose newest minor is `s` and `a ≤ s`. The host then *presents* the newest version
   of major `A` it implements (`H.h`, or `A.s` through a shim) as `BaeHost.abi_version`.
2. **Binary check (native, after `bae_plugin_entry`):** `abi_required` MUST equal the
   manifest `api` (stale-manifest guard); the host re-applies rule 1 to `abi_required`;
   `abi_built` is informational (it tells the host which struct sizes the plugin knows).
3. **Plugin check:** the plugin sees `BaeHost.abi_version` and MAY refuse with
   `BAE_ERR_VERSION_MISMATCH` (the SDK does this automatically when the presented
   version is a different major or an older minor than `abi_required`).
4. **Feature negotiation:** optional features are discovered at runtime:
   `get_suite(name, version)` returns NULL when unsupported; `BaeHost.capabilities` lists
   host features; extensible structs are read with `BAE_HAS_FIELD`.
5. **Majors:** a host supporting an older major `A < H` does so through a host-side
   compatibility shim that presents the old suites/vtables. Policy (§14.6): each major
   is supported for at least two years after its successor ships.

Major 0 (this draft) is exempt from "older minors keep working": a host `0.h` accepts a
plugin requiring `0.a` iff `a == h` or `a` is an older minor the host explicitly lists as
still compatible (`HostApiSupport::compatible_0x_minors`); 0.x plugins never run on a
stable (≥ 1) host. Reference implementation and tests: `bae_plugin_sdk::version`.

### 6.7 Project plugin lock

Each project records the plugins its documents use (`project.json`, owned by 02):
`[{id, version, contributions:[…], scope_hint}]`, updated when an effect is added or a
plugin is updated. The lock is advisory (selection hint §6.4, missing-plugin report,
packaging in 21 can embed declarative/script plugins into `<Project>.bae/plugins/`).

---

## 7. Runtime lifecycle (load / unload)

### 7.1 States

```
 Discovered ──resolve──► Resolved ──load──► Loaded ──init──► Active ◄─────┐
     │                      │                 │                │  reload  │
     │ invalid manifest     │ deps missing    │ load error      │ unload   │
     ▼                      ▼                 ▼                ▼          │
  Invalid               Unresolved          Failed          Draining ──► Unloaded
                                                              (leases → 0, GPU idle)
```

| State | Meaning |
|---|---|
| Discovered | Manifest parsed; lazy registry entries may exist (listing only). |
| Resolved | Dependencies satisfied and this version selected (§6.4). |
| Loaded | Code available: native library mapped and `bae_plugin_entry` succeeded; or WGSL compiled to a validated module; or JS module evaluated. |
| Active | Contributions registered with their implementation; globals set up lazily. |
| Draining | Unregistered for new uses; in-flight work holds leases; waiting. |
| Unloaded | All callbacks returned, GPU work referencing it completed, `shutdown` called, library closed. |
| Invalid / Unresolved / Failed | Listed in the Plugin Manager with diagnostics; never executed. |

### 7.2 Load sequence (native)

1. Pre-checks from the manifest (§6.6 rule 1, platform, trust).
2. `libloading::Library::new` (macOS: `dlopen(RTLD_NOW | RTLD_LOCAL)`; Windows:
   `LoadLibraryExW` with `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`
   so the plugin's own directory is searched for its dependencies).
3. Resolve `bae_plugin_entry`; call it on the plugin thread with a `BaeHost`.
4. Validate exports: `plugin_id`/`plugin_version` equal the manifest; §6.6 rule 2;
   every manifest contribution of `implementation = "native"` has exactly one exported
   contribution with the same kind and id, and vice versa; vtable `struct_size` ≥ the
   size of the required prefix.
5. Register implementations (replacing the lazy entries). For each effect, on first use:
   `global_setup` → description validated (flags, devices, formats) → `define_params`
   → schema registered with 02 → (per GPU device, lazily on first GPU render)
   `gpu_device_setup`.

Declarative: read shader → generate prelude (§11) → compile with naga → validate → register.
Pipelines are created per GPU device on first use and stored in the host pipeline cache.

### 7.3 Leases

Every use of a plugin's code holds a lease: an `Arc<LoadedPlugin>` clone captured when a
render graph is built, when a command executes, etc. Render graphs resolve registry
entries **once at build time** and keep the `Arc`s, so there is no registry lookup or
lock per frame. A lease prevents unload of the code it points to, not unregistration.

### 7.4 Unload protocol

Triggered by uninstall, disable, update (load new + unload old), dev hot-reload,
crash-quarantine, or app exit.

1. **Unregister** all contributions of the plugin from all registries (one
   `remove_owner(Plugin(id))` call per registry). From now on, new render graphs treat
   its effects as placeholders (§20); menus/MCP tools disappear; UI is notified.
2. **Invalidate** render-graph caches that referenced it (04 rebuilds affected graphs;
   cached frames produced by the plugin are kept only if the plugin is merely being
   disabled, dropped if it is being replaced by another version).
3. **Drain:** set the abort flag of every in-flight call that supports aborting
   (`is_aborted()` → true); wait until the lease count reaches zero (bounded wait,
   default 5 s; then the plugin is marked *stuck*, the remaining steps run at exit).
4. **Tear down** on the plugin thread, in reverse creation order: render instances
   (`instance_destroy`), edit instances (`instance_serialize` is *not* needed: the state
   blob in the document is already current), frame data, `gpu_device_setdown` per
   device, `global_setdown` per contribution, then `shutdown`.
5. **GPU cleanup:** destroy host-owned GPU objects created for the plugin, tracked in its
   `GpuResourceLedger` (pipelines from `create_pipeline`, uniform-ring allocations,
   pooled temporaries still attributed to it, declarative pipelines/bind groups). Then
   wait until the GPU has passed a fence submitted after the plugin's last command buffer
   (completion handlers registered by the plugin must not run after its code is unmapped).
   The ledger MUST be empty afterwards; a non-empty ledger is a bug reported in debug
   builds and tests ("no GPU leak" acceptance test, §22.3).
6. **Unmap** the library (`dlclose`/`FreeLibrary`). Physical unmapping is best effort:
   macOS does not unmap images that registered thread-local variables (typical for Rust
   cdylibs) — logical unload is guaranteed regardless. Reloading the same id loads the
   new version from its own versioned directory, so stale code can never be reused.

Declarative/script unload = steps 1–5 (no library). Update = load new version (Loaded),
then swap registry entries atomically (`replace_owner_entries`), then unload the old one
— open projects switch at the next graph rebuild without a gap.

### 7.5 Hot reload (development)

With `--plugin-dev <dir>` (or a toggle in the Plugin Manager) the host watches the package:
declarative/script changes recompile and swap between frames; a compile error keeps the
previous version running and shows the diagnostics; native changes trigger unload +
load from a fresh copy of the library (copied to a temp path so the build can overwrite
the original on Windows).


---

## 8. Effect lifecycle

This section is the plugin-side contract of an effect. §9 maps it onto After Effects,
§10 states what the render graph (04) does around it. Thread letters: **L** plugin/load
thread, **M** model thread (owns the Stage, runs commands; may be the main thread),
**R** render worker, **G** GPU setup (a render worker holding the effect's setup lock).
Exact rules per thread: §13.

### 8.1 Objects and lifetimes

| Object | Created by | Destroyed by | Thread | Cardinality |
|---|---|---|---|---|
| global (`void* global`) | `global_setup`, on first use of the effect in the process | `global_setdown` during unload (§7.4) | L | 1 per effect contribution per process |
| parameter schema | `define_params`, right after `global_setup` (result cached, §6.2) | — | L | 1 per effect per plugin version |
| GPU data | `gpu_device_setup`, lazily before the first `render_gpu` on a device | `gpu_device_setdown` (unload, device lost, device removed) | G | 1 per effect per GPU device |
| edit instance | lazily, when the model needs instance logic (§8.5, §8.6, §8.9) | effect Prim deleted, project closed, state replaced remotely, unload | M | ≤ 1 per effect Prim per open Stage |
| render instance | when a render graph references the effect Prim and no render instance exists for its current state revision | when no graph and no in-flight frame references it | R | 1 per (effect Prim, state revision) in use |
| frame data | `pre_render` (`out_result->frame_data`) | `release_frame_data`, always, also after abort/error/skip | R | per frame request |

Effects without `BAE_EFFECT_FLAG_STATEFUL` have **no instances**: `instance_*` callbacks are
never called and every `instance` argument is NULL. This is the default and costs nothing.

### 8.2 Call sequence

```
load (L)            bae_plugin_entry
first use (L)       global_setup → define_params → schema registered with 02 (§8.4)
apply effect (M)    [STATEFUL] instance_create(EDIT, state = empty) → instance_serialize
                    → the AddEffect command stores the state blob (one undo step)
open project (M)    edit instances are created lazily: instance_create(EDIT, state = blob)
build graph (M/R)   [DYNAMIC_FLAGS] query_dynamic_flags when non-animated values change
                    [STATEFUL] instance_create(RENDER, state = blob) once per state revision
per frame (R)       pre_render → render_gpu | render_cpu → release_frame_data
first GPU use (G)   gpu_device_setup (per device, before the first render_gpu there)
user edit (M)       [SUPERVISE] param_changed → optional bae.param.edit ops in the same command
                    → if BAE_CHANGED_STATE: instance_serialize → new blob (same command)
UI refresh (M)      [SEND_UPDATE_PARAMS_UI] update_params_ui (UI state only)
copy/duplicate (M)  commands copy the blob; new instances are created from it
remote change (M)   blob replaced by sync → edit instance destroyed + recreated, render instances rebuilt
delete effect (M)   instance_destroy(edit); render instances dropped with their graphs
unload (L/R/G)      §7.4: instance_destroy … gpu_device_setdown … global_setdown … shutdown
```

### 8.3 Instance state (AE sequence data)

1. The **state blob** in the document is the single source of truth (02: opaque bytes on
   the effect Prim, plus `state_plugin_version`). Instances are caches of it; saving a
   project never calls the plugin.
2. **Edit instances** are mutable and live only on M. **Render instances** are created
   from the same blob, are immutable after `instance_create` returns and are shared by all
   render threads (this is AE's Multi-Frame-Rendering model: flattened sequence data
   marshalled to render threads). A plugin that wants a render-side cache keeps it inside
   the render instance with its own synchronization; the cache MUST NOT change output.
3. The state changes **only** on M: `param_changed` or `message` return/report
   `BAE_CHANGED_STATE`; the host then calls `instance_serialize` on the edit instance and
   writes the blob as an op of the *same* command (undoable, synced). New render instances
   are created for the new revision; old ones die when unused.
4. `instance_serialize` is non-destructive (AE `GET_FLATTENED_SEQUENCE_DATA` semantics):
   the instance stays valid and unchanged.
5. Format: plugin-defined, MUST be byte-order and platform independent and SHOULD carry its
   own version tag. The host never interprets it. Soft limit 1 MiB (warning: blobs are
   synced through the CRDT), hard limit 16 MiB.
6. `instance_create` receives `state_plugin_version`, the plugin version that wrote the
   blob; a plugin that cannot read it returns `BAE_ERR_PARSE` → the effect becomes a
   placeholder with diagnostics (§20), data untouched.
7. Determinism: same params + same state + same inputs ⇒ same pixels, unless the effect
   declares `NON_DETERMINISTIC` (then nothing it produces is cached).

#### 8.3.4 Migration across plugin majors

When a document written by plugin major `N` is opened with major `M > N`, the host first
applies manifest `[[effect.migration]]` tables whose `from` requirement matches the
writer version (rename / drop / defaults on param keys), then, for native effects that
handle it, sends `message("bae.migrate")` on M with input
`{"from_version": "...", "params": [{"id", "key", "value"}...], "state_b64": "..."}` and
expects the same shape back for the current version. The result is applied as one
command ("Update effect to version …", undoable). If neither exists, major mismatches
stay placeholders (§20 item 5).

### 8.4 Parameters

- `define_params` is called once per process (L) and MUST depend only on the plugin
  version and locale-independent data (it is cached). Display names may be localized via
  `l10n_key`; dynamic UI changes go through `update_params_ui`.
- The host converts the definitions into an **effect schema** registered with 02's schema
  registry under (full id, plugin version): stable numeric ids (AE disk ids), keys, types,
  defaults, ranges, popup choices, group structure, flags.
- The document stores values **by param id**. Keys are what WGSL (§11.3), scripts and
  expressions use. Expressions (03) may address a param by key, by its default English
  name or by 1-based definition index (AE `effect("Blur")(1)` semantics).
- `BaeParamValues` has one entry per defined param **in definition order, including GROUP
  and BUTTON entries** (their value is meaningless), so index `i` is always the `i`-th
  definition and AE-style index access is O(1).
- Value semantics: FLOAT/ANGLE are f64 (angles in degrees, unbounded); INT i32; COLOR is
  straight RGBA f32 in the working color space (§6 of ARCHITECTURE, 06 decides storage);
  POINT/POINT_3D are in **render pixel space** (downsample applied) during pre_render/
  render, and in full-resolution layer pixels in M callbacks; POPUP is the stable choice
  id, never an index; LAYER is a host handle (0 = none); STRING/ARBITRARY are borrowed.
- Removed or unknown params: values of ids the current definition does not know are kept
  by the document and never passed to the plugin (forward/backward compatibility, §6.5).

### 8.5 Supervision: `param_changed` and `update_params_ui`

- `param_changed` runs for params with `SUPERVISE` (and every BUTTON) when a **user-level
  change** is applied through a command: UI, scripts, MCP. It runs *inside* that command:
  values set through `bae.param.edit.set_value` become ops in the same transaction (one
  undo step) and do not re-trigger supervision.
- It is **not** called for: time changes (keyframe interpolation), undo/redo (ops are
  replayed), and remote changes arriving through sync (the originating client already ran
  it and its resulting ops are synced). This keeps collaboration deterministic.
- Budget: < 1 ms typical; the host logs a warning above 16 ms (it blocks M).
- `update_params_ui` (needs `SEND_UPDATE_PARAMS_UI`) is called when the Effect Controls UI
  shows the effect and after value changes, at most once per UI frame. It may only call
  `set_ui` (hide/disable/rename). That UI state is per client and session, not in the
  document, not undoable.
- Neither callback may render or check out inputs.

### 8.6 Dynamic flags

With `DYNAMIC_FLAGS`, `query_dynamic_flags` (M) is called when the render graph is built
and whenever non-animated values change. It may change only bits in
`BAE_EFFECT_DYNAMIC_FLAGS_MASK`. The answer MUST hold for all times; an effect whose flags
depend on animated values returns the conservative union. The result is part of the
graph's cache key.

### 8.7 Render protocol

1. **`pre_render`** (R) receives the request rect (render pixel space of the effect's
   layer) and the planned device. The plugin MUST declare every input it will read with
   `checkout_input` (source, time, rect, `checkout_id`) and every param value at another
   time with `checkout_param`; it returns `result_rect` (what it will produce, may exceed
   the request only with `RETURNS_EXTRA_PIXELS`), `max_result_rect` (largest output for
   any request; used for layer bounds), flags (`SOLID`, `IDENTITY`, clear `GPU_POSSIBLE`
   to force CPU) and optional frame data.
2. `checkout_id` is plugin-chosen and unique within one `pre_render`; **id 0 is reserved for
   the self input** (`source = SELF`). A NULL `pre_render` behaves as if it checked out the
   self input as id 0 with `rect = request_rect` and returned the input's rects (identity
   ROI).
3. The host renders the declared inputs, then calls `render_gpu` if the planned device is
   GPU and `GPU_POSSIBLE` is still set, otherwise `render_cpu`. Only checked-out ids can be
   fetched (`get_input_*`); others return `BAE_ERR_NOT_FOUND`.
4. **Output**: covers `result_rect ∩ request_rect` (or all of `result_rect` with
   `RETURNS_EXTRA_PIXELS`). Its content is undefined on entry; the plugin MUST write every
   pixel. Pixels outside `result_rect` are transparent by definition.
5. `IDENTITY` (or a manifest `identity_when`) means output = self input: the host skips the
   render call and aliases the input (zero cost).
6. **Abort**: `is_aborted` is cheap (an atomic load); poll at least every few
   milliseconds of work and return `BAE_ERR_ABORTED`.
7. **GPU fallback**: `render_gpu` may return `BAE_STATUS_FALLBACK`; the host then releases
   the frame data, re-runs `pre_render` with `device = CPU` and calls `render_cpu`. This
   is a per-frame exception path; effects that know earlier clear `GPU_POSSIBLE`.
8. `release_frame_data` is called exactly once for every non-NULL frame data.

### 8.8 Generators and transitions

- **Generator**: the self input exists (usually a solid) but need not be checked out. The
  default extent is the layer bounds; an infinite generator returns
  `BAE_COORD_MIN..BAE_COORD_MAX` and the host clips to the request.
- **Transition**: inputs `SELF` ("from") and `TRANSITION_TO` ("to"), progress from the
  FLOAT param flagged `TRANSITION_PROGRESS` (0..1). In the motion workspace a transition
  is a normal layer effect (AE Linear Wipe style) and `TRANSITION_TO` checks out `EMPTY`;
  in edit sequences (14) the host supplies both clips and drives the progress.

### 8.9 Dependencies and messages

- `get_dependencies` (with `EXTERNAL_DEPENDENCIES`, M): JSON array of
  `{"kind": "file"|"font"|"asset", "path"|"name": ...}`; used by Collect Files and
  `.baez` packaging (21) and by missing-file reports.
- `message` (M, or any thread for selectors the plugin documents): generic extension point
  (AE `COMPLETELY_GENERAL`), reachable from scripts as `effect.message(selector, bytes)`.
  `bae.*` selectors are reserved: `bae.about`, `bae.options_dialog`, `bae.migrate`.

### 8.10 The same lifecycle in every form

| Step | Declarative WGSL | Native | Built-in Rust |
|---|---|---|---|
| global setup | host-internal (manifest → description) | `global_setup` | `Effect::describe` |
| params | manifest `[[effect.param]]` | `define_params` | `Effect::define_params` |
| instances | none (stateless) | `instance_*` if `STATEFUL` | `Effect::Instance` |
| pre-render | from `[effect.roi]` + `identity_when` (§11.6) | `pre_render` | `Effect::pre_render` |
| render | host-encoded passes (§11.5) | `render_cpu` / `render_gpu` | `Effect::render_cpu` / `render_gpu` |
| supervision | none | `param_changed` / `update_params_ui` | same trait methods |

---

## 9. After Effects mapping

Goal (ARCHITECTURE §15): the future `bae-aex-host` (20) translates an AE effect's
`EffectMain` calls to and from this interface without a second effect system. Names below
are from Adobe's public SDK guide and are used for cross-reference only; nothing from the
SDK is copied. 20's `bae-aex-host::selector` encodes the same table (`LifecycleHook`).

### 9.1 Command selectors

| AE selector | BAE | Notes |
|---|---|---|
| `ABOUT` | manifest `description` + `message("bae.about")` | |
| `GLOBAL_SETUP` | `global_setup` | PiPL / out-flags → `BaeEffectDescription` (§9.2) |
| `GLOBAL_SETDOWN` | `global_setdown` | |
| `PARAMS_SETUP` | `define_params` | |
| `SEQUENCE_SETUP` | `instance_create(EDIT, empty)` | compat effects are always `STATEFUL` |
| `SEQUENCE_RESETUP` | `instance_create(EDIT or RENDER, blob)` | load, paste, duplicate, remote change |
| `SEQUENCE_FLATTEN` | `instance_serialize` | after `BAE_CHANGED_STATE` |
| `GET_FLATTENED_SEQUENCE_DATA` | `instance_serialize` | non-destructive by contract (§8.3 item 4) |
| `SEQUENCE_SETDOWN` | `instance_destroy` | |
| `FRAME_SETUP` | `pre_render` | compat host synthesizes ROI from the legacy output size/origin |
| `RENDER` | `render_cpu` | legacy 8/16 bpc path |
| `FRAME_SETDOWN` | `release_frame_data` | |
| `SMART_PRE_RENDER` | `pre_render` | layer checkouts → `checkout_input`; result/max rects; `GPU_RENDER_POSSIBLE` → `GPU_POSSIBLE` |
| `SMART_RENDER` | `render_cpu` | input checkouts → `get_input_cpu`; output world → `args.output` |
| `SMART_RENDER_GPU` | `render_gpu` | GPU worlds via `bae.gpu.native` + host conversion (20's BGRA128 buffers) |
| `AUDIO_SETUP` / `AUDIO_RENDER` / `AUDIO_SETDOWN` | reserved kind `BAE_KIND_AUDIO_EFFECT` | not in ABI 0.x; such effects become placeholders |
| `EVENT` | reserved `ui_event` + `bae.draw` (ABI 0.2, `BAE_EFFECT_FLAG_CUSTOM_UI`) | Drawbot → backend-neutral display list (20 `ui.rs`); never a native window |
| `USER_CHANGED_PARAM` | `param_changed` | |
| `UPDATE_PARAMS_UI` | `update_params_ui` | |
| `DO_DIALOG` | `message("bae.options_dialog")` + `BAE_EFFECT_FLAG_OPTIONS_DIALOG` | |
| `ARBITRARY_CALLBACK` | `BaeArbitraryVTable1` | new/dispose/copy/flatten are host-managed (values are bytes); interpolate/compare/print/scan map to `interpolate`/`compare`/`to_text`/`from_text` |
| `GET_EXTERNAL_DEPENDENCIES` | `get_dependencies` | |
| `COMPLETELY_GENERAL` | `message` | |
| `QUERY_DYNAMIC_FLAGS` | `query_dynamic_flags` | |
| `GPU_DEVICE_SETUP` | `gpu_device_setup` | returning no GPU data = opt out on that device |
| `GPU_DEVICE_SETDOWN` | `gpu_device_setdown` | |

### 9.2 Capability flags

| AE out-flag (public name) | BAE |
|---|---|
| `PIX_INDEPENDENT` | `BAE_EFFECT_FLAG_PIXEL_INDEPENDENT` |
| `I_EXPAND_BUFFER` / `I_SHRINK_BUFFER` | `EXPANDS_BUFFER` / `SHRINKS_BUFFER` |
| `NON_PARAM_VARY` | `NON_PARAM_VARY` |
| `WIDE_TIME_INPUT` | `WIDE_TIME_INPUT` |
| `I_USE_SHUTTER_ANGLE` | `USES_SHUTTER` |
| `I_USE_3D_CAMERA` / `I_USE_3D_LIGHTS` | `USES_CAMERA` / `USES_LIGHTS` (+ planned `bae.scene` suite, §14.2) |
| `SEND_UPDATE_PARAMS_UI` | `SEND_UPDATE_PARAMS_UI` |
| `I_DO_DIALOG` | `OPTIONS_DIALOG` |
| `CUSTOM_UI` | `CUSTOM_UI` (reserved) |
| `I_HAVE_EXTERNAL_DEPENDENCIES` | `EXTERNAL_DEPENDENCIES` |
| `REVEALS_ZERO_ALPHA` | `REVEALS_ZERO_ALPHA` (host must not trim inputs to their alpha bounds) |
| `DEEP_COLOR_AWARE` / `FLOAT_COLOR_AWARE` | `cpu_formats` include `ARGB_U16_15BIT_PREMUL` / `ARGB_F32_PREMUL` |
| `SUPPORTS_SMART_RENDER` | compat host provides `pre_render`; otherwise the legacy path above |
| `SUPPORTS_THREADED_RENDERING` | `threading = CONCURRENT` (else `SERIAL_GLOBAL`) |
| `MUTABLE_RENDER_SEQUENCE_DATA_SLOWER` | compat host keeps per-thread render-instance copies |
| `SUPPORTS_GPU_RENDER_F32` | `devices |= GPU`, `gpu_formats ∋ RGBA32F` |
| `FORCE_RERENDER` (from user-changed-param) | `BAE_CHANGED_RERENDER` |

### 9.3 `in_data` and render request fields

| AE | BAE |
|---|---|
| `current_time`, `time_step`, `time_scale` | `BaeRenderInfo.time`, `time_step` in flicks (exact conversion, 20 `time.rs`) |
| `total_time` | `layer_duration` |
| `downsample_x/y` | `downsample_x/y` |
| `pixel_aspect_ratio` | `pixel_aspect` |
| `shutter_angle`, `shutter_phase` | same |
| `width`, `height` (layer) | `layer_bounds` |
| `extent_hint` / request rect | `request_rect` |
| `output_origin_x/y` | `result_rect` origin |
| `quality` | `quality` |
| `field` | not supported: BAE renders progressive frames only |
| `global_data` / `sequence_data` / `frame_data` | `global` / `instance` / `frame_data` |
| `effect_ref`, `utils` | `BaeRenderCtx` / `BaeParamEditor` + suites; AE per-pixel utility suites are implemented *inside* the compat host |

### 9.4 Parameter types

| AE param type | BAE type |
|---|---|
| `LAYER` | `BAE_PARAM_LAYER` (checkout = layer after masks and effects, before transform, in that layer's space; `WITHOUT_EFFECTS` = source only) |
| `SLIDER` (integer) | `INT` |
| `FIX_SLIDER`, `FLOAT_SLIDER` | `FLOAT` (precision, display flags) |
| `ANGLE` | `ANGLE` |
| `CHECKBOX` | `CHECKBOX` |
| `COLOR` | `COLOR` (float; 8-bit values converted) |
| `POINT`, `POINT_3D` | `POINT`, `POINT_3D` |
| `POPUP` | `POPUP` (same 1-based index) |
| `GROUP_START` / `GROUP_END` | `GROUP` (`end_id` = the GROUP_END index) |
| `ARBITRARY_DATA` | `ARBITRARY` |
| `PATH` | `PATH` |
| `BUTTON` | `BUTTON` |

Param flags: `CANNOT_TIME_VARY` → `NOT_ANIMATABLE`; `SUPERVISE` → `SUPERVISE`;
`START_COLLAPSED` → `START_COLLAPSED`; hidden/disabled UI flags → `HIDDEN`/`DISABLED`
(via `set_ui` when dynamic). The AE param index ↔ BAE definition order mapping is the
identity (groups occupy a slot in both).

---

## 10. Render-graph contract (for 04)

This is what `bae-render` implements around an effect node. All of it is form-agnostic:
the node holds an `Arc<dyn bae_render::Effect>` resolved once at graph build (§7.3).

### 10.1 Request

Every node evaluation carries a request (`BaeRenderInfo` / `bae_plugin_sdk::RenderInfo`):
layer time and comp `time_step` (flicks), comp frame rate, downsample (x, y), layer pixel
aspect, quality (draft/high), shutter angle/phase, working color space + linearized flag,
final/preview/headless flags, layer bounds, layer duration, render id, and the param
values at that time. All rects are in the effect layer's **render pixel space** =
full-resolution layer pixels × downsample, origin at the layer's top-left, y down.

### 10.2 ROI negotiation

1. Graph evaluation is demand-driven from the comp output: each node receives a request
   rect and answers with a result rect, recursively (AE SmartFX semantics).
2. For an effect node the host calls `pre_render(request)`; each `checkout_input` becomes
   a request to the upstream node (same layer: the previous effect or the layer source;
   layer params: the referenced layer's post-mask, post-effect, pre-transform output;
   other times: the same subgraph evaluated at that time). The upstream answer is returned
   synchronously as `BaeInputInfo` — pre-render is planning only, nothing renders yet.
3. Within one frame, several requests to the same (node, time) are merged (union of rects)
   so every node renders at most once per frame and time.
4. `max_result_rect` answers are cached per node and parameter revision; 04 uses them for
   layer-bounds queries (motion-blur extents, collapse transformations, precomp bounds).
5. Cycles through layer params (A's effect reads B whose effect reads A) are broken: the
   back edge reads the layer with its masks only (no effects) and logs a diagnostic.

### 10.3 Dependencies

`pre_render` records the node's dependency set: inputs (node, time, rect), params at other
times, layer params, and with `USES_CAMERA`/`USES_LIGHTS`/`USES_SHUTTER` the comp camera,
lights and shutter. 04 keeps a reverse index from Prims/time ranges to nodes so a
`StageEvent` invalidates exactly the affected cache entries. `WIDE_TIME_INPUT` is required
to check out other times (else `BAE_ERR_INVALID_ARGUMENT`).

### 10.4 Caching

Cache key of an effect node output = (effect Prim, plugin id + version, time or
time-independence, values of all params except `UI_ONLY`, state revision, dynamic flags,
input keys, result rect, downsample, quality, color space + linear flag, shutter if
`USES_SHUTTER`, camera/lights if `USES_*`). Time-independence: without `NON_PARAM_VARY`,
a node whose params and inputs are constant over a time range reuses one result across
that range. `NON_DETERMINISTIC` disables caching for the node and everything downstream.

### 10.5 Scheduling and threading

- Effect nodes run on render workers, never on UI or M.
- `threading` is enforced per effect contribution: `CONCURRENT` (no restriction),
  `SERIAL_INSTANCE` (one call at a time per render instance; host keeps a per-instance
  lock or queue), `SERIAL_GLOBAL` (one render-side call at a time for the whole effect).
  The lock is taken only around the plugin call, never around input rendering.
- Abort: every frame request has an abort flag (atomic); the scheduler sets it when the
  frame is no longer needed (scrubbing); `is_aborted` reads it.
- Progress reporting is aggregated per frame for the UI and the render queue.

### 10.6 Devices, formats and CPU nodes

- **GPU path**: the node format is the working format (RGBA16F default, RGBA32F per
  project). If the effect's `gpu_formats` lacks it, the host converts to the first
  accepted format and back on the GPU (cost only on that node).
- **CPU path** (`render_cpu`): the host picks the first entry of `cpu_formats` it supports
  (every `BaePixelFormat` combination is supported) and:
  1. converts the GPU input (format, channel order RGBA/BGRA/ARGB, component U8/U16/
     U16-15-bit/F16/F32, premultiplied ↔ straight, working space unchanged) with a compute
     kernel into a staging buffer with 64-byte-aligned rows;
  2. reads it back asynchronously (`map_async`), double-buffered;
  3. runs the plugin on a worker thread on the mapped/copied memory;
  4. uploads the output and converts back to the working format on the GPU.
  The graph executor splits GPU work at CPU nodes so GPU work of other branches and of the
  next frames proceeds while the CPU node runs (pipelining during playback). The round
  trip costs only the CPU node (budget §22).
- Straight-alpha CPU formats are converted with the usual epsilon rule (alpha 0 → RGB
  0, or preserved when `BAE_INPUT_FLAG_PRESERVE_RGB_OF_ZERO_ALPHA` is requested).
- U16_15BIT is AE's 0..32768 range; U16 is 0..65535.

### 10.7 Downsample, pixel aspect, time

- Downsampled requests are in downsampled pixels; params of type POINT are delivered
  already scaled. Effects scale distance-like params themselves (AE behaviour); WGSL gets
  `frame.downsample` helpers (§11.3).
- Pixel aspect: the effect works on the layer's pixel grid; the host applies PAR in the
  layer transform. `pixel_aspect` is informational (e.g. for radial effects that want
  round results on non-square pixels).
- Time: `time` is **layer time** after time remapping/stretch (AE `current_time`
  semantics); `time_step` is one comp frame (negative when the layer plays reversed).

### 10.8 Identity, disabled effects, placeholders

Disabled effects (fx switch off) and placeholders (§20) are removed from the graph at
build time; `IDENTITY` nodes alias their input. None of these cost a pass.

### 10.9 Fusion of pixel-style chains

Consecutive declarative effects with `style = "pixel"` on the same layer (no other
consumers of the intermediate results, same working format) are fused by 04 into one
compute kernel: one input read, N inlined pixel functions, one write. Each fused effect
keeps its own params uniform. Fusion happens at the naga IR level (per-effect global
renaming, bindings renumbered); the fused module is cached by the ordered list of
(effect id, version). Built-in pixel effects (07) are pixel-style too, so a built-in
chain and a plugin chain fuse the same way. The fused chain caches only its final output.

### 10.10 Errors in the graph

A failing node (status < 0 other than `BAE_ERR_ABORTED`) passes its self input through
unchanged and reports a diagnostic (badge on the effect in Effect Controls and the
timeline, log entry with plugin id, callback, message). Previews continue. Final renders
(render queue, `bae render`) fail with the diagnostic unless "continue on effect errors"
is set. Repeated failures of the same node are rate-limited (one diagnostic per node and
error code per second).

### 10.11 Node interface summary

```text
graph build (once per graph):
  entry   = effect_library.get(match_name)       // Arc<dyn bae_render::Effect>, a lease (§7.3)
  desc    = entry.description()                   // flags, threading, devices, formats
  flags   = desc.flags ⊕ entry.dynamic_flags(...) // if DYNAMIC_FLAGS
  inst    = render_instance_cache.get_or_create(prim, state_revision)
per frame:
  plan    = entry.pre_render(inst, request, &services)     // checkouts recorded by services
  inputs  = render(plan.checkouts)                          // recursive
  out     = if plan.identity { alias(self) }
            else if device == Gpu && plan.gpu_possible { entry.render_gpu(...) }
            else { cpu_node(entry.render_cpu, plan.format) }   // §10.6
  release(plan.frame_data)
```

---

## 11. Declarative WGSL effects

### 11.1 Model

A declarative effect is manifest data + WGSL. The host turns it into a
`DeclarativeEffect` (crate `bae-plugin`) that implements `bae_render::Effect`; it runs with
the **same generic shader node, uniform ring, bind-group cache, transient texture pool
and pipeline cache** as the built-in WGSL effects of 07, which are themselves authored as
declarative packages embedded in the binary (§19). There is no interpreter, no extra
copy and no extra dispatch: the < 2 % budget (§22) holds by construction.

### 11.2 Module assembly

*(implemented: `bae_plugin::wgsl`)* The host compiles one WGSL module per (effect,
working format, pass-target format); pixel style has one implicit pass. The modules
differ only in the prelude's `bae_out` declaration:

```
<prelude>       §11.3: structs, bindings, helpers (never written by authors; virtual file)
<include files> §11.10, in manifest order
<author shader> `shader = "..."`
<kernel>        pixel style only (§11.4; virtual file)
```

Rules for author code (checked on the naga IR at load): it MUST NOT declare
`@group`/`@binding` resources (all bindings come from the prelude, so the host can
renumber them for fusion); functions, types, constants and globals starting with `bae_` /
`Bae` are reserved. With `features = ["f16"]` the host emits `enable f16;` at the top of
the prelude (authors MUST NOT write it); a device without f16 makes the effect
unavailable with a diagnostic. Diagnostics map naga spans back to `file:line:column` of
the include or shader file.

### 11.3 Generated prelude

Plugin API 0.1 prelude (abridged; `bae_plugin::wgsl::prelude` is the reference):

```wgsl
struct BaeRect { min: vec2<i32>, max: vec2<i32> }   // half-open, render pixels

struct BaeFrame {
  target_rect: BaeRect,     // rect of the current pass target (= output_rect for "output")
  output_rect: BaeRect,     // the effect's output rect
  layer_bounds: BaeRect,
  downsample: vec2<f32>,    // (1,1) full res, (0.5,0.5) half ...
  time: f32,                // layer time in seconds
  time_step: f32,           // seconds per comp frame (0 if unknown)
  frame_rate: f32,          // comp frame rate (0 if unknown)
  pixel_aspect: f32,
  shutter_angle: f32,       // degrees
  shutter_phase: f32,
  flags: u32,               // BAE_FRAME_LINEAR | BAE_FRAME_FINAL | BAE_FRAME_HIGH_QUALITY
  seed: u32,                // stable per effect instance: noise seeds
  frame_index: i32,
  pass_index: u32,
}
const BAE_FRAME_LINEAR: u32 = 1u;        // working space is linearized
const BAE_FRAME_FINAL: u32 = 2u;         // render queue / bae render
const BAE_FRAME_HIGH_QUALITY: u32 = 4u;
@group(0) @binding(0) var<uniform> frame: BaeFrame;

// One field per numeric param, in definition order (absent if the effect has none).
struct BaeParams {
  amount: f32,              // float, angle (degrees)          → f32
  center: vec2<f32>,        // point (render px)                → vec2<f32>
  color: vec4<f32>,         // color (straight)                 → vec4<f32>
  shape: u32,               // popup (1-based index), checkbox  → u32
  samples: i32,             // int                              → i32
}
@group(0) @binding(1) var<uniform> params: BaeParams;

// Host-internal: one vec4<i32> rect (min.xy, max.xy) per source; use the helpers.
@group(0) @binding(2) var<uniform> bae_sources: BaeSources;
@group(0) @binding(3) var bae_out: texture_storage_2d<FORMAT, write>;   // current pass target
@group(0) @binding(4..) var bae_tex_<src>: texture_2d<f32>;           // one per source
```

Sources, in binding order: `input`, `to` (transitions), each `layer` param (by key),
each `[[effect.texture]]` (compute style). Uniform layouts follow WGSL's uniform
address-space rules; the host reads the member offsets from the naga IR of the compiled
module, so host packing and shader layout cannot disagree. Params without a WGSL
representation (group, button, arbitrary, path, layer) get no field; layer params get
textures instead. Points are in **render pixels** (downsample applied); distances are not
converted — use `bae_scale(x)`. There are no samplers: `bae_sample_*` is an exact
bilinear filter over `bae_load_*`, identical for every format (RGBA32F is not filterable
everywhere) and with AE's transparent-outside semantics.

Helpers (all pure WGSL, inlined by the compiler):

| Helper | Meaning |
|---|---|
| `bae_load_<src>(p: vec2<i32>) -> vec4<f32>` | texel at render pixel `p`; transparent outside the source rect (AE semantics) |
| `bae_sample_<src>(p: vec2<f32>) -> vec4<f32>` | bilinear sample at render-pixel position (centers at `.5`); transparent outside |
| `bae_rect_<src>() -> BaeRect` | the rect the source covers |
| `bae_store(p: vec2<i32>, c: vec4<f32>)` | write the current pass target |
| `bae_target_pixel(id: vec3<u32>) -> vec2<i32>` / `bae_in_target(p) -> bool` | invocation id → render pixel of the target, bounds test |
| `bae_scale(x: f32) -> f32` | full-res distance → render px (`x * frame.downsample.x`) |
| `bae_to_layer(p: vec2<f32>) -> vec2<f32>` / `bae_from_layer(...)` | render px ↔ full-res layer px |
| `bae_layer_uv(p) -> vec2<f32>` | 0..1 across `layer_bounds` |
| `bae_premultiply(c)` / `bae_unpremultiply(c)` | alpha conversions (unpremultiply of alpha 0 returns 0) |
| `bae_to_linear(c)` / `bae_from_linear(c)` | working ↔ linear light: identity if `BAE_FRAME_LINEAR`, else the sRGB curve (sign-preserving for HDR) |
| `bae_luma(c) -> f32` | Rec.709 luma of the working values |
| `bae_hash(seed: u32, p: vec2<i32>) -> f32` | deterministic 0..1 noise (PCG) |

`<src>` is `input`, `to`, a layer-param key or a texture name. Adding helpers or `BaeFrame`
fields is a minor API change; renaming or removing them is a major one.

### 11.4 Styles

**Pixel style** (`style = "pixel"`, requires `pixel-independent` semantics): the author
writes one function

```wgsl
struct BaePixel { pos: vec2<i32>, layer_pos: vec2<f32>, uv: vec2<f32> }  // generated
fn vignette(c: vec4<f32>, px: BaePixel) -> vec4<f32> { ... }            // author
```

It receives the premultiplied input color at the output pixel and returns the output
color. It MAY read `frame` and `params` and call helper functions; it MUST NOT use any
texture, directly or through callees (checked on the naga IR at load). The host
generates the kernel `bae_pixel_main` (`@workgroup_size(16, 16)`: load → function →
store) and may fuse chains (§10.9).
Implied flags: `PIXEL_INDEPENDENT`; ROI = identity.

**Compute style** (`style = "compute"`): the author writes `@compute` entry points named by
`[[effect.pass]]`; each pass declares what it reads and writes. The workgroup size comes
from the entry point's `@workgroup_size` (≤ 256 invocations, validated); the host
dispatches `ceil(target_size / workgroup_size)` groups. Pattern:

```wgsl
@compute @workgroup_size(8, 8)
fn blur_horizontal(@builtin(global_invocation_id) id: vec3<u32>) {
  let p = bae_target_pixel(id);
  if (!bae_in_target(p)) { return; }
  ...
  bae_store(p, sum);
}
```

### 11.5 Passes and intermediate textures

- Passes run in manifest order, each as one dispatch with its own pipeline (derived from
  the entry point) and bind group. The last pass writes `output`.
- Each pass's module declares every source as a sampled texture and **that pass's
  target** as `bae_out` (`texture_storage_2d<FORMAT, write>` with the target's format),
  so `bae_store` always writes the current target. The pass's own target is bound to a
  1×1 placeholder in that pass. A pass MUST NOT read its own target and MUST list every
  source it reads in `reads` (both checked at load from the entry point's global uses).
- `[[effect.texture]]` intermediates come from the host's transient pool: they cover
  `rect = "output"` or `"input"` at full render resolution (`scale` is reserved), format
  `working` or one of the portable storage formats. Each texture must be written before
  it is read.
- Pass targets come from the transient pool with **undefined contents**: every pass MUST
  write every pixel of its target rect (the generated pixel kernel does).
- Data-dependent pass structures (pyramids with a param-dependent depth, iterative
  solvers, reductions feeding later passes) are out of scope for v1 declarative effects:
  use native `bae.gpu.compute`. A future `repeat = "<expr>"` key is reserved.

### 11.6 ROI from the manifest

`[effect.roi]` replaces `pre_render`:

- Input request = request rect inflated by the `read*` margins (evaluated, §11.7, in
  full-resolution layer pixels, multiplied by the downsample and rounded up).
- Result rect: `extent = "input"` → the input's result rect, grown by the mirrored margins
  if `grows = true`; `"layer"` → layer bounds; `"infinite"` → the request rect. Then
  intersected with the request (declarative effects never return extra pixels).
- `max_result_rect` follows the same rule with the input's max rect.
- Layer params are checked out with the same inflated rect at the same time; transitions
  check out `to` likewise.
- `identity_when` true → `IDENTITY` (node skipped).

### 11.7 Expressions

Used by `read*`, `identity_when` and (reserved) `repeat`. Grammar:

```
expr    := or
or      := and ("||" and)*          and := cmp ("&&" cmp)*
cmp     := sum (("=="|"!="|"<"|"<="|">"|">=") sum)?
sum     := prod (("+"|"-") prod)*   prod := unary (("*"|"/") unary)*
unary   := ("-"|"!") unary | atom
atom    := number | "params." key | "frame.downsample_x" | "frame.downsample_y"
         | func "(" expr ("," expr)* ")" | "(" expr ")"
func    := min | max | abs | floor | ceil | sqrt | clamp
```

Values are f64; booleans are 0/1; `params.<key>` is allowed for float, int, angle,
checkbox (0/1) and popup (1-based index) params. Expressions are parsed at load into a tiny
bytecode, evaluated in `pre_render` with the values at the frame time (no allocation),
and validated by `bae plugin check`. Division by zero yields 0 and a debug diagnostic.

### 11.8 Compilation, pipelines, diagnostics

1. Load (L, or a worker): read files → generate prelude → naga parse + validate → check
   rules (no author bindings, entry points exist with the right signatures, pixel
   function reads no textures, workgroup ≤ 256, only manifest-declared features).
2. Pipelines are created per (device, working format) asynchronously when a project using
   the effect opens or the effect is applied (pre-warm), never synchronously on the first
   frame; they go into the host pipeline cache (wgpu `PipelineCache` where supported).
3. Errors produce diagnostics with file:line:column and keep the previous version running
   in dev hot-reload (§7.5); at normal load they mark the plugin `Failed`.

### 11.9 Performance contract

The declarative node is the built-in node: identical kernel shape, the same uniform ring
with dynamic offsets (no per-frame buffer creation), cached bind groups, pooled
textures, no CPU work per frame beyond packing the params uniform (a memcpy of ≤ 256 bytes
typical) and evaluating ROI bytecode. 07 authors its WGSL built-ins in this format, so
any optimization 04 makes applies to plugins as well.

### 11.10 Shared WGSL modules

An effect may list `includes = ["shaders/common.wgsl", "plugin:bae.std-shaders/noise.wgsl"]`.
Package-relative files and files exported by dependencies (`[exports] wgsl = [...]` in
the dependency's manifest) are concatenated in order between prelude and shader. No
preprocessor and no `#import`: duplicate definitions are compile errors, which keeps the
model simple and diagnostics exact.

### 11.11 Example

`plugins/examples/vignette/` is the reference package (pixel style, popup + point +
float + color params, `identity_when`); `plugins/examples/directional-blur/` shows
compute style with two passes and an intermediate texture.

---

## 12. Native GPU interop

### 12.1 Two levels

| | `bae.gpu.compute` (recommended) | `bae.gpu.native` |
|---|---|---|
| What | the plugin supplies WGSL compute pipelines; the host encodes dispatches into its own command stream | raw Metal / D3D12 / Vulkan device, queue and texture handles |
| Portability | all backends, also out-of-process later (record/replay, §15.3) | per backend; in-process only |
| Synchronization cost | none (host-ordered) | one extra queue submit per node + fence handling |
| Use for | most GPU effects | existing Metal/HLSL/CUDA-interop code, AE GPU effects (20) |

### 12.2 Device sharing

There is one `wgpu::Device` per process (`bae_gpu::GpuContext`, 01) shared by renderer, UI
and plugins. Native handles are obtained through wgpu-hal (`Device::as_hal`,
`Texture::as_hal`), exposed by `bae_gpu::native` (request R01-3). The plugin receives the
host's device and **the host's queue**: Metal `id<MTLDevice>` / `id<MTLCommandQueue>`;
D3D12 `ID3D12Device*` / `ID3D12CommandQueue*` (+ adapter LUID); Vulkan handles when the
Vulkan backend is enabled.

### 12.3 Scope protocol (`begin` / `end`)

Within `render_gpu`:

1. `begin(images, usages)`: the host closes and submits its pending command encoder for
   this frame's dependency chain (so everything the inputs depend on is enqueued before
   the plugin's work), transitions the requested textures to the requested usage
   (D3D12 resource states / Vulkan layouts; Metal has none) and returns their native
   handles plus a scope with optional fences.
2. The plugin encodes its own command buffers and commits them to `scope.device.queue`.
   If `wait_fence` ≠ 0 it MUST encode a GPU-side wait for `wait_value` before the first
   access; if `signal_fence` ≠ 0 it MUST encode a GPU-side signal of `signal_value` after
   the last access. It MUST NOT block the CPU waiting for the GPU.
3. `end()`: the host assumes the textures are back in the state they were handed out in
   (`BaeNativeGpuTexture.state`) and continues encoding; later host submissions are
   ordered after the plugin's.

Fence use per backend: D3D12 — same queue, ordering by `ExecuteCommandLists` order, fences
normally 0. Metal — wgpu may create untracked resources, so the host passes an
`MTLSharedEvent` pair (`encodeWaitForEvent` / `encodeSignalEvent`). Vulkan — timeline
semaphores. The host decides; plugins only honour non-zero fences.

### 12.4 Lifetimes and ownership

- All handles are host-owned and valid until `render_gpu` returns; the host keeps the
  textures alive on the GPU until the plugin's signalled work completes.
- Device and queue may be retained (ObjC `retain` / COM `AddRef`) in `gpu_device_setup`
  and MUST be released in `gpu_device_setdown`.
- Plugin-created GPU objects (pipelines, buffers) are created in `gpu_device_setup` and
  destroyed in `gpu_device_setdown`; per-frame temporaries must be released only after GPU
  completion (completion handlers / fences), and **no completion handler may run after
  `gpu_device_setdown` returns** (the host waits for its last fence before unmapping code,
  §7.4 step 5).
- Formats: the working format maps to `MTLPixelFormatRGBA16Float` /
  `DXGI_FORMAT_R16G16B16A16_FLOAT` (RGBA32F analogously); textures may be larger than
  their `rect` (pooled).

### 12.5 Device loss, multiple GPUs, CUDA

- Device lost: the host calls `gpu_device_setdown` for every plugin on that device (handles
  are then invalid and must not be used), recreates the device and calls
  `gpu_device_setup` lazily again.
- One GPU device per process in v1; `device_index` is reserved for multi-GPU.
- CUDA/OpenCL are not offered. CUDA interop for AE GPU effects on Windows would be a new
  suite (`bae.gpu.cuda`, D3D12 ↔ CUDA external memory) if 20 needs it.

---

## 13. Threading model

### 13.1 Host threads

| Letter | Thread | Notes |
|---|---|---|
| UI | main/UI thread | plugins never run here, except script panels' render callbacks (§17.6) |
| M | model thread | owns the Stage, executes commands; may coincide with UI in early milestones but plugin code never blocks UI frames for long (§8.5 budgets) |
| L | plugin thread | one host thread for load/unload, `bae_plugin_entry`, `global_setup`/`setdown`, `define_params`, `shutdown` |
| R | render workers | pool sized to cores; GPU encoding and CPU nodes |
| G | GPU setup | a render worker that holds the effect's per-device setup lock |
| S | script workers | QuickJS runtimes (§17.1) |
| D | media workers | decoders/encoders (§18.4) |

### 13.2 Callback threads and exclusivity

| Callback | Thread | Concurrency guarantee |
|---|---|---|
| `bae_plugin_entry`, `shutdown` | L | nothing else of the plugin runs |
| `global_setup`, `global_setdown`, `define_params` | L | nothing else of that effect runs |
| `gpu_device_setup` / `setdown` | G | no `render_gpu` of that effect on that device concurrently |
| `instance_*` (EDIT), `param_changed`, `update_params_ui`, `query_dynamic_flags`, `get_dependencies`, `message` | M | serialized (M is one thread) |
| `instance_create`/`destroy` (RENDER), `pre_render`, `render_*`, `release_frame_data` | R | per `threading` (§10.5) |
| shape operator `apply`, expression functions | any | fully concurrent (pure) |
| importer/exporter/command callbacks | M (commands, importers) / worker (exporters) | one call at a time per contribution |
| decoder/encoder handle calls | D | serialized per handle; different handles concurrent |

### 13.3 Plugin obligations

- Suites are called only from the thread of the callback that received the context
  (`BaeRenderCtx`, `BaeParamEditor`, ...), and only until it returns. Exceptions:
  `bae.core` (`log`, `alloc`, `free`, `has_permission`, `get_path`) is callable from any
  thread at any time while the plugin is loaded.
- Threads a plugin creates MUST NOT touch host-provided memory after the callback that
  provided it returns; prefer `bae.render.parallel_for` (host pool, no oversubscription).
- Floating-point environment: the host enters callbacks with default rounding, no FP
  exceptions enabled; FTZ/DAZ unspecified. A plugin that changes MXCSR/FPCR restores it
  before returning.
- Re-entrancy: suite calls never re-enter the same plugin synchronously, except
  `bae.command.invoke`, which may run another plugin's command; invoking a command of the
  calling plugin recursively returns `BAE_ERR_BAD_STATE`.
- Thread-local storage in plugins is allowed but should not hold host pointers.

---

## 14. C ABI: suites, vtables, compatibility policy

The header `sdk/c/bae_plugin.h` is normative for layouts; this section states the policy.
Rules R1–R10 are at the top of the header.

### 14.1 Handshake

```
host                                              plugin
 │ dlopen / LoadLibraryExW                          │
 │ bae_plugin_entry(&BaeHost, &exports) ──────────► │ check abi_version (rule 3, §6.6)
 │                                                  │ get_suite("bae.core", 1) ...
 │ ◄─────────────────────────── BAE_OK + exports    │ fill exports (ids, contributions, vtables)
 │ validate exports vs manifest (§7.2)              │
 │ per contribution on first use: global_setup ...  │
```

`BaeHost.abi_version` is the version the host presents to this plugin (§6.6 rule 1); the
plugin's `abi_built` tells the host which struct sizes the plugin knows (informational:
R3 makes sizes self-describing).

### 14.2 Suites

| Suite | Version | Valid in | Purpose |
|---|---|---|---|
| `bae.core` | 1 | always (any thread) | log, `set_error`, aligned alloc/free, `has_permission`, plugin paths |
| `bae.param.define` | 1 | `define_params` | add parameter definitions |
| `bae.param.edit` | 1 | `param_changed`, `update_params_ui`, `message` on M | set values (undoable ops), set UI state |
| `bae.render` | 1 | `pre_render`, `render_*` | checkouts, inputs, abort, progress, temp images, `parallel_for` |
| `bae.gpu.compute` | 1 | `gpu_device_setup`, `render_gpu` | host-encoded WGSL compute |
| `bae.gpu.native` | 1 | `gpu_device_setup`, `render_gpu` | raw device/queue/texture handles (§12) |
| `bae.path` | 1 | shape operator `apply` | output paths |
| `bae.command` | 1 | commands, importers, exporters | invoke commands, read-only queries, progress |
| `bae.scene` | planned 1 | `pre_render`, `render_*` | comp camera, lights, comp size/time (needed by `USES_CAMERA`/`USES_LIGHTS` effects and 20) |
| `bae.draw` | planned 1 | `ui_event` (ABI 0.2) | custom UI display lists (20 `ui.rs`) |
| `bae.l10n` | planned 1 | any | translated strings from the plugin's locale files |

A suite table is acquired once (typically in `bae_plugin_entry` or `global_setup`) and
cached; acquisition is not a hot-path operation. A suite version is **frozen** once
released (R4): fixing a mistake means a new version, both stay available.

### 14.3 VTables

Each native contribution exports `(kind, id, vtable_version, vtable)`. VTables start with
`struct_size`; callbacks are only ever appended; a NULL callback means "documented
default" (e.g. NULL `pre_render` = identity ROI, NULL `can_execute` = always enabled).
A new *incompatible* callback set is a new `vtable_version` (e.g. `BaeEffectVTable2`) and
the host keeps calling version 1 plugins through version 1 semantics.

### 14.4 Capability negotiation

Three directions, all at runtime:
1. Host → plugin: `BaeHost.capabilities` (`GPU_COMPUTE`, `GPU_NATIVE`, `OUT_OF_PROCESS`,
   `HEADLESS`), `get_suite` returning NULL for unsupported suites, `BaeGpuDeviceArgs.features`.
2. Plugin → host: `BaeEffectDescription` (flags, threading, devices, formats), vtable
   `struct_size` and non-NULL callbacks, dynamic flags.
3. Plugin → plugin: `BaePluginExports.get_suite` lets a plugin export versioned suites to
   its dependents (§5.3.5), with the same freezing rule.

### 14.5 Compatibility policy

1. **ABI 0.x** (design and early implementation): no stability promise; 0.x hosts accept
   only the minors they list (§6.6). ABI **1.0 is frozen at milestone M1** *(implemented)*.
   The 0.1 pre-release has the identical layout: 1.0 hosts list it in
   `compatible_0x_minors` and present `abi_version` = 0.1 to such plugins. Suites and
   contribution kinds a 1.0 host does not serve yet (`get_suite` returns NULL) are
   *provisional*: their layout is frozen when a host first serves them. BAE 1.0 serves
   the entry point, effects (filters, CPU path: `global_setup`, `define_params`,
   `pre_render`, `render_cpu`, `release_frame_data`, `global_setdown`), `bae.core` 1,
   `bae.param.define` 1 and `bae.render` 1.
2. **Within a major**, only additive changes are allowed: new suites and suite versions,
   fields appended to extensible structs, callbacks appended to vtables, new flag bits and
   enum values (consumers ignore unknown ones, R9), new contribution kinds. Never:
   removing/reordering/retyping fields, changing semantics of an existing field or
   callback, renumbering constants, changing a released suite version.
3. **Old plugin on new host** (same major): guaranteed. CI keeps the example plugins built
   with every released SDK minor as binary artifacts and runs them against the current
   host (acceptance test, §22.3).
4. **New plugin on old host**: the manifest `api` lets an old host reject it before any code
   loads, with "update BAE". Plugins that use newer suites *optionally* (NULL check) keep
   `api` low.
5. **New major**: only when unavoidable; the host serves the previous major through a
   compatibility shim for at least **two years** after the new major ships.
6. `bae-plugin-sdk` (Rust) major version tracks the API major; Rust-only API changes in the
   SDK never change the ABI.
7. Generation: once the ABI is close to 1.0 the header is generated by cbindgen from
   `bae_plugin_sdk::abi`; until then the header is authoritative and the Rust mirror is
   checked by layout tests and a constant-drift test (both directions).

### 14.6 Binary rules

- Export only `bae_plugin_entry` (other exported C symbols are ignored; libraries are
  opened `RTLD_LOCAL`). Never link against BAE itself; everything comes through suites.
- Statically link language runtimes where possible (Rust does by default; C++: static
  libc++ / `/MT`), so two plugins can use different runtime versions.
- macOS: arm64 and x86_64 are separate platform keys (no universal binaries required);
  minimum macOS 12; code-signed. A notarized BAE with hardened runtime needs the
  `disable-library-validation` entitlement to load third-party in-process plugins, or the
  out-of-process runner (packaging decision for 01/21, §23).
- Windows: x64 DLL; loaded with `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR` so the plugin's own
  directory resolves its dependencies; never assume a shared CRT with the host.

---

## 15. Out-of-process mode

### 15.1 When

*(implemented, M1)* A plugin crash must never take BAE down (hard requirement, M1), and an
in-process crash cannot be recovered (§16 item 5). Native plugins therefore run out of
process unless they are trusted for in-process use:

| Manifest `isolation` | Bundled / system plugin | User / project plugin |
|---|---|---|
| `in-process` (default) | in process | out of process, unless the user trusts it for in-process use |
| `prefer-out-of-process` | out of process, unless trusted | out of process, unless trusted |
| `out-of-process` | out of process | out of process |

"Trusted" = the Plugin Manager toggle (stored in user settings; until that UI exists:
`BAE_PLUGIN_IN_PROCESS=<id>,<id>` or `*`). The cost is small for the CPU path, which
already pays a GPU readback (§15.4); in-process stays available for trusted plugins and
is required for `bae.gpu.native`. AE plugins (20) use the same mechanism. The runner is
the application binary itself (`bae plugin runner`, hidden subcommand), so packaging
ships no extra executable; `BAE_PLUGIN_RUNNER` names a dedicated runner (the
`bae-plugin-runner` binary of `bae-plugin`, used by tests).

### 15.2 Architecture

- `bae-plugin-runner`: a small helper executable shipped with BAE. One runner process per
  native library (all contributions and instances of that plugin share it, matching
  per-module globals), started lazily, connected by a local socket/pipe.
- Control channel: length-prefixed messages over the runner's stdin/stdout (v1: JSON,
  one request in flight per runner; the runner first points its stdout at stderr so
  plugin `printf` cannot corrupt the channel; plugin log lines travel with the
  responses). Suite calls that need host state
  (`checkout_input`, `checkout_param`, `bae.command`) become synchronous RPCs back to the
  host; per-pixel helpers (`parallel_for`, `alloc_cpu_image`) run inside the runner.
- Frames: CPU images live in a shared memory file mapped by both processes (v1: a
  temporary file with `memmap2`), allocated by the host, page-aligned, rows 64-byte
  aligned, grown on demand and reused across frames. The runner hands slot memory to the
  plugin directly.
- Host side: `OutOfProcessEffect` implements `bae_render::Effect` and forwards; the render graph
  cannot tell the difference except for cost.

### 15.3 Supported subset

Effects on the CPU path, shape operators, importers, exporters, commands, decoders and
encoders. Not supported out of process: `bae.gpu.native` (no shared device;
`BAE_HOST_CAP_GPU_NATIVE` is cleared) and, in v1, `bae.gpu.compute` (planned: the runner
records dispatches — pipeline ids, bindings, uniform bytes — and the host replays them,
which the host-encoded design makes possible). Expression libraries always run in
process (call granularity too fine).

### 15.4 Cost

Per frame: the in-process CPU-node round trip (§10.6) + one memcpy per direction between
the readback/upload staging buffers and the shared slot + 2 IPC round trips (pre_render,
render; ~20–50 µs each) + RPCs for checkouts (pre-sent where possible: the host includes
param values and input infos in the request). Budget: ≤ 0.3 ms per frame plus the copies
(§22).

### 15.5 Crashes and hangs

- Runner death (pipe EOF / process exit) fails in-flight calls with `BAE_ERR_INTERNAL`; the
  nodes pass their input through with an error badge (§10.10); the runner restarts
  lazily, globals are set up again and instances are recreated from the state blobs.
- Three crashes within 60 s disable the plugin for the session (user can re-enable).
- Watchdog: render calls time out after 10 s, other calls after 30 s (configurable); a hung
  runner is killed and treated as a crash.
- Out-of-process mode is crash isolation, not a security sandbox: the runner has the user's
  privileges (§21).

---

## 16. Errors, crashes, diagnostics

1. **Status + message.** Callbacks return `BaeStatus`; `bae.core.set_error` attaches a
   per-thread message to the error being returned. The host wraps it into a diagnostic
   with plugin id, version, contribution, callback name and (for effects) the effect Prim.
2. **No unwinding across the boundary** (R8). The Rust SDK wraps every callback in
   `catch_unwind` → `BAE_ERR_PANIC`; C++ plugins must catch all exceptions.
3. **Render errors**: §10.10 (pass-through + badge; final renders fail unless configured).
4. **Load errors**: the plugin goes to `Failed` with diagnostics in the Plugin Manager;
   its effects are placeholders (§20). `bae plugin check` reports the same diagnostics.
5. **In-process crashes** cannot be recovered. Every host→plugin call sets a per-thread
   breadcrumb ("plugin X, contribution Y, callback Z"; two TLS stores, ~1 ns) that the
   crash handler of `bae-platform` writes into the crash report (request R01-6). On the
   next start BAE offers: disable the plugin, run it out of process, or keep it.
6. **Hangs** in process: a watchdog reports "plugin X is not responding" with the
   breadcrumb (it cannot kill the call) and offers out-of-process mode for next time.
7. **Validation mode** (`--plugin-validate`, also used by CI for examples): checks result
   rects against `max_result_rect`, fills outputs with a sentinel pattern to detect
   unwritten pixels, checks for NaN/Inf, renders twice to check determinism, checks
   instance serialize/restore round trips and that `param_changed` edits are undoable. It
   is a developer tool (like the OpenFX validator) and costs nothing when off.
8. **Logging**: `bae.core.log` goes to `tracing` with target `plugin::<id>`, rate-limited
   per plugin.
9. **GPU**: WGSL compile errors → diagnostics with file:line; wgpu validation errors during
   a plugin's dispatch are attributed to the plugin; device loss → §12.5.

---

## 17. Script plugins

### 17.1 Runtime

- QuickJS through `rquickjs`, the same library as the expression engine (03), so the
  binary contains one JS engine. Each script plugin gets its **own runtime** (isolation,
  per-plugin memory limit `memory_limit_mb`, interrupt handler for time limits).
- Runtimes execute on a small pool of script workers (S); one runtime runs on at most one
  thread at a time. Scripts never block UI or rendering.
- ES modules. The entry module exports the functions named by `entry` keys. Imports:
  package-relative paths, `plugin:<id>/<path>` from dependencies' `[exports] js`, and the
  built-in module `bae`.

### 17.2 Object model

AE scripting (ExtendScript) names, so AE scripts port with little change:

```js
import { app } from "bae";
const comp = app.project.activeItem;                 // CompItem
for (const layer of comp.selectedLayers) {           // Layer
  const pos = layer.transform.position;              // Property
  pos.setValue([Math.round(pos.value[0] / 10) * 10, pos.value[1]]);
  layer.effect("Gaussian Blur")("Blurriness").setValueAtTime(1.0, 20);
}
```

Objects: `app` (project, version, settings), `Project` (items, activeItem, selection,
importFile), `CompItem`/`FootageItem`/`FolderItem`, `Layer` (name, index, inPoint, outPoint,
startTime, enabled, solo, locked, parent, transform, effects, masks), `Property`/
`PropertyGroup` (value, valueAtTime, setValue, setValueAtTime, keys, expression,
addKey/removeKey). ExtendScript-only facilities are replaced: `File`/`Folder` → `bae.fs`
(permissioned), `$.writeln` → `console.log`, ScriptUI → declarative panels (§17.6).

### 17.3 Mutation, undo, commands

- Every mutation is a command invocation; reads see the script's own uncommitted writes.
- All mutations of one entry-point call form **one transaction = one undo step** with
  author = (invoking user or agent, via plugin id); `app.beginUndoGroup(name)` /
  `app.endUndoGroup()` split it, like AE.
- Scripts can invoke any registered command: `bae.commands.invoke(id, args)`; scripts and
  script commands are themselves registered commands, so menus, shortcuts and MCP see them.

### 17.4 Permissions and limits

| Permission | Script API it unlocks | Default |
|---|---|---|
| `project = "read"` | read the object model, `bae.commands.query` | granted |
| `project = "write"` | mutations, `bae.commands.invoke` of mutating commands | consent |
| `filesystem` scopes | `bae.fs` limited to the scopes (`plugin-data`, `plugin-cache` always) | consent per scope |
| `network` origins | `bae.net.fetch` to listed https origins | consent |
| `clipboard` | `bae.clipboard` | consent |
| `process` | `bae.process.spawn` | consent, shown as dangerous |
| `render` | `bae.render.frame(comp, time) → ImageData/PNG` within a render budget | consent |

Checks happen in the host functions (capability style: user-picked files arrive as
handles, not paths). Time limits: 10 s per command call by default (then the user is asked
to continue or cancel), 50 ms per panel event handler; memory per manifest.

### 17.5 Not in the frame path

Scripts never render pixels. Expression libraries written in JS are executed by the
expression engine (03) under expression rules (pure, deterministic, no project access, no
IO) during animation evaluation, where 03 caches results.

### 17.6 Panels

A script panel returns a declarative widget tree (column, row, label, button, slider,
number, text, checkbox, dropdown, list, image, separator) with callbacks; the host renders
it with egui (11) and calls back on events; `panel.update(state)` re-renders (the host
diffs). No HTML/DOM/WebView (ARCHITECTURE §1). Native panels are reserved
(`BAE_KIND_PANEL`): egui types cannot cross a C ABI, so a native panel API would also be
declarative; deferred until a concrete need.

### 17.7 Agents

Plugin commands and scripts appear as MCP tools automatically (name = registry id,
description and JSON Schema from the manifest). Their permission level (read/write)
determines which agent permission tier may call them (ARCHITECTURE §10).

---

## 18. Other contribution kinds

### 18.1 Shape operators (05)

`apply(params, time, input paths) → output paths` (cubic Béziers, AE vertex/tangent
convention), pure and concurrent, evaluated in the shape group's operator order like AE's
Zig Zag or Pucker & Bloat. `BAE_SHAPE_OP_FLAG_PER_PATH` lets the host split work per path.
Params use the same definitions as effects.

### 18.2 Importers

`probe(path, header bytes) → confidence 0..100` (fast, any thread) and `run` on M inside a
transaction: the importer creates content **only by invoking commands** (one undo step,
progress, abortable). File > Import and drag-and-drop pick the highest-confidence importer
among those claiming the extension/MIME; ties by registry priority (built-ins first).

### 18.3 Exporters

Read-only access (`bae.command.query`), may request rendered frames (permission `render`),
write to the chosen path; run on a worker thread with progress and abort.

### 18.4 Codecs (08)

Decoders: `probe`, `open`, `get_info`, `decode_video` (into a host-allocated packed or
planar image: NV12/P010/YUV formats are converted to the working format on the GPU by
08), `decode_audio`, `close`. Encoders: `open` (declares the pixel format it wants),
`encode_video`, `encode_audio`, `finish`, `close`. Handles are serialized per handle.
Registry priority decides among codecs claiming an extension (system codecs of 08 are
built-in contributions). The optional FFmpeg download (ARCHITECTURE §1) is a native codec
plugin through this interface.

### 18.5 Commands

Registry id `plugin.<plugin-id>.<command-id>`; JSON Schema for arguments is required (it is
also the MCP tool schema); `execute` runs on M in a transaction with the invoking author;
`can_execute` (optional) drives menu enablement and is called at most once per UI frame.

### 18.6 Panels, workspaces, keymaps

Panels: §17.6. Workspaces: a TOML layout (egui_dock tree of panel ids, sizes) registered
in the workspace registry (11). Keymaps: shortcut → command id per workspace; precedence
user > plugin > built-in; conflicts are listed in the shortcut editor.

### 18.7 Expression libraries (03)

`namespace.function(args)` in expressions (`easing.outBack(t)`). Forms: a JS module (runs
in 03's expression runtime) or native `BaeExprFunction`s (pure, numeric, scalar..vec4, no
allocation, thread-safe). Namespaces are unique; documents using a library record it in
the project plugin lock (§6.7).

### 18.8 Text selectors

Reserved (`[[text_selector]]`, `BAE_KIND_TEXT_SELECTOR`): parsed and listed, never loaded,
until 05 defines the per-glyph evaluation contract.

---

## 19. Built-in features through the plugin path (dogfooding)

1. Built-in effects, importers, exporters, codecs, commands, panels, workspaces and
   keymaps register into the **same registries** with the **same descriptors** as plugins,
   with `Owner::Module(<module>)` instead of `Owner::Plugin(<id>)`.
2. Built-in effects (07) are authored either as
   - **declarative packages embedded in the binary** (`crates/bae-effects/packages/<id>/`
     with `bae-plugin.toml` + WGSL, `include_str!`'d and registered through
     `DeclarativeEffect::from_embedded`), or
   - **Rust `bae_render::Effect` implementations** registered in `bae_render::EffectLibrary`
     with their `EffectDef` in `bae_model::EffectRegistry`.
3. **No private host API for built-ins.** If a built-in needs something the SDK lacks, it is
   added to the SDK (and therefore to the C ABI) first. Compositor-internal operations
   (layer transforms, blend modes, masks, track mattes, motion blur, 3D) are not effects
   and are exempt.
4. Built-in effects use the plugin id `bae.effects` (`bae.effects/gaussian-blur`) and declare
   `ae_match_names` for AE import (15/20).
5. Built-in packages are in the *bundled* scope but compiled in: no file IO, no parsing
   cost at startup beyond registering descriptors (manifests parsed lazily on first use or
   at build time via a `const` table).
6. Acceptance (implementation wave): 07 ships at least one pixel-style effect, one
   multi-pass compute effect and one Rust CPU/GPU effect through these paths, and a CI test
   enumerates all built-ins via the plugin registries.

---

## 20. Missing plugins and placeholders

1. **Document data** (02) for an effect: `type = "<plugin-id>/<contribution-id>"`, writer
   `plugin_version`, values by param id (static values, keyframes, expressions), state blob
   + `state_plugin_version`, enabled flag, and a **cached schema snapshot** (names, types,
   groups) so a missing plugin's params can still be listed.
2. **Render**: placeholders are removed from the graph (identity, §10.8) and flagged; a
   final render warns (or fails, if configured).
3. **UI**: Effect Controls shows "Missing plugin: <name> (<id> <version>)" with the cached
   params (raw values editable, nothing interpreted); the Plugin Manager lists missing
   plugins with the projects that use them.
4. **Round trip**: commands never touch unknown data, so saving keeps everything; AE export
   (15) writes the original matchName and parameter data back.
5. **Version cases**: newer document than installed plugin (same major) → unknown param ids
   are preserved and hidden, the state blob is passed with `state_plugin_version` and the
   plugin may refuse it (`BAE_ERR_PARSE` → placeholder); different major → migration
   (§8.3.4) or placeholder with an "update plugin" action.
6. **Runtime unload** turns live effects into placeholders without changing the document
   (§7.4); re-installing brings them back at the next graph build.

---

## 21. Trust, permissions, sandboxing

### 21.1 Trust by form

| Form | Risk | Measure |
|---|---|---|
| Declarative WGSL | GPU hang / excessive work (no host API, no memory unsafety: WGSL access is bounds-checked by naga/wgpu) | dispatch-size limits, GPU timeout handling, device-loss recovery |
| Script | whatever the granted API allows | QuickJS isolation + enforced permissions (§17.4) + time/memory limits |
| Native | full user privileges, in-process memory access | consent at install; permissions are declarations shown to the user and checked only on host-mediated APIs; out-of-process mode for crash isolation (not security) |

### 21.2 Scopes and consent

- Bundled and system plugins are trusted; user plugins are trusted after the install
  consent dialog (permissions, native code, platforms).
- **Project plugins** (`<Project>.bae/plugins/`) travel with shared projects: declarative
  packages load automatically (they cannot do harm beyond GPU load); script and native
  plugins load only after the user trusts them for this project (stored in user settings,
  keyed by project id + plugin content hash; a changed hash asks again).
- Agents installing plugins (`plugin.install` via MCP) need the highest agent permission
  tier and still trigger the consent dialog for native/script code.

### 21.3 Permissions (manifest `[permissions]`)

`has_permission(name)` strings: `project:read`, `project:write`,
`filesystem:<read|write>:<scope>`, `network:<origin>`, `clipboard:<read|write>`, `process`,
`gpu_native`, `render`. Enforcement: scripts — always (host functions check);
native — host-mediated APIs check (`bae.command.invoke` of mutating commands needs
`project:write`; `bae.gpu.native` needs `gpu_native`), everything else is on trust.

### 21.4 Integrity and signing

At install the host records a content hash of the package; a modified installed package
is reported in the Plugin Manager. Code signing and a marketplace are out of scope (§1.2).

---

## 22. Performance budget and acceptance tests

### 22.1 Budgets

Reference hardware: Apple M1 and an RTX 3060-class Windows PC (ARCHITECTURE §11).

| Item | Budget | Measured by |
|---|---|---|
| Declarative effect vs the equivalent built-in | **< 2 %** frame time (design target 0: same node) | bench `plugin_overhead`: 1080p, 10-effect chain, declarative vs built-in, median of 200 frames |
| Fused chain of N pixel-style effects | ≈ one load + one store + N × ALU | same bench, fused vs unfused |
| Registry lookups on the frame path | 0 (resolved at graph build) | review + bench |
| Native in-process callback overhead | ≤ 50 ns per call (indirect call + arg struct) | microbench in `bae-plugin` |
| Host bookkeeping per native node per frame | ≤ 2 µs (pre_render plumbing, checkouts) | microbench |
| `bae.gpu.compute` dispatch vs built-in dispatch | ≤ 1 µs extra CPU, 0 GPU | microbench |
| `bae.gpu.native` begin/end | ≤ 50 µs CPU (one extra submit) | bench |
| CPU-node round trip, 1080p, RGBA16F ↔ RGBA F32 premultiplied | ≤ 4 ms latency on M1, ≤ 6 ms on RTX 3060; no throughput loss when pipelined | bench `cpu_node_roundtrip` (04) |
| CPU-node round trip, 1080p, RGBA U8 | ≤ 1.5 ms | same |
| Out-of-process extra per frame | ≤ 0.3 ms + one memcpy per direction | bench |
| Startup scan | ≤ 0.5 ms per cached plugin; ≤ 50 ms for 100 plugins | bench `scan` |
| First use of a native plugin | ≤ 20 ms, off the UI thread | trace |
| Declarative compile + pipelines | ≤ 30 ms per effect, async, cached | trace |
| Memory of an installed, unused plugin | manifest only (< 10 KB) | test |
| `bae-plugin` release size | ≤ 300 KB; no new large dependencies (naga, libloading, rquickjs are already in the app) | `cargo xtask size` |

Arithmetic behind the CPU-node row: 1920×1080 RGBA16F = 16.6 MB on the GPU, 33.2 MB as
RGBA F32. Readback and upload are each ~33 MB at ≥ 20 GB/s effective (unified memory on
M1; PCIe 4 ×16 on the 3060) ≈ 1.7 ms each, plus two conversion kernels (< 0.2 ms) and
mapping overhead. U8 moves 8.3 MB per direction.

### 22.2 Hot-path rules (for 04 and 19 implementers)

No allocation, lock or registry lookup per frame on the plugin path; uniform data through
the ring buffer; bind groups cached by resource identity; transient textures pooled;
`is_aborted` is an atomic load; breadcrumbs are TLS stores; suite tables are cached by
the plugin; checkouts recorded in pre-sized per-frame arenas.

### 22.3 Acceptance tests (from the workstream brief)

1. **Declarative overhead** < 2 % vs the same effect built in (bench above, CI trend).
2. **ABI stability**: the native examples built with the oldest supported SDK of the current
   major (binary artifacts kept by CI) load and render correctly on the current host.
3. **Runtime load/unload without restart and without GPU leaks**: load → use in a rendered
   project → unload, 100 times; afterwards the plugin's `GpuResourceLedger` is empty, wgpu's
   resource report is back to baseline and RSS growth is bounded.
4. **Crash isolation**: a test plugin that crashes on frame N in out-of-process mode leaves
   the host running; the node shows the error; the runner restarts; rendering continues.

---

## 23. Requests to other workstreams

These are the interfaces 19 needs. IDs are referenced from code and other specs.

### 23.1 01 foundation (`bae-app`, `bae-gpu`, `bae-platform`)

| ID | Request |
|---|---|
| R01-1 | **Done** (`bae_app::Registry<T>`, `Owner::Plugin`, `App::unregister_owner`, `Entry::is_unique`). Remaining: an **atomic owner swap** for plugin updates (remove all entries of `Owner::Plugin(id)` and register the new version's in one batch with one `RegistryEvent`, §7.4 "Update").|
| R01-2 | A generic way for `bae-plugin` to reach every contribution registry without depending on each domain crate: e.g. a `PluginRegistries` facade in `bae-app` (or a convention: each registry type is a `Resources` entry implementing a small `ContributionRegistry` trait with `register_plugin_entry` / `remove_owner`). |
| R01-3 | `bae_gpu::native`: wgpu-hal access to Metal device/queue, D3D12 device/queue/adapter LUID (Vulkan when enabled), native texture handles + formats from `wgpu::Texture`, D3D12 resource-state transitions for handed-out textures, "submit pending work now", and fence helpers (encode signal/wait of an `MTLSharedEvent` / `ID3D12Fence` on the host queue). |
| R01-4 | `GpuContext` device-lost notification (callback registration) so plugin GPU data is torn down and set up again. |
| R01-5 | `bae-platform` paths: bundled/system/user plugin directories (**done**: `AppDirs::plugin_dirs`), per-plugin data and cache directories; CLI flags `--plugin-dir`, `--plugin-dev`, `--safe-mode`, `--plugin-validate`; env `BAE_PLUGIN_PATH`; settings storage for enabled/trusted plugins. |
| R01-6 | Crash handler: a per-thread breadcrumb slot (static TLS, no allocation) that `bae-plugin` writes around every plugin call and that crash reports include; an API to read the last crash's breadcrumb at startup. |
| R01-7 | **Done in 19's interface PR** (root `Cargo.toml`, two lines): `semver = "1.0.28"`, `naga = "30.0.2"` with `wgsl-in` (the copy wgpu already links). |
| R01-8 | Packaging (with 21): ship `bae-plugin-runner` next to the app; decide macOS hardened-runtime strategy for third-party dylibs (`disable-library-validation` vs out-of-process only). |

### 23.2 02 doc-model (`bae-model`)

| ID | Request |
|---|---|
| R02-1 | Effect Prim fields: `type` = full id `<plugin-id>/<contribution-id>`, writer `plugin_version`, params keyed by stable numeric id (AE disk id) with static value / keyframes / expression, **unknown ids preserved**, opaque **state blob** (bytes, last-writer-wins, synced) + `state_plugin_version`, `enabled`, a stable `instance_uid: u64`. |
| R02-2 | **Done** (PR #11: `EffectDef`, `EffectRegistry`, `ParamKind`, `ParamFlags`, `EffectStatus::MissingPlugin`). Remaining: key the registry by (match name, plugin version) when two versions are installed (§6.4); same registry for shape-operator params. |
| R02-3 | Cached schema snapshot stored with each effect Prim (or per document per (full id, version)) for placeholder UI (§20). |
| R02-4 | Effect-logic hooks without a `bae-model` → `bae-plugin` dependency: a trait (e.g. `EffectLogic`) registered per effect type that commands call synchronously: `init_state` on AddEffect (returns the initial blob), `param_changed` during SetParam for supervised params (may append ops to the same transaction, returns state-changed), never invoked for undo/redo/remote ops (§8.5). |
| R02-5 | Value types: done in `bae_model::Value` (popup = 1-based `Int`, as AE). Remaining: an interpolation hook for ARBITRARY values (`BaeArbitraryVTable1.interpolate`), used by 03. |
| R02-6 | Project plugin lock in `project.json` (§6.7): `[{id, version, contributions, scope_hint}]`, maintained by AddEffect/plugin update commands. |
| R02-7 | `StageEvent` granularity down to (effect Prim, param id / state) so render caches invalidate exactly. |
| R02-8 | `ParamKind::String` (value `String`) and `ParamKind::Asset` (value `AssetRef`) so the manifest's reserved `string` / `asset` params can be enabled. |
| R02-9 | Relative point defaults: `ParamKind::Point { default_relative: bool }` (or a `DefaultValue::LayerRelative`) so AddEffect writes `fraction × layer size`, as AE's `PF_ADD_POINT` does. Until then `bae-plugin` leaves the `EffectDef` default empty and the effect resolves it against the layer bounds. |

### 23.3 04 compositor (`bae-render`)

| ID | Request |
|---|---|
| R04-1 | Effect node executing `Arc<dyn bae_render::Effect>` per §10 (output_bounds / input_roi / render exist): request merging, checkouts of layer params / other times / transition `to`, identity aliasing. |
| R04-11 | `EffectParams` additions for declarative effects (all have AE equivalents): layer bounds at the request scale, comp frame rate and `time_step`, `linear` working-space flag, final/preview flag and quality, shutter angle/phase, a stable per-instance seed. `bae-plugin` fills `BaeFrame` with fallbacks (input rect as layer bounds, 0 for unknowns) until then. |
| R04-12 | A way to construct `EncodeContext` outside `bae-render` for effect tests (e.g. make `ImageAllocator` public or add `EncodeContext::for_tests(gpu, encoder, format)` with a simple pool). |
| R04-13 | Build the renderer's `EffectLibrary` from the app registry `Registry<dyn bae_render::Effect>` (where `bae_plugin::PluginModule` publishes plugin effects, owner `Owner::Plugin(id)`), like `Registry<dyn SourceProvider>`; rebuild graphs after plugin load/unload. |
| R04-2 | CPU nodes: GPU conversion kernels for every `BaePixelFormat` (RGBA/BGRA/ARGB × U8/U16/U16_15BIT/F16/F32 × premultiplied/straight), 64-byte-aligned staging rows, async double-buffered readback/upload, graph splitting so GPU work continues while CPU nodes run. |
| R04-3 | Scheduler honouring `threading`, per-frame abort flags and progress aggregation. |
| R04-4 | Render request fields of §10.1 (flicks time/time_step, frame rate, downsample x/y, PAR, quality, shutter, color space + linear flag, final/preview/headless, layer bounds, layer duration, render id). |
| R04-5 | Cache keys and dependency-based invalidation per §10.3–§10.4 (`UI_ONLY` excluded, `NON_PARAM_VARY`, `NON_DETERMINISTIC`, state revision, dynamic flags). |
| R04-6 | Generic WGSL shader node used by **both** built-in and declarative effects: uniform ring with dynamic offsets, bind-group cache, transient texture pool with aliasing, pipeline cache with async pre-warm, pixel-style fusion (§10.9). 19 supplies prelude generation, validation and `DeclarativeEffect`; 04 supplies execution. |
| R04-7 | `bae.gpu.native` scope support (with R01-3): flush pending work, transition textures, fences, keep textures alive until the plugin's signal. |
| R04-8 | Attribution of GPU objects created on behalf of a plugin (pipelines, pooled temporaries) to its `GpuResourceLedger` for leak checks. |
| R04-9 | Error policy of §10.10 and the "continue on effect errors" render setting. |
| R04-10 | Benchmarks `plugin_overhead` and `cpu_node_roundtrip` in the bench harness (§22). |

### 23.4 Other workstreams

| ID | Request |
|---|---|
| R03-1 | Expression function registry with namespaces; JS module loading for expression libraries; arbitrary-value interpolation hook (R02-5); param access by key / name / index (§8.4). |
| R05-1 | Shape-operator registry and evaluation order contract (§18.1); text-selector contract later. |
| R07-1 | Author built-in effects as embedded declarative packages or SDK `Effect`s (§19), with `ae_match_names`; no private host API. |
| R08-1 | Codec registry with priorities; planar formats and GPU conversion (§18.4). |
| R11-1 | Plugin Manager UI (states, diagnostics, enable/trust/isolation toggles), declarative script panels, workspace/keymap registries, placeholder Effect Controls, reserved custom-draw area for ABI 0.2. |
| R17-1 | MCP tools generated from plugin commands (id, description, schema) honouring permission tiers (§17.7). |
| R20-1 | Confirm the selector and flag mapping of §9 against `bae-aex-host::selector`; requests R19-01/04/05 of 20 are covered by §9.1, §12 and the reserved `ui_event`/`bae.draw`. |
| R21-1 | Embed declarative/script plugins used by a project into `.baez` / Collect Files (`<Project>.bae/plugins/`), use `get_dependencies` for collection, package `bae-plugin-runner`. |

---

## 24. Open questions and decision log

### 24.1 Decisions

| # | Decision | Why |
|---|---|---|
| D1 | One plugin API version for ABI, declarative interface and script API | one compatibility story for users and authors |
| D2 | C ABI with frozen suites + extensible structs (AE PICA / OpenFX style), not a Rust ABI | Rust has no stable ABI; plugins must survive host upgrades |
| D3 | Native params come from `define_params`; the manifest copy is optional and must match | listing without loading code, without two sources of truth |
| D4 | Effects are stateless unless `STATEFUL` | zero cost for the common case |
| D5 | `checkout_id` 0 = self input; NULL `pre_render` = identity ROI | simple effects need no pre-render code |
| D6 | Render errors pass the input through with a diagnostic | previews keep working; final renders still fail loudly |
| D7 | Declarative effects have no per-frame host logic beyond ROI bytecode | guarantees the < 2 % budget |
| D8 | Supervision is not re-run for undo/redo/remote ops | deterministic collaboration |
| D9 | ~~Out-of-process is opt-in for BAE plugins, default for AE plugins~~ Superseded by D16 | performance first; AE plugins are the unstable ones |
| D10 | Project-scoped declarative plugins load without a trust prompt | they cannot do harm beyond GPU load; shared projects just work |
| D11 | Popup values are the 1-based index (AE), choices are append-only; no separate choice ids | matches `bae_model` (merged) and AE projects 1:1 |
| D12 | Automation scripts are `[[automation]]`, not `[[script]]` | `[script]` is the runtime table; TOML cannot hold both |
| D13 | The render node trait is `bae_render::Effect` (04); definitions are `bae_model::EffectDef` (02) | both merged first; one trait for built-ins and every plugin form |
| D14 | No samplers in the WGSL prelude; `bae_sample_*` is bilinear over `bae_load_*` | identical results for every working format (RGBA32F is not filterable everywhere), AE edge semantics |
| D15 | Uniform member offsets are read from the naga IR of the compiled module | host packing and shader layout cannot drift apart |
| D16 | (M1) Native plugins run out of process unless trusted (bundled/system with `in-process`, or user-trusted); the runner is `bae plugin runner` | "a plugin crash must not take BAE down" is a hard requirement; the CPU path already pays a readback, so isolation costs little |
| D17 | (M1) ABI 1.0 = the 0.1 layout, frozen; 0.1 plugins still accepted; unserved suites are provisional | stable promise for what is implemented without freezing untested designs |
| D18 | (M1) Native CPU effects are a `bae_render::Effect` that submits the frame's encoder, reads its input back, calls the plugin and uploads the result; a failing call passes the input through | one render-graph trait for every form; a broken plugin never fails a frame |

### 24.2 Open

1. Licensing of the plugin API and SDK (ARCHITECTURE §14 "许可"): undecided.
2. WASM as a sandboxed portable CPU form (contracts are form-agnostic).
3. Custom UI (`ui_event`, `bae.draw`) details for ABI 0.2, with 11/12 and 20.
4. `bae.scene` contents (camera, lights, comp info) with 04 and 20.
5. Audio effects (`BAE_KIND_AUDIO_EFFECT`) once BAE has an audio graph.
6. Multi-GPU (`device_index`).
7. Out-of-process `bae.gpu.compute` via record/replay; OS sandbox profiles for runners.
8. Text selectors with 05.
9. Signing, marketplace, paid plugins.
