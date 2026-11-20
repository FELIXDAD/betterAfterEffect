# After Effects inventory: project, compositions and layers (AE-PRJ)

## Scope

This file inventories everything an After Effects user touches before and around animation itself: the project as a container (project items and folders, import, Interpret Footage, proxies, solids, placeholders, Project Settings, undo, Collect Files / Reduce Project) and the composition/layer layer of the document (Composition Settings, work area, composition markers, every layer type, the layer property tree, layer timing, labels, layer switches, parenting, stacking, layer editing operations, precompose, replace source, sequence layers, auto-orient, layer markers and layer styles). It specifies the AE behaviour and stored data that BAE must reproduce; rendering maths, animation maths and panel UI live in the sibling files README.md, project-comp-layers.md (this file), transform-keyframes-expressions.md, compositing-3d-color.md, shapes.md, text.md, effects.md and ui-preview-render-io.md. The item format, ID scheme, priorities and owners are defined in README.md.

Conventions used below: "AE" means the current After Effects release (26.x/27.0, 2026) unless a version is named; time values are given in seconds or frames as AE shows them, and BAE stores every time as integer flicks (705,600,000 per second, ARCHITECTURE 4.4). Scripting names refer to the After Effects Scripting Guide (docsforadobe) object model.

## Project container

### AE-PRJ-001 · Project document and root folder (项目)

- **What it does:** A project is the single document AE has open; it owns a tree of project items (compositions, footage, folders) under a virtual root folder, plus the render queue and the project settings.
- **Exact semantics:**
  - Exactly one project is open per AE instance. Opening or creating another project closes the current one (with a save prompt if it is dirty, see AE-PRJ-009).
  - The root folder is a virtual `FolderItem` (`app.project.rootFolder`) that is not itself shown in the Project panel and has no name the user can edit; items whose `parentFolder` is the root appear at top level.
  - Project-level owned data (each specified elsewhere): the item tree (AE-PRJ-014 onward), Project Settings (see section "Project Settings"), the render queue (see ui-preview-render-io.md, Render Queue), project XMP metadata (AE-PRJ-013), the project-level solids folder name (AE-PRJ-021), used fonts list (read-only, derived), and the saved panel/workspace state.
  - `numItems` counts every item in the project at any depth (folders included), not only top-level items. `items` is a 1-based flat collection over all items.
  - A new empty project has zero items and default Project Settings (or the settings of the template project, AE-PRJ-012).
  - File formats: `.aep` binary (primary), `.aepx` XML (AE-PRJ-005), `.aet` template (AE-PRJ-012).
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Scripting: `app.project`, `Project.rootFolder`, `Project.items`, `Project.numItems`, `Project.activeItem`, `Project.selection`. BAE decision: BAE stores the project as its own `.bae` directory package (ARCHITECTURE 4.6) with a `Project` Prim and `Folder`/`Comp`/`Asset` Prims; the root folder maps to the Project Prim's children. BAE does not read or write `.aep` in M1; AE-project interchange is out of scope for this item. BAE may allow several projects open in separate windows, but each window keeps AE's one-project semantics.

### AE-PRJ-002 · Item IDs and lookup (项目项 ID（译名待核）)

- **What it does:** Every project item carries an integer ID that is unique within the project and stable across save/reopen, so scripts, expressions and references can address items independently of name and position.
- **Exact semantics:**
  - `Item.id`: positive integer, unique among items in the project, persistent in the saved file, never reused while the project is open. AE assigns IDs from a project-wide counter; the numeric values carry no meaning (do not rely on creation order).
  - `Project.itemByID(id)` returns the Item with that ID (AE 13.0 (CC 2014) and later). UNVERIFIED: result for an ID that no item has (null vs exception; the scripting guide does not say).
  - Layers have their own IDs (`Layer.id`, `Project.layerByID(id)`, both added in AE 22.0 (2022)), unique within the project. `layerByID` returns null when no layer has the ID and throws when the argument is not an unsigned integer.
  - Duplicate (AE-PRJ-019) and Import of an `.aep` into a project (AE-PRJ-037) assign new IDs to the new items.
  - Items are also addressable by 1-based index in `Project.items` / `FolderItem.items`; indices change when items are added, removed or sorted, IDs do not.
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** Source: https://ae-scripting.docsforadobe.dev/general/project/ (itemByID, layerByID). BAE decision: BAE uses Prim paths for addressing and additionally keeps a stable opaque item ID per Prim (needed by expressions `comp("name")` fallbacks, scripting-like agent APIs and the CRDT, where names are not unique). IDs must survive move between folders and rename.

### AE-PRJ-003 · New, Open and Close Project (新建项目 / 打开项目 / 关闭项目)

- **What it does:** Creates an empty project, opens a saved project file, or closes the current project.
- **Exact semantics:**
  - **New Project** (File > New > New Project): closes the current project (save prompt if dirty) and creates an untitled project ("Untitled Project.aep" in the title bar) with default Project Settings; if Preferences > Project > New Project loads template is on, the new untitled project is a copy of the chosen template (AE-PRJ-012).
  - **Open Project** (File > Open Project): accepts `.aep`, `.aepx`, `.aet`. Opening an `.aet` creates an untitled copy (AE-PRJ-012). Opening a project saved by an older major version converts it on open and the project opens as an untitled/converted copy so the original is not overwritten (AE asks for a new name on the first save). UNVERIFIED: the converted project title shows "<name> (converted)". A project saved by a newer major version cannot be opened.
  - **Open Recent** (File > Open Recent) lists recently opened projects.
  - On open AE reports, in one dialog each, missing footage, missing effects and missing fonts (see AE-PRJ-072); the project still opens.
  - Preferences > Project > Maintain Current Workspace When Opening Projects: when on, the workspace stored in the project is ignored and the current one kept.
  - **Close Project** (File > Close Project): closes with a save prompt if dirty; scripting `Project.close(CloseOptions.DO_NOT_SAVE_CHANGES | PROMPT_TO_SAVE_CHANGES | SAVE_CHANGES)`.
- **Priority:** P0
- **Owner:** apps/bae (+ bae-model, bae-ui-motion)
- **Notes:** BAE decision: `.bae` packages and `.baez` single files (ARCHITECTURE 16) are the open/save formats; a newer format version than the app supports must refuse to open (or open read-only) rather than silently drop data, matching AE's newer-version refusal.

### AE-PRJ-004 · Save, Save As and Save a Copy (保存 / 另存为 / 保存副本)

- **What it does:** Writes the project to disk under its current name, under a new name that becomes the open project, or as a copy that does not change the open project.
- **Exact semantics:**
  | Command (中文) | Effect on open project | Effect on disk |
  |---|---|---|
  | Save (保存) | clears dirty flag; untitled project behaves as Save As | overwrites the current file |
  | Save As > Save As (另存为) | open project takes the new name/location | new file written; original file unchanged (keeps its last saved state) |
  | Save As > Save a Copy (保存副本) | open project keeps its name, location and dirty state | copy written; the copy is not opened |
  | Save As > Save a Copy As XML (将副本另存为 XML) | unchanged | `.aepx` copy, see AE-PRJ-005 |
  | Save As > Save a Copy As <previous major version> (将副本另存为 <以前的主版本号>) | unchanged | down-saved copy, see AE-PRJ-006 |
  - Saving an `.aepx` as `.aep` (or vice versa) via Save As converts the format.
  - File paths of footage are stored both absolute and relative to the project file (used by relinking, AE-PRJ-073).
  - Save is undo-neutral: it does not create or clear undo history.
  - Scripting: `Project.save([file])` (same as Save / Save As when a file is passed), `Project.saveWithDialog()` returns false if the user cancels.
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae, bae-ui-motion)
- **Notes:** Shortcuts: Save Ctrl/Cmd+S; Save As Ctrl+Shift+S / Cmd+Shift+S (see ui-preview-render-io.md, shortcuts). BAE decision: because BAE is CRDT-based with per-client update logs (ARCHITECTURE 4.6), "Save" compacts the Loro snapshots of the edited sublayers; the dirty flag means "changes not yet compacted/flushed to the package". Save a Copy must write a self-consistent package without touching the open package's update logs.

### AE-PRJ-005 · Save a Copy As XML (将副本另存为 XML)

- **What it does:** Writes a copy of the project as a text-based `.aepx` XML file meant for automated workflows.
- **Exact semantics:**
  - The XML file stores much of the project as hex-encoded binary blobs; human-readable/editable strings include marker attributes (comments, chapter and cue-point parameters), file paths of footage and proxy sources, composition/footage/layer/folder names and comments.
  - Footage item names appear as strings only when customised (otherwise derived from the file name); solid names are not exposed. Workspace and view names appear but edits to them are ignored on open.
  - An `.aepx` opens exactly like an `.aep`.
- **Priority:** P2 — interchange/automation only.
- **Owner:** bae-model (+ apps/bae)
- **Notes:** BAE decision: the BAE counterpart is `bae export-json` (ARCHITECTURE 4.6), which exports any sublayer as readable JSON; BAE does not need an XML format. AE's XML is not a stable or documented schema.

### AE-PRJ-006 · Save a Copy As previous major version (将副本另存为 <以前的主版本号>)

- **What it does:** Writes a copy in the file format of the previous major AE version so it can be opened there.
- **Exact semantics:** Only the immediately previous major version is offered (e.g. AE 27.0 offers "Save a Copy As 26.x"). Features that the older version does not support are dropped from the copy (AE does not warn per feature). The open project is unchanged.
- **Priority:** P2
- **Owner:** bae-model
- **Notes:** UNVERIFIED: exact menu label in AE 27.0 ("Save a Copy As 26.x"). BAE decision: BAE format versions are declared in `project.json`; a down-save/export to an older BAE format version, if ever offered, must list dropped features in a report instead of dropping silently.

### AE-PRJ-007 · Increment and Save (递增和保存)

- **What it does:** Saves the project under a new automatically numbered name in the same folder.
- **Exact semantics:**
  - The new file is written in the same folder as the current project file. Its name is the current base name followed by a number; if the base name already ends in a number, that number is increased by 1 (e.g. `shot_v07.aep` -> `shot_v08.aep`; digit count is preserved when possible).
  - UNVERIFIED: when the base name has no trailing number AE appends " 2" (with a space), e.g. `shot.aep` -> `shot 2.aep`.
  - UNVERIFIED: after the command the open project is the new incremented file (title bar shows the new name) and the previous file keeps its last saved state; Adobe's help text calls the result a "copy".
  - If the target name already exists, AE keeps incrementing until it finds an unused name. UNVERIFIED.
  - Disabled for an untitled (never saved) project, which uses Save As instead. UNVERIFIED.
- **Priority:** P1 — widely used versioning habit.
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Shortcut Ctrl+Alt+Shift+S / Cmd+Opt+Shift+S. BAE decision: BAE has built-in version history (ARCHITECTURE 7); Increment and Save should still exist because users rely on visible versioned files for hand-off.

### AE-PRJ-008 · Revert (还原（译名待核）)

- **What it does:** Discards all changes since the last save and reloads the saved file.
- **Exact semantics:** File > Revert asks for confirmation, then reloads the project from disk; the undo history is cleared. Disabled when the project is untitled or not dirty. Image caches are discarded for items whose source changed.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-model)
- **Notes:** UNVERIFIED: whether Revert is enabled for a non-dirty project. BAE decision: in collaborative mode Revert must only revert the local author's unsaved ops (ARCHITECTURE 4.5), never remote collaborators' changes; in single-user mode it reloads the last saved snapshot.

### AE-PRJ-009 · Dirty state and revision counter (项目已修改状态（译名待核）)

- **What it does:** Tracks whether the project has unsaved changes and counts edits.
- **Exact semantics:**
  - `dirty` is true after any undoable change (and some non-undoable ones, such as Interpret Footage changes or render-queue edits) since the last save; dirty projects show `*` after the name in the window title.
  - `revision`: integer, starts at 1 for a new project, increases on every user action; read-only.
  - Closing, quitting, opening another project or New Project prompts Save / Don't Save / Cancel when dirty.
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Scripting `Project.dirty` (undocumented, AE 17.5 / CC 2020), `Project.revision`. UNVERIFIED: which non-undoable actions set dirty. Implementation hint: derive dirty from "op log length since last flush" per sublayer; revision = monotonic op counter (useful for cache keys).

### AE-PRJ-010 · Auto-Save (自动保存)

- **What it does:** Periodically writes backup copies of the open project (saved or untitled) to an Auto-Save folder, keeping a limited number of versions.
- **Exact semantics:** Preferences > Auto-Save (首选项 > 自动保存):
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Save Every (保存间隔) | bool + int | on, 20 | UNVERIFIED: 1-? | minutes | interval counted from the last auto-save, not the last manual save |
  | Save when starting render queue (启动渲染队列时保存) | bool | UNVERIFIED: on | — | — | saves when render queue items are added/modified/started |
  | Ask to recover an auto-saved project if After Effects closes unexpectedly (当 After Effects 意外关闭时，询问是否恢复自动保存的项目) | bool | UNVERIFIED: on | — | — | marked beta; see AE-PRJ-011 |
  | Maximum Project Versions (最大项目版本数) | int | 5 | UNVERIFIED: 1-99 | count | per project; oldest auto-save deleted when exceeded |
  | Auto-Save Location (自动保存位置) | enum | Next to Project | Next to Project (项目旁边（译名待核）) / Custom Location (自定义位置) | — | Custom takes a folder |
  - Location "Next to Project": a folder named "Adobe After Effects Auto-Save" (UNVERIFIED exact name) is created in the project's folder; backup files are named `<project name> auto-save <n>.aep` with n cycling up to Maximum Project Versions (UNVERIFIED naming).
  - Untitled projects are also auto-saved ("improved Auto-Save" of AE 27.0, Nov 2026, first in the AE Beta from Sep 2026; older versions only auto-saved saved projects) to the custom or default auto-save location.
  - Auto-save never overwrites the user's project file and does not clear the dirty flag. Auto-save applies to local projects only.
  - Projects written by the newest auto-save feature do not open in earlier AE versions.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-model)
- **Notes:** UNVERIFIED: the defaults marked above (20 min and 5 versions are the long-standing AE defaults). "Improved Auto-Save" with crash recovery and untitled-project auto-save is listed in What's New for AE 27.0 (Nov 2026), after an AE Beta preview (build 27.0.044, 2026-09-15). Source: https://helpx.adobe.com/after-effects/desktop/work-with-projects/after-effects-projects/auto-save-and-crash-recovery.html BAE decision: BAE's incremental update logs (ARCHITECTURE 4.6) are append-only and already crash-safe; auto-save in BAE means periodic snapshot compaction plus a bounded set of rollback snapshots in `cache/` or the custom location, with the same user-facing settings.

### AE-PRJ-011 · Crash recovery (崩溃恢复)

- **What it does:** After an unexpected quit, offers to reopen the most recent auto-save.
- **Exact semantics:** If the preference in AE-PRJ-010 is on, the next launch offers to open the most recent auto-save. Any auto-save, however opened, opens as an untitled "Recovered Project" (恢复的项目); Save then shows the Save As dialog so the original project is not overwritten unless the user chooses it.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-model)
- **Notes:** Introduced with the improved Auto-Save in AE 27.0 (Nov 2026); the preference is labelled "(Currently in beta)" in Preferences. BAE decision: BAE can recover to the last op in the update log rather than the last auto-save; it should still open the recovered state as a distinct, clearly-labelled document until the user confirms.

### AE-PRJ-012 · Project templates (.aet) (模板项目)

- **What it does:** A template project is a project file that, when opened, yields a new untitled project with the template's contents and settings.
- **Exact semantics:**
  - `.aet` = same content as `.aep`; renaming `.aep` to `.aet` (or Save a Copy then rename) makes a template. Opening an `.aet` creates an untitled project; saving never writes back into the template.
  - Preferences > Project > New Project loads template (新建项目加载模板, default off) + Choose Project Template (选择项目模板): New Project then opens a copy of the chosen `.aet`, `.aep` or `.aepx` file. Used to preset color management, folder structure, render settings etc.
  - AE no longer installs sample template projects.
- **Priority:** P2
- **Owner:** apps/bae (+ bae-model)
- **Notes:** BAE decision: a template is a read-only `.bae`/`.baez` copied into a new untitled project; item IDs may be kept (no collisions in a fresh project).

### AE-PRJ-013 · Project metadata (XMP) (项目元数据（译名待核）)

- **What it does:** Stores XMP metadata (title, label, creator, etc.) on the project, editable in the Metadata panel and by scripts; passed to rendered files when the output module option is on.
- **Exact semantics:** `Project.xmpPacket`: RDF/XML string, read/write. Contents are free-form XMP; AE adds its own fields (e.g. creation/modify dates). Footage XMP is read on import and can be shown as Project panel columns (AE-PRJ-017). Output modules can include project XMP in rendered files (see ui-preview-render-io.md, Render Queue).
- **Priority:** P2
- **Owner:** bae-model
- **Notes:** BAE decision: store as an opaque key/value metadata map on the Project Prim; XMP serialisation only at export time.

## Project items and folders

### AE-PRJ-014 · Project item common attributes (项目项)

- **What it does:** Every entry in the Project panel (composition, footage, folder) is an item with a name, comment, label, ID, parent folder and selection state.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | name (名称) | string | type-dependent ("Comp 1", file name, "Black Solid 1", "Untitled 1" folder) | non-empty in UI | — | not required to be unique; for file footage the display name is independent of the file name |
  | comment (注释) | string | "" | ≤ 15,999 bytes after encoding conversion | — | user-only, no effect on rendering; shown in the Comment column |
  | label (标签) | int | per-type default from Preferences > Labels > Label Defaults | 0 = None, 1-16 = label colours | — | see project-comp-layers.md, Labels |
  | id | int | assigned | positive | — | AE-PRJ-002 |
  | parentFolder | FolderItem | root folder or the selected folder at creation | — | — | AE-PRJ-018 |
  | selected | bool | false | — | — | multiple selection allowed |
  | typeName | string | — | "Composition" / "Footage" / "Folder" (localised: 合成 / 素材 / 文件夹) | — | read-only, derived |
  | guides | array | [] | — | — | per-item viewer guides; see ui-preview-render-io.md, guides |
  - Rename: select item, Enter/Return or context menu Rename (重命名); undoable.
  - Item order inside a folder is a Project-panel sort only (by any column, toggled ascending/descending) and has no effect on rendering.
  - `Item.remove()` deletes the item (AE-PRJ-020).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `Item.name/comment/label/id/parentFolder/selected/typeName`, `Item.dynamicLinkGUID` (string GUID used by Dynamic Link; BAE may ignore). The Comment column heading can be renamed per user ("Rename This"), which is a UI preference, not item data. BAE decision: names are stored on the Prim as a display attribute separate from the Prim path segment so duplicate names are allowed like AE.

### AE-PRJ-015 · Audio/visual item attributes (AVItem) (音视频项目属性（译名待核）)

- **What it does:** Compositions and footage items share a set of size/time attributes that layers and expressions read.
- **Exact semantics:**
  | Parameter (中文) | Type | Range | Notes |
  |---|---|---|---|
  | width / height (宽度 / 高度) | int | 1..30000 px | comp: read/write; footage: from source, writable only for solids |
  | pixelAspect (像素长宽比) | float | 0.01..100.0 | UI shows 2 decimals; stored exactly (e.g. UI 0.91 = 0.909091, 1.09 = 1.09402, 1.21 = 1.21212, 1.33 = 1.33333, 1.46 = 1.45869) |
  | duration (持续时间) | float s | 0..10800 | still footage: 0; comp: read/write; footage: from main source after interpretation (conform, pulldown, loop) |
  | frameRate (帧速率) | float fps | 1..999 (UI maximum per Adobe help; the scripting guide still documents 1..99) | footage: = `mainSource.displayFrameRate`; change via conform frame rate |
  | frameDuration | float s | 1/999..1 | exact reciprocal of frameRate |
  | hasVideo / hasAudio | bool | — | read-only |
  | footageMissing | bool | — | true for placeholders and missing files |
  | usedIn | CompItem[] | — | every comp that has a layer (or another reference, e.g. track matte source) using this item; snapshot, not live |
  | useProxy / proxySource | bool / FootageSource | — | AE-PRJ-074 |
  | time | float s | — | current time when previewed from the Project panel/Footage viewer |
  - A still footage item's duration is 0; layers made from it get the still-footage default duration (AE-PRJ-043) or the comp duration.
  - `usedIn` is the basis of in-use warnings (AE-PRJ-020), Reveal in Composition (AE-PRJ-022), Remove Unused Footage and Reduce Project.
- **Priority:** P0
- **Owner:** bae-model (+ bae-media)
- **Notes:** Scripting `AVItem.*`; `isMediaReplacementCompatible` (AE 18.0) relates to Essential Graphics media replacement (see ui-preview-render-io.md, Essential Graphics). BAE decision: frame rates are stored as exact rationals (e.g. 30000/1001) and durations as flicks; the AE float view is derived. Expressions see these via `comp`/`footage` objects (see transform-keyframes-expressions.md, expression language).

### AE-PRJ-016 · Footage item and footage sources (素材项目)

- **What it does:** A footage item wraps one main source (file, solid or placeholder), an optional proxy source, and the interpretation settings that turn decoded media into comp-ready frames.
- **Exact semantics:**
  - Source kinds (`FootageItem.mainSource`):
    | Kind | Scripting | Typical name | isStill | Notes |
    |---|---|---|---|---|
    | File (文件) | `FileSource` (`file`, `missingFootagePath`, `reload()`) | file name, or sequence pattern `name_[0000-0099].png` | true for single images | movie, audio, image, image sequence, vector/layered stills, data files |
    | Solid (纯色) | `SolidSource` (`color` [r,g,b] 0..1) | "<Color> Solid <n>" | true | AE-PRJ-079 |
    | Placeholder (占位符) | `PlaceholderSource` | user name | duration 0 => still | AE-PRJ-081 |
  - Shared interpretation fields on every source (`FootageSource`): `hasAlpha`, `alphaMode`, `invertAlpha`, `premulColor`, `nativeFrameRate`, `conformFrameRate`, `displayFrameRate`, `fieldSeparationType`, `highQualityFieldSeparation`, `removePulldown`, `loop`, `isStill`; defined in the Interpret Footage section.
  - `FootageItem.file`: the main source file (null for solids/placeholders).
  - Video/audio-only files produce items with `hasVideo`/`hasAudio` set accordingly; an audio-only item has width/height of 0 in the UI. UNVERIFIED: scripting reports width/height 0 for audio-only footage.
  - Footage can be trimmed in the Footage viewer and inserted (Ripple Insert / Overlay Edit), see ui-preview-render-io.md, Footage viewer.
- **Priority:** P0
- **Owner:** bae-model (+ bae-media)
- **Notes:** Scripting `FootageItem.mainSource`, `replace()`, `replaceWithPlaceholder()`, `replaceWithSequence()`, `replaceWithSolid()`, `openInViewer()`. BAE decision: maps to the `Asset` Prim (ARCHITECTURE 4.2) with `source` (file reference per ARCHITECTURE 16 / solid colour / placeholder spec), `interpretation`, and `proxy` sub-records. Data files (JSON/CSV/TSV/MGJSON) are footage items without video (see transform-keyframes-expressions.md, data footage). Lottie/bodymovin: project items have no Lottie object of their own; footage referenced by exported layers becomes an entry of the root `assets` array (`id` string, referenced by the layer's `refId`): image asset `{id, w, h, u, p, e}` (`e: 1` = embedded, `p` is then a base64 data URL and `u` is empty; `e: 0` = external file `u` + `p`; optional `t: "seq"` marks one frame of an image sequence), sound asset (same file fields), precomposition asset `{id, layers, fr?}` for comps, data source asset `t: 3`; solids produce no asset (solid layer `ty: 1` carries `sc`, `sw`, `sh`), folders, comments and labels are not exported. Source: https://lottiefiles.github.io/lottie-docs/assets/

### AE-PRJ-017 · Project panel column data (项目面板列数据（译名待核）)

- **What it does:** Each item exposes read-only derived values shown as sortable Project panel columns and in the information header above the item list.
- **Exact semantics:**
  | Column (中文) | Value | Applies to |
  |---|---|---|
  | Name (名称) | item name | all |
  | Label (标签) | label colour index | all |
  | Type (类型) | "Composition", "Folder", "Solid", "Placeholder", or the file format/codec family (e.g. "QuickTime", "PNG Sequence", "Photoshop") | all |
  | Size (大小) | file size on disk (bytes, shown in KB/MB/GB); sequences: total | file footage |
  | Media Duration (媒体持续时间（译名待核）) | duration in the project time display style; Δ notation | comps, time-based footage |
  | File Path (文件路径（译名待核）) | absolute path of main source | file footage |
  | Date (日期（译名待核）) | file modification date | file footage |
  | Comment (注释) | item comment | all |
  | Frame Rate (帧速率) | displayFrameRate | comps, time-based footage |
  | In Point / Out Point (入点 / 出点) | UNVERIFIED: source in/out from embedded timecode | time-based footage |
  | Tape Name, Video Info, Audio Info, Status | UNVERIFIED: present in the Columns menu in recent versions | footage |
  - Information header (thumbnail area) for a selected footage item: name and usage count ("used N times"), `W x H (PAR)`, `Δ duration, fps`, colour depth descriptor ("Millions of Colors" = 8-bit RGB, "Millions of Colors+" = 8-bit RGBA, "Trillions of Colors" = 16-bit, "Floating Point" = 32-bit float) with alpha interpretation in parentheses (e.g. "(Straight)", "(Premultiplied)"), codec name, audio "48.000 kHz / 32 bit U / Stereo", and the embedded colour profile when present. For a comp: `W x H (PAR)`, `Δ duration, fps`. UNVERIFIED: exact header strings.
  - Project metadata (XMP) fields can be added as extra columns (Columns > Project Metadata).
- **Priority:** P1
- **Owner:** bae-model (+ bae-media, bae-ui-motion)
- **Notes:** Panel behaviour (show/hide/reorder/resize columns, sort, search/filter, thumbnail) is specified in ui-preview-render-io.md, Project panel. Alt/Option-click on a footage item shows the file creator ID (macOS type/creator). BAE decision: column values are computed from Asset metadata cached at import, not by re-probing files on every repaint.

### AE-PRJ-018 · Folders (文件夹)

- **What it does:** Folders organise items in the Project panel; they can be nested to any depth and have no effect on rendering.
- **Exact semantics:**
  - Create: File > New > New Folder (新建文件夹) or the Create a New Folder button; the new folder is created inside the currently selected folder (or at the root), named "Untitled 1", "Untitled 2", ... and put into rename mode. UNVERIFIED: default name pattern.
  - Move: drag items/folders into a folder; drag to the information area at the top of the panel to move to the root. A folder cannot be moved into itself or its descendants. Moving is undoable. Scripting: set `Item.parentFolder`.
  - Import with a folder selected places imported items in that folder; Import Folder creates a folder (AE-PRJ-026).
  - `FolderItem.items` / `numItems` list only direct children; `FolderItem.item(i)` is 1-based.
  - Deleting, duplicating and collecting folders: see Delete items and folders, Duplicate items, and Collect Files (only comps, not folders, count as "selected comps").
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `ItemCollection.addFolder(name)`, `FolderItem`. Team Projects did not allow nested folders (removed feature). BAE decision: `Folder` Prims; folder membership is a pure organisational attribute so moving never invalidates render caches.

### AE-PRJ-019 · Duplicate and copy/paste items (重复 / 复制)

- **What it does:** Creates copies of selected project items in the same project.
- **Exact semantics:**
  - Edit > Duplicate (重复, Ctrl/Cmd+D) with Project panel focus duplicates each selected item into the same folder, placed right after the original and selected.
  - Duplicating a composition is shallow: the new comp contains copies of the layers (with all properties, keyframes, expressions, masks, effects, markers), but nested comps and footage are referenced, not duplicated. Comp-level data (settings, markers, work area, guides) is copied.
  - Name of a duplicate: a trailing number in the name is incremented to the next unused value (e.g. "Comp 1" -> "Comp 2"); if there is no trailing number, " 2" is appended. UNVERIFIED: footage duplicates may keep the same name instead (footage items commonly show the same file name twice).
  - Duplicating a footage item copies its main source reference, interpretation and proxy settings; the file is not copied.
  - Duplicating a folder duplicates the folder and (recursively) its contents. UNVERIFIED.
  - Edit > Copy then Paste with Project panel focus behaves like Duplicate into the selected folder. UNVERIFIED.
  - Expressions in a duplicated comp that refer to the comp by name are not renamed.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-model)
- **Notes:** Duplicating a footage item to the desktop by dragging copies the file in the OS (out of scope for BAE M1). BAE decision: "Duplicate comp deep" (duplicate nested comps too) is a common third-party script; consider offering it as a separate command.

### AE-PRJ-020 · Delete items and folders (删除项目（译名待核）)

- **What it does:** Removes items from the project, removing any layers that use them.
- **Exact semantics:**
  - Select items and press Delete/Backspace (or Edit > Clear). If any selected footage/comp is used in compositions (`usedIn` non-empty), AE shows a warning stating that the item(s) are used in compositions and asks to confirm; confirming removes every layer that uses the item from every comp. UNVERIFIED: exact dialog text and whether the count of uses is shown.
  - Deleting a folder deletes all of its contents recursively; a non-empty folder triggers a confirmation. UNVERIFIED: whether the confirmation also appears when the contents are unused.
  - Deleting a comp removes all precomp layers referencing it.
  - The operation is a single undo step that restores the items, their IDs and all removed layers in their original stacking positions.
  - Render queue items whose comp is deleted are removed from the render queue.
  - Deleting a footage item used as a proxy elsewhere is not possible (proxies are sources, not items).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `Item.remove()` (no warning). Expressions that referenced deleted layers/comps break and show errors (see transform-keyframes-expressions.md, expression errors). Bulk variants: AE-PRJ-096 (Remove Unused Footage), AE-PRJ-097 (Consolidate All Footage), AE-PRJ-095 (Reduce Project).

### AE-PRJ-021 · Solids folder (纯色文件夹)

- **What it does:** A designated folder into which every new solid footage item (from solid, null and adjustment layers) is placed automatically.
- **Exact semantics:**
  - The project has at most one designated solids folder. It is created on demand, at the root, the first time a solid is created; its default name is "Solids" (纯色), taken from Preferences > Project > New Project Solids Folder (新建项目纯色文件夹) at project creation.
  - Since AE 23.x ("enhanced solids folder organisation"): the solids folder can be renamed; on the next solid creation AE asks whether to keep using the renamed folder (Yes) or create a new folder with the default name (No). It can be nested in other folders and keeps receiving new solids. Any folder can be made the solids folder via its context menu (Solids Folder (纯色文件夹)); only one at a time.
  - If the designated folder was deleted, a new one is created at the root on the next solid creation.
  - New null objects and adjustment layers create solid footage items too and go to the same folder.
- **Priority:** P1
- **Owner:** bae-model (+ bae-motion)
- **Notes:** UNVERIFIED: AE version of enhanced solids folder (believed AE 23.0 (2022)). The preference shows the current project's solids-folder name for reference; changing it affects new projects only. BAE decision: BAE null and adjustment layers need no footage source (they are layer types); BAE should still create a solid Asset for Solid layers so that "Affect all layers that use this solid" (AE-PRJ-080) works, but need not create assets for nulls/adjustments unless exported to AE.

### AE-PRJ-022 · Reveal usage (在合成中显示 / 在项目中显示图层源)

- **What it does:** Navigates between an item and the layers that use it.
- **Exact semantics:**
  - Project panel context menu > Reveal in Composition (在合成中显示): submenu lists each use as "<comp name>, <layer name>" (from `usedIn` plus layer scan); choosing one opens that comp, selects the layer and moves the CTI into the layer's span if needed. UNVERIFIED: CTI behaviour.
  - Timeline layer context menu > Reveal Layer Source in Project (在项目中显示图层源): selects the layer's source item in the Project panel, expanding folders.
  - Reveal Composition in Project for a comp's Timeline (UNVERIFIED menu name).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Requires a reverse index source item -> layers; BAE should keep it incrementally (also used by in-use warnings and Reduce Project).

### AE-PRJ-023 · Project Flowchart (项目流程图)

- **What it does:** A read-only node graph of comps, footage and layers with arrows showing which item feeds which comp.
- **Exact semantics:** Project flowchart (Ctrl/Cmd+F11) or Composition Flowchart (Composition > Composition Flowchart) shows tiles for comps, footage items and (when expanded) layers; arrows show nesting/use. Mid-gray lines indicate the Video/Audio switch of that layer is off. Selecting a tile selects the item/layer in the Project/Timeline panel; Delete deletes the selected item or layer; relationships cannot be edited in the flowchart.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** Panel UI belongs with ui-preview-render-io.md panels; listed here because it is a view of project-item relationships. BAE decision: a dependency graph view is cheap to provide from the same reverse index as AE-PRJ-022.

## Import

### AE-PRJ-024 · Import File dialog (导入文件)

- **What it does:** File > Import > File (Ctrl/Cmd+I) opens a file browser that creates footage items (or compositions) from one or more selected files, referencing them in place.
- **Exact semantics:**
  - Also opened by double-clicking an empty area of the Project panel, or via a folder's context menu > Import (imports into that folder). Otherwise new items go into the selected folder (or root).
  - The file content is never copied into the project; the item stores a reference (absolute + relative path) to the file.
  - Dialog options (shown depending on the selected file):
    | Parameter (中文) | Type | Default | Notes |
    |---|---|---|---|
    | Import As (导入为) | enum | Footage for flat files; for layered files the last used value (UNVERIFIED) | Footage (素材) / Composition (合成) / Composition - Retain Layer Sizes (合成 - 保持图层大小); AE-PRJ-029 |
    | <format> Sequence (序列) | bool | sticky: AE remembers the last state | AE-PRJ-038 |
    | Force alphabetical order (强制按字母顺序排列) | bool | off | AE-PRJ-039 |
    | Multiple Sequences (多个序列) | bool | off | imports each selected file's sequence as its own item |
    | Create Composition (创建合成（译名待核）) | bool | off | UNVERIFIED: present in recent versions; creates a comp from the imported item(s) |
    | Files of type / Enable (文件类型) | enum | All Acceptable Files | "All Footage Files" excludes project files |
  - The bottom of the dialog shows what will be imported (e.g. the detected sequence range).
  - Windows only: selecting a folder and pressing Import Folder imports the folder (AE-PRJ-026).
  - Selecting several files imports each as a separate item (unless they form a sequence and Sequence is on).
  - If a file has an alpha channel without an alpha label and the Interpret Unlabeled Alpha As preference (AE-PRJ-042) is Ask User, the Interpret Footage dialog opens for that file during import.
  - Layered PSD/AI/PDF/EPS files imported as Footage open the Layer Options dialog (AE-PRJ-030).
  - The undo step "Import" covers the whole batch.
- **Priority:** P0
- **Owner:** bae-media (+ bae-model, bae-ui-motion)
- **Notes:** Scripting `Project.importFile(ImportOptions)`, `ImportOptions.importAs` (`ImportAsType.FOOTAGE | COMP | COMP_CROPPED_LAYERS | PROJECT`), `ImportOptions.sequence`, `forceAlphabetical`, `rangeStart`/`rangeEnd` (sub-range), `canImportAs()`; `Project.importFileWithDialog()`; `Project.setDefaultImportFolder(folder)`. Supported formats: see ui-preview-render-io.md, import/export format lists. BAE decision: the importer probes files asynchronously (metadata first, decode on demand) so a large batch import never blocks the UI.

### AE-PRJ-025 · Import Multiple Files and Import Recent Footage (导入多个文件 / 导入最近的素材（译名待核）)

- **What it does:** Import Multiple Files keeps the import dialog open for repeated selections; Recent Footage re-imports a recently imported file.
- **Exact semantics:**
  - File > Import > Multiple Files (Ctrl+Alt+I / Cmd+Opt+I): after each Open, the dialog reappears until Done/Cancel; all imports land in the selected folder.
  - File > Import Recent Footage: lists recently imported files (most recent first); choosing one imports it again as a new item with default interpretation. UNVERIFIED: list length 10.
- **Priority:** P2
- **Owner:** bae-ui-motion (+ bae-media)
- **Notes:** —

### AE-PRJ-026 · Import folders (导入文件夹)

- **What it does:** Imports the contents of a disk folder either as one image sequence or as individual items inside a new project folder.
- **Exact semantics:**
  - Dragging a disk folder from Finder/Explorer into the Project panel imports its contents as a still-image sequence (single footage item) when the files form a sequence.
  - Alt/Option-dragging a folder imports every file as an individual item inside a new project folder named after the disk folder, preserving sub-folder structure. UNVERIFIED: sub-folder recursion.
  - Windows Import dialog "Import Folder" button: same as Alt-drag. UNVERIFIED.
  - Files AE cannot import are skipped; AE reports them. UNVERIFIED: whether skipped files are reported.
- **Priority:** P1
- **Owner:** bae-media (+ bae-model)
- **Notes:** —

### AE-PRJ-027 · Drag-and-drop import (拖动导入)

- **What it does:** Files dragged from the OS into AE are imported; dropping into a comp also creates layers.
- **Exact semantics:**
  - Drop on the Project panel: import into the folder under the pointer (or root).
  - Drop on a Composition viewer or Timeline: import, then add each imported item as a layer in the active comp. Viewer drop: layer centred where it would be for Add to Composition (UNVERIFIED: drop position is ignored and layer is centred). Timeline drop: inserted at the drop row in the layer stack; layer In point at the CTI when dropped on the layer area (UNVERIFIED).
  - Layered files dropped in are imported per the Drag Import Multiple Items As preference (AE-PRJ-028) without showing the Layer Options dialog.
  - Dragging a Render Queue output module into the Project panel imports its output file, or a placeholder if not yet rendered (AE-PRJ-083).
  - Dragging items from the Project panel into a comp is layer creation, not import (see project-comp-layers.md, layer creation).
- **Priority:** P0
- **Owner:** bae-ui-motion (+ bae-media)
- **Notes:** —

### AE-PRJ-028 · Drag Import Multiple Items As preference (拖拽导入多个项目为)

- **What it does:** Chooses how layered files are imported when dragged in (no dialog is shown).
- **Exact semantics:** Preferences > Import > Drag Import Multiple Items As:
  | Value (中文) | Meaning |
  |---|---|
  | Footage (素材) | merged single footage item |
  | Composition (合成) | comp with document-size layers |
  | Composition - Retain Layer Sizes (合成 - 保留图层尺寸) | comp with cropped layers |
  Default: UNVERIFIED: Composition - Retain Layer Sizes. The current help text says it controls what AE creates "when you drag multiple files into a composition"; older AE manuals describe the same menu as deciding whether layered still-image files dragged into AE come in as compositions.
- **Priority:** P1
- **Owner:** bae-media (+ apps/bae)
- **Notes:** The task brief's "Default Drag Import As" is this preference; AE has used the label "Drag Import Multiple Items As" since at least the CS-era manuals (the name was not changed in the AE 26 preferences refresh). Older versions listed only Composition and Composition - Retain Layer Sizes; current help also lists Footage. Source: https://helpx.adobe.com/after-effects/desktop/get-started/preferences-and-settings/preferences.html

### AE-PRJ-029 · Import As Composition for layered files (导入为合成)

- **What it does:** Imports a layered PSD, AI, PDF/EPS (or SVG, AE-PRJ-036) file as a composition whose layers reference per-layer footage items, either all at document size or cropped to each layer's bounds.
- **Exact semantics:**
  - Results: one comp named after the file (without extension) and a folder "<file name> Layers" containing one footage item per source layer (name "<layer name>/<file name>"). UNVERIFIED: folder and item naming patterns.
  - **Composition**: every layer footage item has the document size; layer anchor points are at the document centre; positions are the document centre.
  - **Composition - Retain Layer Sizes**: every layer footage item is cropped to that layer's non-transparent bounds; the layer's anchor point is at the centre of its bounds and its Position puts it at its original location. Layers that extend beyond the canvas keep their full extent. UNVERIFIED: whether pixels outside the canvas are kept for PSD.
  - Comp settings: width/height = document size (rounded up to integers for vector files); PAR from the document (PSD pixel aspect metadata, else 1); frame rate and duration from the last-used comp settings; duration of every layer = comp duration. UNVERIFIED: frame rate/duration source.
  - Layer order, names, visibility (hidden source layers import with Video switch off), opacity, blend modes and other mappings: AE-PRJ-032 (PSD), AE-PRJ-034 (AI/PDF/EPS).
  - Image sequences of layered files imported as a composition: each source layer becomes its own sequence footage item.
- **Priority:** P1
- **Owner:** bae-media (+ bae-model, bae-motion)
- **Notes:** Scripting `ImportAsType.COMP` / `COMP_CROPPED_LAYERS`. BAE decision: per-layer footage items reference the same file plus a layer selector (`layerName`/`layerIndex`) rather than extracting files.

### AE-PRJ-030 · Layer Options for layered footage import (图层选项)

- **What it does:** When a layered file is imported as Footage, chooses whether to merge all layers or take one layer, and at what dimensions.
- **Exact semantics:** Dialog shown after Import As Footage for PSD/AI/PDF/EPS:
  | Parameter (中文) | Type | Default | Values / Notes |
  |---|---|---|---|
  | Layer Options (图层选项) | enum | Merged Layers | Merged Layers (合并的图层) / Choose Layer (选择图层) + layer popup |
  | Layer Styles (图层样式) | enum | Merge Layer Styles into Footage | only with Choose Layer for PSD: Merge Layer Styles into Footage (合并图层样式到素材) / Ignore Layer Styles (忽略图层样式（译名待核）) |
  | Footage Dimensions (素材尺寸) | enum | Document Size | only with Choose Layer: Document Size (文档大小) / Layer Size (图层大小) |
  - Merged PSD: transparency and layer masks of all layers are merged into one alpha that is premultiplied with white (AE sets the interpretation accordingly). Single PSD layer: layer mask combined with transparency, interpreted as straight alpha.
  - A merged footage item can later be converted to a composition: File > Replace Footage > With Layered Comp (all uses) or Layer > Convert to Layered Comp (one layer) (AE-PRJ-033).
- **Priority:** P1
- **Owner:** bae-media (+ bae-ui-motion)
- **Notes:** UNVERIFIED: the exact Layer Styles values for single-layer import; the defaults. Scripting has no layer selection in `ImportOptions` (merged only).

### AE-PRJ-031 · Editable Layer Styles vs Merge Layer Styles into Footage (可编辑的图层样式 / 合并图层样式到素材)

- **What it does:** When importing a PSD as a composition, chooses whether Photoshop layer styles become live AE layer styles or are baked into each layer's pixels.
- **Exact semantics:**
  - Shown in the composition import dialog (Layer Options) for PSD files that contain layer styles. Default: Editable Layer Styles. UNVERIFIED default.
  - Editable Layer Styles (可编辑的图层样式): supported styles (Drop Shadow, Inner Shadow, Outer Glow, Inner Glow, Bevel and Emboss, Satin, Color Overlay, Gradient Overlay, Pattern Overlay, Stroke, Blending Options) are created as AE Layer Styles with the same values (see project-comp-layers.md, Layer styles). A layer with layer styles breaks 3D intersections and shadow casting (Classic 3D). UNVERIFIED: Pattern Overlay is not editable in AE.
  - Merge Layer Styles into Footage (合并图层样式到素材): styles rendered by the Photoshop engine into the layer footage; faster, may not match Photoshop exactly; does not interfere with 3D intersections/shadows.
- **Priority:** P1
- **Owner:** bae-media (+ bae-render)
- **Notes:** BAE decision: BAE has no Photoshop engine; "Merge" must be implemented by rendering BAE's own layer-style implementation into a cached image, or by reading the PSD's composite per-layer data if present.

### AE-PRJ-032 · Photoshop (PSD) composition import mapping (Photoshop 文件导入)

- **What it does:** Defines how each Photoshop document feature maps onto AE comps, layers and properties when importing as a composition.
- **Exact semantics:**
  | Photoshop feature | AE result |
  |---|---|
  | pixel layer | footage layer from a per-layer footage item |
  | layer position | layer Position (document- or bounds-centred per Import As) |
  | opacity | layer Opacity (%) |
  | fill opacity | UNVERIFIED: baked into the layer pixels / Blending Options in editable styles |
  | blend mode | same-named AE blend mode (see compositing-3d-color.md, blend modes); unsupported modes fall back to Normal |
  | visibility off | layer imported with Video switch off |
  | layer mask (raster) | merged into the per-layer footage alpha (straight) |
  | vector mask / clipping path | AE mask(s) on the layer (Add mode), editable and animatable |
  | layer group | nested composition (precomp) with the group's name; group opacity/blend mode on the precomp layer |
  | clipping group (clipping mask) | precomp of the clipped layers (comp-size) with Preserve Underlying Transparency on each clipped layer |
  | adjustment layer (Levels, Curves, Hue/Saturation, Brightness/Contrast, Color Balance, Invert, Threshold, Posterize, Channel Mixer, Photo Filter, Black & White, Vibrance, Selective Color, Exposure, Gradient Map...) | AE adjustment layer with the corresponding AE effect(s) where one exists (e.g. Levels, Curves, Hue/Saturation); unsupported adjustments are imported as an empty adjustment layer. UNVERIFIED: exact supported list |
  | fill layers (Solid Color / Gradient / Pattern) | rasterised footage layers (UNVERIFIED) |
  | text layer | footage layer of rasterised text; Layer > Create > Convert to Editable Text makes a text layer (AE-PRJ-035) |
  | Smart Object | single merged footage layer |
  | layer styles | AE-PRJ-031 |
  | image guides | composition guides |
  | video layer / 3D layer | not supported |
  | CMYK, LAB, Duotone, Monotone, Tritone layered file | imported as one flattened image |
  - Layer names stay linked: renaming a layer in Photoshop keeps the link on Reload; deleting a layer makes that layer item Missing; layers added after import are not picked up on Reload.
  - Only RGB and Grayscale layered PSDs keep layers.
- **Priority:** P1
- **Owner:** bae-media (+ bae-motion, bae-effects)
- **Notes:** Layer > New > Adobe Photoshop File creates an empty PSD at comp size, imports it and opens Photoshop (P2, BAE: not applicable). BAE decision: implement a PSD reader (layer records, masks, vector masks, groups, adjustment descriptors) in bae-media; adjustment-layer mapping depends on effects.md coverage. Large Document Format (PSB) UNVERIFIED supported.

### AE-PRJ-033 · Convert merged footage to layered comp (转换为分层合成)

- **What it does:** Replaces a merged layered-file footage item (or one layer's use of it) with an equivalent layered composition.
- **Exact semantics:** File > Replace Footage > With Layered Comp (图层合成): creates the layered comp (as Import As Composition) and replaces every use of the footage item with the new comp; Layer > Convert to Layered Comp: same, for the selected layer only. The layer keeps its transforms, effects and timing; the new precomp layer's source is the new comp. UNVERIFIED: which Import As variant (document size vs retain sizes) is used.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-media)
- **Notes:** —

### AE-PRJ-034 · Illustrator, PDF and EPS import (Illustrator / PDF / EPS 导入)

- **What it does:** Imports vector documents, rasterised by AE at the comp-required scale, as footage or as layered compositions.
- **Exact semantics:**
  - AI (AI, AI4, AI5, EPS, PS) and PDF are continuously rasterisable vector footage; without Continuously Rasterize (see compositing-3d-color.md, collapse/continuous rasterize) the layer is rasterised once at 100% of the document size and then transformed as pixels.
  - Empty areas become transparent (alpha generated). AE does not read embedded colour profiles from AI files; assign one in Interpret Footage > Color tab (AE-PRJ-057).
  - Fractional document sizes are rounded up to whole pixels (e.g. 216.5 x 275.5 -> 217 x 276; Adobe's help example prints "217x278", apparently a typo), which can leave a transparent/black row at the right/bottom edge.
  - Crop box: AI documents use the artboard (crop area) as the frame; when an AI document has several artboards, the first artboard defines the comp frame and art outside it is retained outside the comp frame only when imported as a layered composition. UNVERIFIED: bounding-box vs artboard rules for single-layer and merged imports.
  - PDF: only the first page is imported. A page-selection option is not offered (UNVERIFIED for AE 27).
  - AI blending modes are read only from files saved as Illustrator CS2 or earlier.
  - Interpret Footage > More Options for AI/PDF/EPS: Antialiasing More Accurate / Faster (AE-PRJ-056).
  - Layered import maps AI top-level layers to AE layers (sublayers are merged into their top-level layer).
  - Since AE 26.3 (June 2026) imported AI layers can be converted to shape layers preserving gradient fills, strokes and transparency (see shapes.md, Create Shapes from Vector Layer).
- **Priority:** P1
- **Owner:** bae-media (+ bae-vector)
- **Notes:** BAE decision: BAE can import PDF/AI (PDF-compatible) via a PDF parser into Vello paths and rasterise at any scale (true continuous rasterisation); EPS/PS (PostScript) import is P2 and may be dropped. Implementation hint: cache rasterisations per scale bucket.

### AE-PRJ-035 · Convert PSD and Illustrator text to editable text (转换为可编辑文字)

- **What it does:** Turns a text layer imported from a PSD (or, since AE 27.0, an Illustrator file) into a native AE text layer.
- **Exact semantics:**
  - PSD: select the imported footage layer whose source is a Photoshop text layer and choose Layer > Create > Convert to Editable Text. A new text layer replaces it, with font, size, tracking, leading, colour, paragraph settings, transform and point/paragraph type from the PSD; the original footage layer is turned off (Video switch off) and kept below. UNVERIFIED: whether the original layer is kept.
  - AI (AE 27.0, Nov 2026): Illustrator text converts to editable text layers preserving fonts, size, fills, positions, styles and variable-font settings.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-text, bae-media)
- **Notes:** Text layer model: see text.md.

### AE-PRJ-036 · SVG import (SVG 导入)

- **What it does:** Imports SVG files as rasterised footage or as a composition of editable shape layers.
- **Exact semantics:** Since AE 26.3 (June 2026): Import As Footage gives a vector footage item (rasterised like AI); Import As Composition creates a comp with native shape layers (paths, fills, strokes, gradients). Pasting SVG text/Illustrator clipboard content into a comp creates shape layers (Edit > Paste). UNVERIFIED: mapping of SVG groups/transforms/clip paths/masks and unsupported features.
- **Priority:** P1 — common hand-off format for motion designers; cheap with Vello/usvg.
- **Owner:** bae-media (+ bae-vector)
- **Notes:** BAE decision: use a usvg-class parser to produce BAE shape Prims (see shapes.md for the target model).

### AE-PRJ-037 · Import an After Effects project into a project (导入 After Effects 项目（译名待核）)

- **What it does:** Imports all items of another project file into the open project.
- **Exact semantics:** File > Import > File with an `.aep`/`.aepx`/`.aet` selected (Import As: Project): all items of that project are added inside a new folder named after the imported project file (e.g. "other.aep"), keeping its folder structure; every imported item gets a new ID; render queue items of the imported project are added to the render queue. Fonts/effects missing are reported as on open. The solids folder of the imported project is not merged with the current one. UNVERIFIED: render-queue and solids-folder behaviour.
- **Priority:** P1
- **Owner:** bae-model (+ apps/bae)
- **Notes:** Scripting `ImportAsType.PROJECT`. Import of Premiere Pro projects (`.prproj`: sequences become comps, clips become layers), Pro Import (AAF/OMF/FCP XML) and Dynamic Link footage are P2 and out of BAE M1. BAE decision: importing one `.bae` into another is a sublayer/Prim copy with ID remap; referencing instead of copying is a BAE-only feature (ARCHITECTURE 4.1 Reference).

## Image sequences and still-image defaults

### AE-PRJ-038 · Image sequence import and numbering (静止图像序列)

- **What it does:** Imports a numbered series of still images in one folder as a single footage item, one file per frame.
- **Exact semantics:**
  - Enabled by the "<format> Sequence" checkbox (e.g. "PNG Sequence", "OpenEXR Sequence") in the Import dialog. The checkbox state is sticky across imports. Selecting any one file of a sequence imports the whole sequence; dragging a disk folder imports its sequence.
  - Detection (AE-compatible rule set; partly UNVERIFIED):
    ```
    name = stem + ext
    find the LAST run of decimal digits in stem -> prefix, digits, suffix
      (AE scripting isFileNameNumbered() finds a digit run; examples: "image0120.png" -> 120,
       "ima0102ge.png" -> 102, "003image.png" -> 3)
    members = files in the same folder with the same prefix, suffix and extension (case-insensitive on
              macOS/Windows) whose middle part is all digits
    frame order = ascending integer value of the digits
    first frame = lowest number found (or rangeStart); last = highest (or rangeEnd)
    frame count = last - first + 1   (gaps become missing frames, AE-PRJ-041)
    ```
    UNVERIFIED: AE's exact choice when a name has several digit runs (e.g. "shot01_v02_0001.png": AE uses the last run); whether different zero-padding widths (frame9 / frame10) are treated as one sequence (believed yes).
  - Item name: `<prefix>[<first>-<last>]<suffix>.<ext>` with the original zero padding, e.g. `render_[0000-0249].png`.
  - Interpretation (alpha, PAR, colour profile, bit depth, size) is taken from the first image of the sequence; all frames are assumed to have the same size. UNVERIFIED: frames of a different size are scaled/centred or cropped.
  - Frame rate = Preferences > Import > Sequence Footage > Frames Per Second (AE-PRJ-044) unless overridden in Interpret Footage. Duration = frame count / frame rate.
  - Uncheck Sequence to import one numbered file as a single still.
  - Multiple Sequences (多个序列) checkbox: select files from different sequences in the same folder to import each as a separate sequence item.
  - Rendering a comp with a numbered sequence output uses the comp start frame number as the first output number (see ui-preview-render-io.md, Render Queue).
- **Priority:** P0
- **Owner:** bae-media (+ bae-model)
- **Notes:** Scripting `ImportOptions.sequence`, `FootageItem.replaceWithSequence(file, forceAlphabetical)`, `AVItem.setProxyWithSequence()`. BAE decision: store the sequence as `{dir, prefix, suffix, ext, padding, first, last, frame_list?}` so relinking and Collect Files can enumerate frames without listing directories; scanning must be lazy for network storage (AE also does not stat each frame by default).

### AE-PRJ-039 · Force alphabetical order (强制按字母顺序排列)

- **What it does:** Imports a set of files as a sequence ordered by file name, ignoring numeric gaps.
- **Exact semantics:** With Force alphabetical order on, every matching file becomes one consecutive frame in alphabetical (name) order; gaps in numbering do not create missing frames (e.g. Seq1, Seq2, Seq3, Seq5 -> 4 frames). It also allows sequences whose names are not numbered. Incompatible with a sub-range (scripting throws if `rangeStart`/`rangeEnd` are set together with `forceAlphabetical`). UNVERIFIED: sort is plain code-point order (so "Seq10" sorts before "Seq2"), not natural order.
- **Priority:** P1
- **Owner:** bae-media
- **Notes:** Scripting `ImportOptions.forceAlphabetical`; `replaceWithSequence(file, forceAlphabetical)`.

### AE-PRJ-040 · Import a sequence sub-range (导入序列的子集)

- **What it does:** Imports only part of a numbered sequence.
- **Exact semantics:** In the Import dialog select the first wanted file, Shift-click the last wanted file, keep Sequence on, Open: the item covers only that number range. Scripting (undocumented) `ImportOptions.rangeStart`/`rangeEnd` (integers; frame numbers): ignored when `sequence` is false; throws when `forceAlphabetical` is true; a `rangeEnd` beyond the last existing file creates missing frames; `rangeStart > rangeEnd` throws and resets to the full range.
- **Priority:** P1
- **Owner:** bae-media
- **Notes:** —

### AE-PRJ-041 · Missing frames in sequences (缺失帧)

- **What it does:** Handles gaps and unreadable files inside an imported image sequence.
- **Exact semantics:**
  - A number missing from the range is a missing frame; it is rendered as the missing-footage placeholder image (colour bars, AE-PRJ-082) in that frame only, with the item still usable.
  - If Preferences > Import > Report Missing Frames (报告缺失帧) is on, AE warns at import (and at open/reload) listing the count and the first missing frame. UNVERIFIED: exact message content.
  - By default AE does not stat every frame at import (fast import from network storage), so unresolved aliases/shortcuts or offline files inside the range are not reported until rendered; Verify Individual Files (验证个体文件, slower) checks each file.
  - Frames that exist but fail to decode render as UNVERIFIED: missing-frame colour bars / an error message in the Info panel.
- **Priority:** P1
- **Owner:** bae-media (+ bae-render)
- **Notes:** BAE decision: missing frames must never abort a render; the render log lists them (AE does the same in the render log only when reporting is on).

### AE-PRJ-042 · Interpret Unlabeled Alpha As preference (将未标记的 Alpha 通道解释为)

- **What it does:** Decides the alpha interpretation of imported files that have an alpha channel but no metadata saying straight or premultiplied.
- **Exact semantics:** Preferences > Import > Import Defaults:
  | Value (中文) | Behaviour at import |
  |---|---|
  | Ask User (询问用户) | Interpret Footage dialog appears for each such file |
  | Guess (猜测) | runs the Guess algorithm (AE-PRJ-048); if uncertain AE prompts or uses its best guess |
  | Ignore Alpha (忽略 Alpha 通道) | alphaMode = Ignore |
  | Straight (Unmatted) (直接（无蒙版）) | alphaMode = Straight |
  | Premultiplied (Matted with Black) (预乘（以黑色蒙版）) | alphaMode = Premultiplied, premulColor = [0,0,0] |
  | Premultiplied (Matted with White) (预乘（以白色蒙版）) | alphaMode = Premultiplied, premulColor = [1,1,1] |
  Default: Ask User. UNVERIFIED default.
  - Files whose format labels the alpha (e.g. PNG = straight, QuickTime with an alpha-type flag, PSD single layer = straight, PSD merged = premultiplied with white, EXR = premultiplied) never consult this preference. UNVERIFIED: per-format labelling list.
  - Interpretation rules (AE-PRJ-060) are applied after this.
- **Priority:** P1
- **Owner:** bae-media (+ apps/bae)
- **Notes:** UNVERIFIED: whether the preference applies to every unlabeled format or only to formats that can be either (TGA, TIFF, some QuickTime codecs). The alpha maths is in AE-PRJ-047.

### AE-PRJ-043 · Still footage default duration (静止素材)

- **What it does:** Sets the duration given to layers created from still images (and the length of comps created from layered stills).
- **Exact semantics:** Preferences > Import > Still Footage:
  | Value (中文) | Meaning | Default |
  |---|---|---|
  | Use Composition Length (使用合成时长) | the layer spans the comp duration from its In point | UNVERIFIED: selected |
  | Use Custom Duration (使用自定义持续时间) + duration field | fixed duration in the project time display style | UNVERIFIED: 0:00:10:00 |
  - The footage item itself always has duration 0 (`isStill`); only layers (and layered-still import comps) get this duration.
  - Still layers can be extended indefinitely by trimming; they have no source time (time remap on a still is a no-op).
  - Placeholders with duration 0 and solids are still footage too and follow the same rule. UNVERIFIED for solids: solid layers always take the comp length.
- **Priority:** P0
- **Owner:** bae-model (+ bae-motion)
- **Notes:** Older UI names: "Length of Composition" / custom duration ("合成的长度"); UNVERIFIED.

### AE-PRJ-044 · Sequence footage import preferences (序列素材)

- **What it does:** Sets the default frame rate and validation for imported image sequences.
- **Exact semantics:** Preferences > Import > Sequence Footage:
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Frames Per Second (每秒帧数) | float | 30 | UNVERIFIED: 1-999 | fps | becomes the item's native rate; changing it later affects only new imports |
  | Report Missing Frames (报告缺失帧) | bool | UNVERIFIED: on | — | — | AE-PRJ-041 |
  | Verify Individual Files (验证个体文件) | bool | off | — | — | slower; checks every frame file |
  - Formats that store a rate in each file (DPX/Cineon headers, EXR metadata) still use this preference unless the frame rate is read from the file; UNVERIFIED: AE ignores per-file rate metadata for DPX.
- **Priority:** P0
- **Owner:** bae-media (+ apps/bae)
- **Notes:** —

### AE-PRJ-045 · Indeterminate Media NTSC timecode preference (非确定性媒体 NTSC)

- **What it does:** Chooses drop-frame or non-drop-frame timecode for NTSC-rate media whose timecode type is unknown (e.g. image sequences).
- **Exact semantics:** Preferences > Import > Import Defaults > Indeterminate Media NTSC: Use Dropframe (使用丢帧时间码) / Use Non-Dropframe (使用非丢帧时间码). Default: UNVERIFIED: Dropframe. Applies only to footage at 29.97/59.94 (and 23.976 is always non-drop). Affects timecode display of footage start time and source timecode, not timing.
- **Priority:** P2
- **Owner:** bae-model (+ bae-media)
- **Notes:** Timecode display maths: see project-comp-layers.md, Composition Settings / time display, and AE-PRJ-086.

## Interpret Footage

### AE-PRJ-046 · Interpret Footage dialog and interpretation model (解释素材)

- **What it does:** Per-footage-item settings that tell AE how to read the source (alpha type, frame rate, start timecode, fields and pulldown, pixel aspect ratio, looping, format-specific options, input colour space); they change how decoded frames enter compositions, never the file.
- **Exact semantics:**
  - Open: File > Interpret Footage > Main (主要, Ctrl+Alt+G / Cmd+Opt+G), the Interpret Footage button at the bottom of the Project panel, or dragging an item onto that button. File > Interpret Footage > Proxy (代理) (or Alt/Option-click the button) edits the proxy source's interpretation (AE-PRJ-074).
  - Tabs: Main Options (主要选项) and Color (颜色) (named Color Management (色彩管理) in older versions). Sections in Main Options, in UI order: Alpha (AE-PRJ-047), Frame Rate (AE-PRJ-049), Start Timecode (AE-PRJ-050), Fields and Pulldown (AE-PRJ-051, AE-PRJ-052), Other Options: Pixel Aspect Ratio (AE-PRJ-053) and Loop (AE-PRJ-055), More Options button (AE-PRJ-056). A Preview checkbox applies changes live.
  - Sections that do not apply are disabled (e.g. Frame Rate, Fields and Loop for stills; Alpha when the source has no alpha).
  - Changing interpretation invalidates cached frames of the item and of every comp using it; it is undoable and marks the project dirty.
  - Decode pipeline for one comp-requested frame (render semantics beyond this point: compositing-3d-color.md):
    ```
    t_layer_source  (seconds into the footage item, from layer timing / time remap; see transform-keyframes-expressions.md)
    t_item = t_layer_source mod D_base         if loop > 1   (AE-PRJ-055)
    frame  = source frame for t_item at displayFrameRate (AE-PRJ-049), after pulldown removal (AE-PRJ-052)
    field  = field selection if fields are separated (AE-PRJ-051)
    pixels = decode(frame) -> alpha interpretation (AE-PRJ-047) -> input colour transform (AE-PRJ-057)
    pixels are then treated as having pixelAspect (AE-PRJ-053) when placed in the comp
    ```
- **Priority:** P0
- **Owner:** bae-media (+ bae-model, bae-ui-motion)
- **Notes:** Scripting: all settings live on `FootageItem.mainSource` / `proxySource` (`FootageSource` attributes). BAE decision: interpretation is a plain data record on the Asset Prim (`alpha_mode`, `premul_color`, `invert_alpha`, `conform_rate`, `start_tc_override`, `field_order`, `preserve_edges`, `pulldown`, `par`, `loop`, `color`), applied by bae-media when it produces frames; decoded-frame cache keys must include it.

### AE-PRJ-047 · Alpha interpretation (Alpha 通道解释)

- **What it does:** Declares whether the source alpha is ignored, straight or premultiplied (with which matte colour), and whether it is inverted.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Values | Notes |
  |---|---|---|---|---|
  | Alpha mode | enum | from file label, else the Interpret Unlabeled Alpha As preference (AE-PRJ-042) | Ignore (忽略) `AlphaMode.IGNORE`; Straight - Unmatted (直接 - 无遮罩) `AlphaMode.STRAIGHT`; Premultiplied - Matted With Color (预乘 - 有彩色遮罩) `AlphaMode.PREMULTIPLIED` | Guess button: AE-PRJ-048 |
  | Matte color (遮罩颜色（译名待核）) | RGB 0..1 | black [0,0,0] | any (eyedropper / colour picker) | only for Premultiplied; scripting `premulColor` |
  | Invert Alpha (反转 Alpha) | bool | off | — | ignored when alpha mode is Ignore or there is no alpha |
  - Conversion to AE's internal representation (AE layers carry straight colour plus alpha; see compositing-3d-color.md, alpha model):
    ```
    a = stored_alpha (normalised 0..1); if invert_alpha: a = 1 - a
    Ignore:        a_out = 1;           rgb_out = rgb                        (premultiplied fringes stay visible)
    Straight:      a_out = a;           rgb_out = rgb
    Premultiplied: a_out = a;
                   if a > 0: rgb_out = (rgb - (1 - a) * C) / a              (C = matte colour, per channel)
                   else:     rgb_out = 0   (UNVERIFIED: AE may use C or leave rgb for a == 0)
    8/16 bpc: rgb_out clamped to [0, 1]; 32 bpc: UNVERIFIED (believed unclamped, so over-range values survive)
    ```
  - Interpreting premultiplied footage as straight produces a dark (black-matte) or light (white-matte) halo on soft edges; the Remove Color Matting effect can fix it per layer (see effects.md, Remove Color Matting).
  - Only the first alpha channel of files with several alpha/extra channels is used.
  - Footage without alpha: alpha controls disabled, a_out = 1.
- **Priority:** P0
- **Owner:** bae-media (+ bae-render)
- **Notes:** UNVERIFIED: order of Invert Alpha relative to unmatting (believed: invert first). Scripting `FootageSource.alphaMode`, `premulColor`, `invertAlpha`, `hasAlpha`. Implementation hint: do the unmatte in the decode-upload compute shader together with colour conversion; BAE's compositor works premultiplied internally (compositing-3d-color.md) so straight -> premultiplied is a multiply, and premultiplied-with-black input can skip the divide entirely.

### AE-PRJ-048 · Guess alpha (猜测)

- **What it does:** Estimates whether a footage alpha is straight or premultiplied, and the matte colour, from the pixels.
- **Exact semantics:** Interpret Footage > Alpha > Guess (and the Guess import preference) sets alpha mode, matte colour and invert flag to AE's best estimate; if AE cannot guess confidently it beeps and leaves the setting (UI) / uses a default. Scripting `FootageSource.guessAlphaMode()` (no-op when `hasAlpha` is false). The algorithm is not documented. UNVERIFIED heuristic consistent with observed behaviour:
  ```
  for semi-transparent pixels (0 < a < 1):
    if every channel c <= a (+ tolerance): premultiplied with black is consistent
    if every channel c >= 1 - a (- tolerance): premultiplied with white is consistent
    otherwise: straight
  choose the hypothesis consistent with (almost) all sampled pixels; if none clearly wins: not confident
  ```
- **Priority:** P2
- **Owner:** bae-media
- **Notes:** BAE decision: implement the heuristic on the first decoded frame (downsampled); report confidence to the UI.

### AE-PRJ-049 · Frame rate interpretation and conform (帧速率)

- **What it does:** Chooses the frame rate at which the footage's frames are laid out in time: the file's own rate or a conformed (assumed) rate.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units | Notes |
  |---|---|---|---|---|---|
  | Use frame rate from file (使用文件中的帧速率（译名待核）) | radio | selected for movies | — | — | shows the native rate, e.g. "(29.970 fps)" |
  | Conform to frame rate (匹配帧速率) | radio + float | native rate | UNVERIFIED: 0.001..999 | fps | scripting `conformFrameRate` (0 = use native); documented UI max 999 fps (scripting docs still say 99) |
  | Assume this frame rate (假定此帧速率) | float | Sequence Footage preference (30) | as above | fps | shown instead of the radios for image sequences |
  - Time mapping (no frame blending; frame blending is a layer switch, see transform-keyframes-expressions.md):
    ```
    rate = conformFrameRate if != 0 else nativeFrameRate      ( * 0.8 when pulldown removed, AE-PRJ-052 )
    N    = number of frames in the file (after pulldown removal)
    D    = N / rate                                            (footage duration, seconds)
    frame_index(t) = clamp(floor(t * rate + eps), 0, N - 1)    for 0 <= t < D
    ```
    UNVERIFIED: AE's exact epsilon/rounding for t landing exactly on a frame boundary (BAE: compute in flicks with exact rational rates so no epsilon is needed).
  - Conforming changes playback speed and item duration (15 -> 30 fps plays twice as fast, half as long). Audio of the same item is NOT conformed: it keeps its original speed and duration, so A/V sync changes (Adobe help). Keyframes on layers using the item stay at their comp times. UNVERIFIED: the item duration is then the video duration and audio beyond it is cut.
  - Layers already using the item keep their In point and keyframe times; their Out point is re-derived from the new duration if the layer was not trimmed. UNVERIFIED.
  - Variable-frame-rate sources are presented as constant-rate (AE-PRJ-063).
- **Priority:** P0
- **Owner:** bae-media (+ bae-anim)
- **Notes:** Scripting `nativeFrameRate`, `conformFrameRate`, `displayFrameRate`. A layer Time Stretch (e.g. 100.1% for 30 -> 29.97) is the alternative that also stretches audio (see transform-keyframes-expressions.md, time stretch). BAE decision: store rates as rationals (30000/1001, not 29.97).

### AE-PRJ-050 · Start timecode (起始时间码（译名待核）)

- **What it does:** Sets the footage's media start time (source timecode of its first frame), used for time display, not for rendering.
- **Exact semantics:**
  - Options: Use source file timecode (default when the file carries timecode, e.g. QuickTime tmcd track, MXF, DPX header) / Override start timecode with a user value (in the project time display style). UNVERIFIED: section added in AE 22.x-23.x; label names.
  - Without embedded timecode the media start is 0 (image sequences may use the first frame number when the time display is Frames; see AE-PRJ-086 Footage Start Time).
  - The media start offsets the timecode shown in the Footage viewer, the Project panel In/Out columns and the layer "source time" displays; drop-frame vs non-drop for NTSC rates follows the file, or the Indeterminate Media NTSC preference (AE-PRJ-045).
  - It does not shift where the footage starts in a comp (layer In point is unaffected).
- **Priority:** P1
- **Owner:** bae-media (+ bae-model)
- **Notes:** Expressions: `sourceTime()` and footage time displays (see transform-keyframes-expressions.md). BAE decision: store `media_start` as flicks + drop-frame flag on the Asset.

### AE-PRJ-051 · Separate fields (分离场)

- **What it does:** Treats each interlaced frame as two fields displaced in time and reconstructs a full-height image per field.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Values | Notes |
  |---|---|---|---|---|
  | Separate Fields (分离场) | enum | from file field-order label; DV and D1 footage automatically separated; else Off | Off (关) `FieldSeparationType.OFF` / Upper Field First (高场优先) `UPPER_FIELD_FIRST` / Lower Field First (低场优先) `LOWER_FIELD_FIRST` | upper field = odd lines 1,3,5... (top line is line 1) |
  | Preserve Edges (Best Quality Only) (保留边缘（仅最佳品质）) | bool | off | — | scripting `highQualityFieldSeparation`; better reconstruction in static areas at Best quality |
  - Semantics: the footage keeps its frame rate; each source frame k provides field 1 at time k/rate and field 2 at (k + 0.5)/rate. A field image is made full-height by interpolating the missing lines (line doubling/interpolation; with Preserve Edges, non-moving areas take lines from the other field). Comps at twice the frame rate, or renders with Field Render on, show every field; a comp at the footage rate samples the first field. UNVERIFIED: exact interpolation kernel.
  - Field order labels in files are honoured automatically; the dialog overrides them.
- **Priority:** P2 — interlaced sources are legacy for a 2026 motion-graphics tool.
- **Owner:** bae-media
- **Notes:** Output field rendering: see ui-preview-render-io.md, Render Settings. Scripting `FootageSource.fieldSeparationType`, `guessPulldown()` also sets field order.

### AE-PRJ-052 · Remove 3:2 / 24Pa pulldown (移除 Pulldown)

- **What it does:** Reconstructs the original 23.976/24 fps progressive frames from telecined 29.97/30 fps interlaced video.
- **Exact semantics:**
  - Requires Separate Fields not Off (setting pulldown with fields Off is an error).
  - Remove (移除) menu, phases named by the pattern of Whole (W) and Split (S) frames in the first five video frames: Off; WSSWW; SSWWW; SWWWS; WWWSS; WWSSW (3:2); and the same five phases for 24Pa (24P Advance, Panasonic DVX100). Scripting `PulldownPhase.OFF`, `WSSWW` ... `WWSSW`, `WSSWW_24P_ADVANCE` ... `WWSSW_24P_ADVANCE`.
  - Buttons Guess 3:2 Pulldown (猜测 3:2 Pulldown) and Guess 24Pa Pulldown (猜测 24Pa Pulldown) analyse the footage and set field order + phase (scripting `guessPulldown(PulldownMethod.PULLDOWN_3_2 | ADVANCE_24P)`).
  - Effect: 4 progressive frames are rebuilt from each 5 video frames; `displayFrameRate = conformFrameRate * 0.8` (29.97 -> 23.976, 30 -> 24); if conformFrameRate was 0 it is set to the native rate when pulldown is enabled.
  - Mixed-phase footage must be imported several times with different phases and trimmed.
- **Priority:** P2
- **Owner:** bae-media
- **Notes:** —

### AE-PRJ-053 · Pixel aspect ratio presets (像素长宽比)

- **What it does:** Declares the shape of the footage's pixels; AE scales the footage horizontally by PAR_footage / PAR_comp when placing it in a comp, and corrects the viewer display.
- **Exact semantics:** Pixel Aspect Ratio menu (same list in Interpret Footage, Composition Settings and Solid Settings), in menu order:
  | Menu entry (中文) | UI value | Exact ratio | Typical frame |
  |---|---|---|---|
  | Square Pixels (方形像素) | 1.0 | 1/1 | any square-pixel source |
  | D1/DV NTSC (0.91) | 0.91 | 10/11 = 0.909091 | 720x486, 720x480, 4:3 |
  | D1/DV NTSC Widescreen (1.21) (D1/DV NTSC 宽银幕) | 1.21 | 40/33 = 1.212121 | 720x486, 720x480, 16:9 |
  | D1/DV PAL (1.09) | 1.09 | 128/117 = 1.094017 | 720x576, 4:3 |
  | D1/DV PAL Widescreen (1.46) (D1/DV PAL 宽银幕) | 1.46 | 512/351 = 1.458689 | 720x576, 16:9 |
  | Anamorphic 2:1 (2) (变形 2:1) | 2.0 | 2/1 | 2:1 anamorphic lens / film transfer |
  | HDV 1080/DVCPRO HD 720 (1.33) | 1.33 | 4/3 = 1.333333 | 1440x1080, 960x720, 16:9 |
  | DVCPRO HD 1080 (1.5) | 1.5 | 3/2 | 1280x1080, 16:9 |
  - The exact ratios above match the values reported by scripting (`pixelAspect` returns 0.909091, 1.21212, 1.09402, 1.45869, 1.33333 for the UI values 0.91, 1.21, 1.09, 1.46, 1.33). Since AE CS4 these are the clean-aperture-correct values; projects from AE CS3 or earlier (0.9, 1.2, 1.07, 1.42) are upgraded on open.
  - UNVERIFIED: menu order of Anamorphic 2:1 relative to the HD entries; whether a Custom entry exists for footage (Composition Settings has none; scripting accepts any 0.01..100).
  - Comp placement: a layer from footage with PAR_f in a comp with PAR_c is displayed with horizontal scale factor PAR_f / PAR_c applied before the layer transform (layer Scale still shows 100%). See compositing-3d-color.md, nested comp/footage sampling.
- **Priority:** P0
- **Owner:** bae-model (+ bae-media, bae-render)
- **Notes:** Square-pixel equivalents of SD comp presets: NTSC D1 720x534, NTSC D1 Widescreen 872x486, PAL D1/DV 788x576, PAL D1/DV Widescreen 1050x576 (CS4+). Viewer PAR correction: see ui-preview-render-io.md, viewer options. Chinese names for the D1/DV entries are the English strings in zh-CN (UNVERIFIED).

### AE-PRJ-054 · Pixel aspect ratio detection on import (像素长宽比自动识别（译名待核）)

- **What it does:** Sets an imported item's PAR from file metadata, or guesses it from the frame size.
- **Exact semantics:**
  - Explicit metadata wins: QuickTime (pasp atom), AVI, PSD (pixel aspect) and other formats that store PAR are tagged automatically (e.g. 16:9 DV -> D1/DV NTSC Widescreen).
  - Without metadata: 720x486 or 720x480 -> D1/DV NTSC (0.91); 720x576 -> D1/DV PAL (1.09); everything else -> Square. UNVERIFIED: whether 1440x1080 without metadata is guessed as 1.33 (believed no).
  - The interpretation rules file can add size-based rules (AE-PRJ-060).
- **Priority:** P1
- **Owner:** bae-media
- **Notes:** A square-pixel image that happens to be 720x480 is mis-tagged and must be reset to Square by the user.

### AE-PRJ-055 · Loop footage (循环)

- **What it does:** Repeats a time-based footage item N times end-to-end, making its duration N times longer.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Loop (循环) ... Times (次) | int | 1 | 1..9999 | count |
  ```
  D_base = duration after frame-rate/pulldown interpretation
  item.duration = loop * D_base
  t_item = t mod D_base          for 0 <= t < loop * D_base
  ```
  - Not available for stills (error in scripting). Audio loops together with video. UNVERIFIED.
  - Layers already using the item are not automatically extended; the user trims them out. UNVERIFIED.
- **Priority:** P1
- **Owner:** bae-media (+ bae-model)
- **Notes:** Scripting `FootageSource.loop`. Expression `loopOut()` is a different, keyframe-level feature (transform-keyframes-expressions.md). Implementation hint: never duplicate decoded frames; map time only.

### AE-PRJ-056 · More Options (format-specific source settings) (更多选项)

- **What it does:** Opens format-specific interpretation dialogs for certain footage types.
- **Exact semantics:**
  | Footage type | Dialog | Key options (defaults) |
  |---|---|---|
  | Illustrator / PDF / EPS | EPS/AI options | Antialiasing: More Accurate / Faster (UNVERIFIED default More Accurate) |
  | Camera Raw (incl. CinemaDNG subset) | Adobe Camera Raw dialog | full ACR develop settings; settings of the first image apply to a raw sequence |
  | Cineon / DPX | Cineon Settings (Cineon 设置) | Conversion type preset (Standard / Over Range / Custom...), 10 Bit Black Point 95, Converted Black Point 0, 10 Bit White Point 685, Converted White Point 1023 (10-bit) / 255 (8-bit), Gamma 1.7, Highlight Rolloff 20, Logarithmic Conversion / Units. UNVERIFIED: all values and names; AE recommends instead colour-managing DPX with a log profile |
  | RED R3D | R3D Source Settings | RED SDK decode/colour settings (interpreted as 32 bpc Rec.709) |
  | ARRIRAW, Sony RAW, Canon RAW, ProRes RAW | source settings | vendor raw settings |
  | OpenEXR | (none; channels via EXtractoR / Cryptomatte effects) | — |
- **Priority:** P2
- **Owner:** bae-media
- **Notes:** BAE decision: M1 needs only the AI/PDF antialias option; camera raw and cinema raw formats are out of M1 scope. DPX log handling should go through colour management (bae-color) rather than the legacy Cineon dialog.

### AE-PRJ-057 · Footage colour interpretation (Color tab) (颜色 / 色彩管理)

- **What it does:** Declares the colour space of the footage so AE can convert it into the project working space, or exempts it from conversion.
- **Exact semantics:** Interpret Footage > Color tab. Options differ by Project Settings colour engine (AE-PRJ-087):
  | Parameter (中文) | Type | Default | Values / Notes |
  |---|---|---|---|
  | Media Color Space / Embedded Profile (媒体色彩空间 / 嵌入的配置文件) | read-only | from file | embedded ICC (PSD, TIFF, PNG, JPEG) or video colour metadata (ProRes, H.264/HEVC, XAVC/AVC MXF, ARRI/Sony/RED/ProRes/Canon RAW); "None" if absent |
  | Override Media Color Space (覆盖媒体色彩空间) | enum | off (use media/embedded space) | Adobe engine: any installed ICC profile (former "Assign Profile" (指定配置文件)); OCIO engine: any colour space of the OCIO config (Show all lists all spaces incl. display/view transforms) |
  | Interpret As Linear Light (解释为线性光) | enum | Off | Off / On / On for 32 bpc; treats the assigned profile's tone curve as gamma 1.0; works even when colour management is off |
  | Preserve RGB (保留 RGB) | bool | off | no conversion into the working space: RGB numbers kept, appearance not (use for data such as displacement maps, normal maps) |
  | Description text | read-only | — | explains the resulting conversion |
  - With colour management off (working space None, Adobe engine) no conversion occurs except Interpret As Linear Light. Footage with no embedded/assigned profile and no rule is assumed to be in the working space.
  - Non-RGB sources (CMYK, Y'CbCr, camera raw) cannot be assigned a profile; their native space is shown and converted automatically.
  - OCIO engine: the default media space comes from the OCIO config's default/file rules set in Project Settings.
  - Conversion maths and Expand ITU-R 601 legacy handling: see compositing-3d-color.md, colour management.
- **Priority:** P1
- **Owner:** bae-color (+ bae-media, bae-model)
- **Notes:** UNVERIFIED: AE version in which "Assign Profile" was renamed "Override Media Color Space" (believed AE 24.x/25.0 with the colour-engine rework). Scripting has no direct `FootageSource` colour attributes (UNVERIFIED). BAE decision: store `{embedded, override, linear_light, preserve_rgb}` on the Asset; bae-color resolves it to a transform.

### AE-PRJ-058 · Copy and apply interpretation (记住解释 / 应用解释)

- **What it does:** Copies the interpretation of one footage item to others.
- **Exact semantics:** File > Interpret Footage > Remember Interpretation (记住解释, Ctrl+Alt+C / Cmd+Opt+C) stores the selected item's main-source interpretation; File > Interpret Footage > Apply Interpretation (应用解释, Ctrl+Alt+V / Cmd+Opt+V) applies it to all selected footage items in one undo step. Settings that do not apply to a target (e.g. frame rate for a still, alpha for a source without alpha) are skipped. UNVERIFIED: whether colour settings are included (believed yes) and which settings are skipped.
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** BAE decision: implement as a Command taking the interpretation record and a list of target Assets.

### AE-PRJ-059 · Footage interpretation on replacement preference (替换时的素材解释规则)

- **What it does:** Chooses whether replacing a footage item's source keeps or resets its interpretation.
- **Exact semantics:** Preferences > Import > Footage Interpretation on Replacement: Reset (重置): new source gets default interpretation for that file; Remember (记住): frame rate, alpha interpretation and other settings of the replaced footage are kept. Default: UNVERIFIED (believed Remember, matching AE's long-standing behaviour of keeping interpretation on Replace Footage). Applies to File > Replace Footage > File and to Alt-drag replacement in the Project panel. UNVERIFIED: introduced as a preference in AE 25.x.
- **Priority:** P1
- **Owner:** bae-media (+ apps/bae)
- **Notes:** See Replace Footage (AE-PRJ-065).

### AE-PRJ-060 · Interpretation rules file (解释规则文件)

- **What it does:** A user-editable text file of rules that set default interpretation for imported footage matching file type, codec, size and frame rate.
- **Exact semantics:** File `interpretation rules.txt` (some updates install a versioned name) in the per-version preferences folder (macOS `~/Library/Preferences/Adobe/After Effects/<version>/`, Windows `%APPDATA%\Adobe\After Effects\<version>\`). Each non-comment line is `match ~ action`: match fields (width, height, frame rate, file-type four-char code, codec four-char code) with `*` wildcards; action fields (PAR, field order, pulldown, alpha, frame rate, colour profile...). Applied at import after file metadata, before user changes; AE must be restarted after editing. UNVERIFIED: exact field order and syntax; AE 26 still ships the file.
- **Priority:** P2
- **Owner:** bae-media (+ apps/bae)
- **Notes:** Alt/Option-click an item in the Project panel shows its file-type and codec codes. BAE decision: provide import rules as a JSON/TOML preference with typed fields rather than AE's positional format.

## Video, audio and data import

### AE-PRJ-061 · Video file metadata on import (视频素材导入)

- **What it does:** Reads the technical properties of a movie file into the footage item and its default interpretation.
- **Exact semantics:** On import AE reads and stores:
  | Property | Source | Used for |
  |---|---|---|
  | width, height | stream dimensions (clean vs production aperture as tagged) | item size |
  | native frame rate | container/stream time base (e.g. 30000/1001) | AE-PRJ-049 |
  | frame count / duration | container | item duration |
  | pixel aspect ratio | pasp / container tag, else size guess | AE-PRJ-053, AE-PRJ-054 |
  | field order | stream/container tag (progressive, TFF, BFF) | AE-PRJ-051 |
  | alpha presence and type | codec (e.g. ProRes 4444, PNG, Animation, HAP Alpha) and alpha-type flag | AE-PRJ-047 |
  | bit depth class | codec (8, 10, 12, 16 bit, float) | Project panel depth descriptor; decode precision |
  | colour metadata | primaries / transfer / matrix / range tags, embedded ICC | AE-PRJ-057 |
  | embedded timecode | tmcd track, MXF/DPX timecode | media start (AE-PRJ-050) |
  | audio streams | see AE-PRJ-062 | — |
  | XMP metadata, tape/reel name | container | Project panel columns |
  - Y'CbCr sources are converted to RGB using the tagged matrix and range (video range 16-235 expanded to full range for 8-bit). UNVERIFIED: handling of untagged range.
  - Hardware-accelerated decoding (Preferences > Import > Video Footage > Enable hardware accelerated decoding, H.264/HEVC) changes speed only, not results. UNVERIFIED default on.
- **Priority:** P0
- **Owner:** bae-media
- **Notes:** Format/codec support list: see ui-preview-render-io.md, import/export format lists. BAE decision: decode via platform frameworks (VideoToolbox / Media Foundation) plus bundled royalty-free decoders; no statically linked FFmpeg (AGENTS.md rule).

### AE-PRJ-062 · Audio import (音频素材导入)

- **What it does:** Imports audio-only files and the audio streams of movies as the audio part of footage items.
- **Exact semantics:**
  - Audio-only files (WAV, AIFF, MP3, AAC/M4A, ...) create items with `hasAudio = true`, `hasVideo = false`, duration from the stream.
  - Read: sample rate, sample format/bit depth, channel count and layout. AE's audio engine mixes in stereo: mono sources are placed in both channels; multichannel (e.g. 5.1) sources are downmixed to stereo. UNVERIFIED: downmix coefficients and whether only the first stereo pair is used.
  - Audio is resampled to the project audio sample rate (AE-PRJ-088) for preview and render; resampling quality: UNVERIFIED.
  - Compressed audio may be conformed into the Media Cache (conformed .cfa files and .pek peak files) the first time it is used. UNVERIFIED for AE 26.
  - Only the first audio stream of a multi-stream file is used. UNVERIFIED.
  - Audio is never affected by frame-rate conform (AE-PRJ-049) but is by Loop (AE-PRJ-055).
- **Priority:** P0
- **Owner:** bae-media
- **Notes:** Audio layer properties (Audio Levels, waveform) and audio effects: see project-comp-layers.md, layer types, and effects.md, Audio effects. Media Cache settings: see ui-preview-render-io.md, Preferences.

### AE-PRJ-063 · Variable-frame-rate media (可变帧速率素材（译名待核）)

- **What it does:** Presents footage whose frames have irregular timestamps (phone/screen recordings) as constant-frame-rate footage.
- **Exact semantics:** AE assigns a constant native frame rate to VFR footage (the stream's nominal/average rate) and maps comp time to source frames by timestamp. UNVERIFIED behaviour (AE does not document its VFR algorithm; observed: frames are shown by presentation timestamp, so duplicated/dropped frames appear where the source had gaps, and A/V sync is preserved). Conforming (AE-PRJ-049) of VFR footage re-times by frame index and breaks sync.
  ```
  BAE rule (proposed, AE-compatible in effect):
    nominal_rate = most common frame interval of the stream (fallback: frame_count / duration)
    frame_at(t) = the last frame whose PTS <= t + half_frame_epsilon   (PTS relative to first frame)
  ```
- **Priority:** P1 — very common source today; wrong handling causes A/V drift.
- **Owner:** bae-media
- **Notes:** UNVERIFIED: AE version that started detecting VFR and warning about it. Implementation hint: build a PTS index at import (keyframe scan) so random access is O(log n).

### AE-PRJ-064 · Data file import (JSON, CSV, TSV, MGJSON) (数据文件导入（译名待核）)

- **What it does:** Imports structured data files as footage items that expressions can read and that can drive data layers.
- **Exact semantics:**
  - JSON (`.json`, AE 15.0), CSV and TSV/TXT (UNVERIFIED: AE 16.1/17.0), and mgJSON (`.mgjson`, motion-graphics JSON with timed streams, AE 15.0) import as footage items without video or audio (duration 0 for JSON/CSV; mgJSON has duration from its samples).
  - Added to a comp, a JSON/CSV item makes a layer with no image whose data is read by expressions (`footage("data.json").sourceData`, `.dataValue([row, col])` for CSV); an mgJSON item makes a Data layer whose Data properties are keyframed from the streams. Layer > Time > Align Video to Data uses mgJSON timing.
  - From AE 15.1, a `.jsx` file with the same content as a JSON file can be imported as data (for expressions `footage("x.jsx").sourceData`); Adobe's help recommends it as the workaround for JSON files whose expressions do not work, via Replace Footage (Source: https://helpx.adobe.com/after-effects/using/data-driven-animation.html).
  - Reload Footage re-reads the file; expressions re-evaluate.
- **Priority:** P1
- **Owner:** bae-media (+ bae-anim, bae-model)
- **Notes:** Expression API: see transform-keyframes-expressions.md, data footage. Layer type: see project-comp-layers.md, layer types.

## Replace, reload and missing footage

### AE-PRJ-065 · Replace Footage with file (替换素材 > 文件)

- **What it does:** Points an existing footage item at a different file; every layer using the item switches to the new source and keeps all its properties.
- **Exact semantics:**
  - File > Replace Footage > File (Ctrl+H / Cmd+H) with one footage item selected; the Import dialog appears (with Sequence / Force alphabetical options). The item keeps its ID, name (unless the name was derived from the file, in which case it follows the new file name; UNVERIFIED), label, comment, folder, proxy and all uses; layers keep masks, effects, keyframes, expressions, timing and transforms.
  - Interpretation: kept or reset per the Footage Interpretation on Replacement preference (AE-PRJ-059); properties that are read from the new file (size, native rate, duration, alpha presence) always update.
  - If the new source has a different size, layers keep their transform values, so anchor point/position relative to the image change; if shorter, layers are not trimmed (frames past the end hold the last frame? UNVERIFIED: AE shows nothing / holds last frame beyond source end for footage layers whose Out point exceeds the source).
  - Alt/Option-drag an item from the Project panel onto another selected item in the Project panel replaces all uses of the target item(s) with the dragged item (the target item remains, now unused). UNVERIFIED: whether the target items are deleted.
  - Single-layer replacement (Alt-drag onto a layer) is layer Replace Source: see project-comp-layers.md, Replace layer source.
  - Replacing works on missing items (relink) and placeholders (AE-PRJ-083).
- **Priority:** P0
- **Owner:** bae-model (+ bae-media, bae-motion)
- **Notes:** Scripting `FootageItem.replace(file)`, `replaceWithSequence(file, forceAlphabetical)`. BAE decision: Replace is a Command that swaps the Asset's source record; layer Prims reference the Asset by ID so no layer is touched.

### AE-PRJ-066 · Replace Footage with placeholder or solid (替换素材 > 占位符 / 纯色)

- **What it does:** Turns an existing footage item into a placeholder or a solid while keeping all its uses.
- **Exact semantics:**
  - File > Replace Footage > Placeholder (占位符): dialog with Name, Width, Height, Frame Rate, Duration (defaults: the item's current values); the item becomes a `PlaceholderSource` (rendered as colour bars, AE-PRJ-082).
  - File > Replace Footage > Solid (纯色): Solid Settings dialog (AE-PRJ-079); the item becomes a `SolidSource` and moves into the solids folder. UNVERIFIED: whether it is moved.
  - File > Replace Footage > With Layered Comp: AE-PRJ-033.
  - Scripting `replaceWithPlaceholder(name, width, height, frameRate, duration)`, `replaceWithSolid(color, name, width, height, pixelAspect)`.
- **Priority:** P1
- **Owner:** bae-model (+ bae-motion)
- **Notes:** —

### AE-PRJ-067 · Reload Footage (重新加载素材)

- **What it does:** Re-reads the selected footage items' source files from disk so changes made by other applications appear.
- **Exact semantics:** File > Reload Footage (Ctrl+Alt+L / Cmd+Opt+L) on selected items (or footage layers' sources): drops cached frames, re-probes metadata (size, duration, frame count, layers of PSD/AI), keeps the interpretation. For layered files, renamed source layers stay linked, deleted layers become Missing, new layers are not added. For image sequences the frame range is re-scanned. Missing files become found if they reappeared. Not undoable (UNVERIFIED). Scripting `FileSource.reload()` (main source only).
- **Priority:** P0
- **Owner:** bae-media (+ bae-render)
- **Notes:** Implementation hint: compare the stored fingerprint (size + mtime, ARCHITECTURE 16) to decide what actually changed and invalidate only those caches.

### AE-PRJ-068 · Automatic footage reloading (自动重新加载素材)

- **What it does:** Reloads changed footage automatically when AE becomes the active application again.
- **Exact semantics:** Preferences > Import > Automatic Footage Reloading > Auto Reload: All Footage Types (所有素材类型) / Non-sequence Footage (非序列素材, default) / Off (关). On application activation AE checks modification dates of the relevant sources and reloads changed ones (as Reload Footage). Image sequences are excluded by default because checking every frame is slow.
- **Priority:** P1
- **Owner:** bae-media (+ apps/bae)
- **Notes:** BAE decision: use OS file-watch notifications (FSEvents / ReadDirectoryChangesW / inotify) instead of polling on activation, with the same three-way preference.

### AE-PRJ-069 · Reveal source file (在 Finder 中显示 / 在资源管理器中显示)

- **What it does:** Opens the OS file browser at the footage item's source file.
- **Exact semantics:** Project panel context menu > Reveal in Finder (macOS) / Reveal in Explorer (Windows) (在 Windows 资源管理器中显示); Reveal in Bridge (在 Bridge 中显示) when Bridge is installed. For sequences, the first file is selected. Disabled for missing files, solids and placeholders. Also available for a layer's source from the Timeline context menu (Reveal Layer Source in Project, then reveal). UNVERIFIED: Timeline also offers Reveal in Finder directly.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ apps/bae)
- **Notes:** Reveal in Bridge is P2 / not applicable to BAE.

### AE-PRJ-070 · Edit Original (编辑原稿)

- **What it does:** Opens the footage's source file in the application associated with it, and reloads it when saved.
- **Exact semantics:** Edit > Edit Original (Ctrl+E / Cmd+E) with a footage item or a layer selected opens the source in its creating/associated app. For a sequence selected in the Timeline/Composition panel the currently displayed frame's file opens; selected in the Project panel, the first file. Changes are picked up when AE becomes active again (auto reload or manual Reload Footage). Alt/Option-double-click in the Project panel opens the file in the OS-associated player.
- **Priority:** P2
- **Owner:** apps/bae (+ bae-ui-motion)
- **Notes:** —

### AE-PRJ-071 · Missing footage state and display (缺失素材)

- **What it does:** A footage item whose source file cannot be found stays in the project with all its uses, flagged as missing and rendered as colour bars.
- **Exact semantics:**
  - Detection: on project open, reload, or when a decode fails because the file is gone. `AVItem.footageMissing = true`; `FileSource.missingFootagePath` holds the last known path.
  - Project panel: name in italics, Type/File Path show it as Missing; search filter `missing` finds them. Size/duration/frame rate/interpretation remain as last known, so layer timing is unchanged.
  - Rendering: every frame of the item is replaced by a colour-bars placeholder image at the item's width x height (the same image as placeholders, AE-PRJ-082); effects, masks and transforms still apply to it.
  - Missing frames inside a sequence: AE-PRJ-041.
  - Render Queue: GUI renders proceed using colour bars (UNVERIFIED: a warning is shown before rendering); `aerender` stops on missing footage unless `-continueOnMissingFootage` is given (see ui-preview-render-io.md, headless render).
- **Priority:** P0
- **Owner:** bae-model (+ bae-media, bae-render)
- **Notes:** BAE decision: the missing state is derived (not persisted) from the asset resolution result of ARCHITECTURE 16; the persisted data is the reference record. A missing asset must never be dropped or rewritten on save.

### AE-PRJ-072 · Missing footage, effects and fonts reports (查找缺失的素材 / 效果 / 字体（译名待核）)

- **What it does:** Reports, on project open and on demand, the footage, effects and fonts the project needs but the machine lacks.
- **Exact semantics:**
  - On open AE shows a dialog per category: "N files are missing" with the last missing path; a list of missing effects (by display name); a list of missing fonts (suppressible: Preferences > "Do not warn me about missing fonts when opening projects"). UNVERIFIED exact texts.
  - File > Dependencies > Find Missing Footage / Find Missing Effects / Find Missing Fonts (added in AE CC 12.0 (2013); the Project panel search term "missing" offers the same filters): filters the Project panel to missing footage, or the Timeline panel to layers that use missing effects/fonts.
  - Missing effects: the effect instance remains on the layer with all parameter values preserved and is shown as missing; it renders as a pass-through (no effect). Missing fonts: text layers keep the font name and render with a substitute font until the font is installed; Edit > Find and replace font / `Project.replaceFont()` can retarget. See text.md, fonts, and effects.md, effect framework.
  - Scripting `Project.usedFonts` lists fonts used by text layers.
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion, bae-plugin, bae-text)
- **Notes:** BAE decision: matches ARCHITECTURE 4.6 (plugin data kept with plugin ID/version and shown as placeholder when the plugin is missing); missing-items report is also produced by Collect Files and `.baez` packaging.

### AE-PRJ-073 · Relinking missing footage (重新链接素材（译名待核）)

- **What it does:** Finds moved footage automatically on open, lets the user point one missing item at its new location, and then relinks other missing items found in the same folder.
- **Exact semantics:**
  - Search on open: for each file reference AE tries the stored absolute path; if not found, the path relative to the project file (stored at save time), which is how collected projects open on other machines. UNVERIFIED: order of these two attempts.
  - Manual relink: double-click a missing item (or Replace Footage > File) and choose the file. After a manual relink, AE searches the chosen file's folder for other missing items (same file names, applying the same old-folder -> new-folder relationship) and relinks them, reporting how many additional files were found. UNVERIFIED: exact search scope (same folder only vs relative sub-folders).
  - Relinking keeps interpretation, IDs and all uses.
- **Priority:** P0
- **Owner:** bae-model (+ bae-media, bae-ui-motion)
- **Notes:** BAE decision (ARCHITECTURE 16): every asset stores (a) path relative to the project, (b) original absolute path, (c) fingerprint (size, mtime, optional content hash); resolution order on open is relative -> absolute -> name+fingerprint search in the project folder and recent search folders -> missing with "Relink". After one manual relink, the same directory mapping (old prefix -> new prefix) is applied to all other missing assets. Paths are stored with `/` separators, and team path-mapping rules (e.g. `Z:\` <-> `/Volumes/NAS`) are applied before searching. This is a superset of AE's behaviour.

## Proxies

### AE-PRJ-074 · Set Proxy (设置代理)

- **What it does:** Assigns a substitute source (usually a smaller or still version) to a footage item or composition; while the proxy is in use, every comp uses the proxy in place of the original.
- **Exact semantics:**
  - File > Set Proxy > File (文件) (Ctrl+Alt+P / Cmd+Opt+P) with one footage item or comp selected: choose a file (Sequence / Force alphabetical options available); `useProxy` becomes true. File > Set Proxy > None (无): removes the proxy, `useProxy` false.
  - Compositions can have proxies too (typically a pre-rendered movie of the comp); a comp with an active proxy is not rendered from its layers when used as a precomp. (See Render Settings Proxy Use, AE-PRJ-078.)
  - The proxy source has its own interpretation (File > Interpret Footage > Proxy).
  - Scripting: `AVItem.setProxy(file)`, `setProxyWithSequence(file, forceAlphabetical)`, `setProxyWithSolid(color, name, width, height, pixelAspect)`, `setProxyWithPlaceholder(name, width, height, frameRate, duration)`, `setProxyToNone()`; `proxySource`, `useProxy`.
  - Anything applied to layers (masks, effects, keyframes, expressions) is unaffected by switching between proxy and original.
- **Priority:** P1
- **Owner:** bae-model (+ bae-media)
- **Notes:** BAE decision: BAE additionally generates automatic decode proxies/caches in `cache/` (ARCHITECTURE 4.6); those are invisible performance caches and must not be confused with user proxies, which are project data.

### AE-PRJ-075 · Proxy scaling to the original (代理缩放（译名待核）)

- **What it does:** Makes a proxy of any size behave exactly like the original's dimensions and duration in comps.
- **Exact semantics:**
  ```
  sx = original.width  / proxy.width ;  sy = original.height / proxy.height
  proxy pixels are resampled by (sx, sy) to original.width x original.height before the layer transform
  (if the frame aspect ratios differ, sx != sy and the proxy is stretched; this is slower)
  layer bounds, anchor point, masks and effect coordinates stay in original pixel space
  ```
  - Duration: the item keeps the original's duration, frame rate and timing; a still proxy is held for the whole duration; a movie proxy is sampled at original time t (UNVERIFIED: a shorter/longer proxy movie is not stretched, frames past its end hold or show empty).
  - PAR: the original's PAR is used. UNVERIFIED.
  - Recommended: same frame aspect ratio, e.g. 160x120 proxy for 640x480 (exact 4x).
- **Priority:** P1
- **Owner:** bae-render (+ bae-media)
- **Notes:** Implementation hint: treat proxy use as a lower-resolution mip of the original source: the render graph samples the proxy texture with a scale factor instead of materialising an upscaled image.

### AE-PRJ-076 · Proxy indicators and toggle (代理指示器（译名待核）)

- **What it does:** Shows and toggles whether each item currently uses its proxy.
- **Exact semantics:** Project panel, left of the item name: filled box = proxy assigned and in use (project-wide); hollow box = proxy assigned but original in use; no box = no proxy. Clicking the box toggles `useProxy` (undoable, UNVERIFIED). When the item is selected, the proxy's file name is shown in bold at the top of the Project panel. Proxy use is project-wide, not per comp.
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** —

### AE-PRJ-077 · Create Proxy (创建代理)

- **What it does:** Renders a still or movie proxy of the selected footage or comp through the Render Queue and assigns it automatically.
- **Exact semantics:** File > Create Proxy > Still (静止图像) or Movie (影片) with a footage item or comp selected (Project or Timeline panel): asks for a name/location, adds a render queue item with Post-Render Action = Set Proxy (设置代理) and default proxy templates. Still: renders the frame at the current time of the Footage/Composition panel (the poster frame). Default templates: UNVERIFIED (believed Render Settings "Draft Settings" at half resolution and output module "Photoshop" for stills / "Lossless with Alpha" for movies, settable in Edit > Templates > Render Settings > "Movie Proxy Default" / "Still Proxy Default"). After rendering, the output is set as the item's proxy and is in use.
- **Priority:** P2
- **Owner:** bae-motion (+ bae-media, bae-ui-motion)
- **Notes:** Render Queue post-render actions: see ui-preview-render-io.md, Render Queue.

### AE-PRJ-078 · Proxy use when rendering (代理使用)

- **What it does:** Render Settings decide whether a render uses proxies.
- **Exact semantics:** Render Settings > Proxy Use (代理使用): Current Settings (当前设置, default; each item's `useProxy`) / Use All Proxies (使用所有代理) / Use Comp Proxies Only (仅使用合成代理) / Use No Proxies (不使用代理). UNVERIFIED: menu order and Chinese strings. Default render-settings template "Best Settings" uses Use No Proxies (UNVERIFIED).
- **Priority:** P1
- **Owner:** bae-render (+ bae-media)
- **Notes:** Specified fully in ui-preview-render-io.md, Render Queue (render settings); listed here for the proxy data model. Scripting `RenderQueueItem` setting "Proxy Use".

## Solids

### AE-PRJ-079 · Solid footage items and Solid Settings dialog (纯色设置)

- **What it does:** Creates or edits a solid-colour footage item of a given size, pixel aspect ratio and colour.
- **Exact semantics:**
  - Create: Layer > New > Solid (Ctrl+Y / Cmd+Y) creates the item plus a layer in the active comp; File > Import > Solid creates only the item. Edit: Layer > Solid Settings (Ctrl+Shift+Y / Cmd+Shift+Y) on a solid layer, or on a solid item in the Project panel.
  - Dialog fields in UI order:
    | Parameter (中文) | Type | Default | Range | Units | Notes |
    |---|---|---|---|---|---|
    | Name (名称) | string | "<colour name> Solid <n>" (e.g. "Black Solid 1"), updated as colour changes until edited | — | — | UNVERIFIED: AE's colour-naming table |
    | Width (宽度) | number | comp width (no comp: last used) | 1..30000 px | per Units | Lock Aspect Ratio to <ratio> checkbox |
    | Height (高度) | number | comp height | 1..30000 px | per Units | — |
    | Units (单位) | enum | pixels | pixels (像素) / inches (英寸) / millimeters (毫米) / % of composition (合成的 %) | — | inches and mm use 72 ppi (UNVERIFIED); % relative to comp size |
    | Pixel Aspect Ratio (像素长宽比) | enum | comp PAR | AE-PRJ-053 list | — | — |
    | Frame Aspect Ratio | read-only | — | — | — | shown, e.g. "16:9 (1.78)" |
    | Make Comp Size (制作合成大小) | button | — | — | — | sets width, height and PAR to the comp's |
    | Color (颜色) | RGB | last used; initially 17% grey 45/255 (AE CS6+, per Adobe help) | 0..1 per channel (picker in 8-bit or float per project depth) | — | — |
    | Affect all layers that use this solid (影响使用此纯色的所有图层) | bool | off | — | — | Solid Settings only, AE-PRJ-080 |
  - The item is `isStill`, duration 0; it renders as a uniform colour with alpha 1 over its whole frame. Colour is stored in the project working space (no colour conversion; with colour management on it is interpreted as working-space values). UNVERIFIED.
  - New items go to the solids folder (AE-PRJ-021).
  - Each New Solid creates a new item (solids are not reused automatically even if identical). UNVERIFIED.
- **Priority:** P0
- **Owner:** bae-model (+ bae-motion, bae-ui-motion)
- **Notes:** Scripting `ItemCollection.addSolid(color, name, width, height, pixelAspect, duration)`, `SolidSource.color`; `LayerCollection.addSolid(...)`. Solid layers (layer type) and Lottie `ty: 1` (`sc`, `sw`, `sh`): see project-comp-layers.md, layer types. Null and adjustment layers created by AE also have solid sources (white 100x100 null solid "Null 1"; comp-size adjustment solid). UNVERIFIED: AE 2026 still creates solid items for nulls.

### AE-PRJ-080 · Affect all layers that use this solid (影响使用此纯色的所有图层)

- **What it does:** Decides whether editing a solid layer's settings changes the shared solid item or forks a new one.
- **Exact semantics:**
  ```
  SolidSettings(layer L, new settings S, affect_all):
    item = L.source
    if affect_all or item.usedIn has only L (single use):     # UNVERIFIED single-use shortcut
        item.width/height/par/color/name = S                 # all layers using item change
    else:
        new_item = new solid item with S (in solids folder); L.source = new_item
  ```
  - Size changes keep the layer's Anchor Point/Position values (the anchor point is not re-centred). UNVERIFIED: AE re-centres the anchor point when the anchor was at the old centre.
  - Editing a solid item from the Project panel always affects all its layers.
- **Priority:** P1
- **Owner:** bae-motion (+ bae-model)
- **Notes:** UNVERIFIED: whether the checkbox is hidden or disabled when only one layer uses the solid.

## Placeholders

### AE-PRJ-081 · Import placeholder (导入占位符)

- **What it does:** Creates a footage item that stands in for footage not yet available, with chosen name, size, frame rate and duration.
- **Exact semantics:** File > Import > Placeholder (占位符) dialog:
  | Parameter (中文) | Type | Default | Range | Units |
  |---|---|---|---|---|
  | Name (名称) | string | "Placeholder" (UNVERIFIED) | — | — |
  | Width (宽度) | int | UNVERIFIED: 640 or last comp width | 1..30000 | px |
  | Height (高度) | int | UNVERIFIED: 480 or last comp height | 1..30000 | px |
  | Frame Rate (帧速率) | float | UNVERIFIED: 30 | 1..999 | fps |
  | Duration (持续时间) | time | UNVERIFIED: 0:00:30:00 | ≥ 0 (0 => still) | display style |
  - Creates a `PlaceholderSource` item with `footageMissing = true`. Recommended: match the expected real footage.
- **Priority:** P1
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** Scripting `Project.importPlaceholder(name, width, height, frameRate, duration)`; `FootageItem.replaceWithPlaceholder(...)`.

### AE-PRJ-082 · Placeholder rendering (占位符显示)

- **What it does:** Placeholders and missing footage render as a colour-bars test image.
- **Exact semantics:** Every frame of a placeholder (or missing footage, AE-PRJ-071) renders as a colour-bars image at the item's size, alpha 1. The pattern is AE's own colour-bars still. UNVERIFIED: exact bar layout and values (believed 75% SMPTE-style vertical bars: grey/white, yellow, cyan, green, magenta, red, blue, with lower bands). Effects, masks and transforms apply normally. In Draft/Adaptive resolution it is downsampled like footage.
- **Priority:** P1
- **Owner:** bae-render (+ bae-media)
- **Notes:** BAE decision: generate the pattern procedurally in a shader (no bundled image). Exact colours are not important for fidelity; document BAE's pattern in the renderer.

### AE-PRJ-083 · Replacing placeholders and output placeholders (替换占位符 / 输出占位符)

- **What it does:** Placeholders are swapped for real footage with Replace Footage, or are created from Render Queue outputs and replaced automatically after rendering.
- **Exact semantics:**
  - Replace: select the placeholder, File > Replace Footage > File (or double-click it): all uses switch to the real file; masks, effects, keyframes, expressions on layers are kept. Interpretation defaults to the new file's (AE-PRJ-059 applies). UNVERIFIED.
  - Output placeholder: dragging a Render Queue output module heading into the Project panel creates a placeholder for that output (name/size/rate/duration of the output) and sets the item's Post-Render Action to Import & Replace Usage (导入和替换用法); when the output finishes rendering, every use of the placeholder is replaced by the rendered file. A comp that uses an output placeholder of another queued item can render after it (pre-render chains; render order by queue order).
- **Priority:** P2
- **Owner:** bae-motion (+ bae-model, apps/bae)
- **Notes:** Post-render actions: see ui-preview-render-io.md, Render Queue.

## Project Settings

### AE-PRJ-084 · Project Settings dialog (项目设置)

- **What it does:** Stores project-wide settings that affect every composition: rendering backend, time display, colour pipeline, audio rate and expression engine.
- **Exact semantics:**
  - File > Project Settings (Ctrl+Alt+Shift+K / Cmd+Opt+Shift+K), also from the Project panel menu and by clicking the bit-depth button at the bottom of the Project panel (opens the Color tab).
  - Tabs in UI order: Video Rendering and Effects (视频渲染和效果), Time Display Style (时间显示样式), Color (颜色), Audio (音频), Expressions (表达式). UNVERIFIED: AE 26/27 may add further tabs (e.g. 3D).
  - All settings are saved in the project file (not in preferences), are undoable as one step per OK, and invalidate caches when they change rendering (bit depth, colour, GPU type does not change results by design).
  - New projects take the defaults below, or the template project's settings (AE-PRJ-012).
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** BAE decision: Project Settings live on the `Project` Prim in the root sublayer (ARCHITECTURE 4.3 "root: 工程设置"); a stronger sublayer may not override them (UNVERIFIED design choice; to be decided by core/doc-model).

### AE-PRJ-085 · Video Rendering and Effects setting (视频渲染和效果)

- **What it does:** Chooses the GPU or CPU backend AE uses for GPU-capable effects and the renderer.
- **Exact semantics:** Use (使用): Mercury GPU Acceleration (Metal) (macOS) / Mercury GPU Acceleration (CUDA) (NVIDIA, Windows) / Mercury GPU Acceleration (OpenCL) (deprecated) / Mercury Software Only (仅 Mercury 软件). Default: the best GPU type available on the machine; Software Only if no supported GPU. Results are intended to be identical across backends (small floating-point differences exist). Scripting `Project.gpuAccelType` (`GpuAccelType.CUDA | METAL | OPENCL | SOFTWARE`, AE 13.8) and `app.availableGPUAccelTypes`. Opening a project saved with an unavailable backend falls back silently. UNVERIFIED: whether OpenCL is still listed on Windows in AE 26.
- **Priority:** P2 — BAE is GPU-only via wgpu.
- **Owner:** bae-model (+ bae-render)
- **Notes:** BAE decision: store the AE value for round-trip only; BAE always renders through wgpu (Metal/Vulkan/DX12) and a CPU fallback is not an M1 goal. Effect GPU/CPU parity is a per-effect concern (effects.md).

### AE-PRJ-086 · Time Display Style (时间显示样式)

- **What it does:** Chooses how times are shown throughout the project: SMPTE timecode, frame numbers, or feet+frames; display only, never timing.
- **Exact semantics:**
  | Parameter (中文) | Type | Default | Values | Notes |
  |---|---|---|---|---|
  | Timecode (时间码) | radio | selected | — | `TimeDisplayType.TIMECODE` |
  | Footage Start Time (素材开始时间) | enum | Use Media Source (使用媒体源) | Use Media Source / Start at 00:00:00:00 (从 00:00:00:00 开始) | `footageTimecodeDisplayStartType`: `FTCS_USE_SOURCE_MEDIA` / `FTCS_START_0`; only with Timecode |
  | Frames (帧) | radio | — | — | `TimeDisplayType.FRAMES` |
  | Use Feet + Frames (使用英尺 + 帧数) | bool + enum | off; 35mm | 35mm (16 frames per foot) / 16mm (40 frames per foot) | `framesUseFeetFrames`, `feetFramesFilmType` (`MM35`, `MM16`) |
  | Frame Count (帧计数) | enum | UNVERIFIED: Timecode Conversion | Timecode Conversion (时间码转换) / Start at 0 (从 0 开始) / Start at 1 (从 1 开始) | `framesCountType` (`FC_TIMECODE_CONVERSION`, `FC_START_0`, `FC_START_1`); `displayStartFrame` 0/1 shortcut |
  ```
  Timecode display of time t (seconds) at frame rate r, comp/footage start offset s:
    base = round(r)                         (29.97 -> 30, 23.976 -> 24, 59.94 -> 60)
    n = floor((t + s) * r + eps)            frame count
    non-drop:  HH:MM:SS:FF  with FF = n mod base, SS = (n / base) mod 60, ...   separators ':'
    drop-frame (only r = 29.97 or 59.94; Composition Settings choose DF/NDF):
       d = 2 (29.97) or 4 (59.94) frame numbers dropped at the start of every minute except minutes divisible by 10
       separators ';'   (e.g. 0;00;59;29 -> 0;01;00;02)
  Frames display:   Start at 0 -> n ; Start at 1 -> n + 1 ; Timecode Conversion -> n including the source/comp start timecode offset
  Feet + Frames:    feet = n div fpf, frames = n mod fpf (fpf = 16 or 40), shown as "ffff+ff"
  ```
  - Ctrl/Cmd-click on any current-time display cycles Timecode <-> Frames/Feet+Frames for the project.
  - "Show Both Timecode and Frames in Timeline Panel" (Preferences > Appearance, default on) shows the other unit in small text.
  - Drop-frame for a comp is chosen in Composition Settings (see project-comp-layers.md, Composition Settings); footage DF/NDF follows the source.
- **Priority:** P0
- **Owner:** bae-model (+ bae-ui-motion)
- **Notes:** UNVERIFIED: the frame-count default. Scripting `timeDisplayType`, `framesCountType`, `displayStartFrame`, `footageTimecodeDisplayStartType`, `framesUseFeetFrames`, `feetFramesFilmType`; `timeToCurrentFormat()` / `currentFormatToTime()` globals use these settings. BAE decision: BAE times are flicks; display conversion uses the exact rational rate, so drop-frame maths is exact.

### AE-PRJ-087 · Color settings (颜色设置)

- **What it does:** Stores the project's colour pipeline choices: colour engine, bit depth, working space and blending options.
- **Exact semantics:** Project Settings > Color tab, stored values (render semantics: compositing-3d-color.md, colour management; bit depth):
  | Parameter (中文) | Type | Default | Values | Scripting / notes |
  |---|---|---|---|---|
  | Color Engine (颜色引擎) | enum | Adobe color managed (Adobe 颜色管理（译名待核）) | Adobe color managed / OCIO color managed (OCIO 颜色管理（译名待核）) | OCIO engine shipped in AE 23.2 (Feb 2023; beta from 23.1, Oct 2022); OCIO library 2.3.2 in AE 25.0; OCIO 2.5.1 with ACES 2.0 configs since AE 26.5 (Sep 2026) |
  | Bit Depth (位深度) | enum | 8 bits per channel | 8 / 16 / 32 (float) bits per channel (位/通道) | `bitsPerChannel`; Alt/Option-click the Project panel depth button cycles it |
  | Working Space (工作空间) | enum | None (无) | None or any RGB ICC profile (e.g. sRGB IEC61966-2.1, HDTV (Rec. 709), Rec.709 Gamma 2.4, Rec.2020, ProPhoto RGB...) | `workingSpace` ("" = None); `listColorProfiles()`; None = colour management off |
  | Working gamma (工作伽玛（译名待核）) | enum | 2.4 | 2.2 / 2.4 | `workingGamma`; used only when Working Space is None (affects some gamma-dependent operations); UNVERIFIED default |
  | Linearize Working Space (线性化工作空间) | bool | off | — | `linearizeWorkingSpace`; available only with a working space |
  | Blend Colors Using 1.0 Gamma (使用 1.0 灰度系数混合颜色) | bool | off | — | `linearBlending`; affects only blending between layers |
  | Match Legacy After Effects QuickTime Gamma Adjustments (匹配旧版 After Effects QuickTime 灰度系数调整（译名待核）) | bool | off | — | legacy 1.8/2.2 gamma handling for QuickTime |
  | Compensate for Scene-referred Profiles (补偿场景引用的配置文件（译名待核）) | bool | UNVERIFIED: on | — | `compensateForSceneReferredProfiles` (AE 16.0) |
  | OCIO Configuration (OCIO 配置) | enum/path | ACES 2.0 Studio v4.0 (UNVERIFIED default) | built-in configs (ACES 1.3 Studio/CG, ACES 2.0 CG v4.0, ACES 2.0 Studio v4.0) / Custom (.ocio file) | OCIO engine only |
  | Working Color Space (OCIO) (工作色彩空间) | enum | config default (e.g. ACEScg) | any scene-linear space of the config | OCIO only |
  | Display Color Space (OCIO) (显示色彩空间) | enum | config default display/view | config displays/views | OCIO only; also switchable in the viewer |
  - "Blend Text Colors Using 1.0 Gamma": not present in AE (UNVERIFIED); text blending follows the layer blending option.
  - Changing bit depth or working space converts nothing stored (colours are stored as floats); results re-render.
- **Priority:** P0 — bit depth and working space; OCIO fields P1.
- **Owner:** bae-model (+ bae-color)
- **Notes:** Owner of semantics: compositing-3d-color.md (bae-color). BAE decision: BAE may default new projects to 16 bpc or 32 bpc float internally for performance/quality (GPU), but must expose and store the AE setting so AE-faithful 8 bpc rounding can be reproduced when requested.

### AE-PRJ-088 · Audio sample rate (音频设置 - 采样率)

- **What it does:** Sets the sample rate at which AE mixes and previews audio for the project.
- **Exact semantics:** Project Settings > Audio > Sample Rate (采样率): enum, default 48.000 kHz (UNVERIFIED). Values (UNVERIFIED list): 22.050, 32.000, 44.100, 48.000, 88.200, 96.000 kHz. All audio sources are resampled to this rate for preview and render mixing; Output Module audio settings may resample again for output (see ui-preview-render-io.md, Render Queue audio).
- **Priority:** P1
- **Owner:** bae-model (+ bae-media)
- **Notes:** UNVERIFIED: AE version that moved the default from 44.1 to 48 kHz; the offered list may differ by platform (a 2021 Adobe community thread reports a macOS AE 18.2 offering nothing above 48 kHz while Windows showed 96 kHz). Scripting exposes no Project attribute for the audio sample rate (UNVERIFIED).

### AE-PRJ-089 · Expression engine setting (表达式引擎)

- **What it does:** Chooses which JavaScript engine evaluates the project's expressions.
- **Exact semantics:** Project Settings > Expressions > Expression Engine (表达式引擎): JavaScript (default for new projects since AE 16.0 (CC 2019); ECMAScript 2018) / Legacy ExtendScript (旧版 ExtendScript, ECMAScript 3). Scripting `Project.expressionEngine` = `"javascript-1.0"` or `"extendscript"`. Projects saved before AE 16.0 open with Legacy ExtendScript. Switching re-evaluates all expressions; syntax valid only in one engine produces errors in the other.
- **Priority:** P0
- **Owner:** bae-model (+ bae-anim)
- **Notes:** Engine semantics and the incompatibility list: see transform-keyframes-expressions.md, expression engines. BAE decision: BAE evaluates both settings with QuickJS (modern JS), applying a legacy-compatibility shim for the documented ExtendScript differences when the stored setting is Legacy ExtendScript.

## Undo, history and memory purge

### AE-PRJ-090 · Undo and redo (撤消 / 重做)

- **What it does:** Steps backward and forward through changes to project data, one named action at a time.
- **Exact semantics:**
  - Edit > Undo <action name> (Ctrl/Cmd+Z), Edit > Redo <action name> (Ctrl+Shift+Z / Cmd+Shift+Z). The menu item shows the action name (e.g. "Undo Change Position", "Undo Import").
  - Depth: up to 99 most recent actions. Older AE had Preferences > General > Levels of Undo (CS4-era default 32, max 99); the field was removed from the dialog in AE 15.0 (Oct 2017) and the value is 99 unless edited in the preferences text file (source: Adobe community answer; UNVERIFIED for AE 26/27).
  - Any new undoable action after an undo discards the redo stack.
  - One user gesture = one undo step: a whole drag (scrub) is one step; a multi-layer operation is one step; a script wrapped in `app.beginUndoGroup(name)` / `app.endUndoGroup()` is one step named `name`.
  - Undo history is per session: it is cleared by opening/closing/reverting a project and is not saved in the file. Saving does not clear it.
  - Cached frames are kept across edits and reused after undo/redo (global RAM cache).
- **Priority:** P0
- **Owner:** bae-model (+ apps/bae)
- **Notes:** BAE decision (ARCHITECTURE 4.5): every Command execution is one undo unit and carries an `author`; undo/redo is author-scoped (a user undoes only their own Commands, never collaborators' or another agent's), implemented as inverse Ops applied on top of the CRDT state. BAE may keep a deeper (unbounded or memory-bounded) history; 99 is AE's limit, not a requirement.

### AE-PRJ-091 · Undoable scope (可撤消的操作（译名待核）)

- **What it does:** Defines which actions enter the undo stack.
- **Exact semantics:**
  | Undoable (project data) | Not undoable |
  |---|---|
  | property values, keyframes, expressions | panel scrolling, zoom/pan of viewers, viewer resolution/magnification |
  | layer create/delete/reorder/trim/switches | tool activation, panel/workspace layout |
  | items: import, delete, rename, move to folder, interpretation, proxy set, solid settings | preferences, keyboard shortcuts |
  | comp settings, work area, markers | CTI (current time) moves; layer/item/keyframe selection (UNVERIFIED: some selection changes are recorded as part of other actions) |
  | project settings | Revert (cannot be undone), Purge, Save |
  | render queue add/remove/settings (UNVERIFIED) | rendering itself; Reload Footage (UNVERIFIED) |
  - Locking a layer prevents accidental modification but the lock switch itself is undoable.
- **Priority:** P0
- **Owner:** bae-model
- **Notes:** BAE decision: view state (zoom, CTI, selection, solo-for-preview) lives in BAE's non-persisted `session` sublayer (ARCHITECTURE 4.3) and therefore never enters undo; selection changes may optionally be restored together with the undone Command for usability.

### AE-PRJ-092 · Edit > History (历史记录)

- **What it does:** Lists recent undoable actions so the user can jump back several steps at once.
- **Exact semantics:** Edit > History submenu lists recent actions (most recent first); choosing one undoes that action and every action after it (redo remains available for them). AE has no History panel. UNVERIFIED: number of entries listed (believed the full undo stack, up to 99).
- **Priority:** P1
- **Owner:** bae-ui-motion (+ bae-model)
- **Notes:** BAE decision: BAE adds a History panel listing Commands with author, time and affected Prims, filterable by author; jumping is author-scoped like undo (ARCHITECTURE 4.5). Long-term versions/snapshots are a separate feature (ARCHITECTURE 7).

### AE-PRJ-093 · Purge (清理)

- **What it does:** Frees caches and the undo stack on demand.
- **Exact semantics:** Edit > Purge (清理) submenu, AE 26 order:
  | Command (中文) | Effect |
  |---|---|
  | All Cache (所有缓存（译名待核）) | all RAM caches + disk cache + 3D cache (UNVERIFIED) |
  | All Memory & Disk Cache (所有内存和磁盘缓存) | RAM caches and the disk cache (same as Empty Disk Cache in Media & Disk Cache preferences); not other AE versions' caches |
  | All Memory (所有内存) | all RAM caches (rendered frames, image buffers) and the undo stack; Alt/Option + All Memory also forces resynchronisation of the project database |
  | All Disk Cache (所有磁盘缓存（译名待核）) | disk cache only |
  | All 3D Cache (所有 3D 缓存（译名待核）) | Advanced 3D renderer caches |
  | Undo (撤消) | clears the undo (and redo) stack |
  | Image Cache Memory (图像缓存内存) | RAM frame cache only |
  | Snapshot (快照) | clears the stored viewer snapshots |
  - Purge is not undoable; it never changes project data.
- **Priority:** P1
- **Owner:** apps/bae (+ bae-render, bae-model)
- **Notes:** UNVERIFIED: which items exactly All Memory clears in AE 26 (believed: undo + image caches). Cache behaviour: see ui-preview-render-io.md, preview & cache. BAE decision: provide the same commands; "Undo" purge clears only the local author's stacks.

## Dependencies and packaging

### AE-PRJ-094 · Collect Files (收集文件)

- **What it does:** Copies the project and the source files it needs into one new folder, with a report of files, effects and fonts, so the project can be archived, moved or rendered elsewhere.
- **Exact semantics:** File > Dependencies > Collect Files (文件 > 依赖项 > 收集文件). AE asks to save the project first if it is dirty. Dialog fields in UI order:
  | Parameter (中文) | Type | Default | Values / Notes |
  |---|---|---|---|
  | Collect Source Files (收集源文件) | enum | UNVERIFIED: All | All (全部): all footage incl. unused and proxies / For All Comps (对于所有合成): footage + proxies used by any comp / For Selected Comps (对于选定合成): used by comps selected in the Project panel (directly or nested) / For Queued Comps (对于已加入队列的合成): used directly or indirectly by Queued render items / None (Project Only) (无（仅项目）): copy project only |
  | Generate Report Only (仅生成报告) | bool | off | write the report, copy nothing |
  | Obey Proxy Settings (服从代理设置) | bool | off | on: copy only what the current proxy settings use (proxy or original); off: copy both proxies and originals. For Queued Comps uses the render settings' Proxy Use instead |
  | Reduce Project (减少项目) | bool | off | only with For All / Selected / Queued Comps: the collected project has unused items removed (as AE-PRJ-095) |
  | Change render output to <name> folder (将渲染输出更改为 … 文件夹) | bool + string | off; folder name UNVERIFIED | re-targets output modules with valid status (Queued, Unqueued, Will Continue) to a folder inside the collection |
  | Enable 'Watch Folder' render (启用"监视文件夹"渲染) | bool | off | writes `<project name>_RCF.txt` render control file (AE-PRJ-098) |
  | Maximum Number of Machines (计算机的最大数目) | int | UNVERIFIED: 10 | only with Watch Folder; number of render engines allowed; dialog shows how many items render multi-machine |
  | Comments (注释) | button/text | empty | appended at the end of the report |
  | Info text | read-only | — | "N file(s) (size) will be collected" summary |
  - Output: the user names a new folder (default "<project name> folder", UNVERIFIED) that receives:
    ```
    <name> folder/
      <project name>.aep               copy whose footage paths point into (Footage)
      <project name>Report.txt         report
      (Footage)/                       copies of footage; sub-folders mirror the Project panel folder tree
      (<output folder>)/               only with "Change render output to"
      <project name>_RCF.txt           only with Enable Watch Folder render
    ```
    Folder names in parentheses tell watch-folder render engines not to search them. Image sequences are copied with all their frames (sequence frames into their own sub-folder, UNVERIFIED). Solids and placeholders have no files. Missing files are listed, not copied.
  - Report contents: collection date, project/collected-project names, collection options, number and total size of collected files, list of missing footage (with last known paths), effects used (name, count, and which layers/comps; UNVERIFIED detail level), fonts used (PostScript names), render settings / output modules for queued items when relevant, and the user's comments. UNVERIFIED: exact layout and fields.
  - The open project is unchanged and remains open (later edits are not in the collected copy).
- **Priority:** P1
- **Owner:** bae-package (+ bae-model, bae-ui-motion)
- **Notes:** BAE decision (ARCHITECTURE 16): BAE offers both "Collect files to folder" (AE-style directory, paths rewritten relative) and a single-file `.baez` zip package (media stored uncompressed, project data compressed, zip64), with options: only used assets, optional trimming of video to used ranges (original media and project untouched), only used sequence frames, optional font embedding (licence permitting), and a report of missing assets, sizes and trim results. Fonts and effects are not collected by AE; BAE may embed fonts.

### AE-PRJ-095 · Reduce Project (精简项目)

- **What it does:** Deletes every comp not selected and not nested inside a selected comp, plus all footage they no longer need.
- **Exact semantics:**
  ```
  keep = selected comps (Project panel) ∪ all comps/footage reachable from them through layer sources
         (precomp layers, footage layers, track-matte/source layers), recursively;
         reachability includes layers whose Video/Audio switches are off
  delete every item not in keep (comps and footage; folders are kept? UNVERIFIED: empty folders are removed)
  ```
  - Available only when the Project panel is active and at least one comp is selected.
  - Expressions that refer to a non-subordinate comp: that comp is deleted and the expression breaks (Adobe: "removes the nonsubordinate composition and the applied expression"); AE shows a message afterwards reminding that the command can be undone. Workaround: nest the referenced comp into the referencing comp with its switches off.
  - Shows a count of removed items; undoable as one step. UNVERIFIED message text.
  - Render-queue items of deleted comps are removed.
- **Priority:** P1
- **Owner:** bae-package (+ bae-model)
- **Notes:** Scripting `Project.reduceProject(array_of_items)` returns the number of removed items. BAE decision: offer an option (default on) that also treats expression references (`comp("...")`, `footage("...")`) as reachability edges, avoiding AE's silent expression breakage; the report lists any items kept only because of expressions.

### AE-PRJ-096 · Remove Unused Footage (移除未使用素材)

- **What it does:** Deletes every footage item that no comp uses.
- **Exact semantics:** File > Dependencies > Remove Unused Footage: deletes footage items (files, solids, placeholders) whose `usedIn` is empty; comps are never removed even if unused; footage used only as a proxy does not exist as an item. Shows the number of items removed (e.g. "N unused footage items removed"); undoable. Footage referenced only by expressions (e.g. data files read by `footage()`) is unused and gets removed. UNVERIFIED: message text; whether a footage item used as a layer in an unused comp counts as used (yes: any comp counts).
- **Priority:** P1
- **Owner:** bae-model (+ bae-package)
- **Notes:** Scripting `Project.removeUnusedFootage()` returns the count. BAE decision: treat expression `footage("name")` references as uses.

### AE-PRJ-097 · Consolidate All Footage (整合所有素材)

- **What it does:** Merges duplicate footage items that reference the same source with identical interpretation.
- **Exact semantics:** File > Dependencies > Consolidate All Footage: for each group of footage items with the same source file (same path; for sequences same file set) and identical Interpret Footage settings, keeps one item and retargets every layer that used another member to it, then deletes the others. Solids are consolidated when colour, size and PAR match (UNVERIFIED). Items with different interpretation are not merged. Shows the count; undoable. Which member survives: UNVERIFIED (believed the first in project order).
- **Priority:** P2
- **Owner:** bae-model (+ bae-package)
- **Notes:** Scripting `Project.consolidateFootage()` returns the count.

### AE-PRJ-098 · Watch Folder rendering (监视文件夹)

- **What it does:** Lets one or more AE instances or render engines watch a folder and render projects collected into it.
- **Exact semantics:** File > Watch Folder starts watch mode on a chosen folder (also: a project named `Watch This Folder.aep` puts AE in watch mode for its folder; Windows `afterfx.exe -wf <folder>`; `-re` for render engine). AE renders each project that has a render control file `<project>_RCF.txt` (written by Collect Files with Enable Watch Folder render), in alphabetical order, splitting image-sequence renders across up to Maximum Number of Machines. Fonts, effects and codecs must be installed on every machine; movie files cannot be split across machines. Folders in parentheses are not searched.
- **Priority:** P2
- **Owner:** apps/bae (+ bae-package)
- **Notes:** Headless rendering (`aerender`) is specified in ui-preview-render-io.md. BAE decision: watch-folder rendering is out of M1; a `bae render` CLI plus an external queue covers the use case.
