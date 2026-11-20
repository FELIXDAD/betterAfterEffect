# After Effects inventory: text (AE-TXT)

**Scope.** This file specifies everything about After Effects text layers that BAE must reproduce: point and paragraph (box) text and their creation/conversion, the TextDocument value stored in Source Text (every character and paragraph attribute with type, default, range and meaning), Source Text keyframes and expressions, the layout rules `bae-text` must follow (line breaking, leading, tracking, kerning, justification, indents, box placement, glyph fallback, fill/stroke order), Path Options, More Options, text animators and every animator property with its application maths, the Range / Wiggly / Expression selectors with exact shape maths, per-character 3D, text conversion commands (Create Shapes/Masks from Text, Convert to Editable Text), fonts and missing fonts (matching, substitution, variable and colour fonts), CJK / vertical / RTL text, and the interchange names (AE matchNames, Lottie keys). Sibling files: README.md, project-comp-layers.md, transform-keyframes-expressions.md, compositing-3d-color.md, shapes.md, text.md, effects.md, ui-preview-render-io.md. The item format, ID scheme, priorities and owners are defined in README.md.

Conventions used in this file:

- "Character" means one Unicode text element of `TextDocument.text` as AE counts it (AE counts UTF-16 code units for scripting indices; see AE-TXT-012); "glyph" means the shaped output glyph.
- Lengths are in layer pixels at 100% layer scale unless a unit is given. `em` = the font size in pixels of the character concerned.
- Lottie keys refer to the bodymovin/Lottie text layer (`ty: 5`) object `t` (`t.d`, `t.a`, `t.p`, `t.m`).

## Text layers

### AE-TXT-001 · Text layer and Text property group (文本图层)

- **What it does:** A synthetic vector layer whose content is laid-out type described by the Source Text value plus path, more-options and animator groups. It has no footage source and is always rasterized from vectors at the resolution it is displayed at.
- **Exact semantics:**
  - Layer property tree (timeline order, top to bottom): `Text` group, then `Transform`, plus the generic layer groups when present (Masks, Effects, Layer Styles, Geometry Options/Material Options when 3D; see project-comp-layers.md, Property tree).
  - `Text` group (`ADBE Text Properties`, 文本) children in order:

    | Child (中文) | matchName | Kind | Notes |
    |---|---|---|---|
    | Source Text (源文本) | `ADBE Text Document` | TextDocument property, hold-only keyframes | AE-TXT-010 and following |
    | Path Options (路径选项) | `ADBE Text Path Options` | group | AE-TXT-046 |
    | More Options (更多选项) | `ADBE Text More Options` | group | AE-TXT-048 |
    | Animator 1..N (动画制作工具 1..N) | `ADBE Text Animators` → `ADBE Text Animator` | indexed group, user-added | AE-TXT-052 |

  - The `Animate:` button on the Text group row and the Animation > Animate Text menu add animators (AE-TXT-052).
  - A text layer has a layer-level flag `threeDPerChar` (per-character 3D, AE-TXT-061) in addition to the normal 3D layer switch.
  - Text layers have no Continuously Rasterize switch in the Switches column: they are always continuously rasterized (AE-TXT-008).
- **Priority:** P0
- **Owner:** bae-model (+ bae-text)
- **Notes:** Scripting: `TextLayer` object (`app.project.item(i).layer(j)` instanceof `TextLayer`), `layer.sourceText` shortcut = `property("ADBE Text Properties").property("ADBE Text Document")`, `layer.text` returns the Text group. Layer matchName `ADBE Text Layer`. Lottie: layer `ty: 5`, text data in `t` (`t.d` document, `t.a` animators, `t.p` path, `t.m` more options). BAE `Layer` kind `Text` (ARCHITECTURE.md §4).

### AE-TXT-002 · New text layer creation (新建文本图层)

- **What it does:** Creates an empty text layer and enters text-editing mode, using the current Character and Paragraph panel settings as the initial style.
- **Exact semantics:**
  - Entry points:
    - Layer > New > Text (图层 > 新建 > 文本): new layer, insertion point at the comp centre (horizontal type tool).
    - Double-click the Horizontal Type Tool (横排文字工具) or Vertical Type Tool (直排文字工具): same, with that tool's orientation.
    - Type tool click in the Composition viewer where the pointer is not over an existing text layer (or Shift+click anywhere): point text at the click (AE-TXT-003).
    - Type tool drag: paragraph text (AE-TXT-004).
    - Scripting `layers.addText(sourceText)` (point text) and `layers.addBoxText([w, h], sourceText)` (paragraph text).
  - The new layer is placed above the selected layer (or at the top of the stack); its in point and duration follow the rules for new synthetic layers (see project-comp-layers.md, Layer timing defaults).
  - Initial TextDocument = current Character/Paragraph panel state (the "default text style"); `Transform.Position` = the click point in comp space (for Layer > New > Text: comp centre); `Transform.Anchor Point` = [0, 0, 0].
  - Layer name: while the user has not renamed it, the layer's displayed name is derived from the current Source Text string (the text at the current time, with line breaks shown as spaces, UNVERIFIED: truncated by the Timeline only, not in the stored data). A layer whose text is empty is shown as `<empty text layer>` (UNVERIFIED exact string). Once the user renames the layer, the name is fixed.
  - Leaving edit mode: Enter on the numeric keypad, Ctrl+Enter (Win) / Cmd+Return (Mac), Esc, or choosing another tool. UNVERIFIED: an empty text layer created by a type-tool click is kept (not auto-deleted) when edit mode ends.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** Tools and shortcuts (Ctrl/Cmd+T cycles type tools; Ctrl+Alt+Shift+T / Cmd+Opt+Shift+T = new text layer) are in ui-preview-render-io.md (Tools). Implementation hint: store `name` as `Option<String>` with `None` = "follow source text", so collaborative edits of the text do not fight with an explicit rename.

### AE-TXT-003 · Point text (点文本)

- **What it does:** Unbounded text anchored at a point: each line grows or shrinks as it is edited and only breaks at explicit returns.
- **Exact semantics:**
  - `TextDocument.pointText = true`, `boxText = false`.
  - Lines are separated only by hard returns (U+000D in `text`; see AE-TXT-011). No automatic wrapping, no overflow.
  - Origin: the layer-space point (0, 0) lies on the baseline of the first line. Horizontal placement of each line relative to x = 0 depends on its paragraph justification (AE-TXT-043):
    - Left / any "Justify last line Left": line starts at x = 0 (start indent added, AE-TXT-029).
    - Center / "Justify last line Center": line centre at x = 0.
    - Right / "Justify last line Right": line ends at x = 0.
    - "Justify All" (Full) on point text: behaves as Left (no box width to justify to). UNVERIFIED.
    - With paragraph direction RTL the meaning of Left/Right in start/end indents swaps (AE-TXT-033); alignment buttons remain visual left/right. UNVERIFIED.
  - Subsequent lines step down by the leading of each line (AE-TXT-041); line 0's baseline is at y = 0.
  - Vertical point text (AE-TXT-090): the origin lies on the centre axis of the first column; columns proceed right to left.
- **Priority:** P0
- **Owner:** bae-text (+ bae-model)
- **Notes:** Lottie: point text has no `t.d.k[].s.sz` / `ps`. Changing justification of point text moves the text relative to the fixed anchor; it does not move the layer. Scripting: `layers.addText()`.

### AE-TXT-004 · Paragraph (box) text (段落文本)

- **What it does:** Text composed inside a rectangular bounding box (定界框): lines wrap at the box width, multiple paragraphs carry paragraph formatting, and text that does not fit is overset (hidden) and flagged with the overflow icon.
- **Exact semantics:**
  - `boxText = true`, `pointText = false`; `boxTextSize = [w, h]` (pixels, each ≥ 1, scripting docs say integers; AE 2026 UI accepts fractional values via handles, UNVERIFIED) and `boxTextPos = [x, y]` = top-left corner of the box in layer space (relative to the layer origin / anchor at [0,0]).
  - Creation by type-tool drag: drag defines the box from a corner; Alt/Option-drag defines it around its centre. UNVERIFIED: AE then sets the layer origin (and Position) so that it sits at the box's top-left x and at the first baseline's y; e.g. scripting docs report `boxTextPos ≈ [0, -25]` for default settings, i.e. the origin is on the left edge, 25 px below the top.
  - Composition: lines are broken to fit `w - startIndent - endIndent` (first line also `- firstLineIndent`), see AE-TXT-040. The first baseline is placed below the box top by the first-baseline rule (AE-TXT-042). Lines are composed top-down while they fit: a line is composed only if it fits inside the box height (UNVERIFIED exact criterion: AE drops a line whose baseline + descent would exceed the box bottom). Remaining text is overset: not rendered, not selectable by animators (UNVERIFIED: overset characters do not count in `textTotal`), `boxOverflow = true` (AE 24.6+), and the box shows the overflow icon (溢出图标, a small square with "+") at its bottom-right handle.
  - Hard return (Enter/Return) starts a new paragraph; Shift+Enter inserts a soft return (U+0003 in the AE text string, UNVERIFIED; renders as a line break within the same paragraph, keeping paragraph formatting such as space before/after only at paragraph boundaries).
  - Resizing the box (type tool on a box handle): drag = one direction, Shift+drag = keep proportion, Ctrl/Cmd+drag = from the centre. Text reflows immediately; resizing the box is a Source Text change (keyframeable as part of the TextDocument, hold interpolation).
  - Box vertical alignment, inset, auto-fit and first baseline options (AE 24.6+): AE-TXT-031.
- **Priority:** P0
- **Owner:** bae-text (+ bae-model, bae-ui-motion)
- **Notes:** Lottie: `t.d.k[].s.sz` = box size, `t.d.k[].s.ps` = box position (top-left, layer space). lottie-web places the box by `ps` and starts the first line at `ps[1] + ascent`-ish (UNVERIFIED in detail). Scripting: `layers.addBoxText([w, h])`; `boxTextSize` throws when `boxText` is false. The bounding box drawn in the viewer is UI (ui-preview-render-io.md). BAE decision: hide overset text exactly like AE; show a BAE warning badge in the timeline as well.

### AE-TXT-005 · Convert to Point Text / Convert to Paragraph Text (转换为点文本 / 转换为段落文本)

- **What it does:** Converts the selected text layer between point and paragraph text.
- **Exact semantics:**
  - Invoked with a type tool active and the layer selected (not in text-editing mode): viewer context menu > Convert To Paragraph Text / Convert To Point Text. BAE also exposes them as commands.
  - Paragraph → Point: every composed line that ended by automatic wrapping gets a hard return inserted at its end (all lines except the last); all overset characters (outside the box) are deleted; `boxText` becomes false. Glyph positions on screen stay the same (UNVERIFIED for non-left justification: AE keeps visual positions by re-placing the origin).
  - Point → Paragraph: a box is created that tightly encloses the current text (UNVERIFIED: width = widest line advance, height = line count × leading plus first-baseline offset), existing hard returns stay as paragraph breaks; `boxText` becomes true.
  - Both conversions are single undoable commands and replace the Source Text value at the current time; if Source Text has keyframes, UNVERIFIED: AE converts every keyframe value.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-text)
- **Notes:** Help page "Creating and editing text layers" (创建和编辑文本图层). Scripting has no direct method; it can be emulated with `TextDocument` writes (`boxTextSize` / `pointText` are read-only except via `addBoxText`).

### AE-TXT-006 · Text layer origin, anchor and transform defaults (文本图层原点与锚点)

- **What it does:** Defines where layer space (0,0) is for text and which point Transform rotates/scales around.
- **Exact semantics:**
  - Layer-space origin = the start of the first line's baseline as defined in AE-TXT-003 (point text) / AE-TXT-004 (paragraph text). Text layers have no "source size": layer space is unbounded and the origin is not the bounding-box corner.
  - Default `Anchor Point` = [0, 0] (or [0,0,0] in 3D); Transform rotation/scale therefore pivot on the first baseline origin until the user moves the anchor (e.g. with Pan Behind tool or Ctrl/Cmd+Alt/Opt+Home "center anchor point in layer content", which uses `sourceRectAtTime()`, see AE-TXT-007).
  - Per-character transforms (animators) are relative to each character's own anchor (AE-TXT-054), not the layer anchor.
- **Priority:** P0
- **Owner:** bae-text (+ bae-anim)
- **Notes:** Layer transform math is in transform-keyframes-expressions.md (Transform matrix order). Lottie: text layers export `ks.a` normally; the text coordinates in lottie are relative to the same baseline origin.

### AE-TXT-007 · Text layer bounds and sourceRectAtTime (文本图层边界)

- **What it does:** Reports the rectangle occupied by the text in layer space, used by the viewer's layer bounding box, "center anchor point", Align panel, expressions (`sourceRectAtTime`) and auto-sizing rigs.
- **Exact semantics:**
  - `sourceRectAtTime(t = time, includeExtents = false)` returns `{top, left, width, height}` in layer space (y down).
  - Text layer, `includeExtents = false`: the union of the rendered glyph bounds of all composed (non-overset) characters at time `t`, after animators, path options and per-character transforms are applied (UNVERIFIED: AE uses glyph outline bounds rather than advance boxes; the stroke width is included when stroke is applied). Empty text → `{0, 0, 0, 0}` (UNVERIFIED).
  - Paragraph text, `includeExtents = true` (AE 15.1+): returns the paragraph box: `{top: boxTextPos[1], left: boxTextPos[0], width: boxTextSize[0], height: boxTextSize[1]}`.
  - Point text, `includeExtents = true`: same as `false`.
  - Effects, masks and layer styles are not included (bounds of the layer source only).
- **Priority:** P0 — every auto-sizing lower-third rig depends on it.
- **Owner:** bae-text (+ bae-anim)
- **Notes:** Expression API itself: transform-keyframes-expressions.md (sourceRectAtTime). Scripting equivalent `Layer.sourceRectAtTime(timeT, extents)` (AE 13.2+). Compositing-level layer bounds: compositing-3d-color.md (Layer bounds). Implementation hint: cache per (layer, time) the composed glyph run bounds; computing it must not require GPU rasterization.

### AE-TXT-008 · Continuous rasterization and quality of text (文本连续栅格化)

- **What it does:** Text is re-rasterized from outlines at the final on-screen/render resolution after layer transforms, so scaling a text layer never pixelates.
- **Exact semantics:**
  - Text outlines are transformed by the full layer (and per-character) matrix and rasterized at the comp sampling resolution, like a shape layer with Continuously Rasterize on. Consequence (same as continuously rasterized layers): effects are applied after the layer transform; see compositing-3d-color.md (Collapse transformations / continuous rasterization).
  - Layer Quality switch: Best = anti-aliased; Draft = UNVERIFIED (AE draws text anti-aliased in Draft too; Draft affects effects only). Wireframe = outline only.
  - Font size is in layer pixels: at 100% layer scale 1 px of font size = 1 comp px; at 200% scale the text is rendered at twice the size with full detail.
- **Priority:** P0
- **Owner:** bae-render (+ bae-text)
- **Notes:** Implementation hint: render glyph outlines through Vello (GPU path rasterization) with the per-character matrix; cache shaped outlines per (font, glyph id, variation coords), never cache rasterized bitmaps across scale changes.

### AE-TXT-009 · Editing text in the viewer (编辑文本)

- **What it does:** Interactive text entry and selection with the type tools inside the Composition viewer, including paste variants.
- **Exact semantics:**
  - Insertion point: type tool click over an existing text layer places the caret (pointer shows the edit-text cursor); Shift+click always creates a new layer instead.
  - Selection: drag; click then Shift+click; double-click = word, triple-click = line, quadruple-click = paragraph, quintuple-click = all; Shift+←/→ extends by character, Shift+Ctrl/Cmd+←/→ by word. Double-clicking the text layer in the Timeline selects all text and activates the last type tool.
  - Formatting applies to the selection; with no text selected but layers selected, Character/Paragraph panel changes apply to whole selected layers and their selected Source Text keyframes; with nothing selected they change the default style for new text.
  - Edit > Paste inserts plain text that takes the formatting of the character at the insertion point (help text: "of the first character in the layer") — UNVERIFIED which one; when copying between AE text layers the source formatting is preserved (AE 25.x behaviour).
  - Edit > Paste Text and Match Formatting (粘贴文本并匹配格式): pastes the characters with the destination formatting. Edit > Paste Text Formatting Only (仅粘贴文本格式): applies the copied formatting to the selected characters (or the whole layer when the layer, not text, is selected). Version: both added in AE 25.x (UNVERIFIED exact release, help page dated 2026).
  - Moving the layer while editing: Ctrl/Cmd held shows the move pointer; dragging outside the text with the type tool moves the layer.
  - Each committed edit session is one undo step (UNVERIFIED: AE records one undo per edit session, not per keystroke).
  - Path Options can be temporarily disabled with its eye switch to make editing easier (AE-TXT-046).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-text, bae-motion)
- **Notes:** Must work with IME (Chinese Pinyin/Wubi, Japanese) — compose in-line, commit on IME confirm; see AE-TXT-087. Keyboard shortcuts belong to ui-preview-render-io.md. Implementation hint: parley's `PlainEditor`-style cursor/selection model over the TextDocument run structure; every commit becomes one `bae-model` Command (`SetSourceText`).

## TextDocument (Source Text value)

### AE-TXT-010 · TextDocument value model (源文本值)

- **What it does:** The single value type of the Source Text property: the string plus all character formatting (per character, as style runs), all paragraph formatting (per paragraph), the box geometry and a few layer-wide options.
- **Exact semantics:**
  - Logical structure BAE must persist (one value per Source Text keyframe, or one static value):

    | Part | Content | Granularity |
    |---|---|---|
    | `text` | Unicode string (AE-TXT-011) | — |
    | character style runs | font, size, fill/stroke, tracking, kerning, leading, baseline shift, scales, faux styles, caps, baseline option, tsume, ligature, no break, digit set, baseline direction, stroke-over-fill, line join | run of characters (AE-TXT-013..AE-TXT-027) |
    | paragraph style runs | justification, indents, space before/after, composer, hyphenation, direction, hanging punctuation, leading type | paragraph (text between hard returns) (AE-TXT-028..AE-TXT-034) |
    | layer-wide | point/box flag, box size/position, box options (AE 24.6+), line orientation (horizontal/vertical), All-Fills/All-Strokes order override | whole document |

  - Scripting reads of a character attribute on the whole document return the value of the **first character** (`allCaps`, `baselineShift`, `font`, ...); writes apply to **all** characters. Paragraph attributes read `undefined` when mixed and writes apply to all paragraphs. Range accessors (AE-TXT-036) read/write sub-ranges.
  - Equality: two TextDocuments are equal iff text and all runs are equal; equal consecutive keyframe values still create separate hold keys.
  - The value is not interpolable: Source Text uses hold interpolation only (AE-TXT-038).
  - Empty text is allowed (`text = ""`); the run tables then hold one "insertion" style used for the next typed character.
- **Priority:** P0
- **Owner:** bae-model (+ bae-text)
- **Notes:** Scripting: `TextDocument` object, `new TextDocument(string)`, `property("Source Text").value` / `setValue()` / `setValueAtTime()`. Expression access: `text.sourceText` and the text style API (transform-keyframes-expressions.md, Text style API). Lottie: `t.d.k[]` keyframes each with `s` = one flattened document (single style only: Lottie cannot express per-character runs; bodymovin exports the first character's style, UNVERIFIED). Implementation hint: store runs as a sorted list of (start, end, style-id) over a CRDT text (Loro `LoroText` with marks) so that concurrent edits keep style ranges consistent.

### AE-TXT-011 · Text string, paragraphs and line breaks (文本字符串与换行)

- **What it does:** Defines the characters stored in `TextDocument.text` and how paragraph and line breaks are encoded.
- **Exact semantics:**
  - Paragraph break (hard return, Enter/Return): stored as U+000D CR. Reading `text` returns `\r` between paragraphs. When scripting/expressions set a string containing `\n` or `\r\n`, AE converts each to a single `\r` (UNVERIFIED for `\r\n`, which AE 2026 treats as one break).
  - Soft return / forced line break (Shift+Enter): stored as U+0003 ETX (UNVERIFIED: AE UI writes U+0003; lottie-web also treats char code 3 as a new line). It breaks the line without starting a new paragraph.
  - Tab U+0009: advances to the next default tab stop. UNVERIFIED: AE uses fixed tab stops (no tab-stop UI); BAE decision (until verified): tab advances to the next multiple of 4 × the space advance.
  - Expressions returning a number or other non-string for Source Text: converted to string (`String(value)`), see transform-keyframes-expressions.md.
  - Maximum length: UNVERIFIED (AE displays a warning for very long text; no documented limit).
- **Priority:** P0
- **Owner:** bae-model (+ bae-text)
- **Notes:** Lottie `t.d.k[].s.t`: newlines are encoded with `\r` (Lottie docs); lottie-web treats char codes 13 and 3 as line breaks. Paragraph count `TextDocument.paragraphCount` ≥ 1 even for empty text.

### AE-TXT-012 · Character indexing (字符索引)

- **What it does:** Defines what one "character" is for selectors, animator indices, `textIndex`/`textTotal`, scripting ranges and Based On counting.
- **Exact semantics:**
  - Scripting range APIs (`characterRange(start, end)`, `paragraphCharacterIndexesAt`) index the string by UTF-16 code units (JavaScript string indices). UNVERIFIED.
  - Text animators operate on "characters" = displayed text elements: UNVERIFIED but required for correct emoji/CJK behaviour: AE (Universal Text Engine, AE 22.x+) groups a base character with combining marks and surrogate pairs into one animated element; lottie-web reproduces this by merging combining marks (U+0300..U+036F etc.), variation selectors, ZWJ sequences, emoji modifiers, regional-indicator flag pairs into the preceding element.
  - Line-break characters (CR, ETX) count as characters for "Characters" based selectors (they are present in the per-character arrays but have no glyph); see AE-TXT-071 for each Based On mode.
- **Priority:** P0
- **Owner:** bae-text
- **Notes:** BAE decision: animate per grapheme cluster (Unicode extended grapheme clusters, UAX #29) and expose UTF-16 offsets only at the scripting boundary. Complex-script (Arabic, Devanagari) per-character animation follows the shaped cluster, see AE-TXT-092.

### AE-TXT-013 · Font selection fields (字体)

- **What it does:** Identifies the font face used by each character run.
- **Exact semantics:**

  | Field (中文) | Type | Default | Notes |
  |---|---|---|---|
  | `font` (字体) | string, PostScript name | default font (AE-TXT-035) | read/write; on write an unknown name creates a substitute font instance (missing font) rather than failing |
  | `fontFamily` (字体系列) | string | — | read-only (AE 13.1+), family name of the first character's font |
  | `fontStyle` (字体样式) | string | — | read-only (AE 13.1+), e.g. "Bold", "Italic" |
  | `fontLocation` | string, file path | — | read-only (AE 13.1+); may be empty for some font kinds |
  | `fontObject` | Font object | — | read/write (AE 24.0+): exact font instance, needed when several fonts share a PostScript name |

  - Persisted identity of a run's font: PostScript name + family + style (+ variable-font instance coordinates when applicable, see AE-TXT-084); matching rules in AE-TXT-082.
  - Character panel: Font family menu + font style menu chosen independently; if several copies of a font are installed, AE shows a format suffix `(T1)`, `(TT)`, `(OT)`. A missing font appears in brackets, e.g. `[Helvetica Neue]`.
- **Priority:** P0
- **Owner:** bae-text (+ bae-model)
- **Notes:** Scripting `TextDocument.font` with duplicate PostScript names returns the 0th element of `app.fonts.getFontsByPostScriptName()`. Lottie: `t.d.k[].s.f` = font name referencing `fonts.list[].fName`; the font list carries `fFamily`, `fStyle`, `fPath`, `origin`, `ascent`. AE also loads fonts from the Adobe-private folder (Win `Program Files\Common Files\Adobe\Fonts`, Mac `/Library/Application Support/Adobe/Fonts`).

### AE-TXT-014 · Font size (字体大小)

- **What it does:** Sets the em size of the run in layer pixels.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Font Size (字体大小) | float | UNVERIFIED: 36 px for a fresh preference set; otherwise the last used Character panel value | 0.1 – 1296 | px (layer pixels at 100% layer scale) |

  - 1 em = font size px. Glyph outlines scale by `fontSize / unitsPerEm`.
  - Values outside the range are clamped on entry (scripting: values must be within 0.1..1296 inclusive, UNVERIFIED whether out-of-range throws or clamps).
- **Priority:** P0
- **Owner:** bae-text (+ bae-model)
- **Notes:** Scripting `fontSize`. Lottie `t.d.k[].s.s`. The Scale animator property scales glyphs after layout and does not change the font size (no reflow); Source Text font size changes reflow the text.

### AE-TXT-015 · Fill (填充)

- **What it does:** Paints the inside of each glyph outline with a solid colour.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Notes |
  |---|---|---|---|---|
  | `applyFill` (填充) | bool | true | — | false = No Fill (无填充颜色) |
  | `fillColor` (填充颜色) | [r, g, b] float | UNVERIFIED: [1,1,1] white for fresh preferences | 0..1 in 8/16 bpc; may exceed 1 (over-range) in 32 bpc projects | read throws if `applyFill` is false; writing it sets `applyFill = true` for the affected characters |

  - Fill uses the non-zero winding rule on glyph outlines (TrueType/CFF outlines are designed for non-zero), fully opaque; per-character fill opacity comes only from animators (Fill Opacity, AE-TXT-062).
  - Colour values are in the project working space, not linearized unless the project linearizes (see compositing-3d-color.md, Color management).
- **Priority:** P0
- **Owner:** bae-text (+ bae-render)
- **Notes:** Character panel also has: Swap Fill and Stroke, Set to Black / Set to White, eyedropper (UI: ui-preview-render-io.md, Character panel). Lottie `t.d.k[].s.fc` = [r,g,b] 0..1; absence of `fc` means no fill.

### AE-TXT-016 · Stroke (描边)

- **What it does:** Paints a centred stroke along each glyph outline.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | `applyStroke` (描边) | bool | false (UNVERIFIED) | — | — | false = No Stroke |
  | `strokeColor` (描边颜色) | [r,g,b] float | UNVERIFIED: [0,0,0] | as fill | — | read throws if `applyStroke` false; writing sets `applyStroke = true` |
  | `strokeWidth` (描边宽度) | float | UNVERIFIED: 1 px | 0 – 1000 (0 is stored as 0.01) | px | |
  | `lineJoinType` (线段连接) | enum | Miter (UNVERIFIED) | Miter `LINE_JOIN_MITER` / Round `LINE_JOIN_ROUND` / Bevel `LINE_JOIN_BEVEL` | — | AE 24.0+ scripting; set via Character panel menu > Line Join |

  - The stroke is centred on the outline: half the width outside, half inside. Caps: glyph outlines are closed so caps do not apply. Miter limit: UNVERIFIED (BAE decision: 4, the common Adobe default).
  - Stroke width scales with layer scale and per-character Scale (it is part of the outline geometry, not a screen-space width).
  - Overlapping contours inside one glyph (e.g. variable-font outlines with overlaps) are stroked as separate contours (UNVERIFIED; BAE decision: remove overlaps before stroking for variable fonts, matching what users expect).
- **Priority:** P0
- **Owner:** bae-text (+ bae-render, bae-vector)
- **Notes:** Lottie `t.d.k[].s.sc` (colour), `sw` (width), `of` (stroke over fill). Implementation hint: produce stroke outlines with kurbo stroking in `bae-vector` and rasterize them with Vello; per-character Stroke Width animator overrides width per glyph (AE-TXT-063).

### AE-TXT-017 · Fill and stroke paint order (填充和描边顺序)

- **What it does:** Decides whether each character's stroke is drawn over or under its fill, and whether ordering is per character or across the whole layer.
- **Exact semantics:**
  - Per-character attribute `strokeOverFill` (bool). Character panel menu options in order:

    | Option (中文) | Effect |
    |---|---|
    | Fill Over Stroke (在描边上填充) | `strokeOverFill = false` for selected characters (default) |
    | Stroke Over Fill (在填充上描边) | `strokeOverFill = true` for selected characters |
    | All Fills Over All Strokes (全部填充在全部描边之上) | layer-wide override: draw every stroke of the layer first, then every fill |
    | All Strokes Over All Fills (全部描边在全部填充之上) | layer-wide override: draw every fill first, then every stroke |

  - The layer-wide choice is stored in More Options > Fill & Stroke (AE-TXT-049) with values Per Character Palette / All Fills Over All Strokes / All Strokes Over All Fills; the Character panel options write that property.
  - Per Character Palette rendering order: characters are drawn in text order; for each character its fill and stroke are drawn in its own order, so a later character's stroke can cover an earlier character's fill.
  - With per-character 3D enabled, the layer-wide options do nothing and Fill & Stroke is unavailable (each character is its own 3D plane).
  - `strokeOverFill` read on the document returns the first character's attribute, which may differ from what renders when the layer-wide override is set.
- **Priority:** P0
- **Owner:** bae-text (+ bae-render)
- **Notes:** Lottie: `t.d.k[].s.of` (true = stroke over fill). lottie-web only supports per-document `of`. Inter-character blending interacts with this order (AE-TXT-050).

### AE-TXT-018 · Tracking (字符间距)

- **What it does:** Adds uniform space after each character in a run.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Tracking (字符间距) | integer in UI, float in scripting | 0 | UNVERIFIED: -1000 – 10000 | 1/1000 em |

  ```
  extra_advance_after(char i) = tracking_i / 1000 * fontSize_i
  ```
  - Applied after each character's advance (and after kerning), including spaces. Tracking and manual kerning are cumulative.
  - Line-width for alignment: UNVERIFIED whether the tracking after the last character on a line counts; lottie-web excludes it (`lineWidth` starts at `-trackingOffset`). BAE decision until verified: exclude trailing tracking from the measured line width (matches lottie-web and visual centring in AE).
  - The Tracking animator (AE-TXT-064) adds further tracking per character without reflowing paragraph text.
- **Priority:** P0
- **Owner:** bae-text
- **Notes:** Scripting `tracking`. Lottie `t.d.k[].s.tr` (lottie-web: `trackingOffset = tr / 1000 * fontSize`). For CJK text, tracking adjusts aki between all characters.

### AE-TXT-019 · Kerning (字偶间距)

- **What it does:** Adjusts the space between specific character pairs, automatically from font data or manually.
- **Exact semantics:**
  - `autoKernType` (AE 24.0+ scripting), Character panel "Set the kerning between two characters" (设置两个字符之间的字偶间距, 译名待核) menu values in order:

    | Value (中文) | Scripting enum | Meaning |
    |---|---|---|
    | Metrics (度量标准) | `AutoKernType.METRIC_KERN` | default; use the font's kerning (OpenType GPOS `kern` feature, or legacy `kern` table) |
    | Optical (视觉) | `AutoKernType.OPTICAL_KERN` | Adobe optical kerning computed from glyph shapes |
    | 0 / manual value | `AutoKernType.NO_AUTO_KERN` + `kerning` | manual pair kern |

  - Manual `kerning`: integer, 1/1000 em, applied between the character before the caret and the one after it (stored on the character before, UNVERIFIED); positive = apart. Setting `kerning` switches the affected characters to `NO_AUTO_KERN`; reading returns 0 when Metrics/Optical. With a range selected, manual kerning is unavailable (use tracking).
  - Formula: `advance_i += kern_pair(i, i+1)` (font units scaled) for Metrics; `+= kerning_i/1000 * fontSize_i` for manual.
- **Priority:** P0 (Metrics, manual); P2 (Optical)
- **Owner:** bae-text
- **Notes:** Implementation hint: Metrics = enable OpenType `kern` feature in the shaper (parley/harfrust); manual = per-cluster advance adjustment after shaping. Optical kerning is proprietary; BAE decision: approximate by side-bearing based spacing and mark as an approximation. Lottie has no kerning key (glyph positions come from the renderer's own shaping).

### AE-TXT-020 · Leading and auto leading (行距)

- **What it does:** Sets the distance between consecutive baselines.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Leading (行距) | float or Auto | Auto (自动) | 0.01 – UNVERIFIED max (scripting: 0 is clipped to 0.01) | px |
  | Auto leading percentage (自动行距百分比, 译名待核) | float | 120% | UNVERIFIED: 0 – 500% | % of font size |

  ```
  autoLeading(line) = autoLeadingPct/100 * max(fontSize of characters on that line)  // UNVERIFIED: AE uses the largest font size on the line
  leading(line)     = autoLeading ? autoLeading(line) : max(leading attribute over characters on the line)
  ```
  - Auto leading percentage: UNVERIFIED whether AE exposes any UI for it (Photoshop sets it in the Paragraph panel menu > Justification dialog; the AE Paragraph panel help documents no such dialog). BAE: store it per paragraph, default 120%, no UI in M1.
  - Leading is a character attribute in AE (as in Photoshop), the largest value on a line wins; Roman leading measures from the previous baseline to this baseline ("bottom-to-bottom", default). Japanese leading type measures top-to-top (AE-TXT-034).
  - Scripting: `autoLeading` (bool, paragraph-level read), `leading` returns 0 when auto; setting `leading` turns auto off (the scripting guide's text says it sets autoLeading to true; that is a documentation error, UNVERIFIED).
  - Line Spacing animator (AE-TXT-065) offsets lines further without reflow.
- **Priority:** P0
- **Owner:** bae-text
- **Notes:** Lottie `t.d.k[].s.lh` = explicit line height in px; lottie-web falls back to `1.2 * fontSize` when 0. Layout use: AE-TXT-041.

### AE-TXT-021 · Baseline shift (基线偏移)

- **What it does:** Raises or lowers characters relative to the baseline without changing line metrics.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Baseline Shift (基线偏移) | float | 0 | UNVERIFIED: -1296 – 1296 | px |

  - Positive moves horizontal text up (layer y decreases) and vertical text to the right; negative down/left. The shift is applied to glyph position only (does not change leading or line breaks), and is applied before per-character animator transforms (it moves the character's baseline origin, UNVERIFIED whether anchor moves with it; BAE: it does).
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** Scripting `baselineShift` (AE 13.2+). Lottie `t.d.k[].s.ls`.

### AE-TXT-022 · Horizontal and vertical scale (水平缩放 / 垂直缩放)

- **What it does:** Stretches glyphs horizontally or vertically as part of the character style, affecting layout.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Vertical Scale (垂直缩放) | float | 100 | UNVERIFIED: 1 – 10000 | % |
  | Horizontal Scale (水平缩放) | float | 100 | UNVERIFIED: 1 – 10000 | % |

  - Horizontal scale multiplies the glyph outline x and the advance width (so it reflows); vertical scale multiplies glyph y around the baseline (y = 0) and does not change leading (UNVERIFIED for auto leading).
  - Scale origin: the glyph's baseline origin (left end of the baseline for horizontal text).
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** Scripting `horizontalScale`, `verticalScale` (AE 13.2+; the scripting guide describes them as "in pixels"; the stored value is a ratio, UNVERIFIED whether 1.0 or 100 means 100%). Not exported to Lottie.

### AE-TXT-023 · Faux Bold and Faux Italic (仿粗体 / 仿斜体)

- **What it does:** Simulates bold or italic for fonts without such styles.
- **Exact semantics:**
  - `fauxBold` (bool, default false): embolden the glyph outline (fill grows outward); advance widths grow so text reflows. Amount: UNVERIFIED (Adobe text engine ≈ fontSize/30 total stroke-out). BAE decision until measured: outline offset = 0.02 em per side and advance += 0.04 em; both must be measured against AE before freezing.
  - `fauxItalic` (bool, default false): shear glyph outline around its baseline origin: `x' = x + tan(θ)·(-y_up)` with θ UNVERIFIED ≈ 12–15°; advances unchanged.
  - Both are character attributes; write support in scripting from AE 24.0 (read since 13.2).
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** Lottie has no keys (bodymovin bakes nothing; renderers ignore them). Implementation hint: implement both as outline transforms before stroking.

### AE-TXT-024 · Caps: All Caps and Small Caps (全部大写字母 / 小型大写字母)

- **What it does:** Displays characters as capitals without changing the stored text.
- **Exact semantics:**
  - `fontCapsOption` (AE 24.0+): `FONT_NORMAL_CAPS` (default) / `FONT_SMALL_CAPS` / `FONT_ALL_CAPS` / `FONT_ALL_SMALL_CAPS`. Read-only booleans `allCaps`, `smallCaps` (AE 13.2+) reflect the first character.
  - All Caps: map each character through Unicode uppercase mapping for display (language-sensitive, e.g. Turkish dotted i UNVERIFIED); the stored `text` keeps the typed case.
  - Small Caps: lowercase letters are rendered with the font's small-cap glyphs (OpenType `smcp`) if available; otherwise faux small caps = uppercase glyph scaled to UNVERIFIED 70% (Adobe default "Small Caps size 70%"). Characters typed as uppercase are not changed. All Small Caps (`c2sc` + `smcp`) converts both.
  - Selectors count the displayed characters (case mapping that changes length, e.g. "ß" → "SS", UNVERIFIED: AE keeps one animated element).
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** Lottie `t.d.k[].s.ca`: 0 Regular, 1 All Caps, 2 Small Caps.

### AE-TXT-025 · Superscript and subscript (上标 / 下标)

- **What it does:** Reduces characters in size and shifts them above or below the baseline.
- **Exact semantics:**
  - `fontBaselineOption` (AE 24.0+): `FONT_NORMAL_BASELINE` (default) / `FONT_FAUXED_SUPERSCRIPT` / `FONT_FAUXED_SUBSCRIPT`; read-only `superscript`, `subscript` (AE 13.2+).
  - If the font has `sups`/`subs` glyphs: UNVERIFIED whether AE uses them (help text says it uses font glyphs when present, else faux). Faux values: UNVERIFIED, Adobe text-engine defaults: size 58.3%, superscript shift +33.3% of font size, subscript shift -33.3%.
  - Leading is computed from the unreduced font size (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** Not exported to Lottie.

### AE-TXT-026 · Tsume (比例间距)

- **What it does:** Reduces the space on both sides of a character (typically CJK) by a percentage without scaling the glyph.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Tsume (比例间距) | float | 0 | 0 – 100 (UI); scripting normalized 0.0 – 1.0 | % |

  ```
  // per character, horizontal text
  lsb = glyph left side bearing (em-box left to ink left), rsb = right side bearing
  shrink = tsume * (lsb + rsb)   // equal percentage taken from each side
  glyph drawn shifted left by tsume*lsb; advance -= shrink
  ```
  - At 100% no space remains between the character's bounding box and its em box.
  - Scripting quirk: `tsume` accepts 0..100 but expects normalized 0..1; values > 1 produce unexpected results (panel clamps at 100%).
- **Priority:** P2 — CJK typographic refinement; Chinese users use Tracking more often.
- **Owner:** bae-text
- **Notes:** Scripting `tsume` (AE 13.2+). Shows in the Character panel only when Show East Asian options are visible (UNVERIFIED, AE 2026 always shows it).

### AE-TXT-027 · Ligatures, digit set and no-break (连字 / 数字 / 不换行)

- **What it does:** Character options that select OpenType features or prevent line breaks.
- **Exact semantics:**

  | Option (中文) | Scripting | Type | Default | Meaning |
  |---|---|---|---|---|
  | Ligatures (连字) | `ligature` (AE 24.0+) | bool | UNVERIFIED: true for new text in AE 2026 (Universal Text Engine) | enable OpenType standard ligatures (`liga`, `clig`); off disables them. Required ligatures (`rlig`, Arabic joining) are always on. |
  | Digits / Hindi Digits (印地语数字) | `digitSet` (AE 24.0+) | enum | `DEFAULT_DIGITS` | `DEFAULT_DIGITS` / `ARABIC_DIGITS` / `HINDI_DIGITS` / `FARSI_DIGITS` / `ARABIC_DIGITS_RTL`: substitutes ASCII digits 0-9 with the chosen script's digits at display time |
  | No Break (不换行, 译名待核) | `noBreak` (AE 24.0+) | bool | false | the characters are kept on one line by the line breaker, and animate together as one word for Based On Words (help text) |

  - Ligated glyphs: one glyph covers several characters; how AE animates a ligature per character is UNVERIFIED (BAE decision: when any animator with a selector exists on the layer, disable discretionary/standard ligatures that would merge characters with different selector values; required ligatures stay).
- **Priority:** P1 (ligatures); P2 (digit set, no break)
- **Owner:** bae-text
- **Notes:** Character panel shows Ligatures and Hindi Digits checkboxes at its bottom (UTE, AE 22.x+). No Lottie keys.

### AE-TXT-028 · Paragraph alignment and justification (段落对齐方式)

- **What it does:** Aligns each line of a paragraph to the left, centre or right, or justifies it to both edges of the box.
- **Exact semantics:**
  - `justification` enum, Paragraph panel buttons in order:

    | Button (中文) | Scripting `ParagraphJustification.*` | Lottie `j` |
    |---|---|---|
    | Left Align Text (左对齐文本) | `LEFT_JUSTIFY` (default) | 0 |
    | Center Text (居中对齐文本) | `CENTER_JUSTIFY` | 2 |
    | Right Align Text (右对齐文本) | `RIGHT_JUSTIFY` | 1 |
    | Justify Last Line Left (最后一行左对齐) | `FULL_JUSTIFY_LASTLINE_LEFT` | 3 |
    | Justify Last Line Center (最后一行居中对齐) | `FULL_JUSTIFY_LASTLINE_CENTER` | 5 |
    | Justify Last Line Right (最后一行右对齐) | `FULL_JUSTIFY_LASTLINE_RIGHT` | 4 |
    | Justify All (全部两端对齐, 译名待核) | `FULL_JUSTIFY_LASTLINE_FULL` | 6 |

  - Mixed paragraphs read as `MULTIPLE_JUSTIFICATIONS`; writing that value results in `CENTER_JUSTIFY`.
  - Justify modes apply only to paragraph text; on point text they behave like the corresponding last-line alignment (UNVERIFIED).
  - Vertical text: Left/Center/Right become Top/Center/Bottom.
  - Layout algorithm: AE-TXT-043.
- **Priority:** P0 (left/centre/right); P1 (justify modes)
- **Owner:** bae-text (+ bae-model)
- **Notes:** Scripting `justification`. Lottie `t.d.k[].s.j`.

### AE-TXT-029 · Paragraph indents (缩进)

- **What it does:** Insets lines of a paragraph from the box or line edges.
- **Exact semantics:**

  | Parameter (中文) | Scripting | Type | Default | Range | Units |
  |---|---|---|---|---|---|
  | Indent Left Margin (缩进左边距) | `startIndent` | float | 0 | UNVERIFIED: any float | px |
  | Indent Right Margin (缩进右边距) | `endIndent` | float | 0 | same | px |
  | Indent First Line (首行缩进) | `firstLineIndent` | float | 0 | same; negative = hanging indent | px |

  - For horizontal LTR text: available line width = `boxWidth - startIndent - endIndent`; the first line of each paragraph additionally starts at `startIndent + firstLineIndent`. For RTL paragraphs "start" is the right side (scripting names start/end follow paragraph direction; UI names Left/Right, UNVERIFIED mapping in AE's UI for RTL). For vertical text: left → top, right → bottom.
  - Point text: indents shift lines (left-aligned text starts at `startIndent`), but there is no wrapping.
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** Scripting AE 24.0+. Not in Lottie.

### AE-TXT-030 · Space before and after paragraphs (段前距 / 段后距)

- **What it does:** Adds vertical space between paragraphs.
- **Exact semantics:**

  | Parameter (中文) | Scripting | Type | Default | Units |
  |---|---|---|---|---|
  | Space Before Paragraph (段前距) | `spaceBefore` | float | 0 | px |
  | Space After Paragraph (段后距) | `spaceAfter` | float | 0 | px |

  ```
  baseline(first line of paragraph p) = baseline(last line of p-1) + spaceAfter(p-1) + spaceBefore(p) + leading(first line of p)
  ```
  - Space before is not applied to the first paragraph of a box (UNVERIFIED; Adobe convention: suppressed at the top of a frame). Soft returns do not trigger paragraph spacing.
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** Scripting AE 24.0+.

### AE-TXT-031 · Paragraph box options: vertical alignment, inset, auto-fit, first baseline (段落文本框选项)

- **What it does:** AE 24.6 (2024) box-text options that align the text block vertically in the box, pad the box, grow it to fit and choose where the first baseline sits.
- **Exact semantics:**

  | Field (中文) | Type | Default | Values / range |
  |---|---|---|---|
  | `boxVerticalAlignment` (垂直对齐, 译名待核) | enum | `TOP` | `TOP` / `CENTER` / `BOTTOM` / `JUSTIFY` (distribute lines to fill height) |
  | `boxInsetSpacing` (内边距, 译名待核) | float px | 0 | ≥ 0; same inset on all four sides; composable area = box shrunk by inset |
  | `boxAutoFitPolicy` (自动调整大小, 译名待核) | enum | `NONE` | `NONE` / `HEIGHT_CURSOR` / `HEIGHT_PRECISE_BOUNDS` / `HEIGHT_BASELINE`; box height grows downward only; disabled unless vertical alignment is `TOP` |
  | `boxFirstBaselineAlignment` (首行基线, 译名待核) | enum | `ASCENT` | `ASCENT` / `CAP_HEIGHT` / `EM_BOX` / `LEADING` / `LEGACY_METRIC` / `MINIMUM_VALUE_ASIAN` / `MINIMUM_VALUE_ROMAN` / `TYPO_ASCENT` / `X_HEIGHT` |
  | `boxFirstBaselineAlignmentMinimum` | float px | 0 | ≠ 0 overrides `boxFirstBaselineAlignment`: first baseline = max(rule, minimum) (UNVERIFIED: InDesign "Min" semantics) |
  | `boxOverflow` | bool, read-only | — | true if some text did not fit |

  - Vertical alignment: after composing lines top-down, the block (first line top to last line bottom, UNVERIFIED which metrics) is offset by 0 / (free/2) / free for Top/Center/Bottom; Justify distributes the free height equally between lines (UNVERIFIED, also between paragraphs).
  - Auto-fit: `HEIGHT_CURSOR` = height to the bottom of the last line's caret box; `HEIGHT_PRECISE_BOUNDS` = to the glyph ink bottom; `HEIGHT_BASELINE` = to the last baseline (UNVERIFIED interpretations from enum names). Auto-fit changes `boxTextSize[1]` as text is edited.
  - Projects saved by earlier versions use `TOP`, inset 0, `NONE`, and `LEGACY_METRIC` first baseline (UNVERIFIED: the legacy layout must stay identical).
- **Priority:** P1
- **Owner:** bae-text (+ bae-model)
- **Notes:** Paragraph panel gained these controls in AE 24.6 (UNVERIFIED exact UI labels). Not in Lottie (bodymovin ignores them; exported box text keeps `sz`/`ps`).

### AE-TXT-032 · Composer and hyphenation (书写器与连字符)

- **What it does:** Selects the line-breaking algorithm and automatic hyphenation for paragraph text.
- **Exact semantics:**

  | Option (中文) | Scripting | Type | Default |
  |---|---|---|---|
  | Adobe Single-line Composer (Adobe 单行书写器) / Adobe Every-line Composer (Adobe 逐行书写器) | `everyLineComposer` (false = single-line) | bool | UNVERIFIED: AE's factory default not confirmed (Photoshop defaults to Every-line; earlier draft said Single-line) |
  | Auto hyphenate (连字符连接, 译名待核) | `autoHyphenate` (AE 24.0+) | bool | UNVERIFIED: false |
  | Text engine | `composerEngine` | enum `LATIN_CJK_ENGINE` / `UNIVERSAL_TYPE_ENGINE` | `UNIVERSAL_TYPE_ENGINE` for new layers; only Latin/CJK → Universal can be written |

  - Single-line composer: breaks each line greedily, preferring to compress spacing before expanding. Every-line composer: optimizes breakpoints over the paragraph (Knuth–Plass-like, squared penalties). Exact Adobe penalties are proprietary.
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** BAE decision: implement greedy single-line breaking with UAX #14 break opportunities + kinsoku (AE-TXT-088) first (P0 via AE-TXT-040); offer a Knuth–Plass every-line mode later; mark output differences against AE as acceptable. Hyphenation dictionaries: none bundled in M1.

### AE-TXT-033 · Paragraph direction (段落方向)

- **What it does:** Sets the base direction of a paragraph for bidirectional text.
- **Exact semantics:**
  - `direction` (AE 24.0+): `ParagraphDirection.DIRECTION_LEFT_TO_RIGHT` (default) / `DIRECTION_RIGHT_TO_LEFT`; Paragraph panel buttons "Left-to-Right Text Direction" / "Right-to-Left Text Direction" (从左到右 / 从右到左文本方向, 译名待核).
  - Base direction feeds the Unicode Bidirectional Algorithm (UAX #9) paragraph level; RTL paragraphs order runs right to left and swap the start/end indent sides.
  - Changing direction keeps alignment buttons literal (Left stays left) UNVERIFIED.
  - Per-character animation follows logical order (index 0 = first logical character), so a left-to-right Range Selector sweep on Arabic text reveals from the right, which is what AE's Universal Text Engine does ("animates them based on their direction").
- **Priority:** P1 — needed for Arabic/Hebrew; not daily use for Chinese users.
- **Owner:** bae-text
- **Notes:** See AE-TXT-092 for shaping. Essential Graphics panel can also set LTR/RTL (out of scope).

### AE-TXT-034 · Leading type, hanging punctuation and line join type (行距类型 / 悬挂标点)

- **What it does:** Paragraph options for how leading is measured and whether punctuation hangs outside the margins.
- **Exact semantics:**

  | Option (中文) | Scripting | Type | Default | Meaning |
  |---|---|---|---|---|
  | Top-to-Top Leading (顶到顶行距) / Bottom-to-Bottom Leading (底到底行距) | `leadingType`: `JAPANESE_LEADING_TYPE` / `ROMAN_LEADING_TYPE` (UNVERIFIED mapping) | enum | Roman (bottom-to-bottom) | Roman: leading is the distance from the previous line's baseline to this line's baseline. Top-to-top: distance between the tops of the em boxes of consecutive lines (for vertical text: right edges). |
  | Roman Hanging Punctuation (罗马式悬挂标点) | `hangingRoman` | bool | false | in box text, the following may hang outside the margins: . , ' " ’ ” - — – : ; (period, comma, single/double quotes, apostrophe, hyphen, em dash, en dash, colon, semicolon). Double-byte CJK punctuation never hangs via this option. |
  | Line join type for stroke | `lineJoinType` | enum | see AE-TXT-016 | character option, listed here only for completeness |

- **Priority:** P2
- **Owner:** bae-text
- **Notes:** Scripting AE 24.0+. Hanging amount: the whole punctuation advance hangs (UNVERIFIED; InDesign optical margin is different and not in AE).

### AE-TXT-035 · Default text style, Reset Character and Reset Paragraph (重置字符 / 重置段落)

- **What it does:** The Character/Paragraph panel state used for new text, and the commands that restore factory defaults.
- **Exact semantics:**
  - With no text and no text layer selected, panel edits change the default style used for the next text created (persisted in preferences).
  - Reset Character (重置字符, Character panel menu) / scripting `resetCharStyle()`: sets all characters (selection or whole layer) to factory character defaults. Reset Paragraph (重置段落) / `resetParagraphStyle()`: factory paragraph defaults.
  - Factory defaults (UNVERIFIED unless stated): font = application default font (AE ships with a default; on fresh install commonly "Myriad Pro Regular"/"Source Sans" depending on version — BAE decision: bundled "Source Han Sans SC Regular" or the OS UI font, so Chinese renders out of the box), size 36 px (UNVERIFIED), fill white [1,1,1] on, stroke off with colour black and width 1 px (UNVERIFIED), leading Auto, autoLeading 120%, kerning Metrics, tracking 0, vertical/horizontal scale 100%, baseline shift 0, tsume 0, faux/caps/baseline options off, ligatures on (UNVERIFIED), justification Left, indents 0, spaces 0, single-line composer (UNVERIFIED), hyphenate off (UNVERIFIED), direction LTR, Fill Over Stroke.
- **Priority:** P1
- **Owner:** bae-text (+ bae-model, bae-ui-motion)
- **Notes:** The Character panel UI and its menu are specified in ui-preview-render-io.md (Character, Paragraph panels). Smart quotes (Use Smart Quotes / 使用智能引号, 译名待核, Character panel menu) is a typing-time substitution of straight quotes by curly quotes, P2, bae-ui-motion.

### AE-TXT-036 · Style runs and range accessors (字符范围与段落范围)

- **What it does:** Per-character and per-paragraph formatting, editable via text selection in the UI and via scripting range objects (AE 24.3+).
- **Exact semantics:**
  - UI: any character attribute can differ per character (select text with a type tool, change Character panel); paragraph attributes apply to every paragraph touched by the selection.
  - Scripting (AE 24.3, 2024):
    - `textDocument.characterRange(start, [end])` → `CharacterRange` (end default `start+1`; `-1` = dynamic text length; cannot span the final CR). Exposes the character attributes (fillColor, strokeColor, strokeOverFill, kerning, font, size, etc.), `text`, `characterStart`, `characterEnd` (exclusive), `isRangeValid`, `pasteFrom(range)`, `toString()`.
    - `paragraphRange(start, [end])` → `ParagraphRange` (paragraph attributes); `paragraphCharacterIndexesAt(i)` → `{start, end}`.
    - `composedLineRange(start, [end])` → `ComposedLineRange` (read-only view of laid-out lines); `composedLineCharacterIndexesAt(i)`; `composedLineCount` (static snapshot taken when the TextDocument was read).
    - Mixed values in a range read as `undefined`; writes apply to the whole range. Range objects store their constructor parameters and throw on access when the document length makes them invalid.
  - Expressions (AE 25.0+): `text.sourceText.style` and `getStyleAt(charIndex, t)`, `setStyle` methods; spec in transform-keyframes-expressions.md (Text style API).
- **Priority:** P0 (per-character styling in the UI); P1 (range scripting API)
- **Owner:** bae-model (+ bae-text)
- **Notes:** Implementation hint: keep run boundaries on grapheme cluster boundaries (AE-TXT-012) so a style never splits a cluster.

### AE-TXT-037 · Read-only layout results (基线位置等布局结果)

- **What it does:** Layout-derived values that scripts and expressions read back.
- **Exact semantics:**

  | Field | Type | Meaning |
  |---|---|---|
  | `baselineLocs` (AE 13.6+) | float array | for each composed line `[start_x, start_y, end_x, end_y]` in layer space; wrapped lines count separately; a line without characters reports 3.402823466e+38 for all four |
  | `composedLineCount` | int | number of composed lines; may be 0 if all text is overset |
  | `paragraphCount` | int | ≥ 1 |
  | `boxOverflow` (AE 24.6+) | bool | text overset |
  | `pointText`, `boxText` | bool | kind of text |

  - Values reflect the composed state at the time the TextDocument was read (`start_x/end_x` = baseline extent of the line's characters, UNVERIFIED whether trailing spaces are included).
- **Priority:** P1
- **Owner:** bae-text (+ bae-model)
- **Notes:** These are not persisted; they are computed by `bae-text` on demand.

## Source Text animation

### AE-TXT-038 · Source Text keyframes (源文本关键帧)

- **What it does:** Animates the whole TextDocument (text and formatting) over time by switching between keyframed values.
- **Exact semantics:**
  - Interpolation is always Hold (定格): the value of key k is used from its time up to (not including) the next key's time; before the first key the first key's value is used. Keyframe icons show as hold keys; temporal ease, roving and spatial interpolation are not available; Keyframe Interpolation dialog cannot change it.
  - Each key stores a complete TextDocument (text + all runs + box). Editing text or formatting while the CTI is not on a key and the stopwatch is on creates a new key at the CTI holding the edited document.
  - Panel edits with the layer (not text) selected apply to the selected Source Text keyframes too (AE-TXT-009).
  - Paste of keys, time-reverse keys, Sequence/Time stretch behave as for any hold key (see transform-keyframes-expressions.md, Keyframes).
  - Box text: box size/position are part of each key, so resizing the box with keys creates a jump at the key time.
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Scripting: `Property.isInterpolationTypeValid(KeyframeInterpolationType.LINEAR)` is false for Source Text; `setValueAtTime(t, TextDocument)`. Lottie: `t.d.k[]` = array of `{t: frame, s: document}`; lottie-web selects the last key with `t ≤ frame` (first key before it). Expression `sourceText.key(n).value`. Typical usage: typewriter via Source Text keys is rare; use animators.

### AE-TXT-039 · Source Text expressions (源文本表达式)

- **What it does:** An expression on Source Text computes the displayed string (and, AE 25.0+, its style) every frame.
- **Exact semantics:**
  - Return value: a string, a number (converted with `String()`), or a TextDocument/style object from the text style API. The returned string replaces `text`; formatting then comes from the pre-expression value's styles. UNVERIFIED rule: AE keeps the style runs of the pre-expression value mapped by character index, and characters beyond the original length take the style of the last character. BAE decision until verified: style of the first character applies to the whole result unless the expression returns a styled object.
  - `value` / `thisProperty.value` inside the expression is the pre-expression TextDocument; `value.toString()` / `text.sourceText` yield its string.
  - `\r` and `\n` in the returned string are paragraph breaks (AE-TXT-011).
  - Common patterns that must work: `thisComp.layer("X").text.sourceText`, `timeToTimecode()`, `Math.round(effect("Slider Control")("Slider"))`, `marker.key(n).comment`, `sourceText.split("")`.
  - Expression errors leave the pre-expression value (see transform-keyframes-expressions.md, Expression errors).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-text)
- **Notes:** The text style API (`getStyleAt`, `setFont`, `setFontSize`, `setFillColor`, `setText`, `createStyle`, AE 17.0+; per-character ranges AE 25.0+) and its full method list are in transform-keyframes-expressions.md (Text style API) — do not duplicate here. Lottie: expressions on `t.d.x`; lottie-web wraps in `ExpressionTextPropertyDecorator`. Implementation hint: evaluate Source Text expressions before layout; only re-shape when the returned value differs from the previous frame (string compare), so counters do not trigger full reshaping when unchanged.

## Layout semantics

### AE-TXT-040 · Line breaking (换行)

- **What it does:** Decides where paragraph text wraps and how point text lines are formed.
- **Exact semantics:**
  - Point text: lines break only at CR (new paragraph) and ETX (soft return).
  - Paragraph text: each paragraph is composed into lines of available width `W = boxW - 2*inset - startIndent - endIndent` (first line `- firstLineIndent`), measured with advances after kerning, tracking, tsume, horizontal scale, faux bold (but not animator effects).
  - Break opportunities: Unicode line-break rules (UAX #14) for the paragraph's language — after spaces and hyphens for Latin; between any two CJK ideographs/kana; never before closing punctuation or after opening punctuation (kinsoku, AE-TXT-088); never inside No Break runs.
  - Trailing spaces at a line end do not count toward the width (they hang; they remain characters for animator indexing). UNVERIFIED: AE keeps the space character at the end of the earlier line.
  - A word longer than `W` is broken at a character boundary (emergency break) (UNVERIFIED; lottie-web does the same).
  - Single-line vs Every-line composer: AE-TXT-032. Auto-hyphenation: AE-TXT-032.
  - Text that cannot fit vertically is overset (AE-TXT-004).
- **Priority:** P0 — correct CJK breaking is required for Chinese users.
- **Owner:** bae-text
- **Notes:** BAE decision: use parley's line breaker (ICU4X segmenter, UAX #14) plus a kinsoku pass; document known divergences from Adobe composers in bae-text tests. lottie-web breaks only at spaces (`lastSpaceIndex`) — do not copy it for CJK.

### AE-TXT-041 · Line placement and leading (行位置与行距)

- **What it does:** Positions successive baselines.
- **Exact semantics:**
  ```
  // Roman (bottom-to-bottom) leading, horizontal text
  y_baseline[0] = firstBaseline           // 0 for point text; AE-TXT-042 for box text
  y_baseline[n] = y_baseline[n-1] + leading(line n)
                + (line n starts a new paragraph ? spaceAfter(prev para) + spaceBefore(this para) : 0)
  leading(line n) = auto ? autoLeadingPct/100 * maxFontSize(line n) : max(leading attr of chars on line n)
  ```
  - Empty lines (consecutive CRs) use the leading of the paragraph-mark character's style.
  - Line Spacing animator offsets are added afterwards (no reflow), see AE-TXT-065.
  - Japanese/top-to-top leading: AE-TXT-034.
  - Vertical text: lines (columns) advance to the left by the leading.
- **Priority:** P0
- **Owner:** bae-text
- **Notes:** lottie-web: `yPos += lh` per line (`finalLineHeight`, default `1.2*size`). Verify against AE by reading `baselineLocs` in test projects.

### AE-TXT-042 · First baseline position in paragraph text (段落文本首行基线)

- **What it does:** Places the first line inside the box.
- **Exact semantics:**
  - Legacy / default behaviour (`boxFirstBaselineAlignment = ASCENT` or `LEGACY_METRIC`): first baseline y = `boxTextPos[1] + inset + ascent(first line)`, where ascent = max over the first line of the font's ascender scaled to its font size (UNVERIFIED which font table: hhea/OS2 typo ascender; lottie-web uses the exported `fonts.list[].ascent` percentage: `ps[1] + ascent*size/100`).
  - Other alignments use cap height, x-height, em-box top (`0.88 em` ideographic, UNVERIFIED), leading (`= leading`), typo ascent, or minimum Roman/Asian values (AE 24.6+, AE-TXT-031).
  - x start of each line: `boxTextPos[0] + inset + startIndent (+ firstLineIndent)` then alignment within `W` (AE-TXT-043).
- **Priority:** P0
- **Owner:** bae-text
- **Notes:** Getting this offset right is what makes BAE box text line up with AE projects; add an AE-reference test (export `baselineLocs` from AE via script for several fonts).

### AE-TXT-043 · Alignment and justification layout (对齐与两端对齐排版)

- **What it does:** Distributes each composed line horizontally.
- **Exact semantics:**
  ```
  lineW = sum(advances of line, excluding trailing spaces and trailing tracking)
  free  = W_line - lineW
  Left:   x0 = start
  Center: x0 = start + free/2
  Right:  x0 = start + free
  Justify (Last Left/Center/Right/Full):
     every line except the paragraph's last line (and lines ended by a soft return? UNVERIFIED) is stretched to W_line:
       distribute free over inter-word spaces (word spacing, Adobe defaults min 80% / desired 100% / max 133%),
       then letter spacing (0/0/0 by default, i.e. none), then glyph scaling (100/100/100, none)  // UNVERIFIED defaults
     the last line uses Left / Center / Right, or for Justify All is also stretched
  Point text: start = 0 and W_line = 0, so Center puts the line centre at x = 0 and Right puts the line end at x = 0.
  ```
  - CJK lines without spaces justify by adding equal space between all characters (UNVERIFIED).
  - Justification adjusts glyph positions only; it does not change character indices.
  - Animator Tracking on centred/right-aligned point text re-centres the line (AE-TXT-064).
- **Priority:** P0 (Left/Center/Right); P1 (Justify)
- **Owner:** bae-text
- **Notes:** AE has no UI for word/letter spacing justification settings (Photoshop does); values are the Adobe defaults (UNVERIFIED). lottie-web: point text `justifyOffset = -boxWidth` (right) or `-boxWidth/2` (centre).

### AE-TXT-044 · Character advance and glyph placement (字符进距与字形定位)

- **What it does:** Combines shaping and character attributes into each glyph's position within the line.
- **Exact semantics:**
  ```
  for each character c in line order (visual order for bidi runs):
     glyphs = shape(c's cluster, font, size, features[liga, kern if Metrics, smcp...], variation coords)
     adv = shapedAdvance * horizontalScale
         + manualKern(c)/1000*size + tracking(c)/1000*size - tsumeShrink(c)
     glyph origin = (penX + tsumeShift, baselineY - baselineShift(c))     // y down
     penX += adv
  ```
  - Each character's "baseline origin" (the per-character transform origin before anchor grouping) is the left end of its advance on the baseline (horizontal text); the character's default anchor point is the centre of its advance at the baseline (UNVERIFIED: AE uses advance centre on the baseline; lottie-web uses `letters[i].an/2`, i.e. half the advance), see AE-TXT-054.
  - Spaces are characters with an advance and no outline.
- **Priority:** P0
- **Owner:** bae-text
- **Notes:** Implementation hint: shape once per (TextDocument, font set) and cache; animators only transform the cached per-character outlines.

### AE-TXT-045 · Glyph fallback for missing glyphs (缺失字形回退)

- **What it does:** Renders characters that the chosen font does not contain using another font.
- **Exact semantics:**
  - AE (Universal Text Engine) substitutes a fallback font per character when the run's font lacks the glyph (e.g. Chinese typed in a Latin font renders in a system CJK font); without a usable fallback a .notdef box (tofu) is shown (UNVERIFIED which fallback list AE uses; it is platform/Adobe internal).
  - Fallback does not change the stored font of the run (the Character panel still shows the chosen font).
  - Metrics (advance, ascent) come from the fallback font for those characters.
- **Priority:** P1 — Chinese users mix Latin fonts with Chinese text daily (CJK fallback), so BAE must never show tofu for CJK.
- **Owner:** bae-text
- **Notes:** BAE decision: fallback chain = (run font) → same-family CJK companions → OS fallback via fontique (by script + locale, zh-Hans first for Chinese UI) → bundled Source Han Sans subset (if size budget allows). Record the resolved fallback face in the render cache key so renders are deterministic across machines (and warn when the fallback differs from the one recorded at save time).

## Path Options

### AE-TXT-046 · Path Options group and Path selection (路径选项 / 路径)

- **What it does:** Makes the text follow one of the layer's own masks used as a path.
- **Exact semantics:**
  - `Path Options` (`ADBE Text Path Options`) contains `Path` (路径): a menu listing `None` (无) followed by every mask of the same layer by name, in mask order. Default `None`. Choosing a mask reveals the five properties of AE-TXT-047.
  - The Path menu value is not keyframeable (it is a layer-level reference; UNVERIFIED whether expressions can drive it — they cannot in AE); the referenced mask's Mask Path is used at the current time, so animating the mask animates the text path.
  - Open and closed masks both work. A closed mask still cuts the layer unless its mode is set to None (AE does not change the mode automatically; the help recommends None). Open masks do not mask the layer.
  - Deleting the mask resets Path to None; renaming it keeps the reference (references are by mask identity, not name).
  - Path Options has an eye (visibility) switch: when off, text renders as if Path = None (useful for editing), without losing settings.
  - With a path set, paragraph text: UNVERIFIED — AE flows the text as a single stream on the path (box width ignored); BAE decision: compose lines as point text (hard returns only) and stack additional lines perpendicular to the path by the leading.
- **Priority:** P1 — common for motion designers, not daily for every user.
- **Owner:** bae-text (+ bae-model)
- **Notes:** Mask path data and modes: compositing-3d-color.md (Masks). Lottie: `t.p.m` = mask index (0-based into `masksProperties`), absent `t.p` or no `m` = no path. Scripting: `text.pathOption.path` is not scriptable as a value in old versions; matchName of the Path menu: UNVERIFIED `ADBE Text Path` (not listed in the docsforadobe tables).

### AE-TXT-047 · Path placement properties and math (路径属性与定位计算)

- **What it does:** Places each character along the path by arc length, optionally rotated to the tangent.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | matchName / Lottie |
  |---|---|---|---|---|---|
  | Reverse Path (反转路径) | bool (On/Off) | Off | — | — | `ADBE Text Reverse Path` / `t.p.r` |
  | Perpendicular To Path (垂直于路径) | bool | On | — | — | `ADBE Text Perpendicular To Path` / `t.p.p` |
  | Force Alignment (强制对齐) | bool | Off | — | — | `ADBE Text Force Align Path` / `t.p.a` |
  | First Margin (首字边距) | float | 0 | unbounded | px (arc length) | `ADBE Text First Margin` / `t.p.f` |
  | Last Margin (末字边距) | float | 0 | unbounded | px (arc length) | `ADBE Text Last Margin` / `t.p.l` |

  All five are keyframeable (booleans as hold keys).
  ```
  P(s), T(s): point and unit tangent of the mask path at arc length s (cubic Béziers, mask vertices in layer space);
              Reverse Path traverses vertices in reverse order (s measured from the old end).
  L = total arc length (closed path includes the closing segment).
  For each line, lineW = composed line width (AE-TXT-043, without trailing spaces):
    Left:   s0 = FirstMargin
    Right:  s0 = L - lineW + LastMargin    // Skottie adds Last Margin as a signed offset; UNVERIFIED sign in AE (help: "relative to the end of the path")
    Center: s0 = L/2 - lineW/2 + FirstMargin + LastMargin           // Skottie model (both margins shift the centred line); UNVERIFIED in AE
    (First Margin is ignored for Right with Force Alignment off; Last Margin is ignored for Left with Force Alignment off.)
  For each character c with advance a_c and pen offset x_c within the line (after tracking/kerning):
    s_c = s0 + x_c + a_c/2  (+ animator Position.x and Tracking, which move along the path)
    if Force Alignment: characters are spread so the first character's centre is at FirstMargin + a_first/2
       and the last character's centre at L - LastMargin - a_last/2, with equal centre spacing in between
       (UNVERIFIED: AE spreads equal gaps; lottie-web spreads centres equally and ignores Last Margin)
    closed path: s_c wraps modulo L (negative margins wrap backwards)
    open path:   s_c < 0 or s_c > L extrapolates along the tangent line of the first / last segment end
    character placement: baseline-centre point of c := P(s_c); 
       rotation := Perpendicular ? atan2(T(s_c)) : 0     // glyph's baseline aligned to the tangent
  Additional lines (point text with hard returns) are offset from the path along the character's local -y (perpendicular) by the line's baseline offset (AE-TXT-041).
  Animator Position.y moves the character perpendicular to the path (in its rotated frame); Position.z along z.
  Baseline shift moves perpendicular to the path.
  ```
  - Tangent at a cusp/zero-length handle: use the nearest non-degenerate derivative.
  - Vertical text on a path: UNVERIFIED (BAE: unsupported, falls back to horizontal on path).
  - Viewer handles: left/center/right margin controls are draggable; Shift-drag snaps to mask vertices (UI).
- **Priority:** P1
- **Owner:** bae-text (+ bae-vector)
- **Notes:** lottie-web places by sampled arc length (`bez.buildBezierData`) and rotates by `atan` of the chord between samples, extrapolating beyond the open end linearly; BAE should use exact arc length (kurbo `ParamCurveArclen` with tolerance 0.01 px) and true tangents. AE default for Perpendicular To Path is On (characters rotate to follow the curve).

## More Options

### AE-TXT-048 · Anchor Point Grouping and Grouping Alignment (锚点分组 / 分组对齐)

- **What it does:** Chooses whether per-character transforms pivot around each character, word, line or the whole text, and offsets that pivot.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Values / Range | Units | Lottie |
  |---|---|---|---|---|---|
  | Anchor Point Grouping (锚点分组) | enum | Character (字符) | Character (字符) / Word (词) / Line (行) / All (全部) | — | `t.m.g` = 1 / 2 / 3 / 4 |
  | Grouping Alignment (分组对齐) | 2D [x, y] | [0, 0] | unbounded | % | `t.m.a` (`ADBE Text Anchor Point Align`) |

  ```
  group(c) = c itself | the word containing c (space-delimited; a space belongs to no group's width) |
             the composed line containing c | all characters
  group width Wg = sum of advances in the group (excluding trailing space)
  groupAnchorX  = left of group + Wg/2 + GroupingAlignment.x/100 * Wg/2
  groupAnchorY  = baseline + GroupingAlignment.y/100 * (0.857 * fontSize)    // y down; UNVERIFIED constant
  // lottie-web: x offset = alignment.x * an * 0.005, y offset = alignment.y * (size*1.2*0.714) * 0.01
  ```
  - Grouping Alignment 0% = the group's anchor at its horizontal centre on the baseline; -100% / +100% x = left / right edge; negative y moves the anchor up, positive down (help: lowering values moves anchors up-left).
  - All per-character Anchor/Position/Scale/Skew/Rotation operations of AE-TXT-054 use the group anchor of the character's group, so a whole word rotates as a unit around the word's anchor with Word grouping.
  - Grouping affects only transform pivots, not selector indexing (use Based On for that).
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** Anchor Point Grouping matchName UNVERIFIED (`ADBE Text Anchor Point Option`). Viewer shows the anchors when these properties are selected (UI).

### AE-TXT-049 · Fill & Stroke layer option (填充和描边)

- **What it does:** Layer-wide paint order of fills and strokes across characters.
- **Exact semantics:**
  - Menu values in order: Per Character Palette (每字符调板, 译名待核) / All Fills Over All Strokes (全部填充在全部描边之上) / All Strokes Over All Fills (全部描边在全部填充之上). Default Per Character Palette. Keyframeable as hold (UNVERIFIED).
  - Render order semantics: AE-TXT-017.
  - Unavailable (greyed out, ignored) with per-character 3D.
- **Priority:** P0 — needed to render stroked text correctly.
- **Owner:** bae-text (+ bae-render)
- **Notes:** matchName UNVERIFIED: `ADBE Text Render Order` (not listed in the docsforadobe text matchName table). Not in Lottie (lottie-web uses the document `of` flag only).

### AE-TXT-050 · Inter-Character Blending (字符间混合)

- **What it does:** Blends each character with the characters drawn before it (within the layer) using a blend mode.
- **Exact semantics:**
  - Menu of blend modes, default Normal. Values: the layer blend modes of the Modes column (see compositing-3d-color.md, Blend modes) — UNVERIFIED whether AE lists the full set or a subset.
  - Characters are composited in text order into a transparent layer buffer: `buffer = blend(char_i over buffer, mode)`; the resulting layer then composites into the comp with the layer's own mode.
  - Unavailable with per-character 3D.
- **Priority:** P2
- **Owner:** bae-render (+ bae-text)
- **Notes:** matchName UNVERIFIED (`ADBE Text Character Blend Mode`). Implementation hint: rendering per character into one buffer with a blend per draw breaks Vello batching; only switch to per-character passes when the mode is not Normal.

### AE-TXT-051 · Variable Font Spacing (变量字体间距)

- **What it does:** Controls how character spacing compensates when variable-font axis animators change glyph widths.
- **Exact semantics:**
  - More Options menu `Variable Font Spacing`, values: Adaptive (自适应) / Per-Character (按字符) / Default (默认). Default: UNVERIFIED (Adaptive). The property appears in More Options only when at least one Variable Font Axes animator property exists on the layer.
    - Adaptive: applies per-character tracking compensation "when appropriate" (heuristic, UNVERIFIED).
    - Per-Character: always adjusts each character's advance to the animated axis values (layout follows the animated width).
    - Default: no compensation; characters keep the spacing of the un-animated font (glyphs may overlap or gap).
  - Version: appears with variable-font axis animators (AE-TXT-068): first public in AE beta 26.0x40 (Nov 2025), shipped in AE 26.x (UNVERIFIED exact release; help page "Working with Variable Font Axes" dated Jun 2026).
- **Priority:** P2
- **Owner:** bae-text
- **Notes:** matchName `ADBE Text Variable Font Spacing`. BAE decision: implement Per-Character and Default exactly; implement Adaptive as Per-Character for weight/width axes only until measured.

## Animators

### AE-TXT-052 · Text animator groups and the Animate / Add menus (文本动画制作器)

- **What it does:** An animator is a named group holding selectors and animator properties; properties affect characters in proportion to the combined selector value.
- **Exact semantics:**
  - Structure (`ADBE Text Animators` > `ADBE Text Animator` "Animator N" (动画制作器 N)): `Selectors` (`ADBE Text Selectors`) containing 0..n selectors (Range, Wiggly, Expression), then `Properties` (`ADBE Text Animator Properties`) containing 1..n animator properties. The timeline shows selectors first, then properties, inside the animator.
  - Animate menu (Timeline `Animate:` button next to Text, Animation > Animate Text (动画 > 动画文本), or viewer context menu > Animate Text), items in order: Enable Per-character 3D (启用逐字 3D 化); Anchor Point, Position, Scale, Skew, Rotation, Opacity, All Transform Properties (全部变换属性); Line Anchor (行锚点), Line Spacing (行距); Character Value (字符值), Character Offset (字符位移); Fill Color > RGB / Hue / Saturation / Brightness / Opacity; Stroke Color > RGB / Hue / Saturation / Brightness / Opacity; Stroke Width (描边宽度); Tracking (字符间距); Blur (模糊); Variable Font Axes > [axis] (AE 26.0, UNVERIFIED version). UNVERIFIED exact grouping separators.
  - Choosing an item with a text layer selected creates a new animator "Animator N" with one Range Selector 1 (defaults of AE-TXT-070) and that property; with an animator selected, Animation > Animate Text adds the property to it instead. With characters selected in the viewer, the new Range Selector's Start/End are set to the selected character range (UNVERIFIED: in Index units? AE sets Start/End percentages matching the selection).
  - Each animator row has an `Add:` menu: Property > (same list) and Selector > Range / Wiggly / Expression (also Animation > Add Text Selector, and viewer context menu > Add Text Selector).
  - All Transform Properties adds Anchor Point, Position, Scale, Skew, Skew Axis, Rotation, Opacity at once.
  - Character Value or Character Offset also adds Character Range. Tracking adds Tracking Type and Tracking Amount. Skew adds Skew Axis.
  - Animation > Remove All Text Animators (移除所有文本动画器) deletes every animator. Animators and selectors can be renamed, reordered by drag, copied/pasted, duplicated and deleted like other property groups.
  - Deleting all selectors of an animator makes its properties apply fully (value 100%) to all characters.
  - An animator can be disabled with its eye switch (UNVERIFIED: animator groups have a visibility switch in AE 2026).
- **Priority:** P0
- **Owner:** bae-model (+ bae-motion, bae-ui-motion)
- **Notes:** Lottie `t.a[]` = animators: each `{nm, s: selector, a: properties}` — Lottie supports exactly one selector per animator (bodymovin exports the first range selector; animators without selectors get a placeholder selector). Scripting: `layer.text.animators.addProperty("ADBE Text Animator")`, then `.property("ADBE Text Animator Properties").addProperty("ADBE Text Position 3D")`, selectors via `.property("ADBE Text Selectors").addProperty("ADBE Text Selector")`.

### AE-TXT-053 · Animator evaluation and accumulation (动画制作器的计算与叠加)

- **What it does:** Defines how per-character values are computed from the base layout, all animators and their selector values.
- **Exact semantics:**
  - For every animator j and every character i, the selector stack yields a selection value `m_ij` (a 3-vector, components in [-1, 1], AE-TXT-069). For 1-D properties a fixed component is used (lottie-web): Rotation Z → m[2], Y Rotation → m[1], X Rotation → m[0], Opacity / Skew / Stroke Width / Tracking / Fill & Stroke colour → m[0], Skew Axis → m[1]; vector properties use their own components.
  - Property kinds:

    | Kind | Properties | Per-animator update of character value v |
    |---|---|---|
    | additive | Anchor Point, Position, Skew, Skew Axis, Rotation, X/Y Rotation, Stroke Width, Tracking Amount, Line Spacing, Character Offset, Blur, Fill/Stroke Hue, Saturation, Brightness | `v += P * m` |
    | multiplicative | Scale | `v *= 1 + (P/100 - 1) * m` (per axis) |
    | interpolative | Opacity, Fill/Stroke Opacity, Fill Color RGB, Stroke Color RGB, Character Value (UNVERIFIED), Line Anchor (UNVERIFIED) | `v = v + (P - v) * m` |

  - Animators are processed in timeline order (top first) per property kind; for transforms the order of matrix composition is fixed by kind (AE-TXT-054), so reordering animators changes only interpolative results (colour/opacity) and Character Value.
  - Base values: transforms identity, opacity 100%, colours from the character's TextDocument fill/stroke, stroke width from the TextDocument.
  - Layout-affecting properties (Tracking, Line Spacing, Line Anchor, Character Offset/Value, Variable Font Axes) modify the glyph placement after the TextDocument layout; they never re-break paragraph lines (UNVERIFIED for Character Offset/Value: AE re-measures replaced glyph advances but keeps line breaks).
  - Opacity quirk: lottie-web computes `o = o + (P*m - o)*m`; BAE follows the interpolative rule above (UNVERIFIED which matches AE for 0 < m < 1 and P ≠ 0).
- **Priority:** P0
- **Owner:** bae-text (+ bae-anim)
- **Notes:** Implementation hint: evaluate selectors into an `[N_chars × N_animators] × vec3` buffer, then per property kind a single pass; this vectorizes well (SIMD or a compute shader for very long texts).

### AE-TXT-054 · Per-character transform composition order (逐字符变换顺序)

- **What it does:** Builds each character's final matrix from layout, grouping, and animator transforms.
- **Exact semantics:**
  ```
  // column-vector convention, applied right-to-left to glyph outline points (layer space, y down)
  ga  = group anchor of the character's group (AE-TXT-048) expressed relative to the character's baseline origin
  M_i = T(layoutOrigin_i)                 // pen position on the baseline incl. alignment/justification, box offset, baseline shift
      · T(ga)                             // move back from the group anchor
      · T(Σ_j Position_j * m_ij)          // all animators' positions (additive), in the character's parent (layout) frame
      · Rx(Σ XRot·m) · Ry(Σ YRot·m) · Rz(Σ Rot·m)   // per animator in order: skew, Z, Y, X are appended per animator (lottie-web order: skew, Rz, Ry, Rx per animator)
      · Skew(Σ ...)
      · S(Π_j (1 + (Scale_j/100 - 1) * m_ij))
      · T(-Σ_j Anchor_j * m_ij)           // animator anchor point (additive)
      · T(-ga)                            // put the group anchor at the origin
  On a path (AE-TXT-047): layoutOrigin_i and an extra rotation R(tangent) replace T(layoutOrigin_i); Position.x moves along the arc length instead of along x.
  3D (per-character 3D): Position/Anchor/Scale carry z; Rx, Ry active; result is a 4x4 matrix per character.
  Final glyph matrix = LayerMatrix · M_i.
  ```
  - Rotation sign: positive Rotation is clockwise on screen (y down), same as layer rotation.
  - Skew: Skew value (degrees, -85..85) shears along the Skew Axis direction (degrees, 0 = shear along x).
  - Per animator lottie-web appends, in this order: skew(axis), Rz, Ry, Rx; i.e. within one animator Rx is applied last (outermost).
- **Priority:** P0
- **Owner:** bae-text (+ bae-anim)
- **Notes:** Layer transform order (anchor → scale → skew → rotation → position) is in transform-keyframes-expressions.md (Transform matrix order); text uses the same order per character. UNVERIFIED: AE's rotation order for per-character 3D (lottie-web: Z then Y then X, matching AE layer "Orientation then X,Y,Z rotation" order is not identical — verify with an AE test).

### AE-TXT-055 · Anchor Point animator property (锚点)

- **What it does:** Offsets the pivot of selected characters for scale/rotation/skew.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Anchor Point (锚点) | 2D (3D with per-character 3D) | [0, 0] / [0, 0, 0] | unbounded | px |

  - Additive (`Σ A_j·m`), applied as `T(-A)` inside the character frame (AE-TXT-054): positive x moves the pivot right and visually shifts the character left by the same amount when no rotation/scale (because `T(ga)·T(-A)` translates; UNVERIFIED that AE compensates position — AE does not, the character moves).
  - Z component only meaningful with per-character 3D; it makes Scale z visible.
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** matchName `ADBE Text Anchor Point 3D` (2D/3D same property). Lottie `t.a[].a.a`.

### AE-TXT-056 · Position animator property (位置)

- **What it does:** Moves selected characters.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Position (位置) | 2D (3D with per-character 3D) | [0, 0] / [0, 0, 0] | unbounded | px (layer space) |

  - Additive offset in the character's layout frame (not rotated by the character's own rotation). On a path, x moves along the path and y perpendicular to it.
  - Viewer: dragging selected text with the Selection tool while the Position property is selected edits x,y (not z).
- **Priority:** P0
- **Owner:** bae-text
- **Notes:** matchName `ADBE Text Position 3D`. Lottie `t.a[].a.p` (lottie-web negates z). Spatial interpolation of this property behaves like any position property (motion path shown when selected), see transform-keyframes-expressions.md.

### AE-TXT-057 · Scale animator property (缩放)

- **What it does:** Scales selected characters around their (group) anchor.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Scale (缩放) | 2D (3D with per-character 3D), constrain-proportions link on by default | [100, 100] / [100, 100, 100] | unbounded (negative flips) | % |

  - Per animator: `factor *= 1 + (Scale/100 - 1) * m` per axis (lottie-web; so m = 0.5 with Scale 0% gives 50%).
  - Scale z has no visible effect unless an Anchor Point animator has non-zero z.
  - Stroke widths scale with the glyph (geometry scale).
- **Priority:** P0
- **Owner:** bae-text
- **Notes:** matchName `ADBE Text Scale 3D`. Lottie `t.a[].a.s` (lottie-web divides by 100 on load).

### AE-TXT-058 · Skew and Skew Axis animator properties (倾斜 / 倾斜轴)

- **What it does:** Slants selected characters.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Skew (倾斜) | 1D | 0 | -85 – 85 (UNVERIFIED, same as layer skew in shapes) | degrees |
  | Skew Axis (倾斜轴) | 1D angle | 0x+0° | unbounded | degrees (revolutions + degrees display) |

  ```
  Skew(sk, ax) = R(ax) · [[1, tan(-sk)],[0,1]] · R(-ax)   // lottie-web skewFromAxis(-sk*m, ax*m); positive skew leans tops to the right (UNVERIFIED sign)
  ```
  - Both values are scaled by the selection (`sk*m[0]`, `ax*m[1]`) in lottie-web; additive across animators.
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** matchNames `ADBE Text Skew`, `ADBE Text Skew Axis`. Lottie `t.a[].a.sk`, `sa`. Same skew definition as shape group transforms (shapes.md, Group transform).

### AE-TXT-059 · Rotation animator properties (旋转 / X 轴旋转 / Y 轴旋转)

- **What it does:** Rotates selected characters around their anchor.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Units | Available |
  |---|---|---|---|---|
  | Rotation (旋转) — shown as Z Rotation (Z 轴旋转) with per-character 3D | 1D angle | 0x+0° | degrees | always |
  | X Rotation (X 轴旋转) | 1D angle | 0x+0° | degrees | per-character 3D only |
  | Y Rotation (Y 轴旋转) | 1D angle | 0x+0° | degrees | per-character 3D only |

  - Additive across animators (`Σ r_j·m_ij`); positive = clockwise for Z. Per-character 3D rotation order per AE-TXT-054.
- **Priority:** P0 (Rotation); P1 (X/Y)
- **Owner:** bae-text
- **Notes:** matchNames `ADBE Text Rotation` (Z), `ADBE Text Rotation X`, `ADBE Text Rotation Y`. Lottie `t.a[].a.r` (or `rz`), `rx`, `ry`.

### AE-TXT-060 · Opacity animator property (不透明度)

- **What it does:** Fades selected characters.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Opacity (不透明度) | 1D | 100 | 0 – 100 | % |

  - Interpolative: `o_i = o_i + (P - o_i) * m` per animator, starting at 100%; applies to the whole character (fill and stroke together). Result multiplies the character's coverage when compositing into the layer.
  - Negative selection values (wiggly) push opacity beyond the base: result clamped to [0, 100] (UNVERIFIED).
- **Priority:** P0 — the most common text reveal.
- **Owner:** bae-text (+ bae-render)
- **Notes:** matchName `ADBE Text Opacity`. Lottie `t.a[].a.o`. Typewriter reveal = Opacity 0 + Range Selector End/Start keyed, Square shape, Smoothness 0%.

### AE-TXT-061 · Per-character 3D (逐字 3D 化)

- **What it does:** Turns every character into its own 3D plane that can move, scale and rotate in z, X and Y.
- **Exact semantics:**
  - Animate > Enable Per-character 3D sets the layer's 3D switch on and `threeDPerChar = true` (switch column shows the per-character 3D icon instead of the 3D cube). It adds no animator, but every animator's Anchor Point, Position and Scale gain a z component and X/Y Rotation become available.
  - Turning per-character 3D off (choose the menu item again: layer stays 3D; or toggle the 3D switch: layer becomes 2D) discards all z values and X/Y rotations permanently (re-enabling does not restore them).
  - Rendering: each character is a separate planar layer in the comp's 3D space: participates in Classic 3D depth sorting and intersections per character, receives lights and casts/accepts shadows with the layer's Material Options (see compositing-3d-color.md, Classic 3D renderer). Fill & Stroke (layer-wide order) and Inter-Character Blending are disabled.
  - Camera depth of field and motion blur apply per character.
- **Priority:** P1
- **Owner:** bae-render (+ bae-text, bae-model)
- **Notes:** Scripting `TextLayer.threeDPerChar` (bool, read/write). Help warns per-character 3D can decrease performance. Lottie: bodymovin exports per-character 3D only in the HTML renderer (UNVERIFIED). Implementation hint: emit one 3D quad/plane per character into the same 3D sort as layers; batch characters with identical depth order.

### AE-TXT-062 · Fill Color animator properties (填充颜色)

- **What it does:** Changes the fill colour or opacity of selected characters.
- **Exact semantics:**

  | Property (中文) | Type | Default when added | Range | Units | Rule |
  |---|---|---|---|---|---|
  | Fill Color (填充颜色) "RGB" | colour | UNVERIFIED: red [1, 0, 0] | colour | — | interpolative toward P in RGB |
  | Fill Hue (填充色相) | angle | 0x+0° | unbounded | degrees | additive hue rotation (HSV/HSB) |
  | Fill Saturation (填充饱和度) | 1D | 0 | -100 – 100 | % | additive saturation |
  | Fill Brightness (填充亮度) | 1D | 0 | -100 – 100 | % | additive brightness (HSB V) |
  | Fill Opacity (填充不透明度) | 1D | 100 | 0 – 100 | % | interpolative; affects fill only |

  - Order inside one animator (lottie-web): RGB lerp, then hue, then saturation, then brightness; animators processed top to bottom.
  - Applies only to characters that have a fill in the TextDocument (UNVERIFIED: an RGB fill animator does not create a fill where `applyFill` is false).
  - Hue/sat/brightness arithmetic in HSV, results clamped to [0,1] per channel in 8/16 bpc (UNVERIFIED for 32 bpc over-range).
- **Priority:** P0 (RGB); P1 (Hue/Sat/Brightness/Opacity)
- **Owner:** bae-text
- **Notes:** matchNames `ADBE Text Fill Color`, `ADBE Text Fill Hue`, `ADBE Text Fill Saturation`, `ADBE Text Fill Brightness`, `ADBE Text Fill Opacity`. Lottie `fc`, `fh`, `fs`, `fb`, `fo`. lottie-web helpers `addHueToRGB`, `addSaturationToRGB`, `addBrightnessToRGB` convert RGB→HSV, add, convert back.

### AE-TXT-063 · Stroke Color and Stroke Width animator properties (描边颜色 / 描边宽度)

- **What it does:** Changes stroke colour, stroke opacity and stroke width of selected characters.
- **Exact semantics:**

  | Property (中文) | Type | Default when added | Range | Units | Rule |
  |---|---|---|---|---|---|
  | Stroke Color (描边颜色) "RGB" | colour | UNVERIFIED: red [1,0,0] | colour | — | interpolative |
  | Stroke Hue (描边色相) | angle | 0x+0° | — | degrees | additive |
  | Stroke Saturation (描边饱和度) | 1D | 0 | -100 – 100 | % | additive |
  | Stroke Brightness (描边亮度) | 1D | 0 | -100 – 100 | % | additive |
  | Stroke Opacity (描边不透明度) | 1D | 100 | 0 – 100 | % | interpolative; stroke only |
  | Stroke Width (描边宽度) | 1D | 0 | UNVERIFIED: 0 – 1000 | px | additive to the TextDocument stroke width; result clamped ≥ 0 |

  - Only characters with a stroke (`applyStroke`) show changes (UNVERIFIED for Stroke Width on unstroked text: AE does not add a stroke).
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** matchNames `ADBE Text Stroke Color`, `ADBE Text Stroke Hue`, `ADBE Text Stroke Saturation`, `ADBE Text Stroke Brightness`, `ADBE Text Stroke Opacity`, `ADBE Text Stroke Width`. Lottie `sc`, `sh`, `ss`, `sb`, `so`, `sw`.

### AE-TXT-064 · Tracking and Line Anchor animator properties (字符间距 / 行锚点)

- **What it does:** Adds extra spacing between selected characters without reflowing, and sets which point of each line stays fixed when tracking changes.
- **Exact semantics:**

  | Property (中文) | Type | Default | Values / Range | Units |
  |---|---|---|---|---|
  | Tracking Type (字符间距类型) | enum | Before & After (前后) | Before & After (前后) / Before (之前) / After (之后) (译名待核) | — |
  | Tracking Amount (字符间距大小) | 1D | 0 | unbounded | UNVERIFIED: px (lottie-web adds it in px); may be 1/1000 em |
  | Line Anchor (行锚点) | 1D | 0 | 0 – 100 | % |

  ```
  extra_i = TrackingAmount * m_i   (summed over animators)
  Before & After: extra_i/2 before and extra_i/2 after character i (Skottie/bodymovin "balanced" model);
  Before: all before; After: all after (UNVERIFIED)
  the first character of a line gets no "before" part and the last no "after" part (Skottie)
  Line Anchor L: after adding tracking, each line is shifted so that the point at fraction L of its original width stays fixed:
     lineShift = -L * (newLineWidth - oldLineWidth)
  0% = left edge fixed (left aligned growth), 50% = centre fixed, 100% = right edge fixed
  ```
  - The first character of each line receives no "before" spacing for left-aligned text (lottie-web skips tracking for the first character of a line when justification is Left).
  - For centred/right-aligned point text lottie-web compensates by -0.5 / -1 × added width (equivalent to Line Anchor 50% / 100%); in AE the compensation is controlled by Line Anchor (UNVERIFIED interaction with paragraph alignment).
  - On a path, tracking adds arc length.
- **Priority:** P0 — tracking animation is one of the most used text animations.
- **Owner:** bae-text
- **Notes:** matchNames `ADBE Text Track Type`, `ADBE Text Tracking Amount`, `ADBE Text Line Anchor`. Lottie `t.a[].a.t` (Letter Spacing); Line Anchor and Tracking Type are not exported (UNVERIFIED).

### AE-TXT-065 · Line Spacing animator property (行距)

- **What it does:** Moves lines apart (or together) in x and y without reflow.
- **Exact semantics:**

  | Property (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Line Spacing (行距) | 2D | [0, 0] | unbounded | px |

  ```
  lineSpacing(line k) = average over the line's characters of Σ_j LineSpacing_j * m_ij     // Skottie
offset(line 0) = 0;  offset(line k) = offset(line k-1) + lineSpacing(line k)            // cumulative; first line ignored
  ```
  - Model taken from Skottie (bodymovin-compatible); UNVERIFIED against AE for partially selected lines.
- **Priority:** P2
- **Owner:** bae-text
- **Notes:** matchName `ADBE Text Line Spacing`. Lottie `t.a[].a.ls` (not implemented in lottie-web).

### AE-TXT-066 · Character Offset, Character Value and Character Range (字符位移 / 字符值 / 字符范围)

- **What it does:** Replaces displayed characters by shifting their Unicode values or by a fixed code point, e.g. for "decoder" effects.
- **Exact semantics:**

  | Property (中文) | Type | Default | Values / Range | Notes |
  |---|---|---|---|---|
  | Character Range (字符范围) | enum | Preserve Case & Digits (保留大小写及数字) | Preserve Case & Digits / Full Unicode (完整的 Unicode) | added automatically with either property below |
  | Character Value (字符值) | integer | 0 (UNVERIFIED) | 0 – 0x10FFFF (UNVERIFIED) | replaces the code point |
  | Character Offset (字符位移) | integer | 0 | unbounded | added to the code point |

  ```
  off = round(Σ_j CharacterOffset_j * m_ij)            // UNVERIFIED rounding (AE shows integer steps)
  if Character Value present and m != 0: cp = CharacterValue    (UNVERIFIED: replacement only when |m| >= 0.5)
  cp' = cp + off
  Preserve Case & Digits: wrap within the character's class:
      'a'..'z' (26), 'A'..'Z' (26), '0'..'9' (10), other classes (symbols, katakana, hiragana, CJK, Hangul, Hebrew...) wrap within their Unicode block (UNVERIFIED);
      cp' = classStart + mod(cp - classStart + off, classSize)
  Full Unicode: cp' = cp + off with no wrapping (invalid/unassigned results render as .notdef or nothing, UNVERIFIED)
  Spaces and line breaks are never changed (UNVERIFIED).
  ```
  - Example from Adobe: offset 5 turns "offset" into "tkkxjy"; value 65 turns "value" into "AAAAA".
  - The replaced glyph is shaped with the run's font (fallback applies); its advance replaces the original (no reflow of paragraph line breaks, UNVERIFIED).
  - Universal Text Engine: correct Unicode ranges for Korean and Hebrew (help: "Character Offset identifies the correct Unicode range for Korean and Hebrew").
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** matchNames `ADBE Text Character Offset`, `ADBE Text Character Replace` (Character Value), Character Range UNVERIFIED `ADBE Text Character Change Type`. Not in Lottie.

### AE-TXT-067 · Blur animator property (模糊)

- **What it does:** Applies Gaussian blur to selected characters individually.
- **Exact semantics:**

  | Property (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Blur (模糊) | 2D [horizontal, vertical], linked | [0, 0] | ≥ 0 | px (blur radius, UNVERIFIED: same scale as Gaussian Blur effect's Blurriness) |

  - Additive across animators (`Σ B·m`), negative results clamp to 0. Each character is blurred separately (UNVERIFIED: AE blurs each character's own coverage, then composites characters in text order, so blur never mixes neighbours' pixels).
  - Blur radius is in layer space (scales with layer scale; UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render (+ bae-text)
- **Notes:** matchName `ADBE Text Blur`. Lottie `t.a[].a.bl` (not rendered by lottie-web). Implementation hint: group characters with identical blur into one offscreen pass; separable Gaussian in WGSL shared with effects.md (Gaussian Blur).

### AE-TXT-068 · Variable font axis animator properties (可变字体轴, 译名待核)

- **What it does:** Animates OpenType variable-font axes (weight, width, slant, custom axes) per character through the animator/selector system.
- **Exact semantics:**
  - Animate > Variable Font Axes > [axis name] (listed from the font's `fvar` axes) adds an animator with a Range Selector and a property "Font Axis <Name>" whose initial value is the current axis value of the (selected) characters.
  - Value: 1D, range = the font's axis min..max (input clamped), units = axis units (e.g. `wght` 100–900, `wdth` %, `slnt` degrees, `ital` 0–1).
  - Application: interpolative per character `axis_i = base_i + (P - base_i) * m` (UNVERIFIED; help: "Amount 50% → half the axis value applied"), then glyphs are re-shaped with the new variation coordinates; spacing compensation per More Options > Variable Font Spacing (AE-TXT-051).
  - Static per-character axis values can also be set in the Properties panel Variable Font pop-up (stored in the TextDocument run, see AE-TXT-084).
  - Scripting: `animatorProps.addVariableFontAxis("wght")`; `fontObject.designAxesData` lists axes `{name, tag, minValue, maxValue, defaultValue}`.
  - Version: UNVERIFIED AE 26.0 (2026) for axis animators.
- **Priority:** P2
- **Owner:** bae-text
- **Notes:** Implementation hint: skrifa supports variation coordinates; cache outlines keyed by quantized axis values (e.g. 1/16 unit) so animated weight does not re-outline every frame.

## Selectors

### AE-TXT-069 · Selector stack and Mode combination (选择器模式)

- **What it does:** Combines the selectors of one animator, top to bottom, into each unit's selection value.
- **Exact semantics:**
  - Mode (模式) menu, order and scripting values: Add (相加) / Subtract (相减) / Intersect (相交) / Min (最小值) / Max (最大值) / Difference (差值). Defaults: Range Selector Add, Wiggly Selector Intersect, Expression Selector UNVERIFIED (Intersect).
  ```
  // per character i, per component d in {x,y,z}; s_k = selector k's value in [-1,1] (amount already applied)
  v = (no selectors) ? 1 : start(mode of first selector)
  start(Add | Max | Difference) = 0;  start(Subtract | Intersect | Min) = 1        // UNVERIFIED (mask-mode analogy; help: a single Subtract selector "inverts the influence")
  for k in selectors (top to bottom):
     Add:        v = v + s_k
     Subtract:   v = v - s_k
     Intersect:  v = v * s_k
     Min:        v = min(v, s_k)
     Max:        v = max(v, s_k)
     Difference: v = |v - s_k|                                      // UNVERIFIED
     v = clamp(v, -1, 1)                                             // Skottie clamps Add to [-1,1]
  m_i = v
  ```
  - Only Add is verified by a reference implementation (Skottie); lottie-web ignores modes and multiplies by Amount only. All other formulas are UNVERIFIED and need AE measurements.
  - Each selector's own value is a 3-vector: Range selectors give the same value in all components; Wiggly (Lock Dimensions off) and Expression selectors can differ per component.
- **Priority:** P0 (Add, single selector); P1 (other modes, multiple selectors)
- **Owner:** bae-text
- **Notes:** matchName of Mode `ADBE Text Selector Mode`. Lottie `t.a[].s.m` (Skottie maps only 1 = Add). Help example: wiggle one word = Range selector on the word + Wiggly selector in Intersect mode.

### AE-TXT-070 · Range Selector parameters (范围选择器)

- **What it does:** Selects a contiguous range of units (characters, words, lines) with a shaped falloff, amount and easing.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | matchName | Lottie |
  |---|---|---|---|---|---|---|
  | Start (起始) | 1D | 0% (Index: 0) | 0 – 100 % (Index: 0 – N, UNVERIFIED clamp) | % or units | `ADBE Text Percent Start` / `ADBE Text Index Start` | `s` |
  | End (结束) | 1D | 100% (Index: N = unit count) | 0 – 100 % | % or units | `ADBE Text Percent End` / `ADBE Text Index End` | `e` |
  | Offset (偏移) | 1D | 0% | -100 – 100 % (Index: -N – N) | % or units | `ADBE Text Percent Offset` / `ADBE Text Index Offset` | `o` |
  | Advanced (高级) group: | | | | | `ADBE Text Range Advanced` | |
  | Units (单位) | enum | Percentage (百分比) | Percentage / Index (索引) | — | `ADBE Text Range Units` | `r`: 1 / 2 |
  | Based On (依据) | enum | Characters (字符) | Characters / Characters Excluding Spaces (不包含空格的字符) / Words (词) / Lines (行) | — | UNVERIFIED `ADBE Text Range Type2` | `b`: 1 / 2 / 3 / 4 |
  | Mode (模式) | enum | Add | AE-TXT-069 | — | `ADBE Text Selector Mode` | `m` |
  | Amount (数量) | 1D | 100% | -100 – 100 | % | `ADBE Text Selector Max Amount` | `a` |
  | Shape (形状) | enum | Square (正方形) | Square / Ramp Up (上斜坡) / Ramp Down (下斜坡) / Triangle (三角形) / Round (圆形) / Smooth (平滑) | — | UNVERIFIED `ADBE Text Range Shape` | `sh`: 1..6 |
  | Smoothness (平滑度) | 1D | 100% | 0 – 100 | % | `ADBE Text Selector Smoothness` | `sm` (Square only) |
  | Ease High (缓和高) | 1D | 0% | -100 – 100 | % | `ADBE Text Levels Max Ease` | `xe` |
  | Ease Low (缓和低) | 1D | 0% | -100 – 100 | % | `ADBE Text Levels Min Ease` | `ne` |
  | Randomize Order (随机排序) | bool | Off | — | — | UNVERIFIED `ADBE Text Randomize Order` | `rn` |
  | Random Seed (随机植入) | integer | 0 | UNVERIFIED: -10000 – 10000 | — | `ADBE Text Random Seed` | — |

  ```
  N = unit count (AE-TXT-071)
  Percentage: s = (Start + Offset)/100 * N ; e = (End + Offset)/100 * N
  Index:      s = Start + Offset         ; e = End + Offset
  if s > e: swap(s, e)          // Start may exceed End
  Offset never wraps: units pushed outside [0, N] are simply unselected (or fully selected when the range covers them)
  selector value for unit i:  sel_i = Amount/100 * F_shape(i; s, e, Smoothness, EaseHigh, EaseLow)   (AE-TXT-072, AE-TXT-073)
  ```
  - Switching Units from Percentage to Index converts Start/End/Offset to unit counts (End becomes N), UNVERIFIED whether values are converted or reset.
  - Viewer: when the selector is selected, Start/End selector bars appear on the text; dragging them changes Start/End; Shift-click a bar with the Selection tool to change Offset (UI).
  - With characters selected in the viewer when the animator is created, Start/End are set to cover them.
- **Priority:** P0
- **Owner:** bae-text (+ bae-model)
- **Notes:** Lottie `t.a[].s` with `t: 0` (range selector; `t: 1` = expression selector in bodymovin). lottie-web reads `a` with multiplier 0.01. Range selector is matchName `ADBE Text Selector`.

### AE-TXT-071 · Selector units: Based On counting (依据)

- **What it does:** Defines what units a selector indexes and which unit each character belongs to.
- **Exact semantics:**
  ```
  Characters:                    every character (incl. spaces); line-break characters excluded (lottie-web)
  Characters Excluding Spaces:   every non-space character; spaces and line breaks are not counted; UNVERIFIED which unit value they take (lottie-web: they keep the running index without incrementing)
  Words:                         unit = word; a word = maximal run of non-space characters; the space(s) after a word belong to that word's unit (lottie-web increments after the space)
  Lines:                         unit = composed line (wrapped lines count separately)
  N = number of units; unit index of character c = u(c) ∈ [0, N-1]; all characters of a unit share the unit's selector value
  ```
  - Index Units count in these units ("show in number of characters including spaces" when Based On = Characters).
  - Help note: with Characters, spaces are counted so the animation "pauses" between words.
  - CJK: no spaces, so Words treats a whole CJK sentence as one word (UNVERIFIED; AE does not do dictionary word segmentation). BAE decision: follow AE (space-delimited) for fidelity; optionally offer UAX #29 word segmentation as a BAE extension.
- **Priority:** P0 (Characters); P1 (others)
- **Owner:** bae-text
- **Notes:** Expression selector `textIndex` is 1-based in these units (AE-TXT-076).

### AE-TXT-072 · Range Selector shape functions (范围选择器形状)

- **What it does:** Exact selection value per unit for each Shape.
- **Exact semantics:**
  ```
  // i = unit index (0-based), unit cell = [i, i+1), centre c = i + 0.5; s,e from AE-TXT-070; E() = ease curve (AE-TXT-073)
  Square:     cov = overlap length of [i, i+1) with [s, e], clamped to [0,1]   // partial coverage at fractional s/e
              (lottie-web: if i >= floor(s): cov = clamp(i < s ? min(e,1)-(s-i) : e-i, 0, 1) ... equivalent for e >= s+1)
              cov = E(cov); then Smoothness (below)
  Ramp Up:    e == s ? (i >= e ? 1 : 0) : clamp((c - s)/(e - s), 0, 1);  then E()
  Ramp Down:  e == s ? (i >= e ? 0 : 1) : 1 - clamp((c - s)/(e - s), 0, 1);  then E()
  Triangle:   e == s ? 0 : u = clamp((c - s)/(e - s), 0, 1); tri = u < 0.5 ? 2u : 2 - 2u;  then E()
  Round:      e == s ? 0 : x = clamp(c - s, 0, e - s) - (e - s)/2; a = (e - s)/2; v = sqrt(1 - x²/a²);  then E()
  Smooth:     e == s ? 0 : u = clamp(c - s, 0, e - s)/(e - s); v = (1 + cos(π + 2π·u))/2  (= sin²(π·u));  then E()
  Note Ramp Up keeps value 1 for all units after e, Ramp Down keeps 1 for all units before s.
  Smoothness (Square only, sm = Smoothness/100):
     sm == 1: unchanged (linear partial coverage)
     else: thr = 0.5 - sm/2;  v = v < thr ? 0 : min((v - thr)/max(sm, 1e-8), 1)
     (sm = 0: hard step — a unit is selected iff at least half its cell is inside [s,e])
  ```
  - These are lottie-web's formulas (reference reproducing AE). Skottie uses cubic approximations for Round (`(0,.5),(.5,1)`) and Smooth (`(.5,0),(.5,1)`) and implements Smoothness by widening the range edges by `sm/2` units with a linear ramp; both agree at Smoothness 100% and 0%. UNVERIFIED which of the two matches AE at intermediate smoothness; BAE decision: use lottie-web formulas and add AE reference tests.
- **Priority:** P0 (Square, Ramp Up/Down); P1 (Triangle, Round, Smooth, Smoothness < 100)
- **Owner:** bae-text
- **Notes:** Shape names in Lottie enum: 1 Square, 2 Ramp Up, 3 Ramp Down, 4 Triangle, 5 Round, 6 Smooth.

### AE-TXT-073 · Ease High and Ease Low (缓和高 / 缓和低)

- **What it does:** Reshapes the 0→1 transition of the selection curve.
- **Exact semantics:**
  ```
  lo = EaseLow/100, hi = EaseHigh/100   (each clamped to [-1, 1])
  P1 = lo > 0 ? (lo, 0) : (0, -lo)
  P2 = hi > 0 ? (1 - hi, 1) : (1, 1 + hi)
  E(v) = y of the cubic Bézier (0,0) P1 P2 (1,1) at the parameter where x = v   (CSS cubic-bezier semantics)
  E is applied to the raw shape value before Smoothness and before Amount.
  ```
  - Ease Low 100%: gradual change near unselected (value near 0 grows slowly); -100%: fast change near 0. Ease High 100%: gradual near fully selected; -100%: fast near 1 (matches the help text).
  - Both 0: E(v) = v (linear).
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** Identical in lottie-web (`BezierFactory.getBezierEasing(x1,y1,x2,y2)`) and Skottie (`EaseVec`). Implementation hint: precompute a 256-entry LUT per (lo, hi) per frame.

### AE-TXT-074 · Randomize Order and Random Seed (随机排序 / 随机植入)

- **What it does:** Applies the range selection to units in a pseudo-random order.
- **Exact semantics:**
  ```
  perm = seeded permutation of [0, N)  // AE algorithm proprietary
  value(unit i) = F_shape(perm[i])     // the shape is evaluated at the permuted position
  seed = RandomSeed != 0 ? RandomSeed : f(animator group identity)   // seed 0 = derived from the animator group, so duplicated animators randomize differently
  ```
  - The permutation is stable over time (does not change per frame) and depends on N (changing the text reshuffles).
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** lottie-web uses `Math.random()` (non-deterministic) and Skottie a fixed mt19937 seed 42, so neither reproduces AE's order. BAE decision: Fisher–Yates with a PCG32 seeded by `(seed, N)` (seed 0 → stable hash of the animator's persistent ID); document that orders differ from AE. Random Seed matchName `ADBE Text Random Seed`.

### AE-TXT-075 · Wiggly Selector (摆动选择器)

- **What it does:** Randomly varies the selection of each unit over time.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Mode (模式) | enum | Intersect (相交) | AE-TXT-069 | — |
  | Max Amount (最大量) | 1D | 50% | -100 – 100 | % |
  | Min Amount (最小量) | 1D | -50% | -100 – 100 | % |
  | Based On (依据) | enum | Characters | as AE-TXT-071 | — |
  | Wiggles/Second (摇摆/秒) | 1D | 2.0 | 0 – UNVERIFIED max | Hz |
  | Correlation (关联) | 1D | 50% | 0 – 100 | % |
  | Temporal Phase (时间相位) | angle | 0x+0° | unbounded | revolutions + degrees |
  | Spatial Phase (空间相位) | angle | 0x+0° | unbounded | revolutions + degrees |
  | Lock Dimensions (锁定维度) | bool | Off | — | — |
  | Random Seed (随机植入) | integer | 0 | — | — |

  ```
  // UNVERIFIED model (AE's noise is proprietary); smooth 1-D noise n(x) in [-1,1]
  for unit i, component d (d fixed to 0 when Lock Dimensions is on):
     phase_t = time * WigglesPerSecond + TemporalPhase/360
     shared  = n(seed, d, phase_t)
     own     = n(seed, d, i, phase_t + i * SpatialPhase/360)
     w       = Correlation * shared + (1 - Correlation) * own          // 100%: all units identical; 0%: independent
     sel     = Min + (w + 1)/2 * (Max - Min)                            // in [-1,1] after /100
  ```
  - Negative values push properties the other way (e.g. negative scale delta); this is why Min Amount defaults to -50%.
  - Lock Dimensions: the same random value for x, y, z (useful for uniform Scale wiggle).
  - Seed 0: derived from the selector/animator (as Range randomize).
- **Priority:** P1
- **Owner:** bae-text (+ bae-anim)
- **Notes:** matchName `ADBE Text Wiggly Selector`; child matchNames UNVERIFIED (`ADBE Text Wiggly Max Amount`, `ADBE Text Wiggly Min Amount`, `ADBE Text Range Type2`, `ADBE Text Temporal Freq`, `ADBE Text Character Correlation`, `ADBE Text Temporal Phase`, `ADBE Text Spatial Phase`, `ADBE Text Wiggly Lock Dim`, `ADBE Text Wiggly Random Seed`). Not exported by bodymovin. BAE decision: reuse the expression `wiggle()` noise generator (transform-keyframes-expressions.md, wiggle) so wiggly text and wiggle expressions look alike.

### AE-TXT-076 · Expression Selector (表达式选择器)

- **What it does:** Computes each unit's selection value with an expression evaluated once per unit.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | Based On (依据) | enum | Characters | as AE-TXT-071 |
  | Amount (数量) | 3D (% per component) | expression `selectorValue * textIndex/textTotal` | the value is meaningful only through its expression |
  | Mode (模式) | enum | UNVERIFIED: Intersect | AE-TXT-069 |

  - Expression context adds: `textIndex` (1-based index of the current unit in Based On units), `textTotal` (N), `selectorValue` (3-vector, 0–100 scale: the combined value of the selectors above this one in the same animator; [100,100,100] when there is none, UNVERIFIED). These identifiers are errors outside expression selectors.
  - Result: number or array; a number applies to all components; values are percentages (100 = fully selected), negative allowed, clamped to [-100, 100] (UNVERIFIED).
  - Evaluated per unit per frame: cost = N × expression cost; time-dependent functions (`wiggle`, `time`) are allowed; `random()` uses a per-unit seed so `seedRandom(textIndex)`-style patterns work (see transform-keyframes-expressions.md, Random).
- **Priority:** P1
- **Owner:** bae-anim (+ bae-text)
- **Notes:** matchName `ADBE Text Expressible Selector`, amount UNVERIFIED `ADBE Text Expressible Amount`. Lottie: selector `t: 1` with expression in `a.x`; lottie-web evaluates it per character (`TextSelectorPropertyDecorator`). Implementation hint: compile the expression once and call it N times with a mutable context object; detect `textIndex`-only expressions without time dependence and cache per text change.

## Conversion commands and presets

### AE-TXT-077 · Create Shapes from Text (从文字创建形状)

- **What it does:** Converts the outlines of a text layer's characters (or the selected characters) into a new shape layer with editable paths, fills and strokes.
- **Exact semantics:**
  - Invocation: Layer > Create > Create Shapes from Text (图层 > 创建 > 从文字创建形状), or context menu on the layer/text. Scope: whole layer when the layer is selected; only the selected characters when text is selected in the viewer. Multiple selected text layers are converted one by one.
  - Output layer: a new shape layer named `<text layer name> Outlines` (轮廓, UNVERIFIED zh suffix) placed at the top of the layer stack (help) — UNVERIFIED: AE 2026 places it directly above the source layer; same in/out points, parent, 3D switch, blend mode and labels copied (UNVERIFIED for label/parent).
  - Contents: one shape group per converted character, named after the character (e.g. `T`, `e`), in text order; spaces produce no group. Each group: one `Path n` per glyph contour; if the glyph has more than one contour (e.g. "i", "e", "o"), the paths are followed by a Merge Paths operator (mode Merge, UNVERIFIED) so holes cut correctly; then Fill (and Stroke if the character has one) matching the character's colour, stroke width, line join and paint order (stroke above fill → Stroke placed before Fill in the group, UNVERIFIED). Fill rule Non-Zero (UNVERIFIED).
  - Group transform: UNVERIFIED — each group's anchor/position is set at the character's position so per-character animation is possible (AE 2020+ behaviour); path vertices are in the shape layer's space.
  - Geometry is the static layout at the current time including Source Text value at that time; text animators are NOT baked (UNVERIFIED), Path Options placement IS baked (UNVERIFIED).
  - The source text layer's Video switch (eye) is turned off. Effects, masks, layer styles, and Transform keyframes/expressions are copied to the new layer.
  - Undoable as one step.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-text, bae-vector)
- **Notes:** Shape layer structure: shapes.md (Shape layer structure, Merge Paths). Implementation hint: reuse the cached per-glyph outlines from `bae-text` (skrifa) converted to kurbo `BezPath` → shape `Path` cubic vertices (quadratic TrueType curves elevated to cubic exactly).

### AE-TXT-078 · Create Masks from Text (从文字创建蒙版)

- **What it does:** Converts the outlines of a text layer's characters into masks on a new solid layer.
- **Exact semantics:**
  - Invocation: Layer > Create > Create Masks from Text (从文字创建蒙版) or the context menu; scope as AE-TXT-077.
  - Output: a new comp-sized solid layer named `<text layer name> Outlines` (UNVERIFIED), solid colour UNVERIFIED (AE uses the text fill colour? BAE decision: text fill colour of the first character), at the top of the layer stack, with one mask per glyph contour named after the character (UNVERIFIED naming); outer contours mode Add, inner contours (holes of compound glyphs such as "i", "e") mode Subtract (help).
  - Masks are in the solid's layer space at the positions the glyphs occupied; the solid's Transform copies the text layer's Transform keyframes/expressions; effects, masks and layer styles are copied; the text layer's Video switch is turned off.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-text)
- **Notes:** Mask modes and data: compositing-3d-color.md (Masks). Commonly used to drive Stroke / Scribble effects from text.

### AE-TXT-079 · Convert to Editable Text (转换为可编辑文字)

- **What it does:** Converts an imported Photoshop text layer into a native editable AE text layer.
- **Exact semantics:**
  - Applies to a layer whose source is a single PSD layer containing type (imported as Composition / layers); merged PSD footage must first be decomposed (Layer > Create > Convert to Layered Comp).
  - Layer > Create > Convert to Editable Text: reads the Photoshop type engine data (text, fonts, sizes, colours, tracking, leading, paragraph settings, point/box, warp ignored UNVERIFIED) and replaces the layer by a text layer with equivalent TextDocument; the layer no longer uses the PSD footage. Layer styles are converted to editable layer styles first (as Layer > Layer Styles > Convert to Editable Styles).
- **Priority:** P2
- **Owner:** bae-media (+ bae-text, bae-motion)
- **Notes:** Depends on PSD import (project-comp-layers.md, Import). BAE decision: parse PSD `TySh` (type tool) layer resources; unsupported features (warp, text on path in PS) produce a warning.

### AE-TXT-080 · Text animation presets (文本动画预设)

- **What it does:** Ready-made animator setups (and Source Text / effect setups) applied from Effects & Presets > Animation Presets > Text.
- **Exact semantics:**
  - Categories shipped with AE (folder names): 3D Text, Animate In, Animate Out, Blurs, Curves and Spins, Expressions, Fill and Stroke, Graphical, Lights and Optical, Mechanical, Miscellaneous, Multi-Line, Organic, Paths, Rotation, Scale, Tracking (UNVERIFIED complete list).
  - Applying a text preset adds its animators/selectors/keyframes (keyframe times relative to the CTI) to the selected text layers. Presets were authored in a 720×480 comp with 72-pt Myriad Pro, so position values may need adjusting; Paths presets replace the Source Text with the preset name and set the fill to white; Fill and Stroke presets need an existing fill/stroke.
  - AE presets are `.ffx` (proprietary).
- **Priority:** P2
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** BAE decision: do not read `.ffx`; ship BAE's own preset library (JSON of Commands) recreating the most used text presets (Typewriter, Fade Up Characters, Tracking In, etc.) with original parameter values. Generic preset mechanics: effects.md (effect framework) and ui-preview-render-io.md (Effects & Presets panel).

## Fonts

### AE-TXT-081 · Font enumeration (字体列表)

- **What it does:** Builds the list of available fonts shown in the Character panel and used to resolve TextDocument fonts.
- **Exact semantics:**
  - Sources: OS-installed fonts (system and user folders), Adobe-private font folders (Win `C:\Program Files\Common Files\Adobe\Fonts`, Mac `/Library/Application Support/Adobe/Fonts`; fonts there are visible only to Adobe apps), activated Adobe Fonts (P2), and fonts in a project's "Fonts" folder when opened from a package (AE 23.x+ "Collect Files" font option, UNVERIFIED version) — see AE-TXT-086.
  - Formats: OpenType (CFF and TrueType outlines), TrueType, TrueType Collections (.ttc), variable fonts, OpenType-SVG colour fonts; Type 1 support was removed by Adobe (AE 24.x? UNVERIFIED version; Adobe ended Type 1 support in 2023).
  - Each enumerated face exposes: PostScript name, family, style, full name, native (localized) family/style names, technology/type, writing scripts, version, file location, variable axes (`designAxesData`), `isFromAdobeFonts`.
  - Font menu presentation (UI): families with style submenus, favourites (star), recent fonts (count set in Preferences > Type), "Show Font Names in English" option for CJK names, filter for variable fonts, filter for Adobe Fonts, format suffix when duplicates exist.
  - The list refreshes when fonts are installed/removed while AE runs (`fontServerRevision` increments).
- **Priority:** P0
- **Owner:** bae-text (+ bae-ui-motion)
- **Notes:** Scripting `app.fonts` (FontsObject, AE 24.0+): `allFonts` (arrays grouped by family), `getFontsByPostScriptName`, `getFontsByFamilyNameAndStyleName`, `getFontByID` (24.2), `fontsDuplicateByPostScriptName` (24.6), `favoriteFontFamilyList`, `mruFontFamilyList` (24.6), `getDefaultFontForCTScript` / `setDefaultFontForCTScript` and `getCTScriptForString` (25.1). Implementation hint: fontique for enumeration; build the index off the UI thread at startup and cache it on disk keyed by font file mtimes (cold start ≤ 2 s target).

### AE-TXT-082 · Font matching on project open (字体匹配)

- **What it does:** Resolves each persisted font reference to an installed font.
- **Exact semantics:**
  ```
  candidates = fonts with postScriptName == stored PostScript name
  if candidates non-empty: use candidates[0] (enumeration order), preferring the one whose family/style/technology also match (UNVERIFIED preference)
  else if a variable font has a PostScript-name prefix match: create the named/custom instance from it (AE behaviour for variable fonts)
  else: create a substitute (missing) font proxy (AE-TXT-083)
  ```
  - Persisted per run: PostScript name, family name, style name, (technology, writing script — used by the CTFI_EQUAL policy), variable design vector.
  - Replacement policy for substitutes (`app.fonts.substitutedFontReplacementMatchPolicy`, AE 24.6): `POSTSCRIPT_NAME` (default: same PostScript name is enough), `CTFI_EQUAL` (PostScript name, technology and primary writing script must match), `DISABLED`.
- **Priority:** P0
- **Owner:** bae-text (+ bae-model)
- **Notes:** Lottie fonts are matched by `fName`/`fFamily`/`fStyle` (`fonts.list`), and `fPath`/`origin` for web fonts. BAE decision: also store a font file hash so collaborators with different font versions get a warning.

### AE-TXT-083 · Missing fonts: substitution, Resolve Fonts and Find Missing Fonts (缺失字体 / 解析字体)

- **What it does:** Keeps projects with uninstalled fonts usable and helps the user fix them.
- **Exact semantics:**
  - A missing font is replaced at render time by a substitute "real" font (the default font for the text's script, UNVERIFIED: AE picks the default font, e.g. Myriad/Arial for Roman, a system CJK font for CJK), while the TextDocument keeps the original identity (`fontObject.isSubstitute = true`). The Character panel shows the original name in brackets, e.g. `[Helvetica Neue LT Std]`.
  - On project open, AE shows a warning listing missing fonts and the Resolve Fonts (解析字体) dialog: lists missing fonts, offers to sync available ones from Adobe Fonts (asynchronous; text re-renders when fonts arrive), and lets the user replace fonts (Replace Font / 替换字体, 译名待核) globally.
  - "Missing" (font removed while the project is open) vs "substituted" (already missing at open) are distinguished (`app.fonts.missingOrSubstitutedFonts`). When a matching font is installed later, all references to the substitute are automatically replaced (per the match policy).
  - File > Dependencies > Find Missing Fonts (查找缺失的字体, 译名待核) selects the layers that use missing fonts in the Project/Timeline.
  - Substitution never rewrites the stored PostScript name unless the user replaces the font.
- **Priority:** P0 — projects move between Mac/Windows and machines constantly.
- **Owner:** bae-text (+ bae-ui-motion, bae-model)
- **Notes:** Help: "Font substitution sometimes occurs when you open a project on macOS that was created on Windows". BAE decision: show missing fonts in a non-modal panel (do not block open); Agent API exposes the missing-font list. Adobe Fonts sync is P2 (requires Adobe account) — BAE may instead offer matching free fonts.

### AE-TXT-084 · Variable fonts (可变字体)

- **What it does:** Uses OpenType variable fonts with named instances and arbitrary axis values.
- **Exact semantics:**
  - Named instances (e.g. "Bold", "Condensed Light") appear as styles in the font menu; selecting one stores its design vector.
  - Custom axis values per character (Properties panel > Text > Variable Font pop-up): each axis shows name, current value, min/max from the font (input clamped). With text selected the values apply to the selection; otherwise only to the first character (help: "if you do not select a specific character, the values apply to the first character only"). Changing axes re-lays out the text (widths change).
  - Persistence: per run design vector `[v_axis0, ...]` in `fvar` axis order (`fontObject.designVector`); AE may create a "custom instance" Font object for non-named vectors (`app.fonts.fontsWithDefaultDesignAxes`, `getFontsByPostScriptName` creates instances on demand).
  - Animation: variable-font axis animators (AE-TXT-068) and Variable Font Spacing (AE-TXT-051).
  - Versions: variable font selection in AE 2020 (17.x, UNVERIFIED); per-character axes and axis animators AE 26.0 (2026, UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** Scripting: `Font.hasDesignAxes`, `designAxesData` (`{name, tag, minValue, maxValue, defaultValue}`), `designVector`. Implementation hint: skrifa `Location` from user coordinates (apply `avar`).

### AE-TXT-085 · Colour and emoji fonts (彩色字体 / 表情符号字体)

- **What it does:** Renders multi-colour glyphs from colour font formats.
- **Exact semantics:**
  - AE supports OpenType-SVG colour fonts (e.g. Trajan Color, EmojiOne/Noto Color Emoji SVG) since AE CC 2018 (15.0, UNVERIFIED); `Font.technology = CT_SVG_FONT`. Colour glyphs use their embedded colours: the character's Fill Color is ignored for coloured parts (UNVERIFIED: AE applies fill to `currentColor` parts only), stroke is applied to the glyph outline fallback (UNVERIFIED).
  - Other colour formats (COLRv0/v1, sbix bitmap as in Apple Color Emoji, CBDT): UNVERIFIED in AE; sbix/CBDT likely render via outlines fallback or not at all.
  - Emoji sequences (ZWJ, skin-tone modifiers, flags) form one animated character (AE-TXT-012).
  - Animators: transforms and opacity apply to colour glyphs; Fill/Stroke Color animators do not recolour embedded colours (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-text (+ bae-render)
- **Notes:** BAE decision: support COLRv0/v1 and OpenType-SVG via skrifa/vello (COLR) with an SVG glyph path through the SVG importer; bitmap emoji (sbix/CBDT) drawn as images, scaled with the glyph matrix. Apple Color Emoji (sbix) is the default emoji font on macOS, so Chinese/English users typing emoji expect it to render.

### AE-TXT-086 · Fonts in packaging and Collect Files (字体打包)

- **What it does:** Carries the fonts a project uses with the project so it opens identically elsewhere.
- **Exact semantics:**
  - AE File > Dependencies > Collect Files can copy used fonts into a `Fonts` folder next to the collected project ("Collect Fonts" option, Adobe Fonts excluded; UNVERIFIED version AE 23.x); when a project is opened, fonts in that folder are activated for the project (UNVERIFIED).
  - Only fonts actually referenced by text layers (all Source Text keyframes, all runs, and fonts used via expressions? — not detectable, UNVERIFIED) are collected; licensing is the user's responsibility.
- **Priority:** P1
- **Owner:** bae-package (+ bae-text)
- **Notes:** Collect Files itself: project-comp-layers.md (Collect Files/Reduce Project). BAE `.baez` packaging: ARCHITECTURE.md §16 — fonts are embedded as subsetted or full files with a manifest of PostScript names and hashes; `bae-text` must load fonts from the package before falling back to system fonts.

## CJK, vertical and complex scripts

### AE-TXT-087 · IME text input (输入法输入)

- **What it does:** Lets users type Chinese, Japanese and Korean (and other IME-based scripts) directly into text layers.
- **Exact semantics:**
  - While editing text, platform IME composition is shown in-line at the caret (preedit text underlined, candidate window positioned next to the caret in screen space, following viewer zoom and layer transform); committing inserts the composed string as one edit with the current insertion style.
  - Composition text is not yet part of the TextDocument (no Source Text change, no undo step) until committed; Esc cancels the composition, not the edit session.
  - Works for point, paragraph, vertical and path text; caret movement and selection operate on grapheme clusters.
- **Priority:** P1 — Chinese users type text through an IME daily.
- **Owner:** bae-ui-motion (+ bae-text, bae-ui)
- **Notes:** AE itself supports IME through the OS; AE's in-line display quality varies by platform (UNVERIFIED). Implementation hint: winit IME events (`Ime::Preedit`/`Commit`) → egui text edit adapter; set the IME cursor area from the caret's comp-space rectangle each frame.

### AE-TXT-088 · CJK line breaking and kinsoku (中日韩换行与避头尾)

- **What it does:** Breaks CJK text between any characters except where punctuation rules forbid it.
- **Exact semantics:**
  - Break opportunities exist between consecutive ideographs, kana, Hangul syllables (Korean also breaks at spaces), and between CJK and Latin runs (UAX #14 classes ID, CJ, H2/H3, AL...).
  - Kinsoku shori (避头尾): characters that may not start a line (closing brackets and quotes `）」』】〕〉》”’`, `、。，．：；？！`, small kana, prolonged sound mark `ー`, iteration marks) and characters that may not end a line (opening brackets/quotes `（「『【〔〈《“‘`) are kept with their neighbours by moving the break earlier (push-in/push-out policy UNVERIFIED; AE exposes no kinsoku settings UI, so the built-in "hard"/"soft" set is UNVERIFIED).
  - Chinese full-width punctuation at line end is not compressed by default (mojikumi not exposed in AE, UNVERIFIED).
  - Latin words inside CJK text do not break except at spaces/hyphens.
- **Priority:** P1 — correct breaking of Chinese paragraph text is a daily need.
- **Owner:** bae-text
- **Notes:** BAE decision: UAX #14 with the "strict" line-break style for zh/ja plus a kinsoku table (JIS X 4051 hard set) applied by parley's line breaker; no user-editable kinsoku sets in M1. Also see AE-TXT-040.

### AE-TXT-089 · Mojikumi and East Asian spacing (标点挤压, 译名待核)

- **What it does:** In Adobe CJK composition, controls spacing between CJK punctuation, ideographs and Latin (e.g. half-width punctuation pairs, aki between Japanese and Roman).
- **Exact semantics:**
  - AE exposes no Mojikumi set selection in the Paragraph panel (unlike InDesign/Photoshop); the Latin/CJK composer applies a default set (UNVERIFIED). Tsume (AE-TXT-026) and tracking are the only user controls.
  - Default behaviour BAE must reproduce: full-width punctuation keeps its full em advance; no automatic space between CJK and Latin characters (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-text
- **Notes:** BAE decision: no mojikumi engine in M1; optional "punctuation squeeze" (标点挤压) later as a BAE extension, off by default for AE fidelity.

### AE-TXT-090 · Vertical text (直排文字)

- **What it does:** Lays text out top-to-bottom in columns that progress right to left.
- **Exact semantics:**
  - Created with the Vertical Type Tool (直排文字工具) (or converted: select the layer with a type tool, context menu > Horizontal / Vertical (水平 / 垂直); not available while editing).
  - `lineOrientation` (AE 24.2): `HORIZONTAL` / `VERTICAL_RIGHT_TO_LEFT` (standard; columns right→left) / `VERTICAL_LEFT_TO_RIGHT` (UNVERIFIED: used for Mongolian).
  - Layout: CJK glyphs upright using vertical metrics (`vhea`/`vmtx`, `VORG`) and vertical alternates (`vert`/`vrt2` features: rotated brackets, punctuation repositioned); Latin and other horizontal-script runs rotated 90° clockwise ("rotated" baseline direction, the default `BASELINE_VERTICAL_ROTATED` for Latin, UNVERIFIED) unless set upright with Standard Vertical Roman Alignment (标准垂直罗马对齐方式, 译名待核, Character panel menu) which corresponds to `BASELINE_WITH_STREAM` (UNVERIFIED mapping).
  - Baseline of vertical text = centre axis of the characters (the type-tool pointer's line marks it). Leading advances columns to the left; paragraph alignment Left/Center/Right maps to Top/Center/Bottom; indents left/right map to top/bottom.
  - Paragraph box: columns wrap at the box height; overflow at the left edge.
  - Animators: Position x/y stay in layer axes (not rotated with the flow, UNVERIFIED); Tracking adds space along the column.
- **Priority:** P2 — vertical text is occasional for Chinese users.
- **Owner:** bae-text
- **Notes:** Scripting `lineOrientation` (24.2), `baselineDirection` (24.0). Not in Lottie.

### AE-TXT-091 · Tate-chu-yoko (直排内横排)

- **What it does:** Sets a short run of horizontal characters (e.g. 2-digit numbers) upright as one block within a vertical line.
- **Exact semantics:**
  - Character panel menu > Tate-Chuu-Yoko toggles it for the selected characters; scripting `baselineDirection = BASELINE_VERTICAL_CROSS_STREAM` (AE 24.0).
  - The run is laid out horizontally, centred on the column axis, occupying one em of column advance (compressed horizontally if wider than 1 em, UNVERIFIED: AE scales it to fit; Adobe calls this kumimoji / renmoji).
  - The run remains editable and stylable; for animators each character remains a separate unit (UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-text
- **Notes:** Only meaningful for vertical text; ignored in horizontal text.

### AE-TXT-092 · Right-to-left and complex-script shaping (从右到左与复杂文字)

- **What it does:** Correctly shapes and orders Arabic, Hebrew, Indic (Devanagari etc.), Thai and other complex scripts, mixed with Latin and digits.
- **Exact semantics:**
  - Universal Text Engine (default `composerEngine = UNIVERSAL_TYPE_ENGINE`; formerly the "South Asian and Middle Eastern" engine selected in Preferences > Type; UNVERIFIED version that made it the only engine for new layers: AE 24.0): full OpenType shaping (GSUB/GPOS: joining forms, mark positioning, reordering, conjuncts) for all scripts in one layer, no preference switch needed.
  - Bidi: Unicode Bidirectional Algorithm with the paragraph direction (AE-TXT-033) as base level; numbers and Latin inside RTL keep LTR order.
  - Per-character animation: units are the logical characters (clusters), animated "based on their direction": index 0 is the first logical character (rightmost in RTL), so a Range Selector sweep reveals RTL text from the right.
  - Digit substitution via `digitSet` (AE-TXT-027).
  - Old projects with `LATIN_CJK_ENGINE` keep their layout; scripting can upgrade them to Universal but not downgrade.
- **Priority:** P1
- **Owner:** bae-text
- **Notes:** BAE decision: parley (HarfRust shaping + ICU4X bidi) gives the Universal behaviour; there is no Latin/CJK-only engine mode in BAE — projects imported with `LATIN_CJK_ENGINE` render with the universal shaper and a note in the import report.

### AE-TXT-093 · Default font per script (各文字系统默认字体)

- **What it does:** Chooses the font used for new text and for substitution per writing script.
- **Exact semantics:**
  - AE 25.1: `app.fonts.getDefaultFontForCTScript(ctScript)` / `setDefaultFontForCTScript(ctScript, font)` and `getCTScriptForString(str)` (returns ranges `{chars, ctScript}`); Preferences > Type holds the defaults (UNVERIFIED UI location).
  - When the user types characters of a script the current font cannot render, the default font of that script is used (fallback, AE-TXT-045).
  - CTScript values include Roman, Japanese, Traditional Chinese, Simplified Chinese, Korean, Arabic, Hebrew, Greek, Cyrillic, Devanagari ... (see `Font.writingScripts`).
- **Priority:** P2
- **Owner:** bae-text (+ bae-ui-motion)
- **Notes:** BAE decision: preference "Default font for Chinese (Simplified)" pre-set to the OS default CJK UI font (PingFang SC / Microsoft YaHei / Noto Sans CJK SC).

## Other text behaviour

### AE-TXT-094 · Motion blur of animated characters (文字动画运动模糊)

- **What it does:** With layer motion blur enabled, per-character animator motion (position, rotation, scale) is blurred, not only the layer transform.
- **Exact semantics:**
  - Each motion-blur sample time re-evaluates animators/selectors and the per-character matrices at that sub-frame time; samples are accumulated per the comp's Shutter Angle/Phase and Samples per Frame (see compositing-3d-color.md, Motion blur rendering; transform-keyframes-expressions.md, motion-blur sampling).
  - Source Text hold keys switch at their key time within the shutter interval (no blending of different texts besides sample accumulation).
- **Priority:** P1
- **Owner:** bae-render (+ bae-text, bae-anim)
- **Notes:** Help: "To smooth the edges and movement of animated text, enable motion blur for the text layer." Implementation hint: re-use the shaped glyph cache across samples; only matrices and colours change per sample.

### AE-TXT-095 · Extruded 3D text (Advanced 3D / Cinema 4D renderer) (3D 文本凸出, 译名待核)

- **What it does:** With the Advanced 3D (or legacy Cinema 4D) renderer, text layers can be extruded and bevelled, with per-surface material animators.
- **Exact semantics:**
  - Geometry Options on a 3D text layer: Bevel Style (None / Angular / Concave / Convex), Bevel Depth, Hole Bevel Depth, Extrusion Depth (px) (see compositing-3d-color.md, Advanced 3D (P2)).
  - Animator properties available in this mode (Animate > Front / Bevel / Side / Back): Color, Hue, Saturation, Brightness, Opacity, Ambient, Diffuse, Specular Intensity, Specular Shininess, Metal, Reflection Intensity, Reflection Sharpness, Reflection Rolloff, Transparency, Transparency Rolloff, Index of Refraction, plus Bevel Depth and Extrusion Depth (matchNames `ADBE 3DText Front RGB` … `ADBE 3DText Extrude Depth`).
- **Priority:** P2
- **Owner:** bae-render (+ bae-text)
- **Notes:** Not in Lottie. BAE decision: defer until BAE has a 3D mesh renderer; keep the data model fields so projects round-trip.

### AE-TXT-096 · Source Text in Essential Graphics / Essential Properties (源文本基本属性)

- **What it does:** Exposes Source Text to Essential Graphics templates and precomp Essential Properties, with optional per-instance font/style overrides.
- **Exact semantics:**
  - Adding Source Text to the Essential Graphics panel creates a text control; its Edit Properties options allow font change, faux styles and font size adjustment by the template user (UNVERIFIED exact option names: "Allow Font Change", "Allow Faux Styles", "Allow Font Size Adjustment").
  - As an Essential Property of a precomp, the instance layer's override replaces the text (and allowed style aspects) inside the nested comp for that instance only.
- **Priority:** P2
- **Owner:** bae-model (+ bae-ui-motion, bae-text)
- **Notes:** Essential Graphics panel UI: ui-preview-render-io.md. Essential Properties data: project-comp-layers.md (precompose / property tree).

## Interchange

### AE-TXT-097 · AE matchNames for text (文本属性匹配名)

- **What it does:** Lists the matchNames BAE must use for import/export, scripting compatibility and AE-plugin reservations.
- **Exact semantics:**

  | Display name (EN) | matchName | Verified |
  |---|---|---|
  | Text layer | `ADBE Text Layer` | yes |
  | Text | `ADBE Text Properties` | yes |
  | Source Text | `ADBE Text Document` | yes |
  | Path Options | `ADBE Text Path Options` | yes |
  | Path (mask menu) | `ADBE Text Path` | UNVERIFIED |
  | Reverse Path / Perpendicular To Path / Force Alignment / First Margin / Last Margin | `ADBE Text Reverse Path` / `ADBE Text Perpendicular To Path` / `ADBE Text Force Align Path` / `ADBE Text First Margin` / `ADBE Text Last Margin` | yes |
  | More Options | `ADBE Text More Options` | yes |
  | Anchor Point Grouping | `ADBE Text Anchor Point Option` | UNVERIFIED |
  | Grouping Alignment | `ADBE Text Anchor Point Align` | yes |
  | Fill & Stroke | `ADBE Text Render Order` | yes (scripting PropertyGroup docs) |
  | Inter-Character Blending | `ADBE Text Character Blend Mode` | UNVERIFIED |
  | Variable Font Spacing | `ADBE Text Variable Font Spacing` | yes |
  | Animators / Animator | `ADBE Text Animators` / `ADBE Text Animator` | yes |
  | Selectors | `ADBE Text Selectors` | yes |
  | Range Selector | `ADBE Text Selector` | yes |
  | Wiggly Selector | `ADBE Text Wiggly Selector` | yes |
  | Expression Selector | `ADBE Text Expressible Selector` | yes |
  | Start / End / Offset (percent) | `ADBE Text Percent Start` / `ADBE Text Percent End` / `ADBE Text Percent Offset` | yes |
  | Start / End / Offset (index) | `ADBE Text Index Start` / `ADBE Text Index End` / `ADBE Text Index Offset` | yes |
  | Advanced / Units / Mode / Amount / Smoothness / Ease High / Ease Low / Random Seed | `ADBE Text Range Advanced` / `ADBE Text Range Units` / `ADBE Text Selector Mode` / `ADBE Text Selector Max Amount` / `ADBE Text Selector Smoothness` / `ADBE Text Levels Max Ease` / `ADBE Text Levels Min Ease` / `ADBE Text Random Seed` | yes |
  | Based On / Shape / Randomize Order | `ADBE Text Range Type2` / `ADBE Text Range Shape` / `ADBE Text Randomize Order` | UNVERIFIED |
  | Properties | `ADBE Text Animator Properties` | yes |
  | Anchor Point / Position / Scale | `ADBE Text Anchor Point 3D` / `ADBE Text Position 3D` / `ADBE Text Scale 3D` | yes |
  | Skew / Skew Axis | `ADBE Text Skew` / `ADBE Text Skew Axis` | yes |
  | X / Y / Z Rotation | `ADBE Text Rotation X` / `ADBE Text Rotation Y` / `ADBE Text Rotation` | yes |
  | Opacity | `ADBE Text Opacity` | yes |
  | Fill Color, Hue, Saturation, Brightness, Opacity | `ADBE Text Fill Color`, `ADBE Text Fill Hue`, `ADBE Text Fill Saturation`, `ADBE Text Fill Brightness`, `ADBE Text Fill Opacity` | yes |
  | Stroke Color, Hue, Saturation, Brightness, Opacity, Width | `ADBE Text Stroke Color`, `ADBE Text Stroke Hue`, `ADBE Text Stroke Saturation`, `ADBE Text Stroke Brightness`, `ADBE Text Stroke Opacity`, `ADBE Text Stroke Width` | yes |
  | Line Anchor / Tracking Type / Tracking Amount | `ADBE Text Line Anchor` / `ADBE Text Track Type` / `ADBE Text Tracking Amount` | yes |
  | Character Value / Character Offset / Character Range | `ADBE Text Character Replace` / `ADBE Text Character Offset` / `ADBE Text Character Change Type` | yes / yes / UNVERIFIED |
  | Line Spacing / Blur | `ADBE Text Line Spacing` / `ADBE Text Blur` | yes |
  | Wiggly children | `ADBE Text Wiggly Max Amount`, `ADBE Text Wiggly Min Amount`, `ADBE Text Temporal Freq`, `ADBE Text Character Correlation`, `ADBE Text Temporal Phase`, `ADBE Text Spatial Phase`, `ADBE Text Wiggly Lock Dim`, `ADBE Text Wiggly Random Seed` | UNVERIFIED |
  | Expression selector Amount | `ADBE Text Expressible Amount` | UNVERIFIED |
  | 3D text material properties | `ADBE 3DText Front RGB` … `ADBE 3DText Extrude Depth` (full list in docsforadobe "Text Layer Match Names") | yes |

- **Priority:** P1
- **Owner:** bae-model (+ bae-plugin)
- **Notes:** Source: docsforadobe Text Layer Match Names (Nov 2025) — rows marked UNVERIFIED are not in that table and must be confirmed by dumping a project with a script (`prop.matchName`) before use.

### AE-TXT-098 · Lottie text layer import/export (Lottie 文本图层)

- **What it does:** Maps BAE text layers to and from the Lottie/bodymovin text layer format.
- **Exact semantics:**
  - Layer `ty: 5`, text data `t`:
    - `t.d.k[]`: Source Text hold keyframes `{t: frame, s: TextDocument}`; document keys: `t` text (`\r` breaks), `f` font name (→ `fonts.list[].fName`), `s` size, `fc` fill [r,g,b] 0–1, `sc` stroke colour, `sw` stroke width, `of` stroke over fill, `lh` line height (px), `ls` baseline shift, `tr` tracking (1/1000 em), `j` justification (0 Left, 1 Right, 2 Center, 3 Justify last left, 4 last right, 5 last center, 6 full), `ca` caps (0 regular, 1 all caps, 2 small caps), `sz` box size [w,h] and `ps` box position [x,y] (paragraph text only). Expression in `t.d.x`.
    - `t.p`: path options `m` (mask index), `f` first margin, `l` last margin, `r` reverse, `a` force alignment, `p` perpendicular.
    - `t.m`: more options `g` anchor point grouping (1 Characters, 2 Word, 3 Line, 4 All), `a` grouping alignment (animatable 2D %).
    - `t.a[]`: animators `{nm, s, a}`; selector `s`: `t` (0 range, 1 expression), `s`/`e`/`o` start/end/offset, `r` units (1 percent, 2 index), `b` based on (1 chars, 2 chars excl. spaces, 3 words, 4 lines), `m` mode, `a` amount, `sh` shape (1..6), `sm` smoothness, `xe` ease high, `ne` ease low, `rn` randomize; properties `a`: `a` anchor, `p` position, `s` scale, `r`/`rx`/`ry`/`rz` rotation, `sk` skew, `sa` skew axis, `o` opacity, `fc`/`fh`/`fs`/`fb`/`fo` fill, `sc`/`sh`/`ss`/`sb`/`so` stroke, `sw` stroke width, `t` tracking, `ls` line spacing, `bl` blur.
  - Fonts: top-level `fonts.list[]` (`fName`, `fFamily`, `fStyle`, `fPath`, `origin`, `ascent`); optional glyph outlines in top-level `chars[]` (`ch`, `fFamily`, `style`, `size`, `w`, `data.shapes`).
  - Export loses: per-character style runs (first style only), multiple selectors (first only), wiggly selectors, kerning/ligature control, faux styles, scales, indents, vertical text, per-character 3D (renderer-dependent). BAE export must warn for each.
  - Import: missing keys take the defaults in this file; `lh` 0 → 1.2 × size (lottie-web).
- **Priority:** P1
- **Owner:** bae-media (+ bae-text, bae-model)
- **Notes:** Lottie export UI and general mapping: ui-preview-render-io.md (Lottie export). Reference implementations: lottie-web `TextProperty.js`, `TextAnimatorProperty.js`, `TextSelectorProperty.js`; Skottie `TextAdapter.cpp`, `RangeSelector.cpp`.
