## Composition item and Composition Settings

### AE-PRJ-300 · Composition item data model (合成)

- **What it does:** A composition is a project item that owns an ordered stack of layers, a time base (frame rate, duration, start timecode), a frame (width, height, pixel aspect) and comp-level render switches. It is both the unit of editing and the unit nested as a precomp layer or rendered by the Render Queue.
- **Exact semantics:** Persisted fields of a `Comp` prim (each field is specified in its own item below; this table is the canonical list for `bae-model`):
  | Field (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | name (合成名称) | string | "Comp 1" (合成 1) | non-empty | — | AE-PRJ-301 |
  | width / height (宽度 / 高度) | int | 1920 / 1080 UNVERIFIED: sticky, see AE-PRJ-303 | 4..30000 | px | AE-PRJ-303 |
  | pixelAspect (像素长宽比) | f64 | 1.0 | 0.01..100.0 | ratio | AE-PRJ-304 |
  | frameRate (帧速率) | f64 | 29.97 UNVERIFIED (sticky) | 1..999 UNVERIFIED (scripting doc says 1..99) | fps | AE-PRJ-305 |
  | dropFrame | bool | true when fps is 29.97/59.94 UNVERIFIED | — | — | AE-PRJ-305 |
  | resolutionFactor (分辨率) | [int; 2] | [1, 1] | 1..99 each | px skip | AE-PRJ-306 |
  | displayStartTime / displayStartFrame (开始时间码 / 开始帧) | time | 0 | -3:00:00:00..23:59:00:00 | s | AE-PRJ-307 |
  | duration (持续时间) | time | 0:00:30:00 UNVERIFIED (sticky) | ≥ 1 frame .. 10800 s | s | AE-PRJ-308 |
  | bgColor (背景颜色) | RGB f32×3 | [0, 0, 0] | 0..1 | — | AE-PRJ-309 |
  | preserveNestedFrameRate | bool | false | — | — | AE-PRJ-311 |
  | preserveNestedResolution | bool | false | — | — | AE-PRJ-312 |
  | shutterAngle / shutterPhase / motionBlurSamplesPerFrame / motionBlurAdaptiveSampleLimit | int | 180 / -90 / 16 / 128 | see AE-PRJ-313 | deg / samples | AE-PRJ-313 |
  | renderer (渲染器) | enum | Classic 3D | see AE-PRJ-314 | — | AE-PRJ-314 |
  | motionBlur, frameBlending, hideShyLayers, draft3d | bool | see AE-PRJ-315 | — | — | AE-PRJ-315 |
  | workAreaStart / workAreaDuration (工作区域) | time | 0 / duration | inside [0, duration] | s | AE-PRJ-323 |
  | time (current time indicator, 当前时间指示器) | time | 0 | [0, duration) | s | persisted per comp |
  | markers (合成标记) | Marker[] | [] | — | — | AE-PRJ-326 |
  | layers (图层) | Layer[] | [] | index 1 = top | — | AE-PRJ-331 onward |
  | comment (注释), label (标签), id | from item base | label Sandstone UNVERIFIED (index 15, see AE-PRJ-393) | — | — | item base fields are owned by prj-a |
- Derived, read-only values: `frameDuration = 1 / frameRate`; `frameAspect = width * pixelAspect / height`; `numLayers`; `activeCamera` (AE-PRJ-315); `hasVideo` = true; `hasAudio` = any layer has audio; `usedIn` = comps that contain a precomp layer of this comp.
- Every field listed here is undoable and is a separate CRDT attribute (not a single blob) so that two users can change, for example, the duration and the background colour at the same time.
- **Priority:** P0 the core document object.
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: `CompItem` (extends `AVItem` → `Item`). Lottie: the root animation object is a comp (`w`, `h`, `fr`, `ip`, `op`, `layers`, `markers`, `mb`, `ddd`); nested comps live in `assets[]` with `id` + `layers`. Lottie has no background colour, no PAR and no start timecode. Time values are stored as flicks (705,600,000 per second) per ARCHITECTURE.md 4.4; the scripting API exposes seconds as f64. Item base fields (name uniqueness rules, folders, comment, label column, item ID) are specified in project-comp-layers.md [prj-a].

### AE-PRJ-301 · Composition Name (合成名称)

- **What it does:** The display name of the comp item, also used as the default source name of any precomp layer that references it.
- **Exact semantics:**
  - New comps are named `Comp N` (zh-CN `合成 N`), where N is the smallest positive integer such that the name is not already used by another comp in the project UNVERIFIED: AE uses "one more than the highest existing Comp N".
  - Names need not be unique; AE allows duplicate comp names (expressions that use `comp("name")` then resolve to the first match in project item order).
  - Comps created from footage take the footage item name (file name without extension for New Comp from Selection, AE-PRJ-317); Precompose uses "Pre-comp N" (预合成 N), AE-PRJ-401.
  - Renaming a comp updates the source name of every precomp layer that uses it; precomp layers with a custom layer name keep their layer name (AE-PRJ-388).
  - Renaming does NOT rewrite expressions that reference the comp by name (`comp("Old")`); these then fail. UNVERIFIED: AE 13.x+ updates expression references on layer renames inside the same comp, not on comp renames.
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** Scripting: `CompItem.name`. Lottie: `nm` on the root and on `assets[i]` is optional. BAE decision: Expressions should reference comps by stable ID internally and display the name, so renames never break links; still accept `comp("name")` from imported AE projects.

### AE-PRJ-302 · Composition Settings presets (合成设置预设)

- **What it does:** A Preset (预设) menu in the Composition Settings dialog fills Width, Height, Pixel Aspect Ratio and Frame Rate in one step; users can save and delete their own presets.
- **Exact semantics:**
  - A preset stores exactly: width, height, pixelAspect, frameRate (and the drop-frame choice for 29.97/59.94 UNVERIFIED). It does NOT store resolution, start timecode/frame, duration, background colour or any Advanced or 3D Renderer tab setting.
  - Choosing a preset sets those four fields; editing any of them afterwards switches the menu to "Custom" (自定义).
  - Save (保存) asks for a name and appends the preset; Delete (删除) removes the selected preset; Alt/Option-click on Save or Delete restores the factory preset list.
  - Main factory presets (AE 2024-2026, order abbreviated; exact list UNVERIFIED):
    | Preset | W×H | PAR | fps |
    |---|---|---|---|
    | Social Media Portrait HD · 1080x1920 · 29.97 fps | 1080×1920 | 1.0 | 29.97 |
    | Social Media Landscape HD · 1920x1080 · 29.97 fps | 1920×1080 | 1.0 | 29.97 |
    | Social Media Square · 1080x1080 · 29.97 fps | 1080×1080 | 1.0 | 29.97 |
    | Social Media Portrait (4:5) UNVERIFIED | 1080×1350 | 1.0 | 29.97 |
    | HDTV 1080 23.976 / 24 / 25 / 29.97 | 1920×1080 | 1.0 | as named |
    | HD · 1920x1080 · 23.976 / 24 / 25 / 29.97 / 50 / 59.94 / 60 fps | 1920×1080 | 1.0 | as named |
    | HDV/HDTV 720 23.976 / 25 / 29.97 | 1280×720 | 1.0 | as named |
    | HDV 1080 25 / 29.97 | 1440×1080 | 1.33 | as named |
    | DVCPRO HD 720 / 1080 variants | 960×720 / 1280×1080 / 1440×1080 | 1.33 / 1.5 | as named |
    | UHD 4K 23.976 / 24 / 25 / 29.97 / 50 / 59.94 / 60 | 3840×2160 | 1.0 | as named |
    | 4K (DCI) 23.976 / 24 / 25 | 4096×2160 | 1.0 | as named |
    | UHD 8K 23.976 / 24 / 25 / 29.97 | 7680×4320 | 1.0 | as named |
    | NTSC DV / NTSC DV Widescreen / NTSC D1 / NTSC D1 Widescreen / NTSC D1 Square Pix | 720×480 / 720×486 / 720×534 | 0.91 / 1.21 / 1.0 | 29.97 |
    | PAL D1/DV / PAL D1/DV Widescreen / PAL D1/DV Square Pix | 720×576 / 768×576 | 1.09 / 1.46 / 1.0 | 25 |
    | Cineon Half / Cineon Full | 1828×1332 / 3656×2664 | 1.0 | 24 |
    | Film (2K) / Film (4K) | 2048×1556 / 4096×3112 | 1.0 | 24 |
    | Web Video, 320 x 240 / 640 x 480 | as named | 1.0 | 29.97 UNVERIFIED |
  - Custom presets are stored in application preferences (not in the project) and do not travel between machines.
- **Priority:** P1 presets are a convenience; the four fields themselves are P0.
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** BAE decision: store user presets in a portable JSON file in the user config directory so they can be shared. The factory list is data, not code. Preset names are not localised in AE's zh-CN UI except "Custom" (自定义).

### AE-PRJ-303 · Width, Height and Lock Aspect Ratio (宽度、高度、锁定长宽比)

- **What it does:** Sets the comp frame size in pixels; an optional lock keeps the frame aspect ratio when one dimension is edited.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Width (宽度) | int | 1920 UNVERIFIED (fresh prefs; otherwise last-used) | 4..30000 | px | |
  | Height (高度) | int | 1080 UNVERIFIED | 4..30000 | px | |
  | Lock Aspect Ratio to w:h (锁定长宽比为 w:h) | bool | false UNVERIFIED | — | — | dialog-only state, not stored in the comp |
  - The label shows the reduced ratio `w/g : h/g` with `g = gcd(w, h)` and the decimal `w/h` to 2 places, e.g. "16:9 (1.78)".
  - With the lock on, editing Width sets `height = round(width * h0 / w0)` and vice versa, where `w0, h0` are the values when the lock was turned on.
  - When the size changes on an existing comp, layers are repositioned according to the Anchor setting (AE-PRJ-310). Layer content is never scaled.
  - The help page warns that the practical maximum may be lower than 30000×30000 because a full 8 bpc frame of that size is about 3.5 GB.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: `CompItem.width`/`height` (range [4..30000]). Lottie: `w`, `h` (integers). Width and height are the frame at Full resolution in comp pixels (non-square if PAR ≠ 1). Implementation hint: tile the frame buffer above the GPU max texture size (typically 16384) rather than lowering the 30000 limit.

### AE-PRJ-304 · Pixel Aspect Ratio (像素长宽比)

- **What it does:** Declares the shape of the comp's pixels so non-square formats display and composite correctly.
- **Exact semantics:** Menu values in AE order (same list as Interpret Footage, see project-comp-layers.md [prj-a]):
  | Menu entry | Stored value | Exact ratio |
  |---|---|---|
  | Square Pixels (方形像素) | 1.0 | 1/1 |
  | D1/DV NTSC (0.91) | 0.9091 | 10/11 |
  | D1/DV NTSC Widescreen (1.21) | 1.2121 | 40/33 |
  | D1/DV PAL (1.09) | 1.0940 | 128/117 UNVERIFIED |
  | D1/DV PAL Widescreen (1.46) | 1.4587 | 512/351 UNVERIFIED |
  | Anamorphic 2:1 (2) | 2.0 | 2/1 |
  | HDV 1080/DVCPRO HD 720 (1.33) | 1.3333 | 4/3 |
  | DVCPRO HD 1080 (1.5) | 1.5 | 3/2 |
  - Range 0.01..100.0 via scripting; the dialog only offers the menu.
  - Frame aspect ratio shown read-only: `width * pixelAspect / height`.
  - Layer sources with a different PAR are resampled into comp PAR at render time (cmp-a).
- **Priority:** P1 square pixels covers nearly all modern work; non-square must still load and render correctly.
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting: `AVItem.pixelAspect`. Lottie has no PAR (assumed 1.0); exporting a non-square comp to Lottie needs a scale. The Composition viewer's "Toggle Pixel Aspect Ratio Correction" is UI only (ui-preview-render-io.md [ui-a]).

### AE-PRJ-305 · Frame Rate and drop-frame timecode (帧速率、丢帧)

- **What it does:** Sets how many frames per second the comp samples and renders, and for 29.97/59.94 whether timecode is displayed drop-frame.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Frame Rate (帧速率) | f64 | 29.97 UNVERIFIED (sticky) | 1..999 UNVERIFIED | fps | menu: 8, 10, 12, 12.5, 15, 23.976, 24, 25, 29.97, 30, 50, 59.94, 60, 120 UNVERIFIED; any typed value allowed |
  | Drop Frame / Non-Drop Frame (丢帧 / 无丢帧) | enum | Drop Frame UNVERIFIED | — | — | menu shown only when frame rate is 29.97 or 59.94 |
  - 23.976, 29.97, 59.94 are stored exactly as 24000/1001, 30000/1001, 60000/1001 when typed that way (AE rounds the typed decimal internally to 1001-based rates UNVERIFIED).
  - Drop-frame affects only display: frame numbers 0 and 1 (29.97) or 0-3 (59.94) are skipped at the start of every minute except minutes divisible by 10. The separator becomes `;` (00;00;00;00) for drop-frame and `:` for non-drop.
    ```
    // drop-frame display for 29.97 (D = 2), 59.94 (D = 4)
    fpsNom = round(fps)                // 30 or 60
    framesPer10Min = fpsNom*600 - D*9
    framesPerMin   = fpsNom*60  - D
    d = frame / framesPer10Min; m = frame % framesPer10Min
    frame += 9*D*d + (m > D ? D*((m - D) / framesPerMin) : 0)
    then split frame with fpsNom into hh;mm;ss;ff
    ```
  - Changing the frame rate never moves layers or keyframes in seconds; only the frame grid changes (keyframes not on the new grid stay off-grid).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting: `AVItem.frameRate` (doc range [1.0..99.0]; UI accepts higher, UNVERIFIED), `frameDuration`, `CompItem.dropFrame`. Lottie `fr`. Flicks represent all listed rates exactly (ARCHITECTURE.md 4.4); arbitrary typed rates (e.g. 7.5) are also exact if 705,600,000 is divisible by the frame duration numerator, otherwise BAE stores the rate as a rational and rounds frame times to the nearest flick. Timecode display base (Project Settings) is prj-a.

### AE-PRJ-306 · Resolution (分辨率)

- **What it does:** Sets the comp's render downsample factor for previews (and for rendering when Render Settings say "Current Settings").
- **Exact semantics:** Menu in AE order:
  | Entry (中文) | resolutionFactor |
  |---|---|
  | Full (完整) | [1, 1] |
  | Half (二分之一) | [2, 2] |
  | Third (三分之一) | [3, 3] |
  | Quarter (四分之一) | [4, 4] |
  | Custom... (自定义) | [x, y], each 1..99 UNVERIFIED (Custom Resolution dialog: "Render every x pixels horizontally, y pixels vertically") |
  - Rendered buffer size = `ceil(width / x) × ceil(height / y)` UNVERIFIED rounding.
  - The same value is shown and edited from the Composition viewer's resolution menu (which also offers Auto); "Auto" is a viewer preference, not a comp field.
  - Not saved in presets (AE-PRJ-302).
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting: `CompItem.resolutionFactor`. Rendering effects of resolution (effect parameter scaling, nested comp resolution) are specified in compositing-3d-color.md [cmp-a]. Not in Lottie.

### AE-PRJ-307 · Start Timecode / Start Frame (开始时间码 / 开始帧)

- **What it does:** Sets the timecode or frame number displayed for the comp's first frame. It changes labelling only, never rendering.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Start Timecode (开始时间码) | time | 0:00:00:00 | -3:00:00:00 .. 23:59:00:00 (AE 17.1+; before 17.1 minimum 0) | timecode | label "Start Frame" (开始帧) when Project Settings display style is Frames; in Frames mode default is 0 or 1 depending on Project Settings "Frame count" (start at 0 / start at 1) UNVERIFIED |
  - All internal times (layer startTime, keyframe times, expression `time`) are relative to comp time 0, not to displayStartTime. Displayed time = `displayStartTime + t`.
  - Expressions: `thisComp.displayStartTime` returns this value in seconds.
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: `CompItem.displayStartTime` (s, range [-10800.0..86339.0]), `displayStartFrame` (int, AE 17.1+). Not stored by Lottie. BAE decision: store as an integer frame count plus the comp frame rate to avoid the float issues AE 17.1 had to work around.

### AE-PRJ-308 · Composition Duration (持续时间)

- **What it does:** Length of the comp timeline; frames at or beyond it do not exist.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Duration (持续时间) | time | 0:00:30:00 UNVERIFIED (fresh prefs; sticky afterwards) | 1 frame .. 3:00:00:00 (10800 s) | timecode/frames | entered in the current display style, relative to Start Timecode ("is 0:00:30:00 Base 30" read-out) |
  - Valid frames are `0 .. ceil(duration * fps) - 1`; the comp's last frame starts at `duration - frameDuration`.
  - Changing duration never moves layers. Shortening it does not trim layer in/out points: layer bars and keyframes past the end are kept and simply not visible/renderable; lengthening reveals them again (AE-PRJ-319).
  - If the work area ended at the old duration, it follows the new end; otherwise it is clipped to `[0, duration]` (AE-PRJ-323).
  - Footage longer than 3 h can be used but times after 3 h display incorrectly (AE help).
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** Scripting: `AVItem.duration` (read/write for comps, [0.0..10800.0]). Lottie root `op` - `ip` in frames (bodymovin exports the work area; see AE-PRJ-323). BAE decision: no hard 3 h limit is required internally (flicks in i64 cover ~418 years), but keep 3 h in the dialog for AE parity and project round-trip.

### AE-PRJ-309 · Background Color (背景颜色)

- **What it does:** The colour the Composition viewer shows behind all layers and that is used as the fill when rendering to formats without alpha.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Background Color (背景颜色) | RGB | black [0, 0, 0] | 0..1 per channel | — | colour picker + eyedropper; no alpha |
  - Rendering rule: the comp's rendered image always has the background as fully transparent (alpha 0) internally. The background colour is visible only (a) in the viewer when the transparency grid is off and (b) in output when the output channels are RGB only (no alpha), where the composite is flattened over the background colour.
  - When the comp is nested as a precomp layer, its background is transparent; the containing comp's background applies. To keep it, users add a solid layer.
  - With RGB+Alpha output, background colour does not appear in RGB for premultiplied output: pixels with alpha 0 are (0,0,0); for straight output, UNVERIFIED: AE writes the background colour into the RGB of fully transparent pixels ("matted with background color" for premultiplied is a Render Queue option, see ui-preview-render-io.md [ui-c]).
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting: `CompItem.bgColor` ([R, G, B] in 0..1). Expressions cannot read it. Lottie has no background (players draw transparent). Implementation hint: never write the background into the accumulation buffer; apply it only in the final display/export flatten pass, so precomps and alpha output stay correct for free.

### AE-PRJ-310 · Anchor for resizing (锚点)

- **What it does:** In the Advanced tab, a 3×3 grid of buttons chooses which point of the frame stays fixed when Width/Height change, so existing layers keep their position relative to that point.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Anchor (锚点) | enum 9 | Center (中) | TL, T, TR, L, C, R, BL, B, BR | — | dialog-only, not stored |
  ```
  ax = {L:0, C:0.5, R:1}[col]; ay = {T:0, C:0.5, B:1}[row]
  dx = (newW - oldW) * ax ; dy = (newH - oldH) * ay   // comp pixels
  for each layer with no parent (2D and 3D, incl. cameras and lights UNVERIFIED):
      Position += (dx, dy, 0)   // static value and every Position keyframe;
                                 // also X/Y Position when dimensions are separated
  ```
  - Parented layers are not moved (they follow their parent). Layer content and anchor points are not changed.
  - Applies only when the size changes through this dialog (and through Crop Comp to Region of Interest, which uses its own offset, AE-PRJ-320).
  - UNVERIFIED: layers whose Position is driven by an expression are not changed (the expression result is unaffected); BAE should warn.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Not a stored setting (scripting has no attribute). Implement as part of the "Set Comp Settings" command so the size change and all position offsets are one undo step.

### AE-PRJ-311 · Preserve frame rate when nested or in render queue (在嵌套时或在渲染队列中保留帧速率)

- **What it does:** Makes a comp keep its own frame rate when it is nested in a comp with another rate or rendered with a different Render Settings frame rate, producing held frames instead of resampling.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | Preserve frame rate when nested or in render queue (在嵌套时或在渲染队列中保留帧速率) | bool | false | not inherited by new comps (not sticky), not in presets |
  - When true, any request for this comp at time t (from a parent comp or the render queue) is evaluated at `t' = floor((t + ε) * fps_own) / fps_own` (snap to this comp's frame grid, measured from this comp's time 0), so inner content changes only on this comp's frames.
  - The Render Queue frame rate override is ignored for this comp (output still has the requested frame count; frames repeat).
  - Data only here; rendering semantics and interaction with time remap/stretch are in compositing-3d-color.md [cmp-a] "nested comp res/fps".
- **Priority:** P1
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting: `CompItem.preserveNestedFrameRate`. Adobe recommends it instead of Posterize Time. ε: AE adds a small tolerance so that exact frame boundaries are not lost to float error; BAE uses exact flick arithmetic instead.

### AE-PRJ-312 · Preserve resolution when nested (在嵌套时保留分辨率)

- **What it does:** Makes a nested comp render at its own Resolution setting instead of inheriting the containing comp's resolution.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | Preserve resolution when nested (在嵌套时保留分辨率) | bool | false | not sticky, not in presets |
  - false (default): a nested comp renders at the resolution factor of the comp that contains it (recursively from the top-level comp being previewed/rendered).
  - true: the nested comp renders at its own `resolutionFactor`, then is scaled to the parent's sampling.
- **Priority:** P2
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting: `CompItem.preserveNestedResolution`. Render behaviour: compositing-3d-color.md [cmp-a].

### AE-PRJ-313 · Composition motion-blur settings (运动模糊设置)

- **What it does:** Comp-wide parameters for layer motion blur: how long the virtual shutter is open, when it opens relative to the frame, and how many sub-frame samples are taken.
- **Exact semantics:** Advanced tab, AE order:
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Shutter Angle (快门角度) | int | 180 | 0..720 | degrees | exposure = angle/360 frame durations |
  | Shutter Phase (快门相位) | int | -90 | -360..360 | degrees | shutter open offset from frame start |
  | Samples Per Frame (每帧采样数) | int | 16 | 2..64 | samples | minimum; used for 3D layers, shape layers and when motion cannot be measured (also for effects that do their own MB UNVERIFIED) |
  | Adaptive Sample Limit (自适应采样限制) | int | 128 | 16..256 | samples | maximum for 2D layers with adaptive sampling |
  ```
  frameDur = 1 / comp.frameRate
  shutterOpen  = t + (phase / 360) * frameDur      // default: t - 0.25 frame
  shutterClose = shutterOpen + (angle / 360) * frameDur  // default: t + 0.25 frame
  ```
  - With the defaults the exposure window is centred on the frame time. Angle 0 means no blur even when switches are on.
  - Sample placement and adaptive count: see transform-keyframes-expressions.md [ani-a] "motion-blur sampling"; accumulation: compositing-3d-color.md [cmp-a].
  - The Render Settings dialog can override the shutter angle (and use comp's phase) for a render; see ui-preview-render-io.md [ui-c].
- **Priority:** P0 motion blur with default shutter is basic AE output.
- **Owner:** bae-model (+ bae-anim, bae-render)
- **Notes:** Scripting: `shutterAngle`, `shutterPhase`, `motionBlurSamplesPerFrame`, `motionBlurAdaptiveSampleLimit`. Lottie root `mb: {sa, sp, spf, asl}` (Shutter Angle, Shutter Phase, Samples per Frame, Adaptive Sample Limit; rarely supported by players). Expression access: `thisComp.shutterAngle`, `thisComp.shutterPhase`. These four settings are sticky for new comps like the rest of Composition Settings: AE help says new comps reuse the last-used settings except the two Preserve options (Source: https://helpx.adobe.com/after-effects/using/composition-basics.html).

### AE-PRJ-314 · 3D Renderer selection (3D 渲染器)

- **What it does:** Chooses the engine that renders 3D layers in the comp, and its options.
- **Exact semantics:** Renderer menu (menu order UNVERIFIED):
  | Entry (中文) | `CompItem.renderer` string | Versions | BAE |
  |---|---|---|---|
  | Classic 3D (经典 3D) | "ADBE Advanced 3d" (historical name: Classic 3D was called "Advanced 3D" in AE CS3-CS5) | all; default for new comps (renderer choice is sticky with the last-used Composition Settings) UNVERIFIED | P0 |
  | Advanced 3D (高级 3D) | "ADBE Calder" | AE 24.0 (October 2023) on | P2 |
  | CINEMA 4D (CINEMA 4D) | "ADBE Ernst" | AE 13.5/CC 2015 UNVERIFIED to present (deprecated, hidden by default in recent versions UNVERIFIED) | P2 |
  | Ray-traced 3D (光线追踪 3D) | "ADBE Picasso" | AE 11.0 (CS6) to AE 2019 UNVERIFIED; legacy projects only | not supported |
  - Renderer options (Options… button):
    - Classic 3D: Shadow Map Resolution (阴影图分辨率) UNVERIFIED values: Comp size-based default, 256, 512, 1024, 2048, 4096, 8000; specified in compositing-3d-color.md [cmp-b].
    - Advanced 3D: quality, anti-aliasing, ray-traced shadows / reflections, environment light behaviour (cmp-b).
    - CINEMA 4D: Quality slider (Draft … Typical … Extreme) controlling ray threshold/ray depth/reflection depth/shadow depth/anti-aliasing; Choose Cinema 4D Installation.
  - Classic 3D does not render extrusions/bevel (Geometry Options are hidden); switching renderer keeps extrusion data but does not render it (see compositing-3d-color.md [cmp-b], Advanced 3D / CINEMA 4D renderer).
  - Unknown renderer strings in an imported project fall back to Classic 3D with a warning.
- **Priority:** P0 for storing the field and Classic 3D; other renderers P2.
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting: `CompItem.renderer`, `CompItem.renderers` (installed modules). The match-name strings are counter-intuitive: "ADBE Advanced 3d" is CLASSIC 3D and "ADBE Calder" is the new Advanced 3D; an AE 2024 beta briefly renamed them ("ADBE Classic 3d"/"ADBE Advanced 3d"/"ADBE Cinema 4d") and Adobe reverted the change. Importers should accept both spellings. Source: https://community.adobe.com/questions-534/match-names-for-renderers-changing-314356. Lottie: no renderer field (players are effectively Classic 3D or 2D only). BAE decision: BAE's single GPU 3D renderer implements Classic 3D semantics first; Advanced 3D is a later addition behind the same enum.

### AE-PRJ-315 · Composition switches: Motion Blur, Frame Blending, Hide Shy Layers, Draft 3D, active camera (合成开关)

- **What it does:** Comp-level toggles in the Timeline panel header that globally gate per-layer switches, plus the derived active camera.
- **Exact semantics:**
  | Switch (中文) | Field | Default | Effect |
  |---|---|---|---|
  | Enables Motion Blur for all layers with the Motion Blur switch set (为设置了"运动模糊"开关的所有图层启用运动模糊) | `motionBlur` | true UNVERIFIED (on in new comps since AE CS6) | layer blur happens iff comp.motionBlur && layer.motionBlur && shutterAngle > 0 |
  | Enables Frame Blending for all layers with the Frame Blend switch set (为设置了"帧混合"开关的所有图层启用帧混合) | `frameBlending` | true UNVERIFIED | layer frame blending happens iff comp.frameBlending && layer.frameBlendingType != None |
  | Hide all layers for which the Shy switch is set (隐藏为其设置了"消隐"开关的所有图层) | `hideShyLayers` | false | hides shy layers in the Timeline only; never affects rendering (AE-PRJ-361) |
  | Draft 3D (草图 3D) | `draft3d` | false | viewer-only: disables lights, shadows, camera depth of field (and in AE 2022+ adds an extended 3D ground plane/viewer) for interactive speed; never affects Render Queue output |
  | Active camera (活动摄像机) | `activeCamera` (derived) | — | the top-most (lowest index) camera layer that has Video on and whose time span contains the current time; null → default comp camera (cmp-b) |
  - Render Settings can override the comp motion-blur and frame-blending switches ("For Checked Layers" / "Off for All Layers") and they are respected per render (ui-preview-render-io.md [ui-c]).
- **Priority:** P0 (motion blur, frame blending, shy, active camera); Draft 3D P1.
- **Owner:** bae-model (+ bae-render, bae-ui-motion)
- **Notes:** Scripting: `CompItem.motionBlur`, `frameBlending`, `hideShyLayers`, `draft3d`, `activeCamera` (read-only). Expressions: `thisComp.activeCamera`. Lottie: no comp on/off switches (root `mb` object carries only `sa`, `sp`, `spf`, `asl`); the per-layer Motion Blur switch is layer `mb` (boolean) in the Lottie schema (Source: https://lottiefiles.github.io/lottie-docs/layers/). Also in this group in AE 2026: the "Enable Live Update", "Graph Editor", "Brainstorm" and "Frame Render Time" toggles, which are UI (ui-preview-render-io.md [ui-a]).

## Creating and resizing compositions

### AE-PRJ-316 · New Composition (新建合成)

- **What it does:** Composition > New Composition (Ctrl+N / Cmd+N) opens Composition Settings and creates an empty comp in the project.
- **Exact semantics:**
  - Dialog fields are pre-filled with the values used the last time Composition Settings was confirmed (any comp), EXCEPT "Preserve frame rate when nested or in render queue" and "Preserve resolution when nested", which always start off. On first launch (fresh preferences) the defaults are those in AE-PRJ-300 (UNVERIFIED: 1920×1080, square, 29.97 fps, 0:00:30:00, black, Full).
  - The name defaults to the next free "Comp N" (AE-PRJ-301).
  - The new comp is created in the currently selected Project panel folder (or the folder containing the selected item; root otherwise), selected, and opened in the Timeline and Composition panels. UNVERIFIED: in AE the new comp goes to the project root unless a folder is selected.
  - Cancel creates nothing. The whole operation is one undo step.
  - The Project panel "Create a new Composition" button does the same with no item selected; with footage selected, or footage dragged onto it, it runs New Comp from Selection (AE-PRJ-317).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: `app.project.items.addComp(name, width, height, pixelAspect, duration, frameRate)`. BAE decision: keep the "sticky last settings" behaviour but store it in user preferences, not in the project.

### AE-PRJ-317 · New Comp from Selection (基于所选项新建合成)

- **What it does:** File > New Comp from Selection (Alt+\\ on Windows / Cmd+\\ on macOS, per the AE shortcut reference), or dragging footage onto the New Composition button or into an empty Timeline, creates a comp whose settings match a footage item and places the item(s) in it as layers.
- **Exact semantics:**
  - **One item selected:** no dialog. comp.name = item name (without extension UNVERIFIED); width, height, pixelAspect = the item's; frameRate = the item's frame rate for movies/sequences/comps, and for stills the last-used comp frame rate UNVERIFIED; duration = item duration for movies, sequences and comps, and for stills the Preferences > Import > Still Footage duration when it is set to a fixed duration; with the factory setting "Length of Composition" the last-used comp duration is used UNVERIFIED; one layer is added spanning the comp; the comp opens.
  - **Several items selected:** the New Composition from Selection dialog (基于所选项新建合成) appears:
    | Parameter (中文) | Type | Default | Range | Notes |
    |---|---|---|---|---|
    | Create: Single Composition / Multiple Compositions (单个合成 / 多个合成) | enum | Multiple Compositions UNVERIFIED | — | |
    | Use Dimensions From (使用尺寸来自) | item menu | first selected item | selected items | Single only; supplies width, height, PAR (and frame rate UNVERIFIED) |
    | Still Duration (静止持续时间) | time | Preferences > Import still length UNVERIFIED | ≥ 1 frame | Single: layer length for stills; Multiple: comp duration for each still |
    | Add to Render Queue (添加到渲染队列) | bool | false | — | adds every created comp to the Render Queue |
    | Sequence Layers (序列图层) | bool | false | — | Single only; arranges layers end-to-end (AE-PRJ-406 math) |
    | Overlap (重叠) | bool | false | — | Single + Sequence only |
    | Duration (持续时间) | time | 0:00:01:00 UNVERIFIED | ≥ 0 | overlap length |
    | Transition (过渡) | enum | Dissolve Front Layer UNVERIFIED | Off / Dissolve Front Layer / Cross Dissolve Front and Back Layers | see Sequence Layers |
  - Single Composition: one comp named after the "Use Dimensions From" item UNVERIFIED; layers stacked in selection order with the first selected item at the top (index 1) UNVERIFIED; all layers start at 0 unless Sequence Layers is on; comp duration = max(layer out points) (sequenced: last layer's out point).
  - Multiple Compositions: one comp per selected item, each created as in the one-item case (with Still Duration for stills); none is opened UNVERIFIED.
  - If a selected item is a comp, it is nested as a precomp layer (no copy).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** Footage frame-rate source follows Interpret Footage (project-comp-layers.md [prj-a]). Also used by dragging footage to an empty Composition panel. Audio-only items create a comp with the last-used frame size UNVERIFIED.

### AE-PRJ-318 · Changing composition duration and layer clipping (更改合成持续时间)

- **What it does:** Defines what happens to layers, keyframes, work area and markers when a comp's duration is lengthened or shortened.
- **Exact semantics:**
  - Layer startTime, inPoint, outPoint, keyframes and layer markers are never modified by a duration change.
  - Shortening: any part of a layer at or after the new duration is outside the comp; the Timeline clips the bars visually; layers entirely outside are still in the stack and still evaluate (e.g. expressions referencing them work). Comp markers after the end are kept but hidden UNVERIFIED.
  - Lengthening: previously hidden parts reappear. Layers whose source is a still, solid, text, shape, null, camera, light or adjustment layer are NOT extended automatically; their outPoint stays where it was. UNVERIFIED: AE does not auto-extend layers that ended exactly at the old comp end.
  - Work area: clamped into `[0, duration]`; if it ended at the old end, it now ends at the new end UNVERIFIED.
  - CTI: clamped to `duration - frameDuration` if past the end.
  - Precomp layers that use this comp keep their in/out points; if the nested comp became shorter than the precomp layer's span, frames beyond the nested comp's end render transparent (no hold) unless time remapping holds them.
- **Priority:** P0
- **Owner:** bae-model (+ bae-motion)
- **Notes:** Lottie: layer `ip`/`op` may extend beyond root `op`; players clip. BAE decision: offer an optional "extend layers that ended at the comp end" checkbox (off by default for AE parity).

### AE-PRJ-319 · Trim Comp to Work Area (将合成修剪到工作区域)

- **What it does:** Composition > Trim Comp to Work Area (Ctrl+Shift+X / Cmd+Shift+X) shortens the comp so it contains only the work area.
- **Exact semantics:**
  ```
  ws = comp.workAreaStart; wd = comp.workAreaDuration
  for each layer: layer.startTime -= ws          // in/out points, keyframes, markers move with it
  for each comp marker: marker.time -= ws
  comp.displayStartTime += ws  // timecode of every remaining frame is preserved UNVERIFIED
  comp.duration = wd
  comp.workAreaStart = 0; comp.workAreaDuration = wd
  comp.time = clamp(comp.time - ws, 0, wd - frameDuration)
  ```
  - Layers are not trimmed; parts outside the new range simply lie outside the comp.
  - Comp markers that end up at negative time or past the end are kept UNVERIFIED (AE may delete them).
  - Not available when the work area already equals the whole comp UNVERIFIED (menu enabled but no-op).
- **Priority:** P1
- **Owner:** bae-motion (+ bae-model)
- **Notes:** zh-CN 将合成修剪到工作区域 (from the zh-CN keyboard-shortcut reference; menu string not checked in the AE 2026 UI). Single undo step.

### AE-PRJ-320 · Crop Comp to Region of Interest (将合成裁剪到目标区域)

- **What it does:** Composition > Crop Comp to Region of Interest resizes the comp frame to the current Region of Interest rectangle drawn in the Composition viewer, keeping layers visually in place.
- **Exact semantics:**
  ```
  roi = (x0, y0, w, h) in comp pixels (integers, inside the frame)
  comp.width = w; comp.height = h
  for each unparented layer: Position -= (x0, y0, 0)   // all keyframes too, like AE-PRJ-310 with explicit offset
  ROI is cleared afterwards
  ```
  - Available only when an ROI is set in the active Composition viewer. Duration, frame rate and other settings are unchanged.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** ROI itself is a viewer state (compositing-3d-color.md [cmp-b] "preview resolution/ROI"). zh-CN 将合成裁剪到目标区域（译名待核）.

### AE-PRJ-321 · Crop Comp to Selected Layer(s) Bounds (将合成裁切到所选图层边界)

- **What it does:** Resizes the comp (shrinking or growing) to the union of the visible bounds of the selected 2D layers. Added in AE 26.3 (June 2026) per Adobe "What's new".
- **Exact semantics:**
  ```
  r = union over selected 2D layers of layerBoundsInComp(layer, comp.time)  // transformed, masked visible bounds UNVERIFIED (mask/effect inclusion)
  r = integer rectangle (floor(min), ceil(max))
  comp.width = r.w; comp.height = r.h        // may be larger than before (expand)
  for each unparented layer: Position -= (r.x, r.y, 0)
  ```
  - Disabled when any selected layer is 3D, or when no layer is selected. Uses the bounds at the current time only (animated layers may leave the new frame at other times).
  - Available from the Composition menu and from the layer context menu in the Timeline and Composition panels.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-model, bae-render)
- **Notes:** Layer bounds definition: compositing-3d-color.md [cmp-a] "layer bounds". zh-CN 将合成裁切到所选图层边界 (Source: https://helpx.adobe.com/cn/after-effects/using/composition-basics.html).

### AE-PRJ-322 · Duplicate composition (复制合成)

- **What it does:** Edit > Duplicate (Ctrl+D) with a comp selected in the Project panel creates an independent copy of the comp.
- **Exact semantics:**
  - Copies every comp field (AE-PRJ-300) and every layer with all properties, keyframes, expressions, markers, effects, masks and switches. Layer IDs are new; parent links and track-matte links are remapped to the copies.
  - Nested comps (precomp layers) and footage are NOT duplicated: the copy's layers reference the same items (shallow duplicate).
  - Name: "<name> 2"; if the name already ends in a number, that number is incremented to the next free one UNVERIFIED (e.g. "Main" → "Main 2", "Main 2" → "Main 3").
  - The copy is placed in the same folder, directly after the original UNVERIFIED.
  - Expressions in the copy that reference `thisComp` refer to the copy; those that reference `comp("Original")` still point to the original.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Scripting: `CompItem.duplicate()`. If project-comp-layers.md [prj-a] specifies item duplication generally, this item adds only the comp-specific remapping rules. BAE decision: offer a separate "Deep Duplicate" (duplicating nested comps) as a BAE extension; AE needs scripts for this.

## Work area

### AE-PRJ-323 · Work area (工作区域)

- **What it does:** A sub-range of the comp timeline that limits previews, caching (by preference) and, by default, rendering; it is the bracket bar under the Timeline time ruler.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | workAreaStart (工作区域开头) | time | 0 | 0 .. duration - 1 frame | s | snapped to frames |
  | workAreaDuration (工作区域持续时间) | time | comp duration | 1 frame .. duration - start | s | end = start + duration (exclusive) |
  - Set work area start to CTI: **B**; set end: **N**. `B: start = cti` (if `cti >= end`, end is moved to `comp.duration` UNVERIFIED); `N: end = cti + frameDuration` so the frame under the CTI is included (if `end <= start`, start is moved to 0 UNVERIFIED).
  - Ctrl+Alt+B / Cmd+Opt+B: work area = union span of the selected layers' [inPoint, outPoint) clipped to the comp, or the whole comp when no layer is selected.
  - Dragging the bar's ends sets start/end (Shift snaps); dragging the middle moves it keeping its duration; double-clicking the bar resets it to the whole comp.
  - Shift+Home / Shift+End: move CTI to work-area start / last frame of the work area.
  - Uses: preview range (unless the Preview panel range is set otherwise, ui-preview-render-io.md [ui-b]); Render Settings Time Span default "Work Area Only" for comps whose work area is not the full comp, otherwise "Length of Comp" UNVERIFIED (ui-preview-render-io.md [ui-c]); Trim Comp to Work Area; Lift/Extract Work Area; New Comp from work area (Precompose does not use it).
  - The work area may never be empty; on comp duration change it is clamped (AE-PRJ-318).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: `CompItem.workAreaStart`, `workAreaDuration`. Lottie: bodymovin exports root `ip`/`op` from the work area (in frames) UNVERIFIED; importers should set the work area to [ip, op). Shortcuts: ui-preview-render-io.md [ui-b].

### AE-PRJ-324 · Lift Work Area and Extract Work Area (提升工作区域、提取工作区域)

- **What it does:** Edit > Lift Work Area removes the part of layers inside the work area and leaves a gap; Edit > Extract Work Area removes it and closes the gap (ripple delete).
- **Exact semantics:**
  ```
  targets = selected layers, or all unlocked layers if none selected
  [a, b) = work area
  for each layer L in targets with span [L.in, L.out) overlapping [a, b):
      if L.in >= a and L.out <= b: delete L                         // fully inside
      elif L.in < a and L.out > b:                                   // spans the work area
          R = split L at b (AE-PRJ-383 Split rules: duplicate above, named like Split Layer)
          L.out = a; R.in = b
      elif L.in < a: L.out = a                                       // overlaps the start
      else: L.in = b                                                 // overlaps the end
  Extract only, afterwards:
      for each affected piece with in >= b (and every target layer lying fully after b UNVERIFIED):
          piece.startTime -= (b - a)        // keyframes and markers move with the layer
  comp duration and work area are unchanged
  ```
  - Keyframes are not deleted; each piece keeps the full keyframe set and only its in/out points change (same as Split Layer) UNVERIFIED.
  - Locked layers are never changed. Comp markers are not moved by Extract UNVERIFIED.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-model)
- **Notes:** One undo step. Help (Selecting and arranging layers) states the "selected or all unlocked layers" rule and the gap vs ripple behaviour; the split details are UNVERIFIED.

## Markers

### AE-PRJ-325 · Marker data model (标记)

- **What it does:** A marker is a time-stamped record carrying comment, duration, chapter, web link, cue point and label data; composition markers and layer markers use the same record type.
- **Exact semantics:** Marker dialog fields in AE order (dialog title "Composition Marker" (合成标记) / "Layer Marker" (图层标记)):
  | Field (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Time (时间) | time | CTI when added | any comp time | s | comp markers: comp time; layer markers: stored relative to the layer (see AE-PRJ-327) |
  | Duration (持续时间) | time | 0 | ≥ 0 | s | 0 = point marker; >0 = span marker with out point |
  | Label (标签) | int | 0 (None) | 0..16 | — | AE 16.0 (2019); colours from Preferences > Labels (AE-PRJ-392) |
  | Comment (注释) | string | "" (numbered comp markers created with Shift+0..9 get "0".."9", AE-PRJ-326) | — | — | shown next to the marker |
  | Chapter (章节) | string | "" | — | — | written as chapter points to formats that support them |
  | URL (URL) | string | "" | — | — | Web Links group (Web 链接) |
  | Frame Target (帧目标) | string | "" | — | — | HTML frame/window target for the URL |
  | Cue Point Name (提示点名称)（译名待核） | string | "" | — | — | Flash/F4V cue point; group "Flash Cue Point" (Flash 提示点)（译名待核） |
  | Event / Navigation (事件 / 导航)（译名待核） | enum | Event | Event, Navigation | — | scripting `eventCuePoint` true = Event |
  | Parameter Name / Parameter Value (参数名称 / 参数值)（译名待核） | ordered list of string pairs | [] | — | — | Add/Remove buttons; scripting `getParameters()`/`setParameters(obj)` |
  | Protected Region (受保护区域)（译名待核） | bool | false | — | — | comp markers only; AE 16.0; AE-PRJ-330 |
  | Marker type (标记类型)（译名待核）: Comment (注释) / Segmentation (分段) | enum | Comment | Comment, Segmentation | — | layer marker dialog radio choice; AE 22.3 (April 2022, with Scene Edit Detection, AE-PRJ-422); each type has its own default label colour in Preferences > Labels UNVERIFIED (which indices); scripting name UNVERIFIED |
  - Markers are values of a special property `ADBE Marker` (type MARKER) on each layer, and `CompItem.markerProperty` for comps (AE-PRJ-350 property types). Keyframe-like: each marker is a key with a time; they cannot have interpolation.
  - A marker with link or cue-point data shows a small dot in its icon (UI).
  - Markers do not affect rendering, except Protected Region (time behaviour of nested comps) and output metadata (chapters/URLs/cue points/XMP are written by the output module, ui-preview-render-io.md [ui-c]).
- **Priority:** P0 for time/duration/comment/label; P2 for chapter, URL, frame target, cue points.
- **Owner:** bae-model
- **Notes:** Scripting: `MarkerValue(comment, chapter, url, frameTarget, cuePointName, params)`, attributes `comment, chapter, url, frameTarget, cuePointName, eventCuePoint, duration, label, protectedRegion`. Expressions: `marker.key(i).time/duration/comment/chapter/url/frameTarget/eventCuePoint/cuePointName/parameters/protectedRegion` (transform-keyframes-expressions.md [ani-b]). Lottie: root `markers: [{tm, cm, dr}]` (frames); bodymovin uses comp markers only; layer markers are not exported UNVERIFIED. Custom label colours cannot be set per marker (only the 16 indices).

### AE-PRJ-326 · Composition markers (合成标记)

- **What it does:** Markers on the comp's time ruler, used for navigation, sync points, numbered cue positions and output chapters.
- **Exact semantics:**
  - Add at CTI with no layer selected: Layer > Add Marker or numpad `*`; numpad Alt/Opt+`*` adds and opens the dialog; drag from the Comp Marker Bin (合成标记素材箱) button at the right end of the time ruler to any time. Pressing `*` during a preview adds markers without stopping playback.
  - Layer > Add Marker, numpad `*` and the bin create a blank comp marker (comment "").
  - Numbered markers: Shift+0..9 (main keyboard) creates a marker with comment "0".."9" at CTI; if a comp marker with that number already exists, AE does not create a new one but MOVES the existing one to the CTI. Pressing 0..9 (main keyboard) moves the CTI to that numbered marker. Deleting a numbered marker does not renumber the others; changing a marker's comment away from its number frees that number for reuse.
  - Delete: drag the marker onto the Comp Marker Bin, or Ctrl/Cmd-click it.
  - Uniqueness: at most one comp marker can start at a given time; adding or moving a marker onto another marker's start time replaces (deletes) the other.
  - Any number of comp markers per comp; marker times outside [0, duration) are allowed but hidden UNVERIFIED.
  - Expressions read them via `thisComp.marker`.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Source: https://helpx.adobe.com/after-effects/using/layer-markers-composition-markers.html (blank vs numbered markers, one marker per start time, Shift+number moves an existing numbered marker). Comp markers correspond to Premiere Pro sequence markers. Exact keyboard bindings: ui-preview-render-io.md [ui-b].

### AE-PRJ-327 · Layer markers (图层标记)

- **What it does:** Markers attached to a layer's duration bar; they move with the layer and are visible to expressions as `marker` on that layer.
- **Exact semantics:**
  - Add: with layers selected, Layer > Add Marker or numpad `*` adds one marker at CTI to each selected layer; Alt/Opt+numpad `*` adds and opens the dialog. Default comment "".
  - Time base: markers are stored in layer time (relative to the layer's startTime) so moving the layer in time moves its markers. Displayed/scripted time is comp time: `markerCompTime = layer.startTime + markerLayerTime * stretch/100` UNVERIFIED (time stretch scales marker positions like keyframes; Time-Reverse mirrors them UNVERIFIED).
  - Markers may lie outside the layer's in/out span (e.g. after trimming); they are kept.
  - Any number of markers per layer; several layer markers may share a time UNVERIFIED (the replace rule documented for comp markers is not documented for layer markers).
  - Split Layer / Duplicate copy all markers to the new layer (AE-PRJ-383, AE-PRJ-384); Precompose "Move all attributes" moves them into the new comp's layer (they stay layer markers there).
  - Footage layers: if the Preferences "Create Layer Markers From Footage XMP Metadata" option (Disk / Media & Disk Cache category depending on version) is on (default on), markers are created from the source file's temporal XMP metadata when the layer is created.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Matchname `ADBE Marker` at layer root (shown as "Marker" (标记) in the Timeline only when markers exist UNVERIFIED). Layer markers correspond to Premiere Pro clip markers; they are retained when rendering to QuickTime.

### AE-PRJ-328 · Marker editing operations (编辑标记)

- **What it does:** Moving, retiming, deleting and locking comp and layer markers.
- **Exact semantics:**
  | Operation | Gesture | Result |
  |---|---|---|
  | Open settings | double-click marker, or context menu > Settings (设置) | marker dialog (AE-PRJ-325); Time field moves the marker |
  | Move | drag the marker (in point); Shift snaps | time changes, duration unchanged |
  | Set duration | drag the marker's out-point handle; or Alt/Opt-drag the marker icon to the right on a 0-duration marker | duration changes; time unchanged. Time and duration cannot change in one drag |
  | Delete one | Ctrl/Cmd-click the marker; or drag a comp marker onto the Comp Marker Bin; context menu > Delete This Marker (删除此标记) | removed |
  | Delete all | context menu > Delete All Markers (删除所有标记) | all markers of that layer (or the comp) removed |
  | Lock | context menu > Lock Markers (锁定标记) | all markers of that layer / comp become non-editable by drag UNVERIFIED version (AE 2022+?) |
  | Update Markers From Source (从源更新标记) | layer marker context menu | AE-PRJ-329 |
  - Moving the CTI with Shift-drag, and dragging layers/keyframes with Shift, snaps to markers (and marker out points) of both kinds.
  - J/K navigation stops at markers (ui-preview-render-io.md [ui-b]).
- **Priority:** P0 (move, duration, delete); P1 (lock, delete all).
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** All marker edits are undoable; Lock is persisted per marker list UNVERIFIED (scripting has no attribute).

### AE-PRJ-329 · Markers on precomp layers and Update Markers From Source (预合成图层上的标记、从源更新标记)

- **What it does:** When a comp is nested, its comp markers are copied as layer markers onto the new precomp layer; "Update Markers From Source" re-synchronises them later.
- **Exact semantics:**
  - On creating a precomp layer (drag comp into comp, Precompose, New Comp from Selection with a comp), every comp marker of the source comp becomes a layer marker on that layer at the same source time (mapped through layer startTime/stretch). They are independent copies: editing or deleting the source comp's markers does not change them.
  - Layer marker context menu > Update Markers From Source: replaces ALL markers of the layer (including user-added ones) with fresh copies of the source comp's comp markers; for footage layers it restores markers from the source file's temporal XMP metadata.
  - Protected-region markers of the source comp appear on the precomp layer and keep `protectedRegion = true` (scripting doc).
  - UNVERIFIED: since AE 16.0 (2019) the copies are created on nesting; older versions did not copy markers.
- **Priority:** P1
- **Owner:** bae-model (+ bae-motion)
- **Notes:** BAE decision: optionally display the source comp's live markers as read-only ghosts on precomp layers in addition to the copied ones; keep AE copy semantics for data and AEP round-trip.

### AE-PRJ-330 · Protected Region and Responsive Design – Time (受保护区域、响应式设计 - 时间)

- **What it does:** A comp marker with Protected Region set marks a time span of a comp that must not be time-stretched when the comp is nested and its precomp layer is stretched or its duration is changed (responsive design).
- **Exact semantics:**
  - Create: tick Protected Region in the comp marker dialog (marker should have duration > 0), or Composition > Responsive Design – Time > Create Intro (创建入点)（译名待核） / Create Outro / Create Protected Region From Work Area; "Create Intro" makes a protected region from comp start to the CTI, "Create Outro" from CTI to comp end UNVERIFIED.
  - Effect when nested: if the precomp layer (or the nested comp duration via Comp Settings) is time-stretched, frames inside protected regions play at 100 % speed; only unprotected spans are stretched to absorb the difference. Implemented by AE as an automatic time-remap mapping UNVERIFIED.
  - A comp with protected regions shows them as shaded spans on the comp marker in the parent's precomp layer UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-anim (+ bae-model, bae-motion)
- **Notes:** AE 16.0 (2019). Scripting: `MarkerValue.protectedRegion`. Rarely used outside Motion Graphics templates; time-mapping math in transform-keyframes-expressions.md [ani-a] "time remap" if specified there, otherwise to be defined by bae-anim.

## Layer types

### AE-PRJ-331 · Layer type taxonomy (图层类型)

- **What it does:** Defines the closed set of layer kinds a comp can contain, how each maps to AE's scripting classes and to Lottie `ty`, and the defaults shared by all new layers.
- **Exact semantics:**
  | BAE kind | AE scripting class / flag | Source | Lottie `ty` (bodymovin layerTypes) | Default label (Preferences > Labels) | Item |
  |---|---|---|---|---|---|
  | Footage (still) | AVLayer, source FootageItem (still) | image file | 2 image (`still`) | Still: Lavender UNVERIFIED | AE-PRJ-332 |
  | Footage (movie / image sequence) | AVLayer, source FootageItem | movie / sequence | 9 `video` / 8 `imageSeq` (bodymovin internal; not in Lottie spec, players ignore) | Video: Aqua UNVERIFIED | AE-PRJ-332 |
  | Audio | AVLayer, hasVideo false | audio file | 6 audio | Audio: Sea Foam UNVERIFIED | AE-PRJ-333 |
  | Solid | AVLayer, source SolidSource | solid item | 1 solid | Solid: Red UNVERIFIED | AE-PRJ-334 |
  | Text | TextLayer | none | 5 text | Text: Red UNVERIFIED | AE-PRJ-335 |
  | Shape | ShapeLayer | none | 4 shape | Shape: Blue UNVERIFIED | AE-PRJ-336 |
  | Null | AVLayer `nullLayer` true | 100×100 null solid | 3 null | Null Object: Red UNVERIFIED | AE-PRJ-337 |
  | Camera | CameraLayer | none | 13 camera | Camera: Pink UNVERIFIED | AE-PRJ-338 |
  | Light | LightLayer | none | 14 light (bodymovin internal; not exported) | Light: Pink UNVERIFIED | AE-PRJ-339 |
  | Adjustment | AVLayer `adjustmentLayer` true | comp-size white solid | 12 adjustment (bodymovin internal; not exported) | Adjustment: Purple UNVERIFIED | AE-PRJ-340 |
  | Precomp | AVLayer, source CompItem | comp | 0 precomp | the comp item's label (Composition: Sandstone) | AE-PRJ-341 |
  | Guide (switch on any layer) | `guideLayer` true | any | 11 guide (not exported) | unchanged | AE-PRJ-342 |
  | Photoshop / CINEMA 4D / 3D model / data | AVLayer / ThreeDModelLayer | file | — / — / — / 15 data | 3D Models / Other 3D: UNVERIFIED | AE-PRJ-343..346 |
  - Common defaults for every new layer: index 1 (top) or directly above the topmost selected layer (AE-PRJ-388 new-layer insertion); `startTime = 0` and `inPoint = 0` when Preferences > General "Create Layers at Composition Start Time" (在合成开始时间创建图层) is on (default on), otherwise both = CTI; Video on, Audio on (if hasAudio), Solo/Lock/Shy off, Quality Best, Sampling Bilinear, blend mode Normal, no track matte, no parent, 2D, Motion Blur and Frame Blending off, Effects active, comment empty, `isNameSet` false.
  - Synthetic layers (solid, text, shape, null, adjustment, camera, light) default to an out point at the comp end; footage layers to the source duration (stills: Preferences > Import > Still Footage, default "Length of Composition").
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** ARCHITECTURE.md 4.2 kinds: `Shape | Text | Solid | Footage | Null | Camera | Light | Adjustment | PreComp | Audio`; guide and adjustment are flags in AE (a solid, footage or precomp layer can be adjustment), so BAE models `adjustmentLayer`/`guideLayer` as bool attributes on visual layers and keeps "Adjustment" only as a creation preset. Lottie `ty` values per lottie-docs layers page: 0, 1, 2, 3, 4, 5, 6, 13, 15; others are bodymovin internals. Lottie `hd` = hidden (exporter writes it for guide/disabled layers UNVERIFIED).

### AE-PRJ-332 · Footage layer: still, movie, image sequence (素材图层)

- **What it does:** A layer whose source is an imported footage item; it shows the item's frames over time according to Interpret Footage.
- **Exact semantics:**
  - Size: `width × height` of the source (after Interpret Footage PAR; layer pixels are source pixels). Default Anchor Point `(w/2, h/2)`; Position `(compW/2, compH/2)` (or the drop point when dragged into the Composition panel; Shift snaps to centre/edges); Scale 100 %; Rotation 0; Opacity 100 %.
  - Name: source name (layer name empty → shows source name; AE-PRJ-388).
  - Duration: movie/sequence = source duration (layer may be trimmed but not extended past the source unless time-remapped; AE-PRJ-379 timing); still = Still Footage preference (default length of comp), and stills can be extended indefinitely.
  - Property groups: Marker, Time Remap (movies, sequences; not stills UNVERIFIED), Motion Trackers, Masks, Effects, Essential Properties (no), Transform, Layer Styles, Geometry/Material Options (when 3D), Audio (when the source has audio).
  - Movie frames are sampled per AE-PRJ-373 layer-time formula; the source frame index is `floor(sourceTime * sourceFps + ε)`.
- **Priority:** P0
- **Owner:** bae-model (+ bae-media, bae-render)
- **Notes:** Lottie: still images are `ty: 2` with `refId` to an image asset (`assets[].p`, `u`, `w`, `h`); movies/sequences are not part of the Lottie spec. Decoding and interpretation: project-comp-layers.md [prj-a] and bae-media.

### AE-PRJ-333 · Audio-only layer (音频图层)

- **What it does:** A layer whose source has audio but no video; it contributes only to the comp's audio mix.
- **Exact semantics:**
  - `hasVideo = false`; the Video switch column is empty; it never renders pixels and cannot be a track matte. It has no Transform group, so it is useless as a parent; UNVERIFIED: whether AE lists it in the Parent menu at all (BAE: exclude it).
  - Property groups: Marker, Time Remap, Effects (audio effects only), Audio > Audio Levels (音频电平) (TwoD, dB, default [0, 0], range -192..+24 dB UNVERIFIED). No Transform, Masks or Layer Styles.
  - Timeline shows waveform under Audio > Waveform (UI).
- **Priority:** P1
- **Owner:** bae-model (+ bae-media)
- **Notes:** Lottie `ty: 6` with `au` audio settings (`lv` level) and `refId` to a sound asset. Audio preview/mixing: ui-preview-render-io.md [ui-b]; Audio effects: effects.md [fx-c].

### AE-PRJ-334 · Solid layer (纯色图层)

- **What it does:** A layer whose source is a solid-colour footage item, used for backgrounds, effect hosts and masks.
- **Exact semantics:** Layer > New > Solid (Ctrl+Y / Cmd+Y) opens Solid Settings (纯色设置):
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Name (名称) | string | "<Colour name> Solid N" e.g. "Black Solid 1" (黑色 纯色 1) UNVERIFIED zh format | — | — | colour name derived from the colour |
  | Width / Height (宽度 / 高度) | int | comp size | 1..30000 | px (entry also in the chosen Units) | Lock Aspect Ratio checkbox |
  | Units (单位) | enum | pixels | pixels, inches, millimeters, % of composition | — | dialog-only |
  | Pixel Aspect Ratio (像素长宽比) | enum | comp PAR | AE-PRJ-304 list | — | |
  | Make Comp Size (制作合成大小) | button | — | — | — | sets width/height/PAR to the comp's |
  | Color (颜色) | RGB | last used; UNVERIFIED: fresh-install value | 0..1 | — | |
  - Creates a solid item in the project's Solids (纯色) folder and a layer using it. Layer size = solid size; Anchor `(w/2, h/2)`; Position comp centre.
  - Layer > Solid Settings (Ctrl+Shift+Y) on a solid layer edits the source; "Affect all layers that use this solid" (影响使用此纯色的所有图层) checked → modify the item in place; unchecked → create a new solid item for this layer only.
  - Property groups: Marker, Masks, Effects, Transform, Layer Styles, Geometry/Material Options (3D). No Time Remap or Audio.
- **Priority:** P0
- **Owner:** bae-model (+ bae-motion)
- **Notes:** Solid footage items and the Solids folder (incl. "Enhanced solids folder organization") are owned by project-comp-layers.md [prj-a]. Lottie `ty: 1` with `sc` (colour hex string), `sw`, `sh`.

### AE-PRJ-335 · Text layer (文本图层)

- **What it does:** A sourceless vector layer that renders a TextDocument.
- **Exact semantics:**
  - Create: Layer > New > Text (Ctrl+Alt+Shift+T) or click with the Type tool (point text) / drag (paragraph text). Via menu the insertion point is the centre of the Composition viewer.
  - Defaults: Anchor Point `(0, 0)` (the text baseline origin); Position = click point, or comp centre via the menu; layer name = the text typed (first line UNVERIFIED), updating live until the layer is renamed (`isNameSet` false).
  - Property groups: Text (ADBE Text Properties: Source Text, Path Options, More Options, Animators), Masks, Effects, Transform, Layer Styles, Geometry Options (Advanced 3D / CINEMA 4D), Material Options (3D).
  - Always rasterized from vectors at render resolution. The Collapse switch is shown as Continuously Rasterize (effect on render order UNVERIFIED, AE-PRJ-362; cmp-a "collapse/continuous rasterize"). `width`/`height` are not meaningful; bounds come from `sourceRectAtTime`.
- **Priority:** P0
- **Owner:** bae-text (+ bae-model)
- **Notes:** Everything about text content and layout: text.md. Lottie `ty: 5`, data in `t` (`t.d` document keyframes, `t.p` path, `t.m` more options, `t.a` animators).

### AE-PRJ-336 · Shape layer (形状图层)

- **What it does:** A sourceless vector layer whose content is a tree of shape groups, paths, fills, strokes and operators.
- **Exact semantics:**
  - Create: Layer > New > Shape Layer (empty "Shape Layer N" (形状图层 N)), or draw with a shape/pen tool with no shape layer selected.
  - Defaults: Anchor Point `(0, 0)`; Position `(compW/2, compH/2)`; drawn shapes are placed so they appear where drawn (path coordinates relative to the layer origin). AE 2020 (17.0)+ "Shape Layer" names, label Blue UNVERIFIED.
  - Property groups: Contents (ADBE Root Vectors Group), Masks, Effects, Transform, Layer Styles, Geometry Options (Advanced 3D / CINEMA 4D), Material Options (3D).
  - Always rasterized from vectors at render resolution. The Collapse switch is shown as Continuously Rasterize (effect on render order UNVERIFIED, AE-PRJ-362).
- **Priority:** P0
- **Owner:** bae-vector (+ bae-model)
- **Notes:** Content model: shapes.md. Lottie `ty: 4`, `shapes[]`.

### AE-PRJ-337 · Null object (空对象)

- **What it does:** An invisible layer used as a parent, controller or expression host; it has transforms but never renders.
- **Exact semantics:**
  - Create: Layer > New > Null Object (Ctrl+Alt+Shift+Y). Name "Null N" (空 N); AE creates a 100×100 px null source in the Solids folder UNVERIFIED (shown as "Null N" item).
  - Defaults: size 100×100; Anchor Point `(0, 0)` (the top-left corner of the 100×100 box, a long-standing AE quirk users complain about; community-reported, UNVERIFIED by Adobe docs); Position `(compW/2, compH/2)` so the box's top-left corner sits at the comp centre UNVERIFIED (some versions offset Position by (-50, -50) so the box is centred); label Red UNVERIFIED.
  - Rendering: never rendered in the output or as a track matte (contributes no pixels; a null used as a matte acts as fully transparent UNVERIFIED); the Composition viewer draws its bounds as a 100×100 outline with handles.
  - Opacity exists but affects nothing (not inherited by children, AE-PRJ-396 parenting). Effects can be applied (typically Expression Controls); masks UNVERIFIED.
  - Can be 3D (no Material Options UNVERIFIED). Video switch: toggling off still lets children follow it (transform evaluation does not depend on Video, AE-PRJ-396).
  - Converting: any AV layer can be made null via scripting `nullLayer` is read-only; UI has no switch.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting: `Layer.nullLayer` (read-only), `layers.addNull(duration)`. Lottie `ty: 3` (players skip drawing). BAE decision: nulls need no source item; store as `Layer { kind: Null }` with fixed 100×100 bounds and create no project item (import of AEP solids-folder nulls maps them).

### AE-PRJ-338 · Camera layer (摄像机图层)

- **What it does:** A 3D-only layer that defines the viewpoint, lens and depth of field for the comp's 3D layers.
- **Exact semantics:**
  - Create: Layer > New > Camera (Ctrl+Alt+Shift+C) → Camera Settings dialog (type One-Node / Two-Node, preset 50 mm default, zoom, DOF...; defined in compositing-3d-color.md [cmp-b]). Name "Camera N" (摄像机 N).
  - Transform group (no Anchor Point, Scale, Opacity): Point of Interest (目标点; Two-Node only; matchName `ADBE Anchor Point`, default comp centre `(w/2, h/2, 0)`), Position (default `(w/2, h/2, -zoom)` UNVERIFIED, see cmp-b), Orientation, X/Y/Z Rotation.
  - Property groups: Marker, Transform, Camera Options (ADBE Camera Options Group). No masks, effects, styles, audio, time remap.
  - Always 3D (no 3D switch). It has a Video switch; turning it off makes the camera inactive (`activeCamera` rule, AE-PRJ-315).
  - Auto-Orient options: Off, Orient Along Path, Orient Towards Point of Interest (AE-PRJ-408).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-render)
- **Notes:** Scripting: `CameraLayer`, `layers.addCamera(name, centerPoint)`. Lottie `ty: 13` with `pe` (perspective) and `ks` (`a` as point of interest). Default label Pink UNVERIFIED.

### AE-PRJ-339 · Light layer (灯光图层)

- **What it does:** A 3D-only layer that lights 3D layers (and casts shadows) in the comp.
- **Exact semantics:**
  - Create: Layer > New > Light (Ctrl+Alt+Shift+L) → Light Settings dialog. Light Type menu (AE order): Parallel (平行), Spot (聚光), Point (点), Ambient (环境), Environment (环境光（译名待核）; Advanced 3D only, AE-PRJ-347; `LightType.ENVIRONMENT` added to scripting in AE 24.3). Scripting `LightType.PARALLEL / SPOT / POINT / AMBIENT / ENVIRONMENT`.
  - Name "<Type> Light N" (e.g. "Spot Light 1") UNVERIFIED (older versions "Light N").
  - Transform: Point of Interest (Parallel, Spot), Position (all but Ambient), Orientation + X/Y/Z Rotation (all but Ambient; Point lights ignore rotation). Ambient lights have no Transform group UNVERIFIED.
  - Property groups: Marker, Transform, Light Options (ADBE Light Options Group): Intensity, Color, Cone Angle, Cone Feather, Falloff, Radius, Falloff Distance, Casts Shadows, Shadow Darkness, Shadow Diffusion (per type; cmp-b).
  - Auto-Orient options: Off, Orient Along Path, Orient Towards Point of Interest.
  - Light Settings also offers "Light Source" (layer) for Environment type (`LightLayer.lightSource`).
- **Priority:** P0 data (render semantics in cmp-b are P1).
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `LightLayer.lightType`, `layers.addLight(name, centerPoint)`. Lottie: not in spec (bodymovin `ty: 14` not exported).

### AE-PRJ-340 · Adjustment layer (调整图层)

- **What it does:** A layer whose effects (and masks, opacity) apply to the composite of all layers below it instead of to its own pixels.
- **Exact semantics:**
  - Create: Layer > New > Adjustment Layer (Ctrl+Alt+Y / Cmd+Opt+Y): a comp-sized white solid named "Adjustment Layer N" (调整图层 N) in the Solids folder, with the Adjustment Layer switch on, label Purple UNVERIFIED, Anchor `(w/2, h/2)`, Position comp centre.
  - The Adjustment Layer switch (AE-PRJ-368) can be set on any AV layer (solid, footage, precomp, shape, text) to turn it into an adjustment layer; its own colour pixels are then ignored and only its alpha (masks, opacity, source alpha) acts as the effect's coverage.
  - Property groups: same as solid.
  - Rendering semantics: compositing-3d-color.md [cmp-a] "adjustment layers".
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.adjustmentLayer`. Lottie: not supported by the spec (bodymovin `ty: 12` internal).

### AE-PRJ-341 · Precomposition layer (预合成图层)

- **What it does:** A layer whose source is another comp, rendering that comp's output (nesting).
- **Exact semantics:**
  - Created by dragging a comp item into a comp, Precompose (AE-PRJ-401), or New Comp from Selection.
  - Size = nested comp `width × height`; Anchor `(w/2, h/2)`; Position comp centre; duration = nested comp duration (can be trimmed; extending past the nested comp's end shows nothing unless time-remapped).
  - Time mapping: `nestedTime = (compTime - layer.startTime) * 100/stretch` (plus time remap if enabled), sampled per the nested comp's frame-rate rules (AE-PRJ-311).
  - Property groups: Marker (copies of source comp markers, AE-PRJ-329), Time Remap, Masks, Effects, Essential Properties (AE-PRJ-420, when the source comp has them), Transform, Layer Styles, Geometry/Material Options (3D), Audio (if the nested comp has audio).
  - Collapse Transformations switch available (AE-PRJ-362).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-render)
- **Notes:** Lottie `ty: 0`, `refId` → `assets[].layers`, `w`, `h`, `tm` (time remap in seconds). Nesting rules (cycles) in AE-PRJ-403.

### AE-PRJ-342 · Guide layer (引导层)

- **What it does:** Any layer can be flagged as a guide layer: it is visible in its own comp's viewer but excluded from rendered output and from parent comps.
- **Exact semantics:**
  - Toggle: Layer > Guide Layer (引导层), or the layer context menu. Flag `guideLayer` (bool, default false).
  - A guide layer renders in the Composition viewer of its own comp. It is skipped when (a) rendering through the Render Queue, unless Render Settings > Guide Layers is "Current Settings" (当前设置), which renders guide layers of the top-level comp only (default "All Off" (全部关闭)（译名待核）), and (b) the comp is nested in another comp (guide layers in nested comps are never rendered).
  - It still evaluates (expressions may reference it; it can still be a parent).
  - Used as a track matte: UNVERIFIED (matte still works in its own comp viewer, not in output).
  - Timeline shows a guide-layer icon next to the layer name.
- **Priority:** P1
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.guideLayer`. Lottie: bodymovin skips guide layers (layerType 11). zh-CN: the zh-CN help (layers page) uses 引导层/引导图层; menu string in AE 2026 not checked. Source (Render Settings rule): https://helpx.adobe.com/after-effects/using/basics-rendering-exporting.html.

### AE-PRJ-343 · Photoshop file layers (Photoshop 图层)

- **What it does:** PSD files import as merged footage or as a composition whose layers each reference one Photoshop layer; Layer > New > Adobe Photoshop File creates a new PSD.
- **Exact semantics:**
  - Import as Composition / Composition – Retain Layer Sizes: each PS layer becomes a footage layer whose source item is "<layer name>/<file>.psd"; PS layer opacity, blend mode, visibility, layer masks, vector masks, layer styles (as AE Layer Styles), text (editable via Layer > Create > Convert to Editable Text) and adjustment layers (as AE effects where supported) are converted. Import options are prj-a.
  - Layer > New > Adobe Photoshop File creates a comp-sized blank PSD, imports it and adds it as the top layer; the source item re-reads the file when it changes on disk.
- **Priority:** P2
- **Owner:** bae-media (+ bae-model)
- **Notes:** BAE decision: PSD import is a P2 importer; layer-style conversion relies on AE-PRJ-409 Layer Styles.

### AE-PRJ-344 · CINEMA 4D file layers (CINEMA 4D 图层)

- **What it does:** A .c4d file imported as footage renders through the Cineware effect (applied automatically) using a Cinema 4D engine.
- **Exact semantics:** Layer > New > MAXON CINEMA 4D File creates a new .c4d; importing places a footage layer with the Cineware effect; render settings (Standard/Software/Current, camera selection, multi-pass) live in the effect.
- **Priority:** P2
- **Owner:** bae-plugin (+ bae-media)
- **Notes:** Requires Maxon's renderer; BAE will not ship it. BAE decision: import as an unsupported-footage placeholder preserving the reference.

### AE-PRJ-345 · 3D model layer (3D 模型图层)

- **What it does:** A layer whose source is an imported 3D model (GLB, glTF, OBJ, FBX; FBX with embedded animation was added after the initial release, UNVERIFIED: in which version), rendered by the Advanced 3D renderer.
- **Exact semantics:**
  - Introduced AE 24.0 (October 2023); scripting `ThreeDModelLayer` (subclass of AVLayer) AE 24.4.
  - Always 3D. Property groups: Marker, Time Remap (for animated models), Transform (Anchor Point, Position, Scale, Orientation, X/Y/Z Rotation, Opacity), Options for the model (animation selection, mesh/material settings UNVERIFIED), Material Options.
  - In Classic 3D comps a 3D model layer renders as a bounding box/nothing UNVERIFIED; the help says Advanced 3D is the only native renderer that can render models.
  - Default anchor: model's pivot/origin; position comp centre; scale chosen so the model fits the view (AE 25.x "Decluttered 3D Model Import" changes import behaviour UNVERIFIED).
- **Priority:** P2
- **Owner:** bae-render (+ bae-media, bae-model)
- **Notes:** Default label "3D Models" (Preferences > Labels). Lottie: unsupported.

### AE-PRJ-346 · Data layer: JSON, mgJSON, CSV, TSV (数据图层)

- **What it does:** Data files import as footage; adding a data item to a comp creates a layer exposing its data (mgJSON/CSV/TSV as data streams; plain JSON only through expressions).
- **Exact semantics:**
  - Supported inputs: JSON, MGJSON, CSV, TSV, BVH (per help). Plain `.json` footage is not shown as streams; expressions read it via `footage("x.json").sourceData`. mgJSON/CSV/TSV layers show a Data group with dynamic and static streams (keyframed data, one stream per column/property) that can be linked with expressions or converted to keyframes.
  - Data layers do not render.
- **Priority:** P2
- **Owner:** bae-model (+ bae-media, bae-anim)
- **Notes:** Data-driven animation added in AE 15.1 (2018) UNVERIFIED. Lottie `ty: 15` data layer (bodymovin `data`). Expression access: transform-keyframes-expressions.md [ani-b] "data footage".

### AE-PRJ-347 · Environment light (环境光)

- **What it does:** A light type that uses an image (typically an HDRI equirectangular footage layer) to light 3D layers with image-based lighting, reflections and backgrounds in Advanced 3D comps.
- **Exact semantics:**
  - Light Type "Environment" (`LightType.ENVIRONMENT`, scripting AE 24.3; the feature shipped with Advanced 3D, UNVERIFIED: in AE 24.0). Light Options: Intensity, Color, Source (`LightLayer.lightSource`: until AE 25.1 only HDR/EXR layers; AE 25.2+ any 2D video, still or precomp layer in the same comp; a 3D layer is rejected with "Invalid light source specified"), Casts Shadows + Shadow Darkness.
  - With no Source, AE uses a built-in default environment map. Choosing a layer as Source turns that layer's Video switch off (user may re-enable it).
  - The default environment light casts no shadows; with Casts Shadows on, 3D layers inside the shadow volume cast voxel shadows (subject to their Casts/Accepts Shadows options).
  - There is no background-visibility option on the light: Layer > Light > Create Environment Light Background Layer adds a solid at the bottom of the stack with CC Environment linked to the light's Source and a 5 px Fast Box Blur.
  - Transform: Orientation/rotations only (rotate the environment).
  - Legacy: `AVLayer.environmentLayer` flags a layer as the environment in Ray-traced 3D comps (deprecated renderer).
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Rendering: compositing-3d-color.md [cmp-b] Advanced 3D. Sources: Adobe help "Create realistic reflections and ambient lighting in a 3D scene" (environment lights), ae-scripting `LightLayer.lightSource`.

## Layer property tree

### AE-PRJ-348 · Property tree structure (属性树)

- **What it does:** Every animatable or settable thing on a layer is a node in a tree: leaf Properties under PropertyGroups, rooted at the layer. Expressions, scripts, AEP files and the Timeline all address properties through this tree.
- **Exact semantics:**
  - Node kinds (`PropertyType`): `PROPERTY` (leaf with a value), `NAMED_GROUP` (fixed children, e.g. Transform; children addressed by name/matchName), `INDEXED_GROUP` (user-orderable list of same-kind children, e.g. Masks, Effects, shape Contents, text Animators; children addressed by 1-based index; users can add, delete, duplicate, rename and reorder them).
  - Every node has: `matchName` (stable, language-independent ID, e.g. `ADBE Opacity`), `name` (display name; localised; for indexed-group children user-renamable, e.g. "Mask 1"), `propertyIndex` (1-based position in parent), `propertyDepth` (0 = layer, 1 = top-level group...), `parentProperty`.
  - Flags on every node: `enabled` (eyeball for effects/masks/layer styles/shape items; for a layer = Video switch), `canSetEnabled`, `active`, `selected`, `elided` (organisational group not shown in the Timeline and not indenting its children, e.g. a text layer's internal group), `isModified` (changed since creation), `isEffect`, `isMask`.
  - Addressing: by path of matchNames or names or indices; expressions use display names or `("ADBE …")` matchNames or indices interchangeably (`layer.effect("Blur")("Blurriness")` ≡ `layer("ADBE Effect Parade")(1)(1)`).
  - Dynamic children: a group can add a child only if its kind accepts that matchName (`canAddProperty`); adding to an indexed group appends at the end unless inserted.
  - Twirl state (expanded/collapsed in the Timeline) is UI state, persisted per property UNVERIFIED.
- **Priority:** P0 the backbone of the data model and AE interop.
- **Owner:** bae-model
- **Notes:** Scripting `PropertyBase`, `Property`, `PropertyGroup`. BAE decision: each Prim attribute carries its AE matchName as metadata so AEP/scripting/expression addressing works without a separate mapping table; display names come from i18n. Lottie uses short keys instead of matchNames (`ks`, `masksProperties`, `ef`, `shapes`, `t`) but also writes `mn` (matchName) on effects, shapes and layers.

### AE-PRJ-349 · Layer top-level property groups and matchNames (图层属性组)

- **What it does:** Lists the fixed top-level groups of each layer kind, their matchNames and Timeline display order; this is the canonical interop table.
- **Exact semantics:** Layer matchNames: `ADBE AV Layer` (footage, solid, null, adjustment, precomp, audio), `ADBE Text Layer`, `ADBE Vector Layer` (shape), `ADBE Camera Layer`, `ADBE Light Layer`, 3D model layer `ADBE 3D Model Layer` UNVERIFIED.
  | Group (中文) | matchName | Exists on | Notes |
  |---|---|---|---|
  | Marker (标记) | `ADBE Marker` | all | MARKER property (AE-PRJ-327); hidden until markers exist |
  | Text (文本) | `ADBE Text Properties` | text | Source Text `ADBE Text Document`, Path Options `ADBE Text Path Options`, More Options `ADBE Text More Options`, Animators `ADBE Text Animators` (text.md) |
  | Contents (内容) | `ADBE Root Vectors Group` | shape | indexed group (shapes.md) |
  | Time Remap (时间重映射) | `ADBE Time Remapping` | AV layers with time-varying source (movie, sequence, precomp, audio, 3D model) | OneD seconds; only when enabled (AE-PRJ-376) |
  | Motion Trackers (动态跟踪器) | `ADBE MTrackers` | AV layers | tracker data (ui-a Tracker) |
  | Masks (蒙版) | `ADBE Mask Parade` | AV, text, shape | indexed; each `ADBE Mask Atom` (cmp-a) |
  | Effects (效果) | `ADBE Effect Parade` | AV, text, shape, audio (audio effects) | indexed (effects.md) |
  | Essential Properties (基本属性) | `ADBE Layer Overrides` | precomp layers whose comp has Essential Graphics controllers | contains overrides and Media Replacement `ADBE Layer Source Alternate`; version in AE-PRJ-420 |
  | Transform (变换) | `ADBE Transform Group` | all except audio (and ambient light) | see below |
  | Layer Styles (图层样式) | `ADBE Layer Styles` | AV, text, shape | shown only when a style is added (AE-PRJ-409) |
  | Geometry Options (几何选项) | `ADBE Extrsn Options Group` (text/shape: Bevel, Extrusion Depth…) or `ADBE Plane Options Group` (footage/solid: Curvature, Segments) | 3D layers in CINEMA 4D / Advanced 3D comps | hidden in Classic 3D |
  | Material Options (材质选项) | `ADBE Material Options Group` | 3D AV/text/shape layers | cmp-b |
  | Camera Options (摄像机选项) | `ADBE Camera Options Group` | camera | Zoom `ADBE Camera Zoom`, Depth of Field, Focus Distance, Aperture, Blur Level, Iris Shape… |
  | Light Options (灯光选项) | `ADBE Light Options Group` | light | Intensity `ADBE Light Intensity`, Color `ADBE Light Color`, Cone Angle… |
  | Audio (音频) | `ADBE Audio Group` | layers with audio | Audio Levels `ADBE Audio Levels` (TwoD, dB) |
  | Data (数据) | UNVERIFIED | data layers | AE-PRJ-346 |
  - Transform group children (matchName → value type, default): Anchor Point `ADBE Anchor Point` (ThreeD_SPATIAL; camera/light: Point of Interest), Position `ADBE Position` (ThreeD_SPATIAL; X/Y/Z Position `ADBE Position_0/_1/_2` OneD when separated), Scale `ADBE Scale` (ThreeD, % [100,100,100]), Orientation `ADBE Orientation` (ThreeD, 3D only), X Rotation `ADBE Rotate X`, Y Rotation `ADBE Rotate Y` (3D only), Rotation / Z Rotation `ADBE Rotate Z` (OneD, degrees), Opacity `ADBE Opacity` (OneD %, 0..100, default 100). 2D layers report 2D values in the UI but store 3 components (z ignored). Semantics and matrix order: transform-keyframes-expressions.md [ani-a].
  - Timeline display order (top to bottom): Time Remap (when enabled), Text / Contents, Masks, Effects, Essential Properties, Transform, Layer Styles, Geometry Options, Material Options, Audio. Scripting order of AVLayer top-level groups per the docsforadobe AVLayer match-name page: `ADBE Marker`, `ADBE Time Remapping`, `ADBE MTrackers`, `ADBE Mask Parade`, `ADBE Effect Parade`, `ADBE Layer Overrides`, `ADBE Transform Group`, … (exact numeric indices UNVERIFIED); BAE must address by matchName, not index.
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** Sources: docsforadobe "Match Names > Layer" pages (AVLayer, 3D Layer, Camera, Light, Text, Shape, Layer Styles). Lottie: `ks` (transform: `a`, `p` (or split `p.x`/`p.y`), `s`, `r`/`rx`/`ry`/`rz`, `or`, `o`, `sk`, `sa`), `masksProperties`, `ef`, `sy`, `shapes`, `t`, `tm`, `au`.

### AE-PRJ-350 · Property value types (属性值类型)

- **What it does:** Every leaf property has one value type that fixes its storage, interpolation and expression result type.
- **Exact semantics:** `PropertyValueType` (AE scripting) in documentation order:
  | Type | Value | Interpolation | Examples |
  |---|---|---|---|
  | NO_VALUE | none | — | effect group headers, Effect "Compositing Options" topic, Layer Styles headers |
  | ThreeD_SPATIAL | [x, y, z] f64 | spatial (motion path, spatial tangents, roving) + temporal | Position, Anchor Point, Point of Interest, 3D Point Control |
  | ThreeD | [x, y, z] f64 | temporal per dimension | Scale, Orientation |
  | TwoD_SPATIAL | [x, y] f64 | spatial + temporal | effect Point Control (e.g. Lens Flare "Flare Center"), Beam start/end points |
  | TwoD | [x, y] f64 | temporal | Audio Levels, Mask Feather |
  | OneD | f64 | temporal | Rotation, Opacity, sliders, angles, checkboxes (0/1), popups (1-based), Time Remap |
  | COLOR | [r, g, b, a] f64 in 0..1 (may exceed 1 in 32 bpc) | temporal per channel | Fill Color, Light Color |
  | CUSTOM_VALUE | opaque blob | not interpolable (hold) or effect-defined | Levels Histogram, Curves, gradient ramp data |
  | MARKER | MarkerValue | none (hold) | `ADBE Marker` |
  | LAYER_INDEX | int, 0 = none | hold | Layer popups in effects (e.g. Set Matte "Take Matte From Layer"); stores a layer reference, displayed as index. AE 26.5 adds a render-stage popup next to layer parameters: `LayerInputStageType.SOURCE` = 0 (before masks and effects), `ALL_EFFECTS` = -1 (masks + all effects), `ONLY_MASKS` = -2 (masks, no effects); scripting `layerInputStage`, `inputLayerAndStage` = [layerIndex, stageIndex], `getInputStageCycleSafeLimit()` |
  | MASK_INDEX | int, 0 = none | hold | Mask popups in effects (Stroke "Path") |
  | SHAPE | Shape {vertices, inTangents, outTangents, closed, featherSegLocs…} | path interpolation (vertex-wise) | Mask Path, shape Path |
  | TEXT_DOCUMENT | TextDocument | hold only | Source Text |
  - Integer-like OneD properties (checkbox, popup, layer/mask index) only allow Hold keyframes UNVERIFIED for checkbox/popup; AE shows hold-icon keys.
  - Boolean switches (Video, Shy…) are NOT properties; they are layer attributes and cannot be keyframed.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Expression result types: transform-keyframes-expressions.md [ani-b]. BAE decision: LAYER_INDEX stores a layer ID internally (AE stores the layer ID too and shows the index UNVERIFIED) so reordering does not break the reference. 32 bpc colours: compositing-3d-color.md [cmp-b].

### AE-PRJ-351 · Property attributes and flags (属性特性)

- **What it does:** Per-property metadata that governs what UI and edits are allowed: keyframing, expressions, spatiality, separation, units and value limits.
- **Exact semantics:**
  | Attribute | Type | Meaning |
  |---|---|---|
  | canVaryOverTime | bool | may have keyframes or an expression (stopwatch shown). False for e.g. effect "Compositing Options" topic headers, some popups UNVERIFIED |
  | isTimeVarying | bool | has keyframes or an enabled expression (implies canVaryOverTime) |
  | isSpatial | bool | value is a position: motion path and spatial interpolation apply |
  | dimensionsSeparated / isSeparationLeader / isSeparationFollower / separationDimension / separationLeader | bool / int / ref | Separate Dimensions state; Position leads X/Y/Z Position followers (ani-a) |
  | canSetExpression | bool | an expression may be set (false e.g. for Marker in older versions, for MASK_INDEX UNVERIFIED) |
  | expression / expressionEnabled / expressionError | string / bool / string | ani-b |
  | unitsText | string | "pixels", "degrees", "percent", "seconds", "dB" … (display only) |
  | hasMin / minValue, hasMax / maxValue | bool / number | hard limits enforced on set (values outside throw in scripting; UI clamps). E.g. Opacity 0..100; effect limits per parameter in effects.md |
  | numKeys | int | keyframe count |
  | alternateSource / canSetAlternateSource | item | Media Replacement for MOGRT (AE 18.0) |
  | essentialPropertySource | ref | for Essential Properties overrides, the original property (AE 22.0) |
  | propertyParameters | string[] | item strings of any dropdown-menu property, incl. custom Dropdown Menu Control items (getter added AE 26.0; setter `setPropertyParameters()` AE 17.0.1) |
  - Slider ranges vs limits: many effect parameters have a UI slider range smaller than the valid range (e.g. slider 0..100 but typed values up to 1000); only hasMin/hasMax limits are enforced. The slider range is effect metadata (effects.md [fx-a]).
  - Values typed beyond limits are clamped; expression results beyond limits are clamped at evaluation UNVERIFIED (e.g. Opacity expression returning 150 renders as 100).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting names as in the table. BAE decision: keep these flags in the property schema (static per matchName) and not per instance, except state (dimensionsSeparated, expression, keyframes).

### AE-PRJ-352 · Reset property and group (重置)

- **What it does:** Restores a property or every property in a group to its default value.
- **Exact semantics:**
  - Group: click "Reset" (重置) next to the group name in the Timeline (Transform, Effect, Layer Style, Text animator groups…) or Effect Controls; resets every child property.
  - Single property: context menu on the property name > Reset.
  - If the property has keyframes, Reset does NOT delete them: it adds/sets a keyframe at the CTI with the default value. Without keyframes it sets the static value. Expressions are kept UNVERIFIED.
  - Defaults are layer-dependent where AE defines them so: Anchor Point resets to the layer's source centre (or (0,0) for text/shape), Position to the comp centre UNVERIFIED (AE resets Position to comp centre for AV layers).
  - Resetting an effect resets its parameters but not its enabled state or name.
  - Related: double-click Pan Behind tool resets Anchor Point to the layer's default location; Alt/Opt+double-click additionally moves the layer to the comp centre.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** One undo step per reset. Implementation: defaults come from the property schema; layer-dependent defaults are computed by a function of the layer (source size, comp size).

## Layer common attributes

### AE-PRJ-353 · Layer name and source name (图层名称、源名称)

- **What it does:** A layer has an optional user-set name; when unset it displays its source's name. The Timeline column toggles between Layer Name and Source Name views.
- **Exact semantics:**
  - Stored: `name: Option<String>`. `isNameSet = name.is_some()`; for sourceless layers (text, shape, camera, light, null? UNVERIFIED) AE always reports `isNameSet = true` because a name is generated at creation ("Shape Layer 1", "Camera 1"); text layers are special: the name tracks the first line of the Source Text until renamed.
  - Displayed name in Layer Name view: `name` if set, else `[sourceName]` in square brackets. Brackets mean "name taken from source"; renaming removes them; clearing the name (rename to empty) restores the source name UNVERIFIED.
  - Source Name view shows the source item name regardless (for sourceless layers the layer name).
  - Rename: select layer, Enter/Return, type, Enter. Also for property groups that allow it (masks, effects, shape groups, animators).
  - Changing the source item name updates every layer without its own name. Replace Source (AE-PRJ-404) keeps a set name.
  - `isNameFromSource` (AVLayer) = !isNameSet && source != null.
  - Names need not be unique within a comp; `thisComp.layer("name")` resolves to the topmost (lowest index) match.
  - Renaming a layer updates expressions in the project that reference it by name (`thisComp.layer("Old")` → `"New"`) UNVERIFIED version: AE 17.x+ does this for references within the same project; earlier versions broke them.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `Layer.name`, `isNameSet`, `AVLayer.isNameFromSource`. Lottie `nm` (always explicit; exporters write the displayed name). BAE decision: expressions reference layers by ID internally and render names on display so renames never break.

### AE-PRJ-354 · Layer comment (注释)

- **What it does:** Free-text note per layer shown in the Timeline Comments column.
- **Exact semantics:** `comment: String`, default "". Edited by clicking in the Comments column (hidden by default). Not used by rendering; searchable in the Timeline search field. Copied by Duplicate/Split/copy-paste.
- **Priority:** P2
- **Owner:** bae-model
- **Notes:** Scripting `Layer.comment`. Not in Lottie. Used by tagging scripts.

### AE-PRJ-355 · Layer index and layer ID (图层索引、图层 ID)

- **What it does:** `index` is the 1-based stacking position (1 = top); `id` is a persistent unique identifier that survives reordering.
- **Exact semantics:**
  - `index ∈ 1..numLayers`, contiguous, recomputed after every insert/delete/reorder. Displayed in the Timeline "#" column. Expressions `thisLayer.index`, `thisComp.layer(i)`.
  - `id`: integer unique within the project and persistent across save/reload (scripting `Layer.id`, read-only, AE 22.0; lookup `app.project.layerByID(id)`). Not shown in the UI. When a project is imported into another project, all its layers get new IDs.
  - Parent links, track matte links, LAYER_INDEX effect parameters and expression pick-whip links (when written by AE as `thisComp.layer("name")`) refer to layers; AE stores parent/matte/LAYER_INDEX by internal ID so reordering keeps them, while expressions refer by name or index text.
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** Lottie: `ind` (any integer, unique within a comp; exporters write index) and `parent` (an `ind`). BAE: Prim path uses ID, display uses index.

### AE-PRJ-356 · Layer selection state (图层选择状态)

- **What it does:** Which layers (and properties/keyframes) are selected in a comp; commands operate on the selection.
- **Exact semantics:** `selected: bool` per layer, per property and per keyframe; selection is per comp and saved in the project (AE persists the selection UNVERIFIED). Locked layers cannot be selected via Composition panel or Timeline clicks. Ctrl+A selects all layers, Ctrl+Shift+A deselects; Ctrl+Up/Down selects the next layer above/below (ui-preview-render-io.md [ui-b]).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Scripting `Layer.selected`, `CompItem.selectedLayers` (0-based order of selection), `selectedProperties`. BAE decision: selection lives in the `session` sublayer (ARCHITECTURE.md 4.3), never in the shared document, so collaborators do not fight over it; selection ORDER is kept because Sequence Layers, Precompose and New Comp from Selection depend on it.

### AE-PRJ-357 · Video switch (视频)

- **What it does:** The eyeball toggles whether the layer's visual contributes to previews and renders.
- **Exact semantics:**
  - `enabled: bool`, default true. For adjustment layers: whether their effects apply. For cameras: whether the camera can be the active camera. For lights: whether the light lights the scene.
  - A disabled layer still evaluates its transforms and expressions (children follow it; expressions can sample it). It can still be used as a track matte: assigning a track matte turns the matte layer's Video off and the matte still works (in all versions; AE 23.0+ lets the user turn the matte's Video back on to also show it).
  - `active(t) = enabled && soloRule(layer) && inPoint <= t < outPoint` (scripting `Layer.active`, `activeAtTime(t)`).
  - Layer > Switches > Show All Video; Hide Other Video (disables all but selected).
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `Layer.enabled`. Lottie `hd: true` for hidden layers (exporters often omit disabled layers instead).

### AE-PRJ-358 · Audio switch (音频)

- **What it does:** The speaker icon toggles whether the layer's audio is heard in previews and included in renders.
- **Exact semantics:** `audioEnabled: bool`, default true; only shown when `hasAudio`. `audioActive(t) = audioEnabled && audioSoloRule && in span`. No effect on visuals.
- **Priority:** P1
- **Owner:** bae-model (+ bae-media)
- **Notes:** Scripting `AVLayer.audioEnabled`, `audioActive`, `audioActiveAtTime()`.

### AE-PRJ-359 · Solo switch (独奏)

- **What it does:** Soloing isolates layers: when any layer of a category is soloed, only soloed layers of that category render.
- **Exact semantics:**
  ```
  // categories: visual (AV layers with video, text, shape, solid, null, adjustment), light, audio
  anySoloVisual = any(L.solo for L in comp if L is visual)
  anySoloLight  = any(L.solo for L in comp if L is light)        UNVERIFIED (cameras: solo has no effect)
  anySoloAudio  = any(L.solo for L in comp if L.hasAudio)
  visualRenders(L) = L.enabled && (!anySoloVisual || L.solo)
  lightOn(L)       = L.enabled && (!anySoloLight  || L.solo)
  audible(L)       = L.audioEnabled && (!anySoloAudio || L.solo)
  ```
  - Multiple solos combine as a union. Alt/Opt-click a Solo switch: solo this layer and unsolo all others.
  - The Video switch of non-soloed layers is drawn dimmed (UI).
  - Solo affects previews and renders; Render Settings "Solo Switches" (独奏开关): Current Settings (当前设置) (default) or All Off (全部关闭) ignores solos for that render.
  - Solo of a layer inside a precomp affects only that precomp (solo is per comp).
  - Track matte layers: soloing a layer that uses a matte still renders its matte (the matte is not "rendered" as visual output) UNVERIFIED.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `Layer.solo`. Preference "Solo Switches Affect Nested Comps" does not exist; AE help: soloing excludes other layers of the same type; lights and audio layers are unaffected by soloing a video layer.

### AE-PRJ-360 · Lock switch (锁定)

- **What it does:** Prevents a layer from being selected or edited.
- **Exact semantics:** `locked: bool`, default false. A locked layer cannot be selected in the Composition panel or Timeline; attempts make it flash; its properties cannot be changed by UI (scripts also cannot change most attributes of a locked layer; Scale Composition.jsx fails on locked layers). Commands that act on "all layers" (Lift/Extract, Select All) skip locked layers. Layer > Switches > Lock (Ctrl+L / Cmd+L) locks the selection; Unlock All Layers is Ctrl+Shift+L / Cmd+Shift+L (AE shortcut reference). Rendering unaffected.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `Layer.locked`. BAE decision: Agent/collaborator commands must also respect Lock (Command validation rejects edits to locked layers unless the command unlocks first).

### AE-PRJ-361 · Shy switch and Hide Shy Layers (消隐)

- **What it does:** Marks layers to be hidden from the Timeline layer outline when the comp's Hide Shy Layers switch is on.
- **Exact semantics:** `shy: bool`, default false. Visible in Timeline iff `!(comp.hideShyLayers && layer.shy)`. Never affects rendering, previews or selection via the Composition panel (shy layers can still be clicked in the viewer UNVERIFIED). Layer > Switches > Shy toggles; Timeline panel menu > Hide Shy Layers.
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `Layer.shy`, `CompItem.hideShyLayers`. zh-CN AE 2026 UI: 消隐 (glossary); some help pages say 隐蔽.

### AE-PRJ-362 · Collapse Transformations / Continuously Rasterize switch (折叠变换 / 连续栅格化)

- **What it does:** One switch with two meanings: for precomp layers it collapses the nested comp's transforms into the parent's (no intermediate flattening); for vector-source layers (Illustrator/PDF/EPS/SVG footage) it re-rasterizes vectors at the transformed resolution every frame.
- **Exact semantics:**
  - `collapseTransformation: bool`, default false. Settable when `canSetCollapseTransformation`. Per the AE help (Layers page, switch list), the switch means Collapse Transformations for precomp layers and Continuously Rasterize for shape layers, text layers and vector-footage layers (Illustrator, PDF, EPS, SVG). The precompose help page states that text and shape layers are always continuously rasterized; UNVERIFIED: for them the switch then only changes the render order (with it on, masks/effects are applied after the layer transform, like other continuously rasterized layers) rather than sharpness; details in cmp-a.
  - Data only here. Rendering consequences (blend modes passing through, 3D layers interleaving with the parent comp, mask/effect order change, frame boundary removal, adjustment layers inside collapsed precomps) are specified in compositing-3d-color.md [cmp-a] "collapse/continuous rasterize".
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.collapseTransformation`, `canSetCollapseTransformation`. Lottie `ct` (0/1; older `cp`).

### AE-PRJ-363 · Quality switch (品质)

- **What it does:** Sets how precisely the layer is rendered: Best, Draft or Wireframe.
- **Exact semantics:**
  | Value (中文) | Scripting | Meaning |
  |---|---|---|
  | Best (最佳) | `LayerQuality.BEST` | subpixel positioning, anti-aliasing, full effect precision; sampling per AE-PRJ-364 |
  | Draft (草图) | `LayerQuality.DRAFT` | no anti-aliasing, no subpixel positioning (positions rounded to pixels), some effects approximate |
  | Wireframe (线框) | `LayerQuality.WIREFRAME` | layer drawn as its bounding box outline only |
  - Default Best. Clicking the Quality switch toggles among Best (bilinear) → Draft → Best (bicubic) → back to Best (bilinear) (order as listed in the help); Layer > Quality (图层 > 质量) offers all of these plus Wireframe. Ctrl+U / Ctrl+Shift+U / Ctrl+Alt+Shift+U (Cmd on macOS) set Best / Draft / Wireframe for the selection (AE shortcut reference lists the three shortcuts in that order).
  - Render Settings "Quality" (Current Settings / Best / Draft / Wireframe) can override it for a render.
  - Duplicated or split layers keep the Quality setting.
- **Priority:** P0 (Best/Draft); Wireframe P2.
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.quality`. Rendering details: compositing-3d-color.md [cmp-a] "quality/sampling/subpixel". zh-CN help: switch 品质, menu 图层 > 质量, values 最佳 / 草图 / 线框, sampling 双线性 / 双立方 (Source: https://helpx.adobe.com/cn/after-effects/using/layers.html).

### AE-PRJ-364 · Sampling: Bilinear / Bicubic (采样：双线性 / 双立方)

- **What it does:** For Best-quality layers, chooses the resampling filter used when the layer is transformed.
- **Exact semantics:** `samplingQuality ∈ {BILINEAR (双线性) default, BICUBIC (双立方)}`; AE 12.0 (CC). Alt+B / Opt+B = Best/Bilinear; Alt+Shift+B / Opt+Shift+B = Best/Bicubic. Ignored in Draft/Wireframe. Effects' internal transforms still use bilinear unless the effect implements otherwise.
- **Priority:** P1
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.samplingQuality`. Filter kernels: compositing-3d-color.md [cmp-a].

### AE-PRJ-365 · Effect switch (效果)

- **What it does:** The fx switch toggles all effects of a layer on/off for rendering without changing each effect's own on/off state.
- **Exact semantics:** `effectsActive: bool`, default true; shown only when the layer has effects. Effective `effect.on = layer.effectsActive && effect.enabled`. Layer styles are not affected UNVERIFIED. Masks are not affected.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.effectsActive`. Per-effect eyeball: `PropertyBase.enabled` (effects.md [fx-a]).

### AE-PRJ-366 · Frame Blending switch (帧混合)

- **What it does:** Per-layer choice of how to synthesize in-between frames when a footage/precomp layer is time-stretched, remapped or conformed to a different frame rate.
- **Exact semantics:**
  | Value (中文) | Scripting | Lottie |
  |---|---|---|
  | Off (关) | `FrameBlendingType.NO_FRAME_BLEND` | — |
  | Frame Mix (帧混合) | `FrameBlendingType.FRAME_MIX` | — |
  | Pixel Motion (像素运动) | `FrameBlendingType.PIXEL_MOTION` | — |
  - Default Off. Clicking the switch cycles Off → Frame Mix → Pixel Motion → Off UNVERIFIED; Layer > Frame Blending > Off / Frame Mix / Pixel Motion.
  - Only movie, image-sequence and precomp layers can use it (stills, text, shapes, solids: switch not available).
  - Effective iff `comp.frameBlending && layer.frameBlendingType != NO_FRAME_BLEND` (AE-PRJ-315). `AVLayer.frameBlending` (read-only) reports the layer switch.
  - Math: transform-keyframes-expressions.md [ani-a] "frame blending"; rendering: cmp-a.
- **Priority:** P1 (Frame Mix); Pixel Motion P2.
- **Owner:** bae-model (+ bae-render)
- **Notes:** Render Settings Frame Blending override: ui-preview-render-io.md [ui-c].

### AE-PRJ-367 · Motion Blur switch (运动模糊)

- **What it does:** Per-layer opt-in to motion blur.
- **Exact semantics:** `motionBlur: bool`, default false. Effective iff `comp.motionBlur && layer.motionBlur && comp.shutterAngle > 0` (and Render Settings motion blur not "Off for All Layers"). Available on all visual layers (not camera/light/audio; camera motion blurs 3D layers that have the switch on). Comp settings: AE-PRJ-313.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-render)
- **Notes:** Scripting `AVLayer.motionBlur`. Lottie layer `mb` (boolean; lottie-web ignores it).

### AE-PRJ-368 · Adjustment Layer switch (调整图层)

- **What it does:** Turns any visual AV layer into an adjustment layer (AE-PRJ-340) or back.
- **Exact semantics:** `adjustmentLayer: bool`, default false (true for layers made by New > Adjustment Layer). Allowed on footage, solid, precomp, text and shape layers (shape/text adjustment layers are a common AE technique: the shape/text alpha becomes the effect coverage); not on camera, light, audio-only layers. UNVERIFIED: whether a null object can be switched to an adjustment layer. Toggling does not change effects, masks or transforms. Render semantics: cmp-a.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.adjustmentLayer`.

### AE-PRJ-369 · 3D Layer switch (3D 图层)

- **What it does:** Makes a layer participate in 3D space (Z position, X/Y rotation, orientation, cameras, lights).
- **Exact semantics:**
  - `threeDLayer: bool`, default false. Available on footage, solid, null, precomp, text, shape, adjustment layers; cameras, lights and 3D model layers are always 3D; audio-only layers cannot be 3D.
  - Turning ON: Position, Anchor Point and Scale gain a z component (Position z = 0, Anchor z = 0, Scale z = 100 %); adds Orientation [0,0,0], X Rotation 0, Y Rotation 0, Geometry Options, Material Options; Rotation is renamed Z Rotation (same value/keys).
  - Turning OFF: Orientation, X Rotation, Y Rotation, Geometry Options and Material Options are REMOVED with their values, keyframes and expressions (not restored by turning 3D on again); Position/Anchor/Scale keep their keyframes and expressions, but their z values are hidden and ignored.
  - Text with "Enable Per-character 3D" shows a distinct switch icon (`threeDPerChar`, text.md).
  - Layer > 3D Layer toggles for the selection.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim, bae-render)
- **Notes:** Scripting `AVLayer.threeDLayer`, `threeDPerChar`. Lottie `ddd: 1` on the layer (and on the root if any layer is 3D). BAE decision: keep removed 3D values in an undo-only snapshot so that 2D→3D right after 3D→2D can be restored by Undo, but not by re-toggling (AE parity).

### AE-PRJ-370 · Preserve Underlying Transparency (保留基础透明度)

- **What it does:** The "T" switch in the Modes column makes the layer paint only where the layers below already have opacity.
- **Exact semantics:** `preserveTransparency: bool`, default false; available on visual layers. Data only here; formula in compositing-3d-color.md [cmp-a] "preserve transparency".
- **Priority:** P1
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.preserveTransparency`. Not in Lottie.

### AE-PRJ-371 · Blending Mode attribute (混合模式)

- **What it does:** Stores how the layer is combined with the composite below it.
- **Exact semantics:** `blendingMode: enum`, default Normal. Menu groups in AE order (scripting `BlendingMode.*`, Lottie `bm` where it exists):
  | Group | Modes |
  |---|---|
  | Normal | Normal (正常) `NORMAL` bm 0, Dissolve (溶解) `DISSOLVE`, Dancing Dissolve (动态抖动溶解) `DANCING_DISSOLVE` |
  | Subtractive | Darken (变暗) `DARKEN` bm 4, Multiply (相乘) `MULTIPLY` bm 1, Color Burn (颜色加深) `COLOR_BURN` bm 7, Classic Color Burn (经典颜色加深) `CLASSIC_COLOR_BURN`, Linear Burn (线性加深) `LINEAR_BURN`, Darker Color (较深的颜色) `DARKER_COLOR` |
  | Additive | Add (相加) `ADD` bm 16, Lighten (变亮) `LIGHTEN` bm 5, Screen (屏幕) `SCREEN` bm 2, Color Dodge (颜色减淡) `COLOR_DODGE` bm 6, Classic Color Dodge (经典颜色减淡) `CLASSIC_COLOR_DODGE`, Linear Dodge (线性减淡) `LINEAR_DODGE`, Lighter Color (较浅的颜色) `LIGHTER_COLOR` |
  | Complex | Overlay (叠加) `OVERLAY` bm 3, Soft Light (柔光) `SOFT_LIGHT` bm 9, Hard Light (强光) `HARD_LIGHT` bm 8, Linear Light (线性光) `LINEAR_LIGHT`, Vivid Light (亮光) `VIVID_LIGHT`, Pin Light (点光) `PIN_LIGHT`, Hard Mix (实色混合) `HARD_MIX` bm 17 |
  | Difference | Difference (差值) `DIFFERENCE` bm 10, Classic Difference (经典差值) `CLASSIC_DIFFERENCE`, Exclusion (排除) `EXCLUSION` bm 11, Subtract (相减) `SUBTRACT`, Divide (相除) `DIVIDE` |
  | HSL | Hue (色相) `HUE` bm 12, Saturation (饱和度) `SATURATION` bm 13, Color (颜色) `COLOR` bm 14, Luminosity (发光度) `LUMINOSITY` bm 15 |
  | Matte | Stencil Alpha (模板 Alpha) `STENCIL_ALPHA`, Stencil Luma (模板亮度) `STENCIL_LUMA`, Silhouette Alpha (轮廓 Alpha) `SILHOUETTE_ALPHA`, Silhouette Luma (轮廓亮度) `SILHOUETTE_LUMA` |
  | Utility | Alpha Add (Alpha 添加) `ALPHA_ADD`, Luminescent Premul (冷光预乘) `LUMINESCENT_PREMUL` |
  - Shift+= / Shift+- cycles through modes for selected layers (ui-b). Formulas: compositing-3d-color.md [cmp-a] "blend modes & formulas".
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Chinese mode names follow AE zh-CN UI UNVERIFIED（译名待核） (verify against AE 2026). Scripting `AVLayer.blendingMode`.

### AE-PRJ-372 · Track Matte attribute (轨道遮罩)

- **What it does:** Stores which layer this layer uses as a matte and how (alpha/luma, normal/inverted).
- **Exact semantics:**
  | Field | Type | Default | Notes |
  |---|---|---|---|
  | trackMatteLayer | layer ref or null | null | AE 23.0+: any layer in the same comp chosen via the Track Matte pick whip/menu; before 23.0: implicitly the layer directly above |
  | trackMatteType | enum | NO_TRACK_MATTE | `ALPHA` (Alpha 遮罩), `ALPHA_INVERTED` (Alpha 反转遮罩), `LUMA` (亮度遮罩), `LUMA_INVERTED` (亮度反转遮罩) |
  - AE 23.0 UI: Track Matte column (layer menu + pick whip) plus two toggle buttons: Alpha/Luma and Normal/Inverted. Choosing a matte layer turns that layer's Video switch off (it still works as a matte). One matte layer can serve several layers.
  - Read-only derived: `hasTrackMatte`, `isTrackMatte`.
  - Constraints: the matte layer cannot be the layer itself; cycles (A mattes B mattes A) are prevented UNVERIFIED; matte must be in the same comp.
  - Legacy projects (pre-23.0): matte = layer at index-1; on open, AE converts to the explicit link.
  - Duplicate/Precompose rules: AE-PRJ-384, AE-PRJ-402. Render formulas: compositing-3d-color.md [cmp-a] "track mattes".
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Scripting `AVLayer.setTrackMatte(layer, type)`, `removeTrackMatte()`, legacy `trackMatteType`. Lottie: `tt` on the matted layer (1 alpha, 2 alpha inverted, 3 luma, 4 luma inverted), `td: 1` on the matte layer, `tp` = `ind` of the matte layer (newer exporters; default = layer above).

## Layer timing

### AE-PRJ-373 · Layer time model: startTime, inPoint, outPoint, stretch (图层时间)

- **What it does:** Defines where a layer sits in comp time, which part of it is visible, and how comp time maps to the layer's own (source) time.
- **Exact semantics:**
  | Field (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | startTime (开始时间) | time | 0 or CTI (AE-PRJ-388) | -10800..10800 | s (comp time) | comp time at which layer time 0 (source frame 0) occurs |
  | inPoint (入点) | time | = startTime for new layers | -10800..10800 | s (comp time) | first visible instant (inclusive) |
  | outPoint (出点) | time | in + duration | -10800..10800 | s (comp time) | end of visibility (exclusive) |
  | stretch (伸缩) | f64 | 100 | -9900..9900; values in (0, 1) are set to 1 and values in (-1, 0) to -1 (scripting doc); exactly 0 UNVERIFIED (treat as invalid) | % | negative = reversed playback |
  | duration (持续时间) | derived | | | s | `outPoint - inPoint` (Timeline Duration column) |
  ```
  k = stretch / 100
  layerTime(compTime) = (compTime - startTime) / k          // a.k.a. source time before time remap
  compTime(layerTime) = startTime + layerTime * k
  visible(compTime)   = inPoint <= compTime < outPoint      // half-open; evaluated at frame start times
  if timeRemapEnabled: sourceTime = TimeRemap.valueAt(compTime) else sourceTime = layerTime
  for k < 0: source time decreases as compTime increases (AE-PRJ-375)
  ```
  - Keyframes, layer markers and the time-remap curve are stored in layer time, so they move with startTime and scale with stretch. Expressions expose comp time (`key.time`, `time`) and layer time via `sourceTime()`/`thisLayer.sourceTime(t)` (ani-b).
  - Footage sampling: source frame = `floor(sourceTime * sourceFps + ε)` within `[0, sourceDuration)` (ε handles float error; BAE uses exact rationals).
  - The In/Out/Duration/Stretch Timeline columns edit these fields: typing In moves the whole layer (keeps duration) UNVERIFIED; Alt/Opt-click on In/Out moves that point to CTI (AE-PRJ-380); Ctrl/Cmd-click In/Out stretches the layer so that point reaches the CTI; clicking Duration or Stretch opens Time Stretch (AE-PRJ-374).
  - Alt+Home / Opt+Home moves the in points of selected layers to comp start; Alt+End to comp end (out point at comp end).
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting `Layer.startTime`, `inPoint`, `outPoint`, `stretch`, `time`. Lottie `st` (frames), `ip`, `op` (frames), `sr` (stretch as factor, 1 = 100 %). lottie-web: `layerTime = (compFrame - st) / sr`. Evaluation math: transform-keyframes-expressions.md [ani-a] "time remap/frame selection".

### AE-PRJ-374 · Time Stretch dialog (时间伸缩)

- **What it does:** Layer > Time > Time Stretch changes the playback speed of a whole layer (and its keyframes) by a factor, holding one point in place.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Stretch Factor (拉伸系数) | f64 | current stretch | -9900..9900 (|x| ≥ 1) | % | |
  | New Duration (新持续时间) | time | current duration × new/old | > 0 | timecode | linked to Stretch Factor: `newDur = oldDur * newStretch/oldStretch` |
  | Hold in Place (原位定格) | enum | Layer In-point | Layer In-point (图层入点), Current Frame (当前帧), Layer Out-point (图层出点) | — | |
  ```
  r = newStretch / oldStretch
  pivot = (hold == In) ? inPoint : (hold == Current) ? CTI : outPoint
  // every comp-time attribute T of the layer (inPoint, outPoint, startTime) maps
  T' = pivot + (T - pivot) * r
  // keyframes/markers are in layer time -> they follow automatically
  ```
  - Negative factor reverses the layer (like AE-PRJ-375 but with the hold point semantics).
  - The layer's frame sampling follows the new mapping; inter-frame results need frame blending (AE-PRJ-366).
  - Applies to every layer type, including sourceless text and shape layers, where it only rescales keyframe and marker timing (UNVERIFIED: camera and light layers).
  - With time remap enabled, Time Stretch scales the Time Remap keyframe timing too.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim, bae-model)
- **Notes:** Help: dialog fields "Stretch Factor", "New Duration", "Hold in Place" (zh-CN help: 拉伸系数, 新持续时间, 原位定格). Ctrl/Cmd-click on In/Out column: stretch to CTI. Precision: AE rounds New Duration to whole frames when typed in timecode UNVERIFIED.

### AE-PRJ-375 · Time-Reverse Layer (时间反向图层)

- **What it does:** Layer > Time > Time-Reverse Layer (Ctrl+Alt+R / Cmd+Opt+R) plays the layer backwards while keeping its in and out points.
- **Exact semantics:**
  ```
  stretch' = -stretch
  startTime' = inPoint + outPoint - startTime
  // inPoint and outPoint unchanged; keyframes, markers (stored in layer time) mirror around (in+out)/2
  ```
  - All keyframes of all properties reverse order (help). Applying twice restores the original.
  - The layer bar shows a red/hatched stripe for negative stretch (UI).
  - Adobe recommends reversing inside a precomp to keep keyframes un-reversed; "Time-Reverse Keyframes" (Animation > Keyframe Assistant) is the keyframe-only variant (ani-a).
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Scripting: no direct method; set `stretch` negative and adjust startTime. Lottie: `sr` negative is not supported by lottie-web UNVERIFIED; exporter should bake time remap instead.

### AE-PRJ-376 · Enable Time Remapping and the Time Remap property (启用时间重映射、时间重映射)

- **What it does:** Layer > Time > Enable Time Remapping (Ctrl+Alt+T / Cmd+Opt+T) adds a keyframable Time Remap property whose value is the source time shown at each comp time.
- **Exact semantics:**
  - Available when `canSetTimeRemapEnabled`: layers with a time-varying source (movie, image sequence, audio, precomp, 3D model). Not for stills, solids, text, shape, null, camera, light.
  - On enable: Time Remap (`ADBE Time Remapping`, OneD, units seconds, displayed as timecode/frames) gets two linear keyframes: at the layer in point with value = layerTime(inPoint), and at the layer out point with value = layerTime(outPoint) (for an untrimmed layer: 0 and the source duration) UNVERIFIED for trimmed layers.
  - After enabling, the layer's out point can be dragged beyond the source end (the layer becomes extendable both directions); after the last keyframe the value holds (constant extrapolation), so the last frame freezes.
  - Values are clamped at evaluation to `[0, sourceDuration - 1 frame]` for footage UNVERIFIED (precomps: beyond the nested comp's duration render transparent UNVERIFIED).
  - Disabling (toggle again) removes the property and all its keyframes; in/out points that exceed the source are clamped back UNVERIFIED.
  - Time Remap can have expressions (common: `loopOut()`), and the property is not affected by the Speed graph semantics beyond normal keyframes.
  - With Time Remap on, stretch still scales its key times (AE-PRJ-374) but the source time comes only from the Time Remap value.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim, bae-model)
- **Notes:** Scripting `AVLayer.timeRemapEnabled`. Lottie precomp `tm` (value in seconds of the nested comp, keyframable); lottie-web uses `tm` only for `ty: 0`. Evaluation: ani-a "time remap/frame selection".

### AE-PRJ-377 · Freeze Frame (冻结帧)

- **What it does:** Layer > Time > Freeze Frame shows the frame at the CTI for the layer's whole duration.
- **Exact semantics:**
  ```
  enable time remapping if needed; DELETE all existing Time Remap keyframes
  add one keyframe at CTI with value = layerTime(CTI) (current source time), interpolation Hold
  ```
  - Layer in/out points are unchanged. Applies to each selected layer that can time-remap.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Help confirms deletion of previous Time Remap keyframes. Lottie export: `tm` static value.

### AE-PRJ-378 · Freeze On Last Frame (在最后一帧上冻结)（译名待核）

- **What it does:** Layer > Time > Freeze On Last Frame holds the layer's last source frame until the end of the comp.
- **Exact semantics:**
  ```
  enable time remapping (keeping existing keys UNVERIFIED)
  lastT = time of the last source frame in comp time = compTime(sourceDuration - sourceFrameDur)
  add a Hold keyframe at lastT with value = sourceDuration - sourceFrameDur
  outPoint = comp.duration
  ```
- **Priority:** P2
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Introduced AE 16.1 (2019) UNVERIFIED. Help text describes "freeze the last frame of a layer until the end of the composition; sets time remapping keyframes and extends the layer".

### AE-PRJ-379 · Layer extent limits: stills versus time-based sources (图层时长限制)

- **What it does:** Defines how far a layer's in/out points can be dragged depending on its source.
- **Exact semantics:**
  | Source | Can extend in/out beyond source? |
  |---|---|
  | Movie, image sequence, audio, precomp, 3D model (time remap off) | No: `inPoint >= compTime(0)` and `outPoint <= compTime(sourceDuration)` (reversed: mirrored); trims only |
  | Same with time remap on | Yes, both directions, any length (frames hold) |
  | Still image, solid, null, adjustment (solid source), text, shape, camera, light | Yes, any length both directions; startTime is just a reference |
  | Image sequence interpreted as a single still (one file) | as still |
  - Looping footage (Interpret Footage > Loop N times) multiplies `sourceDuration` (prj-a).
  - In and out points are snapped to comp frames when edited in the UI; scripting can set fractional values.
  - A layer may have zero duration? No: minimum duration is one comp frame UNVERIFIED.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Lottie layers have no source-length constraint (`ip`/`op` free).

## Layer editing operations

### AE-PRJ-380 · Move layers in time (在时间上移动图层)

- **What it does:** Shifts whole layers (with keyframes and markers) earlier or later in comp time.
- **Exact semantics:**
  - Every move changes `startTime`, `inPoint`, `outPoint` by the same delta `d` (keyframes/markers follow because they are in layer time).
  | Gesture | Delta |
  |---|---|
  | Drag the duration bar (Shift snaps to CTI, markers, other layers' in/out, keyframes, work area) | drag amount, frame-quantised |
  | Alt/Opt+PageUp / PageDown | -1 / +1 comp frame |
  | Alt/Opt+Shift+PageUp / PageDown | -10 / +10 frames |
  | `[` | per selected layer: `d = CTI - inPoint` (each layer's in point to CTI) |
  | `]` | per selected layer: `d = CTI - outPoint` (each out point to CTI; out point = end of the frame at CTI UNVERIFIED: AE uses `CTI + 1 frame`) |
  | Shift+`[` | one `d` for all: `CTI - min(inPoint)` (keeps relative offsets) |
  | Shift+`]` | one `d` for all: `CTI(+1 frame) - max(outPoint)` |
  | Alt/Opt+Home | `d = 0 - inPoint` (in point to comp start) |
  | Alt/Opt+End | `d = comp.duration - outPoint` |
  | Typing a value in the In / Out column; Alt/Opt-click the In/Out value | moves the layer so that point equals the typed value / CTI |
  - Movement is not limited to the comp range (layers can start before 0 or after the end).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** Shortcuts from the AE help table (Selecting and arranging layers; Keyboard shortcuts). Scripting: set `startTime`.

### AE-PRJ-381 · Trim layer in/out points (修剪图层入点/出点)

- **What it does:** Changes only the in point or out point of layers, without moving the layer content in time.
- **Exact semantics:**
  | Gesture | Effect per selected layer |
  |---|---|
  | Drag the start/end of the duration bar (Shift snaps) | inPoint / outPoint follow the pointer, frame-quantised |
  | Alt/Opt+`[` | `inPoint = CTI` |
  | Alt/Opt+`]` | `outPoint = CTI + 1 frame` (the frame at CTI stays visible) UNVERIFIED |
  | Alt/Opt+Shift+`[` | trims/extends all selected layers by one shared delta so that the earliest in point = CTI |
  | Alt/Opt+Shift+`]` | same for the latest out point |
  | Layer panel Set In / Set Out buttons | same as Alt+[ / Alt+] in layer time |
  - Clamped by source extent (AE-PRJ-379) and `inPoint < outPoint` (minimum one frame).
  - startTime, keyframes and markers do NOT move. Keyframes outside the new span are kept (invisible).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Scripting `inPoint`/`outPoint`. Trimming never alters the footage item.

### AE-PRJ-382 · Slip edit (滑动编辑)

- **What it does:** For a trimmed time-based layer, shifts which source frames play inside the fixed in/out span by dragging the pale slip-edit bar (or dragging the layer with the Pan Behind tool in the Timeline).
- **Exact semantics:**
  ```
  d = drag delta (frames)
  startTime += d                       // source content moves
  inPoint, outPoint unchanged          // clamped so the span stays inside the source
  for each keyframe of the layer:
      if selected: stays attached to the source (comp time += d)
      else:        keeps its comp time  => layerTime -= d / k
  ```
  - Only for layers with a time-based source (no slip bar for stills/solids/text/shapes). Shift+F2 deselects keyframes but keeps the layer selected, so a slip leaves all keyframes in place.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Help: "Dragging the slip edit bar moves selected keyframes, but does not move unselected keyframes." Precompose keyframes behave the same.

### AE-PRJ-383 · Split Layer (拆分图层)

- **What it does:** Edit > Split Layer (Ctrl+Shift+D / Cmd+Shift+D) cuts selected layers at the CTI into two independent layers.
- **Exact semantics:**
  ```
  targets = selected layers, or ALL layers if none selected (locked layers skipped UNVERIFIED)
  for each L in targets with L.inPoint < CTI < L.outPoint:
      R = full copy of L (properties, keyframes, expressions, effects, masks, markers, switches, parent, matte link)
      L.outPoint = CTI ; R.inPoint = CTI
      position R: directly above L if Preferences > Composition "Create Split Layers Above Original Layer" (default on UNVERIFIED), else directly below
  ```
  - Both layers keep ALL keyframes at their original times (help).
  - Track mattes: the split layer keeps using the same matte layer; in legacy (pre-23) implicit mattes, AE also duplicates the matte so each piece keeps its matte directly above ("Any applied track mattes retain their order on top of the layer").
  - Naming: the new layer gets the original name with a number appended/incremented ("Title" → "Title 2") UNVERIFIED; unnamed footage layers stay unnamed.
  - Selection afterwards: the new layers are selected UNVERIFIED.
  - Children parented to L stay parented to L (the first piece) UNVERIFIED.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** One undo step. Help: "If no layer is selected when you select Edit > Split Layer, all layers are split at the current time."

### AE-PRJ-384 · Duplicate layer (重复图层)

- **What it does:** Edit > Duplicate (Ctrl+D / Cmd+D) copies selected layers in place, each copy directly above its original.
- **Exact semantics:**
  - Copies everything: properties, keyframes, expressions, effects, masks, layer styles, markers, switches, label, comment, parent link, timing. New layer ID.
  - Stacking: each copy is inserted directly above its original; the copies become selected and the originals are deselected.
  - Naming: if the layer has a set name ending in a number, the copy takes the next free number in the comp ("Shape Layer 1" → "Shape Layer 2" or higher if taken); a set name without number gets " 2" appended; unset names stay unset (copy shows `[source]`).
  - Track mattes (AE 23.0+ explicit links): a copy of a matted layer points to the same matte layer; if the matte layer is duplicated in the same operation, the copy points to the matte copy. Legacy implicit mattes: duplicating preserves "the relative ordering of the layer and its track matte" (copies of the pair are placed together).
  - Parent links: copy keeps the same parent; if the parent was duplicated in the same operation, copies link to the parent copy UNVERIFIED.
  - Expressions are copied verbatim: references by name resolve to the original targets (e.g. `thisComp.layer("Null 1")`); `thisLayer` refers to the copy.
  - Alt/Opt-drag of a layer in the Composition panel also duplicates (UNVERIFIED: a preference choosing between duplicating layers and camera navigation for Alt-drag in recent versions).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Scripting `Layer.duplicate()`. Exact name-increment algorithm UNVERIFIED (AE scans existing names).

### AE-PRJ-385 · Copy, cut and paste layers (复制、剪切、粘贴图层)

- **What it does:** Clipboard transfer of whole layers within or between comps.
- **Exact semantics:**
  - Copy (Ctrl+C) with layers selected and no property/keyframe selected copies whole layers; if a mask, effect or keyframe is selected only that component is copied (Shift+F2 deselects components first).
  - Paste (Ctrl+V): pasted layers keep their original startTime/in/out (comp times). Placement: directly above the topmost selected layer in the target comp; with no selection, at the top of the stack. Pasted order: top-to-bottom in the order the layers were selected before copying (first selected ends up on top).
  - Paste with Ctrl+Alt+V / Cmd+Opt+V ("Paste layers at current time" in the shortcut reference): same, but the pasted layers are moved in time so their in points are at the CTI; UNVERIFIED: whether several pasted layers keep their relative offsets (earliest in point at CTI) or all start at the CTI.
  - Cut (Ctrl+X) = copy + delete.
  - Cross-comp paste: sources are referenced (no new items); parent links to layers not pasted are dropped unless the target comp has the same layer? UNVERIFIED (AE keeps parent only when pasting into the same comp); track matte links to non-pasted layers are dropped (AE 23+) UNVERIFIED; expressions are copied verbatim and may break.
  - Cross-project / to text editors: AE puts a proprietary clipboard format; pasting a layer into a text editor yields nothing useful (transform keyframes paste as tab-separated "Adobe After Effects Keyframe Data", ani-a).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** BAE decision: the BAE clipboard carries a JSON fragment of Prims so layers can be pasted between BAE instances and to Agents.

### AE-PRJ-386 · Delete layers (删除图层)

- **What it does:** Removes selected layers (Delete / Backspace, Edit > Clear).
- **Exact semantics:**
  - Removes the layers and everything they own. Sources remain in the project.
  - Children of a deleted layer: parent becomes None; their world transform at the CTI is preserved by rewriting local values (same as unparenting, AE-PRJ-397) UNVERIFIED (AE may instead let children jump).
  - Layers using a deleted layer as track matte lose the matte (`trackMatteType` → none).
  - Effect LAYER_INDEX parameters pointing to it become None.
  - Expressions referencing it by name fail and show an expression error (they are not rewritten).
  - Locked layers cannot be deleted.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Scripting `Layer.remove()`.

### AE-PRJ-387 · Overlay Edit and Ripple Insert Edit from the Footage panel (叠加编辑、波纹插入编辑)

- **What it does:** Inserts a trimmed footage clip (in/out set in the Footage panel) into the active comp at the CTI.
- **Exact semantics:**
  - Overlay Edit: new layer at the top of the stack, `inPoint = CTI`, trimmed to the Footage panel in/out.
  - Ripple Insert Edit: same, and every other layer that spans the CTI is split at the CTI; all split-off parts and layers starting at/after the CTI are moved later by the inserted duration so their in points equal the inserted layer's out point.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Footage panel UI: ui-preview-render-io.md [ui-a].

### AE-PRJ-388 · New layer placement and new-layer preferences (新图层的位置与首选项)

- **What it does:** Rules for where new layers go in the stack and in time, and the preferences that change them.
- **Exact semantics:**
  - Stack position: when one or more layers are selected, a new layer (from Layer > New, drag-and-drop to the comp name/icon, Ctrl+/) is inserted directly above the topmost selected layer; otherwise at index 1. Dragging into the Timeline layer outline inserts where the highlight bar shows; dragging into the Composition panel inserts above the selected layer or at the top.
  - Time: `startTime = inPoint = 0` when "Create Layers at Composition Start Time" (在合成开始时间创建图层) is on (default on; Preferences > Composition in AE 2026, formerly General); otherwise CTI. Dragging into the time graph sets the in point at the drop time (Shift snaps to CTI).
  - Space: centred in the comp (Position = comp centre) unless dropped in the Composition panel at a point.
  - Other preferences affecting new layers: "Center Anchor Point in New Shape Layers" (在新形状图层上居中放置锚点) (shape layers drawn with tools get their anchor at the shape's centre; default on UNVERIFIED, AE 17.0+), "Default Spatial Interpolation to Linear", "Default Position Properties to Separated Dimensions" (ani-a), Still Footage duration (Preferences > Import), label defaults (AE-PRJ-393).
  - Multiple items dropped at once: order follows Project-panel selection order (first selected on top).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Help: "New layers are created immediately above a selected layer and at the center of the composition. If no layer is selected, then new layers are created at the top of the layer stack." zh-CN preference strings（译名待核）.

### AE-PRJ-389 · Fit to Comp and centring commands (适合复合 / 居中)（译名待核）

- **What it does:** Layer > Transform commands that size or centre layers relative to the comp.
- **Exact semantics:**
  | Command | Shortcut | Effect |
  |---|---|---|
  | Fit to Comp (适合复合) | Ctrl+Alt+F / Cmd+Opt+F | Scale (non-uniform allowed) so the layer's source rect fills the comp frame exactly; Position = comp centre; Anchor unchanged at source centre UNVERIFIED (AE sets anchor to centre?) |
  | Fit to Comp Width | Ctrl+Alt+Shift+H / Cmd+Opt+Shift+H (preserves each layer's aspect ratio) | uniform scale so layer width = comp width |
  | Fit to Comp Height | Ctrl+Alt+Shift+G / Cmd+Opt+Shift+G (preserves aspect ratio) | uniform scale so layer height = comp height |
  | Center Anchor Point in Layer Content (在图层内容中居中放置锚点) | Ctrl+Alt+Home | Anchor = centre of `sourceRectAtTime(CTI)` (visible content, AE 17.0+), Position adjusted so the layer does not move |
  | Center In View (在视图中居中) | Ctrl+Home | Position so that the anchor point lands at the centre of the current Composition view |
  - Each applies to all selected layers; when Scale/Position are keyframed, a keyframe is set at CTI.
  - Parented layers: computations happen in the parent's space UNVERIFIED.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Transform semantics: transform-keyframes-expressions.md [ani-a].

## Stacking order

### AE-PRJ-390 · Layer stacking order and render order (图层堆积顺序)

- **What it does:** The order of layers in the Timeline defines compositing order: index 1 is the top (frontmost) layer; layers are composited from the bottom (highest index) upward.
- **Exact semantics:**
  ```
  for i in numLayers down to 1:          // bottom to top
      composite(layer[i]) over accumulated result   // using blend mode, matte, adjustment rules (cmp-a)
  ```
  - 2D layers follow index order strictly. Consecutive runs of 3D layers are depth-sorted by the 3D renderer instead; a 2D layer between 3D layers splits them into separate 3D groups (cmp-b "2D/3D grouping").
  - Index also defines the implicit track matte (legacy: layer above), the adjustment layer scope (all layers below), and Preserve Underlying Transparency's "underlying" composite.
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Lottie: `layers[0]` is the TOP layer (same as AE index 1); players draw the array in reverse. Render order details: compositing-3d-color.md [cmp-a] "render order".

### AE-PRJ-391 · Reorder layers: Arrange commands and drag (排列图层)

- **What it does:** Changes layer indices.
- **Exact semantics:**
  | Command (中文) | Win / Mac shortcut | Effect on the selected set S |
  |---|---|---|
  | Bring Layer Forward (将图层前移一层) | Ctrl+Alt+↑ / Cmd+Opt+↑ | each layer in S moves up one index (blocks of adjacent selected layers move together; a layer already at 1 stays) |
  | Send Layer Backward (将图层后移一层) | Ctrl+Alt+↓ / Cmd+Opt+↓ | each moves down one index |
  | Bring Layer to Front (将图层置于顶层) | Ctrl+Alt+Shift+↑ / Cmd+Opt+Shift+↑ | S moved to indices 1..|S| keeping their relative order |
  | Send Layer to Back (将图层置于底层) | Ctrl+Alt+Shift+↓ / Cmd+Opt+Shift+↓ | S moved to the bottom keeping relative order |
  - Drag in the layer outline: the highlight bar shows the insertion gap; all selected layers move there keeping relative order.
  - Cut + Paste reorders in selection order (AE-PRJ-385).
  - Reordering never changes parent links, explicit track matte links (AE 23+) or layer IDs; it does change `index` and any expressions that use `thisComp.layer(index)` or `index - 1` semantics.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model, bae-ui-motion)
- **Notes:** Layer > Arrange menu (排列). Scripting `moveBefore`, `moveAfter`, `moveToBeginning`, `moveToEnd`. zh-CN strings（译名待核）.

## Labels

### AE-PRJ-392 · Label colours (标签颜色)

- **What it does:** Sixteen named colour labels (plus None) that can be assigned to project items, layers, keyframes, markers and effects for organisation.
- **Exact semantics:** Preferences > Labels > Label Colors; index stored is 0..16 (0 = None). Factory names and colours (values as stored in AE preference files since CC 2014 and widely published; not documented by Adobe help, UNVERIFIED against an AE 2026 install):
  | Index | Name (中文)（译名待核） | Default sRGB |
  |---|---|---|
  | 0 | None (无) | — (no colour) |
  | 1 | Red (红色) | #B53838 |
  | 2 | Yellow (黄色) | #E4D84C |
  | 3 | Aqua (浅绿色) | #A9CBC7 |
  | 4 | Pink (粉红色) | #E5BCC9 |
  | 5 | Lavender (淡紫色) | #A9A9CA |
  | 6 | Peach (桃红色) | #E7C19E |
  | 7 | Sea Foam (海泡色) | #B3C7B3 |
  | 8 | Blue (蓝色) | #677DE0 |
  | 9 | Green (绿色) | #4AA44C |
  | 10 | Purple (紫色) | #8E2C9A |
  | 11 | Orange (橙色) | #E8920D |
  | 12 | Brown (棕色) | #7F452A |
  | 13 | Fuchsia (紫红色) | #F46DD6 |
  | 14 | Cyan (青色) | #3DA2A5 |
  | 15 | Sandstone (砂岩) | #A89677 |
  | 16 | Dark Green (深绿色) | #1E401E |
  - Names and colours are user-editable in Preferences (application-wide, not per project); the project stores only indices, so a project shows different colours on machines with different prefs.
  - Label colours also tint: layer bars in the Timeline, layer handles/paths in the viewer (preference "Use Label Color for Layer Handles and Paths"), viewer tabs ("Use Label Color for Related Tabs"), and mask colours ("Cycle Mask Colors (uses label colors)").
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting: label attributes are integers 0..16; custom colours cannot be set from scripts. BAE decision: store the label palette in the project (with user override) so collaborators see the same colours; still store indices for AE interop.

### AE-PRJ-393 · Default labels by item and layer type (标签默认值)

- **What it does:** Preferences > Labels > Label Defaults assigns a label automatically to new items and layers by type.
- **Exact semantics:** Categories (AE 2026 order) with factory defaults UNVERIFIED (values from AE CC defaults):
  | Category (中文) | Default label |
  |---|---|
  | Composition (合成) | Sandstone (15) |
  | Video (视频) | Aqua (3) |
  | Audio (音频) | Sea Foam (7) |
  | Still (静止图像) | Lavender (5) |
  | Folder (文件夹) | Yellow (2) |
  | Solid (纯色) | Red (1) |
  | Camera (摄像机) | Pink (4) |
  | Light (灯光) | Pink (4) |
  | Adjustment (调整) | Purple (10) |
  | Null Object (空对象) | Red (1) |
  | Shape (形状) | Blue (8) |
  | Text (文本) | Red (1) |
  | 3D Models (3D 模型) | UNVERIFIED |
  | Other 3D (其他 3D) | UNVERIFIED |
  - A footage/precomp layer takes the label of its source item at creation time (not linked afterwards).
  - Changing a default affects only items/layers created afterwards.
- **Priority:** P1
- **Owner:** bae-model
- **Notes:** "3D Models" and "Other 3D" categories exist in AE 2024+.

### AE-PRJ-394 · Assigning labels and Select Label Group (指定标签、选择标签组)

- **What it does:** Commands to set the label of selected items/layers and to select everything with the same label.
- **Exact semantics:**
  - Click the label swatch in the Label column (Timeline or Project panel) → menu of the 16 colours + None + Select Label Group; or Edit > Label > <colour>. Applies to all selected layers/items.
  - Edit > Label > Select Label Group (选择标签组): selects all layers in the comp (or all items in the Project panel, depending on the active panel) that have the same label as the selected one(s) (union over the labels of the current selection UNVERIFIED).
  - Recolouring a whole group = Select Label Group then pick a new label.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-ui-motion, bae-model)
- **Notes:** Scripting `Layer.label`, `Item.label`.

### AE-PRJ-395 · Keyframe, marker and effect labels (关键帧、标记和效果的颜色标签)

- **What it does:** Colour labels on individual keyframes (AE 22.6, 2022), on comp/layer markers (AE 16.0), and on effect instances in Effect Controls (AE 26.5, September 2026, per Adobe "What's new").
- **Exact semantics:**
  - Keyframe label: per-keyframe int 0..16, default 0; set via keyframe context menu > Label; shown as keyframe icon colour; Select Label Group works on keyframes UNVERIFIED. Scripting `Property.keyLabel(i)`, `setLabelAtKey(i, n)`.
  - Marker label: AE-PRJ-325 field.
  - Effect label: per-effect int 0..16, default 0 (grey swatch = no label); shown as a coloured square swatch next to the effect name in Effect Controls, clicking it opens the label picker; optionally tints the effect row background; scripting attribute name UNVERIFIED.
  - Labels never affect rendering. Copy/paste of keyframes carries their labels UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Versions from docsforadobe (`keyLabel` added 22.6; MarkerValue.label 16.0) and What's New (Color labels for effects in AE 26.5).

## Parenting and property links

### AE-PRJ-396 · Parent relationship and what it inherits (父级)

- **What it does:** A child layer's transform becomes relative to its parent layer's transform, so moving, rotating or scaling the parent carries the child.
- **Exact semantics:**
  - Data: `parent: Option<LayerId>` (one parent per layer; any number of children). Not animatable (cannot change over time).
  - Inherited: the parent's complete layer transform (Anchor Point, Position, Scale incl. negative values, Rotation / X/Y/Z Rotation, Orientation, and Auto-Orient rotation), recursively up the chain:
    ```
    worldMatrix(child, t) = worldMatrix(parent, t) * localMatrix(child, t)
    ```
    (matrix definition and order: transform-keyframes-expressions.md [ani-a] "parenting/auto-orient math").
  - NOT inherited: Opacity, effects, masks, layer styles, blend mode, track matte, quality/switches, Video/solo visibility, time (stretch, remap, in/out), and 3D-ness (a 2D child of a 3D parent stays 2D; a 3D child of a 2D parent is 3D with the parent treated as z = 0) UNVERIFIED for the 2D/3D mixing details (ani-a).
  - The parent's transform is evaluated at comp time t even outside its in/out span and when its Video switch is off.
  - Non-uniform parent scale combined with child rotation produces shear in the child (matrix product; no decomposition).
- **Priority:** P0
- **Owner:** bae-anim (+ bae-model)
- **Notes:** Help: parenting affects all transform properties but not Opacity. Scripting `Layer.parent`. Lottie `parent` = parent's `ind`. Expressions: `parent`, `hasParent`, `toWorld()` (ani-b).

### AE-PRJ-397 · Assigning and removing parents: normal, Alt/Option (jump), Shift (move to parent) (指定和移除父级)

- **What it does:** Parent & Link column pick whip / menu sets or clears a parent, with modifiers controlling whether the child stays in place.
- **Exact semantics:**
  | Action | Child result |
  |---|---|
  | Pick-whip or choose parent from the Parent & Link menu (no modifier) | child keeps its current world appearance: its local Position, Scale, Rotation (and Orientation / X/Y Rotation for 3D) are rewritten so that `world(t_cti)` is unchanged |
  | Alt/Opt + assign | child keeps its local values; they are now interpreted in parent space, so the child "jumps" |
  | Shift + assign | child is moved to the parent's location: child Position set so the child's anchor coincides with the parent's anchor point (UNVERIFIED: Rotation/Scale unchanged) |
  | Choose None, or Ctrl/Cmd-click the pick whip | unparent; local values rewritten so the world appearance is unchanged |
  | Alt/Opt + None, or Alt+Ctrl / Opt+Cmd-click the pick whip | unparent keeping local values (child jumps) |
  - Rewriting with keyframes: the same inverse parent matrix (parent world matrix evaluated at the CTI) is applied to every keyframe value of the rewritten properties and to the static value UNVERIFIED (AE applies the CTI-time correction to all keys; it does not bake per-key parent motion). Expressions on rewritten properties are not changed UNVERIFIED.
  - Rewrite decomposition: Position = inverse(parentWorld) · childWorldPosition; Rotation -= parent world rotation; Scale /= parent world scale (component-wise) — exact only without shear; with shear AE's result is approximate UNVERIFIED.
  - Selecting several layers and pick-whipping one of them parents all selected layers.
  - Layer context menu > Select Children (选择子项): adds all descendants (recursive UNVERIFIED) of the selected layer to the selection.
  - Scripting: `layer.parent = p` behaves like the no-modifier assignment (keeps world appearance); `layer.setParentWithJump(p)` behaves like Alt.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim, bae-model)
- **Notes:** Help text in "Layer properties > Parent and child layers". Info panel shows a hint about modifiers while dragging. zh-CN pick whip = 关联器; Parent & Link = 父级和链接.

### AE-PRJ-398 · Parenting constraints (父级约束)

- **What it does:** Which layers may be parents of which.
- **Exact semantics:**
  - Parent must be in the same comp. Cycles are impossible: the Parent menu and pick whip exclude the layer itself and all its descendants.
  - Any layer with a Transform group can be a parent or child: AV layers, nulls, text, shape, cameras, lights (incl. a camera parented to a null). Audio-only layers and ambient lights cannot be parents or children UNVERIFIED.
  - 2D and 3D layers can be mixed in either direction (ani-a defines how 3D parent transforms apply to a 2D child).
  - Parents with negative scale mirror their children (allowed).
  - Deleting a parent: AE-PRJ-386. Duplicating/Precomposing: AE-PRJ-384, AE-PRJ-401.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Command validation must reject cycles (including those created by collaborators concurrently: BAE decision: on CRDT merge, a cycle is broken by clearing the parent link with the latest timestamp).

### AE-PRJ-399 · Property pick whip and expression links (属性关联器、表达式链接)

- **What it does:** Links one property to another by generating an expression, via the property pick whip or Alt-drag.
- **Exact semantics:**
  - Each property with an expression or in "Parent & Link" mode shows a pick whip (spiral icon). Dragging it from property A onto property B (in the same or another comp, in the Timeline, Effect Controls or Composition panel) sets A's expression to a reference to B, e.g. `thisComp.layer("Null 1").transform.position` (same comp) or `comp("Main").layer("Null 1").effect("Slider Control")("Slider")`.
  - Dimension mismatch: AE adapts automatically, e.g. linking 1D to a 2D property produces `temp = …; [temp, temp]`; linking 2D to 1D uses `[0]` of the source.
  - Alt/Opt-click the stopwatch opens the expression editor; Alt-dragging a property's pick whip onto a layer in the Timeline sets the expression to that layer reference UNVERIFIED.
  - Since AE 15.0 (CC 2018) the pick whip can target properties in other comps and in the Effect Controls panel of other layers.
  - Expression text references layers by name; renaming updates them (AE-PRJ-353).
- **Priority:** P0
- **Owner:** bae-motion (+ bae-anim, bae-ui-motion)
- **Notes:** Expression language: transform-keyframes-expressions.md [ani-b]. BAE decision: keep a structured "link" record next to the generated expression text so links survive renames and can be visualised.

### AE-PRJ-400 · Copy With Property Links / Copy With Relative Property Links (复制并保留属性链接)（译名待核）

- **What it does:** Edit > Copy With Property Links copies properties (or whole layers) so that pasting creates expressions linking each pasted property back to its source.
- **Exact semantics:**
  - Copy With Property Links (AE 15.0): pasted properties get expressions referencing the source by absolute path including the comp: `comp("Main").layer("A").transform.opacity`.
  - Copy With Relative Property Links (AE 16.0 UNVERIFIED): references use `thisComp`, so pasting into a duplicated comp links within that comp.
  - When whole layers are copied this way, every property that has a value (not only modified ones UNVERIFIED) is linked; effects are copied with their parameters linked.
  - Paste puts the links on the selected target layer or creates new layers as normal paste does.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Common for building rigs and Essential Graphics masters.

## Precompose and nesting

### AE-PRJ-401 · Precompose (预合成)

- **What it does:** Layer > Pre-compose (Ctrl+Shift+C / Cmd+Shift+C) replaces the selected layers with one precomp layer whose source is a new comp containing them.
- **Exact semantics:** Dialog fields:
  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | New composition name (新合成名称) | string | single layer: "<layer name> Comp N" UNVERIFIED; several: "Pre-comp N" (预合成 N) | |
  | Leave all attributes in "<comp>" (保留"<合成>"中的所有属性) | radio | selected when available UNVERIFIED | not available when more than one layer, a text layer or a shape layer is selected (help); UNVERIFIED: camera, light, null, adjustment |
  | Move all attributes into the new composition (将所有属性移动到新合成) | radio | forced when Leave is unavailable | |
  | Adjust composition duration to the time span of the selected layers (将合成持续时间调整为所选图层的时间跨度) | bool | false UNVERIFIED | only with Move all; newer AE (UNVERIFIED version, ≈ AE 2020+) |
  | Open New Composition (打开新合成) | bool | false UNVERIFIED | opens the new comp in Timeline/viewer |
  **Leave all attributes** (single AV layer L):
  ```
  new comp C: width,height = L.source size (layer size); PAR, fps = original comp's UNVERIFIED (fps: source's?); duration = original comp duration UNVERIFIED
  inside C: one layer referencing L.source with DEFAULT transforms, no effects/masks/keyframes, timing UNVERIFIED (startTime = L.startTime so frames align)
  L.source = C   // L keeps name? (layer name becomes C's name if L had no set name), transforms, masks, effects, styles, keyframes, expressions, switches, matte, parent, timing
  ```
  **Move all attributes:**
  ```
  new comp C: width,height,PAR,fps,bgColor? = original comp's; duration = original comp duration
  move each selected layer (with everything it owns) into C keeping its index order, timing and transforms
  insert one precomp layer P at the index of the topmost selected layer; P: startTime 0, in 0, out = C.duration, default transforms (anchor = C centre, position = comp centre), label = C's label
  if "Adjust duration" checked:
      s = min(L.inPoint), e = max(L.outPoint) over selected; C.duration = e - s
      every moved layer startTime -= s ; P.startTime = s ; P.in = s ; P.out = e
  ```
  - New comp item goes into the same Project folder as the original comp UNVERIFIED.
  - Undo is one step.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Help: Leave keeps properties on the new precomp layer and the new comp has the layer's frame size; Move gives the new comp the original comp's frame size; Leave is unavailable for multiple layers, text or shape layers. zh-CN dialog strings（译名待核）.

### AE-PRJ-402 · Precompose: fate of links, mattes, parents, expressions and markers (预合成后的关系处理)

- **What it does:** Defines what happens to cross-layer relationships when layers move into a new comp.
- **Exact semantics:**
  | Relationship | Result (Move all attributes) |
  |---|---|
  | Parent inside the moved set | kept |
  | Moved child whose parent stays outside | parent link removed; the child keeps its local values UNVERIFIED (so it jumps) — BAE should bake world transform at CTI instead and warn |
  | Layer outside whose parent is moved inside | parent link removed (cross-comp parents are impossible) UNVERIFIED |
  | Track matte: both moved | kept |
  | Track matte: matted layer moved, matte not (AE 23+) | link removed; legacy implicit matte: the layer loses its matte |
  | Track matte: matte moved, matted layer stays | matted layer loses its matte UNVERIFIED |
  | Expressions inside moved layers using `thisComp.layer("X")` where X stays outside | not rewritten; they now resolve in the new comp and error unless a same-named layer exists there |
  | Expressions outside referencing moved layers | not rewritten; error UNVERIFIED (AE 17+ may rewrite to `comp("Pre-comp 1").layer("X")` UNVERIFIED) |
  | Layer markers | move with their layers |
  | Comp markers of the original comp | stay; the new comp has none (so the precomp layer gets no markers) |
  | Effects with LAYER_INDEX params pointing across the boundary | set to None UNVERIFIED |
  | Effects, masks, styles of moved layers | move with them (Move) / stay on the precomp layer (Leave) |
  | Solo/shy/lock/label | move with layers; the precomp layer is unlocked, not solo, not shy |
  - With "Leave all attributes" nothing changes for links because the layer stays in place.
- **Priority:** P1 (P0 for the parent/matte rules, which can visibly break comps)
- **Owner:** bae-motion (+ bae-model)
- **Notes:** These rules are largely UNVERIFIED and must be confirmed against AE 26 before implementation; BAE should surface a summary of broken links after Precompose.

### AE-PRJ-403 · Composition nesting rules (合成嵌套规则)

- **What it does:** Constraints and inheritance rules for comps used as layers in other comps.
- **Exact semantics:**
  - No cycles: a comp cannot be added to itself or to any comp that it contains directly or indirectly; attempts are refused with an error ("cannot add a composition to itself" UNVERIFIED wording).
  - A comp may be nested any number of times in any number of comps; all instances share the same source (edits propagate).
  - Precomp layer size = nested comp width × height; the layer's time maps via startTime/stretch/time remap (AE-PRJ-373, AE-PRJ-376).
  - Inherited from the containing comp unless "Preserve …" options are set: resolution (AE-PRJ-312), frame rate sampling (AE-PRJ-311); background colour is NOT inherited (nested background transparent).
  - Preferences > Composition "Switches Affect Nested Comps" (开关影响嵌套的合成) (default on): the precomp layer's Quality, Motion Blur, Frame Blending and Wireframe/Draft switches propagate into the nested comp's layers UNVERIFIED (exact list in cmp-a). 3D renderer: a nested comp renders with its own renderer setting unless collapsed.
  - Preferences > Composition "Synchronize Time of All Related Items" (同步所有相关项目的时间) (default on): moving the CTI in one comp moves it in related comps (UI only).
  - Comp Navigator / Mini-Flowchart (Tab), Shift+Esc to the previous comp in the network: UI (ui-a).
- **Priority:** P0
- **Owner:** bae-model (+ bae-render)
- **Notes:** Render semantics of nested comps: compositing-3d-color.md [cmp-a] "nested comp res/fps" and "collapse".

## Replacing sources

### AE-PRJ-404 · Replace layer source (替换图层源)

- **What it does:** Alt/Option-drag a project item onto selected layers in the Timeline (or Composition panel) to swap the source of those layers while keeping everything else.
- **Exact semantics:**
  - Keeps: layer name if set (unset → shows new source name), transforms (values unchanged, not rescaled), keyframes, expressions, effects, masks, styles, switches, timing (startTime, in, out, stretch), parent, matte, markers, label, comment.
  - Changes: `source`; layer width/height become the new source's; Anchor Point is NOT recentred (a different-size source appears offset) UNVERIFIED; in/out clamped to the new source's extent if it is time-based and shorter UNVERIFIED.
  - Allowed sources: any footage, solid or comp item, except a comp that would create a cycle. Text, shape, camera, light, null layers have no replaceable source.
  - Applies to every selected layer. Also via scripting `AVLayer.replaceSource(item, fixExpressions)`: with `fixExpressions = true`, expressions referencing the old source name are updated.
- **Priority:** P0
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Help: "Alt-drag (Windows) or Option-drag (Mac OS) a footage item from the Project panel onto a selected layer in the Timeline".

### AE-PRJ-405 · Replace Footage for all usages (替换素材)

- **What it does:** File > Replace Footage > File (Ctrl+H / Cmd+H) relinks a footage item to another file, changing every layer that uses it.
- **Exact semantics:**
  - The item keeps its ID, name (UNVERIFIED: name updates to the new file name unless the item was renamed), label, folder and every usage; Interpret Footage settings are kept if compatible UNVERIFIED.
  - Other Replace Footage variants: With Layered Comp, Placeholder, Solid, With After Effects Composition UNVERIFIED list.
  - Alt-dragging an item onto another item in the Project panel replaces all usages of the target item with the dragged item (Project panel replace) UNVERIFIED.
- **Priority:** P0
- **Owner:** bae-model (+ bae-media)
- **Notes:** Footage item relinking, missing footage and placeholders are owned by project-comp-layers.md [prj-a]; this item only states the per-layer effect: all layers referencing the item change together while keeping their own attributes (AE-PRJ-404 list).

## Sequencing layers in time

### AE-PRJ-406 · Sequence Layers keyframe assistant (序列图层)

- **What it does:** Animation > Keyframe Assistant > Sequence Layers arranges selected layers end to end in selection order, optionally overlapping them with dissolve transitions.
- **Exact semantics:** Dialog:
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Overlap (重叠) | bool | false | — | — | |
  | Duration (持续时间) | time | 0:00:01:00 UNVERIFIED | any; negative = gap | timecode | used only when Overlap is on |
  | Transition (过渡) | enum | Dissolve Front Layer UNVERIFIED | Off (关), Dissolve Front Layer (溶解前景图层), Cross Dissolve Front and Back Layers (交叉溶解前景和背景图层)（译名待核） | — | used only when Overlap is on and Duration > 0 |
  ```
  L[1..n] = selected layers in SELECTION order (Ctrl/Cmd-click order); L[1] does not move
  o = overlap ? Duration : 0
  for i in 2..n:
      d = (L[i-1].outPoint - o) - L[i].inPoint
      move L[i] by d   // startTime/in/out/keyframes/markers shift (AE-PRJ-380); no trimming
  transitions (o > 0), for each i in 2..n, overlap window [a, b] = [L[i].inPoint, L[i-1].outPoint]:
      front = the one of L[i-1], L[i] that is higher in the stack (lower index)
      Dissolve Front Layer: Opacity keys on front only: if front is L[i-1]: (a,100%)->(b,0%) else (a,0%)->(b,100%)
      Cross Dissolve Front and Back: L[i-1]: (a,100%)->(b,0%) AND L[i]: (a,0%)->(b,100%)
      keys are linear; existing Opacity values are overwritten at those times UNVERIFIED (AE multiplies? no: sets absolute 0/100)
  ```
  - Layers need durations shorter than the comp to leave room (help); layers may extend past the comp end.
  - Stacking order is not changed.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-anim)
- **Notes:** Help confirms: first selected layer stays, order = selection order, negative Duration leaves gaps, "Cross Dissolve Front And Back Layers to use the transparency of the selected layers". The same options appear in New Comp from Selection (AE-PRJ-317). Opacity key exact values UNVERIFIED.

### AE-PRJ-407 · Quick Offset (快速偏移)

- **What it does:** Ctrl+Alt-drag (Win) / Cmd+Opt-drag (Mac) on selected layers (or keyframes across layers) staggers them evenly in time by selection order.
- **Exact semantics:**
  ```
  L[1..n] = selected layers in selection order; D = drag delta (frames)
  for i in 1..n: move L[i] by round(D * (i-1)/(n-1))   // L[1] fixed, L[n] moves D
  ```
  - Keyframes move with their layers. An overlay shows "Total Offset" and "Per Layer" in the Timeline's time display unit.
  - With keyframes selected across several layers, Ctrl+Alt / Cmd+Opt-drag staggers the keyframes across the layers in the same way (help); UNVERIFIED: the exact grouping when one layer has several selected keyframes (assumed: each layer's selected keys move as a group).
  - The overlay unit follows the Timeline time display (timecode or frames).
- **Priority:** P2
- **Owner:** bae-motion (+ bae-ui-motion)
- **Notes:** Recent feature (exact version UNVERIFIED); help page "Selecting and arranging layers > Quickly offset your layers and keyframes" (zh-CN help: 快速偏移). Rounding to whole frames UNVERIFIED.

## Auto-Orient

### AE-PRJ-408 · Auto-Orient setting (自动方向)

- **What it does:** Layer > Transform > Auto-Orient (Ctrl+Alt+O / Cmd+Opt+O) makes a layer rotate automatically to follow its motion path, face the camera, or aim at its point of interest.
- **Exact semantics:** `autoOrient` enum (scripting `AutoOrientType`), default Off for every layer type except two-node cameras and Spot/Parallel lights, which default to Orient Towards Point of Interest:
  | Option (中文) | Scripting | Available for | Lottie |
  |---|---|---|---|
  | Off (关) | `NO_AUTO_ORIENT` | all | `ao: 0` |
  | Orient Along Path (沿路径定向) | `ALONG_PATH` | all layers with Transform (2D: Z rotation; 3D: full orientation along the tangent) | `ao: 1` |
  | Orient Towards Camera (定位于摄像机) | `CAMERA_OR_POINT_OF_INTEREST` | 3D AV/text/shape layers (not cameras/lights) | — |
  | Orient Towards Point of Interest (定位于目标点) | `CAMERA_OR_POINT_OF_INTEREST` | cameras and lights (Spot, Parallel) | — (camera `a` used as POI) |
  | Independently orient each character towards camera (独立定向每个字符朝向摄像机)（译名待核） | `CHARACTERS_TOWARD_CAMERA` | text layers with per-character 3D | — |
  - Setting Off on a two-node camera turns it into a one-node camera behaviour (POI ignored; the Point of Interest property disappears) UNVERIFIED for property visibility.
  - Auto-orient rotation is added on top of the layer's Rotation/Orientation values (math: transform-keyframes-expressions.md [ani-a] "parenting/auto-orient math").
  - Children inherit the auto-oriented transform (AE-PRJ-396).
- **Priority:** P0 (Off/Along Path/Towards POI); P1 (Towards Camera); P2 (per-character).
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Scripting `Layer.autoOrient`. Dialog menu labels change with layer type; zh-CN labels UNVERIFIED（译名待核）.

## Layer Styles

### AE-PRJ-409 · Layer Styles framework and render order (图层样式)

- **What it does:** Photoshop-compatible styles (shadows, glows, bevel, overlays, stroke) attached to a layer; they render after the layer's transform and blend directly with the layers below.
- **Exact semantics:**
  - Container: `ADBE Layer Styles` group (shown only after a style is added), children in fixed order: Blending Options (`ADBE Blend Options Group`), Drop Shadow (`dropShadow/…`), Inner Shadow (`innerShadow/…`), Outer Glow (`outerGlow/…`), Inner Glow (`innerGlow/…`), Bevel and Emboss (`bevelEmboss/…`), Satin (`chromeFX/…`), Color Overlay (`solidFill/…`), Gradient Overlay (`gradientFill/…`), Pattern Overlay (`patternFill/…`, import-only: cannot be added in AE), Stroke (`frameFX/…`). Each style has an enable eyeball (`<style>/enabled`).
  - Commands: Layer > Layer Styles > <style> adds it to the selected layers; Delete removes the selected style; Layer > Layer Styles > Remove All; Convert To Editable Styles (for PSD layers with merged styles); Copy/Paste styles.
  - Render order per layer: source → masks → effects → transform (incl. motion blur) → layer styles → blend with the composite below. Styles therefore follow the transformed layer edges (shadows do not rotate with the layer's angle setting).
  - Classification: interior styles (Inner Shadow, Inner Glow, Satin, Color Overlay, Gradient Overlay, Bevel and Emboss) affect the layer's opaque pixels; exterior styles (Drop Shadow, Outer Glow) interact only with the layers below; Stroke is both (position dependent).
  - Compositing stack, bottom to top (Photoshop order): Drop Shadow, Outer Glow, [layer fill × Fill Opacity, then interior styles: Satin? order per Photoshop: Color Overlay, Gradient Overlay, Pattern Overlay, Satin, Inner Glow, Inner Shadow, Bevel highlight/shadow], Stroke UNVERIFIED exact interior order; the whole result is then multiplied by layer Opacity.
  - On vector layers (text, shape, Illustrator footage) edge styles follow the vector outlines; on raster layers they follow the alpha edge.
  - 3D: a layer with a layer style cannot intersect other 3D layers or cast/receive shadows; 3D layers on either side of it cannot intersect each other (it splits the 3D group, like a 2D layer).
- **Priority:** P2
- **Owner:** bae-render (+ bae-model, bae-effects)
- **Notes:** Help "Blending modes and layer styles". Lottie layer `sy[]` with `ty` per bodymovin layerStyleTypes: stroke 0, drop shadow 1, inner shadow 2, outer glow 3, inner glow 4, bevel and emboss 5, satin 6, color overlay 7, gradient overlay 8; lottie-web renders only a subset (none on canvas) UNVERIFIED. Defaults below follow Photoshop's style defaults and are UNVERIFIED for AE unless stated.

### AE-PRJ-410 · Drop Shadow style (投影)

- **What it does:** Casts an offset, blurred copy of the layer's alpha behind the layer.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Type | Default | Range | Units |
  |---|---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | enum | Multiply | blend mode list | — |
  | Color (颜色) | `color` | COLOR | black | — | — |
  | Opacity (不透明度) | `opacity` | OneD | 75 % UNVERIFIED | 0..100 | % |
  | Use Global Light (使用全局光) | `useGlobalAngle` | bool | On UNVERIFIED | — | — |
  | Angle (角度) | `localLightingAngle` | OneD | 120° | any | degrees (light direction; shadow offset opposite) |
  | Distance (距离) | `distance` | OneD | 5 | 0..30000 UNVERIFIED | px |
  | Spread (扩展) | `chokeMatte` | OneD | 0 % | 0..100 | % |
  | Size (大小) | `blur` | OneD | 5 | 0..250 UNVERIFIED | px |
  | Noise (杂色) | `noise` | OneD | 0 % | 0..100 | % |
  | Layer Knocks Out Drop Shadow (图层挖空投影) | `layerConceals` | bool | On | — | — |
  ```
  angle = useGlobal ? globalAngle : localAngle
  offset = distance * (-cos(angle), +sin(angle))   // light from angle; shadow opposite (y down) UNVERIFIED sign
  m = alpha(layer) shifted by offset; m = spread(m, spread%*size); m = blur(m, size)
  if layerKnocksOut: m *= (1 - alpha(layer))
  shadow = color with alpha m * opacity, blended with mode under the layer
  ```
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 1` keys `c, o, a, s, d, ch, bm, no, lc` (lottie-docs schema drop-shadow-style). Blur kernel per Photoshop (approximate Gaussian) UNVERIFIED.

### AE-PRJ-411 · Inner Shadow style (内阴影)

- **What it does:** Draws a shadow inside the layer's edges as if the layer were a hole.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range | Units |
  |---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | Multiply | — | — |
  | Color (颜色) | `color` | black | — | — |
  | Opacity (不透明度) | `opacity` | 75 % | 0..100 | % |
  | Use Global Light (使用全局光) | `useGlobalAngle` | On | — | — |
  | Angle (角度) | `localLightingAngle` | 120° | — | degrees |
  | Distance (距离) | `distance` | 5 | 0..30000 | px |
  | Choke (阻塞) | `chokeMatte` | 0 % | 0..100 | % |
  | Size (大小) | `blur` | 5 | 0..250 | px |
  | Noise (杂色) | `noise` | 0 % | 0..100 | % |
  - Mask = inverted layer alpha, offset, choked, blurred, then multiplied by layer alpha (only inside). Defaults UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 2`.

### AE-PRJ-412 · Outer Glow style (外发光)

- **What it does:** Adds a glow around the outside of the layer's edges.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range | Units |
  |---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | Screen | — | — |
  | Opacity (不透明度) | `opacity` | 75 % | 0..100 | % |
  | Noise (杂色) | `noise` | 0 % | 0..100 | % |
  | Color Type (颜色类型) | `AEColorChoice` | Color (颜色) | Color, Gradient (渐变) | — |
  | Color (颜色) | `color` | RGB(255, 255, 190) UNVERIFIED | — | — |
  | Colors (颜色) | `gradient` | gradient (colour → transparent) | — | CUSTOM_VALUE gradient |
  | Gradient Smoothness (渐变平滑度) | `gradientSmoothness` | 100 % | 0..100 | % |
  | Technique (方法) | `glowTechnique` | Softer (柔和) | Softer, Precise (精确) | — |
  | Spread (扩展) | `chokeMatte` | 0 % | 0..100 | % |
  | Size (大小) | `blur` | 5 | 0..250 | px |
  | Range (范围) | `inputRange` | 50 % | 1..100 | % |
  | Jitter (抖动) | `shadingNoise` | 0 % | 0..100 | % |
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 3`. Defaults UNVERIFIED (Photoshop defaults).

### AE-PRJ-413 · Inner Glow style (内发光)

- **What it does:** Adds a glow inside the layer from its edges or centre.
- **Exact semantics:** Same parameters as Outer Glow (AE-PRJ-412) with Choke (阻塞, `chokeMatte`) instead of Spread, plus:
  | Parameter (中文) | matchName suffix | Default | Values |
  |---|---|---|---|
  | Source (源) | `innerGlowSource` | Edge (边缘) | Center (居中), Edge (边缘) |
  - Default Blend Mode Screen, Opacity 75 %, Size 5, Range 50 % UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 4`.

### AE-PRJ-414 · Bevel and Emboss style (斜面和浮雕)

- **What it does:** Adds highlight and shadow shading so the layer looks raised or engraved.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range / values | Units |
  |---|---|---|---|---|
  | Style (样式) | `bevelStyle` | Inner Bevel (内斜面) | Outer Bevel (外斜面), Inner Bevel, Emboss (浮雕效果), Pillow Emboss (枕状浮雕), Stroke Emboss (描边浮雕)（译名待核） | — |
  | Technique (方法) | `bevelTechnique` | Smooth (平滑) | Smooth, Chisel Hard (雕刻清晰), Chisel Soft (雕刻柔和) | — |
  | Depth (深度) | `strengthRatio` | 100 % | 1..1000 | % |
  | Direction (方向) | `bevelDirection` | Up (上) | Up, Down (下) | — |
  | Size (大小) | `blur` | 5 | 0..250 | px |
  | Soften (柔化) | `softness` | 0 | 0..16 | px |
  | Use Global Light (使用全局光) | `useGlobalAngle` | On | — | — |
  | Angle (角度) | `localLightingAngle` | 120° | — | degrees |
  | Altitude (高度) | `localLightingAltitude` | 30° | 0..90 | degrees |
  | Highlight Mode (高光模式) | `highlightMode` | Screen | blend modes | — |
  | Highlight Color (高光颜色) | `highlightColor` | white | — | — |
  | Highlight Opacity (高光不透明度) | `highlightOpacity` | 75 % | 0..100 | % |
  | Shadow Mode (阴影模式) | `shadowMode` | Multiply | blend modes | — |
  | Shadow Color (阴影颜色) | `shadowColor` | black | — | — |
  | Shadow Opacity (阴影不透明度) | `shadowOpacity` | 75 % | 0..100 | % |
  - Gloss contour, contour and texture sub-options of Photoshop are not available in AE (preserved only from PSD UNVERIFIED). Defaults UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 5`. Help recommends it over the Bevel Alpha effect when highlight and shadow need different blend modes.

### AE-PRJ-415 · Satin style (光泽)

- **What it does:** Applies interior shading that creates a satiny finish from two offset, blurred copies of the layer's shape.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range | Units |
  |---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | Multiply | — | — |
  | Color (颜色) | `color` | black | — | — |
  | Opacity (不透明度) | `opacity` | 50 % | 0..100 | % |
  | Angle (角度) | `localLightingAngle` | 19° | — | degrees |
  | Distance (距离) | `distance` | 11 | 1..250 | px |
  | Size (大小) | `blur` | 14 | 0..250 | px |
  | Invert (反转) | `invert` | On | — | — |
  ```
  a = alpha shifted by +offset, b = alpha shifted by -offset (offset from angle, distance)
  m = blur(|a - b|, size); if invert: m = 1 - m; m *= alpha(layer)
  ```
  - Defaults and formula UNVERIFIED (Photoshop behaviour).
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 6`. Satin does not use Global Light.

### AE-PRJ-416 · Color Overlay style (颜色叠加)

- **What it does:** Fills the layer's opaque pixels with a solid colour.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range | Units |
  |---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | Normal | — | — |
  | Color (颜色) | `color` | red RGB(255, 0, 0) UNVERIFIED | — | — |
  | Opacity (不透明度) | `opacity` | 100 % | 0..100 | % |
  - `result = blend(mode, layerRGB, color) * opacity` inside the layer alpha.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 7`.

### AE-PRJ-417 · Gradient Overlay style (渐变叠加)

- **What it does:** Fills the layer's opaque pixels with a gradient.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range / values | Units |
  |---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | Normal | — | — |
  | Opacity (不透明度) | `opacity` | 100 % | 0..100 | % |
  | Colors (颜色) | `gradient` | black → white UNVERIFIED | gradient editor | — |
  | Gradient Smoothness (渐变平滑度) | `gradientSmoothness` | 100 % | 0..100 | % |
  | Angle (角度) | `angle` | 90° | — | degrees |
  | Style (样式) | `type` | Linear (线性) | Linear, Radial (径向), Angle (角度), Reflected (对称的), Diamond (菱形)（译名待核） | — |
  | Reverse (反向) | `reverse` | Off | — | — |
  | Align with Layer (与图层对齐) | `align` | On | — | — |
  | Scale (缩放) | `scale` | 100 % | 10..150 | % |
  | Offset (偏移) | `offset` | [0, 0] | — | px |
  - Align with Layer On: gradient spans the layer's bounding box; Off: spans the comp frame UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 8`.

### AE-PRJ-418 · Stroke style (描边)

- **What it does:** Outlines the layer's alpha edge with a solid colour.
- **Exact semantics:**
  | Parameter (中文) | matchName suffix | Default | Range / values | Units |
  |---|---|---|---|---|
  | Blend Mode (混合模式) | `mode2` | Normal | — | — |
  | Color (颜色) | `color` | red RGB(255, 0, 0) UNVERIFIED | — | — |
  | Size (大小) | `size` | 3 | 1..250 | px |
  | Opacity (不透明度) | `opacity` | 100 % | 0..100 | % |
  | Position (位置) | `style` | Outside (外部) | Outside, Inside (内部), Center (居中) | — |
  - Photoshop fill types Gradient/Pattern for strokes are not available in AE.
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Lottie `ty: 0`. Distinct from the shape-layer Stroke (shapes.md) and the Stroke effect (effects.md).

### AE-PRJ-419 · Layer style Blending Options (混合选项)

- **What it does:** Per-layer options that control how the styles and the layer content blend: global light, fill opacity, channel exclusion, interior-group blending and blend ranges.
- **Exact semantics:**
  | Parameter (中文) | matchName | Type | Default | Range | Notes |
  |---|---|---|---|---|---|
  | Global Light Angle (全局光角度) | `ADBE Global Angle2` | OneD | 120° | — | used by styles with Use Global Light on |
  | Global Light Altitude (全局光高度) | `ADBE Global Altitude2` | OneD | 30° | 0..90 | Bevel and Emboss |
  | Advanced Blending (高级混合) | `ADBE Adv Blend Group` | group | — | — | |
  | Fill Opacity (填充不透明度) | `ADBE Layer Fill Opacity2` | OneD % | 100 | 0..100 | scales layer content (and interior styles UNVERIFIED) but NOT exterior styles; layer Opacity scales everything |
  | Red / Green / Blue (红色 / 绿色 / 蓝色) | `ADBE R/G/B Channel Blend` | bool (OneD 0/1) | On | — | Off: that channel of the result is taken from the underlying composite (channel excluded from blending); animatable |
  | Blend Interior Styles as Group (将内部样式混合成组) | `ADBE Blend Interior` | bool | Off | — | On: interior styles are blended with the layer first and the group uses the layer's blending mode |
  | Use Blend Ranges from Source (使用源的混合范围) | `ADBE Blend Ranges` | bool | Off | — | applies the "Blend If" ranges stored in the source PSD |
  - Knockout (挖空, None/Shallow/Deep in Photoshop) is not exposed in AE: the docsforadobe Blending Options matchName list has exactly Global Light Angle, Global Light Altitude, Advanced Blending > Fill Opacity, Red, Green, Blue, Blend Interior Styles as Group, Use Blend Ranges from Source.
  - The Blending Options group exists only after a style has been added (adding then deleting a style leaves it).
- **Priority:** P2
- **Owner:** bae-render (+ bae-model)
- **Notes:** Matchnames from docsforadobe Layer Styles page. Lottie: blending options are not exported UNVERIFIED.

## Essential Properties

### AE-PRJ-420 · Essential Properties on precomp layers (基本属性)

- **What it does:** Properties of a nested comp that are added to its Essential Graphics panel appear on every precomp layer of that comp under Essential Properties, where they can be overridden and keyframed per instance (formerly "Master Properties").
- **Exact semantics:**
  - Source side: a comp's Essential Graphics panel controllers (properties dragged into the panel, with optional renamed labels, grouped) define the list; supported types include Slider/Angle/Checkbox/Color/Point/Dropdown values, Source Text, Media Replacement (alternate source) UNVERIFIED.
  - Layer side: `ADBE Layer Overrides` group on each precomp layer, one property per controller, initially showing the source value (not overridden).
  - Editing or keyframing an Essential Property on a layer creates an override for that instance only; other instances keep the source value. Overridden properties are shown in a different state; "Revert" (还原)（译名待核） clears the override; "Push to source" / "Push All Values to Source" pushes the instance value back into the source comp UNVERIFIED wording.
  - Expressions can read them via `layer.essentialProperty("Name")` (AE 2020+ expression) UNVERIFIED API name (ani-b).
  - Changing the source comp's controller list adds/removes Essential Properties on all its layers (removed overrides are lost).
  - Rendering: the nested comp is rendered once per distinct set of overrides (instances with different overrides render separately).
  - Introduced as Master Properties in AE 16.1 (CC 2019 April release) UNVERIFIED version; renamed Essential Properties in AE 2020 (17.0) UNVERIFIED.
- **Priority:** P2
- **Owner:** bae-model (+ bae-anim, bae-render)
- **Notes:** Scripting `Property.essentialPropertySource`, `AVLayer.addToMotionGraphicsTemplate()`, `CompItem.motionGraphicsTemplateName`. Essential Graphics panel UI: ui-preview-render-io.md [ui-a]. BAE: maps naturally onto USD-style per-instance opinions (ARCHITECTURE.md 4.1 Reference + override).

## Other composition and layer commands

### AE-PRJ-421 · Composition poster time (设置标识时间)

- **What it does:** Chooses which frame of a composition is used as its thumbnail (poster frame, 标识帧) in the Project panel.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Poster time (标识时间) | time | 0 (first frame of the comp) | [0, duration) | s (comp time, relative to comp start) | set by Composition > Set Poster Time (合成 > 设置标识时间) to the CTI |
  - Command: Composition > Set Poster Time stores the current CTI of the active comp as the poster time (undoable).
  - The Project panel thumbnail renders the comp at the poster time; transparent pixels are drawn black unless Project panel menu > Thumbnail Transparency Grid (缩略图透明度网格) is on; Preferences > Appearance > Disable Thumbnails in Project Panel (在项目面板中禁用缩略图) turns thumbnails off.
  - Never affects rendering or output. If the comp is shortened so that poster time ≥ duration, the thumbnail falls back to the last valid frame UNVERIFIED.
  - Footage items: the thumbnail of a movie is its first frame unless a poster frame was set UNVERIFIED (AE has no Set Poster Time for footage in the Composition menu).
- **Priority:** P2 — cosmetic, but persisted per comp and round-tripped in .aep.
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Not exposed in the ExtendScript `CompItem` object UNVERIFIED (no `posterTime` attribute documented). The Essential Graphics panel has its own "Set Poster Time" button for Motion Graphics templates (ui-preview-render-io.md [ui-a], Essential Graphics). Implementation hint: cache the thumbnail as a small texture keyed by (comp version, poster time) and re-render lazily when the comp changes.

### AE-PRJ-422 · Scene Edit Detection (场景编辑检测)

- **What it does:** Analyses selected video footage layers for cut points (scene changes) and either marks them with layer markers or splits the layer at each cut, optionally precomposing every resulting segment.
- **Exact semantics:**
  - Invocation: select one or more footage layers, Layer > Scene Edit Detection (图层 > 场景编辑检测) or the layer context menu. Only layers whose source is video footage qualify; precomp layers, other layer types and layers with Time Remapping enabled are rejected.
  - Dialog options:
    | Parameter (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|
    | Create Layer Markers (创建图层标记) | radio | UNVERIFIED (believed selected) | — | — | adds numbered markers of type Segmentation (分段) at each detected cut |
    | Split Layer at Scene Edit (在场景编辑位置拆分图层) | radio | — | — | — | splits the layer at each cut with Split Layer semantics (AE-PRJ-383); one layer per scene |
    | Create Pre-comps (创建预合成) | bool | Off UNVERIFIED | — | — | only with Split Layer; each resulting layer is precomposed with "Move all attributes" semantics (AE-PRJ-401) UNVERIFIED which option |
  - Pre-comps created by the split option are placed in the Project panel in a folder named after the footage layer, nested inside a folder "Scene Edit Detection" (场景编辑检测).
  - Marker comments are sequential numbers (1, 2, 3 …) UNVERIFIED (exact comment text); marker type = Segmentation, whose default label colour comes from Preferences > Labels (AE-PRJ-392).
  - Detection: a machine-learning (Adobe Sensei) shot-boundary detector; analysis runs over the layer's source frames between its in and out points UNVERIFIED (whether trimmed portions are analysed); cut times are frame-quantised to the source frame where the new shot begins. Fades and cuts to black are often not detected (user reports).
  - The whole operation is one undo step.
- **Priority:** P2 — convenience; the detector algorithm is BAE's own choice.
- **Owner:** bae-media (+ bae-motion, bae-model)
- **Notes:** AE 22.3 (April 2022); beta from build 22.0x88 (2021); ported from Premiere Pro. Marker type (Comment / Segmentation) introduced with it: see AE-PRJ-325. BAE decision: implement detection with a cheap CPU/GPU histogram + luma-difference shot-boundary detector (no ML runtime, to respect the size budget); expose a sensitivity threshold as an advanced option. Implementation hint: decode at low resolution (e.g. 64x36 thumbnails) on bae-media's decode path; the split/marker edits go through bae-motion Commands.
