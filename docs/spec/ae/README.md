# After Effects basic-feature inventory (AE 基础功能清单)

**中文说明。** 这个目录是 BAE 要实现的 After Effects 基础功能的权威清单，按领域分成 7 个文件，每个功能一条，有固定编号（`AE-<领域>-NNN`）、精确语义（参数、默认值、范围、枚举顺序、公式）、优先级（P0/P1/P2）和负责的 crate。各工作流实现功能时按条目写的做；发现 AE 的实际行为和条目不一致，就在 PR 里改条目并给出出处或实测数据。标了 `UNVERIFIED:` 的地方是目前没能用公开资料核实的内容，实现时以实测为准并回写。条目正文用英文（便于对照 AE 的英文界面、脚本 API 和文档），功能名和参数名后面括注 AE 简体中文版的官方译名。

## Purpose

This is the authoritative list of After Effects functionality that BAE implements. Implementers build exactly what an item says; precision beats prose. Each item specifies AE's behaviour (defaults, ranges, units, enum order, formulas, evaluation order, edge cases); BAE-side choices are marked "Implementation hint:" or "BAE decision:" in Notes.

To change the inventory, open a PR that touches `docs/spec/ae/**`: edit the item in place, cite the source (Adobe documentation, scripting/expression references, Lottie, or a measurement in real AE), keep IDs stable, and run the linter (see Tooling).

## Files

| File | Area | ID ranges | Scope |
|---|---|---|---|
| [project-comp-layers.md](project-comp-layers.md) | PRJ | 001-299, 300-699 | Project items and folders, import, Interpret Footage, proxies, solids, placeholders, Project Settings, undo, Collect Files; Composition Settings, work area, markers, every layer type, the layer property tree, layer timing, labels, switches, parenting, stacking, layer editing, precompose, layer styles |
| [transform-keyframes-expressions.md](transform-keyframes-expressions.md) | ANI | 001-399, 400-999 | 2D/3D transforms and matrix order, parenting and auto-orient math, keyframe model, temporal/spatial interpolation, ease model, Graph Editor semantics, keyframe assistants, time stretch/remap, frame blending, motion-blur sampling; the complete expression language |
| [compositing-3d-color.md](compositing-3d-color.md) | CMP | 001-399, 400-799 | Pixel/alpha model, render order, blend modes, track mattes, masks, adjustment layers, collapse/continuous rasterize, nested comps, quality, motion-blur rendering; Classic 3D, cameras, lights, materials, color management, preview resolution/ROI/channels/exposure |
| [shapes.md](shapes.md) | SHP | 001-599 | Shape layer structure and order of operations, primitives, fills, strokes, gradients, path operators, group transforms, shape conversion commands, shape tools, Bezier model |
| [text.md](text.md) | TXT | 001-599 | TextDocument, point/paragraph text, layout rules, Path/More Options, animators and selectors, text tools, Character/Paragraph data, CJK, Lottie text |
| [effects.md](effects.md) | FX | 001-299, 300-599, 600-899 | Effect framework (stack, parameter types, Compositing Options, bit depth, bounds, presets); every built-in effect BAE provides: Expression Controls, Color Correction, Channel; Blur & Sharpen, Stylize, Perspective, Distort; Generate, Transition, Matte, Keying, Noise & Grain, Time, Simulation, Audio, Text, Utility |
| [ui-preview-render-io.md](ui-preview-render-io.md) | UI | 001-299, 300-599, 600-899 | Panels and viewers (Project, Composition, Layer/Footage, Timeline, Graph Editor, Effect Controls, ...), workspaces; tools, keyboard shortcuts, menus, viewer interaction, preview and cache; Render Queue, Render Settings, Output Modules, headless rendering, import/export formats, Preferences |

`speclint.py` is the linter and index generator for these files.

## ID scheme

- IDs have the form `AE-<AREA>-NNN` (three digits). Each file owns one area code; each area is split into the ID ranges listed above (one range per drafting part, roughly one per topic group).
- IDs are permanent: never renumbered, never reused. Additions take the next free number in the range whose topic fits, and sit in the right section even if that puts them out of numeric order.
- An item that goes away keeps its heading and all five fields and becomes a pointer ("Moved: see AE-XXX-NNN in <file>").
- Cross-references use the bare ID (and the file name when it is in another file).

## Item format

```markdown
### AE-XXX-NNN · English feature name (官方中文名)

- **What it does:** 1-3 sentences.
- **Exact semantics:** the implementable definition: parameter tables in AE UI order, enum values in AE menu order (with scripting enum / Lottie value), formulas as pseudocode, evaluation order, edge cases (zero, negative, out of range, 8 vs 16 vs 32 bpc).
- **Priority:** P0 | P1 | P2 (+ a few words of justification)
- **Owner:** <primary owner>[ (+ <secondary>, <secondary>)]
- **Notes:** gotchas, AE quirks, version differences, scripting names and matchNames, Lottie keys, cross-references, "Implementation hint:", "BAE decision:". "—" if nothing.
```

- The heading text after the ID is the item's Name. `·` is U+00B7 with one space on each side. `##` headings group items into sections; `####` is not used.
- The five fields appear exactly once each, in this order, at column 0; sub-content is indented under a field.
- `UNVERIFIED: <best understanding>` marks a claim that could not be confirmed from public sources. It is written inline, next to the claim. A correct UNVERIFIED beats a confident wrong value; replace it when a source or a measurement settles it.
- `（译名待核）` after a Chinese term marks a translation that could not be confirmed against AE's zh-CN UI. Sources for terms: the BAE glossary (`docs/glossary.md`, workstream 11, verified against AE 2026 zh-CN), then Adobe's zh-CN help pages. AE zh-CN uses 蒙版 for Mask and 遮罩 for Matte (轨道遮罩 = Track Matte).
- "Implementation hint:" = a suggested BAE approach (GPU, caching, data structures). "BAE decision:" = a place where BAE deliberately differs from AE or extends it.
- Adobe documentation is paraphrased, never copied at length. No Adobe images, icons, sample projects, SDK code or headers.

## Priorities

- **P0**: basic AE functionality every AE user expects. Milestone M1 ("a faster AE") must ship it.
- **P1**: commonly used.
- **P2**: advanced or rare.

## Owners

Every item has one primary owner (the crate doing the hardest part) and optional secondary owners.

| Owner | Directory | Workstream (branch) | Responsibility |
|---|---|---|---|
| bae-model | `crates/bae-model` | 02 (`core/doc-model`) | Project data model: project items, folders, comps, layers, every persisted field and setting, property tree and value types, markers data, undo/history, basic edit Commands |
| bae-anim | `crates/bae-anim` | 03 (`core/animation`) | Time evaluation: keyframe interpolation, transforms and matrices, parenting, auto-orient math, expressions (QuickJS), time stretch/remap math, motion-blur sample times |
| bae-render | `crates/bae-render` | 04 (`render/compositor`) | GPU compositor: render graph, per-layer render order, blend modes, masks, track mattes, adjustment layers, collapse/continuous rasterize, 3D layers/cameras/lights, motion-blur accumulation, frame blending, quality/sampling, caching, preview resolution/ROI |
| bae-vector | `crates/bae-vector` | 05 (`render/vector-text`) | Shape layers: paths, primitives, fills/strokes/gradients, path operators |
| bae-text | `crates/bae-text` | 05 (`render/vector-text`) | Text layout, fonts, TextDocument, text animators and selectors |
| bae-color | `crates/bae-color` | 06 (`render/color`) | Color management: bit depth, working space, linearize, 1.0-gamma blending, input/output/display transforms, OCIO subset |
| bae-effects | `crates/bae-effects` | 07 (`render/effects`) | Built-in effects (WGSL) |
| bae-media | `crates/bae-media` | 08 (`io/media`) | Import/decode/encode, image/video/audio formats, footage decoding per Interpret Footage, proxy generation, output-module encoding |
| bae-ui | `crates/bae-ui` | 11 (`ui/shell`) | App shell: docking, panel framework, theme, keymap infrastructure, generic widgets, menu framework |
| bae-motion | `crates/bae-motion` | 12 (`ui/motion`) | AE-domain editing commands (precompose, split/duplicate/trim layers, sequence layers, keyframe assistants, Easy Ease, ...). Commands only, no UI |
| bae-ui-motion | `crates/bae-ui-motion` | 12 (`ui/motion`) | AE-style workspace UI: Project panel, Composition/Layer/Footage viewers, Timeline, Graph Editor, Effect Controls, Character/Paragraph, Align, Info, Preview, Render Queue UI, tools |
| bae-plugin | `crates/bae-plugin` | 19 (`core/plugins`) | Plugin host, registration, effect API, AE-plugin compatibility reservations |
| bae-package | `crates/bae-package` | 21 (`io/packaging`) | Collect files, reduce project, single-file packaging (.baez) |
| apps/bae | `apps/bae` | 01 (`core/foundation`) | Application assembly: module wiring, menus, CLI / headless render |

Pure data and organisation features (items, folders, labels, marker data, settings storage) have bae-model as primary owner.

## Tooling

```sh
python3 docs/spec/ae/speclint.py docs/spec/ae/<file>.md          # lint one file (exit code 1 on errors)
python3 docs/spec/ae/speclint.py docs/spec/ae/<file>.md --range 001-299,300-599 --area FX   # also check ID ranges and area
python3 docs/spec/ae/speclint.py --json                          # all items as JSON (filter by owner/priority with jq or python)
python3 docs/spec/ae/speclint.py --readme                        # regenerate the generated section below
```

## Index (generated)

Regenerate this section with `python3 docs/spec/ae/speclint.py --readme` after editing any inventory file; do not edit it by hand.

<!-- BEGIN GENERATED: python3 docs/spec/ae/speclint.py --readme -->
<!-- END GENERATED -->

## 进度（工作流 00）

- [ ] 7 inventory files written and verified against public sources
- [ ] README with owner map and P0 checklist
- [ ] Resolve remaining UNVERIFIED claims
- [ ] Keep the inventory in sync as implementers find AE behaviour differences

**请求（对其他工作流）：** 实测 AE 数值的工作流（例如 15 interop/ae 的导出脚本、03 的 AE 对照测试）请用实测结果替换相关条目里的 `UNVERIFIED:`，并在 Notes 里写明测量方法和 AE 版本。
