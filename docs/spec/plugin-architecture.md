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
  single node interface; it MAY call `DynEffect::as_declarative()` to inspect the WGSL
  description for optimizations (pass fusion of pixel-independent chains, §10.9).
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
identity_when = "params.amount == 0"     # optional expression (§11.7): node is skipped when true
[effect.roi]                             # optional (§11.6)
extent = "input"                         # "input" (default for filter) | "layer" (default for generator) | "infinite"
read = "0"                               # or read_left/read_right/read_top/read_bottom
grows = false                            # result rect grows by the mirrored read footprint

[[effect.param]]                         # declarative: authoritative; native: optional listing copy (must match)
# ... §5.3.7

[[effect.texture]]                       # declarative intermediates (§11.5)
[[effect.pass]]                          # declarative passes (§11.5)
```

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

1. **Manifest check (no code loaded):** plugin `api = "A.a"`; host API `H.h`; host
   supports majors `S` (e.g. {H, H−1}). Accept iff `A ∈ S` and (`A < H` or `a ≤ h`).
2. **Binary check (native, after `bae_plugin_entry`):** `abi_required` MUST equal the
   manifest `api` (stale-manifest guard); the host re-applies rule 1 to `abi_required`;
   `abi_built` is informational (it tells the host which struct sizes the plugin knows).
3. **Plugin check:** the plugin sees `BaeHost.abi_version` and MAY refuse with
   `BAE_ERR_VERSION_MISMATCH` (the SDK does this automatically when the host is older
   than `abi_required`).
4. **Feature negotiation:** optional features are discovered at runtime:
   `get_suite(name, version)` returns NULL when unsupported; `BaeHost.capabilities` lists
   host features; extensible structs are read with `BAE_HAS_FIELD`.
5. **Majors:** a host supporting an older major `A < H` does so through a host-side
   compatibility shim that presents the old suites/vtables. Policy (§14.6): each major
   is supported for at least two years after its successor ships.

Major 0 (this draft) is exempt: 0.x hosts accept only the exact same 0.x minor or newer
minors that are explicitly declared compatible in `bae-plugin` (`COMPATIBLE_0X_MINORS`).

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

