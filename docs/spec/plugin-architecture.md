# BAE Plugin Architecture

| | |
|---|---|
| Status | **Design v0.1** (workstream 19 design phase). Normative for 02, 04, 07 and 20. |
| Owner | 19 `core/plugins` |
| ABI | `sdk/c/bae_plugin.h`, ABI 0.1 (major 0 = unstable; first frozen ABI is 1.0 at M1) |
| Rust | `crates/bae-plugin-sdk` (shared contract + author SDK), `crates/bae-plugin` (host) |
| Related | `docs/ARCHITECTURE.md` §2, §14, §15, §16; `docs/spec/aex-compat.md` (20) |

The key words MUST, MUST NOT, SHOULD, MAY are used as in RFC 2119. "Host" is the BAE
application (or `bae --headless`, or `bae-plugin-runner` in out-of-process mode).

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
  the *same trait* (`bae_plugin_sdk::DynEffect`); the native one adds an FFI hop.
- **P4 Commands only.** Plugins never mutate the document directly. Edits are command
  invocations or ops recorded into the current transaction (ARCHITECTURE §2.1).
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
| `[[script]]` | script | registered as a command `script.<plugin-id>.<id>` (02) | script thread | — |

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

The canonical effect contract is the Rust trait pair in `bae-plugin-sdk`:

```
bae_plugin_sdk::Effect          author-facing, static types (associated Instance type)
bae_plugin_sdk::DynEffect       object-safe projection; blanket impl for every Effect
bae_plugin_sdk::RenderServices  host services during pre-render / render (inputs, abort, parallel_for, gpu)
bae_plugin_sdk::GpuServices     host-encoded GPU compute + native interop scope
```

- **Built-in Rust effects** implement `Effect` and are registered as `Arc<dyn DynEffect>`.
- **Native plugins**: the plugin implements `Effect` (Rust SDK) or the C vtable directly;
  `export_plugin!` projects `Effect` onto `BaeEffectVTable1`; the host's `NativeEffect`
  adapter (in `bae-plugin`) projects the vtable back onto `DynEffect`. Host services go the
  other way: the host implements `RenderServices` once and exposes it to native plugins as
  the `bae.render` / `bae.gpu.*` suites by thin `extern "C"` shims.
- **Declarative effects** are executed by `DeclarativeEffect` (in `bae-plugin`), which
  implements `DynEffect` on top of `GpuServices`. The render graph (04) therefore sees a
  single node interface; it MAY downcast (`DynEffect::as_any()` → `DeclarativeEffect`)
  to inspect the WGSL description for optimizations (fusion of pixel-style chains, §10.9).
- The render graph never needs to know which form it is running.

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
| Popup choice id | integer 1..=2³¹−1, unique per param, never reused | `2` |
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
format = "working"                       # "working" (default) | "rgba16f" | "rgba32f" | "r16f" | "r32f" | "rg16f" | "rg32f" | "rgba8unorm"
rect = "output"                          # "output" (default) | "input": which rect it covers
scale = 1.0                              # (0, 1]: resolution relative to that rect (0.5 = half res)

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
| `popup` | `default` (choice id), `choices = [{id=1,name="Circle"}, {separator=true}, …]` | u32 choice id | `u32` |
| `layer` | `default` ("none"/"self") | layer ref | texture `bae_tex_<key>` + rect |
| `group` | `collapsed`(false) | — | — |
| `button` | `label` | — | — |
| `string` | `default`, `max_length`, `multiline` | UTF-8 | — |
| `asset` | `extensions` | asset ref | — (native only) |
| `arbitrary` | — (native only) | bytes | — |
| `path` | — | mask ref | — (native only) |

Param `flags` (manifest spelling): `hidden`, `disabled`, `supervise`, `not-animatable`,
`hold`, `start-collapsed`, `ui-only`, `no-expressions`, `transition-progress`. Numeric
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

[[script]]
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
kept), which is also their MCP tool name; scripts get `script.<plugin-id>.<script-id>`.

#### 5.3.9 Localization (`locales/<bcp47>.toml`)

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
[effect.vignette.choices.shape]          # popup key → choice id → label
1 = "圆形"
2 = "椭圆"

[command.align-to-grid]
name = "对齐到网格"
description = "把选中的图层吸附到 N 像素网格。"
```

Categories and menu paths use well-known English keys (`Stylize`, `Blur & Sharpen`, …)
that the host translates with Adobe's official Chinese terms (`docs/glossary.md`, 11).
Missing translations fall back to the manifest strings. Native plugins may also pass
`l10n_key` in `BaeParamDef` to look up `[effect.<id>.params]`.

### 5.4 Validation

`bae plugin check <dir|file>` and the loader run the same validation (in `bae-plugin`):
ids/keys syntax and uniqueness; versions parse; `api` major supported; paths safe and
existing; every `implementation = "native"` contribution has a library for at least one
platform; `[native]`/`[script]` present iff used; param ids/keys/choice ids unique and
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
  host keeps their stored values), popup choice ids are never reused, state blobs stay
  readable.
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
   builds and tests ("no GPU leak" acceptance test, §22.4).
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
stay placeholders (§20.5).

### 8.4 Parameters

- `define_params` is called once per process (L) and MUST depend only on the plugin
  version and locale-independent data (it is cached). Display names may be localized via
  `l10n_key`; dynamic UI changes go through `update_params_ui`.
- The host converts the definitions into an **effect schema** registered with 02's schema
  registry under (full id, plugin version): stable numeric ids (AE disk ids), keys, types,
  defaults, ranges, popup choice ids, group structure, flags.
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
| `POPUP` | `POPUP` (AE 1-based index `i` ↔ choice id `i`) |
| `GROUP_START` / `GROUP_END` | `GROUP` + `group_id` of children |
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
the node holds an `Arc<dyn bae_plugin_sdk::DynEffect>` resolved once at graph build (§7.3).

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
   back edge returns `EMPTY` and a diagnostic.

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
  entry   = effect_registry.get(full_id)          // Arc<dyn DynEffect>, a lease (§7.3)
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
