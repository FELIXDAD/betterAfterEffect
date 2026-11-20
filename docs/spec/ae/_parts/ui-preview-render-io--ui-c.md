## Render Queue

### AE-UI-600 · Render item data model (渲染项)

- **What it does:** A render item (render queue item) is a persisted project object that binds one composition to one set of Render Settings, one log level and one or more Output Modules; the ordered list of render items is the project's Render Queue.
- **Exact semantics:**
  - The Render Queue is an ordered list stored in the project (saved with the project file; scripting `app.project.renderQueue.items`, 1-based). Order = render order.
  - Fields of one render item:

    | Field (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|
    | Comp (合成) | ref → CompItem | — | — | — | Immutable after creation (scripting `comp` read-only: to change the comp, delete and re-add). Deleting the comp from the project deletes its render items. UNVERIFIED: exact behaviour when the source comp is deleted (AE removes the item). |
    | Render (渲染) | bool | true when the item is complete (has output path), otherwise false | — | — | Checkbox; `render` attribute. Setting true → status Queued; false → Unqueued. |
    | Label (标签) | label index 0–16 | label of the comp (UNVERIFIED) | 0..16 | — | Colour chip in the Label column; same label set as project items (see project-comp-layers.md, Labels). |
    | Comment (注释) | string | "" | — | — | Free text; Comment column hidden by default. |
    | Status (状态) | enum RQItemStatus | derived (AE-UI-602) | — | — | Read-only to scripts. |
    | Render Settings (渲染设置) | RenderSettings record (AE-UI-614..AE-UI-620) + template name | default RS template ("Best Settings") | — | — | Template name shown as underlined text; edits mark it "Custom" (UNVERIFIED: the label becomes "Custom: Best Settings"). |
    | Log (日志) | enum LogType | Errors Only | Errors Only / Plus Settings / Plus Per Frame Info | — | AE-UI-609. |
    | Output Modules (输出模块) | ordered list ≥ 1 of OutputModule | 1 module from default OM template | 1..n | — | AE-UI-605. |
    | Notify (通知) | bool | value of Preferences > Notifications "auto enable" (AE-UI-668, Export preferences) | — | — | `queueItemNotify` (AE 22.0). |
    | Skip Frames | int | 0 | 0..99 | frames | Scripting only (`skipFrames`): renders every (n+1)-th frame ("render on twos" = 1). No UI. |
    | Time Span Start / Duration | f64 | from Render Settings Time Span | — | seconds | Scripting `timeSpanStart`, `timeSpanDuration`; writing sets Time Span to Custom. |
    | Started (开始时间) | datetime? | null | — | — | Set when rendering of the item begins (`startTime`). |
    | Render Time (渲染时间) | int? | null | ≥ 0 | seconds | `elapsedSeconds`; excludes pauses (UNVERIFIED). |
    | Log File Path | path? | null | — | — | Shown under Render Settings/Log once a log was written. |
  - Items are independent: changing a template later does **not** change items already in the queue (templates are copied by value at apply time).
- **Priority:** P0 — export is the end of every AE workflow.
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Scripting: `RenderQueueItem` (`comp`, `render`, `status`, `logType`, `outputModules`, `numOutputModules`, `skipFrames`, `startTime`, `elapsedSeconds`, `timeSpanStart`, `timeSpanDuration`, `templates`, `queueItemNotify`, `onStatusChanged`; methods `applyTemplate`, `saveAsTemplate`, `duplicate`, `remove`, `getSetting(s)`/`setSetting(s)` (AE 13.0) with English key strings such as "Proxy Use", "Color Depth", "Time Span Start"). BAE decision: render items are Prims in the project layer so they sync and undo like other data; the live render state (progress, elapsed) is not persisted in the CRDT, only the final status/start/elapsed/log path.

### AE-UI-601 · Render Queue panel layout and columns (渲染队列面板)

- **What it does:** Window > Render Queue (Ctrl+Alt+0 / Cmd+Opt+0) shows the queue as a table of render items with twirl-down rows for Render Settings, Log and each Output Module, plus a header area with the current render progress and the Render / Pause / Stop / Queue in AME buttons, and a footer with batch information.
- **Exact semantics:**
  - Top area (left→right): Current Render twirl + progress bar + Elapsed / Remaining (AE-UI-607); right side buttons **Queue in AME** (AME 中的队列), **Stop** (停止), **Pause** (暂停) / **Continue** (继续), **Render** (渲染). Before rendering only Render and Queue in AME are enabled.
  - Item table columns (context menu on a column header > Columns toggles visibility; columns can be reordered/resized like Project panel columns):

    | Column (中文) | Default visible | Content |
    |---|---|---|
    | Render (渲染) | yes | checkbox (Queued ↔ Unqueued) |
    | Label (标签) | yes | label colour chip, click opens label menu |
    | # | yes | 1-based position in queue |
    | Comp Name (合成名称) | yes | source comp name; twirl expands the item |
    | Status (状态) | yes | AE-UI-602 strings |
    | Started (开始时间) | yes | date/time render of the item started |
    | Render Time (渲染时间) | yes | elapsed render time of the item |
    | Comment (注释) | no | editable text |
    | Notify (通知) | no (UNVERIFIED: shown by default since AE 22.0 when notifications are enabled) | per-item notify checkbox |
  - Expanded item rows: `Render Settings: ▾ <template name>` (triangle opens the template menu, underlined name opens the Render Settings dialog); `Log: ▾ Errors Only`; for each output module `Output Module: ▾ <template name>` and `Output To: ▾ <file name>` with `+`/`−` buttons left of Output To (AE-UI-605). Expanding the Output Module group shows a summary of its settings (format, channels, output path) and the Post-Render Action menu.
  - Selection: click, Shift-click range, Ctrl/Cmd-click toggle; Up/Down arrows move selection; Shift+Up/Down extend (same shortcuts as Project panel). Multi-selection lets template menus apply to all selected items.
  - Footer (AE-UI-608): Message, RAM, Renders Started, Total Time Elapsed, notification bell + "Notify when queue completes" checkbox.
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** The Render Queue opens as a tab in the Timeline frame by default in AE's standard workspaces (UNVERIFIED). Panel shortcut also listed in ui-preview-render-io.md, Panel framework (AE-UI-001). AE disables menus while a modal render runs ("Menus are disabled and Render Queue window is open" is a known AE support topic): see AE-UI-612.

### AE-UI-602 · Render item status values and state machine (渲染项状态)

- **What it does:** Each render item has one status that controls whether pressing Render processes it and reports the outcome.
- **Exact semantics:**
  - Enum (UI string / 中文 / scripting `RQItemStatus`):

    | UI string | 中文 | Scripting | Meaning |
    |---|---|---|---|
    | Unqueued | 未加入队列 | `UNQUEUED` | Listed, Render box off, not rendered by Render. |
    | Queued | 已加入队列 | `QUEUED` | Render box on, output valid; will render. |
    | Needs Output | 需要输出 | `NEEDS_OUTPUT` | At least one output module has no file path ("Not yet specified" (尚未指定)). Render box cannot be checked. |
    | Rendering | 正在渲染（译名待核） | `RENDERING` | Currently rendering. |
    | (paused) | — | `WILL_CONTINUE` | Rendering paused (UI shows Rendering + Continue button). |
    | User Stopped | 用户已停止 | `USER_STOPPED` | Stopped by Stop button or script. |
    | Failed | 失败 | `ERR_STOPPED` | Stopped because of an error; details in log. |
    | Done | 完成 | `DONE` | Rendered successfully. |
  - Transitions:
    ```
    create: has_output_path(all OMs) ? Queued : Needs Output
    Needs Output --(set path on all OMs)--> Queued
    Queued <--(toggle Render checkbox)--> Unqueued
    Render pressed: for item in queue order where status==Queued: -> Rendering
    Rendering --Pause--> WILL_CONTINUE --Continue--> Rendering
    Rendering --finished--> Done
    Rendering --error--> Failed
    Rendering --Stop--> User Stopped (+ new item, AE-UI-606)
    Done / Failed / User Stopped: Render checkbox disabled; item cannot be re-rendered (duplicate it instead)
    ```
  - Duplicating a Done item gives the copy status Queued (if its output is valid).
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Scripting `RenderQueueItem.onStatusChanged` callback fires on every status change. Failed items still keep partial output files on disk (UNVERIFIED for movie containers: AE leaves the incomplete file).

### AE-UI-603 · Adding compositions to the Render Queue (添加到渲染队列)

- **What it does:** Creates render items from compositions, footage or the current frame.
- **Exact semantics:**
  - | Command (中文) | Windows | macOS | Result |
    |---|---|---|---|
    | Composition > Add to Render Queue (合成 > 添加到渲染队列) | Ctrl+M | Cmd+M | One item per selected comp (or the active comp); default RS + OM templates for "Movie". |
    | Add active composition or selected items to render queue | Ctrl+Shift+/ (main keyboard) | Cmd+Shift+/ | Same as above. |
    | Composition > Save Frame As > File… (合成 > 帧另存为 > 文件) | Ctrl+Alt+S | Cmd+Opt+S | Item rendering the single frame at the CTI with the default "Frame" RS/OM templates (AE-UI-641). |
    | Composition > Pre-render… (合成 > 预渲染) | — | — | Item with default "Pre-Render" templates and Post-Render Action = Import & Replace Usage targeting the comp (AE-UI-627). |
    | Drag comp(s) from the Project panel into the Render Queue panel | — | — | Item(s) inserted at the drop position. |
    | Drag footage item into the Render Queue panel | — | — | Creates a new comp from the footage (same as New Comp from Selection) and adds it: a format-conversion shortcut. |
  - Each new item gets: the default Render Settings template for its kind (Movie / Frame / Pre-Render / Movie Proxy / Still Proxy; AE-UI-623), the default Output Module template for its kind (AE-UI-639), and an output file name from the default file-name template (AE-UI-640) in the folder resolved per AE-UI-640.
  - If Preferences > Output "Use Default File Name and Folder" is off (UNVERIFIED default), AE prompts with a Save dialog for the output path when adding via Ctrl+M (UNVERIFIED: modern AE no longer prompts and uses the last folder).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion, apps/bae)
- **Notes:** Scripting `app.project.renderQueue.items.add(comp)` returns the new RenderQueueItem. Ctrl+M on an already queued comp adds another item (no de-duplication); AE asks "This composition is already in the render queue" only for Make Movie in some versions (UNVERIFIED). Ctrl+Alt+M / Cmd+Opt+M = Add to Adobe Media Encoder Queue (AE-UI-611).

### AE-UI-604 · Managing render items (管理渲染项)

- **What it does:** Reordering, queuing/unqueuing, deleting, duplicating and revealing render items.
- **Exact semantics:**
  - | Action (中文) | Windows | macOS | Semantics |
    |---|---|---|---|
    | Toggle Render checkbox | click | click | Queued ↔ Unqueued (disabled for Needs Output / Done / Failed / User Stopped). |
    | Remove item (清除) | Delete / Edit > Clear | Delete | Removes from queue; files on disk untouched. Not allowed while rendering. |
    | Move up / down one | Ctrl+Alt+Up / Down | Cmd+Opt+Up / Down | Layer > Arrange > Bring Render Item Forward / Send Render Item Backward (将渲染项前移一层 / 将渲染项后移一层). |
    | Move to top / bottom | Ctrl+Alt+Shift+Up / Down | Cmd+Opt+Shift+Up / Down | Bring Render Item to Front / Send Render Item to Back (将渲染项置于顶层 / 将渲染项置于底层). |
    | Drag to reorder | drag | drag | Insertion line shows the drop gap. |
    | Duplicate (复制) | Ctrl+D | Cmd+D | Copy of settings and output modules; output file names get a unique suffix (e.g. `name_1`), status Queued/Needs Output; Done → Queued. |
    | Duplicate With File Name (与文件名一起复制) | Ctrl+Shift+D | Cmd+Shift+D | Copy keeping the identical output path(s) (will overwrite). |
    | Reveal Composition in Project (在项目中显示合成) | context menu | context menu | Selects the comp in the Project panel. |
    | Reveal in Explorer / Finder (output) | context menu on Output Module / click underlined path | same | Opens the output folder (UNVERIFIED menu string). |
  - Items can be edited only while not rendering.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** All of these are Commands (undoable) in BAE. Renumbering of the # column follows the new order.

### AE-UI-605 · Multiple output modules per render item (多个输出模块)

- **What it does:** One render pass feeds any number of output modules, each encoding the same rendered frames to its own format and file.
- **Exact semantics:**
  - `+` left of the last `Output To:` adds a module with the default OM template; `−` left of any `Output To:` removes it (the last remaining module cannot be removed (UNVERIFIED: the − button is disabled)).
  - Composition > Add Output Module (合成 > 添加输出模块) adds one default module to every selected render item.
  - Selecting output modules and pressing Ctrl+D / Cmd+D duplicates them inside their item.
  - Choosing a template from one module's Output Module menu with several items/modules selected applies it to all selected modules.
  - Rendering: each frame is rendered once at the Render Settings and handed to every module in order; each module applies its own post-processing (resize → crop → channel/depth/premultiply conversion → colour conversion → encode) independently.
  - Dragging an output module to the Project panel imports its file (or, before it is rendered, a placeholder of the right size/duration (UNVERIFIED)).
- **Priority:** P1
- **Owner:** bae-model (+ bae-media, bae-ui-motion)
- **Notes:** Scripting `RenderQueueItem.outputModules.add()` (OMCollection), `OutputModule.remove()`. Implementation hint: share one GPU readback per frame between modules; perform resize/crop on GPU before readback when only one module exists.

### AE-UI-606 · Render, Pause, Continue and Stop (渲染、暂停、继续、停止)

- **What it does:** Starts rendering all Queued items in order, and lets the user pause, continue or stop the batch.
- **Exact semantics:**
  - **Render**: processes every item with status Queued, top to bottom; items added as Queued during a pause are not picked up (UNVERIFIED). Items with other statuses are skipped.
  - **Pause** → button becomes **Continue**; while paused AE cannot be used for anything else (settings cannot change). AE also auto-pauses when the output disk runs out of space; freeing space and pressing Continue resumes.
  - **Stop** (click): current item → User Stopped; a **new item with status Unqueued** is added to the queue (UNVERIFIED: inserted directly below the stopped item), with an **incremented output file name**, and Time Span starting at the last successfully rendered frame (so the new item's first frame = last good frame of the stopped item) and the same end. Remaining queued items are not rendered.
  - **Alt+click Stop** (Windows) / **Option+click Stop** (macOS): current item → User Stopped; a new item is added with status **Queued**, the **same output file name** and the **same (full) duration** as the original (start over).
  - End of batch: success chime; a different sound on failure (AE-UI-610).
- **Priority:** P0 (Render, Stop) / P1 (Pause, resume-from-stop items)
- **Owner:** apps/bae (+ bae-model, bae-ui-motion)
- **Notes:** Scripting `RenderQueue.render()` (blocks until done), `pauseRendering(bool)`, `stopRendering()`, `rendering` (true while rendering or paused). BAE decision: rendering runs on a background job so the UI stays usable (like AE's Queue in AME but in-process); editing a comp that is currently rendering is blocked or the render uses an immutable snapshot of the document version taken at Render time (preferred, Loro version).

### AE-UI-607 · Current Render progress display (当前渲染)

- **What it does:** Shows progress of the item being rendered.
- **Exact semantics:**
  - Header line: `Current Render` twirl, progress bar spanning the panel, **Elapsed** (已用时间) and **Remaining** (剩余时间) estimate.
  - Progress bar colouring (AE 22.0+ with Multi-Frame Rendering): bright green = frames currently rendering, dark green = rendered but not yet written, blue = frames written to disk.
  - Info button / expanded details: concurrent frames rendering, time of current frame, start frame and end frame, **Estimated size** (估计大小) of the output, **Free space** (可用空间) on the target disk.
  - Expanded Current Render details collapse automatically after a timeout; Alt/Option-click the twirl keeps them open.
  - Remaining estimate: `remaining = elapsed / frames_done × (frames_total − frames_done)` (UNVERIFIED: AE's exact estimator; it is smoothed).
  - Frame counts: `frames_total = ceil(time_span_duration × render_fps)` honouring skipFrames.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ apps/bae)
- **Notes:** Older AE (≤ CC 2021) showed a details table (Composition, Layer, Stage, Last/Difference/Average frame time, File Name, File Size, Est. Final File Size, Free Disk Space, Overflows, Current Disk). BAE may show the same data; numbers are informative only.

### AE-UI-608 · Batch information footer (批量渲染信息)

- **What it does:** The bottom bar of the Render Queue summarizes the whole batch.
- **Exact semantics:**
  - | Field (中文) | Content |
    |---|---|
    | Message (消息) | Status message, e.g. "Rendering 1 of 4". |
    | RAM | Memory available to rendering, e.g. "34% used of 32GB" (UNVERIFIED format). |
    | Renders Started (渲染开始时间) | Date/time the batch started. |
    | Total Time Elapsed (已用总时间) | Batch elapsed time **excluding pauses**. |
    | Notify when queue completes (渲染队列完成时通知) | checkbox, scripting `RenderQueue.queueNotify` (AE 22.0). |
- **Priority:** P2
- **Owner:** bae-ui-motion
- **Notes:** —

### AE-UI-609 · Render log files (渲染日志)

- **What it does:** Writes a text log per render item at the chosen verbosity.
- **Exact semantics:**
  - Log menu (日志) values in order (scripting `LogType`):

    | UI string | 中文 | Scripting | Behaviour |
    |---|---|---|---|
    | Errors Only | 仅错误 | `ERRORS_ONLY` | Log file created only if an error occurs. Default. |
    | Plus Settings | 增加设置 | `ERRORS_AND_SETTINGS` | Always written; lists render settings (and output module settings). |
    | Plus Per Frame Info | 增加每帧信息 | `ERRORS_AND_PER_FRAME_INFO` | Settings plus one line per rendered frame (frame number/time and render time). |
  - When a log is written its path appears under the Render Settings heading and the Log menu (clickable).
  - Location: a folder named `<project name> Logs` next to the first output module's file, file name `<comp name> <date/time>.txt` (UNVERIFIED: naming pattern).
- **Priority:** P1
- **Owner:** apps/bae (+ bae-model)
- **Notes:** BAE decision: also emit a machine-readable JSON log (`--log-format json` in `bae render`, AE-UI-643) with per-frame render times for benchmarks.

### AE-UI-610 · Render completion notifications and sounds (渲染完成通知)

- **What it does:** Notifies the user when items or the whole queue finish.
- **Exact semantics:**
  - A chime plays when all items in the queue finished; a different sound plays when a render failed.
  - Per-item Notify checkbox (Notify column) and footer "Notify when queue completes" send a Creative Cloud (desktop/mobile/watch) push notification (AE 22.0 / 2022).
  - Preferences > Notifications "Automatically enable notifications when you add a composition to the Render Queue" makes Notify checked by default for new items (AE-UI-668, Export preferences; formerly Preferences > Notifications). The bell icon at the bottom-left of the panel opens that preference.
- **Priority:** P2 (sounds P1)
- **Owner:** apps/bae (+ bae-ui-motion)
- **Notes:** BAE decision: replace Creative Cloud push with an OS notification (macOS UserNotifications / Windows toast) plus an optional webhook URL; sounds via cpal. No account service dependency.

### AE-UI-611 · Queue in AME and Add to Adobe Media Encoder Queue (AME 中的队列 / 添加到 Adobe Media Encoder 队列)

- **What it does:** Hands queued items (with their Render Settings) or a comp to the separate Adobe Media Encoder application for background encoding with AME presets.
- **Exact semantics:**
  - **Queue in AME** button: sends every Queued item; render settings travel, **output module settings (format, channels, …) do not**, output file name and folder do (AME ignores AE's file-name templates, which can change image-sequence numbering). Scripting `queueInAME(render_immediately)`, `canQueueInAME`.
  - **Composition > Add to Adobe Media Encoder Queue** / File > Export > Add to Adobe Media Encoder Queue (Ctrl+Alt+M / Cmd+Opt+M): sends the comp itself; AME renders it through Dynamic Link with its own presets.
- **Priority:** P2 — BAE has no AME; covered by background rendering.
- **Owner:** apps/bae
- **Notes:** BAE decision: no AME integration. The equivalent is "Render in Background" (separate `bae render` process fed with a snapshot of the project, AE-UI-643) so the user keeps working; keep a menu item with the same position labelled "Queue in Background Renderer". AE 14.0 (CC 2017) added scripting support.

### AE-UI-612 · Application behaviour during rendering (渲染期间的应用行为)

- **What it does:** Defines what the rest of the app does while the Render Queue renders.
- **Exact semantics:**
  - AE renders modally: menus and panels are disabled (only Pause/Stop and the Render Queue panel react; scrolling/expanding the queue is allowed).
  - The Composition panel shows each rendered frame as it completes unless Caps Lock is on (or Preferences > Display "Show Rendering Progress in Info Panel and Flowchart" is off — UNVERIFIED relation); turning off the viewer update speeds up simple renders.
  - The Info panel shows the layer currently being rendered when the Display preference "Show Rendering Progress in Info Panel and Flowchart" is on (now Preferences > Appearance "Show Rendering Progress in Info Panel", AE-UI-663).
  - Disk full on the output volume → automatic Pause (AE-UI-606).
  - Render Settings apply only to the render item; the comp itself is not changed by them.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-ui-motion)
- **Notes:** BAE decision: background rendering (see AE-UI-606) instead of modal lock; viewer preview of rendered frames optional; Caps Lock behaviour kept as "pause viewer updates".

### AE-UI-613 · Render queue automation interface (渲染队列脚本接口)

- **What it does:** Lets scripts and agents read and drive the queue (add items, apply templates, set settings, render).
- **Exact semantics:**
  - Must expose, at minimum, the AE scripting surface semantics: `renderQueue.items.add(comp)`, `item.render`, `item.status`, `item.applyTemplate(name)`, `item.saveAsTemplate(name)`, `item.getSettings(format)` / `setSettings(obj)` with AE's English setting keys (e.g. `"Quality"`, `"Resolution"`, `"Proxy Use"`, `"Effects"`, `"Color Depth"`, `"Frame Blending"`, `"Motion Blur"`, `"Time Span Start"`, `"Time Span Duration"`), `outputModule.applyTemplate`, `outputModule.getSettings` keys (`"Format"`, `"Output File Info" → {"Base Path","Subfolder Path","File Name","File Template","Full Flat Path"}`, `"Crop"`, `"Crop Top"`, `"Crop Left"`, `"Crop Bottom"`, `"Crop Right"`, …), `outputModule.file`, `postRenderAction`, `includeSourceXMP`, `renderQueue.render()/pauseRendering()/stopRendering()`.
  - `GetSettingsFormat` values: `STRING`, `STRING_SETTABLE`, `NUMBER`, `NUMBER_SETTABLE`, `SPEC`.
  - Known AE quirk: in AE the OM `Format` setting is readable but not settable via `setSettings`; BAE should make it settable.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-model)
- **Notes:** BAE decision: expose the same model through MCP tools (`render_queue.add`, `render_queue.set_settings`, `render_queue.render`) and the JS script plugin API, using the AE key strings as aliases so ported ExtendScript render scripts keep working.

## Render Settings

### AE-UI-614 · Render Settings: Quality, Resolution and Size (渲染设置：品质、分辨率、大小)

- **What it does:** The first block of the Render Settings dialog (渲染设置) overrides layer quality and the render resolution for one render item; the settings apply to the root comp **and all nested comps**.
- **Exact semantics:**
  - Dialog title "Render Settings" with the comp name ("Composition 'Comp 1'"). Fields (setting key for scripting in brackets):

    | Parameter (中文) | Type | Default (Best Settings) | Range / enum (menu order, scripting value) | Units | Notes |
    |---|---|---|---|---|---|
    | Quality (品质) [`"Quality"`] | enum | Best | Current Settings (当前设置) −1 / Best (最佳) 2 / Draft (草图) 1 / Wireframe (线框) 0 | — | Overrides every layer's Quality switch (see compositing-3d-color.md, Layer quality AE-CMP-059). Current Settings keeps each layer's switch. |
    | Resolution (分辨率) [`"Resolution"` = [x,y]] | enum/int pair | Full [1,1] | Current Settings (当前设置) / Full (完整) [1,1] / Half (二分之一) [2,2] / Third (三分之一) [3,3] / Quarter (四分之一) [4,4] / Custom… (自定义) [x,y] | factor | Custom opens "Custom Resolution": Render every x pixels horizontally, y pixels vertically (integer, UNVERIFIED range 1..99). Current Settings uses the comp's Resolution/Down Sample Factor. |
    | Size (大小) | read-only text | "1920 x 1080 → 1920 x 1080" | — | px | `comp size → rendered size`. |
  - Rendered size per axis: `out_w = ceil(comp_w / x)`, `out_h = ceil(comp_h / y)` (UNVERIFIED rounding; see compositing-3d-color.md, Resolution / Down Sample Factor AE-CMP-464 for the downsampling semantics, which are identical for final render).
  - Reduced-resolution renders keep the comp's pixel aspect ratio; the output module can upscale back (Resize, AE-UI-634).
  - Adobe's advice (not a rule): when rendering below Full, use Draft quality.
- **Priority:** P0
- **Owner:** bae-render (+ bae-model, bae-ui-motion)
- **Notes:** Render Settings never modify the comp itself. The dialog has three sections: top (Composition), Time Sampling (时间采样), Options (选项).

### AE-UI-615 · Render Settings: Disk Cache, Proxy Use, Effects, Solo Switches, Guide Layers (磁盘缓存、代理使用、效果、独奏开关、引导层)

- **What it does:** Global overrides of cache use, proxies and per-layer switches for the render item.
- **Exact semantics:**

  | Parameter (中文) | Type | Default (Best Settings) | Enum (menu order, scripting value) | Notes |
  |---|---|---|---|---|
  | Disk Cache (磁盘缓存) [`"Disk Cache"`] | enum | Read Only | Read Only (只读) 0 / Current Settings (当前设置) 2 | Read Only: may read cached frames but writes no new frames to the disk cache. Current Settings: obeys Media & Disk Cache preferences (help calls it the "default" setting, but the Best Settings template ships with Read Only). |
  | Proxy Use (代理使用) [`"Proxy Use"`] | enum | Use No Proxies | Current Settings (当前设置) 2 / Use All Proxies (使用所有代理) 1 / Use Comp Proxies Only (仅使用合成代理) 3 / Use No Proxies (不使用代理) 0 | Current Settings uses each item's proxy toggle. See project-comp-layers.md, Proxy use when rendering (AE-PRJ-078). UNVERIFIED: menu order. |
  | Effects (效果) [`"Effects"`] | enum | Current Settings | Current Settings (当前设置) 2 / All On (全部开启) 1 / All Off (全部关闭) 0 | All On renders every applied effect even if its fx switch is off; All Off renders no effects (layer effects; UNVERIFIED whether layer styles count as effects). |
  | Solo Switches (独奏开关) [`"Solo Switches"`] | enum | Current Settings | Current Settings (当前设置) 2 / All Off (全部关闭) 0 | All Off = render as if no layer were soloed. |
  | Guide Layers (引导层) [`"Guide Layers"`] | enum | All Off | Current Settings (当前设置) 2 / All Off (全部关闭) 0 | Current Settings renders guide layers **of the top-level comp only**; guide layers in nested comps are never rendered. |
  - Evaluation: the overrides are applied when building the render graph for each comp in the tree (root and nested), except Guide Layers (root only by definition).
- **Priority:** P0 (Effects, Solo, Guide), P1 (Proxy Use), P2 (Disk Cache)
- **Owner:** bae-render (+ bae-model)
- **Notes:** These map to render-graph build flags in BAE: `effects_override: Option<bool>`, `ignore_solo`, `render_guides_root`, `proxy_policy`. Disk Cache maps to BAE's persistent frame cache policy (read-only during final render is the safe default).

### AE-UI-616 · Render Settings: Color Depth (颜色深度)

- **What it does:** Overrides the project bit depth for this render item.
- **Exact semantics:**
  - `"Color Depth"`: Current Settings (当前设置) −1 (default; uses the project's bpc) / 8 bits per channel (8 位/通道) 0 / 16 bits per channel (16 位/通道) 1 / 32 bits per channel (32 位/通道) 2.
  - The whole comp tree renders at that depth (effects that are 8-bpc-only still process in 8 bpc and are flagged in the comp; see effects.md, effect framework bpc).
  - Output depth is chosen separately in the Output Module (Depth, AE-UI-628); e.g. 32 bpc render into a PNG Millions of Colors output is quantised at encode time.
- **Priority:** P1
- **Owner:** bae-render (+ bae-color)
- **Notes:** Semantics of 8/16/32 bpc (clamping, 16-bit range 0..32768) in compositing-3d-color.md, Project color depth (AE-CMP-443) and Pixel encoding and clamping (AE-CMP-444). BAE decision: BAE's working buffer is RGBA16F by default; "8 bpc" render must still reproduce AE's 8-bpc clamping/quantisation per layer when fidelity mode is on.

### AE-UI-617 · Render Settings: Frame Blending and Motion Blur overrides (帧混合与运动模糊)

- **What it does:** Time Sampling (时间采样) overrides for frame blending and motion blur, including a shutter-angle override.
- **Exact semantics:**

  | Parameter (中文) | Type | Default (Best Settings) | Enum (menu order UNVERIFIED, scripting value) | Notes |
  |---|---|---|---|---|
  | Frame Blending (帧混合) [`"Frame Blending"`] | enum | On for Checked Layers | Current Settings (当前设置) 2 / On for Checked Layers (对选中图层打开) 1 / Off for All Layers (对所有图层关闭) 0 | On for Checked Layers renders frame blending for layers whose Frame Blending switch is set **regardless of the comp's Enable Frame Blending button**. Current Settings honours the comp button. |
  | Motion Blur (运动模糊) [`"Motion Blur"`] | enum | On for Checked Layers | Current Settings (当前设置) 2 / On for Checked Layers (对选中图层打开) 1 / Off for All Layers (对所有图层关闭) 0 | Same pattern with the Motion Blur switch and the comp's Enable Motion Blur. |
  | Override shutter angle (覆盖快门角度（译名待核）) | bool + number | off, value = comp shutter angle (180) | 0..720 (UNVERIFIED) | degrees | Enabled only when Motion Blur ≠ Off for All Layers. Replaces the comp's Shutter Angle for this render; Shutter Phase and samples still come from the comp (UNVERIFIED). |
  - Formulas for whether a layer blurs/blends: transform-keyframes-expressions.md, Motion blur switches and render settings (AE-ANI-102) and Frame blending switches (AE-ANI-099); rendering in compositing-3d-color.md, Motion blur rendering (AE-CMP-064) / Frame blending rendering (AE-CMP-065).
- **Priority:** P0 (Frame Blending/Motion Blur modes), P1 (shutter override)
- **Owner:** bae-render (+ bae-anim, bae-model)
- **Notes:** The Best Settings default "On for Checked Layers" means a final render shows motion blur on checked layers even if the user turned the comp-level button off for faster previews — a frequent AE surprise that BAE must reproduce.

### AE-UI-618 · Render Settings: Field Render and 3:2 Pulldown (场渲染与 3:2 Pulldown)

- **What it does:** Renders interlaced output (two fields per frame sampled at different times) and optionally applies 3:2 pulldown.
- **Exact semantics:**
  - Field Render (场渲染) [`"Field Render"`]: Off (关) 0 (default) / Upper Field First (高场优先) 1 / Lower Field First (低场优先) 2.
  - With field rendering, each output frame `n` at frame duration `T` is assembled from two renders: first field at `t = start + n·T`, second field at `t + T/2`; the first field occupies the upper (even, 0-based rows 0,2,4…) lines for Upper Field First, the lower (odd) lines for Lower Field First (UNVERIFIED row-parity convention).
  - 3:2 Pulldown (3:2 Pulldown) [`"3:2 Pulldown"`]: Off 0 / WSSWW 1 / SSWWW 2 / SWWWS 3 / WWWSS 4 / WWSSW 5 (phase of whole (W) and split (S) frames). Enabled only when Field Render ≠ Off. Used to render 23.976 fps comps to 29.97 fps interlaced video: 4 film frames → 5 video frames (10 fields) in the chosen phase.
  - DV Settings template = Best Settings with Lower Field First.
- **Priority:** P2 — interlaced delivery is rare for motion graphics.
- **Owner:** bae-render (+ bae-media)
- **Notes:** Field order of footage interpretation (separate fields) is in project-comp-layers.md, Interpret Footage. Cropping one row top/bottom in the Output Module swaps field order (AE-UI-635).

### AE-UI-619 · Render Settings: Time Span (时间跨度)

- **What it does:** Selects which part of the comp is rendered.
- **Exact semantics:**
  - `"Time Span"`: Length of Comp (合成长度) 0 / Work Area Only (仅工作区域) 1 (Best Settings default) / Custom (自定义) 2. Menu order in the dialog: UNVERIFIED (Work Area Only, Length of Comp, Custom).
  - Read-only text shows `Start: <tc>  End: <tc>  Duration: <tc>` in the project's time display format; **Set…** (设置) button (or choosing Custom) opens the Custom Time Span dialog (自定义时间范围) with Start, End, Duration fields (editing End updates Duration and vice versa).
  - Scripting keys `"Time Span Start"`, `"Time Span End"`, `"Time Span Duration"` (seconds); `RenderQueueItem.timeSpanStart/timeSpanDuration`; writing them switches Time Span to Custom.
  - Work Area Only follows later changes to the comp work area until rendering starts (UNVERIFIED: AE stores the mode, not the times).
  - Frames rendered: `N = round(duration × render_fps)` (UNVERIFIED rounding), sample times `t_k = start + k / render_fps`, `k = 0..N−1`.
  - Default for new items: comes from the RS template; AE 2026 Best Settings = Work Area Only (observed in a saved AE 2026 project). Older AE versions defaulted Best Settings to Length of Comp (UNVERIFIED).
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Work area definition: project-comp-layers.md, work area. `bae render` CLI `--start/--end` override this (AE-UI-643).

### AE-UI-620 · Render Settings: Frame Rate (帧速率)

- **What it does:** Sets the temporal sampling rate of the render without changing the comp.
- **Exact semantics:**
  - `"Frame Rate"`: Use comp's frame rate (使用合成的帧速率) 0 (default, shows the comp fps in parentheses) / Use this frame rate (使用此帧速率) 1 with a numeric field (`"Use this frame rate"`, fps, UNVERIFIED range 1..999, fractional allowed e.g. 29.97).
  - This is the **sampling** rate; the comp's own frame rate is unchanged. Adobe's help adds that the frame rate of the final encoded movie is determined by the output module settings: output modules normally encode at this render rate; formats with fixed rates show the "Settings mismatch" warning (AE-UI-638). UNVERIFIED: which current OM formats expose their own frame-rate field.
  - Sampling: `t_k = start + k / fps_render`. Rendering a 24 fps comp at 12 fps samples every second comp frame; rendering at 48 fps samples between comp frames (layers with keyframes interpolate; footage holds or frame-blends).
  - Motion blur shutter interval uses the comp's frame duration and shutter angle, not the render rate (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-anim (+ bae-model, bae-render)
- **Notes:** BAE times are in flicks (705,600,000 per second); `t_k` must be computed as `start_flicks + k * 705600000 / fps_render` with rational fps (e.g. 30000/1001) to avoid drift.

### AE-UI-621 · Render Settings: Options (Skip Existing Files, Use Storage Overflow) (选项：跳过现有文件)

- **What it does:** Options for partial and multi-machine sequence renders.
- **Exact semantics:**
  - **Skip existing files** (跳过现有文件) [`"Skip Existing Files"`], bool, default off ("allows multi-machine rendering"). Only for image-sequence outputs: before rendering, AE scans the output folder for files belonging to the same sequence (same name template, start number, frame rate and time span) and renders only missing frame numbers. Multiple machines sharing the same project and output folder each render frames that do not yet exist; AE writes a placeholder/"in progress" file per frame so other machines skip it (UNVERIFIED mechanism).
  - **Use storage overflow** (使用存储溢出（译名待核）): legacy option (removed from current AE, UNVERIFIED version) that continued a render onto overflow volumes (Preferences > Output) when the disk filled.
- **Priority:** P2
- **Owner:** apps/bae (+ bae-media)
- **Notes:** Multi-Machine Settings (RS) + Multi-Machine Sequence (OM) templates use this. BAE decision: implement Skip Existing Files with an atomic "claim" file per frame (`<frame>.claim`, O_EXCL create) so it is safe across machines on a NAS; storage overflow not implemented.

### AE-UI-622 · Render Settings templates (渲染设置模板)

- **What it does:** Named presets of all Render Settings fields; the triangle menu next to "Render Settings:" applies one to the selected items.
- **Exact semantics:**
  - Built-in templates (AE 2026 order as listed by scripting `templates`): **Best Settings** (最佳设置), **Current Settings** (当前设置), **DV Settings** (DV 设置), **Draft Settings** (草图设置), **Multi-Machine Settings** (多机设置). AE also has a hidden internal template `_HIDDEN X-Factor` (not shown in UI).

    | Field | Best Settings | Current Settings | Draft Settings | DV Settings | Multi-Machine |
    |---|---|---|---|---|---|
    | Quality | Best | Current Settings | Draft | Best | Best |
    | Resolution | Full | Current Settings | Half (UNVERIFIED) | Full | Full |
    | Disk Cache | Read Only | Current Settings (UNVERIFIED) | Current Settings (UNVERIFIED) | Read Only | Read Only |
    | Proxy Use | Use No Proxies | Current Settings | Current Settings (UNVERIFIED) | Use No Proxies | Use No Proxies |
    | Effects | Current Settings | Current Settings | Current Settings | Current Settings | Current Settings |
    | Solo Switches | Current Settings | Current Settings | Current Settings | Current Settings | Current Settings |
    | Guide Layers | All Off | Current Settings (UNVERIFIED) | All Off | All Off | All Off |
    | Color Depth | Current Settings | Current Settings | Current Settings | Current Settings | Current Settings |
    | Frame Blending | On for Checked Layers | Current Settings | Current Settings (UNVERIFIED) | On for Checked Layers | On for Checked Layers |
    | Field Render | Off | Off | Off | Lower Field First | Off |
    | Motion Blur | On for Checked Layers | Current Settings | Current Settings (UNVERIFIED) | On for Checked Layers | On for Checked Layers |
    | Time Span | Work Area Only | Work Area Only (UNVERIFIED) | Work Area Only (UNVERIFIED) | Work Area Only | Work Area Only |
    | Frame Rate | comp | comp | comp | comp | comp |
    | Skip Existing Files | off | off | off | off | on |
  - Best Settings column observed in an AE 2026 project's `getSettings`; other columns derived from Adobe's descriptions ("DV = Best + Lower Field First", "Multi-Machine = Best + Skip Existing Files").
  - Ctrl/Cmd + choosing a template from the menu makes it the default for new **movie** items. The menu also lists "Custom…" (opens the dialog) and "Make Template…" (opens the Templates dialog, AE-UI-623).
  - Changing a template never updates items already in the queue.
- **Priority:** P0 (Best/Draft/Current, default selection), P2 (DV, Multi-Machine)
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `RenderQueueItem.templates`, `applyTemplate(name)`, `saveAsTemplate(name)`. Templates are stored in the application preferences, not in the project (AE stores them in its prefs file; items in the project embed full copies).

### AE-UI-623 · Render Settings Templates dialog and per-purpose defaults (渲染设置模板对话框)

- **What it does:** Edit > Templates > Render Settings… (编辑 > 模板 > 渲染设置) manages render-settings templates and chooses which template is the default for each kind of render.
- **Exact semantics:**
  - Defaults area (默认值), one menu each:

    | Default for (中文) | Used when | Default template |
    |---|---|---|
    | Movie Default (电影默认值) | Add to Render Queue | Best Settings |
    | Frame Default (帧默认值（译名待核）) | Composition > Save Frame As > File | Current Settings |
    | Pre-Render Default (预渲染默认值（译名待核）) | Composition > Pre-render | Best Settings |
    | Movie Proxy Default (影片代理默认值（译名待核）) | File > Create Proxy > Movie | Draft Settings |
    | Still Proxy Default (静止代理默认值（译名待核）) | File > Create Proxy > Still | Best Settings |
    UNVERIFIED: the default template per row (Frame Default = Current Settings and Movie Proxy = Draft Settings are from memory).
  - Settings area: Settings Name menu, **New…** (新建), **Edit…** (编辑), **Duplicate** (复制), **Delete** (删除), and **Save All…** (全部保存) / **Load…** (加载) to export/import a template file (`.ars`, After Effects Render Settings; UNVERIFIED extension).
  - Summary text shows the selected template's settings.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** BAE decision: templates stored as JSON in the user config dir, exportable; ship AE's built-in names so imported AE projects/scripts referencing "Best Settings" resolve.

## Output Module

### AE-UI-624 · Output Module settings data model and dialog layout (输出模块设置)

- **What it does:** An output module (输出模块) describes how the frames produced by a render item's Render Settings are post-processed (resize, crop, channel/depth/alpha conversion, color transform) and encoded to one file or file sequence. Every render item owns 1…n output modules (AE-UI-605); each has its own destination path.
- **Exact semantics:**
  - Dialog: **Output Module Settings** (输出模块设置), opened by clicking the underlined template name next to "Output Module:" (输出模块) in the Render Queue. Two tabs: **Main Options** (主要选项) and **Color Management** (色彩管理; the zh help calls it 颜色选项卡).
  - Main Options layout, top to bottom: Format (格式) + Post-Render Action (渲染后动作) row; Include Project Link (包括项目链接), Include Source XMP Metadata (包括源 XMP 元数据); **Video Output** (视频输出) group: Format Options… (格式选项) button + summary text, Channels (通道), Depth (深度), Color (颜色), Starting # (开始 #), Use Comp Frame Number (使用合成帧编号); **Resize** (调整大小) group; **Crop** (裁剪) group; **Audio Output** (音频输出) group with its own Format Options…; status line showing the settings-mismatch warning (AE-UI-638).
  - Persisted fields and the values a fresh AE 2026 module has (from `getSettings` dumps of AE-saved projects):

    | Parameter (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|
    | Format (格式) | enum | per default template (H.264 since AE 23.x, AE-UI-639) | AE-UI-625 | — | scripting key `"Format"` |
    | Include Project Link (包括项目链接) | bool | on | — | — | AE-UI-626 |
    | Include Source XMP Metadata (包括源 XMP 元数据) | bool | off | — | — | off in every factory template |
    | Post-Render Action (渲染后动作) | enum | None | AE-UI-627 | — | |
    | Video Output (视频输出) | bool | on | forced off for audio formats, forced on for sequences | — | |
    | Channels (通道) | enum | RGB | RGB / RGB + Alpha / Alpha | — | AE-UI-628 |
    | Depth (深度) | enum | Millions of Colors (24) | per format | bits/pixel | AE-UI-628 |
    | Color (颜色) | enum | Premultiplied (Matted) | Straight / Premultiplied | — | AE-UI-628 |
    | Starting # (开始 #) | int | 0 | 0 … UNVERIFIED max | frame | AE-UI-629 |
    | Use Comp Frame Number (使用合成帧编号) | bool | on | — | — | AE-UI-629 |
    | Resize (调整大小) | bool | off | — | — | AE-UI-634 |
    | Resize to (调整大小到) | int×2 | comp size | 1 … 30000 (UNVERIFIED) | px | |
    | Lock Aspect Ratio (锁定长宽比) | bool | on | — | — | |
    | Resize Quality (调整大小后的品质) | enum | High | Low / High | — | |
    | Crop (裁剪) | bool | off | — | — | AE-UI-635 |
    | Use Region of Interest (使用目标区域) | bool | off | — | — | |
    | Crop Top/Left/Bottom/Right (顶部/左侧/底部/右侧) | int | 0 | negative allowed | px | |
    | Output Audio (音频输出) | enum | Auto | Auto / On / Off | — | AE-UI-636 |
    | Audio Sample Rate (采样率) | enum | 48.000 kHz | per format | Hz | |
    | Audio Bit Depth (采样深度) | enum | 16 Bit | 8 / 16 / 32 Bit | bits | |
    | Audio Channels (声道) | enum | Stereo | Mono / Stereo | — | |
    | Output To (输出到) | path + file-name template | `[compName].[fileExtension]` in the last-used folder | — | — | AE-UI-640 |
    | Output profile / color space, Convert to Linear Light, Embed Profile, Preserve RGB | — | — | — | — | AE-UI-637 |
    | Format options (格式选项) | per-format blob | per format | — | — | AE-UI-630…633 |

  - Changing Format re-clamps dependent fields (Depth, Channels, Color, audio) to the nearest value the new format allows; the dialog never stores an illegal combination (see AE-UI-628 rule table).
  - The module's settings are a full copy; editing a template later never changes existing modules (AE-UI-639).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion, bae-media)
- **Notes:** Scripting: `OutputModule` (`file`, `name`, `postRenderAction`, `includeSourceXMP`, `templates`, `applyTemplate()`, `saveAsTemplate()`, `getSetting(s)()`, `setSetting(s)()` since AE 13.0 (CC 2014)). Setting keys are the English labels (`"Channels"`, `"Depth"`, `"Starting #"`, `"Output File Info"` …); BAE should accept the same keys in its scripting layer for script portability. BAE decision: store format options as a typed per-encoder struct (serde) rather than an opaque blob.

### AE-UI-625 · Output format list (输出格式)

- **What it does:** The Format menu (格式) of the Output Module chooses the container / file type; each format implies a kind (movie, image sequence, audio only) that locks Video Output / Audio Output.
- **Exact semantics:**
  - Format menu in AE 2026 (alphabetical, as in the menu); scripting label = the menu string. Kind and the bae-media path:

    | # (py-aep id) | Format (中文) | Kind | Extension(s) | Video depths offered | BAE priority | BAE encoder path |
    |---|---|---|---|---|---|---|
    | 0 | AIFF | audio | .aif/.aiff | — | P2 | pure Rust (hound-style PCM writer) |
    | 1 | AVI | movie (Adobe lists "Video for Windows (AVI; Windows only)"; UNVERIFIED whether current macOS builds offer it) | .avi | 24/32/48/64 | P2 | pure Rust RIFF writer, uncompressed only |
    | 2 | DPX/Cineon Sequence (DPX/Cineon 序列) | sequence | .dpx/.cin | 48/64 (10-bit log/lin) | P1 | pure Rust writer |
    | 3 | H.264 | movie | .mp4 (or .3gp / elementary stream) | 24 only | P0 | VideoToolbox (macOS) / Media Foundation (Windows); FFmpeg plugin elsewhere |
    | 4 | IFF Sequence (IFF 序列) | sequence | .iff | 24/32/48/64 | P2 | — (not planned) |
    | 5 | JPEG Sequence (JPEG 序列) | sequence | .jpg | 24 | P1 | `image`/`zune-jpeg` encoder (pure Rust) |
    | 6 | MP3 | audio | .mp3 | — | P2 | system encoder or FFmpeg plugin |
    | 7 | OpenEXR Sequence (OpenEXR 序列) | sequence | .exr | Floating Point / Floating Point+ | P1 | `exr` crate (pure Rust) |
    | 8 | PNG Sequence (PNG 序列) | sequence | .png | 24/32/48/64 | P0 | `png` crate (pure Rust) |
    | 9 | Photoshop Sequence (Photoshop 序列) | sequence | .psd | 24/32/48/64/96/128 | P1 | own flat-PSD writer (single composite + optional alpha) |
    | 10 | QuickTime | movie | .mov | 24/32/48/64 (per codec) | P0 (ProRes on macOS) | AVFoundation/VideoToolbox (macOS); Media Foundation has no ProRes encoder → FFmpeg plugin on Windows/Linux |
    | 11 | Radiance Sequence (Radiance 序列) | sequence | .hdr | Floating Point | P2 | `image` crate HDR writer |
    | 12 | SGI Sequence (SGI 序列) | sequence | .sgi/.rgb/.bw | 24/32/48/64 | P2 | — (not planned) |
    | 13 | TIFF Sequence (TIFF 序列) | sequence | .tif | 24/32/48/64/96/128 | P1 | `tiff` crate (pure Rust) |
    | 14 | Targa Sequence (Targa 序列) | sequence | .tga | 24/32 | P2 | `image` crate TGA writer |
    | 15 | WAV | audio | .wav | — | P1 | pure Rust PCM writer |

  - Kind locks: audio formats force Video Output = off and Output Audio = On; image sequences force Video Output = on and Output Audio = Off (no audio in sequences); movies allow both.
  - Not in the AE Format menu (removed): Animated GIF (removed AE CC 2015), FLV/F4V, Windows Media, MPEG-2/MPEG-4/DPX via AME-embedded exporters (removed AE CC 2014+/2015; UNVERIFIED exact versions), XFL. H.264 returned as a native exporter in AE 23.0 (2022-10) (UNVERIFIED: version — some sources say 23.0 beta then 23.1).
- **Priority:** P0 (H.264, QuickTime/ProRes, PNG Sequence); P1 (EXR, TIFF, JPEG, DPX, PSD, WAV); P2 (rest)
- **Owner:** bae-media (+ bae-model)
- **Notes:** Format ids are py-aep/AEP internal ids (four-char codes `H264`, `MooV`, `png!`, `oEXR`, `8BPS`, `sDPX`, `JPEG`, `.AVI`, `Mp3 `, `IFF `, `AIFF` …), useful when importing AEP files. Adobe's "Supported output formats" page states that image formats are exported at 8 bpc unless otherwise noted. BAE decision (ARCHITECTURE §1): movie encoding uses system frameworks first, FFmpeg only as the optional downloaded plugin; still formats use pure-Rust crates. BAE additionally offers Lottie JSON (AE-UI-656), animated GIF/WebP and APNG as P2 extras that AE lacks natively.

### AE-UI-626 · Include Project Link and Include Source XMP Metadata (包括项目链接、包括源 XMP 元数据)

- **What it does:** Two metadata checkboxes: embed a link back to the source project so other Adobe apps can "Edit Original"; copy XMP metadata of the source footage into the output.
- **Exact semantics:**
  - Include Project Link (包括项目链接): default **on**; when the format supports XMP (MOV, MP4, PSD, TIFF, JPEG, PNG, EXR …; UNVERIFIED full list), writes XMP fields identifying the `.aep` path and comp so Premiere Pro / AME "Edit Original" (编辑原始) re-opens the comp.
  - Include Source XMP Metadata (包括源 XMP 元数据): default **off** in every factory template; when on, XMP of the source footage items used by the comp is aggregated into the output's XMP packet.
  - Formats without an XMP slot ignore both flags silently.
- **Priority:** P2
- **Owner:** bae-media (+ bae-model)
- **Notes:** Scripting: `OutputModule.includeSourceXMP` (read/write); `"Include Project Link"` setting key. BAE decision: store both flags for AEP round-trip; implement "project link" as a BAE-specific XMP field (`bae:project`, `bae:comp`) only for MOV/MP4/PNG; source-XMP passthrough deferred.

### AE-UI-627 · Post-Render Action (渲染后动作)

- **What it does:** Tells AE what to do with the rendered file after the item finishes: nothing, import it, import it and replace a project item's usage, or set it as a proxy.
- **Exact semantics:**
  - Enum, menu order (scripting `PostRenderAction.*`):

    | Value (中文) | Scripting | Effect after the item reaches Done |
    |---|---|---|
    | None (无) | `NONE` | nothing (default) |
    | Import (导入) | `IMPORT` | import the output file/sequence as a footage item (into the project root, UNVERIFIED: or into the folder of the source comp) |
    | Import & Replace Usage (导入和替换用法) | `IMPORT_AND_REPLACE_USAGE` | import, then replace every usage (layer source) of the target item with the new footage |
    | Set Proxy (设置代理) | `SET_PROXY` | import and set as the proxy of the target item (proxy switched on) |

  - For Import & Replace Usage and Set Proxy a pick whip (关联器) appears next to the menu; the target defaults to the render item's comp and can be dragged to any project item.
  - Composition > Pre-render… (合成 > 预渲染) = add to queue with the Pre-Render Default templates (AE-UI-623, AE-UI-639) and Post-Render Action = Import & Replace Usage targeting the comp.
  - File > Create Proxy > Movie/Still… (创建代理) adds an item using the Movie/Still Proxy defaults with Post-Render Action = Set Proxy.
  - Changing the Output Module template can change the action (it is a template field).
  - Chaining: item B may use the output of item A because A's action runs before B starts; if A fails, B renders with the original sources (UNVERIFIED).
  - If the output is a sequence, the import uses the sequence frame rate from Preferences > Import (AE-UI-667) unless the comp frame rate is used (UNVERIFIED: AE uses the comp rate for post-render imports).
- **Priority:** P1 (Import, Import & Replace Usage for pre-render), P2 (Set Proxy)
- **Owner:** bae-model (+ bae-ui-motion, bae-media)
- **Notes:** Scripting: `OutputModule.postRenderAction`, `OutputModule.postRenderTargetComp` (UNVERIFIED name; AE 2021+ exposes the target comp). Proxy data model: see project-comp-layers.md, Proxies. Implementation hint: implement as a bae-model Command emitted by the render runner after the encoder closes the file, so it is undoable.

### AE-UI-628 · Video Output: Channels, Depth and Color (视频输出：通道、深度、颜色)

- **What it does:** Chooses which channels are written, the bit depth per pixel and whether RGB is premultiplied (matted) by alpha or straight.
- **Exact semantics:**
  - Channels (通道), menu order: **RGB** (scripting/py-aep 0), **RGB + Alpha** (1), **Alpha** (2). Default RGB.
    - Alpha writes the alpha channel as a grayscale image (in RGB, or a 1-channel image when the format supports gray, UNVERIFIED).
  - Depth (深度) labels and bits per pixel:

    | Label (中文) | Bits/pixel | Meaning |
    |---|---|---|
    | 256 Grays (256 级灰色) | 8 | 8-bit gray (Alpha channel only, some formats) |
    | 256 Colors (256 色) | 8 | indexed (legacy) |
    | Millions of Colors (数百万种颜色) | 24 | 8 bpc RGB |
    | Millions of Colors+ (数百万种以上颜色) | 32 | 8 bpc RGBA |
    | Trillions of Colors (数万亿种颜色) | 48 | 16 bpc RGB |
    | Trillions of Colors+ (数万亿种以上颜色) | 64 | 16 bpc RGBA |
    | Floating Point (浮点) | 96 | 32-bit float RGB |
    | Floating Point+ (浮点 +) | 128 | 32-bit float RGBA |
    | Floating Point Gray (浮点灰色（译名待核）) | — | float gray (alpha only) |

  - Pairing rule (observed on every AE-saved module): Channels = RGB + Alpha ⇒ only "+" depths; Channels = RGB or Alpha ⇒ only base depths. Switching Channels re-maps Depth to its partner (24↔32, 48↔64, 96↔128).
  - Allowed Depth per format (base/“+” pairs): H.264 and JPEG: 24 only, Channels RGB or Alpha (no RGB + Alpha); Targa: 24/32; DPX/Cineon: 48/64; OpenEXR: 96/128; Radiance: 96 only, no RGB + Alpha; PNG, IFF, SGI: 24/32/48/64; TIFF, Photoshop: 24…128; QuickTime: 24/32/48/64 (per codec: Animation and ProRes 4444/4444 XQ allow +; ProRes 422 family, DNxHR 422, H.264-in-MOV only base; 10-bit YUV codecs give 48); AVI: 24/32/48/64.
  - Color (颜色): **Premultiplied (Matted)** (预乘（遮罩）, py-aep 1, default) or **Straight (Unmatted)** (直接（无遮罩）, 0). Only meaningful when alpha is written; PNG with RGB + Alpha forces Straight.
    ```
    // compositor frame is premultiplied in working space (see compositing-3d-color.md, Alpha model)
    if color == Straight and a > 0: rgb_out = rgb_premul / a   // un-premultiply after color transform
    if color == Straight and a == 0: rgb_out = 0
    if channels == RGB: rgb_out = rgb_premul composited over black (i.e. premultiplied RGB, alpha discarded)
    ```
  - Output of RGB only from a comp with transparency = premultiplied over black (AE does not composite over the comp background colour; the background colour is only shown in the viewer). 
  - Files written with a "+" depth carry a labeled alpha (premultiplied flag or straight flag in the file), so re-importing needs no alpha interpretation.
  - Quantization from float working buffer: `round(clamp(v,0,1) * (2^n − 1))` for 8-bit; AE 16 bpc internal range is 0…32768 but 16-bit files are written 0…65535 (scaled; UNVERIFIED exact rounding). Float formats write unclamped values (overbrights preserved).
- **Priority:** P0
- **Owner:** bae-media (+ bae-render, bae-model)
- **Notes:** Scripting enum names are the labels (`"RGB + Alpha"`, `"Millions of Colors+"`…). Depth set greater than the project bit depth is allowed (upconverts). See compositing-3d-color.md (Bit depth) for 16-bpc internal representation. Implementation hint: do un-premultiply + dither-free quantization in a single compute pass on the GPU before readback; read back in the output's bit depth to halve PCIe traffic for 8-bit outputs.

### AE-UI-629 · Starting # and Use Comp Frame Number (开始 # 与使用合成帧编号)

- **What it does:** Sets the number of the first file of an image sequence.
- **Exact semantics:**
  - Visible/enabled only for sequence formats.
  - `Use Comp Frame Number` (default on): first file number = the comp frame number of the first rendered frame (e.g. work area starting at frame 48 ⇒ `_00048`), counting from the comp's Start Frame Number / start timecode (see project-comp-layers.md, Composition Settings).
  - Off: first file number = `Starting #` (default 0). Help example: Starting # = 38 ⇒ first file `name_00038`.
  - Number = `start + i` for the i-th rendered frame (frames are numbered by output frame index, so with a Render Settings custom frame rate the numbers follow output frames, not comp frames).
  - Zero padding comes from the `[####…]` token in the file name (AE-UI-640); default sequence template is `[compName]_[#####].[fileExtension]` (5 digits).
  - Edge: negative comp frame numbers (comp starting at negative time) — UNVERIFIED: AE writes a leading minus sign.
  - Limit: a still-image sequence holds at most 32,766 frames (Adobe help, "Rendering and exporting still images"); BAE decision: no such limit, but warn when exporting for AE round-trip.
- **Priority:** P0
- **Owner:** bae-media (+ bae-model)
- **Notes:** Scripting keys `"Starting #"`, `"Use Comp Frame Number"`. aerender `-s`/`-e` override the range but not the numbering mode.

### AE-UI-630 · QuickTime format options (QuickTime 选项)

- **What it does:** The QuickTime Options dialog (opened by Format Options… when Format = QuickTime) selects the video codec and its quality / key-frame / bitrate options, plus the audio codec of the MOV.
- **Exact semantics:**
  - Video Codec (视频编解码器) list offered by AE 2026 (py-aep enum names; four-char code):

    | Codec | FourCC | Alpha (“+” depths) | 10/16-bit | BAE priority / path |
    |---|---|---|---|---|
    | Animation | `rle ` | yes | 8-bit only | P2 (pure-Rust RLE writer possible) |
    | Apple ProRes 422 Proxy | `apco` | no | 10-bit | P1 |
    | Apple ProRes 422 LT | `apcs` | no | 10-bit | P1 |
    | Apple ProRes 422 | `apcn` | no | 10-bit | P0 (macOS) |
    | Apple ProRes 422 HQ | `apch` | no | 10-bit | P0 (macOS) |
    | Apple ProRes 4444 | `ap4h` | yes | 12-bit + 16-bit alpha | P0 (macOS) |
    | Apple ProRes 4444 XQ | `ap4x` | yes | 12-bit | P1 |
    | DNxHR/DNxHD | `AVdn` | DNxHR 444 / DNxHD 444 only (UNVERIFIED) | 8/10/12-bit per resolution | P2 (FFmpeg plugin) |
    | DV25 / DV50 / DVCPRO HD variants | `dvc `, `dv5n`, `dvh5`… | no | 8-bit | P2 (not planned) |
    | GoPro CineForm | `CFHD` | yes | 10/12-bit | P2 (FFmpeg plugin) |
    | H.264 (in MOV) | `avc1` | no | 8-bit | P1 |
    | None / Uncompressed RGB 8-bit | `raw ` | (32 with alpha, UNVERIFIED) | 8-bit | P2 |
    | Uncompressed YUV 8-bit 4:2:2 | `2vuy` | no | 8-bit | P2 |
    | Uncompressed YUV 10-bit 4:2:2 | `v210` | no | 10-bit (Depth 48) | P2 |

  - Basic Video Settings: **Quality** (品质) slider 0…100 (default 100 for Animation, UNVERIFIED) — only for codecs with a quality knob; **Key Frame Every** (关键帧间隔) N frames (only temporal codecs); **Frame Reordering** (帧重新排序) checkbox (H.264); **Limit Data Rate / Bitrate** (限制比特率（译名待核）) kbps for H.264-in-MOV.
  - DNxHR/DNxHD: Resolution menu (py-aep `DnxResolution`): DNxHR 444 12-bit, HQX 10-bit, HQ 8-bit, SQ 8-bit, LB 8-bit; DNxHD 1080p/1080i/720p in RGB 444 10-bit, HQX 10-bit, HQ 8-bit, SQ 8-bit, SQ thin raster, LB; plus Color Space (Rec.709 / RGB levels) UNVERIFIED.
  - Audio settings (MOV): Audio Codec Uncompressed (PCM) default; AAC options when available (UNVERIFIED for MOV).
  - The Output Module Depth/Channels menus are re-clamped when the codec changes (e.g. ProRes 422 ⇒ Channels RGB only). UNVERIFIED: for ProRes the Depth menu offers Millions and Trillions of Colors, and the encoder receives 16-bit samples (giving true 10/12-bit output) only when Depth = Trillions and the project is 16/32 bpc.
  - Pixel aspect, field order and frame rate come from the comp / Render Settings; the MOV gets `fiel`, `pasp`, `colr` (nclc from the output color space) atoms.
- **Priority:** P0 (QuickTime + ProRes 422 HQ / 4444 on macOS), P1 (ProRes elsewhere via plugin), P2 (other codecs)
- **Owner:** bae-media
- **Notes:** ProRes encoding on Windows is native in AE since CC 2019 (16.0) / 2020 (UNVERIFIED exact version); BAE decision: ProRes via VideoToolbox/AVFoundation on macOS; on Windows/Linux only through the optional FFmpeg plugin (`prores_ks`), and the Format Options dialog lists only codecs whose encoder is present, greying out the others with a "needs FFmpeg plugin" hint. Writing ProRes 4444 alpha: encoder receives straight or premultiplied per the OM Color setting; ProRes stores alpha unassociated in practice — set the OM default to Straight for 4444 like AE's "High Quality with Alpha" template does (UNVERIFIED).

### AE-UI-631 · H.264 format options (H.264 选项)

- **What it does:** Native H.264 exporter (AE 23.0, 2022-10; uses the Adobe Media Encoder backend in-process) writing MP4 with AAC audio; supports hardware and software encoding.
- **Exact semantics:**
  - Output Module restrictions: Channels RGB (or Alpha) only, Depth Millions of Colors only; frame size must be even (and ≤ codec level limits); mismatches trigger the settings-mismatch warning and automatic adjustment (AE-UI-638).
  - H.264 Options dialog groups (UNVERIFIED: field labels follow AME's H.264 export settings):

    | Parameter (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|
    | Performance: Hardware / Software Encoding (性能：硬件编码 / 软件编码) | enum | Hardware if available | — | — | hardware = Intel QuickSync / NVENC / Apple VT |
    | Profile (配置文件) | enum | Auto via template (High) | Baseline (-1) / Main (1) / High (3) / High 10 (4) | — | py-aep `MPEGProfile`; "Match Render Settings" templates choose by resolution |
    | Level (级别) | enum | Auto by resolution | 3.0 … 6.2 (UNVERIFIED list) | — | |
    | Bitrate Encoding (比特率编码) | enum | VBR, 1 pass | CBR / VBR, 1 pass / VBR, 2 pass (software only) | — | UNVERIFIED |
    | Target Bitrate (目标比特率) | float | 15 (template) | 1 … 300 (UNVERIFIED) | Mbps | templates: 5, 15, 40 |
    | Maximum Bitrate (最大比特率) | float | UNVERIFIED (≈ target × 1.x) | ≥ target | Mbps | VBR only |
    | Key Frame Distance (关键帧距离) | int | auto (UNVERIFIED) | 1 … | frames | |
    | Audio Format (音频格式) | enum | AAC | AAC / MPEG / PCM | — | py-aep `MPEGAudioFormat` |
    | Audio Codec (音频编解码器) | enum | AAC | AAC / AAC+ Version 1 / AAC+ Version 2 | — | |
    | Audio Sample Rate | enum | 48000 | AAC: 16000…48000; MPEG: 32000/44100/48000; PCM: 48000/96000 | Hz | |
    | Audio Bitrate (音频比特率) | enum | 320 kbps (UNVERIFIED) | 16 … 320 (ladder depends on rate × channels) | kbps | |
    | Multiplexer (多路复用器) | enum | MP4 | MP4 / 3GPP / None | — | None = elementary `.264` stream |
    | Stream Compatibility (流兼容性) | enum | Standard | Standard / iPod / PSP | — | |

  - Factory templates "H.264 - Match Render Settings - 5 Mbps / 15 Mbps / 40 Mbps" (note the AE template name pads " 5" with two spaces): profile/level auto from resolution, frame size and rate = render settings.
  - Fresh-install default for both Movie Default and Save Current Preview Default is "H.264 - Match Render Settings - 15 Mbps" (Adobe help).
- **Priority:** P0
- **Owner:** bae-media (+ bae-ui-motion)
- **Notes:** BAE decision: hardware encoder through VideoToolbox (`kVTProfileLevel_H264_*_AutoLevel`) on macOS and Media Foundation H.264 MFT on Windows, both shipped with the OS; software x264 only via the FFmpeg plugin; Linux without plugin offers no H.264 (show reason). Expose the same three templates. Write `colr`/VUI color primaries/transfer/matrix from the output color space (Rec.709 by default) and full/limited range = limited. H.265/HEVC is not in AE's Format menu (AME only); BAE may add HEVC as P2 since the same system APIs support it.

### AE-UI-632 · OpenEXR format options (OpenEXR 选项)

- **What it does:** Options for OpenEXR Sequence output: compression method, half vs full float, luminance/chroma encoding and DWA level.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Compression (压缩) | enum | ZIP (UNVERIFIED; PIZ in pre-2019 ProEXR-based versions) | None, RLE, ZIP, ZIP16, PIZ, PXR24, B44, B44A, DWAA, DWAB, HTJ2K 256, HTJ2K 32 | — | py-aep ids 0…11; HTJ2K added AE 25.x (UNVERIFIED) |
  | 32-bit float (32 位浮点) | bool | off (16-bit half) | — | — | half = IEEE binary16 |
  | Luminance/Chroma (亮度/色度) | bool | off | — | — | n/a for DWAA/DWAB |
  | DWA Compression Level (DWA 压缩级别) | float | 45.0 | > 0 | — | only DWAA/DWAB |

  - Channels written: R, G, B (+ A for RGB + Alpha); Alpha-only writes `A` (UNVERIFIED: or Y). Premultiplied output is the EXR convention; Straight is allowed but non-standard.
  - Values are written unclamped, in the output color space (linear if Convert to Linear Light is on — default "On for 32 bpc", AE-UI-637).
  - Data window = display window = output frame size (no overscan); pixel aspect ratio written to the header.
- **Priority:** P1
- **Owner:** bae-media
- **Notes:** AE 2022+ uses its built-in OpenEXR (not the old fnord ProEXR); layered/multi-pass EXR output is not available natively (Extractor / ProEXR plugins only). BAE decision: `exr` crate supports None/RLE/ZIP/ZIP16/PIZ/PXR24/B44/B44A; DWAA/DWAB/HTJ2K write not supported → map to ZIP with a warning (P2 to add).

### AE-UI-633 · Still-image sequence format options (PNG、TIFF、JPEG、Targa、DPX/Cineon、Photoshop 选项)

- **What it does:** Per-format option dialogs for the image-sequence formats.
- **Exact semantics:**
  - **PNG Options** (PNG 选项): Bit depth 8 / 16 (follows Depth: 24/32 ⇒ 8, 48/64 ⇒ 16); Compression (压缩): older versions offer a dropdown None (default) / Interlaced (Adam7); current AE (UNVERIFIED version, AE 26.x) replaces it with a **Compression slider** between "Larger files, faster" and "Smaller files, slower" (lossless: only file size and encode time change; UNVERIFIED mapping to zlib levels 0…9); **Include HDR10 Metadata** (包括 HDR10 元数据（译名待核）) — 16-bit only, with Color Primaries Rec.709 / P3 D65 / Rec.2020, Luminance Min / Max (nits), Content Light Levels Maximum / Average (nits) (AE 24.x, UNVERIFIED version). PNG with alpha is always written Straight. Source: https://helpx.adobe.com/after-effects/using/rendering-exporting-still-images-still.html ("Set PNG compression level during export").
  - **TIFF Options** (TIFF 选项): LZW Compression (LZW 压缩) checkbox (default off, UNVERIFIED); IBM PC Byte Order (IBM PC 字节顺序) checkbox (little-endian; default on Windows-created modules, UNVERIFIED). 32 bpc writes IEEE float samples; ICC profile embedded when Embed Profile is on.
  - **JPEG Options** (JPEG 选项): Quality (品质) 0 … 10 integer (default 10, UNVERIFIED) with labels Low/Medium/High/Maximum; Format Type (格式类型): Baseline ("Standard") (基线（标准）), Baseline Optimized (基线（优化）), Progressive (连续); Scans (扫描) 3 / 4 / 5 (Progressive only). Quality q maps to libjpeg quality ≈ q × 10 (UNVERIFIED).
  - **Targa Options** (Targa 选项): Resolution 24 / 32 bits/pixel (this value, not OM Depth, decides the file); RLE Compressed (RLE 压缩) checkbox.
  - **DPX/Cineon Settings** (Cineon 设置): Presets (预设) Standard / Over Range / Full Range / Custom (UNVERIFIED list); 10-bit Black Point (10 位黑场) 0…1023 (Standard 95); 10-bit White Point (10 位白场) 0…1023 (Standard 685); Converted Black Point (转换黑场) (Standard 0); Converted White Point (转换白场) (Standard 1.0 / 32767 in 16 bpc units); Current Gamma (当前灰度系数) (Standard 1.7); Highlight Rolloff / Expansion (高光滚降) (Standard 20, UNVERIFIED); Logarithmic Conversion (对数转换) checkbox; File Format (文件格式): FIDO/Cineon 4.5 or DPX; DPX bit depth 8 / 10 / 12 / 16 (default 10). The log conversion formula is shared with the Cineon Converter effect (see effects.md [fx-a], Cineon Converter) and Interpret Footage Cineon settings (project-comp-layers.md, Interpret Footage).
  - **Photoshop Sequence**: no options dialog; writes a flattened composite (plus alpha as a channel/transparency) at 8/16/32 bpc. Layered PSD output is only via Save Frame As > Photoshop Layers (AE-UI-641).
  - **IFF, SGI, Radiance**: no options (SGI writes RLE, UNVERIFIED).
- **Priority:** P0 (PNG), P1 (TIFF, JPEG, DPX, PSD), P2 (Targa, IFF, SGI, Radiance, HDR10 metadata)
- **Owner:** bae-media
- **Notes:** BAE decision: use `png` crate (`Compression::Default`, Adam7 for Interlaced, write `cHRP`/`cICP`/`mDCv`/`cLLi` chunks for HDR10), `tiff` crate (LZW, Deflate), `jpeg-encoder` or `image` JPEG encoder (pure Rust), own DPX writer. Implementation hint: encode sequence frames on a CPU thread pool in parallel with GPU rendering of later frames; sequences parallelise perfectly.

### AE-UI-634 · Output Module Resize (调整大小)

- **What it does:** Scales the rendered frame to another pixel size after rendering, before encoding.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Resize (调整大小) | bool | off | — | — | enables the group |
  | Rendering at (渲染在) | read-only | Render Settings size (after Resolution) | — | px | |
  | Resize to (调整大小到) | int W, H | rendering size | 1 … 30000 (UNVERIFIED) | px | preset menu: Custom, common video/film sizes (NTSC DV, PAL D1/DV, HDV/HDTV 1280×720, HDTV 1920×1080, DCI 2K/4K, UHD … UNVERIFIED list) |
  | Resize % (调整大小 %) | float W%, H% | 100 | — | % | linked to Resize to |
  | Lock Aspect Ratio to (锁定长宽比为) | bool | on | — | — | label shows the ratio, e.g. "16:9 (1.78)" |
  | Resize Quality (调整大小后的品质) | enum | High | Low (0) / High (1) | — | Low for tests |

  - Pipeline: `render (Render Settings size) → resize → crop (AE-UI-635) → channel/depth/colour conversion → encode`.
  - Resizing never changes the pixel aspect ratio written to the file (PAR comes from the comp); to change PAR the comp must change.
  - Interpolation: Low = bilinear, High = bicubic (UNVERIFIED; Adobe only says Low for tests, High for final).
  - Resolution in Render Settings is applied first: Half resolution + Resize to full size renders at half then upsamples (useful for fast drafts).
- **Priority:** P1
- **Owner:** bae-media (+ bae-render)
- **Notes:** Scripting keys `"Resize"`, `"Resize to"` ([w, h]), `"Lock Aspect Ratio"`, `"Resize Quality"`. Implementation hint: do the resize on the GPU (separable bicubic / Lanczos compute pass) before readback; with multiple output modules of different sizes, share one render and resample per module.

### AE-UI-635 · Output Module Crop and Use Region of Interest (裁剪与使用目标区域)

- **What it does:** Removes (or adds, with negative values) rows/columns at the frame edges, or outputs only the viewer's region of interest.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Crop (裁剪) | bool | off | — | — | |
  | Use Region of Interest (使用目标区域) | bool | off | — | — | fills the four values from the comp's current ROI |
  | Top (顶部) | int | 0 | negative = add pixels | px | |
  | Left (左侧) | int | 0 | ditto | px | |
  | Bottom (底部) | int | 0 | ditto | px | |
  | Right (右侧) | int | 0 | ditto | px | |
  | Final Size (最终大小) | read-only | — | — | px | `W − L − R` × `H − T − B` |

  ```
  final_w = resized_w - left - right ; final_h = resized_h - top - bottom
  // negative values pad with transparent black (alpha 0, rgb 0)
  require final_w >= 1 and final_h >= 1   // else the dialog refuses (UNVERIFIED message)
  ```
  - Applied after Resize (UNVERIFIED: AE dialog computes Final Size from the resized dimensions).
  - Adding one row at the top and removing one at the bottom shifts the image by one line, which swaps field dominance (Adobe tip).
  - ROI: the region of interest drawn in the Composition panel (see ui-preview-render-io.md [ui-a]/[ui-b] and compositing-3d-color.md, Region of interest); if the comp has no ROI the checkbox is disabled.
- **Priority:** P1 (Crop), P2 (Use Region of Interest)
- **Owner:** bae-media (+ bae-render)
- **Notes:** Scripting keys `"Crop"`, `"Use Region of Interest"`, `"Crop Top"/"Crop Left"/"Crop Bottom"/"Crop Right"`. Implementation hint: when Resize is off, push the crop rectangle into the render as an ROI so cropped pixels are never rendered.

### AE-UI-636 · Audio Output (音频输出)

- **What it does:** Controls whether and how comp audio is mixed down and written into the output.
- **Exact semantics:**
  - Mode menu (scripting `"Output Audio"`): **Audio Output Auto** (自动音频输出, default, value 3) — write audio only if the comp contains at least one audible audio-capable layer in the rendered span; **Audio Output On** (打开音频输出, 2) — always write (silence if none); **Audio Output Off** (关闭音频输出, 1). Auto was added in AE 17.0 (2020) (UNVERIFIED version).
  - Sample Rate (采样率): 8.000, 11.025, 16.000, 22.050, 24.000, 32.000, 44.100, 48.000, 88.200, 96.000 kHz (subset per format; default 48.000 kHz; MP3 template 44.100 kHz). AIFF/WAV/AVI uncompressed: {8, 11.025, 16, 22.05, 32, 44.1, 48, 88.2, 96} kHz; H.264 AAC: 16…48 kHz.
  - Sample size (采样深度): 8 Bit, 16 Bit (default), 32 Bit (float, WAV only). 24 Bit is not offered.
  - Channels (声道): Mono / Stereo (default Stereo).
  - Format Options… (audio) per format: WAV codec Uncompressed (default) / IMA ADPCM / Microsoft ADPCM / CCITT A-Law / CCITT u-Law / GSM 6.10 (GSM forces Mono; ADPCM/GSM limit rates to {8, 11.025, 22.05, 44.1} kHz, A/u-Law add 16 kHz); MP3 bitrate 16 … 320 kbps (16, 18, 20, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320); H.264 see AE-UI-631; AVI interleave None / 1 frame / ½ s / 1 s / 2 s.
  - Mixdown: sum of all audible layers' audio after Audio Levels / audio effects / layer time remapping and stretch, at the chosen rate; values outside ±1.0 are clipped for integer formats (no limiter). Mono = (L+R)/2 (UNVERIFIED). Audio span = same time span as the video.
  - Image-sequence formats: audio controls disabled (AE-UI-625); to get audio, add a second output module with an audio-only format (e.g. "AIFF 48kHz" template).
- **Priority:** P0 (Auto/On/Off, 48 kHz 16-bit stereo in MOV/MP4), P1 (WAV), P2 (other codecs)
- **Owner:** bae-media (+ bae-anim)
- **Notes:** Audio evaluation (levels, time remap of audio) is defined with layers in project-comp-layers.md and effects.md [fx-c] (Audio effects). Implementation hint: mix audio once per render item on a separate thread in f32, resample with a high-quality sinc resampler only when the comp audio rate differs.

### AE-UI-637 · Output Module Color Management tab (输出模块色彩管理)

- **What it does:** Chooses the color space the rendered frames are converted to before encoding, whether linear light is written, and whether a profile is embedded.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Output Profile (输出配置文件) / Output Color Space (输出色彩空间) | enum | Working Space (project working space) | ICC profiles (Adobe CM) or OCIO color spaces (OCIO/ACES mode) | — | |
  | Convert to Linear Light (转换为线性光) | enum | On for 32 bpc (对 32 bpc 打开) | Off (0) / On (1) / On for 32 bpc (2) | — | On for 32 bpc applies only when the output depth is Floating Point(+) |
  | Embed Profile (嵌入配置文件) | bool | on (UNVERIFIED) | — | — | only formats that carry ICC: PSD, TIFF, PNG, JPEG (+ MOV `colr` tag) |
  | Preserve RGB (保留 RGB) | bool | off | — | — | no conversion: write working-space numbers unchanged |

  - When project color management is off (Working Space = None) the tab shows only Preserve RGB-equivalent behaviour: values are written unconverted.
  - Order: `working-space pixels → (output transform unless Preserve RGB) → (linearize if Convert to Linear Light active) → un-premultiply if Straight → quantize`.
  - Movie formats get color tags (primaries/transfer/matrix) from the output space (Rec.709 for HDTV).
- **Priority:** P1
- **Owner:** bae-color (+ bae-media, bae-model)
- **Notes:** Color-space math, working spaces, OCIO/ACES and the AE 24.x–25.x color-management modes are specified in compositing-3d-color.md, Color management (output). Scripting: `OutputModule` exposes no key for the profile; py-aep notes AE 2026 `getSettings` returns neither `"Convert to Linear Light"` nor `"Preserve RGB"`. zh help calls this tab 颜色 (颜色选项卡).

### AE-UI-638 · Settings mismatch warning (设置不匹配警告)

- **What it does:** When the chosen format constrains frame size, frame rate or other settings that the comp / Render Settings / Output Module violate, AE shows a yellow warning icon and "Settings mismatch" at the bottom of the Output Module dialog and auto-adjusts the output.
- **Exact semantics:**
  - Triggers (examples): H.264 needs even width/height and a level-legal frame size × rate; DV codecs need exact NTSC/PAL sizes and rates; DNxHD needs fixed raster sizes.
  - Clicking the icon shows a message describing the automatic change (e.g. "frame width will be padded/scaled to 1920"); OK accepts; the user can instead change comp/RS/OM settings.
  - The automatic change is applied at render time; the stored settings are not rewritten (UNVERIFIED).
  - For H.264, the same check also runs when an item is queued and shows a modal warning dialog with the adjustment (Adobe "Troubleshoot settings mismatch warnings").
- **Priority:** P1
- **Owner:** bae-media (+ bae-ui-motion)
- **Notes:** BAE decision: each encoder exposes `validate(config) -> Vec<Adjustment>`; the UI lists adjustments and the CLI prints them as warnings. Odd sizes for 4:2:0 codecs: pad by one pixel of edge replication (not scale) so the image stays pixel-exact; record the adjustment.

### AE-UI-639 · Output Module templates and Templates dialog (输出模块模板)

- **What it does:** Named, reusable output-module presets, plus per-purpose defaults used when items are created.
- **Exact semantics:**
  - Factory templates in AE 2026 (from `OutputModule.templates`, menu order is alphabetical):

    | Template | Format / settings (UNVERIFIED where marked) |
    |---|---|
    | AIFF 48kHz | AIFF, audio only, 48 kHz 16-bit stereo |
    | Alpha Only | QuickTime, Channels Alpha (UNVERIFIED codec Animation) |
    | H.264 - Match Render Settings -  5 Mbps | H.264 MP4, target 5 Mbps, AAC |
    | H.264 - Match Render Settings - 15 Mbps | H.264 MP4, target 15 Mbps, AAC (fresh-install default) |
    | H.264 - Match Render Settings - 40 Mbps | H.264 MP4, target 40 Mbps, AAC |
    | High Quality | QuickTime, Apple ProRes 422 HQ (UNVERIFIED; Animation before AE 2020) |
    | High Quality with Alpha | QuickTime, RGB + Alpha, Millions of Colors+ (ProRes 4444, UNVERIFIED) |
    | Lossless | AVI uncompressed on Windows (observed `Format` = AVI); QuickTime Animation on macOS (UNVERIFIED) |
    | Lossless with Alpha | as Lossless with RGB + Alpha, Millions of Colors+ (UNVERIFIED) |
    | Multi-Machine Sequence | Photoshop Sequence for watch-folder rendering (UNVERIFIED) |
    | Photoshop | Photoshop Sequence, RGB, Millions of Colors |
    | TIFF Sequence with Alpha | TIFF Sequence, RGB + Alpha, Millions of Colors+ |
    | `_HIDDEN X-Factor 8/16/32 [Premul]` | internal templates hidden from the UI (used by Dynamic Link / AME); listed by scripting only |

  - Applying a template from the "Output Module:" menu copies its settings into the module; Ctrl/Cmd+choose makes it the default for new movie items. Applying to several selected modules applies to all.
  - Edit > Templates > Output Module… (编辑 > 模板 > 输出模块) dialog: **Defaults** (默认值) menus — Movie Default (电影默认值) = H.264 - Match Render Settings - 15 Mbps; Frame Default (帧默认值) = Photoshop (UNVERIFIED); Save Current Preview Default (保存当前预览默认值) = H.264 - Match Render Settings - 15 Mbps; Pre-Render Default (预渲染默认值) = Lossless (UNVERIFIED); Movie Proxy Default (影片代理默认值（译名待核）) = Lossless with Alpha (UNVERIFIED); Still Proxy Default (静止代理默认值（译名待核）) = TIFF Sequence with Alpha (UNVERIFIED). **Settings** area: Settings Name, New… / Edit… / Duplicate / Delete, Save All… / Load… (template file `.aom`, UNVERIFIED extension).
  - Template edits never affect modules already in the queue. "Make Template…" in the module menu opens the dialog pre-filled with the current module.
- **Priority:** P0 (templates + default), P1 (Templates dialog, Save/Load)
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `OutputModule.templates`, `applyTemplate()`, `saveAsTemplate()`; names include the double space in " 5 Mbps" — BAE must match exactly for scripts. Templates live in application preferences, not the project. BAE decision: ship templates using codecs available on the platform; a template whose encoder is missing stays listed but disabled with a reason; store as JSON alongside Render Settings templates (AE-UI-622).

### AE-UI-640 · Output To: destination and file-name templates (输出到与文件命名模板)

- **What it does:** Each output module has a destination path built from a folder plus a file-name template with bracketed tokens; templates can be saved, chosen per module and set as default.
- **Exact semantics:**
  - Clicking the underlined path next to "Output To:" (输出到) opens a save dialog; the triangle menu lists file-naming templates (文件命名模板) and **Custom…** (自定义). Ctrl/Cmd+choose makes a template the default.
  - Factory file-name templates (UNVERIFIED exact list and order): "Comp Name" `[compName].[fileExtension]` (default), "Project Name" `[projectName].[fileExtension]`, "Comp and Project Name" `[projectName]_[compName].[fileExtension]`, "Comp and Output Module Name" `[compName]_[outputModuleName].[fileExtension]`, "Comp and Render Settings Name" `[compName]_[renderSettingsName].[fileExtension]`, "Comp and Frame Range" `[compName]_[startFrame]-[endFrame].[fileExtension]`, "Comp Folder and Name" `[compName]/[compName].[fileExtension]`, "Comp Folder and Frame Numbers" (sequence), "Project Folder and Comp Name" `[projectFolder]/…` (UNVERIFIED).
  - For sequence formats AE inserts `_[#####]` before the extension: default `[compName]_[#####].[fileExtension]`.
  - Tokens (Add Property menu, 添加属性; names case-insensitive — AE writes `[fileextension]` in lower case in saved projects):

    | Token | Expands to |
    |---|---|
    | `[projectName]` | project file name without extension |
    | `[compName]` | render item comp name |
    | `[renderSettingsName]` | Render Settings template name (e.g. "Best Settings") |
    | `[outputModuleName]` | Output Module template name |
    | `[fileExtension]` | format extension without dot |
    | `[width]`, `[height]` | output frame size after resize/crop |
    | `[frameRate]` | output frame rate |
    | `[aspectRatio]` | pixel aspect ratio (UNVERIFIED) |
    | `[startFrame]`, `[endFrame]`, `[durationFrames]` | render span in frames |
    | `[#]`…`[#########]` | frame number, zero-padded to the number of `#` (sequences) |
    | `[startTimecode]`, `[endTimecode]`, `[durationTimecode]` | span as timecode (`;`/`:` replaced by `_`, UNVERIFIED) |
    | `[channels]` | "RGB" / "RGBA" / "Alpha" (UNVERIFIED strings) |
    | `[projectColorDepth]`, `[outputColorDepth]` | bit depth |
    | `[compressor]` | codec name |
    | `[fieldOrder]`, `[pulldownPhase]` | field render / pulldown settings |
    | `[dateYear]`, `[dateMonth]`, `[dateDay]`, `[timeHour]`, `[timeMins]`, `[timeSecs]`, `[timeZone]` | render start date/time (UNVERIFIED names) |

  - Text outside brackets is literal; `/` (or `\` on Windows) creates subfolders, which AE creates at render time if missing; absolute paths are allowed in a template.
  - "Use Default File Name and Folder" (Preferences > Export, formerly Output, AE-UI-668): new items reuse the previous item's folder; when the resolved name already exists in the queue, AE appends `_1`, `_2`… (e.g. `composition_name_1`).
  - Saving a project and opening it on another machine keeps the path; missing folders mark the item **Needs Output** (AE-UI-602) (UNVERIFIED: only if the volume is missing).
  - Characters illegal on the target OS are replaced (UNVERIFIED with `_`); Adobe warns to avoid high-ASCII for cross-platform / watch-folder renders.
- **Priority:** P0 (path + default template + `[#####]`), P1 (custom templates and tokens)
- **Owner:** bae-model (+ bae-ui-motion, bae-media)
- **Notes:** Scripting `OutputModule.file` (File object); `getSettings()["Output File Info"]` returns `Full Flat Path`, `Base Path`, `Subfolder Path`, `File Name`, `File Template`; `setSettings({"Output File Info": {"File Template": "[compName]/[compName].[fileExtension]"}})` sets a template. BAE decision: tokens identical to AE (case-insensitive), plus `[compName]` sanitised for the file system; store the template string, resolve at render time.

### AE-UI-641 · Save Frame As (帧另存为)

- **What it does:** Composition > Save Frame As (合成 > 帧另存为) renders the frame at the current time: **File…** (文件) as a one-frame render-queue item, or **Photoshop Layers…** (Photoshop 图层) as a layered PSD written immediately.
- **Exact semantics:**
  - File…: adds a render item whose Time Span = the single frame at the CTI, using the Frame Default Render Settings (AE-UI-623, "Current Settings") and the Frame Default Output Module (AE-UI-639, "Photoshop"); the item is queued and the Render Queue opens; it does not render until Render is pressed. File name default `[compName] ([#####]).[fileExtension]` (e.g. `Comp 1 (00000).psd`; UNVERIFIED pattern). Shortcut Ctrl+Alt+S / Cmd+Opt+S ("Add current frame to render queue" in Adobe's shortcut table; see [ui-b]). Adobe: change the Frame Default render settings template to change what this command produces.
  - Photoshop Layers…: saves right away (save dialog) a PSD at comp size; the PSD inherits the project's color bit depth. Each AE layer becomes a Photoshop layer rendered with its effects, masks and transforms at the current time; layer name, opacity, blend mode and visibility are preserved, and a blend mode Photoshop lacks is replaced by the most similar one. Nested comps up to **five levels deep** become layer groups. 3D layers are rendered flat. An embedded flattened composite is always written (it keeps AE's exact look even where layer blend modes were substituted). ICC profile: sRGB IEC61966-2.1 when the project working space is None, otherwise the working-space profile. Source: https://helpx.adobe.com/after-effects/using/rendering-exporting-still-images-still.html
  - Frame Default templates are settable in the Templates dialogs.
- **Priority:** P1 (File as PNG/PSD), P2 (Photoshop Layers)
- **Owner:** bae-media (+ bae-render, bae-ui-motion)
- **Notes:** BAE decision: "Save Frame As > File" also offers PNG as default format (more common than PSD outside Adobe pipelines); Photoshop Layers requires a layered-PSD writer (bae-media) and per-layer renders (bae-render) — P2. Also used by scripts via `RenderQueue.items.add()` with a one-frame span.

### AE-UI-642 · Save Current Preview (保存当前预览)

- **What it does:** Composition > Preview > Save Current Preview… (合成 > 预览 > 保存当前预览) writes the frames currently cached for preview (the work area or preview range) to a movie, without re-rendering frames already cached.
- **Exact semantics:**
  - Uses the **Save Current Preview Default** output module template (fresh install: H.264 - Match Render Settings - 15 Mbps) and the cached preview frames at the preview's resolution/quality.
  - Frames not yet cached are rendered first (with the preview settings, not Best Settings). Audio is included if the preview included audio (UNVERIFIED).
  - File name uses the comp name (exception to "Use Default File Name and Folder").
  - Shortcut Ctrl+Numpad 0 / Cmd+Numpad 0 (see [ui-b]).
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-render, bae-media)
- **Notes:** Preview cache semantics: ui-preview-render-io.md [ui-b], Preview & cache. Implementation hint: export straight from the GPU frame cache to the encoder.

## Headless and multi-frame rendering

### AE-UI-643 · aerender command-line arguments (aerender 命令行参数)

- **What it does:** `aerender` (aerender.exe on Windows, in `Support Files`; `aerender` next to the app on macOS) renders a project's render queue or one comp without the UI; it is the reference for BAE's `bae render` CLI.
- **Exact semantics:**
  - Arguments (Adobe's documented set; flags are case-sensitive as written, UNVERIFIED for case):

    | Argument | Value | Default | Meaning |
    |---|---|---|---|
    | `-h`, `-help` | — | — | print usage |
    | `-version` | — | — | print version and exit, no render |
    | `-v` | `ERRORS` \| `ERRORS_AND_PROGRESS` | `ERRORS_AND_PROGRESS` (UNVERIFIED) | verbosity of messages |
    | `-reuse` | — | off | render in an already-running AE instance if any (AE stays open afterwards; prefs are written on quit only with `-reuse`) |
    | `-mem_usage` | `image_cache_percent max_mem_percent` | from prefs | memory limits for this run |
    | `-mfr` | `ON`\|`OFF` `max_cpu_percent` (1–100) | prefs | Multi-Frame Rendering on/off and CPU cap; cap ignored when OFF (AE 22.0+) |
    | `-project` | path or URI | currently open project | project to open; error if none |
    | `-comp` | comp name | — | render the first queueable RQ item for that comp, or add the comp to the queue and render it; without `-comp` the whole queue is rendered as is and only `-project`, `-log`, `-v`, `-mem_usage`, `-close` are honoured |
    | `-rqindex` | 1-based RQ index | — | render that render-queue item; per-item options apply as with `-comp` |
    | `-s` | start frame | item's start | first frame (comp frame number) |
    | `-e` | end frame | item's end | last frame, inclusive |
    | `-i` | increment | 1 | render every n-th frame and repeat it n−1 times (choppy, faster) |
    | `-RStemplate` | template name | item's settings | apply a Render Settings template; unknown name = error |
    | `-OMtemplate` | template name | module's settings | apply an Output Module template; unknown name = error |
    | `-output` | path or URI | path in project | destination file; `[####]` allowed for sequences |
    | `-log` | path | stdout | log destination |
    | `-sound` | `ON`\|`OFF` | `OFF` | play the completion sound |
    | `-close` | `DO_NOT_SAVE_CHANGES` \| `SAVE_CHANGES` \| `DO_NOT_CLOSE` | `DO_NOT_SAVE_CHANGES` | project handling after render; DO_NOT_CLOSE only with `-reuse` |
    | `-continueOnMissingFootage` | — | off | do not stop on missing footage; log it and render color bars |

  - Examples (Adobe): `aerender -project p.aep -comp "Composition_1" -s 1 -e 10 -RStemplate "Multi-Machine Settings" -OMtemplate "Multi-Machine Sequence" -output frames[####].psd`; `aerender -project p.aep` renders the whole queue.
  - The version and build number are printed to stdout first.
  - With `-comp` and an item already queued, `-s/-e/-i/-RStemplate/-OMtemplate/-output` modify that item for this run only (unless `-close SAVE_CHANGES`).
- **Priority:** P1 (AE users with render farms / scripts); BAE's own CLI is required by ARCHITECTURE for CI and agents
- **Owner:** apps/bae (+ bae-media, bae-model)
- **Notes:** BAE decision: `bae render <project> [--comp NAME | --rq-index N] [--start F --end F --increment N] [--rs-template NAME] [--om-template NAME] [--output PATH] [--log PATH] [--verbose errors|progress] [--continue-on-missing-footage] [--threads/--max-cpu PCT]`; also accept the AE spellings (`-project`, `-comp`, `-s`, `-e`, `-i`, `-RStemplate`, `-OMtemplate`, `-output`, `-rqindex`, `-v`, `-log`, `-continueOnMissingFootage`) as aliases so farm scripts port unchanged; `-reuse`, `-mem_usage`, `-sound`, `-close` accepted and ignored with a warning (BAE CLI never modifies the project file unless `--save`). Project inputs: `.bae`/`.baez` (and `.aep` only if an AEP importer exists).

### AE-UI-644 · Headless render behaviour, exit status and render-only nodes (无界面渲染行为)

- **What it does:** Defines what a command-line render does with errors, missing assets, logging and licensing.
- **Exact semantics:**
  - Per-item errors set the item status to **Failed** (AE-UI-602) and the render continues with the next item; the log records the error. Missing footage stops the render unless `-continueOnMissingFootage` (then footage renders as color bars, i.e. the missing-footage placeholder; see project-comp-layers.md, Missing footage).
  - Missing fonts / effects: AE renders with substitutes and logs a warning (UNVERIFIED for effects: missing effects are skipped as disabled).
  - Progress lines (`ERRORS_AND_PROGRESS`): one per rendered frame with frame number and timecode, e.g. `PROGRESS: 0:00:00:01 (2): 0 Seconds` (UNVERIFIED exact format).
  - Exit status: 0 on success, non-zero when any item failed or the project could not be opened (UNVERIFIED: AE returns 0 in some failure cases; scripts parse the log).
  - Newly launched instances quit after the render; `-reuse` keeps the running instance.
  - Render-only (non-royalty-bearing) mode: an empty file `ae_render_only_node.txt` in `~/Documents/` or `/Users/Shared/Adobe/` (macOS), `C:\Users\<user>\Documents` or `C:\Users\Public\Documents\Adobe` (Windows) lets aerender/watch folder run without sign-in; such nodes cannot use the UI.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-media)
- **Notes:** BAE decision: exit codes 0 = all items Done, 1 = at least one item Failed, 2 = usage error, 3 = project open/parse error, 4 = missing media without `--continue-on-missing-footage`; machine-readable progress via `--progress json` (one JSON object per frame: `{"item":1,"frame":12,"of":240,"elapsed_ms":…}`) for agents; never needs a GPU window (wgpu surface-less device; software fallback adapter allowed with a warning).

### AE-UI-645 · Multi-Frame Rendering (多帧渲染)

- **What it does:** Since AE 22.0 (2021-10) AE renders several frames concurrently on multiple CPU threads for both previews and Render Queue renders.
- **Exact semantics:**
  - Preferences > Memory & Performance (内存和性能) > Performance (性能): **Enable Multi-Frame Rendering** (启用多帧渲染) checkbox, default **on**; **% CPU reserved for other applications** (为其他应用程序保留的 CPU 百分比), slider 0 … 70 %, default 10 %.
  - The number of concurrent frames is chosen dynamically by AE ("Dynamic Composition Analysis": AE renders a few frames, measures memory/CPU use, then ramps concurrency up; UNVERIFIED name), bounded by cores × (1 − reserved %) and RAM.
  - Effects/plug-ins that are not MFR-safe force serialised rendering of the comp; such effects show a yellow warning icon in Effect Controls.
  - Output is identical with MFR on or off (frames are independent); expression state is per frame (no cross-frame globals; expressions that relied on persistent state across frames break — see transform-keyframes-expressions.md [ani-b], evaluation semantics).
  - Render Queue shows per-item concurrency/CPU in the info popup (AE-UI-607).
  - Replaced the old "Render Multiple Frames Simultaneously" multiprocessing (separate background processes, removed AE CC 2015 (13.5)).
  - Speculative Preview (cache frames when idle) builds on MFR; its settings are in Preferences > Previews (AE-UI-662).
- **Priority:** P0 (BAE must render frames concurrently to be "a faster AE"; the preference itself P1)
- **Owner:** bae-render (+ bae-anim, apps/bae)
- **Notes:** aerender `-mfr ON|OFF max_cpu_percent`. Implementation hint: BAE's frame graph evaluates `EvaluatedComp` for N frames in parallel on CPU (bae-anim is pure, Send + Sync), and pipelines GPU submissions (one command encoder per frame, ring of readback buffers); encoders consume frames in order through a reorder buffer. Keep the "reserved CPU %" preference to size the CPU pool. BAE decision: GPU work is shared, so concurrency is chosen to keep the GPU queue full, not by CPU core count.

### AE-UI-646 · Watch Folder and network rendering (监视文件夹与网络渲染)

- **What it does:** File > Watch Folder… (文件 > 监视文件夹) puts AE (or the render engine) into a mode that polls a folder for collected projects and renders them; several machines can share one image-sequence render using Skip Existing Files.
- **Exact semantics:**
  - Projects arrive via File > Collect Files with "Enable 'Watch Folder' Render" (启用"监视文件夹"渲染) and a maximum machine count; Collect Files writes a Render Control File (RCF, `*_RCF.txt`) that watch-folder instances use to claim work (UNVERIFIED: projects are picked up in alphabetical order). Source: https://helpx.adobe.com/after-effects/using/automated-rendering-network-rendering.html
  - Watch-folder mode start: Watch Folder command; or a project named `Watch This Folder.aep` (AE watches its folder when opened); or (Windows) `afterfx.exe -wf <folder>`; `-re` starts the render engine instead of the full app.
  - Multi-machine sequence render: Render Settings "Skip Existing Files" (AE-UI-621) + one output module; each machine renders the next frame whose file does not yet exist (in-progress frames are placeholders), so machines can be stopped/started at any time. A single movie file cannot be split across machines; chain a second item with Post-Render Action to build a movie from the sequence (AE-UI-627).
  - Segment preferences (Preferences > Export) limit files per folder / movie size (AE-UI-668).
- **Priority:** P2
- **Owner:** apps/bae (+ bae-package, bae-media)
- **Notes:** BAE decision: no watch-folder daemon in M1; `bae render` + Skip Existing Files semantics (atomic placeholder creation with `O_EXCL`, then rename on completion) give the same farm capability; Collect Files is owned by bae-package (project-comp-layers.md, Collect Files).

## Import formats

### AE-UI-647 · Still image import formats (静止图像导入格式)

- **What it does:** The raster still formats AE imports as footage (single frame or numbered sequence), with bit depth and the BAE decode path.
- **Exact semantics:**
  - Any still format can be imported as an image sequence (see project-comp-layers.md [prj-a], Import; sequence frame rate from Preferences > Import, AE-UI-667).

    | Format (中文) | Extensions | AE bit depth on import | AE alpha default | BAE priority | BAE decode path |
    |---|---|---|---|---|---|
    | PNG | .png | 8/16 bpc | straight | P0 | `png` / `zune-png` (pure Rust) |
    | JPEG | .jpg, .jpe, .jpeg | 8 bpc | — | P0 | `zune-jpeg` (pure Rust); EXIF orientation honoured (UNVERIFIED for AE) |
    | TIFF | .tif, .tiff | 8/16 bpc (32-bit float TIFF read as 32 bpc, UNVERIFIED) | straight unless ExtraSamples says associated | P1 | `tiff` crate |
    | Photoshop | .psd, .psb | 8/16/32 bpc | per file | P0 (merged), P1 (layers) | own PSD reader, AE-UI-648 |
    | OpenEXR | .exr, .sxr, .mxr | 32 bpc (half → float) | premultiplied | P1 | `exr` crate; layers/AOVs via EXtractoR-like channel selection, P2 |
    | DPX / Cineon | .dpx, .cin | converted to project depth; Cineon log settings in Interpret Footage | — | P1 | own reader (10-bit packed) |
    | Targa | .tga, .vda, .icb, .vst | 8 bpc | per header | P2 | `image` crate |
    | BMP | .bmp, .rle, .dib | 8 bpc | — | P2 | `image` crate |
    | GIF (still / first frame) | .gif | 8 bpc | index transparency | P2 | `gif` crate; animated GIF see AE-UI-649 |
    | Radiance HDR | .hdr, .rgbe, .xyze | 32 bpc | — | P2 | `image` crate |
    | SGI | .sgi, .bw, .rgb | 16 bpc | — | P2 | — (not planned) |
    | IFF (Maya) | .iff, .tdi | 8/16 bpc | — | P2 | — (not planned) |
    | HEIF / HEIC | .heif, .heic | 8 bpc (10-bit HDR: UNVERIFIED) | — | P2 | macOS ImageIO; Windows WIC + HEIF/HEVC extensions (same OS requirement as AE) |
    | WebP | .webp | not supported by AE (UNVERIFIED for 2026) | — | P1 (BAE extra) | `image-webp` (pure Rust) |
    | SVG | .svg | AE 25.x/26 beta: imports as composition with cropped layers only (shape layers), not as footage | — | P1 (BAE extra/native: vector → shape layers) | `usvg` → bae-vector shapes |
    | Camera raw | .crw, .cr2, .nef, .arw, .dng, .raf, .orf, … | via Camera Raw dialog, 16 bpc | — | P2 | not planned (OS RAW decoders via ImageIO/WIC possible) |
    | RLA/RPF | .rla, .rpf | 16 bpc + camera data | — | P2 | not planned |
    | ElectricImage, Softimage PIC, PCX (Win), PICT | .img/.ei, .pic, .pcx, .pct | legacy | — | — | not planned |

  - CMYK PSD/AI/EPS are converted to RGB on import; no input profile can be assigned to CMYK files.
  - Embedded ICC profiles of PSD, TIFF, PNG and JPEG are read and used as the input profile when color management is on (see compositing-3d-color.md, Color management).
- **Priority:** P0 (PNG, JPEG, PSD merged), P1 (TIFF, EXR, DPX, WebP, SVG), P2 (rest)
- **Owner:** bae-media
- **Notes:** Source: Adobe "Supported file formats" (updated 2026-03). ARCHITECTURE §1 fixes image/zune (PNG, JPEG, WebP), exr, psd crates. Import-as choices per extension (AE 2026, measured by py-aep): PSD/PSB/AI/EPS/PDF → Footage, Composition, Composition – Retain Layer Sizes; EXR → Footage or Composition – Retain Layer Sizes (multichannel); SVG → Composition – Retain Layer Sizes only. Implementation hint: decode stills on a worker pool straight into GPU upload buffers; cache decoded frames in the media cache keyed by (path, mtime, interpretation).

### AE-UI-648 · Layered and vector still import: PSD, AI, EPS, PDF (分层与矢量文件导入)

- **What it does:** Imports Photoshop and Illustrator/PostScript/PDF files either merged (footage) or as a composition whose layers map to the source layers.
- **Exact semantics:**
  - **PSD/PSB**: Import As Footage (Merged Layers or Choose Layer…, with Footage Dimensions Document Size / Layer Size), Composition, or Composition – Retain Layer Sizes (合成 - 保持图层大小). In Composition mode: each Photoshop layer → AE layer (name, opacity, blend mode, visibility, position), layer groups → nested comps, layer masks / vector masks → masks or track mattes (UNVERIFIED mapping), adjustment layers and fills kept only if "Editable Layer Styles" (可编辑的图层样式) is chosen, otherwise "Merge Layer Styles into Footage" (合并图层样式到素材); text layers import as footage but can be converted with Layer > Create > Convert to Editable Text; video layers and 3D layers are flattened.
  - **AI / EPS / PS / PDF**: rasterized with continuous rasterization when the layer's Collapse/Continuously Rasterize switch is on (see compositing-3d-color.md [cmp-a], Collapse/continuous rasterize); PDF imports the first page only (Adobe; other pages via the PDF Options dialog, UNVERIFIED). AI with layers imports as composition like PSD. Layer > Create > Create Shapes from Vector Layer converts an AI layer to a shape layer (see shapes.md).
  - Document size = artboard/crop box (AI: artboard; EPS: bounding box; UNVERIFIED).
- **Priority:** P0 (PSD merged + Composition import), P1 (AI/PDF via vector path), P2 (EPS/PS)
- **Owner:** bae-media (+ bae-model, bae-vector)
- **Notes:** Import dialog options are in project-comp-layers.md [prj-a], Import. BAE decision: PSD via own reader (`psd` crate if adequate); AI files that are PDF-compatible (default since AI 9) and PDFs are parsed with a pure-Rust PDF content parser into bae-vector paths so they stay resolution-independent (P1); pure PostScript/EPS without PDF preview is not supported (P2, needs Ghostscript-class runtime, excluded by ARCHITECTURE).

### AE-UI-649 · Video container and codec import (视频导入格式)

- **What it does:** The video formats AE decodes and how BAE decodes each.
- **Exact semantics:**

  | Container / codec | Extensions | AE support | BAE priority | BAE decode path |
  |---|---|---|---|---|
  | QuickTime: H.264 / HEVC | .mov | import & export | P0 | macOS AVFoundation/VideoToolbox; Windows Media Foundation (HEVC needs the OS HEVC extension) |
  | QuickTime: Apple ProRes 422 family / 4444 / 4444 XQ (incl. Rec.2020 PQ/HLG tags) | .mov | import & export | P0 | macOS VideoToolbox (hardware on Apple silicon); Windows/Linux: FFmpeg plugin (MF has no ProRes) |
  | QuickTime: Animation, PNG, uncompressed, DNxHD/HR, CineForm, Photo-JPEG, DV | .mov | import | P1 (Animation/PNG/uncompressed), P2 (others) | pure-Rust decoders for Animation(`rle `)/PNG/uncompressed; others FFmpeg plugin |
  | MPEG-4 / H.264 / HEVC / AAC | .mp4, .m4v | import (export H.264 only) | P0 | system frameworks |
  | H.265 elementary | .hevc | import | P2 | system frameworks |
  | AVCHD / MPEG-TS | .mts, .m2ts, .ts, .m2t | import | P1 | system frameworks (macOS AVFoundation does not open .mts — UNVERIFIED; else FFmpeg plugin) |
  | MPEG-1 / MPEG-2 PS | .mpg, .mpeg, .m2p, .m2v, .mpe, .mpv, .mod | import | P2 | FFmpeg plugin |
  | MXF OP1a / OP-Atom (XDCAM, AVC-Intra, DNxHD/HR, ProRes, JPEG 2000, PCM) | .mxf | import | P1 | FFmpeg plugin (no system MXF demuxer) |
  | XDCAM HD/EX | .mxf, .mp4 | import | P2 | as MXF/MP4 |
  | 3GPP | .3gp, .3g2, .amc | import | P2 | system frameworks |
  | AVI (Video for Windows) | .avi | import & export (export Windows only per Adobe page) | P2 | pure-Rust RIFF demuxer for uncompressed/MJPEG; else FFmpeg plugin |
  | Windows Media | .wmv, .wma, .asf, .asx | import, Windows only | P2 | Media Foundation (Windows only) |
  | Animated GIF | .gif | import | P1 | `gif` crate (frame delays → variable rate footage conformed to the GIF's base rate, UNVERIFIED for AE) |
  | DV stream | .dv | import & export | — | not planned |
  | Flash SWF / FLV / F4V | .swf, .flv, .f4v | import (SWF with alpha, no interactivity) | — | not planned |
  | Open Media Framework | .omf | import & export, Windows only | — | not planned |

  - Variable frame rate footage is conformed to a constant rate (AE reports the average; see project-comp-layers.md [prj-a], Interpret Footage).
  - Alpha in video (ProRes 4444, Animation, PNG, CineForm) is interpreted per Interpret Footage (Straight/Premultiplied/Guess).
  - Embedded color metadata (`colr`/nclc, VUI) drives the media color space when color management is on (AE 24.x+ "media color space"; see compositing-3d-color.md).
- **Priority:** P0 (MOV/MP4 with H.264, HEVC, ProRes), P1 (MXF, AVCHD, Animation/PNG MOV, GIF), P2 (rest)
- **Owner:** bae-media
- **Notes:** BAE decision (ARCHITECTURE §1): system frameworks first, FFmpeg as optional downloaded plugin; the importer must report "needs FFmpeg plugin" per file instead of failing silently. Risk: B2 benchmark (4K ProRes) on Windows depends on the plugin — record as a known gap. Implementation hint: hardware decode into GPU textures (VideoToolbox `CVMetalTexture`, MF DXGI surfaces → wgpu via external memory where possible) to avoid CPU copies; frame-accurate seeking needs a GOP index built on first open and cached in the media cache.

### AE-UI-650 · Camera raw video import (摄像机原始视频导入)

- **What it does:** Cinema-camera raw formats AE decodes through vendor SDKs.
- **Exact semantics:**
  - AE imports: RED (.r3d, with RED Image Processing settings in Interpret Footage > More Options / Source Settings), ARRIRAW (incl. MXF/ARRIRAW), Sony X-OCN (incl. VENICE X-OCN in MXF), Canon Cinema RAW Light (.crm), Panasonic camera formats (P2 MXF), CinemaDNG sequences (.dng via Camera Raw). Blackmagic RAW (.braw) is not native (third-party plugin) (UNVERIFIED for 2026).
  - Source Settings… (源设置) dialog lets the user set debayer/ISO/white balance/colour space per footage item (UNVERIFIED per-format fields).
- **Priority:** P2
- **Owner:** bae-media (+ bae-plugin)
- **Notes:** BAE decision: not in M1; raw decoders are proprietary SDKs → deliver later as optional plugins via bae-plugin's media-importer API.

### AE-UI-651 · Audio import formats (音频导入格式)

- **What it does:** Audio-only files AE imports as audio footage.
- **Exact semantics:**

  | Format | Extensions | AE support | BAE priority | BAE decode path |
  |---|---|---|---|---|
  | WAV / Broadcast WAV | .wav, .bwf | import & export | P0 | pure Rust (`hound`-class or `symphonia`) |
  | AIFF / AIFC | .aif, .aiff | import & export | P1 | `symphonia` (pure Rust) |
  | MP3 | .mp3, .mpa, .mpeg, .mpg, .mpe | import & export | P1 | `symphonia` |
  | AAC / M4A | .aac, .m4a | import & export (export only inside H.264 MP4) | P1 | `symphonia` (AAC-LC) or system frameworks |
  | MPEG-1 Audio Layer II in WAV | .wav | import | P2 | `symphonia` |
  | WMA | .wma | import, Windows only | P2 | Media Foundation |

  - Audio in video containers is decoded by the video path (AE-UI-649).
  - Audio is resampled to the comp/preview rate on playback; BWF timecode metadata is read for the footage start timecode (UNVERIFIED).
- **Priority:** P0 (WAV, audio inside MOV/MP4), P1 (AIFF, MP3, AAC)
- **Owner:** bae-media
- **Notes:** Implementation hint: decode whole audio files to f32 PCM in the media cache on import (small relative to video) so scrubbing and waveform drawing never block on decode. `symphonia` is pure Rust and covers WAV/AIFF/MP3/AAC/FLAC/Vorbis; FLAC/OGG are BAE extras (AE does not import them, UNVERIFIED).

### AE-UI-652 · Project file import: AEP, AEPX, AET, PRPROJ, AAF, C4D, XML (项目文件导入)

- **What it does:** Project-level files AE opens or imports into the current project.
- **Exact semantics:**
  - **AEP / AEPX / AET** (After Effects project, XML project, template): File > Open or Import (imports the whole project into a folder named after the file). Saving: File > Save As > Save a Copy As XML (`.aepx`); "Save a Copy As 2x.x" for one major version back (UNVERIFIED version scope).
  - **Premiere Pro project** (.prproj): Import offers a dialog to choose one sequence or all sequences; each sequence becomes a comp with clips as layers (cuts, opacity, basic transforms; most Premiere effects dropped) (UNVERIFIED detail). Export: File > Export > Adobe Premiere Pro Project.
  - **AAF** (.aaf): Windows only, import.
  - **Cinema 4D** (.c4d): imported as footage via the Cineware effect; File > New > MAXON CINEMA 4D File creates one.
  - **XML** (.xml): Adobe page lists XML import/export (Final Cut Pro XML is not directly importable; FCP XML goes through Premiere) (UNVERIFIED meaning of the XML row).
  - **Maya camera data** (.ma): imports cameras (baked keyframes) as a comp.
- **Priority:** P2 for all in BAE (AEP import P2 as a migration aid)
- **Owner:** bae-media (+ bae-model, bae-motion)
- **Notes:** Project data model and Import of projects into a folder: project-comp-layers.md [prj-a]. BAE decision: native format `.bae`/`.baez` (bae-package); Lottie JSON import is the documented interchange path (ARCHITECTURE: Lottie import/export); AEP import is a later P2 effort (binary RIFX; py-aep shows it is feasible); never write AEP.

### AE-UI-653 · 3D model import (3D 模型导入)

- **What it does:** Since AE 24.0 (October 2023) AE imports 3D models as 3D model layers rendered by the Advanced 3D renderer.
- **Exact semantics:**
  - Formats: glTF binary (.glb), glTF (.gltf + external resources), OBJ (.obj + .mtl), FBX (.fbx, added later, AE 25.x, UNVERIFIED); embedded animations play on the model layer (AE 24.x+, UNVERIFIED version).
  - Imported as footage; placing it in a comp creates a 3D Model layer (requires the Advanced 3D renderer; see compositing-3d-color.md [cmp-b], Advanced 3D).
- **Priority:** P2
- **Owner:** bae-media (+ bae-render)
- **Notes:** BAE decision: P2; if built, use `gltf` crate (pure Rust) and convert OBJ/FBX offline. Not part of M1.

### AE-UI-654 · Data file import: JSON, CSV/TSV, MGJSON, JSX (数据文件导入)

- **What it does:** Imports data files as data footage items readable from expressions (`footage("x.json").sourceData`, `.dataValue()`) and, for MGJSON, as data-driven streams on layers.
- **Exact semantics:**
  - JSON (.json): strict parser since AE 15.1 (illegal syntax is rejected); JSX (.jsx) may be imported and must contain a JS object literal (UNVERIFIED shape requirement).
  - CSV (.csv), TSV (.tsv, .txt): tabular, first row = column names; values accessed by `dataValue([col,row])`.
  - MGJSON (.mgjson): Motion Graphics JSON with sampled streams (time-based data, e.g. GPS/telemetry); dragging to a comp creates a layer with Data properties that can be pick-whipped.
  - Data items have no frame size (0×0) and cannot be placed as visual layers except MGJSON.
- **Priority:** P2
- **Owner:** bae-media (+ bae-anim)
- **Notes:** Expression API for data footage: transform-keyframes-expressions.md [ani-b], data footage. Implementation hint: parse with `serde_json`/`csv` once on import; expose an immutable value tree to QuickJS.

## Export

### AE-UI-655 · File > Export menu (文件 > 导出)

- **What it does:** The Export submenu collects the ways to send a comp out of AE besides the Render Queue panel itself.
- **Exact semantics:**
  - Items in AE 2026 (UNVERIFIED order): **Add to Adobe Media Encoder Queue…** (添加到 Adobe Media Encoder 队列, Ctrl+Alt+M / Cmd+Opt+M), **Add to Render Queue** (添加到渲染队列, Ctrl+M / Cmd+M; also Ctrl+Shift+/ / Cmd+Shift+/ , UNVERIFIED), **Adobe Premiere Pro Project…** (Adobe Premiere Pro 项目), **Export as Motion Graphics Template…** (导出为动态图形模板, `.mogrt` from the Essential Graphics panel), **Cinema 4D Exporter…** (CINEMA 4D Exporter, `.c4d` of 3D layers/cameras), plus entries added by third-party scripts/plug-ins (e.g. Bodymovin is a separate extension panel, not in this menu).
  - Add to Render Queue / Add to AME use the selected comps (Project panel or active comp); with several selected each becomes an item.
  - Removed: Export > QuickTime / Windows Media / FLV / SWF / XFL (removed in AE CC 2014–2015; UNVERIFIED versions).
- **Priority:** P0 (Add to Render Queue), P2 (others)
- **Owner:** apps/bae (+ bae-ui-motion)
- **Notes:** Shortcuts are listed in ui-preview-render-io.md [ui-b]. Essential Graphics / mogrt: ui-preview-render-io.md [ui-a]. BAE decision: Export menu = Add to Render Queue, Export Lottie JSON… (AE-UI-656), Export Frame (= Save Frame As), Export Package (.baez, bae-package); no AME/Premiere/C4D exporters.

### AE-UI-656 · Lottie (bodymovin JSON) export (Lottie 导出)

- **What it does:** Writes a comp as Lottie JSON (the format produced by the third-party Bodymovin extension for AE, played by lottie-web / Skottie / rlottie / dotLottie). In AE it is an extension; in BAE it is a native exporter.
- **Exact semantics:**
  - Root: `v` (version string, BAE writes "5.12.0" or the targeted schema), `fr` (frame rate), `ip`/`op` (work area or comp span in frames), `w`/`h`, `nm`, `ddd` (any 3D layer), `assets` (precomps `layers`, images `p`/`u`/`e`), `fonts`/`chars` (text), `markers` (comp markers: `tm`, `cm`, `dr`), `layers`.
  - Feature map (AE → Lottie; ✓ = exported, ~ = partial, ✗ = not representable):

    | AE feature | Lottie | Status |
    |---|---|---|
    | Precomp layer | `ty: 0`, `refId`, `tm` (time remap), `w`/`h` | ✓ |
    | Solid layer | `ty: 1`, `sc`, `sw`, `sh` | ✓ |
    | Image (still footage) | `ty: 2`, asset `p` (embedded base64 or external) | ✓ |
    | Null | `ty: 3` | ✓ |
    | Shape layer | `ty: 4`, `shapes` (gr, rc, el, sr, sh, fl, st, gf, gs, tr, tm, rp, rd, mm, op, pb, tw, zz) | ✓ (merge paths/offset/zig-zag/twist ~ in players) |
    | Text layer | `ty: 5`, `t.d` (document keys), `t.a` (animators with range/wiggly/expression selectors), `t.p` (path), `t.m` | ✓/~ (glyphs as shapes or font reference) |
    | Audio | `ty: 6` | ~ (players mostly ignore) |
    | Video / image sequence | `ty: 9` / `ty: 8` | ✗ in most players → BAE warns, offers pre-render to image sequence |
    | Adjustment layer | `ty: 12` (reserved in the schema; UNVERIFIED) — not rendered by lottie-web/Skottie | ✗ (warn; BAE offers to precompose the layers below and apply the effects to the precomp) |
    | Camera | `ty: 13` (`pe` perspective, `tr` with `p`/`a`/`or`/`rx`/`ry`/`rz`) | ~ (lottie-web html renderer only) |
    | Light | `ty: 14` (UNVERIFIED) | ✗ |
    | Transform | `ks` (`a`, `p` or split `p.x/p.y`, `s`, `r`/`rx`/`ry`/`rz`/`or`, `o`, `sk`, `sa`) | ✓ |
    | Parenting | `parent` (layer `ind`) | ✓ |
    | Auto-orient | `ao: 1` | ✓ |
    | Time stretch / in–out | `sr`, `st`, `ip`, `op` | ✓ |
    | Time remap | `tm` on precomp | ✓ |
    | Blend modes | `bm` 0 Normal, 1 Multiply, 2 Screen, 3 Overlay, 4 Darken, 5 Lighten, 6 Color Dodge, 7 Color Burn, 8 Hard Light, 9 Soft Light, 10 Difference, 11 Exclusion, 12 Hue, 13 Saturation, 14 Color, 15 Luminosity, 16 Add, 17 Hard Mix | ✓ (others ✗ → Normal + warning) |
    | Masks | `hasMask`, `masksProperties` (`mode` n/a/s/i/l/d/f, `pt`, `o`, `x` expansion, `inv`); feather ✗ in most players | ✓/~ |
    | Track mattes | `tt` 1 Alpha, 2 Alpha Inverted, 3 Luma, 4 Luma Inverted; `td` on the matte layer; AE 23+ any-layer mattes → `tp` (matte parent index) | ✓ |
    | Keyframes | `a: 1`, `k: [{t, s, e?, i:{x,y}, o:{x,y}, h, to, ti}]` — AE speed/influence converted to cubic-bezier `i`/`o` per dimension | ✓ |
    | Expressions | `x` string on the property | ~ (lottie-web only) — BAE default: bake to keyframes |
    | Effects | `ef` (`ty: 5` expression-control groups: Slider 0, Angle 1, Color 2, Point 3, Checkbox 4, Layer 10, Dropdown 7; Fill 21, Stroke 22, Tint 20, Tritone 23, Pro Levels 24, Drop Shadow 25, Gaussian Blur 29, Displacement Map 27, Mesh Warp 28, Puppet 34) | ~ (UNVERIFIED id list; players support only a subset) |
    | Layer styles | `sy` (drop shadow, inner shadow, stroke…) | ~ |
    | Motion blur, frame blending, 3D lights/shadows, collapse transformations, continuous rasterization of AI, quality switches | — | ✗ |

  - Units: Lottie times are frames at `fr` (not seconds); Position/Anchor in px; Rotation in degrees; Opacity 0–100; Scale percent; colors as 0–1 floats `[r,g,b,a]`.
  - Export options (Bodymovin settings mirrored): Glyphs (text as shapes) vs font reference; Hidden layers include/skip; Guided layers skip; Extra comps; Images: embed (base64) / external folder / skip; Compress JSON; Demo HTML; Skottie/Lottie-web target; `.lottie` (dotLottie zip) output.
  - Unsupported features never fail the export: each produces a warning entry (layer path + reason) listed in the export report.
- **Priority:** P1 (BAE native; AE has it only via the Bodymovin extension)
- **Owner:** bae-media (+ bae-model, bae-anim, bae-vector, bae-text)
- **Notes:** Lottie keys per sibling item Notes (transform-keyframes-expressions.md, shapes.md, text.md, compositing-3d-color.md). Lottie import (ARCHITECTURE: import and export) uses the same mapping in reverse and should round-trip everything marked ✓. BAE decision: the exporter evaluates the doc through bae-anim and writes keyframes, with an option to sample ("bake") any property whose evaluation cannot be expressed exactly (expressions, AE-only interpolation, wiggle) at every frame. Implementation hint: test against lottie-web and Skottie golden renders.

### AE-UI-657 · Additional export formats: animated GIF, WebP, APNG (BAE 额外导出格式)

- **What it does:** AE cannot write animated GIF (removed in AE CC 2015; Adobe recommends rendering a movie and converting in Photoshop / Media Encoder). BAE adds direct output modules for web-animation formats.
- **Exact semantics:**
  - BAE output formats (extend AE-UI-625): **Animated GIF** (palette per frame or global; dithering None/Floyd–Steinberg; loop count 0 = infinite; transparency threshold 50 % alpha), **Animated WebP** (lossless/lossy quality 0–100, alpha), **APNG** (8/16-bit, alpha). Frame delays derived from the output frame rate, rounded to 1/100 s for GIF (GIF timing quantum) with error diffusion so total duration is preserved.
  - AE-compatibility: these formats do not exist in AE; AEP/scripting `"Format"` values never refer to them.
- **Priority:** P2
- **Owner:** bae-media
- **Notes:** BAE decision: pure-Rust encoders (`gif` + palette quantiser, `image-webp` lossless; lossy WebP requires libwebp → only via plugin). Listed here so AE-parity reviewers do not mistake them for AE features.

## Preferences

### AE-UI-658 · Preferences dialog, storage and reset (首选项对话框)

- **What it does:** Application-wide settings, opened with Edit > Preferences > [pane] (Windows) / After Effects > Settings > [pane] (macOS); shortcut Ctrl+Alt+; / Cmd+Opt+; opens the first pane.
- **Exact semantics:**
  - Panes in AE 2026 (list order; zh names from the zh-CN help): Startup & Repair (启动和修复), Project (项目), Composition (合成), Previews (预览), Appearance (外观), Grids & Guides (网格和参考线), Labels (标签), Type (类型), Import (导入), Export (导出), Audio (音频), Disk (磁盘), Memory & CPU (内存和 CPU), Video (视频), 3D (3D), Scripting & Expressions (脚本和表达式). Pre-2026 names: General (常规), Display (显示), Output (输出), Media & Disk Cache (媒体和磁盘缓存), Video Preview (视频预览), Auto-Save (自动保存), Memory & Performance (内存和性能), Audio Hardware (音频硬件), Audio Output Mapping (音频输出映射), Notifications (通知) — AE 2026 regrouped the same options (AE 26.0, UNVERIFIED version of the regrouping).
  - Storage: per user and per major version, `~/Library/Preferences/Adobe/After Effects/<version>/` (macOS) or `%APPDATA%\Adobe\After Effects\<version>\` (Windows); includes keyboard shortcuts and workspaces. Written when AE quits normally.
  - Startup & Repair pane: Enable Home Screen (启用主屏幕（译名待核）, default on; forced on for trials); Show System Compatibility Issues (default on); Start in Safe Mode (third-party plug-ins and extensions disabled); Empty All Caches (disk, media and database cache); Reveal Preferences in Finder/Explorer; Reset All Preferences To Defaults (also resets workspaces; restart); Migrate Previous Version Preferences; File Bug Report.
  - Reset at launch: hold Ctrl+Alt+Shift / Cmd+Opt+Shift while starting AE. Restore default keyboard shortcuts: Alt/Option + OK in the confirmation (per Adobe page).
  - Preferences never change project data; options marked "project setting" (bit depth, color, time display, expression engine) live in File > Project Settings (project-comp-layers.md [prj-a], Project settings).
- **Priority:** P0 (dialog, storage, reset), P2 (repair tools)
- **Owner:** bae-ui (+ apps/bae, bae-ui-motion)
- **Notes:** BAE decision: one TOML/JSON settings file per user (`<config dir>/bae/<major>/settings.json`) with a schema; unknown keys preserved; settings are hot-reloaded; `bae --reset-settings` CLI flag replaces the key chord. Each preference below names the semantic item it affects; implementers of that item read the setting through a typed accessor (no ad-hoc keys).

### AE-UI-659 · Project preferences: new-project template, solids folder (“项目”首选项)

- **What it does:** Defaults for newly created projects and project-open behaviour.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Maintain Current Workspace When Opening Projects (打开项目时保持当前工作区) | bool | off (UNVERIFIED) | — | — | else the workspace saved with the project is restored |
  | Dynamic Link with After Effects Uses Project File Name with Highest Number | bool | off (UNVERIFIED) | — | — | Adobe-only |
  | New Project Loads Template (新建项目加载模板) | bool | off | — | — | File > New Project opens a copy of the chosen `.aep`/`.aepx`/`.aet` (untitled) |
  | Choose Project Template (选择项目模板) | path | — | — | — | |
  | New Project Solids Folder (新建项目纯色文件夹) | string | "Solids" (纯色) | — | — | folder that receives new solids (project-comp-layers.md [prj-a], Solids) |

  - Levels of Undo (撤消级别): 32, range 1 … 99 (UNVERIFIED: AE 2026 no longer lists it on the help page; in AE ≤ 25 it was in General). Affects project-comp-layers.md [prj-a], Undo.
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** BAE decision: undo depth unlimited by default (bounded by memory, configurable), since bae-model undo is command-based and cheap; keep a "Levels of Undo" setting for parity.

### AE-UI-660 · Auto-Save (自动保存)

- **What it does:** Periodically writes copies of the open project and keeps a bounded number of versions.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Automatically Save Projects / Save every (保存间隔) | bool + int | on, 20 | 1 … 999 (UNVERIFIED) | minutes | timer counts editing time; no save while rendering (UNVERIFIED) |
  | Save when starting render queue (启动渲染队列时保存) | bool | off (UNVERIFIED) | — | — | auto-save when Render is pressed |
  | Ask to recover an auto-saved project if AE closes unexpectedly (beta) | bool | on (UNVERIFIED) | — | — | AE 26.x beta |
  | Maximum Project Versions (最大项目版本数) | int | 5 | 1 … 99 (UNVERIFIED) | files | oldest deleted |
  | Auto-Save Location (自动保存位置) | enum | Next to Project (项目旁边) | Next to Project / Custom Location (自定义位置) | — | folder "Adobe After Effects Auto-Save" |

  - File names: `<project name> auto-save <n>.aep` with n cycling 1…max (UNVERIFIED pattern). Untitled projects are not auto-saved until first saved (UNVERIFIED).
  - Auto-save never overwrites the user's project file.
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae, bae-package)
- **Notes:** Sibling: project-comp-layers.md [prj-a] (saving). BAE decision: continuous crash-safe journal of Loro CRDT updates (cheap incremental writes every few seconds) plus periodic full snapshots honouring "Maximum Project Versions"; recovery prompt on next launch is on by default.

### AE-UI-661 · Composition preferences (“合成”首选项)

- **What it does:** Defaults for new layers, keyframes and editing behaviour; several change semantics of other features.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Affects (sibling item) |
  |---|---|---|---|
  | Center Anchor Point in New Shape Layers (在新建形状图层中居中放置锚点) | bool | on (UNVERIFIED) | shapes.md, shape tools (anchor at shape centre vs comp centre) |
  | Create Layers at Composition Start Time (在合成开始时创建图层) | bool | on | project-comp-layers.md [prj-b], layer timing: on ⇒ new layers start at comp start; off ⇒ at the CTI |
  | Create Split Layers Above Original Layer (在原始图层上方创建拆分图层) | bool | on | project-comp-layers.md [prj-b], Split Layer |
  | Default Spatial Interpolation to Linear (将空间插值默认为线性) | bool | off | transform-keyframes-expressions.md [ani-a], interpolation (new spatial keys Linear instead of Auto Bezier) |
  | Default Position Properties to Separated Dimensions (将默认位置属性设为分离尺寸) | bool | off | [ani-a], separate dimensions (new layers get separated Position) |
  | Pen Tool Shortcut Toggles Between Pen and Mask Feather Tools (钢笔工具快捷键在钢笔和蒙版羽化工具之间切换) | bool | off (UNVERIFIED) | [ui-b], tools (G cycling) |
  | Preserve Constant Vertex and Feather Point Count when Editing Masks (编辑蒙版时保持常量顶点和羽化点数量) | bool | on | compositing-3d-color.md [cmp-a] masks / shapes.md paths: adding a vertex at one keyframe adds it at all keyframes |
  | Show Bounding Boxes for Layers Within Pre-Comps (显示预合成内图层的定界框) | bool | off (UNVERIFIED) | [ui-b], viewer |
  | Switches Affect Nested Comps (开关影响嵌套合成) | bool | on | [cmp-a], nested comps: comp-level Quality, Effects, Frame Blending, Motion Blur switches on a precomp layer propagate recursively |
  | Synchronize Time of All Related Items (同步所有相关项目的时间) | bool | on | [ui-a], viewers/timeline CTI sync across nested comps |
  | Use System Shortcut Keys (使用系统快捷键) (macOS) | bool | off | [ui-b], shortcuts (Cmd+H/Cmd+M) |
  | On Footage Layer Opens (素材图层打开时) | enum | Layer panel (UNVERIFIED) | double-click on a footage layer: Layer panel / Footage (source) |
  | On Comp Layer Opens (合成图层打开时) | enum | Nested comp (UNVERIFIED) | double-click on a precomp layer: Layer panel / nested comp (Alt+double-click = the other) |
  | Open Layer Panel when Double-clicking with Paint, Roto Brush, and Refine Edge Tools | bool | on | |
  | Synthetic layers duration (合成图层) | enum | Length of Composition (合成长度) | solids, text, shape, null, adjustment, camera, light duration: comp length or Use Custom Duration (使用自定义持续时间) value |

- **Priority:** P0 (Create Layers at Comp Start Time, Default Spatial Interpolation to Linear, Preserve Constant Vertex Count, Switches Affect Nested Comps, synthetic layer duration), P1 (rest)
- **Owner:** bae-model (+ bae-motion, bae-anim, bae-ui-motion)
- **Notes:** Older AE names: these were in General (常规) — e.g. "Create New Layers at Composition Start Time", "Preserve Constant Vertex Count When Editing Masks". BAE decision: commands in bae-motion read these settings at command-creation time and store the resolved values in the command, so replay/undo is deterministic regardless of later preference changes.

### AE-UI-662 · Previews preferences (“预览”首选项)

- **What it does:** Interactive-preview degradation, viewer quality, idle caching and zoom behaviour.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Notes |
  |---|---|---|---|---|
  | Adaptive Resolution Priority (自适应分辨率优先级) | slider | UNVERIFIED (old "Adaptive Resolution Limit" default 1/8) | — | minimum resolution while dragging/scrubbing; see compositing-3d-color.md [cmp-b], fast previews |
  | 3D Renderer Reduces (3D 渲染器减少) | enum | Resolution & Quality (分辨率和品质) (UNVERIFIED) | Resolution & Quality / Only Render Quality (仅渲染品质) | |
  | Zoom Quality (缩放品质) | enum | More Accurate Except Cached Preview | Faster (更快) / More Accurate Except Cached Preview (更精确（缓存预览除外）) / More Accurate (更精确) | viewer resampling when zoom ≠ 100 % |
  | Color Management Quality (色彩管理品质) | enum | More Accurate Except Cached Preview | same three | display transform precision |
  | Automatically Enable Frame Blending and Motion Blur Rendering (自动启用帧混合和运动模糊渲染) | bool | on (UNVERIFIED) | — | turning a layer switch on also enables the comp-level switch |
  | Mute Audio When Preview Is Not Real-time (非实时预览时静音) | bool | off | — | |
  | Cache Frames When Idle (空闲时缓存帧) | bool | on | — | Speculative Preview (AE 22.0) |
  | Idle Delay Before Caching Starts (缓存开始前的空闲延迟) | int | 8 | s | |
  | Cache Frames (缓存帧) | enum | Around Current Time (UNVERIFIED) | From Current Time (从当前时间) / Around Current Time (当前时间前后) / From Start of Range (从范围开头) | |
  | Cache Range (缓存范围) | enum | Work Area Extended by Current Time (UNVERIFIED) | Work Area (工作区域) / Work Area Extended by Current Time (工作区域按当前时间延伸) / Entire Duration (整个持续时间) | |
  | Magnification Type (放大率类型) | enum | Smooth (UNVERIFIED) | Legacy (旧版, fixed steps 33/50/100/200 %…) / Smooth (平滑) | |
  | Magnification Behavior (行为) | enum | Follow Cursor (UNVERIFIED) | Centered (居中) / Follow Cursor (关注光标) | |
  | Trackpad Scrolling (触控板滚动) | enum | Pans (UNVERIFIED) | Zooms (缩放) / Pans (平移) | only with Smooth |

- **Priority:** P1 (P0 for idle caching in BAE since it is a speed feature)
- **Owner:** bae-render (+ bae-ui-motion)
- **Notes:** Preview/cache behaviour itself: ui-preview-render-io.md [ui-b] (preview & cache) and compositing-3d-color.md [cmp-b]. The old Previews pane also had "Show Internal Wireframes", "Alternate RAM Preview", "GPU Information" — gone in AE 2026.

### AE-UI-663 · Appearance preferences (“外观”首选项)

- **What it does:** UI look, handle sizes, label-colour usage and motion-path display.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Notes |
  |---|---|---|---|---|
  | Path Point and Handle Size (路径点和手柄大小) | int | 7 (UNVERIFIED) | 5 … 15 (UNVERIFIED) px | masks, shapes, motion paths |
  | Hardware Accelerate Composition, Layer, And Footage Panels (硬件加速合成、图层和素材面板) | bool | on | — | |
  | Use System Color Picker (使用系统颜色选择器) | bool | off | — | |
  | Show Rendering Progress in Info Panel (在“信息”面板中显示渲染进度) | bool | off (UNVERIFIED) | — | |
  | Disable Thumbnails in Project Panel (在项目面板中禁用缩略图) | bool | off | — | |
  | Show Both Timecode and Frames in Timeline Panel (在“时间轴”面板中同时显示时间码和帧数) | bool | on (UNVERIFIED) | — | |
  | Show Tool Tips (显示工具提示) | bool | on | — | |
  | Show Coach Marks (显示引导提示标记) | bool | on | — | |
  | Color Theme (颜色主题) | enum | Darkest (UNVERIFIED) | Darkest (最暗) / Dark (暗色) / Light (明亮) | |
  | Accessible color contrast (无障碍颜色对比) | bool | off | — | |
  | Use Label Color for Layer Handles and Paths (对图层手柄和路径使用标签颜色) | bool | on | — | |
  | Use Label Color for Related Tabs (对相关选项卡使用标签颜色) | bool | on | — | |
  | Cycle Mask Colors (uses label colors) (循环蒙版颜色（使用标签颜色）) | bool | on | — | new masks get successive label colours |
  | Use Contrasting Color for Mask Path (为蒙版路径使用对比色) | bool | on (UNVERIFIED) | — | |
  | Use Gradients (使用渐变) | bool | on | — | |
  | Motion Path (运动路径) | enum | Show Keyframes for Duration 0:00:15:00 (UNVERIFIED) | No Motion Path (无运动路径) / All Keyframes (所有关键帧) / No More Than N keyframes (不超过, default N = 5) / Show Keyframes for Duration (显示关键帧持续时间) | how much of a layer's motion path the viewer draws around the CTI |

- **Priority:** P1 (handle size, label colours, motion path), P2 (theme options)
- **Owner:** bae-ui (+ bae-ui-motion)
- **Notes:** Pre-2026 these were split between General, Display and Appearance (Brightness slider, Affect Label Colors, Highlight Color). Motion path drawing: ui-preview-render-io.md [ui-b]. BAE decision: ship dark/light themes via bae-ui theme tokens; handle size in logical px scaled by DPI.

### AE-UI-664 · Grids & Guides preferences (“网格和参考线”首选项)

- **What it does:** Appearance and spacing of the grid, proportional grid, guides, safe margins and transparency checkerboard.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Grid Color (网格颜色) | color | UNVERIFIED (light green) | — | — |
  | Grid Style (网格样式) | enum | Lines (UNVERIFIED) | Lines / Dashed Lines / Dots | — |
  | Gridline every (网格间隔) | float | 100 (UNVERIFIED) | > 0 | px |
  | Subdivisions (分区) | int | 4 (UNVERIFIED) | ≥ 1 | — |
  | Proportional Grid Horizontal / Vertical (对称网格 水平 / 垂直) | int | 4 / 4 (UNVERIFIED) | ≥ 1 | divisions |
  | Guides Color / Style (参考线 颜色 / 样式) | color / enum | cyan (UNVERIFIED) / Lines | Lines / Dashed Lines | — |
  | Action Safe (动作安全) | % | 10 | 0 … 50 | % inset per side |
  | Title Safe (标题安全) | % | 20 | 0 … 50 | % |
  | Center-Cut Action Safe (中心剪切动作安全) | % | 32.5 | 0 … 50 | % (4:3 inside 16:9) |
  | Center-Cut Title Safe (中心剪切标题安全) | % | 40 | 0 … 50 | % |
  | Transparency Grid Size (网格大小) | enum | Medium (UNVERIFIED) | Small / Medium / Large | — |
  | Transparency Grid Colors (网格颜色) | enum + 2 colors | Light (light gray / white) | Light / Medium / Dark / Custom (颜色 1, 颜色 2) | — |

  ```
  // safe-margin rectangles (centered), for frame W×H and inset percent p:
  inset_x = W * p / 200 ; inset_y = H * p / 200   // p is the total of both sides (UNVERIFIED: AE inset = p/2 per side)
  ```
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-ui)
- **Notes:** Grid/guide/safe-margin display and snapping behaviour: ui-preview-render-io.md [ui-b] (viewer interaction). The per-side interpretation of the percentage must be checked against AE (title safe 20 % of 1920 ⇒ 192 px per side if p/2).

### AE-UI-665 · Labels preferences (“标签”首选项)

- **What it does:** Names and colours of the 16 labels and which label each new item/layer type receives.
- **Exact semantics:**
  - Label Defaults (标签默认值) per type: Composition (合成), Video (视频), Audio (音频), Still (静止图像), Folder (文件夹), Solid (纯色), Camera (摄像机), Light (灯光), Adjustment (调整), Null Object (空对象), Shape (形状), Text (文本), 3D Models (3D 模型), Other 3D (其他 3D).
  - Factory defaults (UNVERIFIED, AE ≥ 2018 scheme): Composition = Sandstone, Video = Aqua, Audio = Sea Foam, Still = Lavender, Folder = Yellow, Solid = Red, Camera = Pink, Light = Pink (UNVERIFIED), Adjustment = Peach (UNVERIFIED), Null Object = Red, Shape = Blue, Text = Red, 3D Models / Other 3D = UNVERIFIED.
  - Label Colors (标签颜色): 16 labels, each with Label Name (标签名称) and colour; factory names in order: Red, Yellow, Aqua, Pink, Lavender, Peach, Sea Foam, Blue, Green, Purple, Orange, Brown, Fuchsia, Cyan, Sandstone, Dark Green (index 1…16; 0 = None). Colour values: see project-comp-layers.md [prj-b], Labels.
  - Changing a label's colour or name updates every item using that label index (labels are stored by index).
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Label data model, scripting `label` index and keyframe labels (AE 22.x+): project-comp-layers.md [prj-b], Labels. BAE decision: label table stored in app settings and copied into the project on save so projects render identical label colours on other machines (AE does not do this).

### AE-UI-666 · Type preferences (“类型”首选项)

- **What it does:** Font-menu presentation, missing-font warning and (older versions) the text engine.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Notes |
  |---|---|---|---|---|
  | Preview Size (预览大小) | enum | Medium (UNVERIFIED) | Small / Medium / Large / Extra Large (UNVERIFIED) | font preview in Character panel menu |
  | Number of Recent Fonts to Display (要显示的近期字体数量) | int | 10 (UNVERIFIED) | 0 … 30 (UNVERIFIED) | |
  | Do not warn me about missing fonts when opening projects (打开项目时不提示缺失字体警告) | bool | off | — | Resolve Fonts dialog |
  | Text Engine (文本引擎) (AE 22.x–25.x) | enum | Latin and East Asian (UNVERIFIED) | Latin and East Asian / South Asian and Middle Eastern | selects the line composer; affects text.md (layout, RTL/complex scripts) |
  | Show Font Names in English (以英文显示字体名称) | bool | off (UNVERIFIED) | — | CJK font names |

- **Priority:** P1
- **Owner:** bae-text (+ bae-ui-motion)
- **Notes:** Text layout semantics: text.md (fonts, CJK/vertical). BAE decision: one shaping engine (parley/swash with full complex-script support) — no engine switch; keep the "font names in English" toggle since BAE targets Chinese users.

### AE-UI-667 · Import preferences (“导入”首选项)

- **What it does:** Defaults applied when footage is imported; several directly change Interpret Footage values.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Affects |
  |---|---|---|---|---|
  | Indeterminate Media NTSC (非确定性媒体 NTSC) | enum | Use Dropframe (使用丢帧时间码) (UNVERIFIED) | Use Dropframe / Use Non-Dropframe (使用非丢帧时间码) | timecode of 29.97/59.94 sequences without timecode |
  | Interpret Unlabeled Alpha As (将未标记的 Alpha 通道解释为) | enum | Ask User (询问用户) (UNVERIFIED) | Ask User / Guess (猜测) / Ignore Alpha (忽略 Alpha 通道) / Straight (Unmatted) (直接（无蒙版）) / Premultiplied (Matted with Black) (预乘（以黑色蒙版）) / Premultiplied (Matted with White) (预乘（以白色蒙版）) | project-comp-layers.md [prj-a], Interpret Footage alpha |
  | Drag Import Multiple Items As (拖拽导入多个项目为) | enum | Footage (素材) (UNVERIFIED) | Footage / Composition (合成) / Composition – Retain Layer Sizes (合成 – 保留图层尺寸) | layered files dragged from the OS |
  | Footage Interpretation on Replacement | enum | Reset (重置) (UNVERIFIED) | Reset / Remember (记住) | Replace Footage keeps or resets interpretation |
  | Still Footage (静止素材) | enum + time | Use Composition Length (使用合成时长) | or Use Custom Duration (使用自定义持续时间), default 0:00:10:00 when chosen (UNVERIFIED) | still layer duration when added to a comp |
  | Sequence Footage Frames Per Second (每秒帧数) | float | 30 | 0.001 … 999 (UNVERIFIED) | frame rate assigned to imported image sequences |
  | Report Missing Frames (报告缺失帧) | bool | on | — | gaps in sequences → warning; missing frames render as color bars/placeholder |
  | Verify Individual Files (验证个体文件) | bool | off | — | slower, checks every file |
  | Enable hardware accelerated decoding (启用硬件加速解码) | bool | on | — | H.264/HEVC decode on GPU |
  | Automatic Footage Reloading (自动重新加载素材) | enum | Non-Sequence Footage (非序列素材) | All Footage Types (所有素材类型) / Non-Sequence Footage / Off (关) | footage changed on disk is reloaded when AE regains focus |

- **Priority:** P0 (unlabeled alpha, still duration, sequence fps, auto reload), P1 (rest)
- **Owner:** bae-media (+ bae-model)
- **Notes:** Interpret Footage semantics: project-comp-layers.md [prj-a]. BAE decision: default Interpret Unlabeled Alpha = Guess without a modal prompt (non-blocking import), with the guess shown in the Project panel; file watching via OS notifications (FSEvents/ReadDirectoryChangesW) instead of reload-on-focus.

### AE-UI-668 · Export (Output) preferences (“导出”首选项)

- **What it does:** Render-output defaults: automatic file names, completion sound, ProRes hardware encoding, segmenting, notifications, storage overflow.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Affects |
  |---|---|---|---|---|---|
  | Use Default File Name and Folder (使用默认文件名和文件夹) | bool | on (UNVERIFIED) | — | — | AE-UI-640 |
  | Preview output during render queue output (渲染队列输出期间预览输出) | bool | off | — | — | sends frames to Mercury Transmit |
  | Play Sound When Render Finishes (渲染完成时播放声音) | bool | on | — | — | AE-UI-610 |
  | Enable ProRes hardware accelerated encoding, if available (启用 ProRes 硬件加速编码（如果可用）) | bool | on (UNVERIFIED) | — | — | AE-UI-630; restart required |
  | Segment Sequences At (按指定数量分割序列) | bool + int | off, 700 (UNVERIFIED) | ≥ 1 | files per folder | new subfolder after N files; ignored with Skip Existing Files |
  | Segment Video-only Movie Files At (分割纯视频影片文件于) | bool + int | off, 1024 (UNVERIFIED) | ≥ 1 | MB | ignored when the module writes audio; not allowed to a volume root |
  | Automatically enable notifications when you add a composition to the Render Queue (将合成添加到渲染队列时自动启用通知) | bool | on (UNVERIFIED) | — | — | AE-UI-610 |
  | Overflow Volumes / Minimum Diskspace Before Overflowing (溢出卷 / 溢出前的最小磁盘空间) (≤ AE 25) | list + int | none, 20 (UNVERIFIED) | — | MB | Render Settings "Use storage overflow" (AE-UI-621) |
  | Audio Block Duration (音频块时长) (Audio pane in 2026) | time | 1 s (UNVERIFIED) | — | s | audio rendering chunk length |

  - Segmenting movies: when a segment reaches the size, the file is closed and the next one gets `_n` appended (UNVERIFIED suffix).
- **Priority:** P1 (default name, sound), P2 (segmenting, overflow, ProRes HW toggle)
- **Owner:** bae-media (+ bae-model)
- **Notes:** BAE decision: segmenting/overflow not implemented in M1 (modern file systems); keep the settings readable from AE imports only.

### AE-UI-669 · Audio preferences (“音频”首选项)

- **What it does:** Audio device selection for preview playback and the mapping of the stereo mix to device channels.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | Device Class (设备类型) | enum | system default (Core Audio on macOS; MME/WASAPI on Windows, UNVERIFIED) | Built-in / third-party / ASIO (Windows) / Core Audio (macOS) |
  | Default Input (默认输入设备) | device | system default | |
  | Default Output (默认输出设备) | device | system default | |
  | I/O Buffer Size (I/O 缓冲器大小) | int | device-dependent (UNVERIFIED 512) | samples |
  | Sample Rate (采样率) | enum | device rate (44.1 / 48 / 96 kHz …) | hardware rate; comp audio is resampled to it |
  | Output Channel Mapping Left / Right (左声道 / 右声道) | channel | device channels 1 / 2 | |
  | Audio Block Duration (音频块时长) | time | UNVERIFIED | render chunk length (also listed in AE-UI-668) |

- **Priority:** P1
- **Owner:** bae-media (+ bae-ui-motion)
- **Notes:** Preview audio behaviour: ui-preview-render-io.md [ui-b] (preview). BAE decision: `cpal` (pure Rust over Core Audio / WASAPI / ALSA); ASIO only as an optional build feature.

### AE-UI-670 · Disk preferences: disk cache, media cache, XMP (“磁盘”首选项)

- **What it does:** Location and size of the persistent disk cache, the conformed media cache and XMP behaviour on import.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Enable Disk Cache (启用磁盘缓存) | bool | on | — | — | blue cache bar in the timeline |
  | Maximum Disk Cache Size (磁盘缓存最大容量) | int | 10 % of the volume, ≤ 100 | ≥ 1 | GB | AE requires 10 GB free beyond this and warns otherwise |
  | Choose Folder (选择文件夹) | path | user cache folder | — | — | not a volume root |
  | Empty Disk Cache (清空磁盘缓存) | button | — | — | — | also Edit > Purge > All Memory & Disk Cache |
  | Enable Preview from Disk Cache (启用从磁盘缓存预览) | bool | on (UNVERIFIED) | — | — | play frames from disk when RAM is full |
  | Enable Compressed Frames (Lossless) (启用压缩帧（无损）) | bool | on (UNVERIFIED) | — | — | lossless compression of disk-cache frames |
  | Conformed Media Cache Database / Cache (数据库 / 缓存) | path | `…/Common/Media Cache Files` (UNVERIFIED) | — | — | shared with Premiere/AME (audio conform, indexes) |
  | Clean Database & Cache (清理数据库与缓存) | button | — | — | — | removes entries whose sources are gone |
  | Write XMP IDs to Files on Import (导入时向文件写入 XMP ID) | bool | off (UNVERIFIED) | — | — | modifies source files |
  | Create Layer Markers from Footage XMP Metadata (基于素材 XMP 元数据创建图层标记) | bool | on | — | — | project-comp-layers.md [prj-b], layer markers |

  - The disk cache stores a frame only if reading it back is faster than re-rendering; cache entries are keyed by the render state so they survive restarts (persistent disk cache) and undo (global RAM cache).
- **Priority:** P1 (disk cache), P2 (media cache, XMP)
- **Owner:** bae-render (+ bae-media)
- **Notes:** Cache semantics: ui-preview-render-io.md [ui-b] (preview & cache) and compositing-3d-color.md [cmp-a] (caching). BAE decision: disk cache stores GPU-friendly compressed frames (e.g. lossless zstd of f16/u8 planes) keyed by a 128-bit hash of the evaluated layer subtree; never writes into source files (no XMP ID writing).

### AE-UI-671 · Memory & CPU preferences (“内存和 CPU”首选项)

- **What it does:** How much RAM AE may use and the Multi-Frame Rendering controls.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | RAM reserved for other applications (为其他应用程序保留的 RAM) | float | machine-dependent (UNVERIFIED: ≈ 25 % of installed RAM, min 3–6 GB) | ≥ OS minimum | GB | pool shared with Premiere Pro, Photoshop, Audition, AME via the memory balancer |
  | RAM available for (可用于以下用途的 RAM) | read-only | installed − reserved | — | GB | |
  | Show Memory Usage Details (展开内存使用详细信息) | button | — | — | — | Memory Details dialog |
  | GPU Information (GPU 信息) | button | — | — | — | |
  | Enable Multi-Frame Rendering (启用多帧渲染) | bool | on | — | — | AE-UI-645; also applies to AE comps rendered from AME/Premiere |
  | % CPU reserved for other applications (为其他应用程序保留的 CPU 百分比) | int | 10 | 0 … 70 | % | |

  - Edit > Purge (清理) commands: All Memory & Disk Cache, All Memory, All Disk Cache (UNVERIFIED wording), All 3D Cache, Undo, Image Cache Memory, Snapshot; Alt/Option + All Memory also re-synchronises the project database.
- **Priority:** P1
- **Owner:** bae-render (+ apps/bae)
- **Notes:** BAE decision: a single "Memory budget" (RAM cache size) and "Reserved CPU %" setting; no cross-app memory balancer. GPU memory budget is a separate BAE setting (VRAM cache size).

### AE-UI-672 · Video preferences: HDR preview and Mercury Transmit (“视频”首选项)

- **What it does:** HDR display preview and video output to external monitors / I/O cards.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Notes |
  |---|---|---|---|
  | Enable Extended Display Range / High Dynamic Range preview (when available) (启用扩展显示范围/高动态范围预览（如可用）) | bool | off (UNVERIFIED) | EDR on macOS / HDR on Windows; see compositing-3d-color.md [cmp-b], display color |
  | Enable Mercury Transmit (启用 Mercury Transmit) | bool | off | numpad `/` toggles video preview; macOS without numpad: Ctrl+Shift+/ |
  | Disable video output when in the background (在后台时禁用视频输出) | bool | on (UNVERIFIED) | |
  | Video devices list (Adobe Monitor 1/2, AJA, Blackmagic …) + Setup | list | none | per-device options; "Video Monitor Uses Composition Color Space" (UNVERIFIED) |

- **Priority:** P2 (HDR preview P1 in BAE)
- **Owner:** bae-ui-motion (+ bae-color, bae-render)
- **Notes:** BAE decision: fullscreen second-display preview (wgpu window on another monitor) replaces Mercury Transmit in M1; SDI cards via plugin later.

### AE-UI-673 · 3D preferences (3D 首选项)

- **What it does:** 3D camera navigation keys/mouse behaviour and the 3D model cache.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Options |
  |---|---|---|---|
  | 1,2,3 and 4,5,6 key behavior (1、2、3 和 4、5、6 键行为) | enum | Camera Navigation and Transform Gizmos (UNVERIFIED) | Camera Navigation and Transform Gizmos (摄像机导航与变换 Gizmo) / Go To Markers (跳转至标记点); when navigation is active, marker keys move to Shift+number |
  | Option/Alt-drag behavior in 3D Compositions (在 3D 合成中按 Option 键拖动的行为) | enum | Activate Camera Navigation (UNVERIFIED) | Activate Camera Navigation (激活摄像机导航) / Duplicate Layers (复制图层) |
  | Camera manipulation point (摄像机操作点) | enum | Indicator (UNVERIFIED) | None / Indicator / Directional indicator |
  | Mouse scroll wheel behavior (鼠标滚轮行为) | enum | Magnify composition (UNVERIFIED) | Magnify composition / Dolly camera / Reverse dolly camera |
  | Drag direction (拖动方向) | enum | Normal (正常) | Normal / Reverse (反向) |
  | Empty 3D Cache (清空 3D 缓存) | button | — | clears imported 3D model caches |

- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-render)
- **Notes:** 3D camera tools and gizmos: ui-preview-render-io.md [ui-b] (tools); cameras: compositing-3d-color.md [cmp-b].

### AE-UI-674 · Scripting & Expressions preferences (脚本和表达式首选项)

- **What it does:** Script security, expression pick-whip format, error banner and expression-editor settings.
- **Exact semantics:**

  | Parameter (中文) | Type | Default | Range | Affects |
  |---|---|---|---|---|
  | Allow Scripts to Write Files and Access Network (允许脚本写入文件和访问网络) | bool | off | — | scripting File/Socket objects |
  | Enable JavaScript Debugger (启用 JavaScript 调试器) | bool | off | — | ExtendScript debugger on error |
  | Warn User When Executing Files (执行文件时警告用户) | bool | on | — | File > Scripts > Run Script File |
  | Show Scripting Progress Dialog (显示脚本执行进度对话框) | bool | on (UNVERIFIED) | — | |
  | Expression Pick Whip Writes Compact English (以简明英语编写表达式拾取) | bool | on | — | transform-keyframes-expressions.md [ani-b]: on ⇒ `thisComp.layer("A").transform.position`; off ⇒ match names `thisComp.layer("A")("ADBE Transform Group")("ADBE Position")` (UNVERIFIED exact form) |
  | Show Warning Banner When Project Contains Expression Errors (当项目包含表达式错误时显示警告横幅) | bool | on | — | [ani-b], errors |
  | Font Size (字体大小) | int | 12 (UNVERIFIED) | 8 … 24 | pt |
  | Syntax Highlighting / Show Line Numbers / Show Indent Guides / Highlight Matching Braces / Auto Insert Braces / Auto Indent Braces / Folding / Inline Errors / Auto Complete (语法高亮 / 显示行号 / 显示缩进标尺 / 高亮显示匹配的大括号 / 自动插入大括号 / 自动缩进大括号 / 折叠 / 内联错误 / 自动完成) | bool each | on (UNVERIFIED) | — | expression editor (AE 16.0+) |
  | Word Wrap (自动换行) | enum | Off (UNVERIFIED) | Off / Word / Character | |
  | Use Tabs (使用制表符) | bool | off (UNVERIFIED) | — | |
  | Tab Width (制表符宽度) | int | 4 (UNVERIFIED) | 2 … 8 | spaces |
  | Backspace Unindents (退格键取消缩进) | bool | on (UNVERIFIED) | — | |
  | Theme (主题) + Color Customization (颜色自定义) | enum + colors | default dark theme | predefined / Custom | editor colours (text, background, line numbers, keywords, identifiers, comments, numbers, strings, operators, indent guides, good/bad brace, selection) |

- **Priority:** P0 (pick-whip format, error banner), P1 (editor options), P2 (script security flags until BAE scripting exists)
- **Owner:** bae-ui-motion (+ bae-anim, bae-plugin)
- **Notes:** The expression engine choice (JavaScript vs Legacy ExtendScript) is a project setting, not a preference (project-comp-layers.md [prj-a], Project settings; [ani-b]). BAE decision: BAE scripting runs sandboxed; "write files / network" maps to an explicit capability grant per script.
