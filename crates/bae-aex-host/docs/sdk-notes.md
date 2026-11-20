# After Effects effect plug-in SDK — research notes (appendix to `docs/spec/aex-compat.md`)

This is the detailed, English research appendix of workstream 20. The design
document is the Chinese spec `docs/spec/aex-compat.md`; it summarises every
section below and is authoritative for the mapping table, the reservation
requests, the performance analysis and the hosting model. These notes keep the
per-item detail a P3 implementer needs: one row per selector, flag, param type,
suite and event, with a citation for every behavioural claim.

Section numbers are those of the original single-document draft and are kept
so that cross references stay valid. Sections that moved into the Chinese spec:

| reference in these notes | now in `docs/spec/aex-compat.md` |
|---|---|
| §0, §1 (scope, principles) | §0, §1 |
| §17 (capability mapping) | §4 |
| §18 (reservation requests `Rxx-nn`) | §5 (same ids) |
| §19 (performance) | §6 |
| §20 (hosting model) | §2.4 |
| §21 (legal checklist `L-nn`) | §7 (Chinese summary); the full English checklist with evidence links is kept below as §21 |
| §22 (skeleton crate) | §8 |

Request ids `Rxx-nn` (two digits) are 20's; 19's requests in
`docs/spec/plugin-architecture.md` §23 use one digit (`R04-1`) and are written
`19:R04-1` in the Chinese spec.

**Sources and citations.** All behavioural claims cite the public *After
Effects C++ Plug-in SDK Guide* as published at
<https://ae-plugins.docsforadobe.dev/> (community-hosted edition of Adobe's SDK
guide; content © Adobe), in the form
`[SDK: Section › Page](https://ae-plugins.docsforadobe.dev/<section>/<page>/)`.
Other public sources (Apple, Microsoft, vendor documentation, court decisions)
are linked directly. Statements marked *(inference)* are our engineering
conclusions, not documented behaviour.

**What this document deliberately does not contain.** No SDK header text, no
struct definitions, no sample code, and **no numeric ABI values** (selector
numbers, flag bit positions, error-code values, suite name strings). AE
identifiers such as `PF_Cmd_SMART_RENDER` or `PF_OutFlag2_SUPPORTS_THREADED_RENDERING`
appear only as names that cross-reference the public guide. See §21 before
adding any ABI detail anywhere in the repository.

**Words.** *Host* = the future BAE compatibility host. *Plug-in* / *effect* =
a third-party AE effect binary. *Module* = one plug-in binary, which may
contain several effects. *Instance* = one application of an effect to a layer
(an `Effect` Prim in BAE). *World* = an AE pixel buffer (`PF_EffectWorld`).
*MUST / SHOULD / MAY* are used in the RFC 2119 sense for host obligations.
Request ids `R<workstream>-<n>` refer to §18.

## 2. Effect plug-in anatomy, PiPL and capability flags

This section describes what an AE effect binary *is*, how a host finds out what it can do,
and how it reports success, failure and capabilities. It covers the binary forms and code
slices, the single entry point, the two ways a plug-in can register itself, every PiPL
property, the version fields, every documented `PF_OutFlag_*` / `PF_OutFlag2_*` capability
flag, the error model, and the Premiere Pro precedent for what a non-AE host may leave out.
Selector semantics, parameters, worlds, SmartFX, sequence data, GPU and custom UI have their
own sections. This section names them only where a flag points to them.

Column "BAE" names the workstream that owns the concern: 01 (bae-gpu/platform), 02
(doc-model), 04 (render graph), 06 (color), 11/12 (UI), 15 (AE project import), 19 (plug-in
system). Requests that need to be reserved now are collected in §18.

### 2.1 Binary forms, discovery and code slices

| Item | What AE / the plug-in does | Host obligation | BAE | P3 risk |
|---|---|---|---|---|
| Container | Plug-ins are written in C or C++. On macOS they are bundle packages and on Windows they are DLLs. Both platforms **must** carry a PiPL resource [SDK: Introduction › What Can I Do With This SDK?](https://ae-plugins.docsforadobe.dev/intro/what-can-i-do/). | MUST accept Windows DLLs (`.aex`) and macOS `.plugin` bundles. MUST read the PiPL without running plug-in code (§2.3). | 19 | Bundle layout and the PiPL binary encoding are defined in SDK headers and build tools, not in the guide (§21). |
| Byte order | PiPL properties are always stored in macOS byte order, on Windows too. On Windows a `.r` source is converted to a `.rc` by Adobe tools (`pipltool`/`cnvtpipl`) [SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/). | The parser MUST decode PiPL data as big-endian on both platforms (inference: "macOS-specific byte order" means the classic big-endian resource order). | 19 | Endianness bugs show up only with real binaries, so P3 needs an Adobe-free test corpus. |
| Code slices: Apple Silicon | The PiPL names a separate Apple Silicon entry point next to the Intel Mac one. The two names may differ. Universal binaries carry both slices [SDK: Introduction › Apple Silicon Support](https://ae-plugins.docsforadobe.dev/intro/apple-silicon-support/). | MUST choose the entry-point descriptor for the running architecture (`CodeMacARM64` or `CodeMacIntel64`). MUST NOT assume both names are equal. | 19, 01 | Native Adobe hosts only make plug-ins with a native slice available [same page](https://ae-plugins.docsforadobe.dev/intro/apple-silicon-support/). Running an Intel-only plug-in under Rosetta in a helper process would be a BAE extension (inference). |
| Code slices: Windows on Arm | `CodeWinARM64` sits next to `CodeWin64X86`, and the names may differ. Native ARM64 Adobe apps only load native ARM64 effects [SDK: Introduction › Windows on Arm Support](https://ae-plugins.docsforadobe.dev/intro/windows-on-arm-support/). | MUST choose the descriptor for the running architecture. The guide does not mention ARM64EC. | 19, 01 | A WoA build of BAE needs an x64 helper process for x64-only plug-ins (inference, §20). |
| Exported symbols | AE requires only the entry point to be exported. Plug-ins that exported everything, including their own Boost, caused ODR clashes, hangs and crashes [SDK: Introduction › Exporting Symbols in Effects](https://ae-plugins.docsforadobe.dev/intro/symbol-export/). | MUST look up only the declared entry symbol. SHOULD load modules with local symbol scope so a plug-in's symbols cannot interpose on the host's or on another plug-in's (inference). | 19, 01 | Rust host code is not affected by C++ ODR clashes, but two plug-ins in the same helper process can still clash. This is a reason to group plug-ins per process (§20). |
| Install locations | The common folder (shared with Premiere Pro) is the default. An AE-only folder also exists. Both are found through the registry on Windows and fixed paths on macOS [SDK: Introduction › Where Installers Should Put Plug-ins](https://ae-plugins.docsforadobe.dev/intro/where-installers-should-put-plug-ins/). | SHOULD scan the same folders, read-only, and SHOULD let users add extra roots (inference). | 19 | Scanning Adobe's install paths is an interoperability choice that legal should confirm (§21). |
| Scan rules | AE descends up to 10 directory levels. It follows macOS aliases but not Windows shortcuts. It skips folders whose names end in parentheses or start with `¬` (macOS) or `~` (Windows) [same page](https://ae-plugins.docsforadobe.dev/intro/where-installers-should-put-plug-ins/). | SHOULD follow the same rules, because users rely on them to disable plug-ins. | 19 | — |
| macOS code signing | macOS 15+ refuses to load unsigned plug-ins. Changing a bundle after signing breaks loading [SDK: Introduction › Debugging Plug-ins](https://ae-plugins.docsforadobe.dev/intro/debugging-plug-ins/). | The helper process that loads third-party code MUST be signed so it can load libraries signed by other teams (an exemption from library validation, inference from Apple's hardened-runtime rules). The main BAE app SHOULD NOT need that exemption. | 01 | Notarisation and entitlement policy; this is a request to 01 (§18). |
| Several plug-ins in one file | Several PiPLs (effects and AEGPs) may share a file, and AEGPs must come first. No other host supports several effect PiPLs in one binary, Premiere Pro included [SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/). | MUST model one module → N descriptors. MAY host only the first effect PiPL at first (Premiere precedent, §2.11). MUST skip non-effect PiPLs. | 19 | The descriptor model must be 1:N from the start (crate discrepancy, §2.12). |
| Unloading | `PF_Cmd_GLOBAL_SETDOWN` does not mean the module is leaving memory, so plug-ins use OS-specific entry points for that [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). AE never unloads SmartFX plug-ins [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | SHOULD never unload a module during a session after first load (inference: unloading invites use-after-free in plug-ins that rely on AE's behaviour). Hot reload means restarting the helper process. | 19 | Conflicts with BAE's "load/unload plug-ins at runtime" goal (ARCHITECTURE §14). The compat host achieves it by restarting processes. |

### 2.2 The entry point (`EffectMain`) contract

All communication starts from the host, through one entry function per effect. Its name is
whatever the PiPL declares (the samples use `EffectMain`). Before every call the host
refreshes `PF_InData` and the parameter array (with documented exceptions). After the call
it reads `PF_OutData` and, when appropriate, the output world
[SDK: Effect Basics › Entry Point](https://ae-plugins.docsforadobe.dev/effect-basics/entry-point/).
The entry point always uses the C calling convention
[SDK: Introduction › Apple Silicon Support](https://ae-plugins.docsforadobe.dev/intro/apple-silicon-support/).

| # | Argument | Role (per guide) | Valid when | Owner / lifetime | Host obligation |
|---|---|---|---|---|---|
| 1 | `cmd` | The command selector, which says what the plug-in must do [Entry Point](https://ae-plugins.docsforadobe.dev/effect-basics/entry-point/) | Always | Value | MUST send only selectors permitted by the lifecycle and by the flags (e.g. `PF_Cmd_DO_DIALOG` only with `PF_OutFlag_I_DO_DIALOG`) [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). |
| 2 | `in_data` | Application state and the data to act on, plus pointers to the interaction callbacks (`inter`), the graphics utilities (`utils`, "defined at all times"), `effect_ref`, and the PICA basic suite for getting suites [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/) | Always (some fields are valid only for certain selectors) | Host-owned, valid for the duration of the call | MUST fill a fresh, consistent structure before every call. MUST keep callback tables valid while the module is loaded (inference). `effect_ref` identifies the instance in callbacks. |
| 3 | `out_data` | Where the plug-in passes information back: version, flags, handles, output size and origin, `return_msg` [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) | Always | Host-owned structure. The plug-in writes fields. Handles stored in it are allocated through the host. | MUST read after every call. MUST check and clear `return_msg` after every selector (§2.10). |
| 4 | `params` | The parameter array at `in_data->current_time`. `params[0]` is the input layer [Entry Point](https://ae-plugins.docsforadobe.dev/effect-basics/entry-point/) | Only during certain selectors. SmartFX pre-render and render get no array and check parameters out instead [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/) | Host-owned | MUST pass the input layer at index 0. MUST pass null or invalid only where the guide allows it. |
| 5 | `output` | The output world (`PF_LayerDef` / `PF_EffectWorld`) | Only during certain selectors, chiefly `PF_Cmd_RENDER` | Host-owned pixels. The plug-in writes into them. | MUST allocate it at the size negotiated in `PF_Cmd_FRAME_SETUP` (legacy path). |
| 6 | `extra` | Depends on the selector. For `PF_Cmd_EVENT` it depends on the event type. Mostly used for events and parameter supervision [Entry Point](https://ae-plugins.docsforadobe.dev/effect-basics/entry-point/). SmartFX, GPU, arbitrary-data and external-dependency selectors put their I/O structures here [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | Selector-specific | Host-owned unless documented otherwise (e.g. the plug-in owns and must release the GPU device `gpu_data` pointer) | MUST build the right `extra` shape for each selector. |

**Return value.** Every call returns a `PF_Err`. Plug-ins must pass back every error they did
not handle, including errors returned by host callbacks and suites
[SDK: Effect Basics › Errors](https://ae-plugins.docsforadobe.dev/effect-basics/errors/).
The host MUST treat any non-success return as "the output of this call is invalid" (§2.10).

**Memory ownership.**
- The plug-in allocates `global_data`, `sequence_data` and `frame_data` as **host handles**,
  using AE's allocation functions
  [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/).
- `flat_sdata_size` is unused because the host already knows the size of handles it
  allocated [same page](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/).
- The host locks handles before each call and unlocks them afterwards
  [SDK: Effect Details › Memory Allocation](https://ae-plugins.docsforadobe.dev/effect-details/memory-allocation/).
- Plug-ins are asked to make large allocations through the host, so the host can free cache
  memory under pressure
  [same page](https://ae-plugins.docsforadobe.dev/effect-details/memory-allocation/).
- The host therefore MUST run a handle allocator that records each handle's size and can
  purge caches when an allocation fails (04/19, inference).

**Threads.**
- `PF_Cmd_GLOBAL_SETUP` and `PF_Cmd_GLOBAL_SETDOWN` run only on the main thread, and never
  at the same time as any other selector
  [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/).
- Since AE CC 2015 (13.5), UI selectors run on the UI thread and render selectors run on a
  render thread that works on its own copy of the project
  [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/).
- The host MUST give the same guarantees. Concurrency levels are in §2.8 (threading rows).

### 2.3 Registration: PiPL resource vs code-exported registration

| Aspect | PiPL resource | Code-exported registration (`PluginDataEntryFunction`) |
|---|---|---|
| What it is | A property list read **without executing** the plug-in. It came from Photoshop and has "largely been supplanted" by `PF_Cmd_GLOBAL_SETUP` and dynamic out-flags, but must still agree with them [SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/). | A function the plug-in exports. It registers basic information with the host at runtime, without relying on a PiPL, and can register several entry points in one binary. Premiere Pro was the first host to support it, and AE "will support this in a future release". The samples still keep a PiPL for backward compatibility [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). |
| Inputs from the host | None (static data). | An opaque registration context, a registration callback, the PICA basic suite pointer, and the **host name and host version strings** [SDK: Introduction › Exporting Symbols in Effects](https://ae-plugins.docsforadobe.dev/intro/symbol-export/). |
| What the plug-in supplies | All properties in §2.4. | Through the registration macro, per effect: display name, match name, category and reserved info [same page](https://ae-plugins.docsforadobe.dev/intro/symbol-export/). The newer `PF_REGISTER_EFFECT_EXT3` form uses "version 3 of the plug-in entry point" and adds search keywords and a description [SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/). The guide does not document the intermediate variant(s). |
| Cost / safety | Cheap, needs no sandbox, works across architectures (a Mac host can read a Windows PiPL). | Runs plug-in code, so it MUST only run inside the helper process (§20). |
| Is it required? | AE requires a PiPL on both platforms [SDK: Introduction › What Can I Do With This SDK?](https://ae-plugins.docsforadobe.dev/intro/what-can-i-do/). | Optional. |

**Host algorithm (P3).**
1. For each candidate binary, parse every PiPL. Keep the effect-kind entries and skip AEGPs.
2. If no effect PiPL exists and the binary exports the registration function, call it once
   in a helper process. Pass BAE's own host name and version (an open question, §2.6), and
   collect one descriptor per registered effect.
3. Treat a descriptor obtained either way as **provisional**. Capabilities and version only
   become authoritative after `PF_Cmd_GLOBAL_SETUP` (§2.5). The registration macro arguments
   the guide shows carry no out-flags and no effect version, so GLOBAL_SETUP is needed in
   any case (inference).
4. Cache the result per (path, size, mtime, content hash, architecture), following the
   Premiere Pro precedent. Premiere caches the PiPL and GLOBAL_SETUP capabilities across
   launches, and lets a plug-in opt out through `PF_SetNoCacheOnLoad()` in a Premiere-only
   suite [SDK: Premiere Pro › Plug-Ins... Reloaded](https://ae-plugins.docsforadobe.dev/ppro/plug-ins-reloaded/).
   BAE needs the cache so that start-up never loads third-party code (performance first).

### 2.4 PiPL properties the host must parse

Source for every row: [SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/), unless the row gives another link.

| Property | Meaning | Host MUST / SHOULD / MAY | BAE |
|---|---|---|---|
| Kind | The plug-in type. `AEEffect` marks an effect. | MUST host only effect kinds. MUST ignore AEGP, AEIO and artisan kinds. Premiere Pro tries to load AEGPs, and those detect the wrong host by probing for an AE-only suite [SDK: Premiere Pro › Unsupported Features](https://ae-plugins.docsforadobe.dev/ppro/unsupported-features/). BAE SHOULD NOT even try. | 19 |
| Name | The display name, up to 47 characters (31 before AE CC 12.0 [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). It may change dynamically, unlike the match name. | MUST show it in the effect browser. MUST NOT use it as an identity. SHOULD store the raw bytes plus an encoding hint: non-UTF-8 strings use the locale's multibyte encoding [SDK: Introduction › Localization](https://ae-plugins.docsforadobe.dev/intro/localization/). | 11/12, 19 |
| Category | The Effects menu and Effects & Presets category [SDK: Introduction › What Can I Do With This SDK?](https://ae-plugins.docsforadobe.dev/intro/what-can-i-do/). | SHOULD group the effect under it in BAE's browser. | 11/12 |
| Entry point, one per platform | Exports the entry point. A multi-platform plug-in declares several descriptors. No `.def` file or manual exports are needed. Named architectures: `CodeMacIntel64`, `CodeMacARM64` [Apple Silicon](https://ae-plugins.docsforadobe.dev/intro/apple-silicon-support/), `CodeWin64X86`, `CodeWinARM64` [Windows on Arm](https://ae-plugins.docsforadobe.dev/intro/windows-on-arm-support/). | MUST pick the descriptor that matches the host process architecture. MUST ignore descriptors for other or legacy architectures (inference: old binaries may carry obsolete ones). Missing descriptor means "not hostable natively" (§2.1). | 19, 01 |
| `AE_PiPL_Version` | Unused. | MAY parse. MUST NOT depend on it. | — |
| `AE_Effect_Spec_Version` | The SDK (effect API) version the effect was built with. | MUST record it. MUST gate behaviour that AE gates on it: Multi-Frame Rendering requires the plug-in to report at least the March 2021 SDK version [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). SHOULD use it to choose fallbacks. | 19, 04 |
| `AE_Effect_Version` | The effect's own version. It must equal `out_data->my_version` [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | MUST decode it (§2.6). MUST use it to choose between duplicate installs. MUST fold it into the cache key (§2.6). | 19, 04, 02 |
| `AE_Effect_Info_Flags` | Unused. | MAY parse. MUST NOT depend on it. | — |
| `AE_Effect_Global_OutFlags` | Must match the `out_flags` set at GLOBAL_SETUP. | MUST parse. Used before loading (browser badges) and as the authority when `PF_OutFlag_PiPL_OVERRIDES_OUTDATA_OUTFLAGS` is set (§2.5). | 19, 04 |
| `AE_Effect_Global_OutFlags_2` | Must match the `out_flags2` set at GLOBAL_SETUP. | As above. | 19, 04 |
| `AE_Effect_Match_Name` | A unique, constant identifier. The display name can change, the match name cannot. Match names are 7-bit ASCII, with a maximum length given by a header constant (`AEGP_MAX_EFFECT_MATCH_NAME_SIZE`) [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/). Unknown effects whose match name contains the Adobe `ADBE ` prefix are excluded from GPU rendering [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | MUST use it as the persistent identity of the effect type, compared byte for byte with no case folding or normalisation (inference). MUST reject or quarantine non-ASCII names. SHOULD treat `ADBE `-prefixed names as Adobe first-party (15's native mapping table, never the compat host). | 02, 15, 19 |
| `AE_Reserved_Info` | Unused. | MAY parse. | — |
| `AE_Effect_Support_URL` | New in AE 23.5. A URL shown in the Effects Manager, for more information or updates. | SHOULD show it as a link in the effect's info UI. MUST treat it as untrusted. | 11/12 |
| `AE_Effect_Search_Keywords` | New in the 26.5 SDK, Premiere Pro Beta 27.0 only. Locale-keyed JSON of search terms. Searches use `en_US` plus the user's locale, and malformed JSON is ignored. | MAY use it for browser search. MUST ignore malformed JSON. | 11/12 |
| `AE_Effect_Description` | Premiere Pro Beta only. A short description shown in the Effects Manager. Non-ASCII text uses the system code page. | MAY show it. MUST decode it with the system code page. | 11/12 |

Properties that are unknown or newer than the host MUST be skipped, not treated as errors
(inference: the property list keeps growing, as the 23.5 and 26.5 additions show).

### 2.5 Consistency rules between PiPL and `PF_Cmd_GLOBAL_SETUP`

| Rule (per guide) | Host behaviour |
|---|---|
| The behaviours a plug-in declares at GLOBAL_SETUP must agree with its PiPL [SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/). Plug-ins are told to update both whenever they change a flag [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags). | Treat GLOBAL_SETUP as authoritative, except as in the next row. On a mismatch, log a diagnostic and refresh the cached descriptor. The guide does **not** say what AE does on a mismatch (open question). |
| With `PF_OutFlag_PiPL_OVERRIDES_OUTDATA_OUTFLAGS`, AE uses the PiPL flags and ignores those set at GLOBAL_SETUP [PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags). | MUST apply the PiPL words when this flag is present (checked in the PiPL word). |
| Flags that must appear in **both** places: `PF_OutFlag2_SUPPORTS_QUERY_DYNAMIC_FLAGS` and `PF_OutFlag2_PARAM_GROUP_START_COLLAPSED_FLAG` [PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags2); `PF_OutFlag_SEND_UPDATE_PARAMS_UI` [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/); `PF_OutFlag_USE_OUTPUT_EXTENT` [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/); `PF_OutFlag2_SUPPORTS_DIRECTX_RENDERING` (otherwise CPU fallback) [SDK: Introduction › Building GPU Effects](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/); `PF_OutFlag2_STRINGS_ARE_UTF8` [SDK: Introduction › Localization](https://ae-plugins.docsforadobe.dev/intro/localization/). | MUST require these flags in both words (logical AND) before enabling the behaviour (inference: conservative, and it matches the "must be set in both" wording). |
| `out_data->my_version` must equal the PiPL effect version. A plug-in built with an old SDK and a large major version breaks the PiPL version check, which "is not supported" [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | MUST compare the *decoded* versions (§2.6). On a mismatch: warn, and prefer the runtime value (inference). |
| `out_data->num_params` is checked against the number of `PF_ADD_PARAM` calls plus the implicit input layer [PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). | MUST check this after `PF_Cmd_PARAM_SETUP` and refuse the instance on a mismatch (inference: AE checks it, and a mismatch means the parameter table is corrupt). |
| Dynamic flags may only change in `PF_Cmd_QUERY_DYNAMIC_FLAGS` if they were set at GLOBAL_SETUP [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | §2.9. |

### 2.6 Versions and host identification

**Effect version (`AE_Effect_Version`, `out_data->my_version`).**
- The plug-in builds this value with the `PF_VERSION` macro, and AE uses it to decide which
  of two duplicate effects to load
  [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/).
- The guide does not document the packing. The components the crate models (major, minor,
  bug-fix, stage, build) follow the macro's parameter list in the public headers. Stage
  ordering develop < alpha < beta < release is an inference.
- **Widened major field.** AE CC 2015 (13.5) widened the major component to a maximum of 127,
  using extra high-order bits that are not contiguous with the old field. Hosts older than
  13.5 see only the original small range, so version 8 reads as version 0. Plug-ins built
  with an older SDK must keep major ≤ 7
  [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/).
- **Decoding.** The host MUST decode with the widened layout, MUST compare decoded values,
  and MUST NOT expose raw words.
- **Cache key.** The effect version and the AE build number are part of AE's disk-cache key,
  so a vendor fixes stale cached frames by bumping the version
  [SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/).
  BAE MUST put (match name, decoded effect version, compat-host build) into every persistent
  cache key of an AE node (04).
- **Project record.** BAE SHOULD record the producing version on the effect Prim, so a
  project opened with another plug-in version can warn (02, inference).

**Spec version vs host API version.** These are two different numbers.
- `AE_Effect_Spec_Version` is the SDK version the plug-in was built with (§2.4).
- `in_data->version` is the **host's** effect API version. Plug-ins read it to run code only
  in specific AE versions
  [SDK: Introduction › Compatibility Across Multiple Versions?](https://ae-plugins.docsforadobe.dev/intro/compatibility-across-multiple-versions/).
  The same page tabulates which effect API version each AE release reports. Premiere Pro
  reports its own sequence of values, which does **not** promise AE-equivalent features
  [SDK: Premiere Pro › Premiere Pro & Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/ppro/).
- BAE MUST report a host API version whose AE semantics it actually implements. Which value
  to report is an open question (§21).
- For more precise checks, plug-ins can run `app.version` through the AEGP `AEGP_ExecuteScript`
  [Compatibility](https://ae-plugins.docsforadobe.dev/intro/compatibility-across-multiple-versions/).
  A host without AEGP support cannot answer that, and such plug-ins will take their fallback
  path (P3 risk).

**Host identity.**
- `in_data->appl_id` carries a four-character creator code that identifies the host. AE and
  Premiere Pro each have their own. Plug-ins use it to detect a licence for one application
  being used in another
  [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/),
  and to avoid partially implemented APIs in other hosts
  [SDK: Introduction › Third-party Plug-in Hosts?](https://ae-plugins.docsforadobe.dev/intro/third-party-plug-in-hosts/).
- The same page notes that, as of writing, no third-party host supports SmartFX or AEGP
  functions.
- `in_data->serial_num` is the invoking application's serial number
  [PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). BAE SHOULD pass a
  neutral value (inference).
- Code-registered plug-ins also receive host name and version strings (§2.3).
- The Portable sample shows how plug-ins check host and version and fall back when a suite
  is missing [SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/).
  This is the mechanism a non-AE host relies on.
- **Policy.** Adobe "neither supports nor recommends" Adobe-compatible third-party hosts, but
  offers to help other hosts with newer features
  [SDK: Premiere Pro › Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/other-hosts/).
  Which `appl_id` BAE presents is a legal and product decision (open question, §21).

### 2.7 `PF_OutFlags` (first flag word): one row per documented flag

"D" marks a flag that may change in `PF_Cmd_QUERY_DYNAMIC_FLAGS`
[SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/).
Rows without their own link are sourced from
[SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags).

| Flag | Meaning | When set | Host obligation | BAE |
|---|---|---|---|---|
| `PF_OutFlag_KEEP_RESOURCE_OPEN` | The plug-in needs its resources during every command. By default they are open only during GLOBAL_SETUP, ABOUT and DO_DIALOG. Adobe recommends copying resources into global data instead. | GLOBAL_SETUP | MUST keep the module's resources reachable all session when set (resource-file path via platform data, inference). On macOS, ABOUT sets the current resource file to the module [Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | 19, 01 |
| `PF_OutFlag_WIDE_TIME_INPUT` | Checks out parameters (including layers) at times other than `current_time`. Without the flag, AE does not invalidate cached frames correctly. Pair it with the automatic variant. | GLOBAL_SETUP, D | MUST allow checkouts at other times. Unless `AUTOMATIC_WIDE_TIME_INPUT` applies, MUST invalidate the node on **any** upstream change at **any** time. | 04 |
| `PF_OutFlag_NON_PARAM_VARY` | Output varies with something other than parameters (e.g. particles, Wave Warp). AE will not reuse output rendered for a still. "Turn this off whenever possible." | GLOBAL_SETUP, D | MUST NOT reuse a frame across times when the input is a still. In Premiere Pro it also decides whether UPDATE_PARAMS_UI/DRAW fire when the playhead moves without keyframes [Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/). | 04, 11/12 |
| `PF_OutFlag_RESERVED6` | Unused. Formerly `PF_OutFlag_SEND_PARAMS_UPDATE`, replaced by `REFRESH_UI`. | — | MUST ignore. | — |
| `PF_OutFlag_SEQUENCE_DATA_NEEDS_FLATTENING` | Sequence data holds pointers or handles and must be flattened for storage. **AE and Premiere Pro both assume the flag is set.** With MFR it must be paired with `SUPPORTS_GET_FLATTENED_SEQUENCE_DATA` [Multi-Frame Rendering](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). | GLOBAL_SETUP | MUST always run the flatten/unflatten protocol, whatever the bit says. MUST store only flat bytes. | 02, 19 |
| `PF_OutFlag_I_DO_DIALOG` | Shows a dialog for DO_DIALOG. The response cannot be undone. | GLOBAL_SETUP, checked at SEQUENCE_SETUP | MUST send DO_DIALOG only if set [Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). SHOULD show an "Options…" button. SHOULD wrap the resulting parameter and state changes in one BAE Command so they *become* undoable (inference). | 11/12, 02 |
| `PF_OutFlag_USE_OUTPUT_EXTENT` | Honours the output `extent_rect`. Obsolete for SmartFX. | GLOBAL_SETUP and PiPL | MAY request that only the extent be filled. Otherwise MUST treat the whole output as written. | 04 |
| `PF_OutFlag_SEND_DO_DIALOG` | The effect needs its dialog to work at all (Photoshop compatibility). AE sends DO_DIALOG after SEQUENCE_SETUP. | Set during SEQUENCE_RESETUP, **not** GLOBAL_SETUP | MUST read it as a per-call result of RESETUP and then send DO_DIALOG once after apply. | 19, 11/12 |
| `PF_OutFlag_DISPLAY_ERROR_MESSAGE` | Show `return_msg` as an error. A non-null `return_msg` is always displayed, and this flag makes the dialog an error dialog. Also used for trial nag messages. | After any command | A per-call status, not a capability: MUST read it with `return_msg` after every call (§2.10). | 19, 11/12 |
| `PF_OutFlag_I_EXPAND_BUFFER` | Output buffer grows beyond the input. Combined with `USE_OUTPUT_EXTENT` the extent intersection applies. Watch `pre_effect_source_origin`. "Drastically reduces caching efficiency." Obsolete for SmartFX. | GLOBAL_SETUP | MUST accept output size and origin from FRAME_SETUP. MUST cope with FRAME_SETUP/SETDOWN being sent twice, once without RENDER, as a visibility probe [Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | 04 |
| `PF_OutFlag_PIX_INDEPENDENT` | Each output pixel depends only on the same input pixel (colour correction yes, distortion no). Can give "dramatic performance improvements". `in_data->field` is valid only when it is set [PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). | GLOBAL_SETUP, D | MAY skip ROI growth and MAY split work into tiles or fields. Tiling legacy effects is a BAE optimisation, not documented AE behaviour (inference). | 04 |
| `PF_OutFlag_I_WRITE_INPUT_BUFFER` | Writes into its input buffer. This saves an allocation but invalidates some pipeline caching. | GLOBAL_SETUP | MUST give the node a private, writable copy of its input, never a cached or shared buffer. | 04 |
| `PF_OutFlag_I_SHRINK_BUFFER` | Shrinks its output to the extent to save memory. Obsolete for SmartFX. | GLOBAL_SETUP | MUST accept a smaller output from FRAME_SETUP. | 04 |
| `PF_OutFlag_WORKS_IN_PLACE` | Unused. | — | MUST ignore. | — |
| `PF_OutFlag_SQUARE_PIX_ONLY` | Unused. | — | MUST ignore. Still supplies the real pixel aspect ratio. | — |
| `PF_OutFlag_CUSTOM_UI` | Has custom UI and wants `PF_Cmd_EVENT`. | GLOBAL_SETUP | If BAE supports custom UI: MUST route ECP and viewer events. Otherwise MAY show standard controls and mark the effect "custom UI unavailable". | 11/12, 19 |
| `PF_OutFlag_RESERVED5` | Unused. | — | MUST ignore. | — |
| `PF_OutFlag_REFRESH_UI` | Redraw the effect controls, composition and layer windows. A finer-grained redraw mechanism is preferred for custom UI. | During EVENT, RENDER, DO_DIALOG | MUST treat it as a redraw request. When it comes from a render thread, MUST post it to the UI thread and SHOULD coalesce repeats (inference). | 11/12 |
| `PF_OutFlag_NOP_RENDER` | Invalidates the current render. Setting width/height to 0 in FRAME_SETUP has the same effect [Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | FRAME_SETUP | MUST skip or discard RENDER for that frame. | 04 |
| `PF_OutFlag_I_USE_SHUTTER_ANGLE` | Rendered images depend on `shutter_angle`. One of the caching-relevant flags [Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/). | GLOBAL_SETUP, D | MUST add shutter angle (and phase, inference) to the node cache key. | 04 |
| `PF_OutFlag_I_USE_AUDIO` | Parameters depend on audio checked out with `PF_CHECKOUT_LAYER_AUDIO`. Meant for visual effects that read audio [SDK: Audio › Global Outflags](https://ae-plugins.docsforadobe.dev/audio/global-outflags/). | GLOBAL_SETUP | Premiere Pro **does not load** such plug-ins [Unsupported Features](https://ae-plugins.docsforadobe.dev/ppro/unsupported-features/). BAE MAY do the same in P3. If supported later: audio checkout plus audio in the cache key. | 04 (later) |
| `PF_OutFlag_I_AM_OBSOLETE` | Usable in old projects but hidden from the effect menu. Premiere Pro supports it since CC 2015.3 [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | GLOBAL_SETUP | MUST keep the effect resolvable by match name on open and import. MUST hide it from the browser. | 19, 11/12, 15 |
| `PF_OutFlag_FORCE_RERENDER` | Asks for a re-render. Since 13.5 it is also *the* trigger that copies UI-side sequence data to the render copy. It works in USER_CHANGED_PARAM, and in CLICK/DRAG only when GET_FLATTENED is implemented. Adobe prefers GUID mix-in, arbitrary data or `PF_ChangeFlag_CHANGED_VALUE`, which keep cache hits after undo [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | Per call (UI selectors) | MUST snapshot the instance state from the UI instance into render instances and invalidate the node. MUST NOT invalidate anything else. | 04, 02 |
| `PF_OutFlag_PiPL_OVERRIDES_OUTDATA_OUTFLAGS` | AE uses the PiPL flags and ignores GLOBAL_SETUP flags. | PiPL | MUST choose the authoritative source accordingly (§2.5). | 19 |
| `PF_OutFlag_I_HAVE_EXTERNAL_DEPENDENCIES` | Depends on an external file or font. AE sends GET_EXTERNAL_DEPENDENCIES, asking for all dependencies or only missing ones [Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | GLOBAL_SETUP | SHOULD query it when packaging a project (ARCHITECTURE §16) and when opening one, to report missing items. | 19 |
| `PF_OutFlag_DEEP_COLOR_AWARE` | Handles 16 bpc. | GLOBAL_SETUP | MAY drive the effect at 16 bpc, with channel range 0..32768 (pixel section). | 04, 06 |
| `PF_OutFlag_SEND_UPDATE_PARAMS_UI` | Wants UPDATE_PARAMS_UI, where only cosmetic changes are allowed and parameters must never be checked out [Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | GLOBAL_SETUP and PiPL | SHOULD send it when the ECP opens and when time moves. MUST reject value changes made during it. | 11/12, 19 |
| `PF_OutFlag_AUDIO_FLOAT_ONLY` | Wants float audio; AE converts. Requires `AUDIO_EFFECT_TOO` or `_ONLY`. | GLOBAL_SETUP | MAY skip (audio is out of scope at first). | — |
| `PF_OutFlag_AUDIO_IIR` | An IIR audio filter: output depends on earlier output. May request earlier input audio, caches its delay line in sequence data. | GLOBAL_SETUP | MAY skip. | — |
| `PF_OutFlag_I_SYNTHESIZE_AUDIO` | Generates audio even from silence. Caching-relevant [Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/). | GLOBAL_SETUP | MAY skip. | — |
| `PF_OutFlag_AUDIO_EFFECT_TOO` | Alters audio as well as video. Every audio effect sets this or `_ONLY` [Audio › Global Outflags](https://ae-plugins.docsforadobe.dev/audio/global-outflags/). | GLOBAL_SETUP | MAY host the video part only, pass audio through, and warn the user (inference). | — |
| `PF_OutFlag_AUDIO_EFFECT_ONLY` | Alters audio only. | GLOBAL_SETUP | MAY leave these out of the browser in P3. | — |

### 2.8 `PF_OutFlags2` (second flag word): one row per documented flag

Rows without their own link are sourced from
[SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags2).
The last five rows are documented only on other pages.

| Flag | Meaning | When set | Host obligation | BAE |
|---|---|---|---|---|
| `PF_OutFlag2_NONE` | Nothing. | — | — | — |
| `PF_OutFlag2_SUPPORTS_QUERY_DYNAMIC_FLAGS` | Responds to QUERY_DYNAMIC_FLAGS. Must be set in the PiPL and at GLOBAL_SETUP. | Both | MUST NOT send QUERY_DYNAMIC_FLAGS otherwise. Without it, static flags are final (§2.9). | 19, 04 |
| `PF_OutFlag2_I_USE_3D_CAMERA` | Reads 3D camera information. Caching-relevant [Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/). In AE, effects read it through a small set of AEGP suites [SDK: AEGPs › Cheating Effect Usage of AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/cheating-effect-usage-of-aegp-suites/). | GLOBAL_SETUP, D | MUST add the active camera to the cache key. MAY leave 3D out at first (Premiere Pro precedent, §2.11). | 04, 02 |
| `PF_OutFlag2_I_USE_3D_LIGHTS` | Reads 3D lights. | GLOBAL_SETUP, D | MUST add the lights to the cache key. MAY leave 3D out at first. | 04, 02 |
| `PF_OutFlag2_PARAM_GROUP_START_COLLAPSED_FLAG` | Makes the per-group `PF_ParamFlag_START_COLLAPSED` take effect. Without it, every group starts collapsed. | PiPL and GLOBAL_SETUP | SHOULD honour it. Premiere Pro ignores it [Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/). | 11/12 |
| `PF_OutFlag2_I_AM_THREADSAFE` | "Currently this does nothing." | — | MUST ignore. MUST NOT read it as MFR-safe. | — |
| `PF_OutFlag2_CAN_COMBINE_WITH_DESTINATION` | Added for Premiere, no longer used. | — | MUST ignore. | — |
| `PF_OutFlag2_DOESNT_NEED_EMPTY_PIXELS` | The host may trim the input to exclude empty pixels: zero alpha, and also zero RGB if `REVEALS_ZERO_ALPHA` is set. The origin goes in `pre_effect_source_origin`. Together with `I_EXPAND_BUFFER` the input may be **null**. Can change the output size. Obsolete for SmartFX. | GLOBAL_SETUP, D | MAY trim (a performance gain). MUST set the origin. MUST allow a null or empty input and a changed output size. | 04 |
| `PF_OutFlag2_REVEALS_ZERO_ALPHA` | Can make zero-alpha pixels' RGB visible, so the host must not trim them. Changes `extent_hint` but not the buffer size. Since AE 6.0, pixels outside the mask bounding box are zeroed. "The one flag implementors need to pay most attention to." | GLOBAL_SETUP, D | MUST keep the RGB of alpha-0 pixels upstream of this node (no alpha-based trimming or zeroing). | 04 |
| `PF_OutFlag2_PRESERVES_FULLY_OPAQUE_PIXELS` | "Preserve those pixels!" with no further detail. | GLOBAL_SETUP | MAY use it as an optimisation hint, assuming output equals input wherever input alpha is fully opaque (inference). MAY ignore it. | 04 |
| `PF_OutFlag2_SUPPORTS_SMART_RENDER` | Uses SmartFX: gets SMART_PRE_RENDER/SMART_RENDER instead of FRAME_SETUP/RENDER/FRAME_SETDOWN [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). Required for float, GPU and automatic wide time. | GLOBAL_SETUP | MUST use the SmartFX path when set. It maps onto 04's ROI negotiation. | 04 |
| `PF_OutFlag2_FLOAT_COLOR_AWARE` | Supports 32 bpc float. Requires `SUPPORTS_SMART_RENDER`. Legacy RENDER is limited to 8/16 bpc [Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | GLOBAL_SETUP | SHOULD prefer 32 bpc, because BAE's working buffers are half-float. MUST ignore the flag without SmartFX. | 04, 06 |
| `PF_OutFlag2_I_USE_COLORSPACE_ENUMERATION` | Premiere Pro colour-space optimisation (see the Premiere Pro SDK). | GLOBAL_SETUP | MAY ignore. | 06 |
| `PF_OutFlag2_I_AM_DEPRECATED` | Puts the effect in the localised "Obsolete" folder. Compare `I_AM_OBSOLETE`. | GLOBAL_SETUP | SHOULD list it under an obsolete group. | 11/12 |
| `PF_OutFlag2_PPRO_DO_NOT_CLONE_SEQUENCE_DATA_FOR_RENDER` | Premiere Pro only, not AE. Adobe advises against it because it causes parameter UI problems [SDK: Premiere Pro › Multithreading](https://ae-plugins.docsforadobe.dev/ppro/multithreading/). | GLOBAL_SETUP | MUST ignore (BAE follows AE semantics). | — |
| `PF_OutFlag2_AUTOMATIC_WIDE_TIME_INPUT` | Requires `WIDE_TIME_INPUT` but effectively replaces it. The host tracks **all parameter checkouts**, so a frame that checked out only earlier times stays valid after later edits. The effect must not cache time-dependent data unless it validates it with `PF_GetCurrentState`/`PF_AreStatesIdentical`. SmartFX only, otherwise silently treated as `WIDE_TIME_INPUT`. | GLOBAL_SETUP | MUST record each render's (parameter or layer, time) checkouts, attach them to the cached frame, and invalidate precisely. MUST give the state-hash functions the same dependency semantics [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/). | 04 |
| `PF_OutFlag2_I_USE_COMP_TIMECODE` | Re-render when the composition start time or drop-frame setting changes. | GLOBAL_SETUP | MUST add comp start timecode and the drop-frame flag to the cache key. | 04, 02 |
| `PF_OutFlag2_DEPENDS_ON_UNREFERENCED_MASKS` | Reads masks that no path parameter references (e.g. stroking all masks). New in CS6. | GLOBAL_SETUP, D | MUST invalidate the node when any mask on the layer changes. | 04, 02 |
| `PF_OutFlag2_OUTPUT_IS_WATERMARKED` | Output is unfit for final use (demo licence). May change in QUERY_DYNAMIC_FLAGS. The next render must match the last state reported, or wrong frames get cached. Since 13.5, GUID mix-in can replace it [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | GLOBAL_SETUP, D | MUST put the watermark state in the cache key. SHOULD warn before final export. | 04, 11/12 |
| `PF_OutFlag2_SUPPORTS_GPU_RENDER_F32` | GPU support (AE 16.0). The effect gets GPU selectors and a GPU badge. At `PF_Cmd_GPU_DEVICE_SETUP`, the flags describe support **per device and framework**. The guide mentions a family of `…_GPU_RENDER_Fxx` flags but names only F32. Effects that set no flag there are CPU-only for that device [Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). Unknown `ADBE ` match names never get GPU [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | GLOBAL_SETUP, then per device | MUST treat GPU as per (device, framework) and always keep a CPU fallback. MUST require SmartFX. | 01, 04, 19 |
| `PF_OutFlag2_SUPPORTS_THREADED_RENDERING` | Safe for Multi-Frame Rendering: one or more instances may render concurrently. Sequence data is read-only at render time, through `PF_EffectSequenceDataSuite1` [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). Effects without the flag force render threads through one at a time and get a warning icon [Multi-Frame Rendering](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). | GLOBAL_SETUP | MUST honour it only for plug-ins whose spec version is at least the March 2021 SDK. MUST provide read-only state at render. Otherwise MUST serialise and SHOULD show a performance badge. | 04, 19, 11/12 |
| `PF_OutFlag2_MUTABLE_RENDER_SEQUENCE_DATA_SLOWER` | Sequence data is replicated per render thread and writable. Changes are discarded after each render span (a RAM preview, a render queue export). | GLOBAL_SETUP | MUST clone the instance state per render thread at span start and drop the clones at span end. | 04, 19, 02 |
| `PF_OutFlag2_STRINGS_ARE_UTF8` | AE 27.0+, AE only. Strings are UTF-8 instead of locale multibyte. Parameter-name capacity does not grow [SDK: Introduction › Localization](https://ae-plugins.docsforadobe.dev/intro/localization/). | GLOBAL_SETUP and PiPL | MUST pick the string codec per plug-in from this flag. Without it, MUST convert using the locale code page. | 11/12, 19 |
| `PF_OutFlag2_SUPPORTS_GET_FLATTENED_SEQUENCE_DATA` | Implements GET_FLATTENED_SEQUENCE_DATA: a flattened copy without disposing the live structures. Required with flattening for MFR, and AE will later enforce it with a load-time warning [Multi-Frame Rendering](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). FORCE_RERENDER in CLICK/DRAG depends on it [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | GLOBAL_SETUP | SHOULD prefer it over FLATTEN for snapshots (save, sync, render copies). | 02, 19 |
| `PF_OutFlag2_I_MIX_GUID_DEPENDENCIES` | SmartFX only. During SMART_PRE_RENDER the effect mixes extra state into the cached frame's GUID with `GuidMixInPtr()` [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | GLOBAL_SETUP | MUST provide the mix-in callback and fold its bytes into the frame cache key. | 04 |
| `PF_OutFlag2_CUSTOM_UI_ASYNC_MANAGER` | Custom UI requests frames asynchronously through the "Async Manager" (e.g. histograms) [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | GLOBAL_SETUP | If custom UI is supported: MUST offer async, cancellable frame requests from the UI thread. | 11/12, 04 |
| `PF_OutFlag2_AE13_5_THREADSAFE` | UI-thread ↔ render-thread safe. Different threads on different project copies may be inside the effect at once, but never on the same instance [What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | GLOBAL_SETUP | MAY overlap UI and render calls on distinct instance copies only when set. Otherwise SHOULD serialise UI and render calls into the module. The guide says nothing about AE's behaviour without the flag (inference). | 19, 04 |
| `PF_OutFlag2_SUPPORTS_DIRECTX_RENDERING` | Needed for DirectX GPU rendering, also in the PiPL. Without it the effect falls back to CPU [SDK: Introduction › Building GPU Effects](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/). | GLOBAL_SETUP and PiPL | MUST require it before choosing the D3D12 route on Windows. | 01, 04 |

### 2.9 Dynamic flags protocol (`PF_Cmd_QUERY_DYNAMIC_FLAGS`)

Source: [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) unless noted.

1. The host sends QUERY_DYNAMIC_FLAGS only to plug-ins that set
   `SUPPORTS_QUERY_DYNAMIC_FLAGS` in the PiPL **and** at GLOBAL_SETUP.
2. The documented dynamic set has ten flags:
   - `WIDE_TIME_INPUT`, `NON_PARAM_VARY`, `PIX_INDEPENDENT`, `I_USE_SHUTTER_ANGLE`
   - `I_USE_3D_CAMERA`, `I_USE_3D_LIGHTS`
   - `DOESNT_NEED_EMPTY_PIXELS`, `REVEALS_ZERO_ALPHA`, `DEPENDS_ON_UNREFERENCED_MASKS`, `OUTPUT_IS_WATERMARKED`

   The host MUST ignore changes to any other flag in this response (inference).
3. A flag may change here only if it was *set* at GLOBAL_SETUP. The host therefore MAY
   assume that a dynamic flag which was off at GLOBAL_SETUP stays off (inference).
4. The selector "will be sent at arbitrary times". The plug-in reads its **non-layer**
   parameters with `PF_CHECKOUT_PARAM` and should answer quickly, because AE uses the answer
   for caching and optimisation. Dynamic flags "can greatly improve performance" by avoiding
   over-eager invalidation
   [SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/).
5. BAE policy (inference):
   - Query after every committed parameter change of the instance, and before planning a
     render whose parameter state has no cached answer.
   - Cache the answer per (instance, parameter-state hash).
   - Bump a **caps generation** on every change and invalidate only that node.
   - Never query during playback scheduling in the hot path when a cached answer exists.
6. Watermark race: the next render must match the last QUERY_DYNAMIC_FLAGS answer
   [PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags2).
   The host MUST tag each render with the caps generation it was planned under, and drop
   results whose generation is stale.

### 2.10 Errors, cancellation, messages and exceptions

Error codes are listed by name only. Their semantics come from
[SDK: Effect Basics › Errors](https://ae-plugins.docsforadobe.dev/effect-basics/errors/).

| Code | Meaning | Direction | Host obligation | BAE |
|---|---|---|---|---|
| `PF_Err_NONE` | Success. | both | Output is usable unless a cancel was signalled during the call (see `PF_Interrupt_CANCEL`). | — |
| `PF_Err_OUT_OF_MEMORY` | Allocation failed. RAM preview causes it on purpose, and AE expects it. AE **never reports** out-of-memory errors. | both | MUST NOT show UI. MUST NOT cache the result. SHOULD purge caches and retry or degrade (inference). Host allocators MUST return it rather than abort. | 04, 19 |
| `PF_Err_INTERNAL_STRUCT_DAMAGED` | A problem using a data structure. | both | Fail the frame and log it. MAY treat the instance as suspect and rebuild it from flattened state (inference). | 19 |
| `PF_Err_INVALID_INDEX` | A problem finding or using an array member. | both | Return it from callbacks for bad parameter or layer indices. | 19 |
| `PF_Err_UNRECOGNIZED_PARAM_TYPE` | A problem with parameter data. | both | Return it when a plug-in registers a parameter kind the host cannot represent, so PARAM_SETUP fails cleanly (inference). | 02, 19 |
| `PF_Err_INVALID_CALLBACK` | A problem calling a function through a pointer. The registration sample uses it as the default result. | both | Return it for callbacks that exist but are not implemented, rather than null pointers (inference). | 19 |
| `PF_Err_BAD_CALLBACK_PARAM` | A bad argument to a callback. | host → plug-in | Validate every callback argument and return it. | 19 |
| `PF_Interrupt_CANCEL` | Effect and AEGP callbacks return it when the user aborts a render. The effect must stop and return it. If it does not, "misrendered frames" may be cached. | host → plug-in, echoed back | MUST return it from `PF_ABORT`/`PF_PROGRESS` and checkouts once the call's cancel token fires. If the call was interrupted, results "will not be used" even when the plug-in reports success [Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), so the host MUST discard output from any call during which it signalled cancel. MUST NOT report it as an error. | 04, 19 |
| `PF_Err_CANNOT_PARSE_KEYFRAME_TEXT` | Returned from `PF_Arbitrary_SCAN_FUNC` when clipboard text cannot be parsed into keyframe data. | plug-in → host | Paste fails without corrupting anything. MAY show a short notice. | 02, 11/12 |
| any other non-zero | Plug-in-specific. | plug-in → host | Fail the call. Keep the raw code for diagnostics only. | 19 |

**Reporting policy** [SDK: Effect Basics › Errors](https://ae-plugins.docsforadobe.dev/effect-basics/errors/).
- A plug-in reports **its own** errors to the user before returning. Errors from the
  operating system during plug-in execution count as the plug-in's.
- Errors returned by host callbacks have **already been reported by the host**. The plug-in
  only passes them back. The host MUST therefore report a callback error once, at the
  source, and MUST NOT report it again when it comes back from the entry point. This needs
  provenance on errors.
- To report, the plug-in sets `PF_OutFlag_DISPLAY_ERROR_MESSAGE` and writes `return_msg`.
  That puts the error into the render log and avoids hangs in renders driven by a render
  engine or scripting.
- AE suppresses error reporting during RAM preview and in `-noui` mode. BAE MUST do the same
  for playback, background and headless renders, sending messages to the render log
  (11/12, 19).
- AE displays any non-null `return_msg`, checks and clears it after every selector, and uses
  an error dialog only when the flag is set
  [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/).
  A message can therefore come with **success**, for example from ABOUT
  [Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)
  or a trial nag. The host MUST model messages separately from errors.

**Exceptions.**
- Plug-ins must handle every exception their own code throws. AE's C APIs do not expect
  exceptions, and AE crashes immediately if one escapes
  [SDK: Introduction › Exceptions](https://ae-plugins.docsforadobe.dev/intro/exceptions/).
- On Apple Silicon, an exception propagating through C frames calls `terminate()`
  [SDK: Introduction › Apple Silicon Support](https://ae-plugins.docsforadobe.dev/intro/apple-silicon-support/).
- Host consequences (inference):
  - Every host callback MUST catch Rust panics at the boundary and turn them into an error
    code. Nothing may unwind into plug-in frames.
  - A plug-in exception or crash takes down whatever process it runs in. This is the main
    reason for out-of-process hosting by default (§20).
  - In-process mode is acceptable only for plug-ins on an allow-list.

### 2.11 Premiere Pro precedent: what a non-AE host may legitimately omit

Premiere Pro is a "thorough host implementation" of the AE effect API. Its main omissions are
3D-related calls, 16-bit and SmartFX, and the AEGP utilities
[SDK: Premiere Pro › Premiere Pro & Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/ppro/).

| Area | Premiere Pro behaviour | BAE P3 stance |
|---|---|---|
| AEGP suites | Not supported [Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/), [Unsupported Features](https://ae-plugins.docsforadobe.dev/ppro/unsupported-features/). | MAY omit. Exception: the few AEGP calls effects legitimately use (camera and lights, parent comp, comp background colour [Cheating Effect Usage of AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/cheating-effect-usage-of-aegp-suites/)) SHOULD be offered when 3D is added. |
| SmartFX | Not supported [Bigger Differences](https://ae-plugins.docsforadobe.dev/ppro/bigger-differences/). Plug-ins are advised to keep the legacy commands for such hosts [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST support. It is AE's only route to 32 bpc and GPU, and it maps directly onto 04's ROI model. |
| AE 16-bit | Not supported [Bigger Differences](https://ae-plugins.docsforadobe.dev/ppro/bigger-differences/). | SHOULD support, since the conversion cost is the same as for 8 bpc. |
| 3D (camera, lights, aux channels) | Omitted [Premiere Pro & Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/ppro/). | MAY omit at first. Flag 3D-dependent plug-ins as "limited". |
| `extent_hint` | Unsupported [Unsupported Features](https://ae-plugins.docsforadobe.dev/ppro/unsupported-features/). | MAY pass the full extent. |
| Several effect PiPLs per binary | Unsupported, and no other host supports it either [PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/). | MAY omit (load the first effect and log the rest). |
| Audio | Unsupported. `I_USE_AUDIO` plug-ins are not loaded at all [Unsupported Features](https://ae-plugins.docsforadobe.dev/ppro/unsupported-features/). | MAY do the same in P3. |
| Collapse twirly flags, `START_COLLAPSED` | Not honoured [Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/). | MAY ignore at first. |
| World Transform suite, AE Channel suite, `PF_SUBPIXEL_SAMPLE`, `PF_GET_PIXEL_DATA16` | Unsupported. Equivalent macros (`PF_COPY`, `PF_FILL`, …) exist for some [Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/). | MAY omit. A suite request returns "unavailable" and well-behaved plug-ins fall back. |
| Time values | Uses different `time_scale`/`time_step` values. Only the ratios matter, and plug-ins must not hard-code them. | MAY use a time scale derived from flicks, as long as the ratios are exact (time section). |
| Render order and threads | Speculative renders, a low-quality render followed by a high-quality one, no guaranteed time order. Several instances run concurrently, with a per-instance critical section for every command except arbitrary-data ones [Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/). | Precedent for a **per-instance** serialisation level and for out-of-order rendering. |
| Fields, quality, PAR | Renders per field. `quality` is the monitor's setting. Frame size and PAR conventions differ. | Precedent for documenting BAE's own conventions rather than imitating AE's. |
| Plug-in cache | GLOBAL_SETUP results are cached across launches, with a no-cache opt-out [Plug-Ins... Reloaded](https://ae-plugins.docsforadobe.dev/ppro/plug-ins-reloaded/). | Adopt it (§2.3). |
| Premiere-only extras | Pixel-format suites, BGRA and YUV, 32-bit through `PF_Cmd_RENDER`, checked-out frames in the render format [Bigger Differences](https://ae-plugins.docsforadobe.dev/ppro/bigger-differences/). | MUST NOT emulate. These are Premiere-specific. |
| Identity | Its own `appl_id` and version numbering, which does not imply AE feature parity [Premiere Pro & Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/ppro/). | Precedent for BAE presenting its own identity (§2.6). |

**What BAE may skip in the first compat host:**
- Audio, i.e. every audio flag and selector.
- AEGP suites, apart from the 3D read-outs when 3D lands.
- Several effects per binary.
- Code registration, if every target plug-in still ships a PiPL (AE itself requires one).
- `extent_hint` accuracy.
- Premiere-only flags and suites.
- Custom UI and DO_DIALOG, initially: the effect works with standard controls and a "custom
  UI unavailable" notice.

What it must **not** skip:
- SmartFX.
- The 8/16/32 bpc conversions.
- Dynamic flags.
- Cancellation.
- Error and message reporting.
- The flatten protocol.
- MFR threading levels.
- The cache-key rules in §2.7–§2.9.

### 2.12 Cross-check of the `bae-aex-host` skeleton (`pipl.rs`, `caps.rs`, `error.rs`)

| Module | Discrepancy with the guide | Fix |
|---|---|---|
| `pipl.rs` | `MatchName` accepts any UTF-8 up to 255 bytes. Match names are 7-bit ASCII, with a header-defined maximum [AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/). The reserved `ADBE ` prefix is not modelled. | Reject non-ASCII. Keep the byte-exact comparison. Add `is_adobe_first_party()`. |
| `pipl.rs` | The `SpecVersion` doc says it is also what the plug-in "requests in `in_data->version`". In fact the **host** fills `in_data->version` with its own API version [Compatibility](https://ae-plugins.docsforadobe.dev/intro/compatibility-across-multiple-versions/). | Split it into `SpecVersion` (built-with, from the PiPL) and `HostApiVersion` (what BAE reports). |
| `pipl.rs` | A module has no list of descriptors. AE allows several PiPLs per file, and code registration may register several effects. | `PluginPackage` → `Vec<PluginDescriptor>`, and skip non-effect kinds. |
| `pipl.rs` | `declared_flags` says a mismatch "is a warning AE reports". The guide says only that the flags must agree, and adds the PiPL-overrides flag. | Add an authority rule (§2.5). Drop the unverified claim. |
| `pipl.rs` | `CodeArch::WindowsArm64Ec` has no PiPL descriptor in the guide (only `CodeWin64X86`/`CodeWinARM64`). | Mark it as BAE inference. Map it from the x64 descriptor. |
| `caps.rs` | Missing: `AE13_5_THREADSAFE`, `CUSTOM_UI_ASYNC_MANAGER`, `PiPL_OVERRIDES_OUTDATA_OUTFLAGS`, `SEQUENCE_DATA_NEEDS_FLATTENING`, and the ignored flags (`I_AM_THREADSAFE`, `CAN_COMBINE_WITH_DESTINATION`, Premiere-only ones). | Add the fields, plus an `ignored` diagnostic set. |
| `caps.rs` | No separation between static and dynamic caps. `wide_time_input` and `doesnt_need_empty_pixels` are not marked dynamic. | Add `apply_dynamic()`, which accepts only the ten dynamic flags and only those set at GLOBAL_SETUP. |
| `caps.rs` | `NodeRequirements` drops `writes_input_buffer`, `shrinks_buffer` and `doesnt_need_empty_pixels` (private input copy, trimmed or null input, output size change). `gpu` ignores per-device setup, the DirectX flag and the `ADBE ` exclusion. `SerialPerModule` does not say whether UI-thread calls are included. | Extend `RoiModel::LegacyFullFrame` and `NodeRequirements`. Make GPU a per-device decision. Define how the concurrency levels apply to UI and render threads. |
| `error.rs` | A message can attach only to an error. There is no host→plug-in mapping, no "already reported" provenance, no sticky cancel, and no suppression policy. The `InternalStructDamaged` → "re-run resetup" rule is undocumented. | Add a separate `PluginMessage`, `ErrorOrigin`, a per-call `cancel_signalled`, and a `ReportPolicy`. Mark the resetup rule as an inference. |

### 2.13 P3 risk register (anatomy)

- **Legal and format.** The PiPL binary layout and property keys, the `PF_VERSION` packing,
  the flag bit positions and the error values are defined only in SDK headers. P3 needs a
  clean-room source for each (§21). Adobe's stated policy discourages third-party hosts
  [Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/other-hosts/).
- **Identity.** Plug-ins gate licensing and features on `appl_id` and `in_data->version`.
  A BAE identity may cause refusals. Copying AE's identity raises legal questions and makes
  plug-ins assume full AE support (AEGP, scripting).
- **Undocumented AE behaviour.** What AE does on a PiPL/GLOBAL_SETUP mismatch, without
  `AE13_5_THREADSAFE`, or on version-wrap edge cases is not stated. These need black-box
  testing against licensed AE installs.
- **Dynamic-flag cost.** Querying on every change crosses a process boundary (§20). The
  per-state cache in §2.9 must keep this off the playback hot path.
- **Cache correctness.** Missing any of the cache-key inputs gives silently stale frames:
  shutter, camera, lights, timecode, masks, watermark, GUID mix-in, effect version, and the
  wide-time dependencies. This is the most likely source of "works in AE, wrong in BAE"
  bugs.
- **Cancellation.** Plug-ins that swallow `PF_Interrupt_CANCEL` produce wrong frames. The
  host-side sticky cancel flag is the only defence.
- **Platforms.** macOS signing and library validation, Rosetta and x64-emulation helpers,
  and legacy macOS resource access for `KEEP_RESOURCE_OPEN` and ABOUT.

## 3. Command selectors and call sequence

The host drives every interaction. Each one is a single call to the plug-in's one entry point, passing a command selector, `in_data`, `out_data`, the parameter array, the output world and an `extra` pointer whose type depends on the selector. The host refreshes `in_data` and `params` before each call and inspects `out_data` afterwards ([SDK: Effect Basics › Entry Point](https://ae-plugins.docsforadobe.dev/effect-basics/entry-point/)). Only the first few selectors arrive in a predictable order; the rest follow user actions ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). In AE a "sequence" is one applied effect instance. In BAE it is one effect Prim (02) whose implementation is a compat-hosted plug-in.

Legend:

- **Hook** is the `LifecycleHook` variant in `crates/bae-aex-host/src/selector.rs`, named after the `BaeEffectVTable1` callback of `sdk/c/bae_plugin.h` it is translated to (19 spec §9.1; a unit test checks the names against the header).
- **Hot** marks whether the selector sits on the render path:
  - **Y**: called for every rendered frame or request.
  - **W** (warm): called often and off the pixel path, but its latency affects interactivity or cache decisions.
  - **N**: rare or driven by the UI.
- "(inference)" marks statements the guide does not make. "Guide silent" means the guide does not address the point.

### 3.1 Entry-point contract (host side)

| # | AE behaviour | Host obligation | BAE |
|---|---|---|---|
| E1 | One entry function. The PiPL gives its name, and it receives (cmd, in_data, out_data, params, output, extra) ([SDK: Effect Basics › Entry Point](https://ae-plugins.docsforadobe.dev/effect-basics/entry-point/)). | The P3 loader MUST resolve the symbol named in the PiPL and MUST NOT assume a fixed name. | 19 loader |
| E2 | The host updates `in_data` and `params` before each call and checks `out_data` for changes after it ([SDK: Effect Basics › Entry Point](https://ae-plugins.docsforadobe.dev/effect-basics/entry-point/)). | MUST build a fresh `in_data`/`out_data` pair for every call. With concurrent calls, each in-flight call MUST get its own storage (inference). MUST read `out_data` after **every** selector, not only after setup. | 19 |
| E3 | `params` and `output` are valid only for some selectors. When valid, `params` holds the values at `in_data->current_time` and `params[0]` is the input image ([SDK: Effect Basics › Entry Point](https://ae-plugins.docsforadobe.dev/effect-basics/entry-point/)). | MUST evaluate every non-layer param at exactly `current_time` (after the flicks-to-ticks conversion of §4.3). SHOULD pass null for any argument that is invalid for the selector rather than a stale pointer (inference). Per-selector validity is in §4.2. | 02 / 04 |
| E4 | `extra` changes with the selector and, for events, with the event type ([SDK: Effect Basics › Entry Point](https://ae-plugins.docsforadobe.dev/effect-basics/entry-point/)). | MUST pass the matching record: event record (EVENT), changed-param record (USER_CHANGED_PARAM, [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)), arb function record, dependency record, pre-render/render records, GPU device records. | 19 |
| E5 | Plug-ins must return every error, including errors that callbacks returned to them ([SDK: Effect Basics › Errors](https://ae-plugins.docsforadobe.dev/effect-basics/errors/)). | MUST treat the return code as authoritative (see §4.8). | 04 |
| E6 | Handles AE passes in are locked before the call and unlocked after it ([SDK: Effect Details › Memory Allocation](https://ae-plugins.docsforadobe.dev/effect-details/memory-allocation/), [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). | MUST lock and unlock global, sequence and frame handles around every call. With non-moving handles, lock can return the stable pointer (inference). | 19 |
| E7 | Many selectors are opt-in through out-flags or param flags (per row below, [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | MUST NOT send an opt-in selector unless the plug-in opted in (see the "When" column). | 19 |
| E8 | Every plug-in must answer the global selectors and `PF_Cmd_RENDER`; most other selectors are optional ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | MUST treat an unrecognised-selector result from an optional selector as "not handled", not as a failure of the instance (inference). | 19 |

### 3.2 Opt-in gating: what the plug-in must declare before the host may send a selector

| Selector or behaviour | Sent or allowed only if | Declared where | Source |
|---|---|---|---|
| `PF_Cmd_DO_DIALOG` | `PF_OutFlag_I_DO_DIALOG` | GLOBAL_SETUP; checked at SEQUENCE_SETUP | [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) |
| DO_DIALOG sent automatically after SEQUENCE_SETUP | `PF_OutFlag_SEND_DO_DIALOG` | RESETUP, according to the guide | [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) |
| `PF_Cmd_UPDATE_PARAMS_UI` | `PF_OutFlag_SEND_UPDATE_PARAMS_UI`, or any supervised param | PiPL and GLOBAL_SETUP, or PARAM_SETUP | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/) |
| `PF_Cmd_USER_CHANGED_PARAM` | `PF_ParamFlag_SUPERVISE` on the changed param | PARAM_SETUP | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) |
| `PF_Cmd_EVENT` | `PF_OutFlag_CUSTOM_UI`, PF_REGISTER_UI, and control or topic UI flags on params | GLOBAL_SETUP and PARAM_SETUP | [SDK: Effect UI & Events › Effect UI & Events](https://ae-plugins.docsforadobe.dev/effect-ui-events/effect-ui-events/), [SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/) |
| `PF_Cmd_ARBITRARY_CALLBACK` | An arbitrary-data param was registered | PARAM_SETUP | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) |
| `PF_Cmd_GET_EXTERNAL_DEPENDENCIES` | `PF_OutFlag_I_HAVE_EXTERNAL_DEPENDENCIES` | GLOBAL_SETUP | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) |
| `PF_Cmd_QUERY_DYNAMIC_FLAGS` | `PF_OutFlag2_SUPPORTS_QUERY_DYNAMIC_FLAGS` | PiPL **and** GLOBAL_SETUP | [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) |
| `PF_Cmd_SMART_PRE_RENDER`, `PF_Cmd_SMART_RENDER` instead of the FRAME_* and RENDER path | `PF_OutFlag2_SUPPORTS_SMART_RENDER`. SmartFX plug-ins may keep supporting the legacy path for other hosts. | GLOBAL_SETUP | [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/) |
| 16 bpc worlds | `PF_OutFlag_DEEP_COLOR_AWARE`. Input and output always share a depth, never deeper than claimed. | GLOBAL_SETUP | [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/) |
| 32 bpc float worlds | `PF_OutFlag2_FLOAT_COLOR_AWARE` **and** SmartFX | GLOBAL_SETUP | [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) |
| GPU selectors | `PF_OutFlag2_SUPPORTS_GPU_RENDER_F32` at GLOBAL_SETUP, then GPU flags for each device | GLOBAL_SETUP, GPU_DEVICE_SETUP | [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) |
| `PF_Cmd_GET_FLATTENED_SEQUENCE_DATA` | `PF_OutFlag2_SUPPORTS_GET_FLATTENED_SEQUENCE_DATA` | GLOBAL_SETUP | [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/) |
| Concurrent render calls | `PF_OutFlag2_SUPPORTS_THREADED_RENDERING` | GLOBAL_SETUP | [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/) |
| Writable sequence data per render thread | `PF_OutFlag2_MUTABLE_RENDER_SEQUENCE_DATA_SLOWER` | GLOBAL_SETUP | [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/) |
| Audio selectors | `PF_OutFlag_AUDIO_EFFECT_TOO` or `PF_OutFlag_AUDIO_EFFECT_ONLY` | GLOBAL_SETUP | [SDK: Audio › Global Outflags](https://ae-plugins.docsforadobe.dev/audio/global-outflags/) |
| Plug-in resources open outside GLOBAL_SETUP, ABOUT and DO_DIALOG | `PF_OutFlag_KEEP_RESOURCE_OPEN` | GLOBAL_SETUP | [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) |
| FLATTEN / RESETUP round trips | `PF_OutFlag_SEQUENCE_DATA_NEEDS_FLATTENING` — but AE and Premiere both **assume it is set**, so the host MUST flatten every plug-in's data whatever the flag says | GLOBAL_SETUP | [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) |
| Extra FRAME_SETUP / FRAME_SETDOWN pair without RENDER | `PF_OutFlag_I_EXPAND_BUFFER` (legacy only; obsolete for SmartFX) | GLOBAL_SETUP | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) |
| `in_data->extent_hint` honoured by the plug-in | `PF_OutFlag_USE_OUTPUT_EXTENT` (PiPL and GLOBAL_SETUP; obsolete for SmartFX) | GLOBAL_SETUP | [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/) |
| `in_data->field` meaningful | `PF_OutFlag_PIX_INDEPENDENT` (AE). Premiere fills it regardless ([SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)) | GLOBAL_SETUP or QUERY_DYNAMIC_FLAGS | [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/) |
| `in_data->shutter_angle` meaningful | `PF_OutFlag_I_USE_SHUTTER_ANGLE` | GLOBAL_SETUP or QUERY_DYNAMIC_FLAGS | [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/), [SDK: Effect Details › Motion Blur](https://ae-plugins.docsforadobe.dev/effect-details/motion-blur/) |
| `PF_CHECKOUT_LAYER_AUDIO` from a visual effect | `PF_OutFlag_I_USE_AUDIO` | GLOBAL_SETUP | [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) |
| UI-thread and render-thread calls on *different project copies* at the same time (never the same instance) | `PF_OutFlag2_AE13_5_THREADSAFE` | GLOBAL_SETUP | [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/) |

### 3.3 Global selectors

All plug-ins must answer these ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)).

| Selector | When AE sends it | Valid inputs | Plug-in does / returns | Host MUST | Hook | Hot |
|---|---|---|---|---|---|---|
| `PF_Cmd_GLOBAL_SETUP` | First call, when the effect is first applied ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). Sent only on the main thread and never concurrently with another selector ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | System part of `in_data` (`version`, `appl_id`, `utils`, `pica_basicP`). Resources are open ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). Guide silent on params; treat as invalid. | Sets `my_version`, `out_flags`, `out_flags2` and may allocate `global_data` ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). An MFR-safe plug-in writes global data only here and in GLOBAL_SETDOWN ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | Store version and both flag words. Check that they match the PiPL flags, unless the PiPL-overrides flag is set ([SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/), [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). Keep `global_data` per module. Hold the module-exclusive lock. A failure here makes the module unusable (inference). | GlobalInit | N |
| `PF_Cmd_PARAM_SETUP` (in the guide's spelling) | Right after GLOBAL_SETUP on first application ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | `inter` with PF_ADD_PARAM and PF_REGISTER_UI available ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). | Adds each parameter, registers custom UI and sets `out_data->num_params`. May rename the Options button, and only here ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | Deep-copy every definition. Check that `num_params` equals the added params plus the implicit input layer ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). Turn the definitions into the BAE param schema (02), cached per (match name, `my_version`) (inference). | DefineParams | N |
| `PF_Cmd_ABOUT` | User picks *About…* in the Effect Controls window ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Resources are open. On macOS the current resource file is the module ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Fills `return_msg`, including version information. | Show `return_msg` in a simple modal dialog (11/12). | About | N |
| `PF_Cmd_GLOBAL_SETDOWN` | AE quits, or the last instance of the effect is removed ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). Main thread only, exclusive ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | `global_data`. | Frees global data. | Do not treat this as unload: plug-ins are told not to infer unload from it ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)), and SmartFX modules are never unloaded ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). Re-applying the effect later needs GLOBAL_SETUP again (inference). BAE policy: set down all GPU devices first. | GlobalShutdown | N |

### 3.4 Sequence (instance) selectors

| Selector | When AE sends it | Valid inputs | Plug-in does / returns | Host MUST | Hook | Hot |
|---|---|---|---|---|---|---|
| `PF_Cmd_SEQUENCE_SETUP` | Effect added to a layer ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). Sent from the UI thread ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)), but under MFR it may overlap UI selectors on other threads ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | `in_data` is initialised ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). `time_step` is 0 unless constant, and downsample is valid ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). Guide silent on `params`. | Allocates sequence data and returns it in `out_data->sequence_data` ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). May resize it ([SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)). | Store the handle. If I_DO_DIALOG and SEND_DO_DIALOG are both set, follow with DO_DIALOG ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). BAE policy: write a flat copy into the 02 state slot in the same Command (§3.11). | InstanceCreate | N |
| `PF_Cmd_SEQUENCE_RESETUP` | Sent after sequence data is read from disk, during precompose, when the effect is copied, and when its layer changes. On duplication it goes to both the old and the new instance ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). Sent on both UI and render threads, because the render side holds its own project copy ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | The data may be flat or not flat, and the plug-in must tell which ([SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)). Downsample is valid ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). | Rebuilds live data (usually by unflattening) and may resize the handle. The guide says SEND_DO_DIALOG is set here ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Pass the slot blob or the current handle, and re-read `out_data->sequence_data`. Do not assume a FLATTEN between two RESETUPs ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). For a render-side clone, MUST set `PF_InFlag_PROJECT_IS_RENDER_ONLY` in `in_data->in_flags` (valid only in this selector); the plug-in then treats the project as read-only and expects **no UI selectors** on that instance, so the host MUST NOT send any ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). When BAE substitutes *different* bytes (undo, redo, remote edit, paste-over), send SETDOWN on the old live data first (inference, §3.11). | InstanceRestore | W (render-side clones) |
| `PF_Cmd_SEQUENCE_FLATTEN` | On save, and before duplication ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Live data. | Packs pointer-based data into one byte-order-safe block, frees the live data and sets `out_data->sequence_data` to the flat block ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)). | Copy the flat bytes into the slot verbatim and never interpret them. Send RESETUP before any later selector that needs live data (BAE policy). After a recent flatten, AE may delete the instance without SETDOWN and free the flat data itself ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)), so the host MUST be able to free flat handles. Prefer GET_FLATTENED when available; Adobe says FLATTEN will eventually be unsupported ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)), but every pre-13.5 plug-in still depends on it. | InstanceSerialize | N |
| `PF_Cmd_GET_FLATTENED_SEQUENCE_DATA` | Since 13.5, used to marshal sequence data from UI to render without freeing and reallocating it. Sent only to plug-ins that set `PF_OutFlag2_SUPPORTS_GET_FLATTENED_SEQUENCE_DATA` ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). Will be required for MFR plug-ins whose data needs flattening ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | Live data. | Returns a flat **copy** and leaves the live structures untouched ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Use this as the default way to fill the slot and render snapshots. The host owns the copy (inference). Without it, AE does not honour FORCE_RERENDER from click or drag events ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)); BAE SHOULD do the same. | InstanceSnapshot | W |
| `PF_Cmd_SEQUENCE_SETDOWN` | User removes the effect or closes the project, or AE exits ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). May arrive off the UI thread under MFR ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | Live data. | Frees all sequence data. | Send once per live instance, render-side clones included. Flat-only instances are freed by the host. BAE policy: the slot already holds current bytes, so undoing a delete can RESETUP. | InstanceDestroy | N |

### 3.5 Frame, SmartFX and GPU selectors

| Selector | When AE sends it | Valid inputs | Plug-in does / returns | Host MUST | Hook | Hot |
|---|---|---|---|---|---|---|
| `PF_Cmd_FRAME_SETUP` | Legacy (non-SmartFX) path, immediately before each RENDER. If `PF_OutFlag_I_EXPAND_BUFFER` is set, it is also sent once **without** a RENDER (setup then setdown) so AE can tell whether the layer is visible ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Every param except the input layer ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). `time_step` is correct here ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). | May set output `width`, `height` and `origin`. Setting 0×0 makes AE ignore the following RENDER. May allocate `frame_data` and may set NOP_RENDER ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Use the size and origin to allocate the output world and to report the node's output rect to 04. Run the setup/setdown pair without RENDER as a plan-time **bounds query**. Pass `frame_data` on to RENDER and SETDOWN. | LegacyFramePrepare | Y |
| `PF_Cmd_RENDER` | Every frame on the legacy path ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). Sent on render threads ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | All `in_data` fields, all params including the input world `params[0]`, and the output world ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). 8 or 16 bpc only. | Writes the output and polls PF_ABORT/PF_PROGRESS. Must not delete `frame_data` here. May set REFRESH_UI ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Supply 8 or 16 bpc worlds only (32 bpc requires SmartFX). If any abort or progress callback returned an error during the call, **discard** the output whatever the return code ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). Never cache a cancelled or failed frame ([SDK: Effect Basics › Errors](https://ae-plugins.docsforadobe.dev/effect-basics/errors/)). | LegacyRender | Y |
| `PF_Cmd_FRAME_SETDOWN` | After RENDER, or after the bounds-only setup ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | `frame_data`. | Frees frame data. | Always pair with a successful FRAME_SETUP, even when RENDER was skipped or cancelled (inference). | LegacyFrameFinish | Y |
| `PF_Cmd_SMART_PRE_RENDER` | SmartFX plug-ins (`PF_OutFlag2_SUPPORTS_SMART_RENDER`) get this instead of FRAME_SETUP ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). May be sent any number of times before one SMART_RENDER ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)), and may never be followed by a render. An empty request rect means a bounds-only query ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). | **No** params array, so params must be checked out. `extra` holds the request (rect in layer space, field, channel mask, preserve-RGB-of-zero-alpha), bit depth, and GPU framework and device ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/), [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Checks out every layer input it *might* need. Returns result rect, max result rect, solid, extra-pixels flag, `pre_render_data` and a deleter. On GPU, sets the GPU-render-possible flag ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). Under Premiere's MediaCore it can arrive twice, the second time with the real source sizes ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)), so plug-ins tolerate repeated pre-renders. | Treat the max result rect as independent of the request. Own `pre_render_data` once the call returns and free it with the deleter ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). If the GPU flag is absent, re-call with another framework or with none (CPU) ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). Turn checkouts into 04 dependency edges. | PlanRender | Y |
| `PF_Cmd_SMART_RENDER` | At most once per pre-render, when CPU output is needed ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). | Same request as the pre-render, plus `pre_render_data` and the checkout callbacks. No params array. 8, 16 or 32 bpc as declared ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Checks out each pre-declared input at most once per checkout id. Checks out the output only after at least one input ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). | Refuse checkouts not declared in pre-render. Release leftover checkouts when the call returns ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). Apply the same discard-on-abort rule as RENDER (inference). | RenderCpu | Y |
| `PF_Cmd_SMART_RENDER_GPU` (the guide's table header spells it `PF_Cmd_GPU_SMART_RENDER_GPU`) | Sent instead of SMART_RENDER when the output must be a GPU frame, and only when `what_gpu` is not none ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Same records as SMART_RENDER, with framework and device unchanged since pre-render. World descriptors have a null data pointer; every other field is valid ([SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/)). | Renders with the native API on the given device. Every checkout returns a GPU frame ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Provide native buffers on a device that accepted GPU_DEVICE_SETUP. Keep `what_gpu` unchanged between pre-render and render. Expose device, context and queue through the GPU device suite. | RenderGpu | Y |
| `PF_Cmd_GPU_DEVICE_SETUP` | Any time, at most once per device, after GLOBAL_SETUP and before SEQUENCE_SETUP. Several devices may be in setup at the same time ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | `in_data->what_gpu` is not none, and `extra` carries the device index. | Sets the GPU support flags in `out_flags2` for each device it accepts and returns plug-in-owned `gpu_data`. If it sets no flags, it is treated as not supporting GPU on that device. | Use the CPU path on any device without flags. Store `gpu_data` per (module, device) and pass it at render time. Pair every setup with a SETDOWN. | GpuAttach | N |
| `PF_Cmd_GPU_DEVICE_SETDOWN` | Just before the host releases the device ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | `gpu_data`, framework and device index. | Releases `gpu_data`. | Send before device teardown or loss, and before GLOBAL_SETDOWN (BAE policy). | GpuDetach | N |

### 3.6 Audio selectors

| Selector | When | Valid inputs | Plug-in does / returns | Host MUST | Hook | Hot |
|---|---|---|---|---|---|---|
| `PF_Cmd_AUDIO_SETUP` | Before every audio render ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Audio fields of `in_data` (`start_sampL`, `dur_sampL`, `total_sampL`, `src_snd`) ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). | May ask for a different input span through the sample fields in `out_data` ([SDK: Audio › Accessing Audio Data](https://ae-plugins.docsforadobe.dev/audio/accessing-audio-data/)). May also allocate and initialise sequence-specific data, and may resize the sequence handle ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)). IIR filters (`PF_OutFlag_AUDIO_IIR`) may ask for earlier audio and read params at those earlier times ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Render input audio for the span requested. The guide names the fields both `startsampL`/`endsampL` and `start_sampL`/`dur_sampL` (see §4.12). An audio-only plug-in writes sequence data from an audio selector, so audio selectors MUST run on an instance whose sequence data is writable (inference). | AudioPrepare | Y (audio) |
| `PF_Cmd_AUDIO_RENDER` | After AUDIO_SETUP. | All of `in_data` ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Fills `out_data->dest_snd`. An IIR filter whose output does not match the requested time sets the output duration to zero ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Convert formats; float-only plug-ins (`PF_OutFlag_AUDIO_FLOAT_ONLY`) get float ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). Treat a zero-duration result as "no output for this span" (inference). Discard results on abort ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). Plug-ins cannot lengthen a clip ([SDK: Audio › Accessing Audio Data](https://ae-plugins.docsforadobe.dev/audio/accessing-audio-data/)). | AudioRender | Y |
| `PF_Cmd_AUDIO_SETDOWN` | After render. | — | Frees what AUDIO_SETUP allocated. | Always pair with AUDIO_SETUP. | AudioFinish | Y |

### 3.7 Messaging, UI and parameter selectors

| Selector | When AE sends it | Valid inputs | Plug-in does / returns | Host MUST | Hook | Hot |
|---|---|---|---|---|---|---|
| `PF_Cmd_EVENT` | Custom UI events, for plug-ins that set `PF_OutFlag_CUSTOM_UI` and registered UI ([SDK: Effect UI & Events › Effect UI & Events](https://ae-plugins.docsforadobe.dev/effect-ui-events/effect-ui-events/)). Drawing happens on the UI thread ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | `extra` is the event record. Non-layer params are valid. No layer checkouts during UI events ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). | Handles click, drag, draw, key and other events. May change values on click, drag and keydown ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | Run on the UI thread. Fold all value changes from one gesture into a single Command (inference). Details are in the UI section. | UiEvent | W (UI) |
| `PF_Cmd_USER_CHANGED_PARAM` | User changed a param flagged `PF_ParamFlag_SUPERVISE` ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). Sent on the UI thread ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). Guide silent on undo; users report it is **not** sent on undo ([Adobe community thread](https://community.adobe.com/t5/after-effects-discussions/pf-cmd-user-changed-param-no-called-when-pressing-undo/m-p/14871574)). | `extra` carries the changed param's index ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). `current_time` is the time the user is viewing, in layer time, which is also where an automatic keyframe would go ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | May change the values and appearance of any param. Marks changed values with `PF_ChangeFlag_CHANGED_VALUE` and may set FORCE_RERENDER ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/), [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Call it synchronously inside the Command that changed the value, and add the plug-in's edits to the **same** undo unit, because AE makes them undoable. Ignore CHANGED_VALUE on layer params ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). Afterwards, re-query dynamic flags and re-snapshot sequence data. | ParamChanged | N |
| `PF_Cmd_UPDATE_PARAMS_UI` | Effect Controls panel needs a refresh, for example when it opens, when the time moves, and when the plug-in is first loaded ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). Sent only when `PF_OutFlag_SEND_UPDATE_PARAMS_UI` is set ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)) or any param is supervised ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | Params are valid. The plug-in must **never** check out params here ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Only cosmetic changes through PF_UpdateParamUI: enable state, name, custom UI height ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | Reject value writes. Make PF_CHECKOUT_PARAM fail fast here instead of recursing (inference). Sequence data must not be written ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). In this selector only, AE makes `PF_GetCurrentState` return a random GUID ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)); the host SHOULD do the same so plug-ins cannot key caches off UI refreshes. BAE policy: the cosmetic changes are per-instance view state (11/12), never a Command; the guide is silent on whether AE persists them. | RefreshParamUi | W (UI) |
| `PF_Cmd_DO_DIALOG` | Options button or menu, only when `PF_OutFlag_I_DO_DIALOG` is set ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). Sent on the UI thread ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Non-layer params are valid, and params may be checked out at other times ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Shows a modal dialog. May change and resize sequence data, but may **not** change param values; arb changes are ignored ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). May set REFRESH_UI. | Run it modally (out of process, the dialog is the plug-in's own window). Re-snapshot sequence data into the slot through a Command. That makes it undoable in BAE, whereas AE does not ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)): an intentional fidelity deviation. | OptionsDialog | N |
| `PF_Cmd_ARBITRARY_CALLBACK` | Whenever AE manages an arb-data value: new, dispose, copy, flat size, flatten, unflatten, interpolate, compare, print size, print, scan ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). | `extra` carries the function selector and its arguments. `effect_ref` may be NULL, and then no checkouts are allowed. | Implements the value operation. Must be recursively re-entrant, because its own checkout can trigger it. | Allow calls on any thread, including nested inside a render of the same module. Premiere keeps arb calls out of its per-instance critical section ([SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)). | CustomValueOp | Y while animated arb params are interpolated |
| `PF_Cmd_GET_EXTERNAL_DEPENDENCIES` | Only when `PF_OutFlag_I_HAVE_EXTERNAL_DEPENDENCIES` is set ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | `extra` sets the check type: all dependencies, or only missing ones. | Returns a string handle describing them, or null. | Free the returned handle. Feed the result into packaging and the missing-file report. | ExternalDependencies | N |
| `PF_Cmd_COMPLETELY_GENERAL` | An AEGP sends the effect a message ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | `extra` is whatever the AEGP passed. | Anything. | MAY never send it, since BAE hosts no AEGPs. Reserved for plug-in-to-plug-in messaging in 19. | PrivateMessage | N |
| `PF_Cmd_QUERY_DYNAMIC_FLAGS` | Only when `PF_OutFlag2_SUPPORTS_QUERY_DYNAMIC_FLAGS` is in both the PiPL and GLOBAL_SETUP. Sent at arbitrary times ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | The plug-in checks out its non-layer params with PF_CHECKOUT_PARAM ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Sets any of WIDE_TIME_INPUT, NON_PARAM_VARY, PIX_INDEPENDENT, I_USE_SHUTTER_ANGLE, I_USE_3D_CAMERA, I_USE_3D_LIGHTS, DOESNT_NEED_EMPTY_PIXELS, REVEALS_ZERO_ALPHA, DEPENDS_ON_UNREFERENCED_MASKS and OUTPUT_IS_WATERMARKED. It can change only flags it set at GLOBAL_SETUP, and should answer quickly. | Mask the answer with the GLOBAL_SETUP flags. Re-query after param changes, before planning renders, and cache the answer per param-state hash. The next render must match the last answer, which matters for the watermark race ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | QueryDynamicCaps | W |

### 3.8 Selectors outside the guide

| Selector | Status | Host obligation |
|---|---|---|
| `PF_Cmd_TRANSLATE_PARAMS_TO_PREFS` | Guide silent. Premiere Pro forum threads describe it as part of Premiere's importer "source settings" feature: the importer names a paired effect, the effect registers itself as a source-settings effect at GLOBAL_SETUP through a Premiere-only suite (and usually hides itself with `PF_OutFlag_I_AM_OBSOLETE`), and Premiere sends this selector, alongside SEQUENCE_SETUP, when the user adjusts the source settings, so the effect can copy its params into the importer's prefs blob ([Adobe community thread](https://community.adobe.com/t5/premiere-pro-discussions/importer-with-source-settings-effect/m-p/8972469), [Adobe community thread](https://community.adobe.com/questions-729/premiere-importer-development-with-source-settings-effect-1360382)). After Effects is not described as sending it (inference: it has no importer source settings). | BAE MUST NOT send it and MUST NOT offer the Premiere source-settings suite, so such effects see an AE-like host. An effect that only works as a source-settings effect SHOULD be hidden from the effect browser (inference). |
| Selectors in headers but not in the guide | Guide silent. | The P3 shim MUST never send an undocumented selector, and MUST treat "unrecognised" returns as harmless (inference). |

### 3.9 Selector summary, cross-checked against `selector.rs`

Thread and params columns state what the guide says. "Silent" means the guide does not say. The last column compares the guide with `Selector::info()` (details under crate discrepancies). "Hook" uses the crate's `LifecycleHook` names.

| Selector | Group | Thread (guide) | `params[]` (guide) | `extra` | Hook | Needs instance | Crate agrees? |
|---|---|---|---|---|---|---|---|
| ABOUT | Global | Silent (user action, so UI) | Silent | — | Message (`bae.about`) | No | Yes |
| GLOBAL_SETUP | Global | Main, exclusive | — | — | GlobalSetup | No | Yes |
| GLOBAL_SETDOWN | Global | Main, exclusive | — | — | GlobalSetdown | No | Partly: the tracker makes it terminal |
| PARAM_SETUP | Global | Silent | — (being defined) | — | DefineParams | No | Yes |
| SEQUENCE_SETUP | Sequence | UI; under MFR may overlap others | Silent | — | InstanceCreate | Creates | No: crate says `Render` |
| SEQUENCE_RESETUP | Sequence | UI **and** render | Silent | — (render-only flag in `in_flags`) | InstanceCreate (from state) | Creates or refreshes | Partly: crate says `Render`, and has no render-only instance flag |
| SEQUENCE_FLATTEN | Sequence | Silent | Silent | — | InstanceSerialize | Yes | Yes |
| GET_FLATTENED_SEQUENCE_DATA | Sequence | Silent | Silent | Flat-copy output | InstanceSerialize | Yes | Yes (Main is an inference) |
| SEQUENCE_SETDOWN | Sequence | Any (MFR) | Silent | — | InstanceDestroy | Yes | Partly: crate says `Render` |
| FRAME_SETUP | Frame | Render | All except input layer | — | PreRender (synthesised) | Yes | Partly: crate says `NonLayer` |
| RENDER | Frame | Render | All | — | RenderCpu | Yes | Yes |
| FRAME_SETDOWN | Frame | Render | Silent | — | ReleaseFrameData | Yes | Yes |
| SMART_PRE_RENDER | Frame | Render | None | Pre-render record | PreRender | Yes | Yes |
| SMART_RENDER | Frame | Render | None | Smart-render record | RenderCpu | Yes | Yes |
| SMART_RENDER_GPU | Frame | Render | None | Smart-render record | RenderGpu | Yes | Yes |
| GPU_DEVICE_SETUP | GPU | "Any time" | — | Device record | GpuDeviceSetup | No | Yes (Main is an inference) |
| GPU_DEVICE_SETDOWN | GPU | Silent | — | Device record | GpuDeviceSetdown | No | Yes |
| AUDIO_SETUP / AUDIO_RENDER / AUDIO_SETDOWN | Audio | Silent | Silent | — | AudioEffect (reserved) | Yes | Yes |
| EVENT | Messaging | UI | Non-layer | Event record | UiEvent | Yes | Yes |
| USER_CHANGED_PARAM | Messaging | UI | Valid, and the plug-in edits them ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)) | **Changed-param record** | ParamChanged | Yes | **No: crate says `ExtraKind::None`** |
| UPDATE_PARAMS_UI | Messaging | UI (inference) | Valid, no checkouts | — | UpdateParamsUi | Yes | Yes |
| DO_DIALOG | Messaging | UI | Non-layer | — | Message (`bae.options_dialog`) | Yes | Yes |
| ARBITRARY_CALLBACK | Messaging | Silent, re-entrant | — | Arb record | ArbitraryValue | No (`effect_ref` may be NULL) | Partly: crate says `Render` |
| GET_EXTERNAL_DEPENDENCIES | Messaging | Silent | Silent | Dependency record | GetDependencies | Yes | Yes |
| COMPLETELY_GENERAL | Messaging | Silent | Silent | AEGP pointer | Message | Yes | Yes |
| QUERY_DYNAMIC_FLAGS | Messaging | "Arbitrary times" | Checked out by the plug-in, not passed | — | QueryDynamicFlags | Yes | Partly: crate says `NonLayer` params and `Main` thread |
| FRAME_SETUP / RENDER / FRAME_SETDOWN, SMART_PRE_RENDER / SMART_RENDER, AUDIO_* pairing | — | — | — | — | — | — | **No**: `ProtocolTracker` tracks module and instance state only, not per-frame or per-request pairing |

### 3.10 Required call order (state machine)

```text
MODULE   Unloaded --load--> Loaded --GLOBAL_SETUP--> GlobalReady --PARAM_SETUP--> Ready
         Ready:     ABOUT* | GPU_DEVICE_SETUP(d)/SETDOWN(d)* | ARBITRARY_CALLBACK* | instance selectors
         Ready --GLOBAL_SETDOWN [no live instance, no GPU device]--> Down --GLOBAL_SETUP--> GlobalReady  (or unload)
INSTANCE (none) --SEQUENCE_SETUP (new) | SEQUENCE_RESETUP (blob)--> Live
         Live --GET_FLATTENED--> Live          Live --FLATTEN--> Flat --RESETUP--> Live
         Live --SETDOWN--> (none)              Flat --host frees, no SETDOWN--> (none)
CLONE    (none) --SEQUENCE_RESETUP + PROJECT_IS_RENDER_ONLY (slot blob)--> RenderLive
         RenderLive: render/frame/audio/arb/dynamic-flag selectors only, never UI selectors
         RenderLive --SETDOWN (snapshot retired)--> (none)
FRAME    legacy : FRAME_SETUP -> RENDER -> FRAME_SETDOWN
                  (+ I_EXPAND_BUFFER: an extra FRAME_SETUP -> FRAME_SETDOWN with no RENDER)
         SmartFX: SMART_PRE_RENDER{1..n} -> (SMART_RENDER | SMART_RENDER_GPU){0..1}; host frees pre_render_data
AUDIO    AUDIO_SETUP -> AUDIO_RENDER -> AUDIO_SETDOWN
```

| Rule | Source | Host enforcement |
|---|---|---|
| GLOBAL_SETUP comes first and PARAM_SETUP second. Instances only after both. | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) | ProtocolTracker (debug). |
| GPU_DEVICE_SETUP comes after GLOBAL_SETUP and before SEQUENCE_SETUP, at most once per device. | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) | Attach devices when the module becomes Ready. Hot-plugged devices attach later. |
| Every RENDER sits between a FRAME_SETUP and a FRAME_SETDOWN. `frame_data` is freed only in SETDOWN. | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) | Track per frame (the tracker does not yet, see discrepancies). |
| Pre-render may happen without render. Render happens at most once per pre-render. Pixel checkouts match pre-render checkout ids one to one. | [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/) | Track per request. |
| `what_gpu` is the same in pre-render and GPU render. | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) | Assert. |
| FLATTEN does not have to sit between two RESETUPs. RESETUP may receive flat or live data. | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/) | Allowed. |
| After a recent flatten, the instance may be deleted without SETDOWN. | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) | The host frees the flat data. |
| UPDATE_PARAMS_UI must not cause param checkouts. | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) | The checkout callback returns an error during this selector (inference). |
| GLOBAL_SETDOWN is not unload. | [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) | Allow a later GLOBAL_SETUP, followed by PARAM_SETUP again (inference). SmartFX modules are never unloaded ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)), so the host SHOULD keep every module mapped until exit (inference). |
| A render-only instance never receives UI selectors. | [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/) | Track the render-only bit per instance and reject UI selectors for it. |
| Sequence data may be resized only in AUDIO_SETUP/RENDER/SETDOWN, FRAME_SETUP/SETDOWN, RENDER, SEQUENCE_SETUP/RESETUP/FLATTEN/SETDOWN and DO_DIALOG; contents may change in any selector except where MFR forbids writes. | [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/), [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/) | Re-read handle and size after those selectors only; a size change elsewhere is a plug-in bug to log (inference). |

### 3.11 Document events and the selectors they cause

| BAE event (02 Command or system) | AE precedent | Host sends | Notes and BAE mapping |
|---|---|---|---|
| Apply effect | First application triggers GLOBAL_SETUP and PARAM_SETUP. Every application then triggers SEQUENCE_SETUP ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | GLOBAL_SETUP and PARAM_SETUP once per module, then GPU_DEVICE_SETUP per device, SEQUENCE_SETUP, DO_DIALOG if SEND_DO_DIALOG, and GET_FLATTENED into the slot. | The same Command writes the param defaults from the 02 schema and the slot. |
| Open project | RESETUP on load ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Module setup if needed, then RESETUP with the slot blob. **Never** SETUP for saved instances. | If the plug-in is missing, a placeholder keeps the blob (ARCHITECTURE §15). Instantiation can be lazy, on first render or UI use (inference). |
| Save | FLATTEN on save ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Nothing, provided the slot is current (BAE policy). | The slot is refreshed after every selector that can change data (SETUP, RESETUP, USER_CHANGED_PARAM, EVENT, DO_DIALOG), as AE has done since 13.5 ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). |
| Duplicate effect or layer | AE flattens, then sends RESETUP to the old and the new instance ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Copy the slot bytes, then RESETUP the new instance. The old instance stays Live when GET_FLATTENED was used; otherwise FLATTEN and RESETUP it. | The 02 duplicate Command copies the slot verbatim. |
| Copy and paste effect, including across projects or sessions | RESETUP when the effect is copied ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | RESETUP on the pasted instance with the clipboard blob. | The clipboard carries the blob, match name and `my_version` (02/11). |
| Precompose | RESETUP during precompose ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | RESETUP. | Comp context changes: `time_scale`, extent and width may change. |
| Effect's layer changes (moved, source replaced) | RESETUP ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | RESETUP. | 02 StageEvent on the effect's parent. |
| Undo or redo touching the effect | Guide silent. USER_CHANGED_PARAM is not sent ([Adobe community thread](https://community.adobe.com/t5/after-effects-discussions/pf-cmd-user-changed-param-no-called-when-pressing-undo/m-p/14871574)). | If the slot bytes changed: SETDOWN the live instance, then RESETUP from the restored blob. If only params changed: no state selector; 04 invalidates and UPDATE_PARAMS_UI follows. | BAE policy (inference). Replacing the data without SETDOWN would leak the plug-in's live allocations. |
| Remote edit (09 CRDT; no AE equivalent) | — | Same as undo. Defer while a local custom-UI gesture on this instance is in progress. | AE warns that flattening mid-drag loses UI state ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). |
| Delete effect | SETDOWN on removal ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | SETDOWN on the instance and all its render clones. | Undoing the delete sends RESETUP from the slot. |
| Close project, or quit | SETDOWN per instance. GLOBAL_SETDOWN on quit or when the last instance goes ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | SETDOWN for each instance, GPU_DEVICE_SETDOWN for each device, then GLOBAL_SETDOWN. | |
| Render-side snapshot (04) | The render thread has its own project copy and gets RESETUP. Sequence data is const at render time ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/), [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | RESETUP of a render clone from the slot blob, with `PF_InFlag_PROJECT_IS_RENDER_ONLY` set ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). SETDOWN when the snapshot is retired. With MUTABLE_RENDER_SEQUENCE_DATA_SLOWER: one copy per thread, dropped after each render span ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | 04 snapshot lifetime and span ids. |
| Supervised param edited in the UI | USER_CHANGED_PARAM ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | USER_CHANGED_PARAM, then QUERY_DYNAMIC_FLAGS, then GET_FLATTENED. | Runs inside the editing Command (02). |
| Effect Controls opened, or current time moved | UPDATE_PARAMS_UI ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | UPDATE_PARAMS_UI. | 11/12 view state only, never a Command. |
| Out-of-process host restarts after a crash (19; BAE only) | — | GLOBAL_SETUP, PARAM_SETUP and GPU setup, then RESETUP for each instance from its slot. | Works only because the slot is always current. |

### 3.12 Sequence-data refresh policy (BAE host policy)

AE copies flattened sequence data from the UI to the render project whenever effect state changes ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). BAE follows the same model, with the 02 opaque state slot as the copy that travels.

1. **The slot is the source of truth.** Live (unflattened) data in any process is a cache that can be rebuilt with RESETUP. This lets saving, collaboration, undo and out-of-process restarts work without asking the plug-in (inference).
2. **Refresh after UI-side mutation.** After SEQUENCE_SETUP, a UI-side RESETUP, USER_CHANGED_PARAM, the end of an EVENT gesture, and DO_DIALOG, the host takes a flat copy. It uses GET_FLATTENED when available, otherwise FLATTEN followed by RESETUP. If the bytes changed (by hash), the copy goes into the slot inside the current Command.
3. **The render side never writes the slot.** Render clones are built with RESETUP from the slot version pinned by the 04 render snapshot. Sequence data is const at render time ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). With MUTABLE_RENDER_SEQUENCE_DATA_SLOWER, each render thread gets a copy that is thrown away after the span ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). Render-time writes therefore never reach the UI, exactly as in AE since 13.5 ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)).
4. **External replacement means SETDOWN then RESETUP.** This covers undo, redo, remote edits and paste-over, and is deferred while a local gesture on the instance is in progress (§3.11).
5. **Bytes are opaque and plug-in-ordered.** Flat data must already be byte-order-correct for file storage, which is the plug-in's job ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)). The host stores it verbatim, along with match name and version (`sequence.rs` `StateBlob`).
6. **Cost.** Refreshes happen only for edited AE-plugin instances, and never on the render path. A project without AE plug-ins pays nothing. Large blobs SHOULD be stored content-addressed, not inline in CRDT history (inference).

### 3.13 Threads, concurrency and re-entrancy

| Selector class | AE thread | Concurrency | BAE host policy |
|---|---|---|---|
| GLOBAL_SETUP / GLOBAL_SETDOWN | Main thread only ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | Never concurrent with any other selector of the module. | Module-exclusive lock. |
| UI: SEQUENCE_SETUP, USER_CHANGED_PARAM, DO_DIALOG, EVENT/DRAW, UPDATE_PARAMS_UI | UI thread ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Render selectors may run at the same time on other threads ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | 11/12 main thread. Out of process, calls are forwarded to the plug-in process's main thread. |
| SEQUENCE_SETUP, RESETUP, SETDOWN | UI and render threads ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | May arrive on several threads at once, alongside UI selectors, so they must be thread-safe ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | "Any thread." Serialise per module unless the plug-in is MFR-safe. |
| RENDER, SMART_PRE_RENDER, SMART_RENDER(_GPU) | Render threads ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Concurrent only with `PF_OutFlag2_SUPPORTS_THREADED_RENDERING`. Otherwise threads enter one at a time ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | 04 executor. The per-module serial lock MUST be **re-entrant on the same thread**, see below. |
| Any selector, plug-ins with `PF_OutFlag2_AE13_5_THREADSAFE` only | UI and render threads. | Different threads may be inside the module at once on *different project copies*, never on the same instance ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Without this flag (and without THREADED_RENDERING), the host SHOULD NOT run a UI-side selector and a render-clone selector of the same module concurrently (inference; correctness over speed, and the cost falls only on that module). |
| Render requests in general | — | Premiere renders out of time order and on several clones concurrently; plug-ins must not assume increasing time ([SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)). | BAE MAY schedule frames out of order, as 04 needs. |
| UI-thread work in general | UI thread. | The UI thread should not do slow work such as synchronous frame renders ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Layer checkouts from UI selectors (DO_DIALOG, USER_CHANGED_PARAM) SHOULD go through 04 with a progress/cancel UI, not block the 11/12 frame loop (inference). |
| ARBITRARY_CALLBACK | Guide silent on thread. It must be re-entrant ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). | Premiere does not serialise it ([SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)). | Any thread; nested calls allowed. |
| Host suites | Not thread-safe unless documented; only the main thread may change UI ([SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/)). | — | BAE suites SHOULD be thread-safe anyway, and UI-touching suites marshal to the main thread (inference). |

**Re-entrancy.** A plug-in that checks out a layer at another time can make AE render that layer synchronously. That render can call RENDER on *another instance of the same module*, which is plain recursion ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). Arb handlers can likewise be triggered from inside the plug-in's own checkout ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). Plug-ins must not hold locks while calling back into the host ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). Host consequences:

1. A dependency render requested from inside a plug-in call MUST run on the **calling thread**, or the host MUST otherwise ensure it cannot wait on the calling thread's module lock (inference).
2. The serial lock for a non-thread-safe module MUST be re-entrant for the owning thread (inference).
3. The host MUST detect true cycles, where an instance needs its own output at the same time and ROI, and fail with an error instead of deadlocking (inference).

### 3.14 Risks for the P3 implementation

| Risk | Why | Mitigation |
|---|---|---|
| Plug-ins gate features on the host identity | No third-party host supports SmartFX or AEGP calls ([SDK: Introduction › Third-party Plug-in Hosts?](https://ae-plugins.docsforadobe.dev/intro/third-party-plug-in-hosts/)). Plug-ins check `appl_id` and `version` to decide. | Decide the `appl_id` policy (§4.12 Q1). Test each popular plug-in under both identities. |
| Destructive FLATTEN on older plug-ins | Without GET_FLATTENED, every slot refresh costs a FLATTEN plus a RESETUP. | Refresh only at undo-unit commit and on save. Skip unchanged bytes by comparing hashes. |
| RESETUP used beyond AE's documented triggers | Undo, redo and remote edits replace the state; buggy plug-ins may leak or crash. | Send SETDOWN before RESETUP with substituted bytes. Use an out-of-process mode for flaky modules. |
| Nested synchronous renders | Checkout recursion inside a serial plug-in can deadlock the 04 executor. | Inline nested evaluation (reservation to 04), re-entrant module lock, cycle detection. |
| Supervision across Commands | AE makes supervised edits undoable. BAE Commands must accept plug-in edits added during execution. | 02 supervisor hook (reservation). |
| Agent or script edits | Guide silent on whether non-UI edits trigger USER_CHANGED_PARAM. | Decide per product policy; default is to send it for every local Command-originated change (§4.12 Q7). |
| False MFR claims | Plug-ins built with SDKs older than the one the MFR page names must not get concurrent renders even if they set THREADED_RENDERING; AE disables MFR for them ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | Gate concurrency on the SDK spec version in the PiPL as well as on the flag (inference on the exact mechanism). Keep a per-plug-in deny-list. |
| Render clones cost memory and setup time | Each 04 snapshot needs a RESETUP (unflatten) per clone, and with MUTABLE…SLOWER a copy per thread. | Reuse a clone across renders while the slot hash is unchanged; create clones lazily, only for nodes that use AE plug-ins. |

## 4. PF_InData, PF_OutData and callbacks

### 4.1 PF_InData fields the host must fill

AE refreshes `PF_InData` before every selector ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). Unless noted, the citations in this table are that page.

| Field | AE meaning | Valid during | Host obligation | BAE source |
|---|---|---|---|---|
| `inter` | Interaction callbacks: add params, abort, progress, checkout at other times, custom UI registration (§4.9). | All selectors. Each callback has its own restrictions. | MUST be a complete table. A callback that is illegal for the current selector MUST return an error, never crash (inference). | 19 |
| `utils` | Graphics and math callbacks, "defined at all times" (§4.10). | All. | MUST be fully populated. | 19 / 04 |
| `effect_ref` | Opaque value the plug-in passes back to most callbacks so the host can identify it. | All. May be NULL in arb callbacks, where checkouts are then forbidden ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). | MUST encode module, instance and **call frame**, so a callback from a worker thread resolves the right render request (inference). | 19 |
| `quality` | High or low quality. Effects and the graphics utilities behave differently in each. | All frame and sequence selectors. | MUST derive it from the layer's Best/Draft switch: AE passes the source layer's setting, Premiere the monitor's ([SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)). | 02 layer quality, 04 request |
| `version` | Effect API spec version of the host; plug-ins check at GLOBAL_SETUP that it is new enough for them, and branch on it ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/), [SDK: Introduction › Compatibility Across Multiple Versions?](https://ae-plugins.docsforadobe.dev/intro/compatibility-across-multiple-versions/)). | All, including GLOBAL_SETUP. | MUST report the version whose behaviour BAE actually implements and MUST NOT over-claim. Plug-ins also fall back per suite by acquiring older suite versions ([SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/)). | 19 |
| `serial_num` | Serial number of the host application. | All. | SHOULD be a neutral constant (inference; privacy). | — |
| `appl_id` | Identifies the host. AE and Premiere each have their own four-character creator code. Used for licensing checks ([SDK: Introduction › Third-party Plug-in Hosts?](https://ae-plugins.docsforadobe.dev/intro/third-party-plug-in-hosts/)). | All. | Policy decision (§4.12 Q1). MUST be identical in every call of a session. | 19 |
| `num_params` | Parameter count, including the input layer. | After PARAM_SETUP (inference). | MUST equal the count declared at setup ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | 02 |
| `what_cpu`, `what_fpu` | Legacy macOS Gestalt values; undefined on Windows. | — | SHOULD be zero (inference). | — |
| `current_time`, `time_step`, `local_time_step`, `total_time`, `time_scale` | Layer-time context. See §4.3. | RENDER; partly in other selectors. | §4.3. | 02 timing, 04 request |
| `field` | Whether the plug-in may process only the upper or lower field. Valid only with PIX_INDEPENDENT. | Frame selectors. | BAE does no field rendering (ARCHITECTURE silent; inference). SHOULD always report the whole-frame value. | 04 |
| `shutter_angle` | Motion-blur shutter angle. Zero unless motion blur is enabled for the layer. Valid only with I_USE_SHUTTER_ANGLE. The guide is inconsistent about units (§4.12). | Frame selectors. | MUST pass the comp's shutter angle when the layer's motion blur is on, otherwise zero. The plug-in implements its own blur by checking out params inside the shutter interval ([SDK: Effect Details › Motion Blur](https://ae-plugins.docsforadobe.dev/effect-details/motion-blur/)). | 02 comp/layer attributes, 04 request |
| `shutter_phase` | Offset from frame time to shutter open; the guide calls it a percentage of a frame duration but does not give the encoding (§4.12). | Frame selectors. | MUST pass the comp's shutter phase, in the same encoding AE uses (verify with a probe plug-in). | 02 / 04 |
| `width`, `height` | Size of the source layer. Not affected by downsampling, and may differ from the size of the `params[0]` world. The SmartFX "size" reference ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). | Frame and sequence selectors (inference). | MUST pass the layer's intrinsic size: footage size, comp size for precomps and collapsed layers, solid size. | 02 / 04 |
| `extent_hint` | Visible part of input and output: the comp rect in layer space intersected with mask bounds. In original input-layer coordinates (before resizing and origin shifts), and about 20% larger than the layer since AE 6.0. The output world carries its own `extent_hint` (the part of the layer visible in the comp); the plug-in offsets it by `output_origin` to get output-buffer coordinates. Used only by plug-ins that set `PF_OutFlag_USE_OUTPUT_EXTENT`; obsolete for SmartFX ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Legacy frame selectors (inference). | MUST fill both the `in_data` hint and the output world's hint. SHOULD derive them from the 04 visible region (viewport intersected with masks) plus a margin. MUST never make them smaller than the pixels actually needed, and MUST include the hint in the cache key for USE_OUTPUT_EXTENT plug-ins, since their output outside it is undefined (inference). | 04 |
| `output_origin_x/y` | Origin of the output buffer inside the input buffer. Non-zero only when this effect moved the origin. | RENDER after FRAME_SETUP. | MUST echo the origin returned by FRAME_SETUP. | 04 |
| `downsample_x/y` | Rational downsample factors. Point params are pre-scaled; pixel-distance sliders are not. | SEQUENCE_SETUP, SEQUENCE_RESETUP, FRAME_SETUP, RENDER. | MUST pass exact rationals (1/2, 1/4, custom, possibly different in x and y). MUST never pass zero; some hosts do, and plug-ins guard against it ([SDK: Effect Details › Pixel Aspect Ratio](https://ae-plugins.docsforadobe.dev/effect-details/pixel-aspect-ratio/)). | 04 request (preview resolution) |
| `pixel_aspect_ratio` | Pixel width over height for the layer the effect is applied to. Checked-out layers carry their own value ([SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/)). | Frame selectors (inference). | MUST pass the source's rational PAR, never a float approximation (inference). | 02 footage interpretation and comp PAR |
| `in_flags` | The PF_InData page calls it unused, but since 13.5 it carries `PF_InFlag_PROJECT_IS_RENDER_ONLY`, valid only in SEQUENCE_RESETUP: the instance exists only for rendering, the project is read-only, and no UI selectors will follow ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | SEQUENCE_RESETUP (that flag); otherwise unused. | MUST set the flag when restoring a render clone and clear it for the UI instance. MUST be zero in every other selector. | 04 snapshot clones, 19 |
| `global_data`, `sequence_data`, `frame_data` | Handles stored by the plug-in in earlier selectors, locked around each call ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). | §4.5. | §4.5. | 02 slot, 19 |
| `start_sampL`, `dur_sampL`, `total_sampL`, `src_snd` | Audio span and input sound world ([SDK: Audio › Audio Data Structures](https://ae-plugins.docsforadobe.dev/audio/audio-data-structures/)). | Audio selectors. | MUST describe the requested span in the layer's samples. | Audio (no owner yet) |
| `pica_basicP` | PICA basic suite, used to acquire every other suite ([SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/)). | All, including GLOBAL_SETUP. | MUST be valid. Acquiring a suite BAE lacks MUST fail cleanly so the plug-in falls back. | 19 suite registry |
| `pre_effect_source_origin_x/y` | Origin of the source within the input buffer. Non-zero when earlier effects on the layer moved the origin, or when the input was trimmed by DOESNT_NEED_EMPTY_PIXELS ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). Point params are adjusted for the shift at the current time only. | Frame selectors. | MUST track the cumulative origin shift through the effect chain. | 04 |
| `what_gpu` (named on the Command Selectors page, not on the PF_InData page) | GPU framework in use ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | GPU_DEVICE_SETUP. | MUST name Metal or DirectX only; BAE has no CUDA or OpenCL (inference from ARCHITECTURE §15). | 01 |

### 4.2 Per-selector validity matrix

What the host MUST provide, and what the plug-in may do, per selector. Sources: [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/), [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/) (resize list), [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/) (value writes), [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/) (render-time writes). "silent" = guide silent; "(inf.)" = inference. Always valid in every selector: `inter`, `utils`, `effect_ref`, `version`, `appl_id`, `serial_num`, `pica_basicP`, `global_data` (after GLOBAL_SETUP). `quality` is defined in all frame and sequence selectors. Plug-in resources are open only during GLOBAL_SETUP, ABOUT and DO_DIALOG unless the plug-in set `PF_OutFlag_KEEP_RESOURCE_OPEN` ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)).

| Selector | `params[]` | `output` | Time fields | Downsample | Sequence data (state / may resize) | Param checkout | Param value writes honoured |
|---|---|---|---|---|---|---|---|
| GLOBAL_SETUP, GLOBAL_SETDOWN, ABOUT | no (silent) | no | silent | no | none | no (inf.: no instance) | — |
| PARAM_SETUP | being defined via PF_ADD_PARAM | no | silent | no | none | no (inf.) | — |
| SEQUENCE_SETUP | silent | no | `time_step` is 0 unless constant for all frames | yes | allocate / yes | silent | no |
| SEQUENCE_RESETUP | silent | no | silent | yes | flat or live in, live out / yes; render-only bit in `in_flags` | silent | no |
| SEQUENCE_FLATTEN | silent | no | silent | no | live in, flat out / yes | silent | no |
| GET_FLATTENED_SEQUENCE_DATA | silent | no | silent | no | live, untouched; flat copy out / no | silent | no |
| SEQUENCE_SETDOWN | silent | no | silent | no | freed / yes | silent | no |
| FRAME_SETUP | all except input layer | no | all; `time_step` correct | yes | live (const under MFR) / yes | yes (inf.) | no |
| RENDER | all, including `params[0]` | yes | all `in_data` valid | yes | live (const under MFR unless MUTABLE…SLOWER) / yes | yes | no |
| FRAME_SETDOWN | silent | no | `time_step` correct | silent | live / yes | silent | no |
| SMART_PRE_RENDER | **none** | no | host fills (inf.) | not listed; host SHOULD fill, since SmartFX plug-ins also scale pixel-distance params (inf.) | const under MFR / no | params via PF_CHECKOUT_PARAM; layers via the pre-render `checkout_layer`, which returns bounds, not pixels ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)) | no |
| SMART_RENDER, SMART_RENDER_GPU | **none** | via checkout | host fills (inf.) | as pre-render (inf.) | const under MFR / no | params; layers only as declared in pre-render | no |
| GPU_DEVICE_SETUP / SETDOWN | no | no | — | no | none | no (inf.) | — |
| AUDIO_SETUP, AUDIO_SETDOWN | silent | no | sample fields | no | writable / yes | silent | no |
| AUDIO_RENDER | silent | no | all `in_data` valid | no | writable / yes | silent | no |
| EVENT | non-layer | no | silent (inf.: current UI time) | no | writable (UI side) / no | non-layer only | yes, on click, drag and keydown |
| USER_CHANGED_PARAM | valid | no | `current_time` = time being viewed | no | writable / no | yes | **yes** |
| UPDATE_PARAMS_UI | valid | no | silent | no | **no writes** / no | **never** | no (cosmetic only) |
| DO_DIALOG | non-layer | no | silent | no | writable / yes | yes | no (arb changes ignored) |
| ARBITRARY_CALLBACK | no | no | — | no | — | only if `effect_ref` is non-NULL | — |
| QUERY_DYNAMIC_FLAGS | not passed | no | silent | no | read / no | non-layer params | no |
| GET_EXTERNAL_DEPENDENCIES, COMPLETELY_GENERAL | silent | no | silent | no | silent / no | silent | no |

### 4.3 Time fields: deriving AE ticks from BAE flicks

The five time fields share one unit, `time_scale` ticks per second ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). Their values depend on the frame, layer and comp durations, the comp frame rate, time stretch, time remapping, the outer comp, and the comp's "preserve frame rate when nested" (PFR) switch ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)).

| Field | AE semantics | BAE derivation | Risk |
|---|---|---|---|
| `time_scale` | Units per second. Depends only on the comp frame rate. For a nested comp with PFR off it stays at the inner comp's value ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). Hosts may use different scales, so plug-ins must use ratios ([SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)). | `AeTimeBase` built from the layer's own comp rate with subdivisions, kept stable per instance (`time.rs`). Its overflow bound MUST cover the whole **layer** time range (source duration, negative layer times, remap-extended duration), not just the comp duration (inference). | Plug-ins that hard-code AE's own scale values (bad practice, but it exists). |
| `current_time` | Time of the frame, in **layer** time rather than comp time. Can fall between frames and, since CS3, can be negative ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)); the older Tips & Tricks essay still says "non-negative". It is quantized to `time_step` unless an outer comp renders with PFR off. With PFR on, the effect is isolated from the outer comp, so successive requests may skip frames or go backwards ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). For USER_CHANGED_PARAM it is the time being viewed ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Layer-local flicks converted with `to_ae`. Time remapping sits **below** the effect and changes only `total_time` ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). | Sub-tick request times get rounded (`Exactness::Rounded`). |
| `time_step` | Duration of the source frame being rendered. Can be zero, negative, or different every frame. It is 0 in SEQUENCE_SETUP unless constant and correct in FRAME_SETUP/SETDOWN. Use it for checkout times ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). With PFR off it is the time to the next **outer** comp frame, expressed in layer time ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). | Needs the 04 request to carry the frame duration of the *requesting* context, mapped into layer time (reservation). | It cannot be derived from the comp rate alone. |
| `local_time_step` | Frame spacing within the layer, based on the comp frame rate and layer stretch. Constant over the layer's frames ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). Only time stretch affects it; never zero, negative when the stretch is negative. Equal to `time_step` unless an outer comp renders with PFR off ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). | Comp frame duration scaled by the signed stretch factor. | Non-integral stretch gives a step that is not a whole number of ticks. |
| `total_time` | Layer duration. With time remapping on, the larger of the layer and comp durations ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). Stretch rules contradict each other: the PF_InData page says it is adjusted only when stretched beyond 100%, while Tips & Tricks says stretch never affects it. | Layer source duration, the remap flag and the comp duration from 02. | Open question (§4.12); test against AE. |

Further time obligations:

- **Plug-in time requests.** Checkout callbacks take a (time, step, scale) triple at the plug-in's own scale. The host MUST convert them exactly to flicks and reject a non-positive scale. PF_CHECKOUT_PARAM declares its scale as a signed type, while the audio checkout, the SmartFX `checkout_layer` and the param-util comparisons declare it unsigned ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/), [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/), [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)), so the shim MUST validate before converting into the host's unsigned time type.
- **Footage frame choice.** For footage, AE returns the nearest frame **to the left** of the requested time. Comps are evaluated at the sub-frame time, and frame blending gives an interpolated frame ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). So frame choice MUST round down (floor), while comps and params use the exact time.
- **Unit facts the host must not break.** The frame number is `current_time / time_step`, and seconds are `current_time / time_scale` ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). The host MUST therefore keep all five fields in one scale per call.
- **Timecode.** Plug-ins with `PF_OutFlag2_I_USE_COMP_TIMECODE` re-render when the comp start time or drop-frame setting changes ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). The time-formatting suites need the display preferences (§4.10).
- **Intrinsic frame rate is invisible.** A layer's own frame rate appears in no time field; plug-ins that need real source frames use `local_time_step` in a comp of matching rate ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). The host MUST NOT try to "help" by snapping `local_time_step` to the source rate.
- **Rounding direction.** `current_time` MUST stay on the exact frame-boundary tick for frame-aligned requests, otherwise `current_time / time_step` names the wrong frame (inference). Frame-aligned flicks convert exactly with the `time.rs` base; only sub-frame motion-blur or remap times may round.

### 4.4 Downsampling, pixel aspect ratio and buffer geometry

| Topic | AE behaviour | Host obligation | BAE |
|---|---|---|---|
| Downsample | Point params and layer param sizes are pre-adjusted. Pixel-distance sliders are not, so a 4 px blur at 1/2 means 2 px ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). | Pre-scale point params by downsample and PAR, since they are in input-buffer coordinates ([SDK: Effect Details › Pixel Aspect Ratio](https://ae-plugins.docsforadobe.dev/effect-details/pixel-aspect-ratio/)). Never scale sliders. | 02 param semantics (point vs distance), 04 |
| PAR | AE stretches horizontally, and pixel units mean vertical pixels ([SDK: Effect Details › Pixel Aspect Ratio](https://ae-plugins.docsforadobe.dev/effect-details/pixel-aspect-ratio/)). | Pass PAR per layer, and per checked-out world in its descriptor. | 02 |
| Legacy output resize | FRAME_SETUP returns the output size and an origin that maps input into output ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). Later effects see the shift in `pre_effect_source_origin` ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). | Represent every intermediate as (pixels, rect in layer space). Derive both origins from the rects. | 04 |
| Expanding buffers | Expanding effects should clamp output at 30,000×30,000 px ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). Expansion "drastically reduces caching efficiency" ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | SHOULD reject or clamp absurd sizes before allocating (inference). | 04 |
| Trimmed or null input | With `PF_OutFlag2_DOESNT_NEED_EMPTY_PIXELS` the input is shrunk to exclude empty pixels (zero alpha, or zero RGBA with REVEALS_ZERO_ALPHA), and the trim shows up in `pre_effect_source_origin`. With that flag plus I_EXPAND_BUFFER, a completely empty input may arrive as a **null** buffer ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | MAY trim (it saves CPU-side conversion for nodes that use AE plug-ins). If it trims, the shift MUST be reflected in the origin fields. SHOULD NOT pass null input even where allowed, to avoid untested plug-in paths (inference). | 04 |
| Output contents on entry | Plug-ins must not assume a clean output buffer; the Gaussian Blur sample clears it itself ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). | MAY hand over an uncleared output world (saves a fill per frame). Debug builds SHOULD poison it to catch plug-ins that rely on zeroes (inference). | 04 |
| Writes into the input | `PF_OutFlag_I_WRITE_INPUT_BUFFER` means the plug-in writes into its input buffer ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | MUST give such plug-ins a private copy of `params[0]`, never a cached or shared buffer (inference). | 04 |
| Pixel-centre convention | AE rotates around the top-left corner of the top-left pixel, while the subpixel and area samplers treat integer coordinates as pixel centres; the matrix transform does not have this offset ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). | The host sampling suites MUST reproduce both conventions exactly (§4.10). | 04 |

### 4.5 Global, sequence and frame data handles

| Handle | Scope and persistence | Who writes and when | Resize allowed | Host obligation | BAE |
|---|---|---|---|---|---|
| `global_data` | One per module. Never saved ([SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)). | Plug-in at GLOBAL_SETUP and SETDOWN. Read-only elsewhere for MFR safety ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | Guide silent. | Pass it back in `in_data` on every call ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). Out of process, it lives only in the plug-in process. | 19 |
| `sequence_data` | One per instance. Saved in flat form ([SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)). | Contents may change in any selector. Not at render time for MFR plug-ins (const through `PF_EffectSequenceDataSuite1`), unless MUTABLE…SLOWER gives each thread a copy ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | Only in AUDIO_SETUP/SETDOWN/RENDER, FRAME_SETUP/SETDOWN, RENDER, SEQUENCE_SETUP/SETDOWN/FLATTEN/RESETUP and DO_DIALOG ([SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)). | Re-read the handle and its size after each of those selectors. Render-time writes by legacy plug-ins are not propagated back, as in AE since 13.5 ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | 02 opaque state slot; 04 snapshots |
| `frame_data` | One per legacy frame. Never written to disk ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). SmartFX may use it too ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). | Plug-in at FRAME_SETUP. | — | Pass it to RENDER and FRAME_SETDOWN. The plug-in cannot free it during RENDER. | 04 per-request |
| `pre_render_data` | One per SmartFX request. | Plug-in at pre-render; owned by the host after return. | — | Free it with the plug-in's deleter, even if render never runs ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). | 04 plan payload |

### 4.6 PF_OutData fields the host must read

AE inspects `PF_OutData` after every call ([SDK: Effect Basics › Entry Point](https://ae-plugins.docsforadobe.dev/effect-basics/entry-point/)).

| Field | Written during | Host obligation | BAE |
|---|---|---|---|
| `my_version` | GLOBAL_SETUP ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | Store it. AE uses it to choose between duplicate copies of an effect ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). It is part of the disk-cache key alongside the host build ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). SHOULD check it against the PiPL version (inference). | 19 registry dedupe; 04 cache key |
| `name` | — (unused) | Ignore. | — |
| `global_data` | GLOBAL_SETUP and SETDOWN. | Store per module and return it on every call. | 19 |
| `num_params` | PARAM_SETUP. | Validate against params added plus the input layer. | 02 |
| `sequence_data` | SETUP, RESETUP, FLATTEN (flat), and selectors allowed to resize. | Pre-fill it with the current handle so a plug-in that leaves it alone keeps its data, then re-read it after the call (inference). | 02 slot |
| `flat_sdata_size` | — (unused; AE knows the size from its allocator). | The host allocator MUST track the size of every handle. | 19 |
| `frame_data` | FRAME_SETUP (and SmartFX). | Pass it on to RENDER and SETDOWN. | 04 |
| `width`, `height`, `origin` | FRAME_SETUP, when the output size differs. `origin` is where the input lands in the output ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Allocate the output and report the node's output rect. 0×0 means ignore the next RENDER ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | 04 |
| `out_flags`, `out_flags2` | GLOBAL_SETUP (static capabilities); QUERY_DYNAMIC_FLAGS (dynamic subset); GPU_DEVICE_SETUP (GPU flags per device); RESETUP (SEND_DO_DIALOG); message flags after EVENT, RENDER, DO_DIALOG, USER_CHANGED_PARAM and FRAME_SETUP. | Decode statically once and dynamically per answer (`caps.rs`). Act on the message flags in §4.7. | 04 / 19 |
| `return_msg` | Any selector. It is checked and cleared after every selector ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Show it after the call. ABOUT shows it in a modal. With DISPLAY_ERROR_MESSAGE it becomes an error dialog and a render-log entry ([SDK: Effect Basics › Errors](https://ae-plugins.docsforadobe.dev/effect-basics/errors/)). Decode as UTF-8 only if `PF_OutFlag2_STRINGS_ARE_UTF8` is set, otherwise in the locale encoding ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). In headless or preview renders, send it to the log only (AE suppresses reporting there, [SDK: Effect Basics › Errors](https://ae-plugins.docsforadobe.dev/effect-basics/errors/)). | 11/12 dialogs; render log |
| `start_sampL`, `dur_sampL`, `dest_snd` | AUDIO_SETUP (span) and AUDIO_RENDER (output) ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Honour the requested span and take the output sound world. | Audio |

### 4.7 Message flags read after non-setup selectors

| Flag | Set during | Host action | BAE |
|---|---|---|---|
| `PF_OutFlag_FORCE_RERENDER` | USER_CHANGED_PARAM, and click/drag events only when GET_FLATTENED is implemented ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Invalidate this instance's cached frames and push a new sequence-data snapshot to the render side. | 04 invalidation plus snapshot |
| `PF_OutFlag_REFRESH_UI` | EVENT, RENDER, DO_DIALOG ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Redraw the Effect Controls, comp and layer views. When set from a render thread, marshal to the UI thread. | 11/12 |
| `PF_OutFlag_DISPLAY_ERROR_MESSAGE` | Any selector. | Show `return_msg` as an error and log it. | 11/12 |
| `PF_OutFlag_NOP_RENDER` | FRAME_SETUP. "Invalidates the current render" ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Skip the RENDER result. Whether to pass input through or output empty is open (§4.12 Q6). | 04 |
| `PF_OutFlag_SEND_DO_DIALOG` | RESETUP, according to the guide ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Send DO_DIALOG after SETUP. | 11/12 |
| Param `PF_ChangeFlag_CHANGED_VALUE` (not in `out_data`) | USER_CHANGED_PARAM; DO_CLICK, DRAG and KEYDOWN events ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | Apply as an undoable value change, which also re-renders ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). Not supported for layer params. Since 14.0 it does not trigger a re-render for layer or path params ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | 02 Command |
| `PF_OutFlag2_I_MIX_GUID_DEPENDENCIES` with GuidMixInPtr | SmartFX pre-render. Mixes extra state into the cached frame's GUID ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Append the mixed-in bytes to the 04 cache key for that request. Adobe prefers this (and arb data or CHANGED_VALUE) over FORCE_RERENDER because the cached frames stay reusable after an undo; it also replaces OUTPUT_IS_WATERMARKED and DO_DIALOG-driven invalidation ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | 04 cache key |
| `PF_OutFlag_DISPLAY_ERROR_MESSAGE` vs plain `return_msg` | Any selector. A non-empty `return_msg` is always shown; the flag only makes it an error dialog ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). Tryout "nag" messages use this path. | Show non-error messages as notices that never block a headless render (inference). | 11/12 |

### 4.8 Return codes and error reporting

| Plug-in result | AE behaviour | Host action | BAE |
|---|---|---|---|
| Success | — | Use the results, unless an abort or progress callback returned an error during the call ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | 04 |
| `PF_Interrupt_CANCEL` | Returned after a user abort. Passing it on prevents misrendered frames from being cached ([SDK: Effect Basics › Errors](https://ae-plugins.docsforadobe.dev/effect-basics/errors/)). | Drop the frame and never cache it. | 04 |
| `PF_Err_OUT_OF_MEMORY` | Expected during RAM preview, and never reported to the user ([SDK: Effect Basics › Errors](https://ae-plugins.docsforadobe.dev/effect-basics/errors/)). | Purge caches and retry once (inference). Otherwise fail the frame silently. | 04 cache budget |
| Other `PF_Err_*` | The plug-in reports its own errors through `return_msg`. Callback errors were already reported by the host ([SDK: Effect Basics › Errors](https://ae-plugins.docsforadobe.dev/effect-basics/errors/)). | Mark the node as errored, do not cache, log. Host callbacks MUST report their own failures before returning an error code. | 04 / 11 |
| Crash or hang (no AE precedent) | — | In-process: the app goes down. Out-of-process (19): restart the plug-in host and rebuild instances from slots (§3.11). | 19 |

### 4.9 Interaction callbacks (`in_data->inter`)

Plug-ins call these through macros ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). Citations in this table are that page unless noted. The guide documents only the callbacks below; it is silent on any other members of the `inter` table, so P3 MUST fill every slot the headers define with at least an error-returning stub (inference). Checkouts can re-enter the plug-in (another instance of the same module renders to satisfy the checkout), so plug-ins must not use globals or statics; the host MUST tolerate that recursion (§3.13).

| Callback | Legal in | Plug-in expects | Host MUST | BAE | Hot |
|---|---|---|---|---|---|
| `PF_ADD_PARAM` (+ convenience macros) | PARAM_SETUP | Each call adds one parameter definition. Definitions must be fully cleared first. | Deep-copy each definition. Reject the call outside PARAM_SETUP. Map types and flags into the 02 schema. | 02 / 19 | N |
| `PF_ABORT` | Long operations, especially render | Returns non-zero when the user cancelled; the plug-in returns that value. | Be **O(1), lock-free and thread-safe**, polling the request's cancel token. Out of process, read a shared-memory flag rather than doing IPC (inference). | 04 cancel token | Y |
| `PF_PROGRESS` | Same as PF_ABORT | Reports current and total progress. Non-zero means stop. With total 0 it behaves like ABORT. Called about once per scanline. | Be O(1). Aggregate progress across threads and throttle UI updates. Remember any abort so the output can be discarded (§4.8). | 04 plus 11/12 progress | Y |
| `PF_CHECKOUT_PARAM` | RENDER, SmartFX, QUERY_DYNAMIC_FLAGS, DO_DIALOG, USER_CHANGED_PARAM. **Not** in UPDATE_PARAMS_UI ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)), not for layer params during UI events, not when `effect_ref` is NULL ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). | Value of a param, or the layer's pixels, at (time, step, scale), as a read-only copy. See the checkout semantics below the table. | Evaluate animation and expressions at any time (03). Render layers synchronously and possibly recursively (§3.13). **Record** each (param, time) as a cache dependency, as WIDE_TIME_INPUT and AUTOMATIC_WIDE_TIME_INPUT require; AUTOMATIC only works for SmartFX, and a legacy plug-in setting it is treated as plain WIDE_TIME_INPUT (any upstream change at any time invalidates) ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). AE bases caching on checkout state, so the record is mandatory, not an optimisation. Adjust point params for the pre-effect origin at the *current* time ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). | 02 / 03 / 04 | Y |
| `PF_CHECKIN_PARAM` | After each checkout | Must balance every checkout, even after errors ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). The data is invalid after check-in. | Release the copy. SHOULD auto-release leftovers when the selector returns, and log them (inference). | 04 | Y |
| `PF_REGISTER_UI` | PARAM_SETUP ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)) | Registers a custom UI area. Alignment flags are not honoured. | Record the areas: Effect Controls, comp view, layer view. | 11/12 | N |
| `PF_CHECKOUT_LAYER_AUDIO` | Effects with I_USE_AUDIO ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)) | Audio for (layer index, start, duration, scale, rate, bytes per sample, channels, signedness), as an opaque handle. | Resample to the requested format. Record the audio dependency for caching (inference). | Audio / 04 | Y |
| `PF_CHECKIN_LAYER_AUDIO` | After each audio checkout | Must balance every checkout regardless of errors. | Release; auto-release leftovers. | Audio | Y |
| `PF_GET_AUDIO_DATA` | While audio is checked out | Sample pointer, count, rate, size, channels and signedness. Every output is optional ([SDK: Audio › Accessing Audio Data](https://ae-plugins.docsforadobe.dev/audio/accessing-audio-data/)). | Accept null output pointers. | Audio | Y |

`PF_CHECKOUT_PARAM` semantics the host must reproduce:

- A layer param set to none returns zeros.
- Masks are not applied to checked-out layers.
- Checking out a layer gives a **normal** effect the raw source without its effects. A **SmartFX** effect checking out `params[0]` gets the output of the preceding effects.
- Interlaced sources come back deinterlaced.
- Times between frames: comps are evaluated at the sub-frame time, and footage returns the nearest frame to the left, or a blended frame if frame blending is on.
- Frames are valid over the whole source even if the layer is trimmed, and transparent outside it.
- Layers with a lower frame rate change only at their own rate.
- Continuously rasterized Illustrator layers come back as comp-sized, transformed images.

### 4.10 Utility callbacks (`in_data->utils`) and the utility suites

The guide does not document the layout of the `utils` table. It says only that it holds graphics and math callbacks and is always defined ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). The suites name the legacy macros they replace:

- handle macros ([SDK: Effect Details › Memory Allocation](https://ae-plugins.docsforadobe.dev/effect-details/memory-allocation/))
- ANSI math and string macros ([SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/))
- colour-conversion macros ([SDK: Effect Details › Color Space Conversion](https://ae-plugins.docsforadobe.dev/effect-details/color-space-conversion/))

The Premiere page lists PF_COPY, PF_FILL, `utils->convolve` and PF_GET_PIXEL_DATA8 as equivalents of suite calls ([SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)). The host therefore MUST provide **both** the legacy `utils` table and the suites, backed by a single implementation (inference). The guide recommends that plug-ins use these host functions because they are multi-processor and hardware-accelerated ([SDK: Effect Details › Effect Details](https://ae-plugins.docsforadobe.dev/effect-details/effect-details/)), so their speed is part of the plug-in's speed.

| Group | Functions | Host obligation | BAE | Hot |
|---|---|---|---|---|
| Memory: `PF_HandleSuite1` and the legacy handle macros | New, lock, unlock, dispose, get size, resize. Plug-ins are told to use host memory for any sizeable allocation, so the host knows when to free cached images, especially in low-memory situations such as RAM preview ([SDK: Effect Details › Memory Allocation](https://ae-plugins.docsforadobe.dev/effect-details/memory-allocation/)). | Draw on the host budget so the host can free caches under pressure. Return OUT_OF_MEMORY instead of aborting. Track sizes. Handles MAY be non-moving, with lock returning a stable pointer (inference). | 04 cache budget, 01 allocator | W |
| Worlds: `PF_WorldSuite2` | New and dispose world in 8-bit, 16-bit or float ARGB; query pixel format ([SDK: Effect Details › Image Buffer Management Functions](https://ae-plugins.docsforadobe.dev/effect-details/image-buffer-management-functions/)). | Allocate padded rows; alignment is not guaranteed ([SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/)). Count against the budget. | 04 CPU buffers | Y |
| Graphics: transform, fill/matte, sampling, batch sampling | Copy, blend, convolve, composite, transfer, transform, fill, premultiply, nearest/subpixel/area sampling ([SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/)). | Match AE behaviour: dependence on quality, and subpixel sampling that treats (0,0) as the centre of the top-left pixel ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). | 04 (CPU kernels) | Y |
| Iteration: 8-bit, 16-bit and float iterate suites | Multi-threaded per-pixel iteration with built-in progress and abort, scaling to the CPU core count ([SDK: Effect Details › Iteration Suites](https://ae-plugins.docsforadobe.dev/effect-details/iteration-suites/)). | Use the 04 thread pool without oversubscribing (inference). The suites check abort and report progress themselves, against the caller's progress base and final values, so several passes add up to one bar ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)); the host MUST honour those values and stop iterating on cancel. In generic iteration the plug-in may call abort and progress only from thread index 0 ([SDK: Effect Details › Iteration Suites](https://ae-plugins.docsforadobe.dev/effect-details/iteration-suites/)), so the host MUST run index 0 such that those calls are safe (inference). | 04 | Y |
| ANSI math and string | Math functions, sprintf and strcpy ([SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/)). | Implement locally, and inside the out-of-process shim too (never over IPC). | — | Y |
| Colour conversion callbacks | RGB to and from HLS and YIQ; luminance, hue, lightness, saturation ([SDK: Effect Details › Color Space Conversion](https://ae-plugins.docsforadobe.dev/effect-details/color-space-conversion/)). | Exact 8-bit fixed-point semantics. | 06 | Y |
| `PF_EffectUISuite` | Rename the Options button; PARAM_SETUP only, with a short fixed maximum name length ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | Store per module. | 12 | N |
| `PF_AppSuite` | Background and UI colours, UI language, user info, UI fonts, set cursor, is-render-engine, colour picker (result in the project working space), mouse position, invalidate rect, local-to-global point conversion ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | Theme colours and language from 11; the language query returns one of a fixed set of locale tags, so BAE MUST map its zh/en UI to the matching tags. Return **empty** personal info (privacy). Is-render-engine is true for headless `bae render` (AE: render-engine installs, no-UI runs and watched-folder mode). The picker returns cancel as `PF_Interrupt_CANCEL` and its colour in the project working space, so it converts through 06. Invalidate-rect is legal only while handling a non-drawing event, and local-to-global only during UI events; outside those the host MUST return an error (inference on the error). | 11/12, 06 | N |
| `PF_AdvAppSuite2` | Set project dirty, save project, background-state helpers, bring to front, refresh all windows, Info panel text and colour ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | Route save and dirty through app actions or Commands, never direct writes. Window-order helpers MAY do nothing. | 02 / 11 | N |
| `PF_AdvTimeSuite4` | Format time for the active item, layer or comp; display preferences (start frame, drop-frame, feet+frames); count frames ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | Read the comp display settings from 02 and format exactly as BAE's UI does. | 02 / 11 | N |
| `PF_AdvItemSuite1` | Step the current time, touch the active item, force re-render, ask whether the effect is active or enabled ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | Move the current-time indicator (12). Force re-render becomes 04 invalidation of the instance. Active-or-enabled MUST report false for a disabled effect, since AE stops listening to calls from it ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | 12 / 04 | N |
| `PF_ChannelSuite1` | Auxiliary channels such as depth, normals, object ID and motion vectors: count, describe, check out, check in ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | MAY report zero channels in P3. Must check in every chunk. | 08 (later) | W |
| `PF_EffectSequenceDataSuite1` | Read-only sequence data at render time ([SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)). | Return the snapshot belonging to this render. | 04 | Y |

### 4.11 Risks for the P3 implementation

| Risk | Detail | Mitigation |
|---|---|---|
| The `utils` table layout is undocumented | The guide never lists it, so P3 needs the SDK headers for the struct, which raises a licensing question (ARCHITECTURE §15). | Settle the legal review before P3. Keep the design in BAE-owned types (`suites.rs`). |
| Callbacks from worker threads | Plug-ins call `PF_ABORT`, `PF_PROGRESS` and the iterate suites from their own threads. | Resolve `effect_ref` to a thread-safe request context; no thread-local assumptions. |
| IPC cost out of process | Per-scanline abort, per-pixel sampling and per-evaluation arb INTERP would each cross a process boundary. | Shared-memory flags, local implementations in the shim, interpolation results cached per keyframe pair. |
| Time-field fidelity | `time_step` and `total_time` depend on PFR, stretch and remapping, and the guide contradicts itself on stretch. | Golden tests against AE using a probe plug-in that logs `in_data` (P3; built from the public sample approach, no SDK code in the repository). |
| Downsample or PAR of zero | Some hosts send zero, and plug-ins divide by it ([SDK: Effect Details › Pixel Aspect Ratio](https://ae-plugins.docsforadobe.dev/effect-details/pixel-aspect-ratio/)). | Assert non-zero rationals in the 04 request. |
| Out-of-memory storms in preview | AE expects OOM from plug-ins during RAM preview ([SDK: Effect Basics › Errors](https://ae-plugins.docsforadobe.dev/effect-basics/errors/)). | Plug-in allocations use the cache budget, with purge-and-retry. |
| Guide and newer behaviour disagree | The PF_InData page still calls `in_flags` unused and Tips & Tricks still calls `current_time` non-negative, while What's New and the PF_InData notes say otherwise. | Follow the newest statement; verify each field with the probe plug-in before P3 ships. |
| Wrong `extent_hint` | A legacy plug-in that sets USE_OUTPUT_EXTENT leaves pixels outside the hint undefined; a hint that is too small corrupts cached frames silently. | Derive hints from the same 04 visible region that keys the cache; never shrink below the request. |
| `in_data` sized for a newer SDK | A plug-in compiled against a newer SDK may read fields the host does not fill (inference). | Zero-initialise the whole structure; claim only the `version` BAE implements. |

### 4.12 Open questions

| # | Question | Why it matters | Proposed resolution |
|---|---|---|---|
| Q1 | What `appl_id` and `version` should BAE report? | Plug-ins use them for licensing and feature gating; impersonating AE may break licences or terms, while a new ID may disable features ([SDK: Introduction › Third-party Plug-in Hosts?](https://ae-plugins.docsforadobe.dev/intro/third-party-plug-in-hosts/)). | Legal and product decision before P3; make it a per-plug-in override. |
| Q2 | `shutter_angle` units: the guide says the range maps to 360 degrees and also uses 180 as a half-frame example ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). `shutter_phase` is called a "percentage" with no encoding. | Wrong units give wrong motion blur. | Measure with a probe plug-in in AE. |
| Q3 | Does time stretch change `total_time`? The PF_InData page and Tips & Tricks disagree ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). | Plug-ins that normalise by duration render differently. | Measure in AE. |
| Q4 | Audio span field names: the guide uses both `startsampL`/`endsampL` and `start_sampL`/`dur_sampL` ([SDK: Audio › Accessing Audio Data](https://ae-plugins.docsforadobe.dev/audio/accessing-audio-data/), [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | Start + end vs start + duration changes the span the host renders. | Use the header definitions in P3 (legal review permitting). |
| Q5 | SEND_DO_DIALOG is set during RESETUP but the dialog follows SETUP ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | The host must know which selector's flag word to read. | Read it after both; send DO_DIALOG only for a fresh application. |
| Q6 | NOP_RENDER "invalidates the current render": pass input through, or output empty? | Visible difference in the comp. | Test in AE. |
| Q7 | Should USER_CHANGED_PARAM fire for script, agent, expression-driven or remote (CRDT) edits? AE sends it for user edits; undo does not trigger it (community report). | Supervised plug-ins keep derived params in sync only if told. | Send it for local Command-originated edits; never for undo/redo or remote replay (they carry the plug-in's earlier edits already). |
| Q8 | Does AE re-send GLOBAL_SETUP and PARAM_SETUP after GLOBAL_SETDOWN without unloading? | The tracker currently forbids it. | Allow it in the tracker; verify in AE. |
| Q9 | How does AE learn the SDK version used to build an MFR plug-in ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/))? The PiPL carries the spec version ([SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/)). | Gating concurrency on a false claim crashes plug-ins. | Gate on the PiPL spec version; confirm with headers. |

## 5. Parameters

AE parameters are time-varying streams owned by the host: the input image, sliders, angles, points, colours, paths and plug-in-defined data. AE interpolates them; the plug-in only reads values ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). For the compat host this makes the parameter layer the widest surface after pixels. Every value, keyframe, expression, UI state and identity rule here has to be representable in BAE's document model (02), evaluated by 03, scheduled by 04 and shown by 12, without the core knowing about AE.

Legend (as in §3): **MUST/SHOULD/MAY** are host obligations for the P3 compat host. "(inference)" marks a statement the guide does not make. "Guide silent" means the guide does not address the point. BAE owners: 02 doc-model, 03 animation/expressions, 04 render graph, 06 colour, 12 Effect Controls/timeline UI, 15 AE project import, 19 plug-in system. Pixel-side checkout details are in §8 and are only referenced here.

### 5.1 Definition lifecycle and the `params[]` array

| # | AE behaviour | Host obligation | BAE | P3 risk |
|---|---|---|---|---|
| L1 | Params are declared in `PF_Cmd_PARAM_SETUP` with one `PF_ADD_PARAM` per param. `PARAM_SETUP` is a *global* selector, sent after `GLOBAL_SETUP` when the plug-in is first applied, not once per instance ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | MUST treat the param list as a property of the effect *type* (module + match name + version) and cache it. Instances differ only in values and runtime UI state (§5.13). | 19 registry holds the definition; 02 snapshots it on the Effect Prim for placeholders (R02-06). | A plug-in that varies its param list by licence state or environment produces different lists on different machines (inference). The host SHOULD fingerprint the list and warn on mismatch against the project snapshot. |
| L2 | The plug-in sets `out_data->num_params`, and AE checks it against the number of `PF_ADD_PARAM` calls plus the implicit input layer ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | MUST validate the count. On mismatch it MUST fail the effect type cleanly. Guide silent on what AE does. | 19 | — |
| L3 | `params[0]` is always the input layer. The other params follow in registration order, and that order is the display order in Effect Controls and Timeline ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/), [SDK: Effect Details › Changing Parameter Orders, the Nice Way](https://ae-plugins.docsforadobe.dev/effect-details/changing-parameter-orders/)). | MUST keep an index ↔ disk-id map per definition version. Every index-based plug-in call (checkout, supervision, keyframe utils, `PF_UpdateParamUI`) is translated through it. | Host-side map; 02 stores by id only. | — |
| L4 | The array describes the params "at the current time", and its values are valid only for some selectors ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/), [SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). SmartFX gets **no** param array in pre-render or render and must use `PF_CHECKOUT_PARAM` for non-layer params ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). | MUST fill every non-layer param with the value at `current_time`, after expressions (inference). MUST build a fresh array per call, private to that call when calls run concurrently (inference). | 03 `EvaluatedComp` effect param values; 04 passes them with the request. | Per-call marshalling cost for effects with hundreds of params. The host SHOULD reuse arrays per (instance, time, snapshot). |
| L5 | Plug-ins are told to clear each def before registering it, because stale union bytes "cause many problems" ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/), [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). | MUST read only the union member that `param_type` selects. MUST zero every def the host builds. | — | Garbage in unused fields of badly written plug-ins. |
| L6 | The guide's member table lists the disk id and `change_flags` as separate members, but other pages address them as `PF_ParamDef.uu.id` and `uu.change_flags` ([SDK: Effect Details › Changing Parameter Orders, the Nice Way](https://ae-plugins.docsforadobe.dev/effect-details/changing-parameter-orders/), [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). The name `uu` suggests one storage slot with two meanings (inference). | MUST read the disk id only from the defs added at `PARAM_SETUP`. MUST clear the change-flags slot before every selector in which the plug-in may set it, and MUST NOT interpret it as an id afterwards (inference). | Host | Reading a stale id as change flags gives phantom edits. |
| L7 | No documented maximum param count. The guide jokes about "38 kajillion" ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). Disk ids should lie in 1–9999 ([SDK: Effect Details › Changing Parameter Orders, the Nice Way](https://ae-plugins.docsforadobe.dev/effect-details/changing-parameter-orders/)). | MUST NOT impose a small limit. SHOULD warn, not fail, on ids outside 1–9999. | 02 list sizes unbounded; 12 Effect Controls must virtualise long lists. | Particle and colour-grading plug-ins with several hundred rows. |

### 5.2 Parameter type catalogue

Value column: what the plug-in reads in the def. Anim column: who animates it. BAE column: the 02 value shape. All standard types are interpolated by AE ("not your problem; leave it to After Effects") ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)).

| Type | Value representation | Definition data | Anim | BAE shape (02) | Host obligations and risks |
|---|---|---|---|---|---|
| `PF_Param_LAYER` | A reference to a layer in the current comp, not pixels. Image and audio layers are offered in an AE-generated pop-up ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). Pixels come only through checkout (§5.10). | Default is "myself" or "none" ([SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/)). | Guide silent. AE's UI shows no stopwatch for layer params (inference). | `LayerRef { None \| ThisLayer \| Layer(PrimId), stage }` (§5.10) | MUST resolve "myself" at apply time to a self-reference that survives duplication. `CHANGED_VALUE` is not supported for layer params ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). |
| `PF_Param_SLIDER` | Integer. "No longer used" ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | Valid range, slider range, default. | Host | `Int` (or `Float` with integer display) | MUST still be supported for old binaries (inference). Rounding of interpolated values: guide silent. |
| `PF_Param_FIX_SLIDER` | 16.16 fixed. Deprecated in favour of float sliders. Ignore the low word for integers ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | Valid and slider ranges in fixed point. Display precision. `PF_ValueDisplayFlags` ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | Host | `Float` (f64) | MUST quantise to 16.16 on delivery. Values outside the 16.16 range cannot be delivered (inference) → clamp and log. |
| `PF_Param_FLOAT_SLIDER` | Double ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | Valid and slider ranges, precision, display flags. Audio extras: `PF_FSliderFlag_WANT_PHASE`, a phase field AE fills, and a curve tolerance (0 = default, also for non-audio use) ([SDK: Audio › Audio-Specific Float Slider Variables](https://ae-plugins.docsforadobe.dev/audio/audio-specific-float-slider-variables/)). | Host | `Float` | With WANT_PHASE set, the host MUST fill phase during audio rendering. Curve tolerance tells the host how finely to subdivide time-varying params over an audio buffer. Premiere-only exponential sliders exist ([SDK: Premiere Pro & Other Hosts › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)); AE behaviour guide silent → MAY ignore the exponent. |
| `PF_Param_ANGLE` | 16.16 fixed degrees. Several revolutions allowed, so values exceed 360 ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). A float accessor exists since CS6.0.2 (`PF_AngleParamSuite1`) ([SDK: Effect Details › Parameters & Floating Point Values](https://ae-plugins.docsforadobe.dev/effect-details/parameters-floating-point-values/)). | Default. | Host | `Float` (degrees, unbounded) | MUST serve both forms consistently (§5.3). |
| `PF_Param_CHECKBOX` | Boolean. `CANNOT_INTERP` is forced on ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | Default, label ("checkbox names" are readable by AEGPs ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/))). | Host, hold only | `Bool` | MUST force hold keyframes whatever the def says. |
| `PF_Param_COLOR` | 8-bit RGB pixel, alpha unused. Float values, including over-range HDR, come through `PF_ColorParamSuite1` ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/), [SDK: Effect Details › Parameters & Floating Point Values](https://ae-plugins.docsforadobe.dev/effect-details/parameters-floating-point-values/)). | 8-bit default. | Host | `ColorRgb` (float, declared space, R06) | The legacy field is clamped: over-bright colours read as 255 ([SDK: Effect Details › Parameters & Floating Point Values](https://ae-plugins.docsforadobe.dev/effect-details/parameters-floating-point-values/)). AE's colour picker returns colours in the project working space ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). |
| `PF_Param_POINT` | 16.16 fixed in the coordinate system of the input buffer. Origin top-left, +y down. Float accessor since CS5.5 ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/), [SDK: Effect Details › Pixel Aspect Ratio](https://ae-plugins.docsforadobe.dev/effect-details/pixel-aspect-ratio/)). | Default given as percent of layer size (the guide documents this for API versions before 12.1 and is ambiguous after; samples use percent (inference)). Bounds restriction: guide silent. | Host, spatial (§5.7) | `Vec2` spatial, layer space | MUST apply downsample, PAR and upstream origin shift (§5.3). |
| `PF_Param_POINT_3D` | Three doubles. New in CS5.5, unsupported in Premiere ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | Defaults: guide silent on units. | Host, spatial | `Vec3` spatial | Downsampling and origin-shift of x/y/z: guide silent → open question. |
| `PF_Param_POPUP` | Integer selection. Entries come from a choice string. A `(-` entry draws a separator. Entries cannot change after adding unless the param is `PF_PUI_STD_CONTROL_ONLY` ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | Choice string, default. | Host, hold (inference) | `Enum` + option list incl. separators | 1-based numbering with separators taking an index: guide silent (inference, verify). The guide's sample string shows `/` between entries, which looks like a table-escaping artefact; the delimiter used by SDK samples is `\|` (inference). |
| `PF_Param_ARBITRARY_DATA` | A plug-in-owned handle ("???" in the guide's type column). An arb-type id lets one effect have several arb types, plus a default handle ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/), [SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). | In AE the param must set `PF_PUI_TOPIC`/`PF_PUI_CONTROL` or `PF_PUI_NO_ECW_UI`. Premiere ≥ 8 allows none of them and then shows only the keyframe track ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | Host keys + plug-in `INTERP` (§5.9) | `Blob { arb_type, flat bytes }` | MAY accept the Premiere variant (keyframe track only). |
| `PF_Param_PATH` | `PF_PathID`, a reference to a mask on the *same* layer. Read only through the path suites ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/), [SDK: Effect Details › Working With Paths](https://ae-plugins.docsforadobe.dev/effect-details/working-with-paths/)). | Default: guide silent. | Guide silent (no stopwatch in AE UI (inference)) | `MaskRef { None \| Mask(PrimId) }` | §5.11. |
| `PF_Param_GROUP_START` / `GROUP_END` | No value. Groups ("topics") nest, are indented and have a twirly that the user or the effect can open and close ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | Initial twirl state (§5.4). | — | Group rows | They are added as params (`PF_ADD_TOPICX` / `PF_END_TOPIC` are listed among the add macros ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/))), so they take indices and disk ids (inference). MUST check balance. |
| `PF_Param_BUTTON` | No value. A press is detected through supervision. New in CS5.5 ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | Label (guide silent on the field). | — | Action row | SHOULD send `USER_CHANGED_PARAM` on press (inference: even without `SUPERVISE`, since nothing else would notice the press. Verify). |
| `PF_Param_NO_DATA` | Only named in the `STD_CONTROL_ONLY` exclusion list ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). `PF_ADD_NULL` exists ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). | — | — | Value-less row | Semantics guide silent: treat as a placeholder or custom-UI-only row (inference). |
| `PF_Param_CUSTOM` | Only named in the same exclusion list ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | — | — | Unknown type (round-trip) | Guide silent. SHOULD refuse to host the effect type with a clear "unsupported param type" message. |
| Unknown future types | — | — | — | Unknown type tag kept verbatim (R02-01) | MUST NOT crash. Effect becomes a placeholder. |

There is no separate float-colour param type. Floating-point colour is a second *view* of `PF_Param_COLOR` ([SDK: Effect Details › Parameters & Floating Point Values](https://ae-plugins.docsforadobe.dev/effect-details/parameters-floating-point-values/)).

### 5.3 Numeric representation: dual forms, coordinates and ranges

| Topic | AE rule | Host obligation | BAE | Risk |
|---|---|---|---|---|
| Dual representations | Point, angle and colour have a legacy field (16.16 or 8-bit) **and** a float accessor (`PF_PointParamSuite1`, `PF_AngleParamSuite1`, `PF_ColorParamSuite1`). Each accessor takes a pointer to the param def ([SDK: Effect Details › Parameters & Floating Point Values](https://ae-plugins.docsforadobe.dev/effect-details/parameters-floating-point-values/)). | MUST store full precision and derive the legacy field by rounding (and clamping for colour). The float accessor MUST return the full-precision value for any def the host handed out (array entry or checkout result). | 02 stores f64/f32 only. | A plug-in may copy a def into its own memory and pass the copy (inference). The host then cannot key by pointer. Fallback: recover the value from the legacy field, or from a host-private tag hidden in the def (P3 design). |
| Quantisation-aware write-back | A supervising plug-in writes values back through the legacy fields ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | When a written legacy value equals the quantised stored value, MUST keep the stored full-precision value. Otherwise this destroys HDR colours or sub-1/65536 precision on every supervised edit (inference). Plug-ins often set `CHANGED_VALUE` on params they did not really change (inference). | Host logic; commands carry f64. | Silent precision loss if done naively. |
| Point space | Point values arrive in input-buffer coordinates, scaled for downsampling and PAR ([SDK: Effect Details › Pixel Aspect Ratio](https://ae-plugins.docsforadobe.dev/effect-details/pixel-aspect-ratio/), [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). They are also offset by the origin shift of upstream effects that resized the buffer; the UI still shows the unshifted value ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). Checked-out points use the origin shift of the *current* time, not the checkout time ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). | MUST store points unshifted, in full-resolution layer space. At delivery it MUST apply the downsample factor, PAR and the origin shift of the *current* time. Layer-param dimensions are also downsampled ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). | 02 canonical layer-space point; 04 supplies downsample, PAR and origin shift (R04-12). | An animated buffer expansion moves non-animated points ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). This is AE-faithful and must be reproduced, not "fixed". |
| AE's internal point storage | The keyframe suite notes that tangents of effect point streams "are multiplied by the layer size" and that temporal ease must be multiplied by layer height ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | — | 02 decides the canonical space. 15 converts on import. | Suggests AE stores effect points normalised to layer size (inference). If so, AE points follow a change of layer size and BAE pixel-space points would not. 15 must test footage replacement. |
| Valid vs slider range | Each slider has a valid range and a narrower slider range. Wrong fixed-point ranges give a slider that looks enabled but does not move ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | MUST clamp stored values to the valid range. The UI MUST allow typed values beyond the slider range up to the valid range (inference from the two-range model). | 02 two ranges; 12 control. | Expressions can push values outside the valid range. Whether AE clamps after expressions: guide silent. |
| Display | `PF_ValueDisplayFlag_PERCENT` appends "%". The pixel flag makes the value follow the user's pixel-value preference (0–1, 0–255, 0–32768 or 0.0–1.0). Precision is a separate display setting ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/), [SDK: Effect UI & Events › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-ui-events/tips-and-tricks/)). | Display only; MUST NOT change the stored value. | 12 respects a "pixel value display" preference. | — |
| Distances | Pixel-distance sliders are **not** auto-scaled. The plug-in scales them by the downsample factor itself ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). | MUST NOT scale scalar params. | — | — |

### 5.4 Behaviour flags (`PF_ParamFlag_*`)

The flags are set before adding the param. During event handling only `COLLAPSE_TWIRLY` may change ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)).

| Flag | Semantics | Host obligation | BAE |
|---|---|---|---|
| `CANNOT_TIME_VARY` | No keyframe control in the Timeline ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | MUST NOT allow keyframes. Expressions on such params: guide silent → SHOULD disallow until verified. | 02 attribute `animatable = false`. |
| `CANNOT_INTERP` | No algebraic interpolation. Hold keyframes still work, and it speeds rendering ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). Forced for checkboxes ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | MUST allow hold keyframes only. For arb params MUST NOT call `INTERP`. | 02 `interpolation = hold_only`. |
| `COLLAPSE_TWIRLY` | Set or cleared during `USER_CHANGED_PARAM` and `UPDATE_PARAMS_UI` to twirl params and groups up or down ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | MUST read it after those selectors and apply it to the viewing user's twirl state. | Per-user view state (§5.13). |
| `SUPERVISE` | Enables `USER_CHANGED_PARAM` for this param. Any supervised param also turns on `UPDATE_PARAMS_UI` ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/), [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | §5.8. | 19 hook. |
| `START_COLLAPSED` | Initial twirl state of a topic. Changeable during supervision. Honoured only if `PF_OutFlag2_PARAM_GROUP_START_COLLAPSED_FLAG` is set; otherwise **all groups start collapsed** ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/), [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). Premiere ignores it ([SDK: Premiere Pro & Other Hosts › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)). | MUST collapse by default unless both flags are present. | Definition field `collapsed_by_default`. |
| `USE_VALUE_FOR_OLD_PROJECTS` | When a project saved with an older plug-in version lacks this param, initialise it from the def's `value` field, not from the default. The default still applies to new or reset instances ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | MUST capture **both** `value` and the default at setup, and apply §5.6 D3. | 02 `old_project_value`. |
| `LAYER_PARAM_IS_TRACKMATTE` | Premiere only; layer params only; ignored in AE ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | MAY ignore. | — |
| `EXCLUDE_FROM_HAVE_INPUTS_CHANGED` | Relevant only with `AUTOMATIC_WIDE_TIME_INPUT` and the state calls ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). Honoured by `PF_GetCurrentState` with `CHECK_ALL_HONOR_EXCLUDE` ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | MUST leave the param out of state receipts only in that mode. | 04 state hash selection. |
| `SKIP_REVEAL_WHEN_UNHIDDEN` | On unhide, do not twirl open the parents or scroll the param into view (CS6) ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | MUST suppress auto-reveal in 12. | Definition field. |

### 5.5 UI flags (`PF_PUI_*`), custom-UI size and runtime mutability

UI flags and behaviour flags live in different fields, and mixing them up gives unpredictable behaviour ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)).

| Flag | Semantics | Host obligation | BAE (12 unless noted) |
|---|---|---|---|
| `TOPIC` | The plug-in handles events for the topic, the row that stays visible when the param is twirled up. Requires `PF_OutFlag_CUSTOM_UI` ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | MUST route title-area events and draws to the plug-in. SHOULD reject the combination if `CUSTOM_UI` is missing. | Custom region (R12-01, R19-05). |
| `CONTROL` | The plug-in handles the control area. Requires `CUSTOM_UI` ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). Usable with standard types or arb data ([SDK: Effect UI & Events › Effect UI & Events](https://ae-plugins.docsforadobe.dev/effect-ui-events/effect-ui-events/)). | As above, for the control area. | R12-01. |
| `STD_CONTROL_ONLY` | A standard control with **no data stream** and no keyframes, for example to drive sequence data or arb data. Not allowed on `CUSTOM`, `NO_DATA`, `LAYER`, `ARBITRARY_DATA` or `PATH`. Requires `SUPERVISE`; does not require `CUSTOM_UI` ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). `PF_UpdateParamUI` is the only way to update its UI ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | MUST NOT create a stream (no keyframes, no expression, nothing persisted). MUST still hold the control's current value per instance, give it to the plug-in, and update it from `PF_UpdateParamUI`. | Instance-transient UI state (§5.13). |
| `NO_ECW_UI` | No Effect Controls UI. In AE the keyframes stay visible in the Timeline; in Premiere the whole row goes ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | MUST hide the row in Effect Controls only. | Row visibility per surface. |
| `ECW_SEPARATOR` | Premiere only: a thick line above the row; changeable at runtime there ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | MAY honour. | — |
| `DISABLED` | Greyed out. Usually set from `USER_CHANGED_PARAM`. Since CS6 AE stores the disabled state back into the UI flags so the plug-in can read it later ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/), [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | MUST reflect the latest runtime disabled state in the `ui_flags` of every later def handed to the plug-in. AEGP dynamic-stream flags no longer report it ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | Instance-transient state. |
| `DONT_ERASE_TOPIC` / `DONT_ERASE_CONTROL` | AE does not erase the area before the plug-in draws ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | MUST draw the plug-in's display list over the standard background instead of clearing it. | R12-01. |
| `RADIO_BUTTON` | Premiere only, popups only: show as a radio group ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | MAY honour. | — |
| `INVISIBLE` | AE CS6+: hides the row in both Effect Controls and Timeline. Dynamic only in Premiere ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). Meant for hidden params that still affect rendering ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | MUST keep the stream (values, keyframes) and hide it everywhere. | 02 stream exists; 12 hides. |

`ui_width` and `ui_height` give the custom-UI size and apply to non-standard params only ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). Since CC 2014 AE honours a new `ui_height` passed through `PF_UpdateParamUI` ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). `PF_SetOptionsButtonName` relabels the Options button with a label of up to 31 characters, during `PARAM_SETUP` only ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)).

**What may change after setup, and how:**

| Attribute | Mechanism | Allowed in | Source |
|---|---|---|---|
| Display name | Copy of the def with a new name → `PF_UpdateParamUI` | Event handling, `USER_CHANGED_PARAM`, `UPDATE_PARAMS_UI` | [SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/), [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/) |
| Enabled / disabled | `PF_PUI_DISABLED` via `PF_UpdateParamUI` | Same | same |
| Twirl open / closed | `PF_ParamFlag_COLLAPSE_TWIRLY` on the def | `USER_CHANGED_PARAM`, `UPDATE_PARAMS_UI` | [SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/) |
| Custom UI height | `ui_height` via `PF_UpdateParamUI` (CC 2014+) | Same as name | [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/) |
| Popup entries | New choice string via `PF_UpdateParamUI`, **only** for `STD_CONTROL_ONLY` popups | Same | [SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/) |
| Hidden / shown | `AEGP_SetDynamicStreamFlag` with `AEGP_DynStreamFlag_HIDDEN`, optionally undoable. `DISABLED` cannot be set this way | Any time an AEGP call is allowed | [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/), [SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/) |
| Invisible (static flag) | Premiere only at runtime | — | [SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/) |
| Value | `CHANGED_VALUE` (§5.8) | `USER_CHANGED_PARAM`, click/drag/keydown events | [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/) |
| Effect instance name | `AEGP_SetStreamName` on the effect's stream group (undoable) | AEGP context | [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/) |

The host MUST ignore every other field changed in a def passed to `PF_UpdateParamUI` (type, ranges, defaults, ids). Guide silent on what AE does with them; ignoring them is the safe choice (inference). It MUST reject `PF_UpdateParamUI` on `params[0]`, which the guide forbids ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)).

### 5.6 Identity, ordering, defaults and names

| # | AE rule | Host obligation | BAE |
|---|---|---|---|
| D1 | The disk id is set in the def before `PF_ADD_PARAM`, and saved data is tagged with it. The plug-in may reorder params freely as long as ids stay stable. To delete a param the plug-in removes its code but never reuses its id. New params get new ids appended ([SDK: Effect Details › Changing Parameter Orders, the Nice Way](https://ae-plugins.docsforadobe.dev/effect-details/changing-parameter-orders/), [SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | MUST key values, keyframes and expressions by id. MUST rebuild the index order from the *current* definition on load. | 02 attributes keyed by param id (R02-02). |
| D2 | Ids of zero: AE checks the **first** param added. If its id is zero, AE assumes the plug-in sets no ids and numbers all params itself, sequentially from 1 ([SDK: Effect Details › Changing Parameter Orders, the Nice Way](https://ae-plugins.docsforadobe.dev/effect-details/changing-parameter-orders/)). | MUST apply this list-level rule, not a per-param fallback. A list whose first id is non-zero but has later zero or duplicate ids: guide silent → SHOULD reject the effect type, or number the zero ids deterministically and warn. | Host |
| D3 | "Change defaults? Change IDs." A project saved under an old default and loaded with a new plug-in **takes the new default**. `USE_VALUE_FOR_OLD_PROJECTS` covers params missing from old projects ([SDK: Effect Details › Changing Parameter Orders, the Nice Way](https://ae-plugins.docsforadobe.dev/effect-details/changing-parameter-orders/)). | MUST reproduce: a param left at its default follows the current default. A param that did not exist when the instance was saved gets `value` if the flag is set, otherwise the default. A param the user set keeps its value. | 02: unauthored attribute = current default; Effect Prim records the ids it knew when written (reservation). |
| D4 | Removed params: the plug-in keeps the id reserved ([SDK: Effect Details › Changing Parameter Orders, the Nice Way](https://ae-plugins.docsforadobe.dev/effect-details/changing-parameter-orders/)). What AE does with saved data for an id the plug-in no longer defines: guide silent. | SHOULD keep orphaned values verbatim (not shown, not passed to the plug-in) so downgrades and AE round trips are lossless (inference). | 02 orphan retention. |
| D5 | Names are at most 31 **bytes** plus terminator. Non-ASCII characters take several bytes ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). The encoding is the application locale's multibyte encoding, or UTF-8 when the effect sets `PF_OutFlag2_STRINGS_ARE_UTF8` (AE 27.0+). The buffer does not grow with UTF-8 ([SDK: Introduction › Localization](https://ae-plugins.docsforadobe.dev/intro/localization/), [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | MUST decode by the plug-in's declared encoding. MUST tolerate a missing terminator within the buffer and a truncated multibyte tail, without rejecting the param (inference). MUST encode names written back to the plug-in the same way. | 02 stores UTF-8. |
| D6 | The UI language can be queried (`PF_AppGetLanguage`) and plug-ins localise their own names ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | MUST answer with the BAE UI language mapped to AE's locale codes. Names therefore differ by locale; identity MUST NOT depend on names. | 11 i18n locale. |
| D7 | AEGPs and scripts reach effect params by index (`AEGP_GetNewEffectStreamByIndex`) and by match name (`AEGP_GetMatchName`) ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). How AE derives third-party param match names: guide silent (AE appears to derive them from the disk id (inference)). | SHOULD expose a stable per-param match name derived from the id, for expressions and scripts. | 03 / 15 expression references. |

### 5.7 Animation, keyframes and expressions

| Topic | AE rule | Host obligation | BAE |
|---|---|---|---|
| Who interpolates | AE, for every standard type ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). Arb data uses the plug-in's `INTERP` with a normalised position. Velocity curves are already applied to that position ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). | MUST interpolate standard types with AE semantics (linear, Bezier speed/influence, hold, roving, spatial). For arb data MUST compute the eased position and delegate. | 03 evaluator; blob lazy sample (reservation). |
| Spatial streams | Effect point streams have spatial tangents in the AEGP keyframe suite. The suite also exposes per-dimension temporal ease, keyframe flags (continuous, auto-Bezier, roving) and in/out interpolation types (linear, Bezier, hold) ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). Stream types include 2D/3D spatial and non-spatial, 1D, colour, arb, layer-id and mask-id ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | MUST give point and 3D-point params motion paths like layer position. Mapping of param types to stream types (point → 2D spatial, 3D point → 3D spatial, slider/angle/checkbox/popup → 1D, colour → colour, arb → arb, layer → layer-id, path → mask-id, rows → no data) is (inference) and must be verified. | 02 spatial attribute kind (reservation); 03. |
| Expressions | The host evaluates them. A stream without keyframes may still vary through an expression. Keyframe-suite values do not include expressions; inside an expression `value` is the keyframed value; expression text and state are readable and settable per stream ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | MUST pass post-expression values in `params[]` and in checkouts (inference). MUST record expression dependencies for caching. Expressions on arb, layer or path params: guide silent. | 03 expressions on effect params (`effect("…")("…")`). |
| Keyframe access from effects | `PF_ParamUtilSuite3` gives the keyframe count, the next keyframe from a time in a direction, keyframe index → time, keyframe value checkout and check-in (always balanced), whether a param is identical at two times, and state receipts ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | MUST implement all of them against an immutable snapshot, from any thread that may call them (inference). | 02 keyframe queries (reservation). |
| Keyframe edits from effects | Effects may add keyframes through the AEGP keyframe suite, for example colour sampling from custom UI ([SDK: Effect UI & Events › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-ui-events/tips-and-tricks/)). Each insert is an undo entry unless batched with the Start/Add/End sequence ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). `AEGP_SetStreamValue` only works on streams that do not vary over time ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | MUST map these onto 02 commands; one batch = one undo unit. | 02 commands (R02-08). |
| Stream metadata | `AEGP_GetEffectParamUnionByIndex` exposes defaults, checkbox names and popup strings (not values). Also queryable: can-vary-over-time, valid interpolations, min/max, units, the session-unique stream id, and "modified" (the `UU` reveal) ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | MUST keep the complete definition per instance, including labels and strings. "Modified" for arb params needs `COMPARE` against the default (inference). | 19 registry + 02 definition snapshot. |

**Stream-type view.** AEGPs, scripts, expressions and the AE importer (15) see params as typed streams. The guide lists the stream types and the stream-value union, but it does not map param types to them. The table below is therefore (inference) and must be checked in P3 with `AEGP_GetStreamType`, `AEGP_CanVaryOverTime` and `AEGP_GetValidInterpolations` ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)).

| Param type | Expected stream type | Expected valid interpolations | BAE attribute kind |
|---|---|---|---|
| Slider, fixed slider, float slider, angle | 1D | linear, Bezier, hold | scalar, animatable |
| Checkbox, popup | 1D | hold | scalar or enum, hold-only |
| Point | 2D spatial | linear, Bezier, hold + spatial tangents | spatial vec2 |
| 3D point | 3D spatial | as point | spatial vec3 |
| Colour | colour | linear, Bezier, hold | colour |
| Arbitrary data | arb | hold, or plug-in interpolation (perhaps the "custom" interpolation mask) | blob |
| Layer | layer id (+ stage, StreamSuite7) | none (not time-varying) | reference |
| Path | mask id | none (not time-varying) | reference |
| Group start/end, button, no-data | no data | none | row only |

### 5.8 Supervision and plug-in-initiated changes

| # | AE behaviour | Host obligation | BAE |
|---|---|---|---|
| S1 | `USER_CHANGED_PARAM` is sent when the user changes a supervised param. Its extra carries the index of that param. The plug-in may change values **and** appearance of any param ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). `current_time` is the time the user is looking at (comp time converted to layer time), which is also the time of any auto-added keyframe ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | MUST send it after the user's edit has been applied to the snapshot passed in. MUST pass the UI time as layer time. One user gesture plus all supervised follow-up changes MUST form **one** undo unit. | 02 gesture command (R02-08); 19 `on_param_changed` hook. |
| S2 | Value changes are honoured only in `USER_CHANGED_PARAM` and in click, drag and keydown events. The plug-in writes into the original def and sets `CHANGED_VALUE`. The change updates the UI and is undoable ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). The def page instead says `change_flags` is valid during drag (not click) events, `USER_CHANGED_PARAM` and `UPDATE_PARAMS_UI` ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). **The two pages contradict each other.** | MUST accept value changes in `USER_CHANGED_PARAM` and drag, SHOULD accept them in click and keydown, and MUST ignore them in `UPDATE_PARAMS_UI`, which is "only cosmetic" ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). A changed animated param MUST get a keyframe at `current_time`; a static one gets its static value (inference). | Host diff of the array after the call → 02 command. |
| S3 | `CHANGED_VALUE` is not supported for layer params ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). Since 14.0 it does not trigger a re-render for layer or path params, and `AEGP_SetStreamValue` is the documented route ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). It forces a re-render for other types and keeps cached frames reusable after Undo ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/), [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | MUST ignore `CHANGED_VALUE` on layer params. SHOULD apply it on path params (AE behaviour unclear). MUST accept AEGP stream writes for both. | Cache keyed by value, so undo restores hits (R04-08). |
| S4 | The guide documents only `PF_ChangeFlag_CHANGED_VALUE` ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | SHOULD log any other bits it sees. Their meaning is outside the guide (open question). | — |
| S5 | `UPDATE_PARAMS_UI` goes to plug-ins with `PF_OutFlag_SEND_UPDATE_PARAMS_UI` or with any supervised param. It is sent when Effect Controls opens, when time moves and when the plug-in is first loaded. Only cosmetic changes are allowed, through `PF_UpdateParamUI` with a **copy** of the def. `REFRESH_UI` is not needed ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). The plug-in must not check out params here. Since 13.5 a layer checkout in this selector returns a black frame of the right size, and `PF_GetCurrentState` returns a random receipt ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | MUST send it before first showing an instance's controls, after project load, and on time changes while the controls are visible. SHOULD coalesce during scrubbing (inference). MUST reproduce the 13.5 "seatbelts". Premiere skips it on time moves without keyframes unless `NON_PARAM_VARY` ([SDK: Premiere Pro & Other Hosts › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)); BAE MAY do the same as an optimisation, at some fidelity risk. | 19 `on_refresh_param_ui` hook; UI thread only. |
| S6 | `DO_DIALOG` may read non-layer params, check out params at other times and update UI, but must not change values ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). AE ignores arb changes made there ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). The dialog response is not undoable ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | MUST discard value changes made during `DO_DIALOG`, including arb changes. | — |
| S7 | Effects can show or hide params with `AEGP_SetDynamicStreamFlag` (`HIDDEN`, optionally undoable). `DISABLED` cannot be set that way, and since CS5 it is no longer reported there ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). Effects that use AEGP calls must not let AEGP query results drive rendering without storing them and forcing a re-render ([SDK: AEGPs › Cheating Effect Usage of AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/cheating-effect-usage-of-aegp-suites/)). | MUST implement HIDDEN through 02 (document tier when undoable, §5.13). MUST refuse DISABLED via this path. | 02 / 12 |
| S8 | Custom UI events identify the target param by index and area (title or control) ([SDK: Effect UI & Events › PF_EventExtra](https://ae-plugins.docsforadobe.dev/effect-ui-events/PF_EventExtra/)). Coordinate callbacks map layer ↔ comp ↔ screen at a given time; the comp-to-layer matrix may not exist when a layer scales to zero ([SDK: Effect UI & Events › UI Callbacks](https://ae-plugins.docsforadobe.dev/effect-ui-events/ui-callbacks/)). | MUST supply the transforms at the event time. Drags produce live, non-undo updates and one commit (R02-08). | 12 `ViewTransform` (R12-02). |

**Host-side sequence for one supervised edit** (inference, built from S1–S6 and §5.3):

1. 12 starts a gesture (R02-08). The user's new value is applied live to a working snapshot, with no undo step yet.
2. If the param is supervised (or is a button), the host converts the snapshot to `params[]` at the UI time in layer time, clears every change-flag slot (L6), and sends `USER_CHANGED_PARAM` with the param's index.
3. After the call, the host collects edits:
   - values of params whose `CHANGED_VALUE` is set (not layer params), merged through the quantisation-aware rule;
   - UI changes made through `PF_UpdateParamUI` copies;
   - twirl flags;
   - AEGP stream, keyframe and dynamic-flag writes made during the call.
4. Each animated param that was edited gets a keyframe at the UI time; static params get their static value. Blob values are flattened.
5. The gesture is committed as **one** undo unit, authored by the user, with the effect as the agent. It produces a `StageEvent`, so 04 invalidates only the affected params and time ranges.
6. If `FORCE_RERENDER` was set, the host also snapshots the effect's sequence data (§9) so render-side instances see it (R04-15).
7. During a drag, steps 2–4 repeat per drag event as live updates. Step 5 happens once, on release.

### 5.9 Arbitrary data parameters

Arb data lets a plug-in register any data for AE to keyframe and manage, with the plug-in supplying the "member functions" ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). They arrive via `PF_Cmd_ARBITRARY_CALLBACK`. The extra names the function and carries the arb-type id given at setup ([SDK: Effect UI & Events › PF_EventUnion](https://ae-plugins.docsforadobe.dev/effect-ui-events/PF_EventUnion/)). A plug-in that uses arb data must answer all of them ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)).

| Function | Contract | When the host MUST call it (BAE) |
|---|---|---|
| `NEW` | Allocate and return a new instance with default values. | When it needs a fresh default and has no default handle (inference). The default handle given at setup is the canonical default: flatten it once and store the bytes in the definition. |
| `DISPOSE` | Free an instance. | For every handle the host obtained from the plug-in, exactly once, including interpolation results and scan results. |
| `COPY` | Copy a source instance into a new destination. With a null source, create a default instance. | Building `params[]` and checkout results from cached instances; duplicating effects or keyframes; snapshotting before edits. |
| `FLAT_SIZE`, `FLATTEN` | Report the flat size, then write a disk-safe flat form into a buffer of exactly that size. The data "will be unloaded and reloaded at the user's whim". | Whenever a keyframe or static value enters the document (after supervision, custom-UI edits, scan, paste). The flat bytes are the only persisted and synced form. |
| `UNFLATTEN` | Rebuild an instance from flat bytes. | Lazily, when the plug-in needs the value (render, UI, compare). Results are cached by content hash in the process that runs the plug-in. |
| `INTERP` | Given the start and end instances and a normalised position in [0,1] (velocity already applied), produce the in-between instance. | For each evaluation between two non-hold keyframes, unless `CANNOT_INTERP`. Results cached per (a, b, position). |
| `COMPARE` | Report equal, less, more, or just "not equal". | For change detection: "modified", identical-checkout queries, skipping redundant keyframe writes. Byte-equal flat forms MAY be treated as equal without a call (inference). |
| `PRINT_SIZE`, `PRINT` | Text form for copying keyframes to other applications. | When keyframes are copied as text (12 clipboard). |
| `SCAN` | Parse text (often from the clipboard) into an instance. | When text is pasted onto the arb stream. |

Host obligations ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/) unless noted):

- **Re-entrancy.** Arb callbacks can arrive while the plug-in is inside another call. For example, a layer checkout renders another instance of the same effect, which triggers arb handling. The host MUST allow nested calls on the blocked thread (see §8.3).
- **No instance context.** The guide warns not to check out params when `effect_ref` is null, including inside `INTERP`. So AE does call arb functions without an effect context. The host MAY do the same, and MUST treat these functions as **type-level**, callable without an instance (for example at load, or from 03 helpers).
- **Ownership.** Arb defs in `params[]` are read-only to the plug-in ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). Under concurrent renders the host MAY share one unflattened instance across calls. A plug-in that mutates it anyway breaks this (inference). P3 SHOULD offer a per-call `COPY` mode per plug-in.
- **Out-of-process.** Arb handles live in the plug-in's process. The host keeps only flat bytes. `INTERP`, `COMPARE` and `PRINT` run in the helper. 03 therefore MUST NOT need a synchronous plug-in call to evaluate a blob attribute (reservation: lazy blob samples).
- **Missing plug-in.** Blob values hold, are never interpolated, and round-trip verbatim (R02-04). Custom UI and arb data go well together, since the arb is the durable home of UI state ([SDK: Effect UI & Events › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-ui-events/tips-and-tricks/)).

### 5.10 Layer parameters, camera and lights

| Topic | AE rule | Host obligation | BAE |
|---|---|---|---|
| Value | A layer reference chosen from AE's pop-up of comp layers ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). The stream value is a layer id ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). Default is self or none ([SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/)). | MUST keep the reference stable across rename and reorder, and resolve a deleted target to none. | `LayerRef` (R02-07 + stage). |
| Render stage | Since 14.2 the user can pick Source, Masks, or Effects & Masks for a layer param, and plug-ins see no difference ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). `AEGP_StreamSuite7` stores the stage separately from the layer: source (default, before masks and effects), masks only, all effects, or "through effect N". Setting layer and stage together is one undo step. A query returns the highest stage that creates no render cycle, and the result changes when effects or the source change ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). Self-reference through effects must be impossible ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | MUST store and honour the stage on checkout. MUST prevent cycles at edit time and fail at render time. | 02 `stage` field; 04 per-stage layer outputs (reservation). |
| Checkout | Pixels only through `PF_CHECKOUT_PARAM` (legacy) or `checkout_layer` (SmartFX, during pre-render only) ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/), [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). None → zero-filled world. No masks at source stage. Raw source for legacy checkouts. In/out trims ignored. Not allowed during UI events. | See §8.1 for the full rule set. | 04 |
| Geometry | A checked-out layer has its own PAR (use the world's, not `in_data`'s, for other layers) ([SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/)). Dimensions are downsampled ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). Layer transforms are not applied, since effects get layer-space pixels (inference); continuously rasterised vector layers are the exception and are rendered with geometry in a comp-sized buffer ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). | MUST deliver untransformed layer-space pixels with that layer's PAR. | 04 |
| Camera and lights | Effects reach the camera, camera matrix, layer→comp time and their own layer and effect refs through `AEGP_PFInterfaceSuite1` ([SDK: Effect Details › Accessing Camera & Light Information](https://ae-plugins.docsforadobe.dev/effect-details/accessing-camera-light-information/), [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). The matrix is row-major; its inverse is the model view; non-square comps need a PAR correction ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). `I_USE_3D_CAMERA` / `I_USE_3D_LIGHTS` declare the dependency ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). AEGP query results are otherwise not tracked as dependencies ([SDK: AEGPs › Cheating Effect Usage of AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/cheating-effect-usage-of-aegp-suites/)). | MUST answer from the same snapshot and time as the render. MUST add camera and light state to the cache key only when flagged. | 03 camera evaluation; R04-08 / R04-18. |

### 5.11 Path parameters (mask references)

| Topic | AE rule | Host obligation | BAE |
|---|---|---|---|
| Reference | A path param points at a mask on the effect's own layer. The Parameters page says `path_id` holds the **index** of the selected mask, which AEGP code can turn into a mask handle by index ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). The path suites, however, take a `PF_PathID` "unique id" obtained from `PF_PathInfo(index)` ([SDK: Effect Details › Working With Paths](https://ae-plugins.docsforadobe.dev/effect-details/working-with-paths/)), and the stream value type is a mask id with masks uniquely identified ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). **The guide is inconsistent.** | MUST store the reference by mask identity so mask reordering keeps the binding (inference). MUST derive whatever the plug-in reads from `path_id` (index or id) at call time, and accept both forms in the path-suite calls until verified. | `MaskRef` (R02-07). |
| Access | Values are not read directly. Paths are checked out and in like layers. Never trust the value in the passed def without a checkout: deleted paths disappear and updates arrive lazily ([SDK: Effect Details › Working With Paths](https://ae-plugins.docsforadobe.dev/effect-details/working-with-paths/)). | MUST evaluate the mask at the requested time on checkout. MUST free at check-in, including on error. | 03 mask evaluation at any time. |
| `PF_PathQuerySuite1` | Count the paths of the effect's layer, index → path id, checkout at a time, and check-in with a "changed" flag ([SDK: Effect Details › Working With Paths](https://ae-plugins.docsforadobe.dev/effect-details/working-with-paths/)). | MUST expose **all** masks of the layer, not just referenced ones. Effects reading unreferenced masks set `DEPENDS_ON_UNREFERENCED_MASKS` ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). The "changed" flag at check-in: semantics guide silent → SHOULD ignore. | Cache key includes all mask geometry only when flagged (R04-08). |
| `PF_PathDataSuite` | Open/closed, segment count, vertices (x, y and in/out tangents as doubles in layer space; for closed paths the last vertex equals the first), segment length (with a prepare/cleanup pair and a sampling frequency; internal effects use 100), position and first derivative at a length, inverted, mask mode, name ([SDK: Effect Details › Working With Paths](https://ae-plugins.docsforadobe.dev/effect-details/working-with-paths/)). | MUST implement arc-length evaluation numerically close to AE's. MUST report the mask mode, including the "none" mode. Whether vertices are downsampled: guide silent. | 03/05 Bezier utilities. |
| Writing | Paths are opaque blobs edited through get/set functions. Effects can edit them through `AEGP_MaskOutlineSuite3` ([SDK: Effect Details › Working With Paths](https://ae-plugins.docsforadobe.dev/effect-details/working-with-paths/)). | MUST route edits through 02 commands. | 02 |

### 5.12 Checkout at other times and caching (param side)

The pixel side is in §8. These rows cover non-layer values.

| Topic | AE rule | Host obligation | BAE |
|---|---|---|---|
| Cost expectation | Adobe recommends integrating a speed param by checking out its value at many times, from 0 to now. "The cost of checking out many parameter values is negligible compared to rendering." Checked-out params are read-only and must be checked in even on error ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). | MUST make single-param evaluation at an arbitrary time cheap: no global lock, no full comp evaluation, roughly O(log keyframes) plus any expression. MUST NOT copy the whole param array per checkout. | 02 immutable snapshots readable concurrently; 03 per-attribute evaluation (reservation, R04-10). |
| Where | SmartFX may check out non-layer params at any point. Layer inputs only in pre-render ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). `QUERY_DYNAMIC_FLAGS` reads non-layer params through checkout ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). | MUST support checkout in all of these. | 19 / 04 |
| Dependencies | Using params from other times requires `WIDE_TIME_INPUT`; with `AUTOMATIC_WIDE_TIME_INPUT` (SmartFX) every checkout is tracked ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | MUST journal (param id, time) for non-layer checkouts as well as layers. | R04-10 |
| State receipts | `PF_GetCurrentState` takes one param, all, all except layers, or all honouring exclude, over a time span or all time. The span grows with automatic wide time. `PF_AreStatesIdentical` compares receipts ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). A successful check implies a temporal dependency on the span ([SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)). | MUST build receipts from change stamps without rendering. Receipts MUST cover referenced layers' upstream state. | R02-12 |
| Identical checkout | True when the value is the same at two times, whatever happens between them ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | SHOULD answer from curve structure (same hold or constant segment) before evaluating both times. Arb values use `COMPARE`. | 03 |
| `NON_PARAM_VARY` | Output varies with something besides params; AE then does not reuse output across frames of a still ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | MUST include time in the key. | R04-08 |

### 5.13 Per-instance parameter state: what is persisted, synced or local

| State | AE evidence | BAE tier | Notes |
|---|---|---|---|
| Values, keyframes, expressions | Streams saved with the project ([SDK: Effect Details › Changing Parameter Orders, the Nice Way](https://ae-plugins.docsforadobe.dev/effect-details/changing-parameter-orders/)). | **Document** (synced, undoable) | Keyed by param id. |
| Layer stage, layer and mask references | Stream values; stage changes are undoable ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | Document | — |
| Hidden via dynamic stream flag | Undoable when the caller asks ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | Document when undoable; otherwise instance-transient | **Corrects R02-05**, which made all hiding transient. |
| Effect instance display name | `AEGP_SetStreamName`, undoable ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | Document | — |
| Runtime param names, disabled, custom-UI height, popup labels of control-only popups | Set through `PF_UpdateParamUI` ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). Disabled is fed back to the plug-in. Whether these persist across save: guide silent. | **Instance-transient**: not a command, not synced. MAY be cached locally so placeholders and the first paint look right. | Rebuilt by sending `UPDATE_PARAMS_UI` after load (S5). |
| `STD_CONTROL_ONLY` control values | No stream ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | Instance-transient | The plug-in keeps the durable copy in sequence data or arb data. |
| Twirl open/closed | Plug-in may set it; initial state from flags (§5.4). Persistence: guide silent. | **Per-user view** (local layer, not synced) | So two collaborators can twirl independently. |
| Definition (types, ranges, defaults, labels, flags) | From `PARAM_SETUP` (§5.1). | Registry (19) + snapshot on the Effect Prim | The snapshot serves placeholders when the plug-in is missing (R02-06). |

### 5.14 Summary of what 02 must be able to express

1. Open param-type enum. Unknown tags round-trip verbatim. All types in §5.2, including value-less rows (group start/end, button, no-data), each with a stable id.
2. Per-param definition: id, type, name, flags (animatable, interpolable, supervised, invisible, collapsed by default, skip reveal), valid range and slider range, precision and display mode, default and **old-project value**, popup options with separators, checkbox and button labels, layer default (none/self).
3. Values: f64 scalars, f64 angles (unbounded), bools, float RGB colour in a declared space, layer-space 2D/3D spatial points, 1-based enums, `LayerRef {target, stage}`, `MaskRef`, `Blob {arb_type, flat bytes}`.
4. Default semantics: unauthored → *current* definition default. The Effect Prim records the param ids it knew when last written, plus orphan retention (D3, D4).
5. Animation: AE keyframe model on every animatable effect param, including spatial tangents for points. Hold-only for checkbox, popup and `CANNOT_INTERP`. Blob attributes evaluate to lazy samples.
6. Queries on immutable snapshots: keyframe enumeration, identical-at-two-times, change stamps over time ranges.
7. Three state tiers (§5.13), with plug-in-initiated edits as gesture commands.

### 5.15 Cross-check of `crates/bae-aex-host/src/params.rs`

| Item in crate | Finding | Fix |
|---|---|---|
| `ParamDescriptor::disk_id` doc: "falls back to the index when the plug-in leaves it 0" | AE's rule is per list: if the **first** param's id is zero, AE numbers *all* params sequentially from 1 ([SDK: Effect Details › Changing Parameter Orders, the Nice Way](https://ae-plugins.docsforadobe.dev/effect-details/changing-parameter-orders/)). A per-param fallback can collide with explicit ids. | Add a list-level `assign_disk_ids(&mut [ParamDescriptor])` that applies the first-param rule. Reject or warn on later zero ids. Warn outside 1–9999. |
| `ParamKind` stores only `default` | `USE_VALUE_FOR_OLD_PROJECTS` needs the def's separate `value` field as well ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | Add `old_project_value: Option<…>` per valued kind (or a generic `ParamValue`). |
| `ParamKind::Layer { default_self }`, `BaeValueShape::LayerRef` | No render stage. Since 14.2 a layer param also carries Source / Masks / Effects&Masks / through-effect-N ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/), [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Add `LayerStage` to the value model and document that the stage is user state, invisible to the plug-in. |
| `bae_mapping`: Layer and Path → `Interpolation::HoldOnly` (keyframeable) | Guide silent. AE's UI offers no keyframes on layer or path params (inference). Allowing keyframes would create data AE cannot round-trip. | Map to `Static` until verified against AE, or gate on the `AEGP_CanVaryOverTime` answer recorded during P3 verification. |
| `ParamFlags` / `ParamUiFlags` are definition-time only | There is no model of per-instance runtime state: runtime name, disabled (fed back to the plug-in since CS6), hidden via dynamic stream flag (possibly undoable), twirl, `ui_height`, popup labels of control-only popups, control-only values ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/), [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | Add `ParamRuntimeUi { name, disabled, hidden: Option<HiddenSource>, collapsed, ui_height, popup_choices, control_value }` with the tier of each field (§5.13). |
| Module doc: names "≤ 31 bytes + NUL"; `ParamName::new` rejects longer names | Correct limit, but rejection is wrong for input from the plug-in. A plug-in may supply a full buffer without terminator or a split multibyte tail ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/), [SDK: Introduction › Localization](https://ae-plugins.docsforadobe.dev/intro/localization/)). | Add a lossy `from_plugin_bytes(&[u8], encoding)` that truncates at a character boundary. Keep the strict constructor for host-originated names. |
| `PopupChoice` / `Enum` doc: "1-based, separators counted"; `parse_popup_choices` splits on `\|` | Guide silent on numbering. The guide's example shows `/` between entries (rendering artefact (inference)). There is no model of the def's choice count, nor of the rule that entries are immutable except for control-only popups ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | Mark numbering and delimiter as "verify against SDK samples". Add a `mutable_choices` derived from `std_control_only`, and validate the choice count against the parsed entries. |
| `Point { default_percent }`, `Point3D` doc "percent … pixels for z" | The guide documents percent defaults only as pre-12.1 behaviour, ambiguously, and says nothing about 3D-point defaults ([SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). `BaeValueShape::Point2` "layer pixels" leaves out the delivery transform (downsample, PAR, current-time origin shift) and the fixed/float dual form. | Downgrade the docs to "(inference, verify)". Document the delivery transform beside the mapping. Add an `Interpolation::NativeSpatial` (or a `spatial` flag) for points. |
| `Color { default_rgb8 }` → `BaeValueShape::Color` "float RGBA" | Missing: the colour space of stored values (the picker works in the working space ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/))); 8-bit clamping of HDR; quantisation-aware write-back. | Document `ColorRgb` in the R06-01 space. Add a helper `legacy_rgb8(&ColorRgb)` and `merge_written_rgb8(stored, written)`. |
| `ChangeFlags { changed_value }` | No record of the validity windows, nor that the flag shares storage with the disk id (`uu`) ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). No record that it is ignored for layer params and has no render effect on path params since 14.0 ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Add `fn honoured_in(selector) -> bool` and a per-kind `accepts_changed_value()`. |
| `validate_param_list` | Missing checks: `num_params` equals definitions + 1 ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)); `TOPIC`/`CONTROL` require `PF_OutFlag_CUSTOM_UI`; an AE arb param needs `TOPIC`/`CONTROL` or `NO_ECW_UI` (or Premiere-style leniency); `START_COLLAPSED` is ignored without the out-flag ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/), [SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/)). | Take the global out-flags and `num_params` as inputs. Return warnings separately from errors. |
| `ParamKind` | There is no `Custom`, and no `Unknown { raw_type }` for future types ([SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/)). | Add `Unknown { raw_type: i32 }` so placeholders round-trip. |
| `FloatSlider` | Has `want_phase` and `curve_tolerance`, but not the phase value the host fills during audio rendering ([SDK: Audio › Audio-Specific Float Slider Variables](https://ae-plugins.docsforadobe.dev/audio/audio-specific-float-slider-variables/)). | Note it as a per-call output, not definition data. |
| `ArbOp` | Missing: `Copy` with no source means "create default"; arb ops may run with no instance (null `effect_ref`) and must be re-entrant; `Interpolate.t` is post-ease ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). | Document these. Split the ops into a type-level trait with no instance argument. |
| `has_stream = false` for `STD_CONTROL_ONLY` with doc "lives in sequence data" | The plug-in keeps its copy in sequence data, but the **host** still has to hold and display the control's value per instance ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | Reword. Route the value to `ParamRuntimeUi::control_value`. |

### 5.16 P3 risks

| Risk | Why | Mitigation |
|---|---|---|
| Precision loss on supervised write-back | Legacy 16.16 and 8-bit fields (§5.3) | Quantisation-aware merge; tests with HDR colours and sub-pixel points. |
| Point drift after layer-size change or upstream expansion | AE seems to store normalised points (inference); origin shift uses the current time | 15 import tests that replace footage; replicate the origin-shift quirk exactly. |
| Undo granularity | One gesture can trigger many supervised writes, AEGP keyframe inserts and arb flattens | Gesture commands (R02-08); batch AEGP keyframe adds. |
| Arb interpolation cost under MFR and out-of-process | `INTERP` per evaluation, `UNFLATTEN` per call | Lazy blob samples; caches by content hash in the helper; per-plug-in "copy per call" switch. |
| `UPDATE_PARAMS_UI` storms while scrubbing | Sent on every time change | Coalesce on the UI thread; skip when the controls are hidden; optional Premiere-style skip. |
| Contradictory guide text on when `CHANGED_VALUE` is honoured | §5.8 S2 | Accept the union except in `UPDATE_PARAMS_UI`; log; verify against AE with SDK samples. |
| Default-following semantics surprise BAE users | AE changes unauthored params when the plug-in's default changes (D3) | Show "follows plug-in default" in the UI; 15 imports must preserve authored vs at-default. |
| Index ↔ id drift after plug-in update | Indices change, ids do not | Rebuild the map per definition version; never persist indices. |
| Hundreds of params per instance | Effect Controls, array marshalling, cache-key hashing | Virtualised rows (12); incremental param hashes (R02-12). |

### 5.17 Open questions (verify with AE and SDK samples in P3)

1. Are popup values 1-based, and do separators take an index? Is the choice delimiter `|`?
2. Can layer and path params be keyframed or carry expressions in AE?
3. Units of 3D-point defaults; whether z is downsampled; whether path vertices are downsampled.
4. Does AE persist runtime param names, disabled state and twirl state in the project file?
5. Is `CHANGED_VALUE` honoured on click events and in `UPDATE_PARAMS_UI`, as the def page says, or not, as the supervision page says?
6. Which change-flag bits exist beyond `CHANGED_VALUE`, and what do they mean?
7. Does AE send `USER_CHANGED_PARAM` for a button without `SUPERVISE`?
8. Does AE clamp post-expression values to the valid range before passing them to the plug-in?
9. What happens to saved values for disk ids the current plug-in no longer defines, and to duplicate ids?
10. How does AE build third-party param match names (used by expressions and scripts)?
11. Is "through effect N" in the layer stage an index that shifts when effects are reordered, or a stable reference?
12. Does `PF_PathDef.path_id` hold the mask index (Parameters page) or the unique `PF_PathID` (path suites)?
13. Is the default point position percent of layer size for current API versions (the guide's history note is ambiguous)?

## 6. Pixel worlds, formats and image services

This section covers what an AE effect expects of the image buffers it is given, and of the image
services (allocation, iteration, sampling, compositing, colour helpers) the host has to provide.
Request and result rectangles of SmartFX are in §7/§8; GPU worlds and devices are in §11. The
measured cost of the CPU round trip is in §19, and shared-memory worlds for the out-of-process
helper are in §20. Reservation labels `6-Rn` are folded into §18; existing ids (R04-01, R04-02,
R04-03) are the ones the skeleton crate already uses.

**Summary.** A CPU AE effect works on packed, **A-R-G-B ordered** worlds in host memory at
8 bpc (0..255), 16 bpc (0..**32768**) or 32 bpc float (nominal 0..1, over-range legal). Its
values are **encoded in the project's working colour space**, whatever that is: gamma-encoded by
default, linear-light when the working space is linearized, or the OCIO working space. The host
must (a) give the plug-in exactly that encoding and a depth the plug-in declared, (b) respect
arbitrary `rowbytes`, extents and origins, and (c) implement about a dozen image-service suites
on CPU worlds, with AE's semantics and with its own threads. The cheap path keeps BAE's working
buffers in the same encoding and alpha convention as AE, so that the GPU conversion pass (R04-02)
only swizzles and rescales. Anything else forces an unpremultiply→transform→premultiply step at
every AE node (6-R2, 6-R3).

### 6.1 The world: `PF_EffectWorld` / `PF_LayerDef`

AE describes every image as a `PF_EffectWorld`, which is also called `PF_LayerDef`
[SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/).

| Field / property | What the plug-in expects (SDK) | Host obligation | BAE mapping | P3 risk |
|---|---|---|---|---|
| `data` | Opaque pixel pointer. Plug-ins are told to go through `PF_GET_PIXEL_DATA8/16`, which return null when the world is not of that depth. During `PF_Cmd_SMART_RENDER_GPU` the struct is filled as for CPU but `data` is null [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/). | MUST point at the first pixel of the **top** row (the guide's addressing example is base + y·rowbytes + x·pixel [SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/); top-down is an inference). MUST implement the get-pixel-data callbacks with the null-on-mismatch rule. MUST pass null `data` on GPU worlds (§11). | 04 CPU-node staging buffer (R04-02). wgpu rows are top-down too, so no flip is needed (inference). | Many plug-ins cast `data` directly, even though the guide warns against it. Layout must therefore be exact, not "equivalent". |
| `rowbytes` | Bytes from one row to the next: `width` pixels plus optional padding. Input and output may differ even at the same size. It does not change with field rendering. Plug-ins must not walk the whole buffer and must never write outside the region, because that "can corrupt cached image buffers that don't belong to you". The "Grow Bounds" test produces a larger output rowbytes [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/#rowbytes-in-pf_effectworlds). | MAY pick any rowbytes at or above the row size. SHOULD pick a GPU-copy-friendly alignment (the crate uses 256). MUST NOT read meaning into padding bytes. Callbacks that receive worlds from the plug-in (§6.11) MUST address rows by the signed rowbytes they are given. The guide does not say whether rowbytes can be negative or not a multiple of the channel size, so the host MUST either support such values byte-wise or reject them with a parameter error, never misread them. | R04-02 / 6-R9: each CPU-node buffer has its own stride. Input and output strides are independent. | The GPU converter needs rowbytes divisible by 4 (`row_words`). That is fine for host-made worlds, but not guaranteed for worlds a plug-in builds itself. |
| `width`, `height` | Size of the pixel buffer. `in_data.width/height` is the **source layer** size: not affected by downsampling and not necessarily the input buffer size [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). | MUST set the world size to the real buffer and `in_data` to the full-resolution layer size (for SmartFX, also `ref_width/height`, which ignore downsampling) [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | 04 request rect size; layer source size from 02. | Mixing these two sizes up shifts point controls. |
| `extent_hint` (world) | The smallest rectangle containing all non-zero-alpha pixels. It defines what needs output. An effect "that varies with extent" ignores it and renders everything [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/). | SHOULD compute it from 04 content bounds. MAY set it to the whole buffer (safe but slower). When an output extent smaller than the buffer was given, the host MUST treat output pixels outside it as undefined and replace them with transparent black on decode (inference). | 04 content bounds (6-R9). Legacy semantics are in §6.7. | If the hint is too small, a plug-in leaves garbage that the host would otherwise read back. |
| `world_flags` | `PF_WorldFlag_DEEP`: "set if the world is 16-bpc". `PF_WorldFlag_WRITEABLE`: the plug-in may change the pixels. Normally only the output is writeable [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/). | MUST set DEEP exactly when the world is 16 bpc. Whether AE also sets it on float worlds is undocumented (open question). MUST set WRITEABLE on outputs. An input MAY be writeable if it is a private per-call copy. With `PF_OutFlag_I_WRITE_INPUT_BUFFER` the host MUST NOT reuse the input contents after the call [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags). | BAE input staging is per call, so allowing writes costs nothing (inference). | A plug-in that branches on DEEP alone will mis-handle float worlds if the flag is set wrongly. |
| `pix_aspect_ratio` | Rational. Effects use it for **checked-out** layers and must use `in_data.pixel_aspect_ratio` for their own layer. Different layer parameters can have different PARs [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/), [SDK: Effect Details › Pixel Aspect Ratio](https://ae-plugins.docsforadobe.dev/effect-details/pixel-aspect-ratio/). | MUST fill both: the per-input PAR on each checked-out world, and the own-layer PAR in `in_data`. | 6-R7: each input of a 04 request carries its PAR (footage interpretation in 02/08). | The crate's default `RationalScale::ONE` is only a placeholder. |
| `platform_ref` | "No longer used in CS5" [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/). | MAY be left empty. | — | None. |
| `dephault` | Only for layer parameters: myself or none [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/). | Set it from the parameter definition (§5). | 02 layer-param default. | None. |
| Origin (not a field) | Where the buffer sits relative to the layer comes from elsewhere: `in_data.output_origin_x/y` (set through `out_data.origin` in `PF_Cmd_FRAME_SETUP`), `pre_effect_source_origin_x/y` when earlier effects moved the origin [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/), and the SmartFX rects in layer coordinates [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST keep the mapping from world (0,0) to layer coordinates consistent across all three. | `WorldDesc.origin_x/y` in the crate. 6-R12 propagates the origin shift between effects. | An origin error shows up as a jump when an upstream blur is toggled. The guide gives this as its own test. |
| Alignment, sub-worlds | Pixels are **not** guaranteed to be 16-byte aligned, and "an effect may get a subregion of a larger PF_EffectWorld" [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/#byte-alignment). | MAY hand out sub-worlds of one staging allocation (zero-copy ROI). Element alignment for the channel type MUST hold. | Lets 04 share one staging buffer between overlapping requests (inference). | Some SIMD plug-ins assume alignment anyway. Host-made worlds should still be page/256-aligned as a courtesy. |

### 6.2 CPU pixel formats and channel order

All CPU worlds store four channels per pixel, "Alpha, Red, Green, Blue from the low byte to the
high byte" [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/).
The pixel types are ARGB at 8, 16 and 32 bpc
[SDK: Effect Details › Color Space Conversion](https://ae-plugins.docsforadobe.dev/effect-details/color-space-conversion/).
`PF_GetPixelFormat` reports them as `PF_PixelFormat_ARGB32`, `_ARGB64` and `_ARGB128`
[SDK: Effect Details › Image Buffer Management Functions](https://ae-plugins.docsforadobe.dev/effect-details/image-buffer-management-functions/).

| Depth | Format name | Channel type | Black → white | Out-of-range values | Source / note |
|---|---|---|---|---|---|
| 8 bpc | `PF_PixelFormat_ARGB32` | unsigned 8-bit | 0 → 255 | Not representable. | The AE guide's own utilities use 0..255 throughout [SDK: Effect Details › Color Space Conversion](https://ae-plugins.docsforadobe.dev/effect-details/color-space-conversion/). |
| 16 bpc | `PF_PixelFormat_ARGB64` | unsigned 16-bit | 0 → **32768** | Not representable: no super-white and no negatives. | **The AE guide does not give the 16-bit range.** Adobe's Premiere SDK states "from black at 0 to white at 32768" [Premiere Pro SDK: Universals › Pixel Formats and Color Spaces](https://ppro-plugins.docsforadobe.dev/universals/pixel-formats-and-color-spaces/), and Adobe staff confirm the maximum is 32768, not 32767 [Adobe Community thread](https://community.adobe.com/t5/after-effects-bugs/why-is-ae-visually-15bit-int-instead-of-16bit-int/m-p/15193104). |
| 32 bpc | `PF_PixelFormat_ARGB128` | 32-bit float | 0.0 → 1.0 nominal | **Legal.** Over-bright and negative values carry HDR and wide-gamut data. | Colour parameters carry "over-bright colors… including any high dynamic range component" [SDK: Effect Details › Parameters & Floating Point Values](https://ae-plugins.docsforadobe.dev/effect-details/parameters-floating-point-values/). Adobe's user guide: "To preserve over-range values, work in 32-bpc color" [AE CS3 User Guide, p. 249](https://www.manualsdir.com/manuals/753848/adobe-after-effects-cs3.html?page=249). |
| GPU | GPU world (§11) | 32-bit float | 0.0 → 1.0 nominal | Legal | The AE guide only says that the CPU struct has null `data` during GPU render [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/#new-in-160), and that only F32 GPU rendering exists in AE 16.0 [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). It does **not** give the GPU channel order. The crate assumes B,G,R,A float (Premiere naming); verify in P3 (§11). |

**Why 32768 rather than 65535.** White is a power of two, so a 16-bit value is "15 bits plus one":
normalising is an exact shift and mid-grey (16384) is exact. That is our reading; the AE guide
gives no rationale. Values from 32769 up fit in the type but have **no documented meaning**. Adobe
staff confirm the maximum but do not cover larger values
[Adobe Community thread](https://community.adobe.com/t5/after-effects-bugs/why-is-ae-visually-15bit-int-instead-of-16bit-int/m-p/15193104),
and community answers only show plug-ins rescaling to and from 0..65535 for third-party libraries
[Adobe Community thread](https://community.adobe.com/t5/after-effects-discussions/confused-by-ae-16-bit-rgb-colors-values/m-p/12097393).
The host therefore MUST NOT produce values above 32768, and SHOULD clamp them to white when reading
a plug-in's output. That is a host policy, not documented AE behaviour.

**Conversion rules between BAE working buffers (RGBA16F/RGBA32F, premultiplied) and worlds.**
All of these are host policy; AE's own rounding is undocumented.

| Direction | Rule (host policy) | Exactness |
|---|---|---|
| float → 8 bpc | Clamp to [0,1], scale by 255, round to nearest. | Exact round trip for every 8-bit value, even through half floats (crate test). |
| float → 16 bpc | Clamp to [0,1], scale by 32768, round to nearest. | Lossless only through RGBA32F. Through RGBA16F the error stays within a few 16-bit steps (crate test bound), hence R04-03. |
| float → 32 bpc | Pass through. NaN SHOULD become 0 (protects the plug-in); ±Inf is kept. | Lossless. |
| 8/16 bpc → float | Divide by white (255 / 32768). 16-bit values above 32768 are clamped first. | Exact in f32. |
| 32 bpc → float | Pass through. NaN SHOULD become 0 here as well (the guide is silent), so plug-in NaNs cannot poison downstream GPU blending. | Lossless into RGBA32F. Into RGBA16F, values above the half-float maximum become ±Inf. |

Over-range is clipped whenever a plug-in runs below 32 bpc. AE behaves the same way, because a
plug-in "will never receive… worlds with higher bit depth than they have claimed to be able to
handle" [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/#byte-alignment).
BAE SHOULD flag such nodes in the UI, like AE's depth warning; that UI is for 11/12.

Premiere-only formats (BGRA_8u, YUV, 32f BGRA registered through the PF Pixel Format Suite) and
Premiere's per-field render with half height and doubled rowbytes are host-specific
[SDK: Premiere Pro & Other Hosts › Bigger Differences](https://ae-plugins.docsforadobe.dev/ppro/bigger-differences/),
[SDK: Premiere Pro & Other Hosts › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/).
An AE-compatible host need not offer them and SHOULD NOT; doing so would only change plug-in code
paths away from the AE ones. The [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)
page lists no change to CPU pixel formats in recent releases.

### 6.3 Depth negotiation: how the plug-in learns which depth it gets

| Mechanism | SDK semantics | Host obligation |
|---|---|---|
| `PF_OutFlag_DEEP_COLOR_AWARE` | "The effect handles 16-bpc color" [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags). | Without it, the host MUST drive the plug-in at 8 bpc only. |
| `PF_OutFlag2_FLOAT_COLOR_AWARE` | 32-bpc float support. `PF_OutFlag2_SUPPORTS_SMART_RENDER` "must also be set" [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags2). `PF_Cmd_RENDER` "can only support 8-bit or 16-bit"; 32 bpc goes through `PF_Cmd_SMART_RENDER` only [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST ignore the flag unless smart render is also declared (the crate's `caps.rs` does this). MUST NOT send float worlds to `PF_Cmd_RENDER`. |
| SmartFX `bitdepth` | Pre-render input carries "the bit depth requested", in bpc, and the smart-render input is "identical" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST use the same depth in pre-render and render, and for every world in that render. |
| Querying a world | `PF_GetPixelFormat` returns ARGB32/64/128 [SDK: Effect Details › Image Buffer Management Functions](https://ae-plugins.docsforadobe.dev/effect-details/image-buffer-management-functions/). `PF_WORLD_IS_DEEP` and `PF_GET_PIXEL_DATA16` are also used [SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/). | MUST answer correctly for every world, including worlds the plug-in allocated and checked-out layers. |
| UI events | "There is no way to determine the bit depth of the layer(s) being processed during events" [SDK: Effect UI & Events › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-ui-events/tips-and-tricks/). | Nothing to provide. |
| Invariants | Input and output never differ in depth, and are never deeper than the plug-in claimed [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/#byte-alignment). | MUST hold. SHOULD also give checked-out layers the same depth (inference). |

**Choosing the depth.** AE has a project bit depth of 8, 16 or 32
[SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_projsuite6), and
drives each effect at that depth or the effect's maximum, whichever is lower. The guide implies
this through the "never higher than claimed" rule but never spells it out (inference).

Driving a float-aware plug-in at 32 bpc inside an 8-bpc AE project gives better quality, but it
is **not neutral**. Effects with different code paths per depth can clip, posterise or look
different at 8 bpc, and the imported project was tuned against the 8-bpc look. The host SHOULD
therefore default to min(plug-in maximum, the AE project depth recorded at import), and MAY offer
a per-project "maximum quality" override. Fidelity comes first here, because performance is
unaffected (§19: the 32-bpc world costs about the same to convert). This needs 6-R4 and 6-R5.

### 6.4 Premultiplication, alpha and zero-alpha colour

What the guide states:

- `premultiply` in `PF_FillMatteSuite2` converts to and from colour "pre-multiplied with black".
  `premultiply_color` does the same against any colour, at 8/16/float
  [SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/#pf_fillmattesuite2).
  `AEGP_CopyBits_HQ_Straight` and `AEGP_CopyBits_HQ_Premul` exist
  [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_compositesuite2).
- Zero-alpha pixels can hold colour. `PF_OutFlag2_REVEALS_ZERO_ALPHA` tells AE that the effect can
  reveal RGB under zero alpha (Set Channels is the example). Since AE 6.0, pixels outside a mask's
  bounding box are zeroed unless this flag is set
  [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags2).
  SmartFX's `preserve_rgb_of_zero_alpha` requires the effect to carry the colour of transparent
  pixels through, and to pass the request on recursively to its checkouts
  [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/#preserve_rgb_of_zero_alpha).
- `PF_OutFlag2_DOESNT_NEED_EMPTY_PIXELS` trims "empty" pixels. Empty means zero alpha, or zero
  alpha **and** zero RGB when `REVEALS_ZERO_ALPHA` is set
  [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags2).
- SmartFX `channel_mask` lists the channels the effect must produce: "Data written to other
  channels will not be honored" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/).

What the guide does **not** state is the alpha convention of effect input and output worlds. The
premultiply helpers, their "with black" wording and AE's long-standing compositing model are all
consistent with **worlds being premultiplied with black, in working-space encoding** (inference;
a P3 probe plug-in must confirm this in each colour mode, §6.5).

Host obligations:

- MUST deliver premultiplied-with-black ARGB. MUST NOT un-premultiply or re-premultiply when
  BAE's buffer encoding already equals the AE working encoding. In that case the conversion is a
  swizzle and a rescale only (crate `pixel.rs`, `convert.wgsl`).
- MUST NOT assume RGB ≤ A, and MUST NOT "clean" RGB where A = 0, in either direction. When a
  request carries `preserve_rgb_of_zero_alpha`, or the plug-in sets `REVEALS_ZERO_ALPHA`, upstream
  masks and trimming MUST keep that colour (6-R6).
- When an encoding change is unavoidable (§6.5), the conversion MUST un-premultiply with a
  zero-alpha guard, transform, and re-premultiply. The colour of zero-alpha pixels is then lost
  unless it is carried separately, which is a known fidelity limit (6-R3).
- SHOULD always request all four channels from plug-ins. If a subset is requested, the host MUST
  ignore what the plug-in writes to the other channels.

### 6.5 Working colour space: what values the plug-in sees

AE effects never receive a "colour space" argument. Pixels arrive encoded in the project working
space, and AE's colour helpers are defined against it. `PF_AppColorPickerDialog` returns a colour
"in the project's working color space"
[SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/#pf_appsuite),
and passing null blending tables to `AEGP_TransferRect` blends "in the current working color space"
[SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_compositesuite2).
To behave identically, a plug-in in BAE must see the **same numbers** AE would have shown it.

| AE project mode | What pixel values mean to the plug-in | How a plug-in can tell | Emulation the BAE host needs |
|---|---|---|---|
| Colour management off (AE default) | Footage values unconverted: in practice display-referred and gamma-encoded, roughly sRGB/Rec.709 (inference; the SDK guide is silent). | Read-only queries in `AEGP_ColorSettingsSuite5`. What they return when management is off is undocumented (open question). | BAE "sRGB non-linear (AE default)" working space (ARCHITECTURE §6). Buffers hold encoded values, so the host passes them through. |
| Off, plus "Blend colors using 1.0 gamma" | Unchanged for effects. Only blending between layers happens at gamma 1.0: "If you have not enabled color management, you can still perform blending operations using a gamma of 1.0" [AE CS3 User Guide, p. 249](https://www.manualsdir.com/manuals/753848/adobe-after-effects-cs3.html?page=249). | `AEGP_GetBlendingTables` (opaque) [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_colorsettingssuite5). | **Buffers stay encoded.** Linear blending is an option of the blend operation, not a re-encoding of the buffer (6-R2). Otherwise every AE node pays a transfer conversion. |
| ICC working space, not linearized | Values encoded with the working profile's tone curve and primaries. | `AEGP_GetNewWorkingSpaceColorProfile` (ICC), `AEGP_GetColorProfileApproximateGamma`, `AEGP_IsRGBColorProfile` [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_colorsettingssuite5). | A BAE working space with the same primaries and transfer. The host MUST answer the profile queries with an ICC description of it (06 supplies the profile bytes, 6-R1). |
| Linearized working space | **Linear light**. The linearized space "uses the same primaries and white point as the nonlinear version; it just doesn't use a tone response curve", and lets "all color operations" run in linear light [AE CS3 User Guide, p. 249](https://www.manualsdir.com/manuals/753848/adobe-after-effects-cs3.html?page=249). Over-range values are common at 32 bpc. | Approximate gamma near 1 (inference). | BAE linear working spaces (linear sRGB, linear Rec.2020). Pass-through. |
| OCIO colour management | Values in the OCIO working colour space named by the project (an ACES config, for example). | `AEGP_IsOCIOColorManagementUsed`, `AEGP_GetOCIOConfigurationFile`, `AEGP_GetOCIOConfigurationFilePath`, `AEGPD_GetOCIOWorkingColorSpace`, `AEGPD_GetOCIODisplayColorSpace` [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_colorsettingssuite5). Added with suite version 5 in May 2023 [SDK: History](https://ae-plugins.docsforadobe.dev/history/). | BAE ACEScg or an OCIO-subset working space (06). The host MUST return a real config file path or contents and the colour-space name. BAE does not link OpenColorIO, so 06 must keep the source config, or write an equivalent file for a synthesized one (6-R1). |

Notes for the host:

- Officially, effects may use only a few AEGP suites: camera and light data, `AEGP_GetLayerParentComp`
  and `AEGP_GetCompBGColor`. Their results must not drive rendering unless the effect stores them
  and forces a re-render
  [SDK: AEGPs › Cheating Effect Usage of AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/cheating-effect-usage-of-aegp-suites/).
  Colour-aware third-party effects do call `AEGP_ColorSettingsSuite` in practice (inference). The
  host SHOULD implement its read-only functions, consistent with the emulated mode.
  `AEGP_SetWorkingColorSpace` from an effect MUST be refused, or routed through a Command, never
  applied directly (AGENTS.md rule).
- Colour parameters: `PF_Pixel` is 8-bit with no alpha used, and the float value with HDR range
  comes from `PF_ColorParamSuite1`
  [SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/),
  [SDK: Effect Details › Parameters & Floating Point Values](https://ae-plugins.docsforadobe.dev/effect-details/parameters-floating-point-values/).
  Like the picker result, these are working-space values (inference). BAE colour parameters (02)
  must be convertible into the emulated AE working encoding without loss (6-R1).
- When BAE's working encoding cannot equal the AE one (an AE project that used a profile BAE does
  not support, for example), the host MUST convert at the node boundary. The conversion runs on the
  GPU inside the R04-02 pass and costs one extra ALU-bound kernel. Zero-alpha colour is lost, as
  noted in §6.4 (6-R3).

### 6.6 Colour-space conversion callbacks (`PF_ColorCallbacksSuite`)

| Function | Documented contract [SDK: Effect Details › Color Space Conversion](https://ae-plugins.docsforadobe.dev/effect-details/color-space-conversion/) | Host obligation / risk |
|---|---|---|
| `RGBtoHLS` / `HLStoRGB` | HLS in fixed point, each component scaled 0..1. | MUST implement. The guide gives ranges but not formulas, so hue sector and rounding need golden tests against AE. |
| `RGBtoYIQ` / `YIQtoRGB` | Y in 0..1, I in ±0.5959, Q in ±0.5227, all fixed point. | MUST implement with the NTSC YIQ matrix those ranges imply (inference). |
| `Luminance` | 100 × luminance, 0..25500. | MUST implement. The weights are unspecified (open question). |
| `Hue`, `Lightness`, `Saturation` | Hue as 0..255 for 0..360°; lightness and saturation 0..255. | MUST implement. |

These callbacks take 8-bit RGB pixels and fixed-point outputs. They ignore the working space, so
the host implements them locally in P3 and nothing in the BAE core is needed.

### 6.7 Extent hints, output extent and buffer expansion (non-SmartFX)

| Item | SDK semantics | Host obligation | BAE mapping |
|---|---|---|---|
| `in_data.extent_hint` | The intersection of the visible parts of input and output. It encloses the comp rectangle transformed into layer space [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). It is in the coordinate space of the **original input layer**, before resizing and origin shifts, and offsetting by `output_origin_x/y` converts it to output coordinates. Since 6.0 it is 20% larger than the layer, for predictive rendering [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/#extent_hint-usage). | MUST compute it in layer coordinates, from the 04 viewport ROI intersected with content bounds. SHOULD pad it the way AE does, so that cached frames stay valid while panning (fidelity in caching only). | 04 ROI. |
| `PF_OutFlag_USE_OUTPUT_EXTENT` | The effect honours the output extent. Obsolete for SmartFX [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags). | Without it, the host MUST treat the whole output as rendered. With it, only the extent is valid (§6.1). | 6-R9 |
| `PF_OutFlag_I_EXPAND_BUFFER` + `out_data.width/height/origin` | Output size and origin are set in `PF_Cmd_FRAME_SETUP`. That selector arrives **twice**, once without a render, to find out whether the layer is visible. Width = height = 0 means "ignore my render". The flag "drastically reduces caching efficiency" [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outdata-members). Outputs larger than 30,000 × 30,000 should be clamped [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/#extent_hint-usage). | MUST call frame setup at plan time to learn the per-frame output bounds, and MUST support the zero-size skip. MUST clamp oversized outputs. | 6-R12 (bounds known only after a plug-in call). |
| `pre_effect_source_origin_x/y` | Non-zero when earlier effects on the layer resized the buffer and moved the origin. Point parameters are adjusted for it automatically, and checked-out points use the current time's shift [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/#point-controls-and-buffer-expansion). | MUST track the cumulative origin shift along each layer's effect stack, and MUST add it to point parameters before passing them. | 6-R12 |
| `PF_OutFlag_I_SHRINK_BUFFER`, `PF_OutFlag2_DOESNT_NEED_EMPTY_PIXELS` | The effect may shrink its buffer to the extent, or receive input trimmed to non-empty pixels, with the trimmed origin in `pre_effect_source_origin`. Together with `I_EXPAND_BUFFER` the input may be a **null** buffer [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). | MAY pass trimmed or null inputs exactly as described. SHOULD NOT invent null inputs for plug-ins that did not opt in. | 04 content bounds. |
| `PF_OutFlag_PIX_INDEPENDENT` | Each pixel depends only on itself. This enables optimisations, and `in_data.field` is valid only with this flag [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/#pf_outflags). | MAY tile such nodes across threads or call them on sub-rects. | 04 tiling (inference). |
| Clean slate | The output is not necessarily cleared. AE's Gaussian Blur fills and copies first [SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/#clean-slate). | MAY pass an uncleared output. SHOULD clear it on the GPU (it is cheap), so that out-of-process shared memory never exposes another frame's pixels (inference, §20). | R04-02 pass. |

### 6.8 Pixel content of checked-out layer worlds

How layers are requested and scheduled is covered in §7/§8. This table covers what the pixels
inside a checked-out world must be.

| Situation | SDK semantics | Host obligation | BAE mapping |
|---|---|---|---|
| Layer parameter set to "none" | The returned layer is filled with zeros [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | MUST return a transparent-black world of a valid size. | Host-local. |
| Masks | "Masks are not included with checked-out layers" [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | MUST deliver the layer without its masks. | 6-R13 (pre-mask tap). |
| Legacy versus SmartFX checkout | A legacy effect that checks out a layer gets the **raw, un-effected** source. A SmartFX effect that checks out its own input (param 0) gets the result of the preceding effects [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/#parameter-checkout-vs-param-zero). | MUST offer both stages of a layer: the source before masks and effects, and the output after effect *k*. | 6-R13 |
| Time outside the layer's trim | Valid frames run from source start to source end whatever the in and out points, and are transparent outside that range. Layers with a lower frame rate refresh only at their own rate [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/#parameter-checkout-behavior). | MUST render untrimmed source frames, and transparent outside source duration. | 04 dependency at arbitrary time (R04-01). |
| Times between frames | Comps render at the exact time with interpolated keyframes. Footage returns the nearest frame to the left, or an interpolated frame when frame blending is on [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | MUST follow the same rules. | 04 plus 08 frame lookup. |
| During `PF_Cmd_UPDATE_PARAMS_UI` | A layer checkout returns a black frame of the correct size instead of rendered pixels, to avoid deadlocks [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | SHOULD do the same and never render synchronously on the UI thread. | 11/12 never block. |
| Geometry of a checked-out world | Layer dimensions are pre-scaled for downsampling [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). The world's `pix_aspect_ratio` is that layer's PAR (§6.1). Checked-out point values use the current time's origin shift, not the checkout time's [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). | MUST match. Depth equals the render depth (inference). | 6-R7 |

### 6.9 Downsampling, pixel aspect ratio and fields

| Item | SDK semantics | Host obligation | BAE mapping |
|---|---|---|---|
| `downsample_x/y` | Separate rationals for x and y. Point controls and layer dimensions are pre-scaled, but **scalar pixel distances (sliders) are not**: "a blur of 4 pixels should be interpreted as a blur of 2 pixels" at ½. Valid during sequence setup and resetup, frame setup and render [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). The guide warns that some "compatible" hosts passed **zero** [SDK: Effect Details › Pixel Aspect Ratio](https://ae-plugins.docsforadobe.dev/effect-details/pixel-aspect-ratio/). | MUST pass non-zero rationals and MAY use x ≠ y. MUST pre-scale point parameters by downsample and PAR into input-buffer coordinates, and MUST NOT scale sliders. | 6-R7. Points are stored in full-resolution layer units in 02; the host scales them per request. |
| PAR | Point parameters are "scaled for downsample factor and pixel aspect ratio… in the coordinate system of the input buffer". AE stretches horizontally, so a pixel used as a unit means a **vertical** pixel [SDK: Effect Details › Pixel Aspect Ratio](https://ae-plugins.docsforadobe.dev/effect-details/pixel-aspect-ratio/). | MUST supply the own-layer PAR in `in_data` and a PAR for every checked-out world. | 6-R7, 02/08 footage interpretation. |
| `field` | Legacy: valid only with `PIX_INDEPENDENT` [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). SmartFX: the request field is relative to the layer origin [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). Rowbytes does not change with field rendering [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/). Checking out the source returns a deinterlaced frame [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | BAE renders progressive frames, so the host MUST always request the whole frame. Field rendering is out of scope. | Crate `smart::Field::Frame`. |
| Quality | `in_data.quality` is HI or LO, and the graphics utilities behave differently in each [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). In AE this follows the layer's Best/Draft switch, which `AEGP_GetLayerQuality` exposes [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_layersuite9) (the mapping is inference). | MUST pass the layer's quality. Sampling utilities MUST switch kernels on it (§6.11). | 6-R4 / 6-R7 |

### 6.10 Motion blur and shutter

Effects produce their own motion blur, using `shutter_angle` and `shutter_phase`. They must set
`PF_OutFlag_I_USE_SHUTTER_ANGLE`, and they check out their own parameters at other times within
the shutter interval. `PF_OutFlag_WIDE_TIME_INPUT` is required when they look outside it
[SDK: Effect Details › Motion Blur](https://ae-plugins.docsforadobe.dev/effect-details/motion-blur/).
`shutter_angle` is zero unless motion blur is enabled and checked for the layer, and it is valid
only with the flag. `shutter_phase` is the offset from frame time to shutter open, as a percentage
of a frame [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/).

The guide is inconsistent on units: it says "0 to 1, which represents 360 degrees", but its own
example uses "== 180" for a half-frame shutter. P3 MUST resolve this against the real field
encoding (open question). The flag also feeds AE's caching decisions
[SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/#caching-behavior).

Host obligations:

- MUST pass zero shutter when the layer or comp switch is off.
- MUST include shutter values in the node's cache key only when the flag is set.
- MUST NOT time-supersample an AE effect node to produce motion blur. The effect blurs itself,
  so supersampling would double the blur and multiply the CPU cost. Graph-level motion-blur
  samples apply to the layer transform, downstream of the effects (inference; 6-R8).

### 6.11 Image services the host must implement

All of these run on CPU worlds of any of the three depths unless stated otherwise. They may be
called re-entrantly from many threads, and the callbacks reach plug-in-owned function pointers.
In out-of-process mode (§20) they **must therefore execute inside the helper process**, next to
the plug-in. Only allocation bookkeeping and progress/abort cross the process boundary
(inference). The legacy `in_data.utils` callback table must expose the same functions, because it
is "defined at all times" and older plug-ins use macros such as `PF_COPY` and `PF_FILL`
[SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/),
[SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/#clean-slate).

| Suite / functions | Semantics the plug-in relies on | Host obligation | P3 notes and risks |
|---|---|---|---|
| `PF_WorldSuite2`: `PF_NewWorld`, `PF_DisposeWorld`, `PF_GetPixelFormat` | Creates a world of a given size and pixel format, optionally cleared, and reports the format of any world [SDK: Effect Details › Image Buffer Management Functions](https://ae-plugins.docsforadobe.dev/effect-details/image-buffer-management-functions/). | MUST allocate any of the three formats, whatever the current render depth (inference). MUST clear to transparent black on request. MUST account the memory against the cache budget (6-R11). SHOULD track worlds per call and per instance, and reclaim leaks at sequence setdown. | Out of process: allocate in helper shared memory. |
| `PF_HandleSuite1`: new, lock, unlock, dispose, get size, resize | Relocatable blocks. AE locks handles it passes in and unlocks them afterwards. Plug-ins must use host memory for large allocations so that AE can purge caches under pressure, and must pass out-of-memory errors back [SDK: Effect Details › Memory Allocation](https://ae-plugins.docsforadobe.dev/effect-details/memory-allocation/), [SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/#testing). | MUST keep a locked pointer stable until unlock; resize MAY move the block. MUST return the out-of-memory error rather than abort, after first trying cache eviction (6-R11). Sequence and global data are these handles (§9). | Handle bookkeeping is host-local. |
| `PF_Iterate8Suite1`, `PF_Iterate16Suite1`, `PF_IterateFloatSuite1`: `iterate`, `iterate_origin`, `iterate_origin_non_clip_src`; 8-bit only: `iterate_lut`, `iterate_generic` | AE spreads the work over "as many processors are present", up to all cores since the October 2021 SDK. It uses no fixed traversal order and handles progress and abort itself. Arguments other than `dst` are read-only during the call. A null `area` means every overlapping pixel. `iterate_origin` offsets the source. `non_clip_src` passes transparent black outside the source. `iterate_lut` uses identity tables for null LUTs. `iterate_generic` runs N jobs with a thread index, and only thread 0 may call abort or progress [SDK: Effect Details › Iteration Suites](https://ae-plugins.docsforadobe.dev/effect-details/iteration-suites/), [SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/#responsiveness). | MUST parallelise over all cores without oversubscribing BAE's render pool (6-R10). MUST check abort between chunks and return the abort error, and MUST report progress. MUST NOT guarantee any order. | Under multi-frame rendering, nested parallelism inside several concurrent plug-in calls needs a shared pool, hence 6-R10. |
| `PF_WorldTransformSuite1`: `composite_rect`, `blend`, `convolve`, `copy`, `copy_hq`, `transfer_rect`, `transform_world` | These run "the same code After Effects does internally". `composite_rect` uses copy/behind/in-front with opacity and field. `blend` mixes two equal-size worlds by a ratio. `convolve` takes a per-channel kernel and must not run in place; only `USE_LONG` is implemented, and replicate-borders and alpha-weighted modes "will be ignored". `copy` preserves alpha. `transfer_rect` and `transform_world` take full transfer modes, an optional mask world and quality, and `transform_world` takes an array of matrices for motion blur [SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/#pf_worldtransformsuite1). | MUST implement at all three depths (inference; the AE guide does not limit depth, and only Premiere notes that `PF_BLEND` is 8-bit [SDK: Premiere Pro & Other Hosts › Bigger Differences](https://ae-plugins.docsforadobe.dev/ppro/bigger-differences/)). MUST reproduce the documented "not implemented, ignored" flag behaviour. | Highest fidelity risk. Resampling kernels and transfer-mode maths are unspecified, so P3 needs golden tests against AE. The host SHOULD reuse a CPU port of 04's blend-mode maths. |
| `PF_FillMatteSuite2`: `fill`, `fill16`, `fill_float`, `premultiply`, `premultiply_color` (+16/float) | A null colour fills transparent black, and a null rect fills the whole world. Premultiplication is with black, or with any colour for the `_color` variants, in either direction [SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/#pf_fillmattesuite2). | MUST implement exactly. Rounding for un-premultiply at alpha 0 is unspecified; leave colour 0 (inference). | Low risk. |
| `PF_Sampling8Suite1`, `PF_Sampling16Suite1`, `PF_SamplingFloatSuite1`: `nn_sample`, `subpixel_sample`, `area_sample` (8/16 only), plus `PF_BatchSamplingSuite1` | Coordinates are 16.16 fixed point, with parameters in a sample block (`PF_SampPB`). Outside the image counts as zero alpha. `subpixel_sample` is alpha-weighted interpolation in HI quality and nearest neighbour in LO, and treats integer coordinates as pixel centres ("0,0 is the center of the top left pixel"; callers subtract 0.5 to match AE's corner-origin geometry). `area_sample` is an alpha-weighted box of at most 256 × 256 with a radius of at least 1. Batch sampling has begin/end and optimised batch functions at 8/16 [SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/#sampling-images), [SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/#wheres-the-center-of-a-pixel). | MUST be thread-safe and very cheap per call, because plug-ins call them per pixel from iterate callbacks. MUST honour `in_data.quality`. | Must run in the helper process when out of process. Interpolation weights are unspecified, so golden tests are needed. |
| `PF_ColorCallbacksSuite` | See §6.6. | MUST implement. | Formula fidelity. |
| `PF_ANSICallbackSuite1` | Maths functions returning doubles, angles in radians, plus `sprintf` and `strcpy` emulation. These replace the old `PF_SIN`-style macros [SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/#pf_ansicallbacksuite1). | MUST implement with IEEE-correct libm. `sprintf` needs C varargs, so it belongs in the C shim. | Low risk. |
| `PF_ChannelSuite1` (auxiliary channels: depth, normals, motion vectors…) | Count, look up by index or type (with a found flag), check out and check in a channel [SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/#pf_channelsuite1). | MAY report zero channels, consistently, with "not found". | BAE media has no auxiliary channels in P3. |
| `AEGP_WorldSuite3`, `AEGP_CompositeSuite2` (used by effects unofficially) | `AEGP_World` with type 8/16/32, size, rowbytes, base addresses, `AEGP_FillOutPFEffectWorld` (borrows, does not own), `AEGP_FastBlur`, platform worlds. Composite provides `AEGP_CopyBits_LQ`, `_HQ_Straight`, `_HQ_Premul`, `AEGP_TransferRect` (null tables mean working space), `AEGP_PrepTrackMatte` and `AEGP_ClearAlphaExceptRect` [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_worldsuite3). | SHOULD implement, since it is cheap on top of the PF services. `TransferRect` MUST blend in the emulated AE mode (§6.5). | Effects are not officially entitled to these suites [SDK: AEGPs › Cheating Effect Usage of AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/cheating-effect-usage-of-aegp-suites/). |
| `PF_AppSuite`: `PF_AppColorPickerDialog`, `PF_InfoDrawColor` | The picker returns a float colour in working space, or cancel [SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/#pf_appsuite). | MUST show BAE's picker modally and convert its result into the emulated working encoding. | 11/12 UI plus 06 transforms. |

### 6.12 Per-call world pipeline (P3 reference)

This is what the host does around one CPU render call of an AE node. All steps except 5 run on
BAE's GPU and job system. Only step 5 runs plug-in code: in the helper process when out of
process, or in place.

1. **Plan.** 04 resolves the node's ROI, from the SmartFX pre-render (§7/§8) or the legacy
   frame-setup bounds (6-R12). It chooses the depth (§6.3) and the encoding (6-R2/6-R3). The
   stride and the output's valid extent follow from these (6-R9).
2. **Allocate.** It takes staging buffers from the pool (R04-02, R01-02): one per checked-out
   input and one for the output. Rows are aligned for GPU copies, and starts are page-aligned for
   shared memory.
3. **Encode.** A GPU pass per input converts the RGBA16F/32F region into the world layout: swizzle
   to A-R-G-B, scale to 255 or 32768 or pass float through, sanitise NaN, and transform the encoding
   only if 6-R3 requires it. The output buffer is cleared on the GPU (§6.7).
4. **Fill the call.** `in_data` gets the depth, downsample, PAR, quality, shutter, extent and
   origins, and point parameters are scaled and shifted (§6.9, §6.7). The worlds get rowbytes,
   width and height, DEEP and WRITEABLE flags, and PAR (§6.1).
5. **Call the plug-in.** It reads the inputs in place and writes the output in place. Image
   services (§6.11) run on the core worker pool (6-R10). Plug-in allocations are charged to the
   cache budget (6-R11).
6. **Decode.** One GPU pass converts the output's valid extent back to RGBA16F/32F; anything
   outside the extent becomes transparent black. It clamps 16-bit values above 32768 and applies
   the NaN policy. It never touches the RGB of zero-alpha pixels unless an encoding transform was
   needed.
7. **Release.** Staging buffers go back to the pool. Plug-in-allocated worlds and handles still
   alive at sequence setdown are reported and reclaimed.

Projects without AE nodes skip all of this. Nothing in steps 1–7 exists in their graph (§19).

### 6.13 BAE mapping and reservations

The core needs the following now. Everything else in this section is host-local P3 work.

| Id | To | Reservation | Why (AE basis) |
|---|---|---|---|
| 6-R1 | 06 | A queryable `WorkingSpaceDesc` per project and comp, with primaries, white point, transfer (`Linear` included), `linearized`, `blend_gamma_1`, `color_managed`, ICC profile bytes, and OCIO config source plus working, display and view names. | §6.5: the plug-in sees working-space values, and the ColorSettings queries must be answerable. |
| 6-R2 | 06 (+04) | A guarantee that working buffers are stored **in the working space's own encoding**, including non-linear sRGB in AE-default mode. "Blend in gamma 1.0" is a property of the blend operation, not a re-encoding of the buffer. | §6.5: this is what makes AE nodes pass-through, which matters for both performance and fidelity. |
| 6-R3 | 04 | CPU-node buffer descriptors state `encoding` (a working-space id) and `alpha: Premultiplied`. The R04-02 conversion pass can insert a 06-supplied GPU transform with a guarded unpremultiply. | §6.4 / §6.5 fallback when the encodings differ. |
| 6-R4 | 02 | The project stores the AE project bit depth and AE colour settings (working profile or OCIO, linearize, blend 1.0 gamma). Layer Prims store the AE quality switch (Best/Draft). | §6.3 depth choice, §6.5, §6.9 quality. |
| 6-R5 | 15 | AE import fills 6-R4: project bpc, colour settings, OCIO config reference, layer quality and sampling quality, and motion-blur switches with comp shutter angle and phase. | Without these, fidelity is lost at import and cannot be recovered later. |
| 6-R6 | 04 | ROI requests carry `preserve_rgb_of_zero_alpha` and a channel mask. Masks and empty-pixel trimming honour the flag, and no pass canonicalises the RGB of zero-alpha pixels. | §6.4: `REVEALS_ZERO_ALPHA`, `preserve_rgb_of_zero_alpha`. |
| 6-R7 | 04 | The render request context carries downsample x and y as non-zero rationals, the PAR of the node's own layer and of every input, the layer quality, and the field (always the whole frame for now). | §6.1, §6.8 and §6.9. |
| 6-R8 | 04 (+03) | The request exposes per-layer shutter angle and phase (zero when blur is off). The graph does not time-supersample effect nodes; motion-blur samples apply to transforms. | §6.10 |
| 6-R9 | 04 | Extends R04-02 and R04-03. Each CPU-node buffer has its own rowbytes, depth and order, and input and output strides are independent. Decoding can target RGBA32F. Outputs carry an "only this extent is valid" rect. Inputs are private per call, so they may be writeable. | §6.1 and §6.7. |
| 6-R10 | 19 (+01) | A host-provided parallel-for service in the native plug-in ABI and the core worker pool: N jobs with a thread index, cancellation, progress from job 0, callable from inside a running CPU node without oversubscription. | §6.11 iterate suites. |
| 6-R11 | 04 | A cache-budget hook: external allocators (plug-in worlds and handles) report their bytes and can ask for synchronous eviction before failing with out-of-memory. | §6.11 memory: AE purges caches to satisfy plug-ins. |
| 6-R12 | 04 | Legacy (non-SmartFX) nodes: output bounds and origin come from a plan-time callback (frame setup, possibly twice, with a zero-size skip), and the cumulative origin shift is propagated along a layer's effect stack. | §6.7 `I_EXPAND_BUFFER`, `pre_effect_source_origin`. |
| 6-R13 | 04 | Addressable layer stages as render dependencies: "layer source, time-remapped, before masks and effects" and "layer after effect *k*". Either can be requested at any time and ROI by another node. Coordinate with R04-01. | §6.8: legacy checkouts see the raw source without masks; SmartFX param 0 sees the preceding effects. |

**P3 risks, ranked.**

1. Colour-mode fidelity: whether worlds really are premultiplied with black in each AE colour mode,
   and what the ColorSettings queries return. Mitigation: a probe plug-in run in AE.
2. Resampling and transfer-mode fidelity of the WorldTransform and Sampling suites, which are
   unspecified. Mitigation: golden images.
3. Depth policy: driving at the project depth versus the deepest available.
4. Plug-ins that cast `data` and ignore `rowbytes`. The host must keep worlds exactly AE-shaped.
5. Thread oversubscription from nested iterate calls under multi-frame rendering.
6. Half-float quantisation of 16-bpc chains unless RGBA32F is used.

### 6.14 Check of the skeleton crate (`pixel.rs`, `geometry.rs`, `shaders.rs`, `convert.wgsl`)

| Module | Finding | Suggested fix |
|---|---|---|
| `pixel.rs` (module doc) | It says pixels are "premultiplied in the working colour space… so the host never (un)premultiplies". The guide never states the alpha convention (§6.4). The claim holds only when BAE's buffer encoding equals the emulated AE encoding (6-R2), and zero-alpha pixels may carry colour. | Reword it as a conditional fast path. Add an encoding field and a guarded-unpremultiply transform path. Test pass-through of zero-alpha RGB. |
| `pixel.rs` (16 bpc table, `decode_rows`) | The 32768 white point is correct, but the AE guide is silent on it: the source is the Premiere SDK page and Adobe staff. "Values above 32768 are invalid" is host policy, not documented AE behaviour. | Cite the sources, and label clamp-on-decode and never-produce-on-encode as policy. |
| `pixel.rs` (`FloatPolicy`) | NaN is sanitised only on encode. NaN or Inf in a plug-in's 32 bpc output goes into BAE buffers unchecked, and `convert.wgsl` decode does the same. | Apply the policy on decode too, defaulting to `ZeroNan`. |
| `pixel.rs` (`WorldDesc`) | It has no DEEP flag semantics (set only for 16 bpc), and `writable` is undocumented for inputs with `I_WRITE_INPUT_BUFFER`. The `origin_x/y` doc does not say that AE conveys the origin through `in_data` and SmartFX rects rather than through the world. `extent_hint` mixes coordinate spaces: the world hint versus the legacy `in_data` hint in original-layer coordinates. | Add `world_flags()`. Document the origin channels. Keep the `in_data` extent separately, in layer coordinates. |
| `pixel.rs` (`WorldDesc::rowbytes`, `validate`) | `usize` cannot represent a negative stride. Requiring a multiple of the channel size rejects odd strides that plug-in-built worlds passed to utility callbacks might use (the guide is silent). | Keep this for host-made worlds. Give the callback-side validation a byte-addressed path or a typed rejection. Document that GPU conversion needs rowbytes divisible by 4. |
| `pixel.rs` (`for_rect`) | `pixel_aspect` defaults to 1 and the extent defaults to the whole buffer. Both are placeholders. | Take PAR and extent as parameters. |
| `pixel.rs` (GPU order doc) | "GPU worlds are … B, G, R, A" is not in the AE guide. | Cite the Premiere naming and mark it for verification (§11). |
| `shaders.rs` / `convert.wgsl` | Decode always writes an `rgba16float` storage texture. In RGBA32F mode (R04-03), 16 and 32 bpc plug-in output is still quantised to half, and values above the half-float maximum become infinity. | Add `rgba32float` decode entry points or a second bind-group layout. |
| `caps.rs` (`preferred_cpu_depth`, used by §19) | It always picks the deepest supported depth. AE drives effects at the project depth, so an imported 8-bpc project would render float-aware effects differently (§6.3). | Take the AE project depth (6-R4) and a fidelity/quality policy. |
| `geometry.rs` | It is consistent with the guide: half-open rects in layer coordinates, rational downsample with the zero check, and exact 16.16 conversion. The sampling-centre convention ("0,0 is the centre of the top-left pixel") and the 20% extent padding are not modelled. | Add helpers for the sampling-centre offset and for the legacy `in_data.extent_hint`. |

## 7. SmartFX

SmartFX is the two-phase render protocol of modern AE effects: the host says
which output region it needs, the effect answers which inputs (layer, rect,
time) it needs and how big its result will be, and only then renders [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/).
It is the only route to 32 bpc float and to GPU rendering in AE [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/),
so (inference) every plug-in worth hosting at BAE's working precision is a
SmartFX plug-in. Conceptually it is exactly the ROI negotiation that 04 needs
anyway; the hard parts are that dependencies are discovered *by calling the
plug-in* (at plan time, for any layer at any time), that inputs are pulled
*lazily* during render, and that the plug-in can salt the cache key.

### 7.1 Opt-in, selectors and lifecycle

| Aspect | AE behaviour | Host obligation | BAE mapping | P3 risk |
|---|---|---|---|---|
| Opt-in | `PF_OutFlag2_SUPPORTS_SMART_RENDER` makes AE send `PF_Cmd_SMART_PRE_RENDER` / `PF_Cmd_SMART_RENDER` *instead of* `PF_Cmd_FRAME_SETUP` / `PF_Cmd_RENDER` / `PF_Cmd_FRAME_SETDOWN`; plug-ins may keep the legacy path for other hosts [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST drive the smart path whenever the flag is set; MUST NOT send legacy frame selectors to a smart effect (inference from "instead of"). | 04 `RoiModel::Negotiated` node. | Plug-ins whose legacy path is stale or untested. |
| Float and GPU | `PF_Cmd_RENDER` supports only 8 and 16 bpc; 32 bpc must go through `PF_Cmd_SMART_RENDER` [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/); `PF_OutFlag2_FLOAT_COLOR_AWARE` requires `SUPPORTS_SMART_RENDER` [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). At render time either `PF_Cmd_SMART_RENDER` or `PF_Cmd_SMART_RENDER_GPU` is sent, depending on whether a CPU or GPU frame is expected [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST NOT drive a non-smart effect at 32 bpc. GPU path: see §11. | 04 depth choice per node; 01/19 native device handles (§11). | — |
| Call pattern | `PF_Cmd_SMART_PRE_RENDER` may be sent any number of times before a single `PF_Cmd_SMART_RENDER` [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/); at most one render per pre-render, and render may never happen (bounds-only queries, or output found unnecessary, e.g. a track matte that does not intersect) [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST pair each render with exactly one prior pre-render (a plan handle), MAY discard plans freely. | 04 plan/execute split; plans are cheap, disposable objects. | Plan objects carry plug-in-owned memory (7.3). |
| Parameters | No parameter array is passed to either smart selector; non-layer params are fetched with `PF_CHECKOUT_PARAM` [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/); non-layer params may be checked out at any point, layer inputs only in pre-render [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST serve `PF_CHECKOUT_PARAM` (any time, §8) during both phases. | 04 + 03 evaluator callable from render workers. | Every pre-render may evaluate many params at many times. |
| Module lifetime | "Like AEGPs, SmartFX plug-ins are never unloaded by After Effects" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST keep the module mapped for the session once loaded. | 19 registry: non-unloadable (pinned) modules. | Conflicts with 19's hot unload goal. |
| Ignored legacy flags | `USE_OUTPUT_EXTENT`, `I_EXPAND_BUFFER`, `I_SHRINK_BUFFER`, `DOESNT_NEED_EMPTY_PIXELS` are "obsolete for SmartFX" [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). | MUST ignore them for smart effects. | `caps.rs` already ignores them via `RoiModel`. | — |
| Tracked wide time | `PF_OutFlag2_AUTOMATIC_WIDE_TIME_INPUT` only works for SmartFX; otherwise AE silently treats it as `PF_OutFlag_WIDE_TIME_INPUT` [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). | See §8.2. | 04 `TimeDependency::WideTracked`. | — |

### 7.2 Pre-render input (`PF_PreRenderExtra` → input)

| Field | AE meaning | Host obligation | BAE mapping | P3 risk |
|---|---|---|---|---|
| request `rect` | Output region wanted, in layer coordinates; may be empty, meaning "only compute `max_result_rect`" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST express it in the layer's pixel space at the current downsample (inference: the guide only says "layer coordinates"; reference sizes are explicitly full-resolution, 7.4). SHOULD send an empty rect when only bounds are needed. | 04 back-projects the viewer ROI / downstream request through the layer transform into layer space. | Conservative back-projection (motion blur, 3D) may over-request; under-requesting produces visible holes. |
| `field` | Field to render, relative to the layer origin; whether it falls on even or odd output lines depends on the output buffer origin [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | SHOULD always request whole frames unless BAE renders interlaced output (inference). | 08 if interlaced export is ever supported. | Low. |
| `channel_mask` | Channels the effect must produce; data written to other channels "will not be honored" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | SHOULD request all four; MAY request alpha only when every consumer is an alpha matte (inference). | 04 track-matte consumers. | Plug-ins that ignore the mask; host must still not trust other channels. |
| `preserve_rgb_of_zero_alpha` | Both an input (render colour of transparent pixels too) and an output (passed recursively in the effect's checkouts, else prior effects and masking eliminate those pixels); "discouraged, though still supported" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). Distinct from `PF_OutFlag2_REVEALS_ZERO_ALPHA` [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST propagate it into every checkout of that pre-render (the crate does) and MUST make upstream nodes keep the colour of zero-alpha pixels when asked. | 04 masks/trim must have a "keep RGB under zero alpha" mode; 06 premultiplication rules. | BAE works premultiplied; whether RGBA16F premultiplied buffers can carry colour under zero alpha is an open question (see open questions). |
| `bitdepth` | Depth the effect is driven at for this request [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/); repeated unchanged in the render input [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST pick it from the declared depths (8 always; 16 with `DEEP_COLOR_AWARE`; 32 with `FLOAT_COLOR_AWARE` plus SmartFX [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)) and keep it identical for the pre-render, its render and (inference) all input worlds handed out. | 04 per-node CPU depth (`caps.rs::preferred_cpu_depth`). | 8/16 bpc nodes quantise BAE's half-float data; see pixel-world section. |
| `what_gpu`, `device_index`, `gpu_data` | GPU framework and device for this request (AE 16.0+); the same framework is used for the matching `SMART_RENDER_GPU`; the host may re-call pre-render with another framework or none [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST remember the framework a plan was made for (see 7.8). | §11. | — |

### 7.3 Pre-render output (`PF_PreRenderOutput`)

| Field | AE meaning | Host obligation | BAE mapping | P3 risk |
|---|---|---|---|---|
| `result_rect` | Region the effect will produce, in layer coordinates; can be empty; may not exceed the request unless `RETURNS_EXTRA_PIXELS`; pixels inside the request but outside it are empty [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST validate; MUST treat the uncovered part of the request as transparent; SHOULD allocate the output world for `result_rect` only. | 04 plan result `result_rect`; compositor treats the rest as alpha 0. | Plug-ins that return a smaller rect but still draw outside it; the host must clip. |
| `max_result_rect` | Content bounds: the largest result for any request; must enclose every non-zero pixel; must not depend on the request but may depend on params and time; must be computed carefully, not set arbitrarily large; "fixed once the graph is built" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). Normally cropped to applied masks, but equals the layer size with `REVEALS_ZERO_ALPHA` [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST rely on it for culling and downstream bounds; SHOULD cache it per (instance, time, param/state hash); SHOULD flag request-dependent answers in debug builds (inference). | 04 node `content_bounds`, used for layer bounds, culling and viewer ROI. | Plug-ins returning huge bounds waste memory; host MAY clamp to a sanity limit and log. |
| `solid` | Every output pixel is fully opaque; "set if possible; it enables certain optimizations" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MAY use it. | 04 MAY skip layers fully covered by an opaque normal-mode layer (inference). | Wrong `solid` from a plug-in hides layers. |
| `flags`: `RETURNS_EXTRA_PIXELS` | The effect will return more pixels than requested [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST accept a larger output; SHOULD keep the extra pixels in the cache (inference). | 04 region-aware cache entry (§7.9). | — |
| `flags`: `GPU_RENDER_POSSIBLE` | GPU render is possible for this request with this framework; if unset the host may re-call pre-render with another framework or none [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST re-plan rather than reuse a GPU plan for a CPU render (7.8). | §11. | — |
| `pre_render_data` + delete function | Private data handed back in the matching render; once pre-render returns, AE owns it and disposes of it with the given function "or a standard free call"; because render may never follow, the effect must never free it itself [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST dispose of it exactly once — after the render, or when the plan is dropped without a render — in the plug-in's process (out of process: an opaque token, §20). | 04 plan carries a plug-in-owned payload with a drop hook. | Leaks if plans are dropped on cancellation paths; double free if a plan is cloned. |
| `frame_data` | Smart effects may also store handles in the out-data frame-data field "as with regular (non-smart) effects" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST carry it from the pre-render to the matching render (inference); who frees it when render never comes is undocumented (guide silent) — host frees the handle (inference). | Same plan payload. | Plug-in objects referenced from the handle can leak. |

### 7.4 `checkout_layer` during pre-render

The only pre-render callback documented on the SmartFX page is
`checkout_layer` [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/); the GUID mix-in (7.5) is documented elsewhere.

| Argument / result | AE meaning | Host obligation | BAE mapping | P3 risk |
|---|---|---|---|---|
| layer param `index` | Any layer parameter; for a SmartFX effect, param 0 is the effect's input with *previous effects applied* [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | MUST resolve param 0 to "this layer after effects 0..k-1" and other layer params to the referenced layer's source (8.1). | 04 input reference = (layer, effect-chain position) or (other layer source). | — |
| `checkout_idL` | Chosen by the effect; positive and unique [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). The same layer may be checked out several times under different ids [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST reject non-positive or duplicate ids within one pre-render (the crate does). | — | — |
| request | What the effect needs from that input (rect, field, channels, preserve flag); an empty rect returns `ref_width/height` and `max_result_rect` "without rendering" and is "fairly efficient" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST answer *without rendering pixels* (inference: required by the two-pass design); MUST support empty-rect bounds queries. | 04 bounds-only query on any node, recursively (an upstream AE node answers by running *its* pre-render). | Recursive pre-render of upstream AE nodes, possibly of the same module, while the downstream call is still inside the plug-in: plan-time re-entrancy. |
| `what_time`, `time_step`, `time_scale` | Any time, any step, any scale [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). Times may be sub-frame or negative; steps may be negative (reversed layer) or zero [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). | MUST convert exactly where possible and record the request in flicks (crate `time.rs`); MUST handle times outside the layer's in/out (8.1). | 04 dynamic dependency on (input, time). | Rounding of foreign scales; plug-ins computing `time / step` with step zero. |
| All possible inputs | Every layer input that might be needed must be checked out now, even if render later decides not to use it [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MAY rely on the plan as the complete set of pixel inputs. | 04 plan = complete pixel dependency set; params are not included (8.4). | — |
| `PF_CheckoutResult` | `result_rect` (may be empty), `max_result_rect`, pixel aspect ratio, `solid`, and `ref_width` / `ref_height` = the input layer's original size before effects, ignoring downsample, or the comp size for collapsed layers [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST fill every field; reference size MUST be full-resolution. | 04 `InputBounds` record. | Collapsed / continuously rasterised layers report comp size (8.1). |

### 7.5 GUID mix-in (plug-in-provided cache-key component)

`PF_OutFlag2_I_MIX_GUID_DEPENDENCIES` with `GuidMixInPtr()` lets a SmartFX
effect, during `PF_Cmd_SMART_PRE_RENDER`, mix any additional state that affects
the render (custom UI or `PF_Cmd_DO_DIALOG` changes to sequence data, licence
state, anything not otherwise factored in) into AE's internal GUID for the
cached frame; AE uses that GUID to decide whether the frame exists or must be
rendered [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). It replaces `PF_OutFlag_FORCE_RERENDER` and dialog-triggered
cache removal, can replace `OUTPUT_IS_WATERMARKED`, and, unlike
`FORCE_RERENDER`, lets cached frames be reused after an Undo [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). The
SmartFX page itself still says `checkout_layer` is the only pre-render
callback [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/); the guide is inconsistent here, and the exact placement of the
mix-in entry point is not documented in prose (guide silent).

| Host obligation | Detail |
|---|---|
| MUST accept any number of byte buffers per pre-render, order-sensitive, and fold them into the node's cache key for that (time, request). | The mix-in *adds* to the host key; it never replaces the host's own key components (inference). |
| MUST run pre-render *before* the cache lookup for effects that set the flag. | The key is only known after the plug-in has been called (inference). For such nodes 04's order is plan → key → lookup → (render or hit). |
| SHOULD use a collision-resistant hash of at least 128 bits for the combined key. | Frame and disk caches outlive a session; a 64-bit non-cryptographic digest (current crate) is too weak for a persisted key (inference). |
| SHOULD still honour mix-ins from effects that did not set the flag. | Guide silent; mixing is conservative. |

### 7.6 Smart render callbacks (`PF_SmartRenderExtra`)

| Callback | AE meaning | Host obligation | BAE mapping | P3 risk |
|---|---|---|---|---|
| Render input | Same request, bit depth and `pre_render_data` as the corresponding pre-render [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/); GPU fields added for `SMART_RENDER_GPU` [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST pass the plan's record unchanged. | 04 execute(plan). | — |
| `checkout_layer_pixels(id)` | Pixels of a layer checked out in pre-render; the world stays valid for the current command or until checked in; allowed only once per id; to read a layer twice, check it out twice in pre-render under two ids [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). During `SMART_RENDER_GPU` all check-outs return GPU frames [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST refuse ids that were not checked out in pre-render and a second fetch of the same id. SHOULD materialise the input *on this call* (lazy), because inputs checked out in pre-render but never fetched "need never be rendered" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | 04 `pull(input)` during execute: synchronous, possibly running an upstream subgraph. | Lazy pulls serialise latency inside the plug-in call and can starve a worker pool (7.9). |
| `checkin_layer_pixels(id)` | Optional; AE cleans up all checkouts when render returns; useful to free memory early [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST implement; MUST release the buffer (or its pool slot) immediately; later access to that world is undefined. | 04 `release(input)`. | Missing in the crate. |
| `checkout_output` | Output world; not allowed before at least one input has been checked out unless the effect has no inputs; request output as late, and inputs as few at a time, as possible [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST allocate the output lazily on this call. SHOULD tolerate (log, not fail) an output request that precedes every input fetch, because the guide also says the effect need not use its inputs (inference). | 04 lazy output allocation from the texture/staging pool. | Strict enforcement (current crate) breaks plug-ins that skip all inputs on some frames. |

### 7.7 Rules and edge cases

| Situation | Documented rule | Host policy |
|---|---|---|
| Checkout id ≤ 0 or reused within one pre-render | Must be positive and unique [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | Return an error to the plug-in. |
| Pixel fetch for an id not checked out in pre-render | Effect can only retrieve pixels it requested [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | Return an error. |
| Same id fetched twice | Only once per id [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | Return an error. |
| Ids checked out but never fetched | "One-to-one mapping" is stated, but also "you aren't required to actually use every input" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST tolerate; never render those inputs (lazy) or drop prefetched ones. |
| No layer checkouts at all (generators) | Output may be checked out immediately [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | Allow. |
| Effect does not check out its own input | First input may be found unnecessary (matte example) [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST NOT render the upstream effect chain for that frame. |
| Empty request rect | Bounds-only call [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST NOT follow with a render. |
| Empty `result_rect` returned | "No rendering need be done" [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MAY skip `SMART_RENDER`; MUST still dispose `pre_render_data`. |
| Pre-render without render | Must not leak or destabilise the effect [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | Dispose plan payload; no further call needed. |
| Several pre-renders, then one render | Allowed [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | Render must reference one explicit plan; other plans disposed. |
| Pre-render called twice for one frame | Premiere (MediaCore) does this: first with full-frame source sizes, then again if the real sizes differ [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MAY re-plan when upstream bounds become known (precedent). |
| Render interrupted (`PF_ABORT` / `PF_PROGRESS` error) | Results are not used (stated for `PF_Cmd_RENDER`) [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST NOT cache an aborted smart render either (inference). |
| `max_result_rect` differs between two requests at the same time/params | Must not happen [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | Use the first answer; log in debug builds (inference). |

### 7.8 Interaction with the render cache, downsampling and GPU re-planning

* **Cache correctness depends on the rects.** AE's caching relies on accurate
  result rectangles; too-large rects waste cache memory and render time [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/).
  Glitches after enabling caching are "likely a legitimate bug ... such as
  improper rectangle handling in SmartFX"; the effect version and the AE build
  are part of AE's cache key [SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/). Host: MUST include the plug-in identity
  and effect version in every key of an AE node (04).
* **Downsampling.** `in_data` width/height are the source size and are not
  affected by downsampling; downsample factors are ratios that effects use to
  scale pixel-distance parameters, while point parameters arrive already
  scaled [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/) [SDK: Effect Details › Pixel Aspect Ratio](https://ae-plugins.docsforadobe.dev/effect-details/pixel-aspect-ratio/). The guide lists the downsample fields as valid only in
  sequence setup/resetup, frame setup and render [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/); it is silent for the
  smart selectors, so the host SHOULD fill them for `SMART_PRE_RENDER` /
  `SMART_RENDER` as well (inference). Some third-party hosts have passed zero
  factors [SDK: Effect Details › Pixel Aspect Ratio](https://ae-plugins.docsforadobe.dev/effect-details/pixel-aspect-ratio/): MUST never do that. All rects (request, result, checkout) are
  in downsampled pixels; `ref_width/height` are full resolution [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). The
  downsample factor MUST be part of the cache key and MUST be identical for
  every input of one render (inference).
* **GPU re-planning.** If `GPU_RENDER_POSSIBLE` is not set, the host "may
  re-call PreRender with another what_gpu option (or none)", and the framework
  is the same between pre-render and `SMART_RENDER_GPU` [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). Host: MUST
  dispose the GPU plan and run a fresh CPU pre-render before a CPU render
  (inference: checkouts and `pre_render_data` may differ per framework).
* **Content bounds are intrinsic** to the node and its inputs [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/): 04 MAY
  cache them separately from pixels and reuse them for every viewer ROI.

### 7.9 BAE mapping (04) and P3 risks

| AE construct | BAE 04 construct (to reserve now) |
|---|---|
| `PF_Cmd_SMART_PRE_RENDER` | `plan(request) → plan result`, *called on the node at plan time*; repeatable; discardable. |
| `checkout_layer` | Plan context: `input_bounds(input ref, rect, time, step, flags)` → result rect, content bounds, pixel aspect, opaque, reference size; never renders. |
| result / max result / solid / extra pixels | Plan result fields; cache entries are region-aware. |
| GUID mix-in | Plan result `cache_salt` (opaque bytes) folded into the node key. |
| `pre_render_data` / smart `frame_data` | Plan result private payload with drop hook. |
| `PF_Cmd_SMART_RENDER` | `execute(plan, ctx)` with `ctx.pull(input)`, `ctx.release(input)`, `ctx.output()`. |

Risks: (1) **Plan cost.** Every frame and every ROI change calls into
plug-in code (an RPC out of process, §20) before anything can be scheduled; 04
SHOULD cache plan results keyed by (instance, time, request, param hash,
state-slot hash, upstream bounds) except where the plug-in mixes a GUID that
might change (inference). (2) **Starvation.** A worker blocked in
`checkout_layer_pixels` waits for an upstream subgraph; if all workers are
blocked the pool deadlocks unless the executor runs the subgraph inline on the
waiting thread or adds a compensating worker (inference). (3) **Re-entrancy**
at plan and execute time into the same module (8.3), which collides with
per-module serialisation (10.7). (4) **Lazy vs eager.** Eager prefetch of all
planned inputs maximises GPU overlap but renders inputs that a plug-in may
skip; the host SHOULD prefetch only inputs that are cached or cheap and pull
the rest lazily (inference).

## 8. Layer and parameter checkout at other times

### 8.1 `PF_CHECKOUT_PARAM` semantics

`PF_CHECKOUT_PARAM` obtains a parameter's value, or a layer's pixels, at a
time given as (`what_time`, `step`, `time_scale`); AE "makes caching decisions
based on the checkout state of parameters"; every checkout must be balanced by
`PF_CHECKIN_PARAM`, after which the returned definition is invalid [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/).

| Behaviour | AE rule | Host obligation | BAE mapping | P3 risk |
|---|---|---|---|---|
| Result storage | The plug-in allocates a fresh param definition for the result; the ones passed in are read-only [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | MUST write only into the caller's struct; MUST keep checked-out layer worlds alive until check-in. | — | Unbalanced checkouts leak ("dismal performance") [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/); host SHOULD reclaim all outstanding checkouts when the selector returns (inference). |
| Layer param set to none | Returned layer is filled with zeros [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | MUST return a valid transparent world. | 04 empty source. | — |
| Masks | Not included with checked-out layers [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | MUST render the layer without its masks. | 04 input variant "source without masks". | — |
| Effects on the checked-out layer | A non-smart effect checking out a layer gets the raw, un-effected source regardless of order; a SmartFX effect checking out param 0 gets previous effects applied [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | MUST provide both variants. | 04 input ref: (layer source) vs (layer at effect index k). | The guide does not say what a SmartFX effect gets for *other* layer params (guide silent); P3 must verify against AE. |
| Trimmed layers | Valid frames from source start to end regardless of in/out trims; transparent before and after [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | MUST ignore in/out points; MUST return transparent outside the source range. | 04 source evaluation independent of layer trim. | Differs from BAE's normal layer visibility. |
| Sub-frame times | Infinite time resolution: comps interpolate keyframes; footage returns the nearest frame to the left; frame blending produces an interpolated frame [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | MUST honour sub-frame requests exactly as above. | 04 + 03 evaluation at arbitrary flicks; 08 frame lookup "nearest left". | — |
| Lower frame-rate sources | A 10 fps layer in a 30 fps comp only refreshes every third frame; effects that must change every frame set `NON_PARAM_VARY` [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | MAY reuse the effect's cached output across a source frame's duration unless `NON_PARAM_VARY` (inference). | 04 time quantisation in cache keys. | — |
| Continuously rasterised vector layers | Rendered with geometrics applied in a comp-sized buffer [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | MUST do the same for BAE vector/text layers with "collapse" semantics. | 04/05 layer source. | Large buffers. |
| Interlaced sources | A deinterlaced field is returned; check out twice to reinterlace [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). | SHOULD match if 08 supports fields. | 08. | Low. |
| Time arithmetic | Use `time_step`, not `local_time_step`, to compute other source times; `time_step` may be negative, zero, or vary per frame [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). Effects may be asked for negative or non-integral times [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). | MUST supply consistent `time_step` / `time_scale` and accept any requested time. | crate `time.rs` per-instance time base. | 32-bit tick overflow on long comps at fine scales (inference). |
| Point params | Checked-out point parameters are adjusted for the pre-effect origin at the *current* time, not the checkout time [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). | MUST replicate. | params translation. | Subtle offsets with animated buffer expansion. |
| Where allowed | Forbidden for layer params during UI event handling [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/); never during `PF_Cmd_UPDATE_PARAMS_UI` [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/) (since 13.5 a layer checkout there returns a black frame of the right size [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)); allowed (non-layer, other times) in `PF_Cmd_DO_DIALOG` [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/); non-layer checkouts are how `PF_Cmd_QUERY_DYNAMIC_FLAGS` reads params [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/); SmartFX layer inputs only via `checkout_layer` in pre-render [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | MUST return a black world of correct size for layer checkouts in `UPDATE_PARAMS_UI`; SHOULD return an error for layer checkouts in event handling (inference). | UI-thread calls never trigger renders. | Plug-ins relying on old synchronous behaviour. |

### 8.2 Cache-relevant flags and state queries

| Mechanism | AE meaning | Host obligation | BAE mapping |
|---|---|---|---|
| `PF_OutFlag_WIDE_TIME_INPUT` | The effect uses a param (including layers) from another time; without it cached frames are not invalidated correctly; set at global setup [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/); may change via `QUERY_DYNAMIC_FLAGS` [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST treat the node as depending on its inputs at *all* times: any upstream change at any time invalidates it (that is AE's behaviour when only this flag is set [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | 04 `TimeDependency::WideUntracked`. |
| `PF_OutFlag2_AUTOMATIC_WIDE_TIME_INPUT` | Requires `WIDE_TIME_INPUT` but overrides it; all checkouts are tracked so a frame that read times 0–17 is not invalidated by changes at 18+; SmartFX only; the effect must not cache time-dependent data unless it validates it with `PF_GetCurrentState` / `PF_AreStatesIdentical` [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). | MUST record every (input, time) checkout of a render — layer *and* non-layer params, in pre-render *and* render — with the cached result, and invalidate only on overlapping changes. | 04 dependency journal on cache entries (§8.4). |
| `PF_OutFlag_NON_PARAM_VARY` | Output varies with something other than params (e.g. particles, Wave Warp); AE then does not cache output when the effect is applied to a still; dynamic [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST include time in the node key even when all inputs are time-invariant. | 04 `cacheable_for_stills = false`. |
| `PF_OutFlag2_I_MIX_GUID_DEPENDENCIES` | See 7.5 [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | Fold salt into key. | 04 plan `cache_salt`. |
| `PF_OutFlag_FORCE_RERENDER` | The effect made a change requiring a re-render [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). Since 13.5 it is also the trigger for copying UI-side sequence data to the render copy; it works in `USER_CHANGED_PARAM`, and in click/drag events only if `GET_FLATTENED_SEQUENCE_DATA` is implemented; Adobe recommends GUID mix-in, arbitrary data or `PF_ChangeFlag_CHANGED_VALUE` instead because those keep cached frames reusable after Undo [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | MUST, when set after a UI selector, snapshot the instance state into the state slot and invalidate the node (§9.3). | 02 state-slot Command → StageEvent → 04 invalidation. |
| `PF_ChangeFlag_CHANGED_VALUE` | Forces a re-render; since 14.0 not for layer or path params [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | Follow the param section. | — |
| Shutter, 3D camera/lights, comp timecode, unreferenced masks, watermark, synthesised audio | `I_USE_SHUTTER_ANGLE`, `I_USE_3D_CAMERA`, `I_USE_3D_LIGHTS`, `I_SYNTHESIZE_AUDIO` all influence caching [SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/); `I_USE_COMP_TIMECODE`, `DEPENDS_ON_UNREFERENCED_MASKS`, `OUTPUT_IS_WATERMARKED` (whose next render must match the last dynamic answer to avoid races) [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). | MUST add the corresponding inputs to the node key / dependency set. | 04 `CacheKeyInputs` (crate). |
| `PF_Cmd_QUERY_DYNAMIC_FLAGS` | Sent at arbitrary times; the effect reads non-layer params and sets the dynamic flags; any flag it may ever set must already be set at global setup; "respond as quickly as possible" [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST re-query when params change and before planning a render whose flags might differ (inference); MUST treat global-setup flags as the upper bound. | 19 dynamic-flags hook; 04 re-derives node requirements (`caps.rs`). |
| `PF_GetCurrentState` / `PF_AreStatesIdentical` (`PF_ParamUtilSuite3`) | Opaque receipt of the state of selected params (all, all except layers, honour `EXCLUDE_FROM_HAVE_INPUTS_CHANGED`) over a time span or all time; with `AUTOMATIC_WIDE_TIME_INPUT` the span is expanded to all times needed to produce it [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/). Change tracking uses timestamps; when the effect uses its cache after a successful check, AE assumes a temporal dependency on the passed range [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/). Returns a random value during `UPDATE_PARAMS_UI` [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | MUST compute receipts *without rendering*; MUST register the queried span as a dependency of the current render in tracked mode. | 04 input-state hash query; 02 per-attribute change stamps. |
| `PF_IsIdenticalCheckout` | Whether a param has the same value at two times (values in between may differ) [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/). | MUST implement from the evaluated curves. | 03/04. |
| Effect version | Part of AE's cache key, so plug-in updates invalidate stale disk-cache frames [SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/). | MUST be in the node key. | 04 key namespace per (match name, version). |

### 8.3 Re-entrancy

Checking out layers at other times produces recursion: effect X in comp A
checks out precomp B, and rendering B sends render to another instance of X
(or the same module) while the first call is still running; plug-ins "must
handle re-entrant render requests" [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). Under MFR, a plug-in must not hold
its own locks when calling back into the host [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). Host consequences:

* MUST allow nested renders, including nested calls into the same module, on
  the thread that is blocked in the callback (inference); a serialising lock
  for non-MFR modules (10.7) MUST therefore be re-entrant for the owning thread.
* MUST detect true cycles (an effect requesting, directly or through a precomp,
  its own output at the same time) and fail the checkout with an error rather
  than deadlock (inference).
* MUST NOT hold any host lock across a call into plug-in code that a callback
  of the same call would need (inference).

### 8.4 BAE mapping and P3 risks

| Dependency class | Discovered | BAE 04 mechanism |
|---|---|---|
| Current-time params and (legacy) param 0 | Statically, before the call. | Normal graph edges. |
| SmartFX layer inputs (`checkout_layer`) | At plan time, by calling the plug-in. | Plan-time dynamic edges (7.9). |
| `PF_CHECKOUT_PARAM` of non-layer params at any time; legacy layer checkouts at other times; AEGP frame renders | Only during execution. | Synchronous nested evaluation + dependency journal. |

* **Dependency journal.** Each cached result of an AE node stores the list of
  (Prim, layer-local time interval) it touched; 04 maps those through the
  layer's time stretch, remapping and nested-comp rate into source time and
  intersects them with 02's `StageEvent` (which already carries affected Prim
  paths and time ranges, `docs/ARCHITECTURE.md` §4.5).
* **Evaluation at arbitrary times.** 03's evaluator, including expressions, must
  be callable concurrently from render workers for any flicks value.
* **Prefetch.** Legacy wide-time effects reveal their other-time inputs only by
  calling back; the host MAY learn per-instance access patterns (e.g. "frames
  t-1…t-4") from previous frames and prefetch them speculatively (inference).
* **Risks.** Echo/time-displacement style plug-ins can request dozens of frames
  per output frame; with N parallel renders this multiplies memory, so 04 needs
  back-pressure on transient frames (inference). `WideUntracked` effects
  invalidate on every upstream edit, which is AE-faithful but slow; only
  `AUTOMATIC_WIDE_TIME_INPUT` plug-ins get precise invalidation.

## 9. Global, sequence and frame data

### 9.1 The three scopes (plus the compute cache)

| Scope | Created / destroyed | Persisted | Access rules | BAE home |
|---|---|---|---|---|
| Global data | Allocated in `PF_Cmd_GLOBAL_SETUP` (which also reports flags and version), freed in `PF_Cmd_GLOBAL_SETDOWN`; setdown is sent when AE quits or when the last instance of the effect is removed, and must not be used to detect unloading [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). Handle returned in every call [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/); locked/unlocked by the host around each call [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). | Never [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/). | Under MFR: write only in global setup/setdown, read anywhere [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/); global setup/setdown run on the main thread and never concurrently with any other selector [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). | Host-side per-effect-type record (19 module state). |
| Sequence data | Allocated in `PF_Cmd_SEQUENCE_SETUP` per instance [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/); freed in `PF_Cmd_SEQUENCE_SETDOWN` [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | Saved in the project after flattening [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/). | Contents may change in any selector, size only in the selectors listed in 9.4 [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/); under MFR read-only during render (§10.3). | 02 opaque state slot (flat form) + live form inside the host process. |
| Frame data | Allocated in `PF_Cmd_FRAME_SETUP`, freed in `PF_Cmd_FRAME_SETDOWN`, cannot be deleted during `PF_Cmd_RENDER` [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). "Fallen into disuse" [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/). | Never [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). | Per render call. | Per-call host state (9.5). |
| Compute cache | Registered class, entries computed on demand [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/). | Never; recomputed after reopening [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/) [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/). | Thread-safe, shared across render threads [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/). | 04 unified cache (§10.6). |

### 9.2 Global data lifecycle — host obligations

* MUST send `PF_Cmd_GLOBAL_SETUP` then `PF_Cmd_PARAM_SETUP` before any instance
  selector [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), on the main thread with nothing else in flight [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/).
* MAY defer `PF_Cmd_GLOBAL_SETDOWN` to session end (AE sends it on quit too
  [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)), which avoids setup/setdown churn when the last instance is deleted and
  then restored by Undo (inference). Before sending it the host MUST drain every
  in-flight call of that effect, including UI calls and compute-cache callbacks.
  After a setdown, a re-applied effect gets a fresh global setup (inference).
* MUST NOT unload the module after setdown during the session; the guide tells
  plug-ins to rely on OS unload entry points instead [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), and SmartFX modules
  are never unloaded [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/).
* Compute-cache classes are typically registered in global setup and
  unregistered (purging all entries) in global setdown [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/);
  `AEGP_RegisterWithAEGP` must be called in global setup only [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/).
* Out of process (§20): a sandbox restart repeats global setup; everything the
  plug-in kept in globals or the compute cache is lost.

### 9.3 Sequence-data events

| Event | AE selector behaviour | Plug-in expectation | Host MUST / SHOULD | BAE (02 slot, host instance) |
|---|---|---|---|---|
| Effect applied | `SEQUENCE_SETUP`; in-data is initialised [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/); if `SEND_DO_DIALOG` was set, `DO_DIALOG` follows [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). `time_step` is zero here if not constant [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/). | Allocates live data. | MUST send setup once per new instance. | "Apply effect" Command creates the Prim; host snapshots the initial blob into the slot within the same undo unit (BAE policy). |
| Project opened | `SEQUENCE_RESETUP` after the data is read from disk [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/); may receive flat or unflat data [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/). | Unflattens. | MUST pass the saved flat bytes in a host-allocated handle. | UI instance restored from the slot; render instances on demand (9.6). |
| Save | `SEQUENCE_FLATTEN` when the project is written: the effect frees its unflat data and points sequence data at a flat, correctly byte-ordered block [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/); after a recent flatten the instance may be deleted without `SEQUENCE_SETDOWN`, and AE frees the flat data [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | Destructive. | If used: MUST track the instance as "flat" and MUST resetup it before any other selector (inference; guide silent on what follows a save); MUST free the flat handle itself if the instance is deleted while flat. | BAE save writes the slot as is; host SHOULD refresh dirty instances with the non-destructive snapshot (10.3) instead of a destructive flatten. |
| Duplicate | Flatten before duplication; resetup for both old and new instance; no flatten is guaranteed between resetups [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | Old instance may be flat or unflat at resetup. | MUST resetup both after a destructive flatten. | Duplicate Command copies the slot verbatim; new instance from the copy via resetup. |
| Copy / paste | Resetup "when the effect is copied" [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | — | MUST create the pasted instance by resetup from the copied flat bytes. | Clipboard carries the blob with the Prim. |
| Precompose | Resetup "during pre-composition" [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | — | MUST resetup an instance whose layer moved into a new comp. | 02 must report the move (reservation). |
| Layer changed | Resetup "when the layer to which it's applied changes" [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | — | MUST resetup on re-parenting. | 02 re-parent event. |
| Effect removed, project closed, quit | `SEQUENCE_SETDOWN` [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | Frees everything. | MUST send setdown for live instances. | Delete Command keeps the slot in undo history; undo recreates the instance by resetup. |
| Undo / redo | Guide silent on sequence data. Dialog changes are "not undoable" [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/); GUID mix-in / arbitrary data / changed-value keep cached frames reusable after Undo [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | Sequence data is normally not rolled back by AE (inference). | BAE deviation: slot changes are undoable; MUST replace the live instance (setdown, then a new instance resetup from the restored blob) rather than resetup over live data (inference: no selector frees live data except setdown). | 02 undo restores the slot; StageEvent tells the host. |
| Collaborator change (BAE only) | — | — | Same replacement path as undo. | 09 sync updates the slot (whole-value last-writer-wins). |
| UI → render copy (13.5+) | AE serialises project changes from the UI thread to the render thread's own project copy, "even more often" than before; `GET_FLATTENED_SEQUENCE_DATA` returns a flat copy without disposing the live structures; render instances get resetup with `PF_InFlag_PROJECT_IS_RENDER_ONLY` (read-only project, no UI selectors); resetup is sent on both UI and render threads [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | Render instance is separate from the UI instance. | MUST build render instances from snapshot bytes and set the render-only flag. | Render instances keyed by slot content hash (9.6). |
| UI change to sequence data | `FORCE_RERENDER` after `USER_CHANGED_PARAM`, or after click/drag only with the non-destructive flatten selector [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/); render cannot pass sequence-data changes back to the UI [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | Expects the render side to see its change. | MUST snapshot after any UI selector that may write (user-changed-param, event, dialog, completely-general), compare bytes, and commit only on change; SHOULD coalesce a drag into one commit at mouse-up. | `SetEffectState` Command (one undo unit per gesture). |
| MFR render start / end | Sequence data marshalled to render threads; const during render; with `MUTABLE_RENDER_SEQUENCE_DATA_SLOWER` a private copy per render thread, discarded after each span (one RAM preview or one render-queue export) [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/) [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). | Writes in render are lost. | MUST never write render-side changes back. | 04 render-span lifecycle (10.8). |
| Sandbox crash (BAE only) | — | — | Restart, global setup, resetup every instance from its last committed blob (§20). | UI state not yet committed is lost (risk). |

### 9.4 Flat format, handles and size

* **Flattening** means collecting everything pointed to into one contiguous,
  disk-safe block, including platform byte order, so projects move between
  macOS and Windows [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/). Resetup can receive flat or unflat data; plug-ins
  are told to keep a state flag at a common offset [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/). The host never
  interprets the bytes.
* **Always assume flattening is needed.** "Both After Effects and Premiere Pro
  assume" `PF_OutFlag_SEQUENCE_DATA_NEEDS_FLATTENING` is set [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/): the host
  MUST obtain persisted bytes only through flatten or the non-destructive
  snapshot selector, never by copying the live handle.
* **Host-owned handles.** `flat_sdata_size` is unused because AE knows the size
  from its own allocator [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/): the host MUST allocate global/sequence/frame
  data through its handle suite and track each handle's size; out of process
  the live handle stays in the sandbox and only flat bytes cross (§20).
* **Resizing** is allowed only in audio setup/setdown/render, frame
  setup/setdown, render, sequence setup/setdown/flatten/resetup and do-dialog;
  contents may change in any selector [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/). Under MFR, render-time writes are
  forbidden unless the plug-in set the mutable flag [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/), so (inference) a
  resize in a render selector is legal only on a private per-thread copy.
* **Versioning** of the blob format is the plug-in's business (guide silent);
  the host records producer match name and effect version next to the bytes
  (crate `StateBlob`).
* `PF_OutFlag2_PPRO_DO_NOT_CLONE_SEQUENCE_DATA_FOR_RENDER` is Premiere-only,
  unsupported in AE [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) and discouraged [SDK: Premiere Pro › Multithreading](https://ae-plugins.docsforadobe.dev/ppro/multithreading/): MUST be ignored.

### 9.5 Frame data and per-call state

* Legacy order per frame is frame setup → render → frame setdown [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). With
  `I_EXPAND_BUFFER` the plug-in gets frame setup/setdown twice, once without a
  render in between (visibility probe) [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). Frame setup may resize the output;
  zero width/height makes AE ignore the following render [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/).
* Host: MUST keep `frame_data`, `in_data`, `out_data` and the params array
  *per in-flight call* (per thread under MFR) and SHOULD run one frame's
  setup/render/setdown on the same thread back to back (inference).
* SmartFX: `pre_render_data` and optional `frame_data` live in the plan (7.3).

### 9.6 BAE mapping

**02 state slot.** One optional opaque value per effect Prim: format tag,
producer (match name), producer version, content hash and bytes (large blobs
stored content-addressed out of line). Written only by a Command; treated by
the CRDT as one last-writer-wins register (never merged); copied verbatim on
duplicate, copy/paste, precompose, `.baez` packaging and AE import; never
animated. The slot always holds the *flat* form.

**Host instances per Prim.** (a) one *UI instance* that receives all UI
selectors and owns the authoritative live state between snapshots; (b) zero or
more *render instances*, each restored with the render-only flag from the slot
value of the Stage snapshot being rendered, shared by all render threads when
MFR-safe, private per worker with the mutable flag, discarded when the slot
hash changes or the render span ends. This mirrors AE's separate UI and render
project copies [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/) and makes every frame consistent with the snapshot its
cache key describes (inference).

| BAE hook (19) | AE selector(s) |
|---|---|
| `instance_create` | `SEQUENCE_SETUP` |
| `instance_restore(blob, cause, render_only)` | `SEQUENCE_RESETUP` |
| `instance_snapshot` (non-destructive) | `GET_FLATTENED_SEQUENCE_DATA`; fallback `SEQUENCE_FLATTEN` + `SEQUENCE_RESETUP` |
| `instance_destroy` | `SEQUENCE_SETDOWN` (or free the flat handle if the instance is flat) |
| `global_init` / `global_shutdown` | `GLOBAL_SETUP` + `PARAM_SETUP` / `GLOBAL_SETDOWN` |

**Risks.** Large or frequently changing blobs bloat the CRDT log (mitigation:
content-addressed storage, gesture coalescing). Last-writer-wins can combine
collaborator A's blob with collaborator B's parameter edits, a state the
plug-in never produced; plug-ins already face similar combinations through
copy/paste, but some may misbehave (inference). Undoable sequence data is a
deliberate improvement over AE that some plug-ins (which assume monotonic
state, e.g. licence or cache counters) might not expect. Plug-ins without the
non-destructive snapshot force a flatten + resetup round trip per snapshot,
which can drop transient UI state during drags; BAE SHOULD snapshot those only
at gesture end.

## 10. Multi-Frame Rendering

### 10.1 Flags and version gates

| Flag / gate | AE meaning | Host obligation | BAE mapping |
|---|---|---|---|
| `PF_OutFlag2_SUPPORTS_THREADED_RENDERING` | Set at global setup: single or multiple applications of the effect may render at the same time on multiple threads; only for plug-ins tested as thread-safe [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). | MAY call any instance concurrently from many render threads. | 04 `Concurrency::Parallel`; 19 thread-model declaration. |
| `PF_OutFlag2_MUTABLE_RENDER_SEQUENCE_DATA_SLOWER` | Sequence data replicated per render thread so render may write it; copies discarded after each render span; less concurrency [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/) [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/) [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | MUST give each render thread its own copy at span start; MUST discard at span end. | 04 `ParallelWithPrivateState` + span lifecycle. |
| `PF_OutFlag2_SUPPORTS_GET_FLATTENED_SEQUENCE_DATA` + `PF_Cmd_GET_FLATTENED_SEQUENCE_DATA` | Returns a flat copy without disposing live data; needed to marshal sequence data to render threads efficiently; effects that flatten "must now also set" it, and a future AE will enforce it with a warning [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/) [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | SHOULD prefer it for every snapshot; MUST fall back to flatten + resetup otherwise. | 19 `instance_snapshot`. |
| `PF_OutFlag_SEQUENCE_DATA_NEEDS_FLATTENING` | Assumed set by AE [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). | Always flatten (9.4). | — |
| `PF_OutFlag2_AE13_5_THREADSAFE` | Introduced in 15.0: UI thread and render thread may be in the effect at the same time on different project copies, but not on the same instance [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | MAY run UI selectors concurrently with render calls of that module when set (inference). | Missing from crate `caps.rs`. |
| `PF_OutFlag2_I_AM_THREADSAFE` | "Currently this does nothing" [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). | MUST ignore. | — |
| SDK version gate | Plug-ins must be built with the March 2021 SDK and report at least effect API 13.25, otherwise AE (22.0x6+) stops using MFR for them even if the flag is set [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/) [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/); the build version is the PiPL's `AE_Effect_Spec_Version` [SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/). | MUST treat the threaded flag as absent for older plug-ins (they predate const sequence data and may write it in render). | Gate in `caps.rs` decode. |
| PiPL / global-setup agreement | The PiPL's `AE_Effect_Global_OutFlags_2` must match the flags set at global setup; AE reports the correct value when they differ [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/) [SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/). | SHOULD trust global setup and log mismatches (inference). | 19 manifest scan. |

### 10.2 What may run concurrently with what

| Selector group | Thread | May overlap with |
|---|---|---|
| `GLOBAL_SETUP`, `GLOBAL_SETDOWN` | Main only [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). | Nothing of that effect [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). |
| UI selectors (`USER_CHANGED_PARAM`, `UPDATE_PARAMS_UI`, `EVENT`, `DO_DIALOG`, …) | Main [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/) [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | Render-thread selectors below [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). |
| `SEQUENCE_SETUP`, `SEQUENCE_RESETUP`, `SEQUENCE_SETDOWN`, `SMART_PRE_RENDER`, `RENDER`, `SMART_RENDER` | Possibly several threads at once [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). | UI selectors and each other, on the same or different instances, for MFR effects [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/); for others one render thread at a time inside the effect code [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). |
| `QUERY_DYNAMIC_FLAGS` | "Arbitrary times" [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/); thread not documented (guide silent). | Treat as UI-thread (inference). |
| `GPU_DEVICE_SETUP` | Any time, at most once per device, after global setup and before sequence setup [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | See §11. |
| Compute-cache callbacks | On the thread that called the checkout (inference). | Other threads' renders; must not hold cache locks while running (inference). |

### 10.3 Sequence data at render time

* With MFR, sequence data is read-only during render and is obtained on each
  render thread through `PF_EffectSequenceDataSuite1` (`PF_GetConstSequenceData`)
  [SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/); writing it at render time is undefined unless the mutable flag is set
  [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). Plug-ins check for the suite's presence to support both behaviours
  [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/): the host MUST provide the suite, and SHOULD keep `in_data`'s sequence
  handle valid for non-MFR effects.
* Plug-ins must not write sequence data in render or in `UPDATE_PARAMS_UI`
  [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). Host MAY verify in debug builds by checksumming the shared snapshot
  after each call (inference).
* Mutable copies: duplicated per render thread when a render begins, private,
  discarded "after each span of frames" [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/) [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/); heavy per-thread
  recomputation is why Adobe pushes the compute cache [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/).

### 10.4 Thread-safety contract

A thread-safe effect has no statics/globals or only race-free ones, never
writes global data at render time (only in global setup/setdown), and never
writes sequence data at render time or in `UPDATE_PARAMS_UI`; it must release
its own mutexes before calling back into the host, or deadlocks follow [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/).
Host side (all inference from these rules):

* every callback and suite function MUST be re-entrant and callable from any
  render thread concurrently; per-call state (in-data, out-data, params,
  frame data, checkout tables) MUST be per call, never per instance;
* host callbacks MAY block for a long time (nested renders, compute-cache
  waits) — the plug-in is required to tolerate that;
* `PF_ABORT` / `PF_PROGRESS` MUST report per call, so one superseded frame can be
  cancelled without cancelling its siblings.

### 10.5 AEGP calls on render threads

Since 13.5 the render thread works on its own, immutable project copy; project
modification from render is invalid and validation was tightened: undo groups,
stream name/expression changes, creating or disposing streams and disposing
effects from a render thread now return errors [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). `AEGP_RegisterWithAEGP`
only works in global setup [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). Synchronous frame rendering
(`AEGP_RenderAndCheckoutFrame`) is fine on render threads but discouraged on the
UI thread; `AEGP_NewFromDownstreamOfEffect` is UI-thread only [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). Host:
render-thread AEGP reads MUST see the render snapshot; project mutations from
render threads MUST fail with an error, never crash or mutate; render-thread
frame requests are nested renders (8.3).

### 10.6 Compute cache (`AEGP_ComputeCacheSuite1`, `AEGP_HashSuite1`)

A thread-safe cache that effects use instead of, or alongside, sequence data;
"unified with other caches in After Effects" so memory is balanced across them,
and content-keyed so A/B parameter toggling keeps both states cached [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/).
Enabled by default from AE 2022 [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/).

| Function / callback | Documented semantics | Host implementation |
|---|---|---|
| `AEGP_ClassRegister` / `AEGP_ClassUnregister` | Register a cache class by a globally unique id with four callbacks, usually in global setup; unregister purges every value of the class through the delete callback, usually in global setdown. [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/) | Per-process class table; unregister MUST be synchronous and only with no receipts outstanding. |
| `AEGP_ComputeIfNeededAndCheckout(class, options, wait)` | Returns a receipt; computes if absent. If another thread is computing: wait (`true`) or return the "not in cache or compute pending" error (`false`, never shown to the user). [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/) | Single-flight per key: per-entry state (absent → computing → ready) with waiters; MUST run `compute` on the calling thread without holding any global cache lock. |
| `AEGP_CheckoutCached` | Receipt only if already computed, else the same pending error; usable for polling, e.g. a UI thread waiting for a histogram computed by render. [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/) | MUST be callable from the UI thread too. |
| `AEGP_GetReceiptComputeValue` | Opaque pointer to the computed value. [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/) | Value lives in the plug-in's process. |
| `AEGP_CheckinComputeReceipt` | Must be called before returning to the host; invalid or double check-in returns an error and shows an error dialog. [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/) | Receipt pins the entry; MUST NOT evict pinned entries; SHOULD auto-release and log receipts leaked past the selector (inference). |
| `generate_key` | Fast; must hash every input of the value, including layer-input state via `PF_GetCurrentState` (input hash, not pixels, so no render); key is a GUID, unique within the class and ideally globally. [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/) | Calls back into the 04 input-state query (8.2). |
| `compute` | Produces the value from the options. [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/) | May itself call host callbacks (checkouts) → nested renders. |
| `approx_size_value` | Quick, reasonably accurate footprint; feeds the purge heuristic. [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/) | Charge to the unified memory budget. |
| `delete_compute_value` | Frees everything the value owns, on purge or unregister. [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/) | Called by the host on eviction, in the plug-in's process. |
| `AEGP_CreateHashFromPtr` / `AEGP_HashMixInPtr` | Start a hash from a buffer and mix further buffers into it (helper for keys). [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/) | Deterministic within a session; SHOULD be ≥128-bit collision-resistant (inference). |

Not persisted; entries may be purged at any time, so effects must assume they
recompute [SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/). Out of process, values live in the sandbox; the host keeps
keys, sizes and pin counts and asks the sandbox to delete (inference, §20).

### 10.7 Effects without MFR support

AE forces "each render thread to enter and exit the effect code one thread at a
time" and badges the effect in the Effect Controls panel as a performance
warning; other nodes keep rendering in parallel [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/). Before MFR,
`PF_OutFlag2_AE13_5_THREADSAFE` (added in 15.0) existed to declare
UI-thread/render-thread concurrency on different instances [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/), which
suggests that without it UI and render calls were also kept apart (inference).

Host policy:

* MUST serialise all calls into a non-MFR module through one *serialisation
  domain* per module (all its effects and instances, in every comp); SHOULD
  include UI-thread selectors unless `AE13_5_THREADSAFE` is set (inference);
* the domain lock MUST be re-entrant for the owning thread (8.3), and other
  graph work MUST continue on other workers while one waits for the lock;
* out of process, the host MAY instead run several sandbox processes for one
  non-MFR module (a UI process plus N render processes): separate address
  spaces make statics private, giving parallelism without plug-in changes, at
  the cost of N global setups, N compute caches and per-process licence checks
  (inference; see open questions);
* 11/12 SHOULD show the same "not multi-frame safe" badge AE shows [SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/).

### 10.8 BAE mapping and P3 risks

| Need | BAE owner | Reservation |
|---|---|---|
| Per-node concurrency: parallel, parallel with per-worker state, serialised domain shared across nodes and with UI calls | 04, 19 | Domain id + re-entrant domain lock; thread-model field in the plug-in descriptor. |
| Render span begin/end (preview session, export job, state-slot change) and a stable worker index | 04 | Span lifecycle events for per-worker state. |
| Const shared state per snapshot | 02, 04 | Snapshot-pinned slot value delivered with each render. |
| Non-destructive snapshot hook | 19 | `instance_snapshot`. |
| Compute cache under the unified memory budget | 04, 19 | Opaque-entry cache client API; suite in BAE's own ABI. |
| Render-thread suite contract | 19 | Thread-affinity per suite function; mutation errors on render threads. |
| Badge for serialised AE effects | 11/12 | (P3, no reservation). |

Risks: (1) A single non-MFR plug-in in a heavy comp serialises its whole
module and, through nested renders, can stall export; BAE should surface that
in the UI as AE does. (2) Plug-ins that set the threaded flag but are not
actually thread-safe: AE has the same exposure; out-of-process hosting at
least contains crashes (§20). (3) Mutable-copy plug-ins multiply memory by the
worker count. (4) Compute-cache values computed by one thread are read by others
in the same process; the sandbox design must keep one compute cache per
module process for MFR modules, or cross-thread sharing is lost.

### 10.9 Open questions for §7–§10

1. Can BAE's premultiplied RGBA16F path keep colour under zero alpha when a
   SmartFX request sets `preserve_rgb_of_zero_alpha`, or does 04/06 need a
   straight-alpha side path for such inputs?
2. What does a SmartFX effect receive for a layer parameter *other than* param
   0 (masks/effects applied or raw source)? The guide is silent; verify in AE.
3. The guide documents the GUID mix-in only in "What's New"; its exact entry
   point must be confirmed during the P3 clean-room review (§21).
4. Does AE keep UI-thread selectors and render calls of a non-MFR,
   non-`AE13_5_THREADSAFE` effect mutually exclusive? (Our inference; verify.)
5. Is a pool of sandbox processes per non-MFR module acceptable (licence checks,
   dongles, N× memory, N× global setup)?
6. What is a "render span" in BAE's interactive mode (playback session, scrub
   batch, until the next state-slot change)? It bounds the life of mutable
   per-thread sequence-data copies.
7. 15 imports AE projects through ExtendScript, which (inference) cannot read
   effect sequence data; unless the `.aep` reader extracts the saved bytes, a
   third-party effect's instance state is lost on AE → BAE → AE.
8. Undoable sequence data is a deliberate deviation from AE; 02 and 19 should
   confirm it.

## 11. GPU rendering

AE effects can render on the GPU through a SmartFX extension added in AE 16.0.
The GPU path always sits beside a CPU path. This section lists what such a
plug-in expects, what the host has to provide, and how BAE can provide it on
wgpu (Metal on macOS, D3D12 on Windows) without the compatibility layer
costing anything on nodes that do not use it.

**About the sources.** The AE guide documents the GPU *selectors* and *flags*.
It does **not** document the GPU device suite beyond one call, the GPU world
pixel format, buffer layout, or synchronization. Where this section relies on
other public material (the Premiere Pro SDK guide, which shares the GPU device
suite, or a third-party open-source Rust binding of the SDK headers), it says
so and marks the claim **(secondary)**. P3 MUST check every secondary claim
against the shipped SDK headers and sample before relying on it.

### 11.1 Versions, frameworks and samples

| Item | Documented behaviour | Source |
|---|---|---|
| First GPU support | AE 15.0 adopted the GPU effect rendering previously used by Premiere Pro. Effects that support it get a GPU badge in the Effects panel. Unknown effects whose match name contains the `ADBE ` prefix are excluded from GPU rendering. | [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/) |
| AE GPU SDK | Added in 16.0, together with the `SDK_Invert_ProcAmp` sample. | [SDK: Introduction › GPU-Accelerated Effect Build Instructions](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/) |
| CUDA (Windows) | AE 27.0 targets CUDA 12.8 on x64 and CUDA 13.4 on ARM64; AE 25.4 used CUDA 12.8. The guide recommends the CUDA *driver* API, or a statically linked runtime, so the plug-in keeps working with future drivers. | [SDK: Introduction › GPU-Accelerated Effect Build Instructions](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/), [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/) |
| DirectX 12 (Windows) | Opt-in. The effect sets `PF_OutFlag2_SUPPORTS_DIRECTX_RENDERING` in its global flags *and* in its PiPL. Without the flag it falls back to the CPU implementation. A DirectX build ships a `DirectXAssets` folder (compiled shaders) that is loaded at runtime. | [SDK: Introduction › GPU-Accelerated Effect Build Instructions](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/) |
| Metal (macOS) | The guide names no version. The macOS build setup mentions Boost only, with no CUDA. Adobe's end-user help says Mercury GPU Acceleration uses Metal or OpenCL on macOS, and CUDA (NVIDIA) or OpenCL (AMD/Intel) on Windows **(external; verify)**. | [SDK: Introduction › GPU-Accelerated Effect Build Instructions](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/), [Adobe Help: GPU in After Effects](https://helpx.adobe.com/after-effects/using/basics-gpu-after-effects.html) |
| OpenCL | Not documented in the AE guide. Premiere Pro dropped OpenCL in 2021 and is CUDA/Metal only **(secondary)**. | [PPro SDK: GPU Effects & Transitions › CUDA, OpenCL, Metal, or OpenGL?](https://ppro-plugins.docsforadobe.dev/gpu-effects-transitions/cuda-opencl-metal-opengl/) |
| Samples | `SDK_Invert_ProcAmp` is the only AE GPU sample. The guide's sample table does not list it; the build-instructions page does. | [SDK: Introduction › Sample Projects](https://ae-plugins.docsforadobe.dev/intro/sample-projects/), [SDK: Introduction › GPU-Accelerated Effect Build Instructions](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/) |

**What a BAE host can offer.** On macOS BAE can offer **Metal**, because it runs
on the same device and queue as wgpu. On Windows it can offer **DirectX**, for
the same reason. CUDA is the framework most existing Windows GPU effects use.
Offering it from a D3D12 host needs external-memory and semaphore interop, so
it is deferred. OpenCL is not offered. A plug-in that does not accept an
offered framework renders on its CPU path (§11.10), so offering *no* GPU
framework at first is still correct, only slower. **(inference)**

### 11.2 Capability flags

| Flag / field | Plug-in expects / does | Host MUST / SHOULD / MAY | BAE mapping | P3 risk |
|---|---|---|---|---|
| `PF_OutFlag2_SUPPORTS_GPU_RENDER_F32` at `PF_Cmd_GLOBAL_SETUP` | Declares GPU support. The effect then receives GPU selectors and is badged as GPU-capable [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). | MUST treat GPU as possible only when this flag is set **and** SmartFX is supported, because the GPU render selector is the SmartFX render's GPU twin [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | `caps::EffectCaps::gpu_render_f32` → `NodeRequirements::gpu` (04). | None. |
| Same flag at `PF_Cmd_GPU_DEVICE_SETUP` | Re-set in `out_flags2` to accept one specific device and framework. Not setting it means "not for this device" [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST keep a separate accept/decline answer per device, distinct from the global capability. | 19 per-device attach hook (R19). | The guide's wording "one or both of the `..._Fxx` flags" implies other precisions may come later; only F32 exists as of 16.0. Keep the per-device answer extensible. |
| `PF_OutFlag2_SUPPORTS_DIRECTX_RENDERING` | Global flag plus PiPL entry. Without it, DirectX is not used and the CPU path runs [SDK: Introduction › GPU-Accelerated Effect Build Instructions](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/). | MUST NOT offer the DirectX framework to an effect that lacks the flag. SHOULD check that the PiPL and global-setup values agree. | `caps::EffectCaps::gpu_directx`. Today `gpu::offered_frameworks` ignores it (see the crate discrepancies). | The flag is missing from the guide's PF_OutData table, so its semantics come from one paragraph only. |
| `PF_RenderOutputFlag_GPU_RENDER_POSSIBLE` (pre-render output) | Set per request when *this* request can render on the requested GPU. Unset means parameters or render settings prevent it [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST read it after every GPU pre-render and choose the backend per request (§11.4). | `smart::PreRenderOutput::gpu_render_possible`. | The SmartFX page still says the only render-output flag is `RETURNS_EXTRA_PIXELS` [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). That page is stale; the command-selectors page is the authority. |

### 11.3 Device lifecycle: `PF_Cmd_GPU_DEVICE_SETUP` / `PF_Cmd_GPU_DEVICE_SETDOWN`

| Aspect | Plug-in expects | Host obligation | BAE mapping |
|---|---|---|---|
| When | May come "at any time", at most once per GPU device, after `GLOBAL_SETUP` and before `SEQUENCE_SETUP`. Several devices may be set up at the same time [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST send it at most once per (module, device) and before the first sequence setup that could render on that device. SHOULD send it eagerly at module load for the single BAE device, so the first render does not stall. | 19 hook `gpu_device_attach(device, framework)`. The crate's `selector::Tracker` already rejects a duplicate setup. |
| Input | `what_gpu` (the framework) is never "none" here. A device index is supplied for use with the device suite [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST pass the framework the host actually offers and a stable device index. | `gpu::GpuFramework`, `NativeDeviceHandles::device_index`. |
| Purpose | GPU initialization, such as compiling kernels or loading the `DirectXAssets`. The plug-in may opt out **based only on device properties**, not on frame size or other render context [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | SHOULD allow for slow setup (shader compilation) and run it off the UI thread. MUST treat a decline as final for that device for the session. | Run on a 19 worker thread. |
| Output | Accept flags in `out_flags2`, plus `gpu_data`: a plug-in-owned pointer that is handed back at pre-render and render time [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST store `gpu_data` per (module, device), pass it unchanged in every GPU pre-render and render input, and never dereference it. | Opaque per-device token (19). In out-of-process mode it lives only in the helper process. |
| Decline | The plug-in is "called for CPU render" [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST plan CPU for every request of that effect on that device. | 04 per-node backend choice. |
| Setdown | Releases `gpu_data`. Input carries `gpu_data`, framework and device index. In AE it is sent just before the GPU device is released [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST send it before the native device is destroyed (device loss, adapter change, shutdown) and after all GPU renders on that device have *completed on the GPU*, not merely returned. | 01 must report device loss (R01). The crate's tracker requires setdown before `GLOBAL_SETDOWN`. |

### 11.4 Per-request negotiation in `PF_Cmd_SMART_PRE_RENDER`

- **New pre-render input fields (16.0):** `gpu_data`, `what_gpu` and the
  device index
  [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/).
  The host MUST fill them consistently. Use framework "none" for a CPU
  pre-render.
- **Reply:** the plug-in sets `GPU_RENDER_POSSIBLE` when it can render this
  request on that GPU. If it does not, "the host may re-call PreRender with
  another what_gpu option (or PF_GPU_Framework_None)"
  [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/).
  On "not possible", the host MUST discard that pre-render result, call the
  plug-in's pre-render-data delete function, and re-run pre-render with
  framework "none". The CPU plan may check out different inputs and rects.
- **Consistency:** `what_gpu` "will be the same between
  `PF_Cmd_SMART_PRE_RENDER` and `PF_Cmd_SMART_RENDER_GPU`"
  [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/).
  The host MUST render with the backend of the plan that produced the
  `pre_render_data`. It MUST NOT turn a GPU plan into a CPU render or the
  reverse.
- **Pre-render twice:** MediaCore hosts may send pre-render twice, the second
  time with actual source sizes
  [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/).
  BAE SHOULD NOT do this, because the 04 bounds pass gives exact sizes.
  **(inference)**
- **BAE mapping (04):** planning must be able to return a per-request
  `Backend::{Gpu(framework, device), Cpu}` and re-plan on refusal (**R04**). The
  crate's `smart::PreRenderSession::finish` currently returns
  `gpu = requested && possible` without the re-plan (see the crate
  discrepancies).

### 11.5 `PF_Cmd_SMART_RENDER_GPU`

| Aspect | Documented | Host obligation | BAE mapping |
|---|---|---|---|
| Choice of selector | For each render the host sends either `SMART_RENDER` or `SMART_RENDER_GPU`, depending on whether a CPU or a GPU frame is expected as output. `SMART_RENDER_GPU` is only sent when `what_gpu` is not "none" [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST send exactly one render selector per accepted plan, matching that plan's backend. | `selector::Selector::SmartRenderGpu`. |
| Extra | Shares the SmartFX render extra: request, bit depth, `pre_render_data`, plus `gpu_data`, `what_gpu` and device index [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | MUST fill these the same way as in the matching pre-render. SHOULD report float bit depth, because only the F32 GPU flag exists **(inference)**. | `smart::RenderPlan`. |
| Checkouts | "All frame check-ins and check-outs will operate on GPU frames" while this selector runs. Input and output buffers are prepared on the given framework and device [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/). | `checkout_layer_pixels` and `checkout_output` MUST return GPU worlds on the render device. SmartFX checkout rules are unchanged: one-to-one with pre-render checkouts, and output only after at least one input [SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/). | 04 external GPU node (R04). |
| World struct | `PF_LayerDef` is filled exactly as for CPU renders, except that `data` is null. Width, height, rowbytes and the other fields are valid [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/). | MUST give every GPU world a valid rowbytes and size, and MUST map each world struct to its native buffer so the suite's world-data call can return it. | Host-side table: world → native buffer. |
| Native state | Device, context and command queue are queried through the GPU device suite's `GetDeviceInfo` [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), [SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/). | MUST provide the suite (§11.6). | 19 GPU interop suite (R19). |
| Threading | SmartFX render selectors arrive on render threads, and with MFR on several at once (see §10). | MUST allow concurrent GPU renders to encode on the shared queue. Metal command queues and D3D12 queues accept submissions from several threads; wgpu's own queue access is serialized internally **(inference)**. | 01 queue scope (R01). |

### 11.6 GPU device suite (`PF_GPUDeviceSuite1`)

The AE guide shows only how to acquire the suite and call `GetDeviceInfo`
[SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/).
The function list and its semantics below come from the Premiere Pro guide,
which shares the device suite, and from a third-party binding of the
headers. **(secondary)**: [PPro SDK: GPU Effects & Transitions › Suites](https://ppro-plugins.docsforadobe.dev/gpu-effects-transitions/suites/),
[PPro SDK: GPU Effects & Transitions › Getting Started](https://ppro-plugins.docsforadobe.dev/gpu-effects-transitions/getting-started/),
[3P binding: `gpu_device.rs`](https://github.com/virtualritz/after-effects/blob/master/after-effects/src/pf/suites/gpu_device.rs),
[3P binding (Premiere): `gpu_device.rs`](https://github.com/virtualritz/after-effects/blob/master/premiere/src/suites/gpu_device.rs).

| Function (semantic) | Plug-in expects | Host MUST / SHOULD | BAE implementation note |
|---|---|---|---|
| Device count | Number of GPU devices the host supports. | MUST report the devices that have been set up. BAE has one wgpu device, so one. | Single adapter. Multi-GPU is not needed (§11.10). |
| Device info | Per device: the framework, a "meets minimum requirements / compatible" flag, and native handles for device, context, command queue, platform, and (legacy) offscreen OpenGL context/device. | MUST fill device, queue and framework with live handles that stay valid between device setup and setdown. MUST set the compatible flag truthfully. SHOULD leave the OpenGL and platform handles null on Metal/D3D12 **(inference)**. | From 01 via wgpu-hal (R01). Table in §11.7. |
| Acquire / release exclusive device access | "All calls below this point generally require access be held." Full-GPU plug-ins always hold it implicitly. For CUDA, the plug-in pushes and pops the context on its own thread. | MUST implement as a re-entrant no-op or a counter, never as a lock that can block another render thread that is implicitly "holding" the device **(inference)**. | No real lock; log misuse. |
| Allocate / free / purge device memory | All device memory "must" be allocated through the suite. Purge is for emergencies and returns the bytes freed. The returned pointer is native device memory (for example a CUDA allocation). | MUST allocate on the render device and track ownership per module so that a crashed or unloaded module releases everything. Purge MUST evict the host's GPU cache synchronously and report the bytes freed. | 04 cache purge hook (R04). On Metal and D3D12 the returned "pointer" must be the native buffer object. **(inference)** |
| Allocate / free / purge host memory | Pinned host memory for staging. | SHOULD return page-aligned, GPU-visible memory (Metal shared storage, a D3D12 upload/readback heap) **(inference)**. | P3 only. |
| Create / dispose GPU world | Plug-in-created scratch worlds: device index, size, pixel aspect, field, *GPU pixel formats only*, optional clear. A plug-in may dispose only worlds it created. | MUST allocate in the same layout as host worlds (§11.8). MUST reject non-GPU formats. MUST refuse to dispose host-owned worlds. | Same pool as the input/output worlds. |
| Get world data / size / device index | Native buffer of a GPU world (because `data` is null), its total size in bytes, and the device it lives on. | MUST return the native buffer object for the world. Size MUST be rowbytes × height. | World → buffer table. |

### 11.7 Native objects the host must expose, per framework

| Framework | Device | Context | Queue / stream | World data object | BAE availability |
|---|---|---|---|---|---|
| Metal | `MTLDevice` | n/a | `MTLCommandQueue` | `MTLBuffer` **(secondary/inference; verify in sample)** | Same device and queue as wgpu Metal, via wgpu-hal. In-process this is zero-copy. |
| DirectX 12 | `ID3D12Device` | n/a | `ID3D12CommandQueue` (direct) | `ID3D12Resource` buffer **(inference; verify in sample)** | Same device and queue as wgpu D3D12, via wgpu-hal. |
| CUDA | CUDA device | `CUcontext` | stream **(inference)** | device pointer (a CUDA allocation) **(secondary)** | Deferred. Needs D3D12↔CUDA external memory plus external semaphores, with the adapter matched by LUID. The driver API is loaded dynamically, not linked. |
| OpenCL | `cl_device_id` | `cl_context` | `cl_command_queue` | `cl_mem` buffer **(secondary)** | Not offered. |

Handles are process-local. In out-of-process mode the helper cannot use the
host's handles; §11.11 covers this.

### 11.8 GPU world format and layout

- **Format.** The guide documents only the CPU world formats: 8-bit, 16-bit
  and float ARGB through the world suite
  [SDK: Effect Details › Image Buffer Management Functions](https://ae-plugins.docsforadobe.dev/effect-details/image-buffer-management-functions/).
  The headers also define a GPU-only BGRA format with 32-bit float per channel
  (`PF_PixelFormat_GPU_BGRA128`) **(secondary:
  [3P binding: `pixel_format.rs`](https://github.com/virtualritz/after-effects/blob/master/after-effects/src/pf/suites/pixel_format.rs))**.
  It is the only format a plug-in can request when creating a GPU world.
- **Memory kind.** The world is a *linear buffer*, not a texture. The device
  suite deals in device-memory allocations, and Premiere suggests binding a
  texture view onto that linear buffer if texture sampling is needed **(secondary:
  [PPro SDK: GPU Effects & Transitions › Suites](https://ppro-plugins.docsforadobe.dev/gpu-effects-transitions/suites/))**.
  The host MUST therefore hand out buffers. BAE's RGBA16F/RGBA32F textures
  cannot be lent directly.
- **Row pitch.** Plug-ins index using `rowbytes`, and AE warns never to assume
  `width × pixel size`
  [SDK: Effect Basics › PF_EffectWorld / PF_LayerDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_EffectWorld/).
  BAE converts with a compute shader (`shaders::CONVERT_WGSL`, `row_words`
  parameter) rather than with buffer↔texture copies, so wgpu's 256-byte row
  alignment does not apply. The host SHOULD use *tight* rows (width × pixel
  size, which is always a multiple of the pixel size) to protect GPU kernels
  that wrongly assume pitch equals width. **(inference)**
- **Alpha and colour.** Premultiplication and working-space semantics are the
  same as for float CPU worlds (see the pixel-format section); the guide says
  nothing GPU-specific. **(inference)**
- **Conversion cost.** Encode is one compute dispatch (RGBA16F texture → BGRA
  f32 buffer of the checked-out rect) and decode is another, on the same queue,
  with no CPU involvement. This is the only overhead, and only on AE GPU nodes.

### 11.9 Synchronization and queue ownership

The guide is silent on who commits or waits. The rules below are what the host
MUST guarantee so that a plug-in which encodes on the queue it is given works
whether or not it waits on the CPU. **(inference)**

| # | Rule | Why | Owner |
|---|---|---|---|
| S1 | Before the render call, the host MUST *submit* (not necessarily complete) every command buffer that writes the input worlds, on the queue it exposes. | Same-queue submission order then orders the plug-in's work after the host's. | 01/04 |
| S2 | After the call returns, the host MUST submit the output-decode work on the same queue, and MUST NOT read the output on the CPU before a fence that follows the plug-in's submission has signalled. | The plug-in may have committed without waiting. | 04 |
| S3 | The host MUST keep every lent buffer alive, and unreused, until a host fence *after* the plug-in's submission signals. With wgpu this means an (empty) `queue.submit` after the call, so that wgpu's submission index covers the plug-in's work. | wgpu tracks lifetime only for its own submissions. | 01 |
| S4 | **Metal:** lent buffers MUST use tracked hazard mode, or the host MUST bracket the call with an `MTLEvent` / `MTLFence` signal and wait. | Metal orders command buffers on one queue but only inserts dependencies for tracked resources. | 01 |
| S5 | **D3D12:** lent buffers MUST be in the common state at hand-off and are expected back in it. Buffers promote and decay implicitly across `ExecuteCommandLists`, so no explicit barrier is shared with the plug-in. wgpu's state tracker MUST be told, or must assume, the same. | D3D12 has no automatic hazard tracking. | 01 |
| S6 | The host MUST tolerate a plug-in that blocks (`waitUntilCompleted`, a fence wait) inside the call. It MUST NOT hold a lock that the GPU-completion path needs. | Many samples and plug-ins wait synchronously. | 04 |
| S7 | If the plug-in creates its *own* queue, same-queue ordering is lost. The host SHOULD document that it exposes one queue. Errors or garbage seen on such plug-ins should be investigated with a shared-fence fallback (`GpuSync::SharedFence`). | The guide says only that the queue "can be queried". | P3 |

### 11.10 CPU fallback, multi-GPU and failures

- **Fallback always exists.** A plug-in that declines a device is called for
  CPU render
  [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/).
  A DirectX effect without the DirectX flag falls back to its CPU
  implementation
  [SDK: Introduction › GPU-Accelerated Effect Build Instructions](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/).
  The Premiere guide describes GPU extensions as *supplements* to a software
  effect **(secondary:
  [PPro SDK: GPU Effects & Transitions](https://ppro-plugins.docsforadobe.dev/gpu-effects-transitions/gpu-effects-transitions/))**.
  The host MAY therefore ship with no GPU framework offered.
- **Mid-render failure.** Premiere states there is no API to fall back to
  software in the middle of a GPU render. Plug-ins should only opt in when they
  are confident they have the resources **(secondary:
  [PPro SDK: GPU Effects & Transitions › Getting Started](https://ppro-plugins.docsforadobe.dev/gpu-effects-transitions/getting-started/))**.
  If `SMART_RENDER_GPU` returns an error, the BAE host SHOULD re-plan that
  request for CPU (new pre-render with framework "none") rather than fail the
  frame. **(inference)**
- **Multi-GPU.** The protocol supports several devices at once (device index
  everywhere; several devices may be in the setup state
  [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)).
  BAE renders on one wgpu device. The host MUST present exactly that device,
  with a stable index, and MUST NOT expose other adapters it cannot render on.
- **`ADBE ` match names.** AE excludes unknown `ADBE `-prefixed effects from
  GPU rendering
  [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/).
  BAE MAY ignore this. It matters only for fidelity of the GPU badge.

### 11.11 Out-of-process GPU

The architecture prefers running AE plug-ins out of process (ARCHITECTURE §15;
§20 here). Native device and queue handles are process-local, so out-of-process
GPU needs a different transport. **(inference throughout)**

| Platform | Helper side | Sharing primitive | Sync primitive |
|---|---|---|---|
| macOS | Own `MTLDevice` on the same GPU (matched by registry ID) and its own `MTLCommandQueue` | Apple-silicon unified memory: page-aligned shared memory wrapped as a no-copy `MTLBuffer` in both processes. Otherwise an IOSurface-backed allocation. | `MTLSharedEvent` shared through its handle |
| Windows | Own `ID3D12Device` on the same adapter (matched by LUID) and its own queue | Committed buffer in a shared heap, NT handle duplicated into the helper | Shared `ID3D12Fence` (NT handle), with timeline values per render |

Requirements: 01 must create *shareable* allocations and fences and wrap them
as wgpu resources. 19's helper protocol must carry GPU buffer handles and fence
values as a frame transport, not only CPU shared memory (**R01**, **R19**).
Until that exists, an out-of-process AE GPU effect SHOULD run on its CPU path.
This is correct, only slower.

### 11.12 What the first host may omit

| Omission | Justification |
|---|---|
| Offering no GPU framework at all | A CPU fallback is guaranteed (§11.10). |
| CUDA and OpenCL | Not BAE's backends. Plug-ins fall back to CPU, or use DirectX where they support it. |
| GPU in out-of-process mode | §11.11. |
| Real exclusive-access locking, OpenGL interop handles | Secondary-source features with no AE-specific semantics documented. |

**P3 risks (GPU):** (1) The world-data object type and the Metal/D3D12
expectations are undocumented in the AE guide; the `SDK_Invert_ProcAmp` source
must be read. (2) wgpu-hal access APIs change between wgpu versions, so pin the
version. (3) A plug-in that keeps a reference to a lent buffer past the call
would corrupt BAE's pool; worlds must be retired only after S3. (4) Encoding
from many MFR threads onto wgpu's single queue may serialize. (5) A
DirectX-capable plug-in may still require Windows-only `DirectXAssets`
loading paths relative to its binary.

---

## 12. Custom UI, events and Drawbot

AE effects can draw their own controls in the Effect Controls panel and their
own handles over the Composition and Layer viewers. They receive mouse and
keyboard events and draw through Drawbot, a small 2D vector API. BAE's plan:
never hand a native window to a plug-in. Drawbot calls are recorded into a
backend-neutral display list, replayed with egui/Vello, and the same list
crosses the out-of-process boundary.

### 12.1 Opt-in, registration and sizing

| Mechanism | Plug-in does | Host MUST / SHOULD / MAY | BAE mapping |
|---|---|---|---|
| `PF_OutFlag_CUSTOM_UI` | Set at `GLOBAL_SETUP`. The effect then requires `PF_Cmd_EVENT` [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/), [SDK: Effect UI & Events › Effect UI & Events](https://ae-plugins.docsforadobe.dev/effect-ui-events/effect-ui-events/). | MUST send no events to effects without it. | `caps::EffectCaps::custom_ui`. |
| `PF_PUI_TOPIC` / `PF_PUI_CONTROL` on a param | The effect handles events for the param's title row ("topic", visible when twirled up) and/or its control area. Each requires `CUSTOM_UI` [SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/). | MUST reserve a custom-drawn region for the topic and/or control of that param in Effect Controls. | 11/12 custom param region (R11/12). |
| Arbitrary-data params | In AE these must specify `TOPIC`/`CONTROL` or `PF_PUI_NO_ECW_UI` [SDK: Effect Basics › Parameters](https://ae-plugins.docsforadobe.dev/effect-basics/parameters/). | MUST support a custom control with no standard widget. | Same. |
| `ui_width` / `ui_height` | Size of a non-standard param's UI [SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/). Since CC 2014 the height may be changed at runtime with `PF_UpdateParamUI` [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/), [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | MUST honour the height (in logical pixels) and re-layout when it changes. SHOULD give the control the panel's width and report it in the event's frame rect. | Transient UI state, not a Command (R02). `params::*::custom_ui_size`. |
| `PF_PUI_DONT_ERASE_TOPIC` / `..._CONTROL` | The host does not erase the area before the plug-in draws [SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/). | SHOULD draw the plug-in list *over* whatever the host drew (for example the standard control), not over a cleared background. | Replay order in 11/12. |
| `PF_PUI_DISABLED` | The only UI flag that may change while handling events [SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/). | MUST accept it from an event outcome. | Transient UI state (R02). |
| `PF_REGISTER_UI(PF_CustomUIInfo)` | The effect registers which windows receive events. `PF_UIAlignment` flags are not honoured [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). In Premiere Pro, keydown events need the comp event flag set at `PARAM_SETUP` [SDK: Effect UI & Events › PF_EventUnion](https://ae-plugins.docsforadobe.dev/effect-ui-events/PF_EventUnion/). The record also carries per-window flags (comp, layer, effect, preview) and comp/layer/preview UI sizes **(secondary: [3P binding: `custom_ui.rs`](https://github.com/virtualritz/after-effects/blob/master/after-effects/src/pf/suites/custom_ui.rs), [3P binding: `pf/mod.rs`](https://github.com/virtualritz/after-effects/blob/master/after-effects/src/pf/mod.rs))**. | MUST accept the call during `PARAM_SETUP`, store the window set per effect, and route comp and layer viewer events only to registered effects. MAY ignore alignment and the preview window. | 19 UI registration in the effect descriptor (R19). |

### 12.2 Contexts (`PF_Context`)

| Field / concept | Documented | Host obligation |
|---|---|---|
| Context handle | Passed in every event. Used for Drawbot and the UI callbacks [SDK: Effect UI & Events › PF_EventExtra](https://ae-plugins.docsforadobe.dev/effect-ui-events/PF_EventExtra/). | MUST be stable from `NEW_CONTEXT` to `CLOSE_CONTEXT`, with one context per (effect instance × window). |
| `w_type` | Comp, layer or effect window. This is how one plug-in tells its comp UI from its ECW UI [SDK: Effect UI & Events › PF_EventExtra](https://ae-plugins.docsforadobe.dev/effect-ui-events/PF_EventExtra/). | MUST set it truthfully. |
| `plugin_state` | A small fixed array of integer slots where the plug-in keeps per-context state [SDK: Effect UI & Events › PF_EventExtra](https://ae-plugins.docsforadobe.dev/effect-ui-events/PF_EventExtra/). | MUST preserve it across every event of that context. In out-of-process mode it lives in the helper. |
| Drawing reference | The context carries a Drawbot reference. The plug-in obtains it with `PF_GetDrawingReference`, which may return null [SDK: Effect UI & Events › Custom UI & Drawbot](https://ae-plugins.docsforadobe.dev/effect-ui-events/custom-ui-and-drawbot/). | MUST return a valid reference during `DRAW`. SHOULD return null outside `DRAW`, because drawing is only allowed there. |

### 12.3 Event types (`PF_Cmd_EVENT`, `e_type`)

All events and payloads are from [SDK: Effect UI & Events › Effect UI & Events](https://ae-plugins.docsforadobe.dev/effect-ui-events/effect-ui-events/)
and [SDK: Effect UI & Events › PF_EventUnion](https://ae-plugins.docsforadobe.dev/effect-ui-events/PF_EventUnion/).

| Event | Payload | Plug-in expects / does | Host MUST / SHOULD / MAY | BAE source |
|---|---|---|---|---|
| `PF_Event_NEW_CONTEXT` | Context and type only; ignore the rest. | Allocates per-context state. | MUST send when a viewer or ECW section for the instance appears. | egui panel or viewer shows the effect. |
| `PF_Event_ACTIVATE` | None. | Focus gained. | SHOULD send on panel focus. | egui focus. |
| `PF_Event_DO_CLICK` | Click info: OS time, point, click count, modifiers, per-drag refcon slots, `send_drag`, `last_time`. | Handles the click. Since 7.0 it must **not** block until mouse-up. It sets `send_drag` to ask for drags. | MUST deliver the click count (for double-click) and modifiers. MUST honour `send_drag` by capturing the pointer for that plug-in. | Pointer press. |
| `PF_Event_DRAG` | Same click info. | Continues the gesture. `last_time` marks mouse-up. | MUST keep the refcon slots across the gesture. MUST send a final drag with `last_time` set on release (also on capture loss). | Pointer move or release under capture. |
| `PF_Event_DRAW` | Draw info: `update_rect` (context-window coordinates) and `depth` (of the drawing context). Use `PF_InData` image dimensions, not the input layer's. | Draws with Drawbot. **Drawing is allowed only here.** | MUST provide a surface, set `PF_EI_DONT_DRAW` when drawing is not wanted, and fill `PF_InData` dimensions for the viewer's comp/layer. | Display-list record (§12.7). |
| `PF_Event_DEACTIVATE` | None. | Focus lost. | SHOULD send. | egui focus. |
| `PF_Event_CLOSE_CONTEXT` | None. | Frees the context state. | MUST send before the context handle dies, including on instance delete and module unload. | Panel or viewer closes. |
| `PF_Event_IDLE` | None. | Periodic work. | MAY send at a low rate while a context is open. | Timer. |
| `PF_Event_ADJUST_CURSOR` | Point, modifiers, `set_cursor`. | Sets a host cursor, or `PF_Cursor_CUSTOM` after setting an OS cursor itself. `PF_Cursor_NONE` means "don't override". | MUST map named cursors. MUST treat NONE as "host decides". MAY treat CUSTOM as "keep current" (always in out-of-process mode). | Hover. |
| `PF_Event_KEYDOWN` | `when`, point, `key_code` (unshifted upper-case character, or a control code: navigation, function keys F1–F24, editing and modifier keys), modifiers. | May consume. | MUST normalise to that code set. MUST let an unhandled key fall through to BAE shortcuts. | Key event while the overlay or control has focus. |
| `PF_Event_MOUSE_EXITED` | None. Since CS6, layer and comp viewers only. | Clears hover state. | SHOULD send when the pointer leaves the viewer. | Hover end. |

Arbitrary-data callbacks use the same "extra" model but are a different
selector; see the arbitrary-data parameter section.

### 12.4 `PF_EventExtra` fields and outcomes

| Field | Documented semantics | Host obligation | BAE type |
|---|---|---|---|
| `effect_win` / window union | For ECW events: param `index` (0-based), `area` (title or control), `current_frame` (the control's full rect), `param_title_frame`, `horiz_offset` (where to draw into the title). With the newer union there is also item-window info (port rect) [SDK: Effect UI & Events › PF_EventExtra](https://ae-plugins.docsforadobe.dev/effect-ui-events/PF_EventExtra/). | MUST give pane-local rects that match where the region really is, so the plug-in can lay out inside `current_frame`. | `ui::UiEvent` lacks the rects (see the crate discrepancies). |
| `cbs` | Pointer to the UI callbacks (§12.5). | MUST be valid for every event. | Coordinate service. |
| `evt_in_flags` | Only `PF_EI_DONT_DRAW` ("check before drawing"). | SHOULD set it when the overlay is hidden (for example when overlays are toggled off). | — |
| `evt_out_flags` | `HANDLED_EVENT`; `ALWAYS_UPDATE` (re-render the composite on every click/drag, like alt-scrub); `NEVER_UPDATE` (no re-render until the gesture ends); `UPDATE_NOW` (redraw the view immediately after the event, used with `PF_InvalidateRect`) [SDK: Effect UI & Events › PF_EventExtra](https://ae-plugins.docsforadobe.dev/effect-ui-events/PF_EventExtra/). | MUST map these onto the 04 preview scheduler during gestures: render every step, render at the end, or the default policy. | `ui::EventOutcome`. |
| `out_flags`: `PF_OutFlag_REFRESH_UI` | Redraw the entire ECW, comp and layer windows. Valid in `EVENT`, `RENDER` and `DO_DIALOG` [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/). | MUST invalidate all contexts of that instance. Note that it is also legal from *render*, which in BAE is a cross-thread notification. | Render → UI notification (04 → 11/12). |
| `out_flags`: `PF_OutFlag_FORCE_RERENDER` | Works in CLICK/DRAG only if the effect implements `GET_FLATTENED_SEQUENCE_DATA` [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/). | MUST copy the UI copy of sequence data to render and invalidate the cache. | See §9 (state slot). |

### 12.5 Coordinate spaces and UI callbacks

| Callback | Semantics [SDK: Effect UI & Events › UI Callbacks](https://ae-plugins.docsforadobe.dev/effect-ui-events/ui-callbacks/) | Host obligation | BAE data source |
|---|---|---|---|
| `layer_to_comp` / `comp_to_layer` | Map a fixed-point point between layer and comp space *at a given time* (time and scale arguments). | MUST evaluate the layer's full transform (parenting, 3D projected through the active camera) at that time. | bae-anim evaluation (02/04 data, not UI). |
| `get_layer2comp_xform` / `get_comp2layer_xform` | Return a matrix. Layer→comp always exists. Comp→layer may report "does not exist" when the layer scales to zero. | MUST report non-invertibility rather than return garbage. | Same. |
| `source_to_frame` / `frame_to_source` | Map between the context's source space and the pane's "frame"/screen space, which depends on zoom. | MUST include viewer zoom, pan, downsample and pixel aspect. | Viewer `ViewTransform` (R11/12). |
| `PF_GET_PLATFORM_DATA` | Main window handle, executable and resource paths (wide variants since CS6), bundle reference. | MUST return the plug-in's own paths. MAY return null for the main window in out-of-process mode (risk: dialogs without a parent). | 01 platform. |
| `PF_GetMouse` / `PF_ConvertLocalToGlobal` (App suite) | Mouse in custom-UI coordinates, and local → global screen coordinates [SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/). | MUST answer from the last known pointer and the pane's screen origin. | 11/12. |

**Coordinate conventions:** event points are named "screen" coordinates but
are pane-local and converted with the callbacks. Draw rects are in
context-window coordinates.
[SDK: Effect UI & Events › PF_EventUnion](https://ae-plugins.docsforadobe.dev/effect-ui-events/PF_EventUnion/).
For HiDPI the guide tells plug-ins to draw offscreen images at twice the size
and scale them down with the surface `Transform`
[SDK: Effect UI & Events › Custom UI & Drawbot](https://ae-plugins.docsforadobe.dev/effect-ui-events/custom-ui-and-drawbot/).
This implies surface units are *logical* pixels, with the device scale applied
by the host. **(inference)** The CCU sample shows PAR- and downsample-correct
comp UI
[SDK: Introduction › Sample Projects](https://ae-plugins.docsforadobe.dev/intro/sample-projects/).
Plug-ins that skip the callbacks "behave badly" when zoomed or rotated
[SDK: Effect UI & Events › UI Callbacks](https://ae-plugins.docsforadobe.dev/effect-ui-events/ui-callbacks/).

### 12.6 Changing parameters and project state from events; threads

- **Changing values.** A param value may be changed during `DO_CLICK`, `DRAG`
  and `KEYDOWN`. The plug-in edits the param and sets
  `PF_ChangeFlag_CHANGED_VALUE`. The change is undoable and updates the UI
  [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/).
  The `PF_ParamDef` page says `change_flags` is valid "during drag (not
  click!)" events
  [SDK: Effect Basics › PF_ParamDef](https://ae-plugins.docsforadobe.dev/effect-basics/PF_ParamDef/).
  The host MUST accept changes from all three events (the superset).
- **Changes become Commands.** The host MUST turn changed params into 02
  Commands. A click-drag gesture SHOULD be one undo step, with live
  intermediate values (R02). Layer and path params do not support
  `CHANGED_VALUE`
  [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/).
- **Threads.** Since 13.5, `PF_EVENT_DRAW` and the other UI selectors come on
  the UI thread. The render thread has its own copy of the project, and
  passive UI selectors (such as DRAW) are "not a place to make changes to
  project state"
  [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/).
  The host MUST serialise all events of one instance on one UI-dispatch thread,
  and MUST NOT call events concurrently with that instance's UI selectors.
  The color-sampling tip, which adds keyframes "during the draw" through AEGP
  [SDK: Effect UI & Events › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-ui-events/tips-and-tricks/),
  contradicts the 13.5 rule. The host SHOULD reject project writes during
  DRAW.
- **Bit depth.** During events the plug-in cannot know the layer bit depth
  [SDK: Effect UI & Events › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-ui-events/tips-and-tricks/).
  No host action is needed.

### 12.7 Redraw model

| Mechanism | Documented | BAE behaviour |
|---|---|---|
| `PF_InvalidateRect(context, rect or null)` | Queues a lazy redraw of a pane-relative rect (null = whole pane) for the next idle moment. Valid only while handling a *non-drawing* event [SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/), [SDK: Effect UI & Events › Custom UI & Drawbot](https://ae-plugins.docsforadobe.dev/effect-ui-events/custom-ui-and-drawbot/). | Recorded into the event outcome. The host schedules a `DRAW` for that context. |
| `PF_EO_UPDATE_NOW` | Immediate draw of the pane after the event returns. | `DRAW` before the next egui frame. |
| `PF_OutFlag_REFRESH_UI`, `PF_RefreshAllWindows` | Redraw all windows [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/), [SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/). | Invalidate every context. |
| `PF_UpdateParamUI` | Refreshes a param's ECW UI. No need to set `REFRESH_UI` [SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/). | Re-layout plus `DRAW`. |
| Host-initiated `DRAW` | AE decides when to draw. Premiere does not send `DRAW` or `UPDATE_PARAMS_UI` on time changes without keyframes unless `PF_OutFlag_NON_PARAM_VARY` is set [SDK: Premiere Pro & Other Hosts › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/). | Cache the last display list per context and replay it on every egui repaint. Send `DRAW` only on invalidation, time change (if `NON_PARAM_VARY` or keyframed), param change, zoom/pan change or resize **(inference)**. This keeps IPC off the 60 Hz path. |

### 12.8 Drawbot: what the host must implement

Suites per [SDK: Effect UI & Events › Custom UI & Drawbot](https://ae-plugins.docsforadobe.dev/effect-ui-events/custom-ui-and-drawbot/):
`PF_EffectCustomUISuite1`, `DRAWBOT_DrawbotSuite`, `DRAWBOT_SupplierSuite`,
`DRAWBOT_SurfaceSuite`, `DRAWBOT_PathSuite`. Drawbot "pen" and "image" suites
(a dash-pattern setter and an image scale-factor setter) exist in the headers
but not in the guide **(secondary:
[3P binding: `drawbot/mod.rs`](https://github.com/virtualritz/after-effects/blob/master/after-effects/src/drawbot/mod.rs))**.

The recorder runs on the plug-in side, in process or in the helper. It is
**stateful**: it tracks the current transform, clip and policies so that it
can answer queries synchronously, and it appends drawing operations to a
display list. Objects are reference-counted handles. The recorder snapshots a
path's geometry *at draw time*, because the plug-in may keep mutating the path.

| Call | Semantics | Recorder behaviour → display-list op |
|---|---|---|
| `GetSupplier`, `GetSurface` | Get the supplier and surface from the draw reference. | Return handles bound to the current `DRAW`. |
| `NewPen(color, size)`, `NewBrush(color)` | Float RGBA colour; pen width. | Value objects. Pen dash comes from the pen suite (secondary). |
| `NewPath`, `MoveTo`, `LineTo`, `BezierTo` (cubic), `AddRect`, `AddArc`, `Close` | Arc: zero degrees is 3 o'clock, sweep is clockwise, angles in degrees. | Path builder. Arcs are kept as arcs (or flattened to cubics) with the same orientation in BAE's y-down space. |
| `NewDefaultFont(size)`, `GetDefaultFontSize`, `SupportsText` | Text is optional. Clients should check `SupportsText` first. | Answer from the host snapshot. `SupportsText` MAY be false initially. |
| `NewImageFromBuffer(w, h, rowbytes, layout, data)` | Layouts: 24-bit RGB/BGR, 32-bit RGB/BGR (alpha ignored), ARGB/BGRA straight or premultiplied. | Copy and convert to premultiplied RGBA8 *at creation*. Intern by content hash so that unchanged images (for example a histogram) are not shipped again. |
| `Supports/PrefersPixelLayout{BGRA,ARGB}` | Preferred channel order. | Answer from the snapshot. Pick the order cheapest for the converter. |
| `RetainObject`, `ReleaseObject` | Reference counting. Release is required for every `New*`. Never release the supplier or surface. | Refcount table. Leaks are freed when the context closes. |
| `PushStateStack`, `PopStateStack` | Save and restore surface state. Required around clip, transform and policy changes. | `Save` / `Restore` ops. Unbalanced stacks are auto-closed at the end of `DRAW`, not discarded. |
| `Transform(matrix)` | Concatenate onto the current state. | `ConcatTransform`, also applied to the tracked current transform. |
| `Clip(supplier, int rect)`, `GetClipBounds`, `IsWithinClipBounds` | Intersect with an integer rect; queries. | `IntersectClipRect`. The queries are answered from tracked state. |
| `PaintRect(color, rectF)` | Fill a rectangle. | `FillRect`. |
| `FillPath(brush, path, even-odd or winding)`, `StrokePath(pen, path)` | Fill and stroke. | `FillPath` / `StrokePath` with snapshotted geometry. |
| `DrawString(brush, font, UTF-16 text, origin, alignment, truncation, truncation width)` | Alignment left/centre/right. Truncation none / end / end-ellipsis / path-ellipsis. | `Text` op. Layout and truncation are done *at replay* (parley/egui). No measurement call is documented, so nothing has to flow back. |
| `DrawImage(image, origin, alpha)` | Alpha from zero to one. | `Image{id, origin, alpha, scale}`. |
| `Set/GetInterpolationPolicy`, `Set/GetAntiAliasPolicy` | None / medium / high. | State ops. Replay maps them to the sampler and AA mode. |
| `Flush` | Rarely needed; overuse causes flashing. | No-op. |
| Overlay theme suite | Preferred foreground and shadow colours (adapted to UI brightness), stroke width, vertex size, integer shadow offset; plus `PF_StrokePath`, `PF_FillPath` and `PF_FillVertex` helpers that use them [SDK: Effect UI & Events › Custom UI & Drawbot](https://ae-plugins.docsforadobe.dev/effect-ui-events/custom-ui-and-drawbot/). | Getters come from the theme snapshot. The helpers expand into ordinary ops: shadow pass first, then foreground. |

**Colour domain.** Drawbot and theme colours are UI (display-referred)
colours. Replay MUST NOT pass them through the scene view transform. **(inference)**
`PF_AppColorPickerDialog`, by contrast, returns colours in the project's
*working* space
[SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)
(06).

### 12.9 Other UI services plug-ins call

From [SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/).

| Service | Host obligation | BAE |
|---|---|---|
| `PF_AppGetBgColor`, `PF_AppGetColor` (many named UI colours), `PF_GetFontStyleSheet`, `PF_AppGetLanguage` | MUST answer: map named colours to BAE theme tokens, fonts to the UI font, and language to the BAE UI locale (`zh_CN` / `en_US`). | Theme snapshot (11/12). |
| `PF_SetCursor` | NONE = host decides; CUSTOM = plug-in set an OS cursor. The headers name over a hundred cursors **(secondary: [3P binding: `app.rs`](https://github.com/virtualritz/after-effects/blob/master/after-effects/src/pf/suites/app.rs))**. | Named-cursor registry with a fallback to the arrow (R11/12). |
| `PF_InfoDrawText`, `..Text3`, `..Text3Plus`, `PF_AppendInfoText`, `PF_InfoDrawColor` | SHOULD show them in an Info panel. MAY drop them initially. | 11/12 Info panel. |
| `PF_SetOptionsButtonName` (at `PARAM_SETUP` only) | MUST label the effect's Options button. | Effect header in Effect Controls. |
| `PF_IsRenderEngine` | MUST return true in headless `bae render`. | 19. |

### 12.10 Asynchronous frames for UI (histograms, keyers)

Since 13.5, custom UI that needs rendered frames should use the "Async
Manager" (`PF_OutFlag2_CUSTOM_UI_ASYNC_MANAGER`, `PF_GetContextAsyncManager`,
`AEGP_CheckoutOrRender_ItemFrame_AsyncManager` / `..._LayerFrame_...`). Frames
downstream of the effect come from `AEGP_NewFromDownstreamOfEffect`, which is
callable only from the UI thread. The synchronous
`AEGP_RenderAndCheckoutLayerFrame` is reserved for click-triggered actions
that change the project. `PF_PARAM_CHECKOUT` of layers during
`UPDATE_PARAMS_UI` returns black frames
[SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/).
The HistoGrid sample demonstrates this
[SDK: Introduction › Sample Projects](https://ae-plugins.docsforadobe.dev/intro/sample-projects/).
These APIs are AEGP-level, and no third-party host supports AEGP
[SDK: Introduction › Third-party Plug-in Hosts?](https://ae-plugins.docsforadobe.dev/intro/third-party-plug-in-hosts/).
The host MAY omit them initially. Supporting them later needs a 04
non-blocking UI frame-request API (R04). The synchronous variant MUST NOT run
on BAE's UI thread without a progress UI.

### 12.11 BAE implementation sketch

```
plug-in (in process or helper) ── Drawbot / App / UI-callback suites ──► recorder + snapshot
   event in:  UiEvent { ctx, window, area+rects, kind, point(i32, logical), mods, clicks, key, time,
                        drag{refcon slots, last}, dont_draw }  + UiSnapshot { view transform,
                        layer↔comp matrices at t, theme, fonts, clip/scale }
   result out: EventOutcome { handled, send_drag, redraw policy, invalidated rects, cursor,
                        param edits, ui-state edits, out_flags }  + DisplayList (DRAW only)
BAE UI (11/12) ── replays DisplayList with egui painter / Vello into the param region or viewer overlay
```

- Snapshots let almost every callback be answered without a round trip:
  transforms at the current time, theme, fonts, language. Only callbacks at
  another time, and AEGP calls, become RPCs (§20).
- The display list is plain data. It has no native handles, works the same in
  process and out of process, and is also the format BAE-native plug-ins use
  for overlays (R19). Geometry is in logical pane coordinates; replay applies
  the device scale and renders at device resolution, so the HiDPI advice in
  §12.5 works.
- Retained replay plus invalidation-driven `DRAW` means the plug-in costs
  nothing per egui frame. The cost is paid only when its UI changes.

### 12.12 What a first host may omit (with precedent)

| Omission | Precedent / justification |
|---|---|
| Drawbot text (`SupportsText` = false) | The guide makes text supplier-dependent and asks clients to check [SDK: Effect UI & Events › Custom UI & Drawbot](https://ae-plugins.docsforadobe.dev/effect-ui-events/custom-ui-and-drawbot/). Still accept `DrawString` silently, because some plug-ins skip the check. |
| Custom ECW UI on a *standard* param type | Premiere Pro has the same limitation; the workaround is a separate supervised param [SDK: Premiere Pro & Other Hosts › Plug-Ins... Reloaded](https://ae-plugins.docsforadobe.dev/ppro/plug-ins-reloaded/). |
| Async Manager, AEGP use from UI | No third-party host supports AEGP [SDK: Introduction › Third-party Plug-in Hosts?](https://ae-plugins.docsforadobe.dev/intro/third-party-plug-in-hosts/). |
| `PF_Cursor_CUSTOM`, a real main-window handle, `IDLE`, preview-window UI, Info panel output, UI alignment flags | No observable effect on rendering. AE itself ignores the alignment flags [SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/). |
| Layer-viewer UI (if BAE has no layer viewer yet) | Effects register per window. Comp-only still works for CCU-style effects **(inference)**. |

Premiere supports custom ECW and comp UI: CCU and ColorGrid are marked
Premiere-compatible
[SDK: Introduction › Sample Projects](https://ae-plugins.docsforadobe.dev/intro/sample-projects/).
It has supported high-DPI custom UI since CC 2015
[SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/).
So ECW custom controls plus comp overlays are the minimum set worth shipping.

**P3 risks (UI):** (1) Some plug-ins draw with OS APIs (Core Graphics / GDI)
through platform refs instead of Drawbot. They cannot be supported without a
native surface, and SHOULD be detected and shown with a placeholder region.
(2) Plug-ins that block in `DO_CLICK` (pre-7.0 style) would freeze the UI
thread; a watchdog is needed. (3) Event-to-Command mapping must not echo back
as `USER_CHANGED_PARAM` loops. (4) Each out-of-process `DRAW` adds an IPC
round trip (~µs) plus display-list serialization; images must be interned.
(5) Key-code normalisation (unshifted upper-case) differs from egui's key
model, and IME text must never reach plug-ins as `KEYDOWN`.

## 13. Function suites

Almost every host service an AE plug-in uses comes from a *function suite*: a table of
function pointers identified by a name and a version, obtained through the PICA basic suite
([SDK: Effect Details › Effect Details](https://ae-plugins.docsforadobe.dev/effect-details/effect-details/), [SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/)). This section defines what the BAE compat host must provide. It covers
the acquisition contract (13.1), where each suite can run when the plug-in is out of process
(13.2), the catalog (13.3, 13.4), the BAE counterparts that 19 should provide (13.5), the
P3 risks with a check of `suites.rs` (13.6), and the reservation tags used in §13–§15 with
their §18 ids (13.7).

Legend for the catalog tables:

- **Hot.** "yes" means the call is made per pixel or per scanline, or carries most of the CPU
  time of a render. Such calls must never become a cross-process round trip.
- **Place.** Where the function runs when the plug-in is hosted in a sandbox helper process
  (§20):
  - **L** (local): implemented inside the sandbox with no host state.
  - **S** (snapshot): data the host pushes with each call and the sandbox answers locally.
  - **R** (RPC): a synchronous callback to the BAE process.
  - **U** (UI message): fire-and-forget or batched to BAE's UI thread.
  - **G** (GPU interop): needs the GPU device that BAE renders with.
- **Thread.** "any" means any thread. "render" means a render worker. "UI" means BAE's main
  thread, which corresponds to AE's UI thread.

### 13.1 Acquisition, release and versioning

| Aspect | What AE and the plug-in do | Host obligation | BAE mapping | P3 risk |
|---|---|---|---|---|
| Entry to all suites | `PF_InData` carries a pointer to the PICA basic suite (`SPBasicSuite`). Its `AcquireSuite` member hands out every other suite. C++ plug-ins wrap it in `AEFX_SuiteScoper` and C plug-ins in `PF_Suite_Helper`, and both release the suite when done ([SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/), [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). AEGPs get the same basic suite in their entry point ([SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/)). The code-registration entry point also receives it ([SDK: Introduction › Exporting Symbols in Effects](https://ae-plugins.docsforadobe.dev/intro/symbol-export/)). The Skeleton sample, the template for most effects, also uses the `AEGP_SuiteHandler` utility code ([SDK: Introduction › How To Start Creating Plug-ins](https://ae-plugins.docsforadobe.dev/intro/how-to-start-creating-plug-ins/)). | MUST pass a valid basic suite on every call: every selector, every AEGP hook and the registration call. | 19: a suite registry inside the native C ABI (§14 of `ARCHITECTURE.md`). | Sample-derived helpers acquire suites lazily, so a missing suite can surface deep inside a render rather than at setup (inference). |
| Call context (`effect_ref`) | Most PF suite functions and the `inter` callbacks take the opaque `effect_ref` from `PF_InData` as their first argument ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/), [SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/)). AEGPs may call any effect-suite function that does not need it ([SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/)). It can be NULL in some arbitrary-data callbacks, where plug-ins are told not to check out params ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). Several functions are valid in one context only: the Drawbot reference during drawing, `PF_InvalidateRect` during non-drawing events, `PF_SetOptionsButtonName` during param setup ([SDK: Effect UI & Events › Custom UI & Drawbot](https://ae-plugins.docsforadobe.dev/effect-ui-events/custom-ui-and-drawbot/), [SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | MUST make `effect_ref` a per-call token that identifies (instance, call, selector, thread lane). Every function that takes it MUST validate it and return an error, never crash, for a NULL, stale or wrong-context token. A call without a token (from an AEGP) MUST be treated as "no instance". | R19-call-context (new): every BAE suite function takes an explicit per-call context handle. | A thread-local "current call" would break when a plug-in calls back from its own worker threads, and when a plug-in keeps a token beyond the call (inference). |
| Versions | A newer version supersedes an older one, but a plug-in may acquire several versions of the same suite at once. Adobe never removes or changes a suite it has shipped ([SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/)). Some suites document that a newer version keeps all earlier functions unchanged, for example `AEGP_StreamSuite7` ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_streamsuite7)). | MUST expose each version it claims as its own immutable table with the signatures of that version. MUST allow several versions to be held at the same time. MUST NOT hand out a different version than the one requested (`suites.rs::negotiate`). | 19 uses the same rule for BAE suites. | A function's signature can change between versions (the guide's two-argument vs three-argument example, [SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/)), so the P3 shim needs one thin adapter per version. |
| Missing suite | `AEFX_SuiteScoper` either throws a missing-suite error or returns a null table, depending on a template flag. Plug-ins then fall back to an older version, warn the user, or fail ([SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/)). AEGPs are told to fail gracefully and to identify themselves ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | MUST return the missing-suite error and a null table, with no side effects and no dialog. SHOULD log each unmet (suite, version) request so P3 can prioritise. | 19: a typed `SuiteError::Missing`. | Plug-ins built with the throwing variant and no try/catch crash on a missing suite. Out of process this kills only the sandbox (§20). |
| Host identification by probing | Plug-ins tell hosts apart by the application id in `PF_InData` and by version ([SDK: Introduction › Third-party Plug-in Hosts?](https://ae-plugins.docsforadobe.dev/intro/third-party-plug-in-hosts/), [SDK: Premiere Pro › Premiere Pro & Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/ppro/)). They also probe suites. Adobe suggests that an AEGP check for `AEGP_CanvasSuite` to detect Premiere Pro, which offers no AEGP suites ([SDK: Premiere Pro › Unsupported Features](https://ae-plugins.docsforadobe.dev/ppro/unsupported-features/), [SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)). MFR effects are told to check for `PF_EffectSequenceDataSuite1` to choose between the old (writable) and new (read-only at render) sequence-data behaviour ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). The Portable sample shows host and version checks as the way to handle missing suites ([SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)). | MUST treat the set of exposed suites as part of the compatibility contract. MUST NOT expose a suite version unless every function in it is implemented or returns a defined error. SHOULD choose deliberately which "AE-only" suites to expose (open question). | 20 policy. | A stub suite sends the plug-in down AE-only code paths that BAE may not support. Omitting it sends the plug-in down Premiere-style fallbacks. |
| Legacy callback tables | Besides suites, `PF_InData` carries the `inter` (interaction) and `utils` (graphics and math) tables. Older plug-ins and Premiere-portable plug-ins call them through macros such as `PF_ADD_PARAM`, `PF_CHECKOUT_PARAM`, `PF_ABORT`, `PF_PROGRESS`, `PF_COPY` and `PF_FILL` ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/), [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/), [SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)). | MUST fill both tables on every call, sharing code with the equivalent suites. | Compat-internal. | Easy to forget, because these are not suites. |
| Errors | Plug-ins must return every error they get from callbacks and suites, and AE has already reported it. A cancel error must propagate, or misrendered frames may be cached ([SDK: Effect Basics › Errors](https://ae-plugins.docsforadobe.dev/effect-basics/errors/)). | Suite functions MUST return errors, never abort the process. A user cancel MUST surface as the interrupt error. | 04 cancellation (R04-13), status categories (R19-02). | — |
| Exceptions and panics | AE's APIs assume C and crash if an exception escapes a plug-in ([SDK: Introduction › Exceptions](https://ae-plugins.docsforadobe.dev/intro/exceptions/)). On Apple Silicon an exception crossing a C frame calls `terminate()` ([SDK: Introduction › Apple Silicon Support](https://ae-plugins.docsforadobe.dev/intro/apple-silicon-support/)). | Every Rust callback MUST catch panics and convert them to errors before returning into plug-in code. The host MUST NOT unwind through plug-in frames. | 19 C ABI rule. | One missed `catch_unwind` takes down the sandbox. |
| Thread safety | Unless a suite says otherwise, a plug-in must assume its functions are not thread-safe, and only the main thread may touch UI ([SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/)). For AEGPs, `SPBasicSuite` itself is not thread-safe ([SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/)). Multi-frame rendering (MFR) effects nevertheless acquire suites on render threads, for example `PF_EffectSequenceDataSuite1` inside Render ([SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)) and `AEGP_HashSuite1` inside the compute-cache key callback ([SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/)). Plug-ins must not hold their own locks while calling into the host, or they deadlock ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | MUST make acquire and release thread-safe and cheap: a lock-free read of a static table. MUST classify every function as any, render or UI, and return an error, not deadlock, on a wrong-thread call. AE has done this since 13.5 for project-mutating AEGP calls on the render thread ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | R19-02, R19-call-context. | AE serialises much more internally than BAE will. Wrong-thread calls that "worked" in AE will now fail visibly. |
| Release | Every acquire is balanced by a release (scoper or helper) ([SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/)). | MAY treat release as a reference-count decrement on a static table. MUST tolerate releases at module unload. | 19. | — |
| API version | Effects read the effect API version from `PF_InData`. AEGPs get major and minor versions in their entry point. Both gate behaviour on these, and some plug-ins also run `AEGP_ExecuteScript` to read `app.version` ([SDK: Introduction › Compatibility Across Multiple Versions?](https://ae-plugins.docsforadobe.dev/intro/compatibility-across-multiple-versions/)). | MUST report version numbers no newer than the newest suites and behaviours it really implements. | 20 configuration. | Reporting too new a version turns on code paths BAE lacks. Reporting too old a version disables MFR and GPU. |
| Plug-in-published suites | AEGPs may publish their own suites for effects to use. Because load order varies, they cannot depend on non-AE suites in their entry point. An AEGP may also request a suite and supply its own replacement when the suite is absent ([SDK: AEGPs › Overview](https://ae-plugins.docsforadobe.dev/aegps/overview/), [SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/)). The Sweetie sample publishes a suite for other plug-ins, and Adobe suggests the pattern for sharing an image library across several plug-ins ([SDK: Introduction › Sample Projects](https://ae-plugins.docsforadobe.dev/intro/sample-projects/)). | MUST let a module register suites, and MUST make them available only after every module's entry point has run. A consumer in another module MUST reach them, which constrains sandbox grouping (§15.8, §20). | R19-module-suites (new): registry provenance (host or module id) and helper groups. | The per-module sandbox design breaks vendor AEGP+effect pairs. Without AEGP hosting, such effects see the vendor suite as missing. |

### 13.2 Placement rules for out-of-process hosting

1. **Hot-path suites run locally, always.** This covers iterate, sampling, batch sampling,
   world transform, fill/matte, ANSI, colour callbacks, pixel data, handle and world
   allocation. The sandbox links its own implementation, so an RPC never runs per pixel or per
   row (the invariant tested in `suites.rs` `catalog_is_unique_and_hot_paths_are_local`).
2. **Values the host knows before the call are snapshots.** These include param values, float
   colour/point/angle values, UI theme colours, language, render-engine mode, time display
   preferences and the camera/light at the current time. The host sends them with the call so
   the sandbox answers locally.
3. **Queries at other times or on other objects are RPCs.** These include param or layer
   checkout at another time, AEGP stream, layer and comp reads, sub-renders, path checkout and
   input-state fingerprints. Each one is a synchronous callback (`process.rs::CallbackKind`),
   and the host MUST service nested calls: a checkout can trigger a render of another instance
   of the same module while the first call waits (§8.3, §20).
4. **UI side effects are messages.** Cursor, invalidate-rect, info-palette text and Drawbot
   draw lists are batched per event.
5. **Abort and progress go through shared memory.** `PF_ABORT` and `PF_PROGRESS` are called
   about once per scanline ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). They MUST read an atomic abort flag and write a progress
   counter in the shared slot header, not send messages (§20; `process.rs` check in §15.8).
6. **Context-bound services follow the context.** A Drawbot reference, an invalidate request
   or an options-button rename is legal only inside its event or selector (13.1). The sandbox
   MUST reject such calls locally, without a round trip, once the call that owns the token has
   returned (inference).

### 13.3 Catalog: effect-side suites and callback tables

| Suite / table | Purpose | Host MUST / SHOULD | Typical users | Hot | Place | Thread | BAE counterpart (19 unless noted) |
|---|---|---|---|---|---|---|---|
| `inter` callbacks (`PF_ADD_PARAM`, `PF_ABORT`, `PF_PROGRESS`, `PF_CHECKOUT_PARAM`/`PF_CHECKIN_PARAM`, `PF_REGISTER_UI`, layer-audio checkout) | Declare params. Poll for abort and report progress. Read param values or source frames at other times ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). | MUST follow checkout semantics. A non-SmartFX checkout returns the raw (un-effected) layer, a `<none>` layer comes back as zeros, masks are excluded, a footage time maps to the nearest frame to its left, fields are deinterlaced, and frames exist from source start to source end whatever the trim, transparent outside. MUST be safe when a checkout re-enters a render of another instance ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). Plug-ins are told not to check out layers during UI events ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)). In `PF_Cmd_UPDATE_PARAMS_UI`, AE (since 13.5) returns a black frame of the right size instead of rendering, to avoid deadlocks; the host MUST do the same and SHOULD return an error for layer checkouts in other UI events ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Every effect. | ABORT/PROGRESS: yes | ADD_PARAM L (buffered); ABORT/PROGRESS L (atomic); CHECKOUT R | any (checkout render/UI) | `bae.params.value_at`, `bae.render.layer` (04), cancellation token (04). |
| `utils` graphics callbacks (`PF_COPY`, `PF_FILL`, `PF_BLEND`, convolve, `PF_GET_PIXEL_DATA8`…) | Legacy graphics and math callbacks. The pointer is always defined ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). Premiere offers them where it lacks the newer suites ([SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)). | MUST behave exactly like the suite versions, from the same code. | Older and Premiere-portable effects. | yes | L | any | none (compat-internal CPU kernels). |
| `PF_HandleSuite1` | New, lock, unlock, dispose, size and resize of relocatable handles. Handles the host passes in are locked before the call and unlocked after it ([SDK: Effect Details › Memory Allocation](https://ae-plugins.docsforadobe.dev/effect-details/memory-allocation/)). AE asks plug-ins to allocate large blocks this way so it can free caches under memory pressure ([SDK: Effect Details › Memory Allocation](https://ae-plugins.docsforadobe.dev/effect-details/memory-allocation/)). | MUST track the bytes it hands out against BAE's memory budget, MUST return out-of-memory rather than abort, and MUST lock sequence and global data handles around each call. | All (sequence and global data). | no | L | any | `bae.memory` (01 allocator, budget feed to the 04 cache). |
| `PF_WorldSuite2` | Create and dispose worlds of a given pixel format, and query the format (8-bit, 16-bit or float ARGB) ([SDK: Effect Details › Image Buffer Management Functions](https://ae-plugins.docsforadobe.dev/effect-details/image-buffer-management-functions/)). | MUST support all three formats. MAY back worlds with shared-memory slots. MUST keep row padding legal. | Multi-pass effects. | no | L | any | `bae.image` (CPU buffers from the 04 pool). |
| `PF_Iterate8Suite1`, `PF_Iterate16Suite1`, `PF_IterateFloatSuite1` | Parallel per-pixel callbacks with optional area and origin, an 8-bit LUT variant, a non-clipped source, and a generic once-per-CPU form. The host handles progress and abort, the pixel order is unspecified, and only thread index 0 may report progress in the generic form ([SDK: Effect Details › Iteration Suites](https://ae-plugins.docsforadobe.dev/effect-details/iteration-suites/)). Since the October 2021 SDK, AE runs up to one thread per core ([SDK: Effect Details › Iteration Suites](https://ae-plugins.docsforadobe.dev/effect-details/iteration-suites/)). | MUST fan out across worker threads *from inside a render worker* without deadlock or oversubscription. MUST check abort between chunks. MUST give a stable thread index in the generic form. | Most CPU effects. | **yes** | L | render | `bae.parallel` (R19-09). |
| `PF_WorldTransformSuite1` | `composite_rect` with AE transfer modes, `blend`, `convolve`, `copy`/`copy_hq`, `transfer_rect` with mask, and `transform_world` with a matrix array for motion blur. Several kernel flags are documented as unimplemented ([SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/)). | MUST match AE's transfer-mode, copy and convolve maths for fidelity, and MUST ignore the flags AE ignores. | Compositing, blur and transform effects. | yes (heavy) | L | render | Shares the blend-mode maths with 04 CPU fallbacks (inference). |
| `PF_FillMatteSuite2` | Fill at each depth, premultiply or unpremultiply, and premultiply with a colour ([SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/)). | MUST. | Many effects. | medium | L | any | none. |
| `PF_Sampling8Suite1`, `PF_Sampling16Suite1`, `PF_SamplingFloatSuite1`, `PF_BatchSamplingSuite1` | Nearest-neighbour, sub-pixel and area sampling. Outside the image is zero alpha. Low quality falls back to nearest neighbour. The sample point (0,0) hits the pixel centre ([SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/), [SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). Plug-ins copy the function pointer into their inner loop ([SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/)). | MUST match AE's filtering and quality switch, including the documented maximum area for area sampling, and MUST return stable pointers that stay valid for the call ([SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/)). | Distortion effects. | **yes** | L only | render | none. |
| `PF_ANSICallbackSuite1` | libm wrappers plus `sprintf`/`strcpy` ([SDK: Effect Details › Graphics Utility Suites](https://ae-plugins.docsforadobe.dev/effect-details/graphics-utility-suites/)). | MUST, mapped to the platform libm. | Legacy effects. | possibly | L | any | none. |
| `PF_ColorCallbacksSuite` | RGB↔HLS and RGB↔YIQ in fixed point, plus luminance, hue, lightness and saturation ([SDK: Effect Details › Color Space Conversion](https://ae-plugins.docsforadobe.dev/effect-details/color-space-conversion/)). | MUST reproduce the documented fixed-point ranges. | Colour effects. | yes | L | any | none (06 is not involved). |
| `PF_PixelDataSuite1` | Typed pixel pointer for a world ([SDK: Premiere Pro › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/)). | MUST. | Portable effects. | no | L | any | none. |
| Premiere-side PF Pixel Format and Utility suites (Premiere SDK headers) | Register additional pixel formats, and ask for checked-out frames in the render format ([SDK: Premiere Pro › Bigger Differences](https://ae-plugins.docsforadobe.dev/ppro/bigger-differences/)). | MAY omit. Exposing them implies Premiere pixel formats. | Effects that also target Premiere Pro. | — | — | — | none. |
| `PF_ParamUtilSuite3` | `PF_UpdateParamUI` (always pass a copy, never param 0). Input-state receipts (`PF_GetCurrentState`/`PF_AreStatesIdentical`) over a time span or all time, including layers and honouring the exclude flag. `PF_IsIdenticalCheckout`. Keyframe find, count, checkout and time ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). | MUST compute receipts from all inputs over the span. MUST treat a queried span as a temporal dependency of the frame being rendered ([SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)). MUST honour UI-only rules for `PF_UpdateParamUI` ([SDK: Effect Details › Parameter Supervision](https://ae-plugins.docsforadobe.dev/effect-details/parameter-supervision/)). In `PF_Cmd_UPDATE_PARAMS_UI` only, `PF_GetCurrentState` returns a random GUID in AE since 13.5; the host SHOULD copy this, because computing a real receipt there can block the UI thread ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). The receipt is a hash of the inputs, not of rendered pixels, so it never triggers a render ([SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/)). | Simulation and caching effects, supervised params. | no | state queries R; UpdateParamUI U | render / UI | `bae.params` + R04-10, R02-12. |
| `PF_ColorParamSuite1`, `PF_PointParamSuite1`, `PF_AngleParamSuite1` | Full-precision (HDR) float values of colour, point and angle params ([SDK: Effect Details › Parameters & Floating Point Values](https://ae-plugins.docsforadobe.dev/effect-details/parameters-floating-point-values/)). | MUST deliver BAE's native float value and not re-derive it from the legacy fixed-point fields. | Float-aware effects. | no | S | any | 02 param model (already float). |
| `PF_PathQuerySuite1`, `PF_PathDataSuite` | Enumerate the source layer's masks. Check a path out and in at a time. Read vertices with tangents in layer space, open/closed, segment length (prepare, evaluate, derivative, cleanup), inversion, mask mode and name ([SDK: Effect Details › Working With Paths](https://ae-plugins.docsforadobe.dev/effect-details/working-with-paths/)). | MUST convert BAE paths into AE vertex form and MUST balance checkouts. | Stroke and path effects. | medium | S (snapshot at checkout) | render / UI | 02/05 mask and path data. |
| `PF_EffectSequenceDataSuite1` | Read-only sequence data on render threads for MFR effects ([SDK: Effect Details › Global, Sequence, & Frame Data](https://ae-plugins.docsforadobe.dev/effect-details/global-sequence-frame-data/)). Writing at render needs the "slower" flag, which gives each render thread its own copy ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | MUST return the render-snapshot copy. MUST duplicate it per thread when the flag is set. | All MFR effects with sequence data. | once per render | L | render | 02 opaque state slot, 19 instance state. |
| `AEGP_ComputeCacheSuite1` + `AEGP_HashSuite1` | A thread-safe compute cache. Classes are registered and unregistered. Compute-if-needed comes in waiting and non-waiting forms. Receipts are checked out and in. The effect supplies key, compute, size and delete callbacks, and the hash suite builds keys ([SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/)). Enabled by default in AE 2022 and later ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | MUST compute each key once across threads. MUST return the pending error in non-waiting mode. MUST purge through the delete callback under memory pressure, and on class unregister. MUST report a double or invalid check-in as an error, not crash. AE also shows a dialog for it; BAE SHOULD log instead, because the call comes from a render thread ([SDK: Effect Details › Compute Cache API](https://ae-plugins.docsforadobe.dev/effect-details/compute-cache-api/)). | MFR-era analysis effects. | per render | L (cache); keys often need R (`PF_GetCurrentState`) | render | `bae.cache` (04 budget). |
| `PF_GPUDeviceSuite1` | Device information (device, context, command queue) for the framework and device index chosen at GPU device setup ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), [SDK: Effect Details › Accessing the After Effects Function Suites](https://ae-plugins.docsforadobe.dev/effect-details/accessing-function-suites/)). The guide documents only `GetDeviceInfo`; the remaining functions (GPU world data, device memory) are known only from headers and samples (inference, §11.6). | MUST hand out the same native device and queue that BAE renders with, so frames are not copied. | GPU effects. | per render | G | render | 01/19 native handles (wgpu-hal). Out of process this needs cross-process textures (risk). |
| `PF_AppSuite` | Background colour, UI element colours, UI language, user registration info, UI font style sheet, cursor, render-engine or no-UI mode, colour-picker dialog (returns a working-space colour), mouse position, invalidate-rect and local-to-global coordinates ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | MUST snapshot colours, language and render-engine mode. Cursor and invalidate MUST be UI messages. MUST decide what `PF_GetPersonalInfo` returns (privacy §21.8, licensing §15.6). | Custom-UI and licensing code. | no | S + U | UI (render-engine query: any) | `bae.host`, `bae.ui` (11/12). |
| `PF_AdvAppSuite2` | Mark the project dirty, save it, change window stacking, refresh all windows, and write text or colours to the info palette ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | SHOULD map dirty and save to 02 commands and the info palette to an 11 status area. MAY ignore window stacking. | Trackers and bridges to external apps. | no | R / U | UI | 02 commands, 11. |
| `PF_AdvTimeSuite4` | Format time the way AE displays it, read time-display preferences, count frames ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | MUST format from flicks consistently with BAE's own time display. | UI text. | no | S | any | 03/11 time formatting. |
| `PF_AdvItemSuite1` | Step the current time, touch the active item, force a re-render, ask whether the effect is active ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | SHOULD map force-rerender to invalidating the node in the 04 cache, and time steps to transport commands. | Trackers. | no | R | UI | 04 invalidation, 11 transport. |
| `PF_ChannelSuite1` | A layer's auxiliary channels (depth, normals, motion vectors, object id…), with checkout and check-in ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | MAY report zero channels. Premiere omits this suite ([SDK: Premiere Pro › Unsupported Features](https://ae-plugins.docsforadobe.dev/ppro/unsupported-features/)). | 3D-aware effects. | no | R | render | 08 (EXR AOVs, future). |
| `PF_EffectUISuite` | Rename the options button. Only valid during param setup ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | MUST store the label in the effect-type schema. | Effects with dialogs. | no | S | UI | 02 schema, 12 effect-controls panel (ECW). |
| `PF_EffectCustomUISuite1` | Get the Drawbot drawing reference for the current context ([SDK: Effect UI & Events › Custom UI & Drawbot](https://ae-plugins.docsforadobe.dev/effect-ui-events/custom-ui-and-drawbot/)). Since 13.5 an async manager renders frames for UI drawing ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | MUST return a reference only during draw events. | Custom-UI effects. | no | U | UI | 12 custom-draw region. |
| `PF_EffectCustomUIOverlayThemeSuite1` | Preferred foreground and shadow colours, stroke width, vertex size and shadow offset, plus themed stroke, fill and vertex drawing ([SDK: Effect UI & Events › Custom UI & Drawbot](https://ae-plugins.docsforadobe.dev/effect-ui-events/custom-ui-and-drawbot/)). | MUST follow BAE's overlay theme, including HiDPI. | Viewer handles. | no | S + U | UI | 12 overlay theme. |
| Drawbot (`DRAWBOT_DrawbotSuite`, `…SupplierSuite`, `…SurfaceSuite`, `…PathSuite`) | Pens, brushes, fonts, images (ARGB or BGRA), paths, clipping, transforms, text, anti-alias policy and flush. Drawing is only allowed in the draw event ([SDK: Effect UI & Events › Custom UI & Drawbot](https://ae-plugins.docsforadobe.dev/effect-ui-events/custom-ui-and-drawbot/)). | MUST implement the calls as a recorded display list that BAE's UI renderer replays. Out of process the list MUST go over as one batch per event. MUST state text support through `SupportsText`. | Custom UI. | per event | U | UI | 11/12 vector overlay API. |
| UI callbacks (`PF_EventCallbacks`, `PF_GET_PLATFORM_DATA`) | Layer↔comp and source↔frame transforms. Platform data: main window, executable path, resource path (which may be an external localised file) and bundle reference ([SDK: Effect UI & Events › UI Callbacks](https://ae-plugins.docsforadobe.dev/effect-ui-events/ui-callbacks/)). | MUST provide transforms that respect zoom and rotation. Platform paths are local to the sandbox. A main-window handle across processes is a risk (§14.4, §20). The Windows module handle (`PF_PlatData_RES_DLLINSTANCE`) MUST be the plug-in's own. | Custom UI, dialogs. | no | S | UI | 12 viewport transforms, R01-01. |
| PF Cache On Load suite (Premiere SDK header) | `PF_SetNoCacheOnLoad` during global setup turns off capability caching ([SDK: Premiere Pro › Plug-Ins... Reloaded](https://ae-plugins.docsforadobe.dev/ppro/plug-ins-reloaded/)). | SHOULD honour it in BAE's scan cache (§15.3). A second argument tells the host whether to list the effect after a failed load ([SDK: Premiere Pro › Plug-Ins... Reloaded](https://ae-plugins.docsforadobe.dev/ppro/plug-ins-reloaded/)). | Effects with dynamic capabilities. | no | L | UI | 19 scan cache (R19-03 no-cache opt-out). |
| SmartFX callbacks (`checkout_layer`, `checkout_layer_pixels`, `GuidMixInPtr`) | ROI negotiation, and mixing extra state into the cache GUID ([SDK: SmartFX › SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/), [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | Specified in the SmartFX section. Listed here for completeness. | SmartFX effects. | per render | R | render | 04 ROI. |

### 13.4 Catalog: AEGP suites that effects "cheat" with

Effects may call AEGP suites. Adobe explicitly allows the camera/light suites,
`AEGP_GetLayerParentComp` and `AEGP_GetCompBGColor`, and says this does not license *any* AEGP
call ([SDK: AEGPs › Cheating Effect Usage of AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/cheating-effect-usage-of-aegp-suites/)). The entry point is `AEGP_PFInterfaceSuite1`, and `AEGP_RegisterWithAEGP`
issues the plug-in id ([SDK: AEGPs › Cheating Effect Usage of AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/cheating-effect-usage-of-aegp-suites/)). Rendering from AEGP query results without storing them,
cancelling and forcing a re-render causes caching bugs, because AE does not know about the
dependency ([SDK: AEGPs › Cheating Effect Usage of AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/cheating-effect-usage-of-aegp-suites/), [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#working-with-effects)). BAE SHOULD do better here: it can record
every RPC read made during a render as a dependency of that frame (inference, R04-10).

| Suite | Effect-side use | Host MUST / SHOULD | Hot | Place | Thread | BAE counterpart |
|---|---|---|---|---|---|---|
| `AEGP_PFInterfaceSuite1` | Get the effect's layer and effect ref, convert layer time to comp time, get the camera, and get the camera matrix. The matrix is row-major and its inverse is the model view. The plug-in corrects for a non-square comp PAR ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_pfinterfacesuite1), [SDK: Effect Details › Accessing Camera & Light Information](https://ae-plugins.docsforadobe.dev/effect-details/accessing-camera-light-information/)). | MUST. MUST give camera data for the frame's render snapshot. | per render | S (current time) / R | render, UI | `bae.stage` (R04-15, R04-18). |
| `AEGP_UtilitySuite6` | `AEGP_RegisterWithAEGP` (only during global setup, or AE may crash since 13.5), report-info dialogs, quiet errors, undo groups (UI thread only), main HWND, script execution, host-activated check, plug-in paths, thread-safe idle trigger, debug log ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_utilitysuite6), [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | MUST issue plug-in ids. Undo MUST map to 02 transactions. Report-info MUST be non-modal or suppressed in headless renders. Script execution: §14.7. | no | R / U | mostly UI | R02-08, R19-script, `bae.host`. |
| `AEGP_LayerSuite9` | Layer flags, times, stretch, transfer mode, parent, layer-to-world transform, 2D/3D, track matte, object type (including 3D model and parametric mesh) ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_layersuite9), [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). A layer's ID never changes for the life of the project. A layer param's stream value carries a layer ID, which `AEGP_GetLayerFromLayerID` resolves; this is the documented way for an effect to reach the pixels and properties of a layer-param target ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_layersuite9), [SDK: AEGPs › AEGP Details](https://ae-plugins.docsforadobe.dev/aegps/aegp-details/)). | MUST serve a read-only subset to effects. Layer IDs MUST persist across save, load and sync, because plug-ins store them in sequence data (inference). | no | R | any (read) | 02 + 15 semantics, R02-compact-ids. |
| `AEGP_CompSuite13` | Background colour and parent comp (explicitly allowed), frame rate, shutter angle and phase, downsample, work area, dimensions ([SDK: AEGPs › Cheating Effect Usage of AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/cheating-effect-usage-of-aegp-suites/), [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_compsuite13)). | MUST serve the read subset. Creation functions are AEGP-only (§14). | no | S / R | any (read) | 02. |
| `AEGP_StreamSuite7`, `AEGP_DynamicStreamSuite4` | Stream values at a time (with expressions), `AEGP_SetStreamValue` (needed for layer and path params because the changed-value flag stopped re-rendering them in 14.0, [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)), names, expressions, a session-unique stream id. `AEGP_SetStreamValue` is legal only on streams that do not vary over time. The dynamic `HIDDEN` flag shows or hides params ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_streamsuite7), [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_dynamicstreamsuite4)). Version 7 gives a layer param a second, independent value: the *render stage* at which the target layer is sampled (source pixels, masks only, masks and all effects, or through effect *k*). Layer and stage can be set together as one undo step, and a query returns the highest stage that does not create a render cycle given the current project ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_streamsuite7), [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | MUST map streams to BAE property paths through AE match names (15). Writes MUST go through 02 commands on the UI thread, and stream-name, expression and dispose calls MUST fail on render threads ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). The layer-param stage MUST be persisted with the layer reference and honoured by layer checkout (R02-layer-stage). | no | R | read any, write UI | `bae.stage`, `bae.commands`. |
| `AEGP_KeyframeSuite3` | Read, insert and delete keyframes. Batch adds use the start/add/end "cookie" protocol ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_keyframesuite3), [SDK: AEGPs › AEGP Details](https://ae-plugins.docsforadobe.dev/aegps/aegp-details/)). A colour-sampling custom UI adds keyframes from a draw event ([SDK: Effect UI & Events › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-ui-events/tips-and-tricks/)). | MUST map to 03 animation through commands. A cookie MUST map to one transaction. | no | R | UI | 03, R02-08. |
| `AEGP_EffectSuite4` | Effect refs, installed-effect keys, match names (7-bit ASCII), flags, apply/delete/reorder, `AEGP_EffectCallGeneric` (AEGP→effect message), effect masks ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_effectsuite4)). | MUST support ref and stream navigation and match names. Generic calls MUST be routed to the right instance, possibly in another sandbox. | no | R | UI | 19 effect registry, 02. |
| `AEGP_CameraSuite2`, `AEGP_LightSuite2` | Camera at a time, camera type, film size, default distance; light type. Units and orientation conventions are documented ([SDK: Artisans › Artisan Data Types](https://ae-plugins.docsforadobe.dev/artisans/artisan-data-types/), [SDK: AEGPs › Cheating Effect Usage of AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/cheating-effect-usage-of-aegp-suites/)). | MUST serve reads in AE units (film size in pixels). | no | S / R | any (read) | 02 camera/light Prims. |
| `AEGP_MaskSuite6`, `AEGP_MaskOutlineSuite3` | Enumerate masks, read and write outline vertices, feather ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/), [SDK: Effect Details › Working With Paths](https://ae-plugins.docsforadobe.dev/effect-details/working-with-paths/)). | SHOULD serve reads; writes go through commands. | no | R | read any | 02/05. |
| `AEGP_RenderSuite4`, `AEGP_LayerRenderOptionsSuite1`, `AEGP_RenderOptionsSuite4` | Render a comp or layer frame with options (time, step, depth, downsample, ROI, matte mode, channel order). Upstream or downstream of an effect, with receipts, rendered region and change timestamps. Rendering another layer with its effects from inside a render can loop forever if that layer's effect does the same ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_rendersuite4)). Synchronous renders on the UI thread are deprecated since 13.5 except for a user click whose result updates the project (an "auto" button); render-thread use is fine. `AEGP_NewFromDownstreamOfEffect` is UI-thread only ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | MUST turn a cycle into an error, not a hang. MUST offer the sync form on the UI thread (with progress and cancel) and on render threads. Render workers MUST be able to finish without the UI thread, so a UI-thread caller can block on them (R04-ui-blocking-wait). | no | R | UI and render | R04-10, R04-19, R02-12. |
| Async frame manager (`PF_GetContextAsyncManager`, `AEGP_CheckoutOrRender_ItemFrame_AsyncManager`, `AEGP_CheckoutOrRender_LayerFrame_AsyncManager`) | Since 13.5, custom UI that needs rendered frames (histograms, keyers) asks an async manager, which handles several requests at once and cooperates with other UI. Enabled with `PF_OutFlag2_CUSTOM_UI_ASYNC_MANAGER`. The Histogrid sample shows it ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/), [SDK: Introduction › Sample Projects](https://ae-plugins.docsforadobe.dev/intro/sample-projects/)). The guide names the functions only in What's New and does not say which suites hold them. | MUST never block the UI thread: answer from completed frames, report "not ready" otherwise, and redraw the custom UI when the frame arrives. MUST cancel requests when the context closes (inference from the stated purpose; details in §12.10). | no | R + U | UI | R04-19 (non-blocking UI frame requests), §12.10. |
| `AEGP_WorldSuite3` | AEGP worlds, and conversion from `PF_EffectWorld` ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_worldsuite3)). | MUST. | no | L | any | `bae.image`. |
| `AEGP_MemorySuite1` | Memory handles, 16-byte aligned, with leak reporting ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_memorysuite1)). | MUST. | no | L | any | `bae.memory`. |
| `AEGP_ColorSettingsSuite5` | Blending tables, whether a view has a colour transform, a pixel transform from working to view space on an AEGP world, working-space ICC profile and profile conversions, setting a comp's working space, OCIO mode, config, working space, display and view ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_colorsettingssuite5)). | SHOULD synthesise an ICC profile from 06's working-space definition. SHOULD answer the OCIO queries as "not OCIO", because the OpenColorIO runtime is banned in BAE (`AGENTS.md`). The working-to-view transform is a CPU pixel operation and MUST run locally from a 06 transform snapshot. Setting the working space MUST be a 02 command on the UI thread, or refused. | no (view xform: per image) | S; view xform L | any; set: UI | 06 (R06-01). |
| `AEGP_CompositeSuite2` | Transfer-mode blend of two worlds with optional mask (in the working colour space when no blend tables are given), track-matte preparation, alpha clearing outside a rect, low- and high-quality copies ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_compositesuite2)). | MUST share the blend and copy kernels with `PF_WorldTransformSuite1`. | yes | L | any | shared CPU kernels. |
| `AEGP_ItemSuite9`, `AEGP_FootageSuite5` (read) | Item names, sizes, PAR, durations, footage paths ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | SHOULD serve reads. | no | R | any (read) | 02, 08. |
| `AEGP_IterateSuite1` | Thread count and generic parallel iteration ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_iteratesuite1)). | MUST. | yes | L | any | `bae.parallel`. |
| `AEGP_PersistentDataSuite4` | Preference blobs keyed by (section, key), with several blob kinds (machine-specific, machine-independent and others). The match name is the recommended section. A missing key is written with the supplied default and returned. The data lives in AE's preferences, never in the project, and the prefs folder path is queryable ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#persistent-data-suite)). | SHOULD keep one prefs store per plug-in, outside projects and outside sync. MUST reproduce the write-default-on-miss rule. | no | L | UI | 19 plug-in prefs. |
| AEGP Math suite | Matrix helpers, new in 15.0. The guide gives no function table ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | MUST if exposed (pure functions). | maybe | L | any | none. |
| `AEGP_CanvasSuite8` | Artisans only, but used as a probe to tell AE from Premiere ([SDK: Artisans › Artisan Data Types](https://ae-plugins.docsforadobe.dev/artisans/artisan-data-types/), [SDK: Premiere Pro › Unsupported Features](https://ae-plugins.docsforadobe.dev/ppro/unsupported-features/)). | MUST NOT expose unless artisans are supported (open question). | — | — | — | none. |

### 13.5 BAE suite counterparts (proposal for 19)

The compat host implements AE suites as adapters over BAE's own suites (`ARCHITECTURE.md`
§14). The names below are proposals.

Every function in every BAE suite takes the per-call context handle of 13.1
(R19-call-context), returns the status categories of R19-02 and declares its thread class.

| BAE suite (proposal) | Backs | Contract needed by the compat host | §18 id |
|---|---|---|---|
| `bae.memory` | `PF_HandleSuite1`, `AEGP_MemorySuite1` | Budgeted allocation that 04 sees under memory pressure, attributed to a module, with a distinct out-of-memory status. | R04-13 |
| `bae.image` | `PF_WorldSuite2`, `AEGP_WorldSuite3` | CPU buffers from the 04 pool with explicit row stride, optionally in shared memory. | R04-02, R04-06 |
| `bae.parallel` | iterate suites, `AEGP_IterateSuite1` | Re-entrant `parallel_for` on the render pool, with thread count and per-call thread index. | R19-09 |
| `bae.params` | `inter` checkout, `PF_ParamUtilSuite3`, float param suites | Value at time *t* from the render snapshot, keyframe lists, input-state fingerprints. | R04-10, R02-12 |
| `bae.stage` | PF interface, layer, comp, stream, camera, light, mask and item reads | Snapshot-consistent read API at any time, recordable as dependencies, with persistent compact ids. | R04-15, R04-18, R02-compact-ids |
| `bae.commands` | stream/keyframe writes, undo groups, project dirty/save | Commands plus named transactions spanning calls, force-closed by the host. | R02-08 (extension) |
| `bae.render` | layer checkout, AEGP render suites, async frame manager | Synchronous and asynchronous sub-render at an effect stage (including layer-param stages), with cycle detection; UI-thread blocking wait is safe. | R04-10, R04-19, R04-ui-blocking-wait |
| `bae.cache` | compute cache | Single-flight keyed cache in the 04 budget. | R04-16 |
| `bae.gpu` | `PF_GPUDeviceSuite1` | Native device, queue and buffer handles. | R19-04, R01-01 |
| `bae.ui` / `bae.overlay` | app suite UI parts, Drawbot, overlay theme, UI callbacks | Display-list drawing, cursor, invalidation, theme, viewport transforms (11/12). | R19-05, R11-02, R12-02 |
| `bae.host` | app suite info, utility suite info | Host id and version, language, headless flag, plug-in paths, logging and messages. | R11-01 |
| `bae.script` | `AEGP_ExecuteScript` | Synchronous evaluation of a script string on the UI thread, returning a UTF-8 result or error. | R19-script (new, keep possible) |
| registry | plug-in-published suites | Modules can register suites; consumers in other modules acquire them. | R19-02, R19-module-suites (new) |

### 13.6 P3 risks and check of `suites.rs`

- **Breadth.** The guide names about 80 distinct suite families, about 45 of them AEGP
  suites. Implementing only T0 and T1 still means roughly 25 suite versions with hundreds of
  functions (estimate). A "missing suite" log (13.1) is the
  only reliable way to prioritise.
- **Fidelity of local kernels.** Transfer modes, sampling and convolution must match AE
  closely, or composites drift. Golden-image tests against AE output are needed, generated by
  scripts, not copied from Adobe.
- **Probing changes behaviour.** Exposing a suite is a signal as well as a service (13.1).
  Exposing `PF_EffectSequenceDataSuite1` switches MFR plug-ins to read-only sequence data at
  render time. Exposing `AEGP_CanvasSuite` tells AEGPs they run in AE. Each exposed suite
  version is therefore a product decision, not only an implementation task.
- **Context errors.** Plug-ins that "worked" in AE because AE serialised everything may now
  call context-bound functions from the wrong thread or after the call returned. Strict
  validation (13.1) turns latent bugs into visible errors. The host SHOULD offer a per-module
  "lenient" mode that logs instead of failing, where that is safe (inference).
- **`suites.rs`** (checked against the guide):
  - `SuiteKey::PixelFormat` (T0, "pixel format queries and conversions") conflates the
    Premiere-only PF Pixel Format Suite ([SDK: Premiere Pro › Bigger Differences](https://ae-plugins.docsforadobe.dev/ppro/bigger-differences/)) with
    AE's format query, which is `PF_GetPixelFormat` in `PF_WorldSuite2` ([SDK: Effect Details › Image Buffer Management Functions](https://ae-plugins.docsforadobe.dev/effect-details/image-buffer-management-functions/)).
  - The `App` comment lists "progress, abort" and "info palette". Progress and abort are the
    `inter` callbacks `PF_PROGRESS`/`PF_ABORT` ([SDK: Effect Details › Interaction Callback Functions](https://ae-plugins.docsforadobe.dev/effect-details/interaction-callback-functions/)), and the info palette is in
    `PF_AdvAppSuite2` ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)).
  - The `EffectCustomUi` comment says "custom UI registration". Registration is
    `PF_REGISTER_UI` in the `inter` table. The suite only returns the drawing reference
    ([SDK: Effect UI & Events › Custom UI & Drawbot](https://ae-plugins.docsforadobe.dev/effect-ui-events/custom-ui-and-drawbot/)).
  - `AegpRegister` mentions panels. The guide documents no panel registration (14.4).
  - Missing keys: the legacy `inter` and `utils` tables, `PF_PixelDataSuite1`,
    `PF_AdvTimeSuite4`, `PF_AdvItemSuite1`, `PF_ChannelSuite1`, `PF_EffectUISuite`,
    `PF_BatchSamplingSuite1` (folded into `Sampling` is fine), `AEGP_HashSuite1`,
    `AEGP_EffectSuite4`, `AEGP_MaskSuite6`/`AEGP_MaskOutlineSuite3`, `AEGP_WorldSuite3`,
    `AEGP_CompositeSuite2`, `AEGP_ColorSettingsSuite5`, `AEGP_IterateSuite1`,
    `AEGP_PersistentDataSuite4`, the item/footage read subset, the async frame manager, the
    AEGP Math suite, the PF Cache On Load suite, and the `AEGP_CanvasSuite` probe decision.
  - `AegpProject` (T3) lumps project, item and footage writes with reads that effects use
    (item IDs, footage paths), which belong in T1/T2.
  - It has no placement (L/S/R/U/G) and no thread-class dimension. `needs: Ui` for `App` is
    wrong for `PF_IsRenderEngine` and the colour and language queries, which are snapshots
    callable from any thread.
  - `ComputeCache` says `needs: None`. It needs the 04 cache budget (R04-16), and keys often
    need `PF_GetCurrentState` (StageRead).
  - There is no call-context model (13.1) and no provenance for plug-in-published suites.
  - `negotiate` matches the guide: exact versions only, a missing version is a clean error.

### 13.7 Reservation tags used in §13–§15

Tags with a §18 id refer to an existing request. Tags marked *new* are requests raised by this
section and listed in the structured result. Tags marked *future* are not reservations: they
matter only if AEGPs are ever hosted (P4) and need nothing from the core now.

| Tag | Status | Meaning |
|---|---|---|
| R19-02 | §18 | Versioned suites, status categories, per-function thread class, no unwinding. |
| R19-call-context | new | Every BAE suite function takes an explicit per-call context handle (instance, call, selector, lane), never thread-local state. |
| R19-module-suites | new | Modules can publish suites; registry records provenance; helper processes can host a group of modules. |
| R19-script | new (keep possible) | A native plug-in can evaluate a script synchronously on the UI thread. |
| R19-03, R19-04, R19-05, R19-08, R19-09 | §18 | Registry and scan cache, GPU interop, UI hooks and display list, trust registry, parallel-for. |
| R02-08 (extension) | §18 + new detail | Plug-in-opened transaction with a user-visible name, nesting folded into the outermost group, and a host-forced close. |
| R02-compact-ids | new | Persistent compact integer aliases for layers and items. |
| R02-layer-stage | new | Layer references carry a render stage. |
| R02-12, R04-10, R04-13, R04-15, R04-16, R04-18, R04-19 | §18 | Change stamps, time dependencies, memory, snapshot pinning, foreign cache, recorded reads, async UI frames. |
| R04-ui-blocking-wait | new | Renders complete without the UI thread, so the UI thread may block on one. |
| R01-01, R01-05 | §18 | Native GPU handles; per-architecture helper executables with their own signing. |
| R06-01, R11-01, R11-02, R12-02 | §18 | Working-space descriptor, message centre, cursors/theme/info sink, viewer overlay. |
| P4-main-hooks | future | Command, menu-update, idle (with any-thread wake) and death hooks for AEGPs. |
| P4-foreign-view | future | A dockable panel that hosts a foreign native child view. |

## 14. AEGP general plug-ins

### 14.1 What an AEGP is

AEGPs (After Effects General Plug-ins) look to the user like part of AE.
- They add, intercept and trigger menu commands, read and edit the keyframe database, add and
  remove items, layers, effects and keyframes, change interpretations, replace files, and
  publish suites ([SDK: AEGPs › AEGPs](https://ae-plugins.docsforadobe.dev/aegps/aegps/)).
- Keyframers, Artisans (3D renderers) and AEIOs (file import/export) are specialised AEGPs
  ([SDK: AEGPs › AEGPs](https://ae-plugins.docsforadobe.dev/aegps/aegps/)).
- All communication after the entry point goes through hook functions the AEGP registers
  ([SDK: AEGPs › Overview](https://ae-plugins.docsforadobe.dev/aegps/overview/)).

### 14.2 Entry point and lifecycle

| Aspect | AE behaviour | Host obligation | BAE mapping | P3 risk |
|---|---|---|---|---|
| Entry point | Named in the PiPL, which is still required ([SDK: AEGPs › Overview](https://ae-plugins.docsforadobe.dev/aegps/overview/)). Called once at launch with the basic suite, API major and minor versions, a plug-in id and a global refcon ([SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/)). Every later call goes to a registered hook, each with its own refcon. AEGPs may use any effect-suite function that does not need an `effect_ref` ([SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/)). | MUST call the entry point once per session on the UI thread. MUST issue a unique plug-in id. MUST pass each hook its own refcon back unchanged. | 19 plug-in load hook. | Launch-time cost of loading every AEGP. BAE SHOULD load AEGPs lazily and only on opt-in. |
| Registration | Hooks MUST be registered through `AEGP_RegisterSuite5`, which may only be used in the entry point ([SDK: AEGPs › Overview](https://ae-plugins.docsforadobe.dev/aegps/overview/), [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | MUST reject later registration with an error. | 19 registries (P4-main-hooks). | — |
| Load order | Not specified. An AEGP must not acquire non-AE suites in its entry point ([SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/)). | MAY load in any order. MUST make plug-in-published suites available before the first hook call. | 19. | — |
| Lifetime | AEGPs are never unloaded during a session ([SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/)). | MUST NOT unload them. The death hook runs at quit ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). When a second, command-line AE instance starts, every AEGP handle is duplicated, and Adobe warns this may cause problems ([SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/)). | 19 hot-unload does not apply to AE modules (R19-03 "pinned" modules). | Conflicts with 19's "load and unload at runtime" goal. A sandbox restart or a second helper is the BAE analogue of a second AE instance: the AEGP re-initialises and loses in-memory state (inference). |
| Version checks | Uses the AEGP API version, or `app.version` through script execution ([SDK: Introduction › Compatibility Across Multiple Versions?](https://ae-plugins.docsforadobe.dev/intro/compatibility-across-multiple-versions/)). | MUST report a consistent version (13.1). | 20 configuration. | — |
| Other hosts | Premiere Pro loads AEGPs but supports none of their calls. AEGPs are told to detect this by probing `AEGP_CanvasSuite` and to return an error ([SDK: Premiere Pro › Unsupported Features](https://ae-plugins.docsforadobe.dev/ppro/unsupported-features/)). No third-party host supports AEGPs ([SDK: Introduction › Third-party Plug-in Hosts?](https://ae-plugins.docsforadobe.dev/intro/third-party-plug-in-hosts/)). | If BAE does not host AEGPs, it MUST NOT call their entry points at all. | 20 policy. | Partial AEGP support invites undefined behaviour. |

### 14.3 Registration hooks and menu commands

| Hook / function | AE behaviour | Host obligation | BAE mapping |
|---|---|---|---|
| `AEGP_RegisterCommandHook` | One handler for all of the plug-in's commands. The register suite can also replace some of AE's own commands; a before/after-AE priority exists for that case but AE currently ignores it ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_registersuite5)). | MUST deliver the command id. Replacing built-in AE commands MUST NOT be supported (BAE commands differ). | 19 command registry. Commands appear in menus, shortcuts and MCP tools (`ARCHITECTURE.md` §14). |
| `AEGP_RegisterUpdateMenuHook` | Called whenever any menu is about to be drawn. Without it the plug-in's items stay disabled ([SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/), [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | MUST call it before showing menus or the command palette that contain AE-plug-in items, and keep it cheap (UI thread). | P4-main-hooks (enabled/checked-state callback), 11 menus. |
| `AEGP_RegisterDeathHook` | Called at quit ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | MUST call it before shutting down the sandbox. | 19 shutdown hook. |
| `AEGP_RegisterIdleHook`, `AEGP_CauseIdleRoutinesToBeCalled` | Main-thread calls at irregular intervals. The trigger is the only thread-safe AEGP call ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/), [SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/)). | MUST run idle hooks on the UI thread, throttled. The trigger MUST be callable from any thread. | P4-main-hooks (idle and any-thread wake). |
| Version, About-string and About hooks | Not currently called ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | MAY ignore. | — |
| Artisan, interactive Artisan and IO registration | Specialised streams of messages ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | Out of scope (14.9). | — |
| `AEGP_RegisterPresetLocalizationString` | Localises property names in presets ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | MAY ignore (no `.ffx` support). | — |
| `AEGP_CommandSuite1` | Get a unique command id. Insert into named AE menus (file, edit, composition, layer, effect, window, keyframe assistant, import, export, preferences…), sorted or at top/bottom; top and bottom are refused for the Window menu. A "-" name inserts a separator. Remove, rename, enable, disable and check items. AE occasionally sends a reserved null command, which handlers must ignore. `AEGP_DoCommand` runs any AE command, but AE does not guarantee command ids stay the same across versions; developers find ids with the scripting call `app.findMenuCommandId` ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_commandsuite1)). | MUST map AE menu locations to BAE menus. MUST allocate ids at runtime and never hand out the null id. `AEGP_DoCommand` on AE-internal ids MAY support a small mapped set (play/stop, purge caches) and MUST return an error otherwise. | 19 command registry, 11 menus. |
| `AEGP_EffectCallGeneric` | AEGP→effect message channel ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_effectsuite4)). | MUST route the message to the effect instance, across sandboxes if needed. | 19 instance messaging. |

### 14.4 Panels and native UI

- **Panels.** The guide documents no panel suite API (no `AEGP_PanelSuite` page or function
  table). The only reference is the Panelator sample, which creates a dockable panel; the
  suite it uses is known only from the SDK headers (inference). Adobe says building a panel this way is far more work
  than using the HTML5 panel SDK and recommends that SDK instead ([SDK: Introduction › Sample Projects](https://ae-plugins.docsforadobe.dev/intro/sample-projects/), [SDK: Introduction › Other Integration Possibilities](https://ae-plugins.docsforadobe.dev/intro/other-integration-possibilities/)).
  - A native AEGP panel implies a platform child view (HWND or NSView) docked inside BAE's
    window. BAE would need a "foreign native view" panel type (P4-foreign-view) and native
    window handles (R01-01).
  - Cross-process child views are not portable. On macOS an NSView cannot be parented across
    processes (inference), so panels would need in-process AEGPs.
  - HTML5 (CEP) panels are out of scope: `AGENTS.md` bans Chromium and WebView.
- **Dialogs.** On Windows, AEGPs and effects open modal dialogs parented to
  `AEGP_GetMainHWND`. If they don't, their dialog does not block the windows behind it
  ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_utilitysuite6)). Effects get the same window through `PF_PlatData_MAIN_WND`, and their
  module handle through `PF_PlatData_RES_DLLINSTANCE` ([SDK: Effect UI & Events › UI Callbacks](https://ae-plugins.docsforadobe.dev/effect-ui-events/ui-callbacks/)). The host MUST provide a
  parent handle and the plug-in's own module handle. Out of process the parent window belongs
  to another process; the helper needs an owner-window token and a modal session with BAE
  (R19-06, R11-01).
- **Floating palettes.** `AEGP_ShowHideAllFloaters` toggles all floating palettes ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_utilitysuite6)). The host
  MAY ignore it.

### 14.5 Project model access

Handles are opaque and short-lived:
- Anything that changes the *number* of items invalidates references, and adding a keyframe
  invalidates stream references ([SDK: AEGPs › Data Types](https://ae-plugins.docsforadobe.dev/aegps/data-types/)).
- Several handle types must be disposed ([SDK: AEGPs › Data Types](https://ae-plugins.docsforadobe.dev/aegps/data-types/)).
- Changing the render queue invalidates render-queue item references ([SDK: AEGPs › AEGP Details](https://ae-plugins.docsforadobe.dev/aegps/aegp-details/)).
- AE supports a single open project ([SDK: AEGPs › Data Types](https://ae-plugins.docsforadobe.dev/aegps/data-types/)).
- Plug-ins are told not to cache references between hook calls and not to cache layer pixels
  ([SDK: AEGPs › Data Types](https://ae-plugins.docsforadobe.dev/aegps/data-types/)).

Some *ids*, unlike handles, are durable:
- An item ID persists across saves and loads of the project, and a layer ID never changes for
  the life of the project ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_itemsuite9)).
- A stream's unique ID is only session-unique ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_streamsuite7)).
- Mask IDs identify masks uniquely ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_effectsuite4)).

The host MUST implement handles as (stable id, generation) pairs that are validated on every
use, returning an error and never crashing on a stale handle. Item and layer IDs MUST be
persisted compact integers, because AE's are integer-sized and plug-ins store them; BAE's own
Prim identities are wider (R02-compact-ids).

| Area | AEGP suites | Semantics the host must reproduce | BAE mapping | Difficulty |
|---|---|---|---|---|
| Project | `AEGP_ProjSuite6` | Single project, root folder, bit depth, time display. Opening or creating a project does not save the current one ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_projsuite6)). | 02 Stage, 11 file commands. | medium |
| Items and selection | `AEGP_ItemSuite9`, `AEGP_CollectionSuite2` | Item types, IDs, folders, selection sets, comments, labels ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | 02, 11 selection. | medium |
| Compositions | `AEGP_CompSuite13` | Creating solids, cameras, lights, text, null, vector and parametric-mesh layers. Frame rate, work area, motion-blur settings ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_compsuite13), [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | 02 + 15 (AE semantics). | high |
| Footage | `AEGP_FootageSuite5` | Interpretation (alpha, field order, PAR, frame rate, proxies) ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | 08, 02. | high |
| Layers and effects | `AEGP_LayerSuite9`, `AEGP_EffectSuite4` | Layer flags, time mapping, parenting, track mattes, effect apply and reorder ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | 02 + 15. | high |
| Streams and keyframes | `AEGP_StreamSuite7`, `AEGP_DynamicStreamSuite4`, `AEGP_KeyframeSuite3`, `AEGP_MarkerSuite2` | "Just about everything is a stream". Stream groups are addressed by match name, and streams can be added, deleted and reordered in dynamic groups (shape, paint, text). Keyframe ease, interpolation and tangents. Expressions ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | 02/03 property model through the 15 match-name map. | very high |
| Masks and text | `AEGP_MaskSuite6`, `AEGP_MaskOutlineSuite3`, `AEGP_TextDocumentSuite1`, `AEGP_TextLayerSuite1` | Mask modes and feather, outline vertices, text document and outlines ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | 05, 02. | high |
| Rendering | `AEGP_RenderSuite4`, `AEGP_RenderOptionsSuite4`, `AEGP_LayerRenderOptionsSuite1`, `AEGP_WorldSuite3`, `AEGP_CompositeSuite2`, `AEGP_SoundDataSuite1` | Frame receipts (check in promptly, because AE bases caching on what is checked out), rendered-region queries, project timestamps and "has this item changed since", frame GUIDs, compositing and transfer modes, audio. An external (speculative) renderer can ask whether a frame is worth rendering and hand finished frames to AE's cache ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_rendersuite4)). | 04 (R04-14 regions, R02-12 stamps), 08. Cache adoption of foreign frames: not planned. | high |
| Render queue | `AEGP_RenderQueueSuite1`, `AEGP_RQItemSuite4`, `AEGP_OutputModuleSuite4`, `AEGP_RenderQueueMonitorSuite1` | Queue items, output modules, render monitoring ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/)). | BAE export pipeline (08). | very high, low value |
| Colour, prefs, import, guides | `AEGP_ColorSettingsSuite5`, `AEGP_PersistentDataSuite4`, `AEGP_FIMSuite3`, `AEGP_GuideSuite2`, `AEGP_ItemViewSuite2` | Colour settings, preference blobs, import "flavours" that create compositions, guides and per-view guide options ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/), [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | 06, 19 prefs, 15 importer hooks, 12. | medium |

### 14.6 Undo, errors and utilities

- **Undo groups.** `AEGP_StartUndoGroup` and `AEGP_EndUndoGroup` bracket one undoable step, and
  the name appears in the Edit menu. Many functions also take an "undoable" switch
  ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_utilitysuite6), [SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_dynamicstreamsuite4)).
  - The host MUST map a group to one 02 transaction, which is one undo step and one CRDT change
    set.
  - The host MUST auto-close an unbalanced group when the hook returns (R02-08 extension).
  - Undo calls on a render thread are errors, and passive UI selectors such as draw events
    are not a place to change project state ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)).
  - Effects write project state through AEGP suites too. The guide's colour-sampling recipe
    stores a click in sequence data, forces a re-render, cancels it once the colour is known,
    and then adds keyframes *during the draw event* ([SDK: Effect UI & Events › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-ui-events/tips-and-tricks/)). That
    conflicts with the 13.5 rule above. The host SHOULD accept such writes on the UI thread and
    apply them as one transaction after the event returns, rather than fail them (inference).
    This mapping is therefore needed for the P3 effect host, not only for AEGPs.
- **Errors.** `AEGP_ReportInfo` (and a Unicode variant) shows a dialog that names the plug-in.
  There are also quiet-error brackets that must be balanced and access to the last error
  message ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_utilitysuite6), [SDK: AEGPs › AEGP Details](https://ae-plugins.docsforadobe.dev/aegps/aegp-details/)). AEGPs are asked to identify themselves
  and fail gracefully when a suite is missing ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#fail-gracefully)).
  - The host MUST route these to BAE's message centre (R11-01), with no modal dialog in
    headless renders, and MUST keep a quiet-error depth per plug-in that is reset when the hook
    returns.
- **Utilities.** Main HWND, show/hide floating palettes, paint and character palette colours
  (get and set), horizontal-scale-factor conversions, plug-in paths (user and all-users
  plug-in folders, application path; the plug-in's own path is documented as not
  implemented), "host is activated", "suppress interactive UI", OS console and debug log,
  font-list rescan, and the platform bundle reference on macOS (always null on Windows)
  ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_utilitysuite6)).
  - "Suppress interactive UI" MUST be true in headless and render-only helpers, consistent
    with `PF_IsRenderEngine` (13.3).
  - Paint and character palette colours map to BAE tool state (11); setters MAY be ignored.

### 14.7 Scripting through `AEGP_ExecuteScript`

- **What AE offers.** `AEGP_ExecuteScript` runs a script in UTF-8 or the application encoding
  and returns the value of the last line plus an optional error. `AEGP_IsScriptingAvailable`
  says whether scripting can run ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_utilitysuite6)). AE scripting is ExtendScript,
  which is JavaScript-based ([SDK: Introduction › Other Integration Possibilities](https://ae-plugins.docsforadobe.dev/intro/other-integration-possibilities/)). Plug-ins use it, for example, to read `app.version`
  ([SDK: Introduction › Compatibility Across Multiple Versions?](https://ae-plugins.docsforadobe.dev/intro/compatibility-across-multiple-versions/)).
- **BAE mapping.** BAE's script plug-ins run on QuickJS with an object model aligned to AE
  scripting (`ARCHITECTURE.md` §14). The compat host MUST execute the script synchronously on
  the UI thread through a C-ABI script bridge (R19-script).
- **Risk.** ExtendScript is an older JavaScript dialect with AE-specific objects and methods,
  such as `app.findMenuCommandId` ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_commandsuite1)). Fidelity is limited to the
  subset of the AE object model that BAE implements. Script-driven plug-ins will partly fail.

### 14.8 Threading

AEGPs have no threading support at all. Everything runs on the main thread, either inside a
hook or from the idle hook. The only exception is the idle trigger ([SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/)). The host MUST
call every AEGP hook on BAE's UI thread. In a sandbox, it MUST call them on the sandbox's
designated main thread, because macOS UI code must run there (inference).

### 14.9 Specialised AEGPs

| Kind | AE behaviour | BAE position |
|---|---|---|
| Artisan | Takes over rendering of 3D layers, one per composition, through render contexts available after effects are applied. Adobe discourages it, and interactive artisans exist for previews ([SDK: Artisans › Artisans](https://ae-plugins.docsforadobe.dev/artisans/artisans/)). | Out of scope. BAE owns its 3D path (04). |
| AEIO | Imports and exports media as ARGB worlds at 8, 16 or 32 bits, and must handle its own codecs. Adobe prefers MediaCore importers ([SDK: AEIOs › AEIOs](https://ae-plugins.docsforadobe.dev/aeios/aeios/)). | Out of scope. Equivalents are written against BAE's own importer and exporter plug-in API (08/19). |
| Keyframer | A menu-driven keyframe assistant, as in the Easy Cheese sample ([SDK: Introduction › Sample Projects](https://ae-plugins.docsforadobe.dev/intro/sample-projects/)). Adobe's suggested AEGP starting points are Projector (creating projects), Easy Cheese (keyframe assistant), IO (media formats) and Persisto (menu command and preferences) ([SDK: Introduction › How To Start Creating Plug-ins](https://ae-plugins.docsforadobe.dev/intro/how-to-start-creating-plug-ins/)). | Best served by BAE script plug-ins. These samples are also a good list of what AEGP authors expect to work. |

### 14.10 Why AEGP is the largest and last-considered scope

1. **Breadth.** About 45 AEGP suite families cover the entire AE object model, render queue
   and output modules included ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/), [SDK: Artisans › Artisan Data Types](https://ae-plugins.docsforadobe.dev/artisans/artisan-data-types/)). Effects touch about 20 read-mostly
   suites (§13.4).
2. **Semantic fidelity.** AEGPs assume AE's exact model: item types, stream match names and
   groups, layer flags, footage interpretation and command ids. Every mismatch with BAE's
   USD-like model is a behaviour difference that someone has to define in 15's mapping.
3. **Write access.** Every mutation must become 02 commands that are undoable and CRDT-synced.
   AE's "nasty, brutish and short" handle rules ([SDK: AEGPs › Data Types](https://ae-plugins.docsforadobe.dev/aegps/data-types/)) need generation-checked handles.
4. **UI coupling.** Menus, idle hooks, dialogs parented to the main window and native panels
   need the UI thread and native views. That conflicts with out-of-process hosting and
   portability (14.4).
5. **Lifecycle conflict.** AEGPs are loaded at launch and never unloaded ([SDK: AEGPs › Implementation](https://ae-plugins.docsforadobe.dev/aegps/implementation/)). BAE wants
   lazy loading and hot unload.
6. **Ecosystem signal.** No third-party host supports AEGPs ([SDK: Introduction › Third-party Plug-in Hosts?](https://ae-plugins.docsforadobe.dev/intro/third-party-plug-in-hosts/)). Adobe itself steers new
   integrations to scripting, HTML5 panels and Premiere importers ([SDK: Introduction › Other Integration Possibilities](https://ae-plugins.docsforadobe.dev/intro/other-integration-possibilities/), [SDK: Introduction › Sample Projects](https://ae-plugins.docsforadobe.dev/intro/sample-projects/)).
7. **Value.** Most AEGP functions already exist for BAE users through commands, the
   AE-aligned script API and BAE's own plug-in API (`ARCHITECTURE.md` §14).

**Recommended staging.**
- **P3**: the effect host plus only the effect-side "cheat" subset (§13.4).
- **P4** (if demand exists): read-mostly AEGPs, such as exporters of project data, run in
  their own sandbox. They get menu commands, idle hooks and project reads, and writes through
  commands.
- **Never**: Artisans, AEIOs and render-queue control.

## 15. Packaging and platforms

### 15.1 Binary formats

| Platform | Form | Facts | Host obligation | BAE |
|---|---|---|---|---|
| Windows | `.aex`, a DLL | Only the entry point needs exporting ([SDK: Introduction › Exporting Symbols in Effects](https://ae-plugins.docsforadobe.dev/intro/symbol-export/)). The PiPL is compiled from a `.r` file by Adobe tools into an `.rc` resource. Its properties are always in macOS byte order ([SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/)). GPU (DirectX) effects also ship a `DirectXAssets` folder next to the binary, which is loaded at runtime. Plug-ins that link the CUDA runtime dynamically may rely on the copy of that DLL that AE ships, which Adobe says may change ([SDK: Introduction › Building GPU Effects](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/)). There is no universal format: a Windows on Arm build is a separate ARM64 binary with its own PiPL entry ([SDK: Introduction › Windows on Arm Support](https://ae-plugins.docsforadobe.dev/intro/windows-on-arm-support/)). | MUST read the PiPL from the PE resources without executing code. MUST decode it big-endian (inference: the classic macOS byte order). MUST read the PE machine type and hybrid (ARM64X) metadata. MUST keep the plug-in's own folder in the DLL search path, so side-by-side DLLs and `DirectXAssets` resolve (inference). | 19 scanner, 01 loader (R01-04). |
| macOS | `.plugin`, a bundle containing a Mach-O executable | Universal binaries (arm64 + x86_64) need Xcode 12.2 or later ([SDK: Introduction › Apple Silicon Support](https://ae-plugins.docsforadobe.dev/intro/apple-silicon-support/)). The PiPL is compiled with Rez ([SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/)). Plug-ins ask for their `CFBundleRef` (`AEGP_GetPluginPlatformRef`, `PF_PlatData_BUNDLE_REF`) ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_utilitysuite6), [SDK: Effect UI & Events › UI Callbacks](https://ae-plugins.docsforadobe.dev/effect-ui-events/ui-callbacks/)). | MUST load through the bundle APIs so a bundle reference exists. MUST read the PiPL resource from the bundle. A slice is usable only if the PiPL also names an entry point for that architecture. Adding the `CodeMacARM64` entry is a required step of the Apple Silicon port ([SDK: Introduction › Apple Silicon Support](https://ae-plugins.docsforadobe.dev/intro/apple-silicon-support/)), so a universal binary without it is treated as Intel-only (inference). | 19, 01. |
| Both | Several PiPLs per file | Allowed, with AEGPs first. No other host supports several effects per binary ([SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/)). | MUST model one module with N descriptors (§2.1). | 19. |
| Both | Code registration | A newer entry point registers effects at runtime, possibly several per binary. Premiere supports it first, and AE "in a future release". Samples keep a PiPL as well ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/), [SDK: Introduction › Exporting Symbols in Effects](https://ae-plugins.docsforadobe.dev/intro/symbol-export/)). | MUST prefer the PiPL. MAY call the registration function, but only inside a sandbox at scan time. | 19 scan cache. |
| Both | Localised resources and extra files | Platform data can point to an external localised resource file ([SDK: Effect UI & Events › UI Callbacks](https://ae-plugins.docsforadobe.dev/effect-ui-events/ui-callbacks/)). Preview media lives in a `Preview` folder inside the bundle resources, keyed by the binary's file title, and only Premiere Pro Beta uses it ([SDK: Effect Details › Effect Preview Media (Effects Panel)](https://ae-plugins.docsforadobe.dev/effect-details/effect-preview-media/)). Presets are `.ffx` files that installers put in the Presets folder next to the AE executable ([SDK: Effect Details › Tips & Tricks](https://ae-plugins.docsforadobe.dev/effect-details/tips-tricks/)). | SHOULD keep side files with the module. Never copy them into a project bundle. MAY show preview media in BAE's effect browser. | 19, 21, 12 (R12-03). |

### 15.2 Architectures and hosting feasibility

AE's PiPL names one entry-point descriptor per architecture:
- `CodeMacIntel64` and `CodeMacARM64` on macOS, `CodeWin64X86` and `CodeWinARM64` on Windows.
- The names may differ per architecture ([SDK: Introduction › Apple Silicon Support](https://ae-plugins.docsforadobe.dev/intro/apple-silicon-support/), [SDK: Introduction › Windows on Arm Support](https://ae-plugins.docsforadobe.dev/intro/windows-on-arm-support/)).
- Native Apple Silicon and Windows on Arm Adobe hosts offer only plug-ins with a native
  build (same pages). AE never runs a foreign-architecture plug-in in process.
- The guide is silent on ARM64EC and ARM64X. Microsoft documents that an Arm64EC binary
  presents as an x64 image (x64 machine type plus hybrid metadata), and that an ARM64X binary
  carries both x64 and Arm64 code ([Microsoft: Arm64EC for Windows 11 apps on Arm](https://learn.microsoft.com/en-us/windows/arm/arm64ec)). Such a binary would
  therefore declare its entry under the x64 descriptor, or under both (inference).
- Usability needs both a PiPL entry descriptor and a matching code slice. The host MUST take
  the intersection, not either alone.

| BAE process | Plug-in has | Mode | Basis |
|---|---|---|---|
| macOS arm64 | arm64 or universal | in-process (trusted only) or native sandbox | AE native on Apple Silicon offers only plug-ins with an Apple Silicon build ([SDK: Introduction › Apple Silicon Support](https://ae-plugins.docsforadobe.dev/intro/apple-silicon-support/)). |
| macOS arm64 | x86_64 only | x86_64 helper under Rosetta (translated sandbox) | Apple does not allow arm64 and x86_64 code in one process. Translation covers a whole process ([Apple: About the Rosetta translation environment](https://developer.apple.com/documentation/apple-silicon/about-the-rosetta-translation-environment)). Run out-of-process plug-ins through one helper per architecture ([Apple: Porting your macOS apps to Apple silicon](https://developer.apple.com/documentation/apple-silicon/porting-your-macos-apps-to-apple-silicon)). **Sunset:** Rosetta stays general-purpose only through macOS 27, then a subset for older games remains ([Apple: About the Rosetta translation environment](https://developer.apple.com/documentation/apple-silicon/about-the-rosetta-translation-environment)). Rosetta translates AVX and AVX2 but does not run AVX-512 code. Apple also suggests that users can launch a universal app under Rosetta to run older plug-ins ([Apple: About the Rosetta translation environment](https://developer.apple.com/documentation/apple-silicon/about-the-rosetta-translation-environment)); for BAE that would slow the whole app, so a translated helper is preferred (inference). |
| macOS x86_64 | x86_64 | native | — |
| Windows x64 | x64 (including Arm64EC-hybrid x64 images) | native | An x64 process loads x64 and Arm64EC binaries ([Microsoft: Arm64EC for Windows 11 apps on Arm](https://learn.microsoft.com/en-us/windows/arm/arm64ec)). |
| Windows ARM64 | ARM64, or ARM64X | native | Native WoA Adobe apps load only native plug-ins ([SDK: Introduction › Windows on Arm Support](https://ae-plugins.docsforadobe.dev/intro/windows-on-arm-support/)). An Arm64 process loads only Arm64 code. Arm64X binaries load in both ([Microsoft: Arm64EC for Windows 11 apps on Arm](https://learn.microsoft.com/en-us/windows/arm/arm64ec)). |
| Windows ARM64 | x64 (possibly with ARM64EC code inside) | x64 helper under emulation (sandbox only) | x64 and Arm64EC processes load x64 and Arm64EC binaries. Arm64 processes cannot ([Microsoft: Arm64EC for Windows 11 apps on Arm](https://learn.microsoft.com/en-us/windows/arm/arm64ec), [Microsoft: Add Arm support to your Windows app](https://learn.microsoft.com/en-us/windows/arm/add-arm-support)). |
| Windows ARM64EC build of BAE | x64 | in-process, emulated | Per Microsoft's table ([Microsoft: Arm64EC for Windows 11 apps on Arm](https://learn.microsoft.com/en-us/windows/arm/arm64ec)). Slow (emulated x64) but needs no IPC. Not recommended. |
| any | no matching slice | unsupported → missing-effect placeholder | §15 of `ARCHITECTURE.md`. |

Windows on Arm GPU effects need the ARM64 CUDA toolchain on ARM64. The guide says AE 27
selects the CUDA version by architecture ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/), [SDK: Introduction › Building GPU Effects](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/)). The guide says nothing on
whether GPU frameworks work from an emulated x64 helper on Windows on Arm. Treat translated
helpers as CPU-only (inference).

### 15.3 Install locations and discovery

| Location | Facts | BAE behaviour |
|---|---|---|
| Common (MediaCore) folder | Shared with Premiere Pro and Media Encoder.<br>- Windows: the explicit path is in `HKLM\SOFTWARE\Adobe\After Effects\[version]\CommonPluginInstallPath`.<br>- macOS: `/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/`, where "7.0" is fixed for all CC versions.<br>- Premiere does not follow aliases or shortcuts there ([SDK: Introduction › Where Installers Should Put Plug-ins](https://ae-plugins.docsforadobe.dev/intro/where-installers-should-put-plug-ins/)).<br>- Effects installed only for Premiere are missed by Media Encoder ([SDK: Premiere Pro › Plug-in Installation](https://ae-plugins.docsforadobe.dev/ppro/plug-in-installation/)). | SHOULD scan it read-only. |
| Per-user common folder (macOS) | `~/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/` is the recommended development build location. On Windows the development example is the MediaCore folder under Program Files, and installers must use the registry path instead ([SDK: Introduction › Sample Projects](https://ae-plugins.docsforadobe.dev/intro/sample-projects/)). | SHOULD scan it read-only. Plug-ins found only here are often unsigned development builds (15.4). |
| AE-only folder | - Windows: `HKLM\SOFTWARE\Adobe\After Effects\[version]\PluginInstallPath`.<br>- macOS: `/Applications/Adobe After Effects [version]/Plug-ins/`.<br>- Used when a plug-in needs AE-only suites ([SDK: Introduction › Where Installers Should Put Plug-ins](https://ae-plugins.docsforadobe.dev/intro/where-installers-should-put-plug-ins/)). This folder also holds AE's own bundled plug-ins; the guide's thread-safety tool example points at Adobe's Curves effect under it ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/)). | SHOULD scan it read-only. MUST exclude Adobe's bundled first-party plug-ins by default. They belong to the AE installation, and hosting them is a licensing question (open question). Identify them by location inside the AE installation and by Adobe's code signature, not by an "ADBE" match-name prefix alone: Adobe's own samples carry "ADBE" match names, and Adobe warns developers that unknown "ADBE " names are excluded from GPU rendering, which suggests some third-party plug-ins keep the prefix (inference) ([SDK: Introduction › Exporting Symbols in Effects](https://ae-plugins.docsforadobe.dev/intro/symbol-export/), [SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). |
| Scan rules | AE descends up to 10 levels. It follows macOS aliases, not Windows shortcuts. It skips folders whose names end in parentheses or start with `¬` (macOS) or `~` (Windows) ([SDK: Introduction › Where Installers Should Put Plug-ins](https://ae-plugins.docsforadobe.dev/intro/where-installers-should-put-plug-ins/)). | SHOULD apply identical rules, because users disable plug-ins with them. |
| User-added folders | — | MUST allow additional folders. MUST keep them separate from BAE's own plug-in folders (19). |
| Duplicates | The guide is silent on how AE picks between two installs of one match name. On Windows, separate x64 and ARM64 builds can coexist in different folders (15.1). | MUST key installs by (match name, architecture). MUST pick deterministically (highest effect version, then folder priority) and show the choice to the user (inference). |

**Discovery pipeline (P3).**
1. Enumerate the folders. On Windows, do this for every AE version found in the registry.
2. For each candidate, read the binary headers: PE machine type, ARM64X hybrid, Mach-O slices.
   Read the code signature identity (Team ID or Authenticode signer) for the trust registry
   (R19-08), without loading the code.
3. Parse the PiPLs statically, and intersect their entry descriptors with the slices (15.2).
4. Optionally, inside a sandbox, call the registration entry point when no PiPL is found.
5. Store the result in a scan cache keyed by (path, size, mtime, content hash, host
   architecture). Premiere loads every plug-in once, sends `PF_Cmd_GLOBAL_SETUP` and caches
   the capabilities; a plug-in can opt out with `PF_SetNoCacheOnLoad`
   ([SDK: Premiere Pro › Plug-Ins... Reloaded](https://ae-plugins.docsforadobe.dev/ppro/plug-ins-reloaded/)). BAE SHOULD do the same global setup inside a sandbox, so
   the scan never runs third-party code in the main process.
6. Publish lazy effect types, keyed by match name and version, to the 19 registry
   (R19-03).

Parameter definitions are known only after `PF_Cmd_PARAMS_SETUP` runs inside the sandbox. They
are cached by (match name, effect version) and refreshed when the scan cache entry changes.

### 15.4 macOS: code signing, notarization, hardened runtime, library validation

| Rule | Source | Implication for BAE |
|---|---|---|
| Notarization requires the hardened runtime. Entitlements apply to executables only, and in-process plug-ins inherit the entitlements of their host executable. | [Apple: Hardened Runtime](https://developer.apple.com/documentation/security/hardened-runtime) | Whatever executable loads AE plug-ins defines what they may do. |
| Under the hardened runtime, library validation is on. A process loads only code signed by Apple or by its own Team ID. `com.apple.security.cs.disable-library-validation` allows plug-ins signed by other developers. Gatekeeper runs extra checks on programs that disable it. | [Apple: Disable Library Validation Entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.cs.disable-library-validation) | Putting the entitlement on the main BAE executable weakens it and triggers extra Gatekeeper checks. Instead, sign a dedicated plug-in-host helper with this entitlement and keep the main app validated (R01-05). Consequence: on macOS the "in-process" hosting mode is impossible for third-party plug-ins unless the main app also carries the entitlement; it remains available only for code signed with BAE's own Team ID (inference from the same page). |
| An app that loads plug-ins directly must match their architecture; out-of-process plug-ins may differ. One XPC service manages either native or translated plug-ins, not both. | [Apple: Porting your macOS apps to Apple silicon](https://developer.apple.com/documentation/apple-silicon/porting-your-macos-apps-to-apple-silicon) | Same split as 15.2. |
| Plug-ins copied from a download (not installed by a signed installer) may carry the quarantine attribute, and Gatekeeper may assess them when first loaded. | Not documented in the guide or the pages above (inference, open question). | The scanner SHOULD report quarantined bundles and let the user resolve them; BAE MUST NOT strip the attribute silently. |
| On Apple Silicon every native executable must be signed, but an ad-hoc signature is enough. The rule does not apply to Rosetta-translated x86_64 code. | [Apple: macOS Big Sur 11.0.1 Universal Apps Release Notes](https://developer.apple.com/documentation/macos-release-notes/macos-big-sur-11_0_1-universal-apps-release-notes) | Unsigned arm64 plug-ins cannot run. |
| AE on macOS 15+ refuses unsigned plug-ins, so developers ad-hoc sign them. Modifying a bundle after signing stops it loading. | [SDK: Introduction › Debugging Plug-ins](https://ae-plugins.docsforadobe.dev/intro/debugging-plug-ins/) | BAE MUST never write into plug-in bundles: no patching and no resource rewriting. |
| AE release builds can no longer be attached to with a debugger without re-signing. Beta builds need a developer-mode marker file. | [SDK: Introduction › Attaching a Debugger to After Effects on macOS](https://ae-plugins.docsforadobe.dev/intro/debugging-ae-macos/) | Plug-in vendors debugging under BAE need a helper build with `get-task-allow` (inference). |
| One XPC service or helper per architecture, each with a unique bundle id so both can run at once. | [Apple: Porting your macOS apps to Apple silicon](https://developer.apple.com/documentation/apple-silicon/porting-your-macos-apps-to-apple-silicon) | Helpers `bae-plugin-host` (arm64) and `bae-plugin-host-x86_64`. The second is optional and sunset with Rosetta (through macOS 27). |

The helper MUST NOT use the App Sandbox. Plug-ins read licence files and caches in arbitrary
system and user locations and may need network access (inference, see 15.6). Plug-ins that JIT
or generate code may need further runtime exceptions on the helper, such as unsigned executable
memory. This is unknown per plug-in, and AE's own entitlements are not documented in the guide.

### 15.5 Windows: Authenticode, SmartScreen, Smart App Control, DLL loading

| Topic | Fact | Implication |
|---|---|---|
| Smart App Control | Unknown, unsigned code is blocked by default. An app is allowed if Microsoft's app-intelligence service predicts it is safe or it carries a trusted CA signature ([Microsoft: Smart App Control](https://learn.microsoft.com/en-us/windows/apps/develop/smart-app-control/overview)). When an app loads an unsigned file, the user sees "part of this app has been blocked" and only the dependent functionality fails ([Microsoft Support: Smart App Control has blocked part of this app](https://support.microsoft.com/en-US/Windows/Security/Threat-Malware-Protection/smart-app-control-has-blocked-part-of-this-app)). | BAE and its helpers MUST be Authenticode-signed. Unsigned third-party `.aex` files may be blocked by the OS whatever BAE does. Surface this as a clear "blocked by OS policy" state, not a crash. |
| SmartScreen | Applies to downloaded installers and apps, not to DLLs an app loads (inference). | Affects BAE's installer only. |
| Signature mitigations | A process can opt into requiring Microsoft-signed DLLs (inference, Windows process mitigation policies). | The plug-in helper MUST NOT enable such policies. The main BAE process MAY enable them only if it never loads AE plug-ins in process. |
| DLL search and symbols | Plug-ins exporting their own Boost clashed with AE's Boost and caused hangs and crashes ([SDK: Introduction › Exporting Symbols in Effects](https://ae-plugins.docsforadobe.dev/intro/symbol-export/)). | Load modules with local scope. On Windows use the plug-in folder as the dependency search path, and isolate vendors from each other in separate sandboxes (inference). |

### 15.6 Licensing, DRM and host identity (risk register)

| Risk | Evidence | Mitigation / decision needed |
|---|---|---|
| Plug-ins read registration identity | `PF_GetPersonalInfo` returns the user's registration info, and the guide jokes about watermarking with it ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). `PF_InData` carries the host serial number and application id ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). `AEGP_HostIsActivated` reports whether AE is activated ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_utilitysuite6)). | BAE MUST NOT invent Adobe serial or activation data. It MUST return neutral values or the missing-suite error, and accept that some plug-ins then refuse or watermark. Open question. |
| Host checks | Plug-ins branch on the application id ([SDK: Introduction › Third-party Plug-in Hosts?](https://ae-plugins.docsforadobe.dev/intro/third-party-plug-in-hosts/), [SDK: Premiere Pro › Premiere Pro & Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/ppro/)). Render-engine mode is queryable ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). Vendor "render-only" licences work only in headless command-line rendering (`aerender`) and need a full licence elsewhere ([Maxon support: Red Giant Render Only](https://support.maxon.net/hc/en-us/articles/15216357770140-How-do-I-set-up-the-Red-Giant-Render-Only), [aescripts: Plexus 3 Render Only](https://aescripts.com/plexus-3-render-only/); both seen only through a search summary, the Maxon page refused direct fetch). | Presenting as AE or Premiere is impersonation. Presenting as BAE may disable plug-ins. This needs a legal and product decision (open question). `PF_IsRenderEngine` MUST be true only in genuinely headless renders, so a vendor's render-only licence is never unlocked in an interactive session (inference). |
| Online activation and periodic checks | Some vendors require periodic online licence checks ([aescripts: License Code FAQ](https://aescripts.com/knowledgebase/index/view/faq/license-code-faq); seen only through a search summary). | The helper needs network access and a writable licence store. Licensing dialogs come from the helper process, with focus and parenting risk on macOS (14.4). |
| Adobe policy | Adobe "neither supports nor recommends" third-party hosts of its plug-in API and will not support such efforts ([SDK: Premiere Pro › Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/other-hosts/)). | Feeds the legal assessment that `ARCHITECTURE.md` §15 requires before implementation. |
| Watermark and cache | Plug-ins flag output as watermarked ("unsuitable for final use", typically an unlicensed demo) at global setup, and may flip the flag at runtime through dynamic flags when, for example, a floating licence changes; the next render must match the last answer or wrong frames get cached ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/), [SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/)). GUID mixing can be used instead of the flag ([SDK: Introduction › What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). A vendor knowledge base notes that AE's cache can keep showing watermarked frames after licensing (search summary of [aescripts: How to register plugins](https://aescripts.com/knowledgebase/index/view/faq/how-to-register-plugins/)). | MUST re-query dynamic flags and invalidate the instance's cached frames when the watermark state changes (R04-11). SHOULD warn before a final export that contains watermarked frames (inference). |

### 15.7 Localisation and strings

- **UI language.** `PF_AppGetLanguage` returns AE locale codes such as `zh_CN` and `en_US`
  ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/), [SDK: Introduction › Localization](https://ae-plugins.docsforadobe.dev/intro/localization/)). The host MUST map BAE's zh/en UI locales to these codes.
- **Non-Unicode strings.** Strings such as param names are multibyte in the application's
  locale: the OEM code page on Windows and the application text encoding on macOS. From AE 27
  a plug-in can opt into UTF-8, and the fixed-size name buffers do not grow ([SDK: Introduction › Localization](https://ae-plugins.docsforadobe.dev/intro/localization/)).
  - The host MUST transcode to UTF-8 for the 02 model, using the codepage the plug-in expects.
  - Out of process, the helper's locale MUST be set to match (inference).
- **Match names.** Match names are 7-bit ASCII. UI names use the application runtime encoding,
  for example ISO 8859-1 for most languages on Windows ([SDK: AEGPs › AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/aegp-suites/#aegp_effectsuite4)).
- **PiPL strings.** The display name may be up to 47 characters. Non-ASCII text in a PiPL
  resource uses the system code page ([SDK: Introduction › PiPL Resources](https://ae-plugins.docsforadobe.dev/intro/pipl-resources/)). The scanner MUST decode PiPL
  name and category with the code page of the build platform (Windows ANSI, or Mac Roman for
  Rez output, inference) and store UTF-8.
- **Plug-in UTF-8 opt-in.** `PF_OutFlag2_STRINGS_ARE_UTF8` applies to AE 27 and later only,
  and the PiPL flags must agree with the code ([SDK: Introduction › Localization](https://ae-plugins.docsforadobe.dev/intro/localization/)). The host MUST pick
  the transcoding per module from this flag.
- **Language codes.** The documented codes cover Chinese, English, French, German, Italian,
  Japanese, Korean and Spanish ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). BAE ships zh and
  en (`AGENTS.md`), so only `zh_CN` and `en_US` are ever reported.

### 15.8 Check of `pipl.rs` and `process.rs` against the guide

- **`pipl.rs`.**
  - `MatchName` accepts any UTF-8. Match names are 7-bit ASCII (15.7); it should reject or
    flag anything else.
  - `CodeArch::WindowsArm64Ec` is not a documented PiPL descriptor. An Arm64EC binary is an
    x64 image to the loader, and ARM64X is a property of the PE image (15.2). The PiPL
    descriptor set and the binary's code slices are two different facts.
  - `hosting_feasibility` looks at slices only. It ignores the PiPL entry descriptors (15.2),
    returns `Unsupported` for an x64 slice under a `WindowsArm64Ec` host although Microsoft
    documents that case as in-process, and cannot express ARM64X (usable by both x64 and
    ARM64 hosts).
  - `PluginPackage::WindowsDll` has no machine type, hybrid flag, signer or side-car asset
    list, so Windows feasibility cannot be computed at all.
  - `PluginDescriptor.name`/`category` are `String` with no record of the source code page
    (15.7). `search_keywords_json` and `description` are documented as Premiere Pro Beta only.
  - There is no scan-cache metadata (path, size, mtime, hash, host architecture, no-cache
    opt-out) and no signing identity for the trust registry.
- **`process.rs`.**
  - "One sandbox per module" breaks plug-in-published suites, AEGP↔effect calls
    (`AEGP_EffectCallGeneric`) and vendor AEGP+effect pairs (13.1). It needs helper *groups*.
  - Its rationale for macOS ("a notarized, library-validated BAE app keeps its entitlements")
    is right, but `HostingMode::InProcess` is then impossible on macOS for third-party code
    (15.4). The doc should say so.
  - There is no re-entrancy model: no parent-call id on a nested `Call`, and no thread lane on
    a `Callback`, although callbacks come from the plug-in's own worker threads and a checkout
    can trigger a nested render of the same module (13.2).
  - `SandboxToHost::Progress` is a message. `PF_PROGRESS`/`PF_ABORT` run once per scanline
    and need a shared atomic control block per call (13.2).
  - `Return { err: i32 }` documents "0 = success", a numeric ABI convention the crate rules
    keep out of P1; it should carry a typed error kind (`error.rs`) mapped by the P3 shim.
  - `CallbackKind::Suite { suite: &'static str, .. }` cannot be decoded from the wire into a
    `&'static str`; it should carry a `SuiteKey` plus version.
  - There is no UI/window channel: Drawbot display-list batches, cursor and invalidate
    messages, info-palette text, owner-window token and modal begin/end (13.2, 14.4).
  - There is no scan/registration request message, although `pipl.rs` says registration
    callbacks run only in the sandbox.
  - `TranslatedSandbox` lacks the Rosetta caveats: general-purpose only through macOS 27, no
    AVX-512, GPU interop unknown (15.2).

## 16. Third-party plug-in expectations

This section looks at what commercial AE effect plug-ins actually depend on, so that BAE can rank P3 compatibility work by value and reserve the right core hooks now. SDK facts cite the community-hosted copy of Adobe's guide. Vendor facts cite public vendor or retailer pages. Some vendor pages could not be fetched directly and were read only through search-engine summaries; those are marked "(via search)" and must be checked again before anyone relies on them. Anything not documented is marked "(inference)".

### 16.1 What the SDK itself says about non-AE hosts

| Fact | Source | Consequence for BAE |
|---|---|---|
| Adobe's stated policy is that it "neither supports nor recommends" building Adobe-compatible third-party hosts. It calls a reverse-engineered partial host possible but unsupported. In the same page it also offers to help other hosts and asks developers to refer such hosts to Adobe. | [SDK: Premiere Pro & Other Hosts › Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/other-hosts/) | Expect no formal support; the offer of help is informal. An outreach request is cheap (L-04, L-34). |
| Plug-ins tell hosts apart with `PF_InData>appl_id`. AE reports its own four-character creator code; Premiere Pro and Premiere Elements report a different one. The guide says to use this field "to test whether your plug-in, licensed for use with one application, is being used with another". | [SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/), [SDK: Introduction › Third-party Plug-in Hosts?](https://ae-plugins.docsforadobe.dev/intro/third-party-plug-in-hosts/) | Whatever identity BAE reports picks the plug-in's code path *and* its licensing behaviour. This is an engineering decision and a legal one (L-12, L-13). |
| "As of this writing, no third party hosts support SmartFX, or our AEGP functions." Adobe says SmartFX "has created quite a bit of distance" between the AE API and other hosts. | [SDK: Introduction › Third-party Plug-in Hosts?](https://ae-plugins.docsforadobe.dev/intro/third-party-plug-in-hosts/), [SDK: Premiere Pro & Other Hosts › Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/other-hosts/) | A host with SmartFX and selected AEGP calls would be the first third-party one. Plug-in vendors have never tested such a host. |
| Adobe's own second host, Premiere, omits 3D calls (cameras, lights, auxiliary channels), AE-style 16 bpc, SmartFX and all AEGP calls. It reaches 32-bit float through its own pixel-format suites instead. | [SDK: Premiere Pro & Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/ppro/), [SDK: Premiere Pro & Other Hosts › Bigger Differences](https://ae-plugins.docsforadobe.dev/ppro/bigger-differences/), [SDK: Premiere Pro & Other Hosts › Unsupported Features](https://ae-plugins.docsforadobe.dev/ppro/unsupported-features/) | Plug-ins already branch on the host. A host that is neither AE nor Premiere may land in an untested fallback branch, or be refused (inference). |
| Plug-ins probe for AE-only suites (the guide names the AEGP canvas suite as an example) and return an error so that a host blacklists them. Samples show "host application and version" checks. | [SDK: Premiere Pro & Other Hosts › Unsupported Features](https://ae-plugins.docsforadobe.dev/ppro/unsupported-features/), [SDK: Premiere Pro & Other Hosts › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/) | A missing suite is a signal that plug-ins act on. The host MUST answer "absent" cleanly for suites it does not implement, never with a stub that misbehaves (inference). |
| Premiere sends field-by-field renders, low-quality-then-high-quality renders, speculative renders and out-of-order times, and calls multiple instances concurrently. The guide tells effects not to rely on render order or mutable statics. | [SDK: Premiere Pro & Other Hosts › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/) | BAE's scheduler may also render out of order. Plug-ins tested in Premiere are more tolerant of this; AE-only plug-ins may not be (inference). |
| Installers are told to use a common Adobe plug-in folder (MediaCore) or an AE-only folder. AE scans recursively to a fixed depth and skips specially named folders. | [SDK: Introduction › Where Installers Should Put Plug-ins](https://ae-plugins.docsforadobe.dev/intro/where-installers-should-put-plug-ins/) | BAE would find third-party binaries in Adobe-owned folders mixed with Adobe's own binaries (see 16.4 E12, L-30). |
| Native-ARM Adobe hosts load only plug-ins that have a native-ARM slice. macOS 15+ refuses unsigned plug-ins. | [SDK: Introduction › Apple Silicon Support](https://ae-plugins.docsforadobe.dev/intro/apple-silicon-support/), [SDK: Introduction › Windows on Arm Support](https://ae-plugins.docsforadobe.dev/intro/windows-on-arm-support/), [SDK: Introduction › Debugging Plug-ins](https://ae-plugins.docsforadobe.dev/intro/debugging-plug-ins/) | Vendors only test their native slices in native hosts. BAE running x86_64 slices under translation goes beyond anything Adobe hosts do. |

### 16.2 Precedents: non-Adobe hosts that ran AE plug-ins

| Host | What it supported | Source | Lesson |
|---|---|---|---|
| Grass Valley EDIUS ("After Effects Plug-in Bridge") | The user registers a folder of AE plug-ins, which then appear as video filters. Only 64-bit plug-ins are supported, and the manual says operation of all AE plug-ins is "not guaranteed". Early beta notes (2009) excluded mask/path-dependent filters and some Red Giant tools (via search). Later EDIUS versions steer users to an OFX bridge instead (via search). | [Web: EDIUS 7 manual, AE Plug-in Bridge](https://wwwapps.grassvalley.com/manuals/EDIUS7_USER_EN/l06/l0_6_l1_15_l2_3.html), [Web: slashcam on EDIUS AE bridge](https://www.slashcam.de/news/single/After-Effects-Plugins-in-Edius-per-Plugin-nutzen-7686.html), [Web: ediusworld OFX bridge](https://www.ediusworld.com/?p=7138) | An opt-in, folder-based, "best effort" bridge is the established pattern. Path/mask/3D-dependent plug-ins are the first to fail. |
| eyeon Fusion 5.x | Ran AE plug-ins on Windows (reported in 2006). Support was dropped around Fusion 7 because Fusion was 64-bit and most AE plug-ins of the time were 32-bit. Fusion moved to OFX. | (via search) [Web: Blackmagic forum thread](https://forum.blackmagicdesign.com/viewtopic.php?p=196027), [Web: Wikipedia, Blackmagic Fusion](https://en.wikipedia.org/wiki/Blackmagic_Fusion) | Architecture transitions (32→64-bit then; x86→ARM now) kill bridges. Keep the bridge isolated so it can be dropped cheaply. |
| Discreet/Autodesk Combustion | Supported "various" AE-compatible third-party plug-ins against a vendor-tested list. Full compatibility was not guaranteed, and AE's bundled plug-ins did not work. | (via search, reposted manual excerpt) [Web: render.ru thread](https://render.ru/xen/threads/gt-gt-gt-gt-plagi-dlja-ae-v-kombaxe.88595/) | Publish a tested-plug-in list rather than claim generic compatibility. That also fits the trademark rule that compatibility claims must be true (L-20). |
| Adobe Premiere Pro / Elements | A partial host with no SmartFX, no AE-style 16 bpc, no AEGP and no 3D (16.1). | [SDK: Premiere Pro & Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/ppro/) | Even Adobe ships a subset, and vendors write host-specific branches. |

Market shift (vendor pages): the big multi-host suites now ship separate OpenFX builds for non-Adobe hosts. Examples are Sapphire (AE, Premiere, Avid, and OFX for Flame, Nuke, Resolve, Fusion and VEGAS) and parts of Red Giant (Resolve and FCP builds) ([Web: Sapphire supported hosts](https://borisfx.com/products/sapphire/supported-hosts/), [Web: Red Giant 2026.2.0 notes](https://support.maxon.net/hc/en-us/articles/24114684657692-Red-Giant-2026-2-0-December-3-2025), via search). OpenFX has its own region-of-interest action ([Web: OpenFX rendering reference](https://openfx.readthedocs.io/en/main/Reference/ofxProcessingArch.html)). This leads to an **inference**: for these multi-host suites, an OFX host in 19 may give the same user value with far less ABI and legal risk. The AE bridge is needed mainly for AE-only tools (Video Copilot, Trapcode, Plexus, many aescripts products).

### 16.3 What popular plug-in classes rely on

Legend: ✔ documented by the vendor or the SDK, ✱ vendor marketing claim, ? unknown, (inf) inference. "Scene" means comp-level data that the effect reads through AEGP calls (cameras, lights, other layers). Public evidence was gathered on 2026-11-20. Rows marked "(via search)" were read only through search-engine summaries of the vendor pages, so their wording must be checked again.

**Summary matrix** (what the host must provide for the class to work fully):

| Class | Example products | 32 bpc / SmartFX | Host GPU API | Own GPU context | MFR | Scene / AEGP | Custom ECW / overlay | Own windows | Activation | Non-AE builds |
|---|---|---|---|---|---|---|---|---|---|---|
| Particle / 3D generators | Trapcode Particular, Form | (inf) yes | ? | ✱ yes | ✱ yes | ✱ heavy | (inf) yes | ✱ yes | ✱ yes | few |
| 3D object renderers | Element 3D | ? | no (inf) | ✱ OpenGL | ✱ yes | (inf) heavy | (inf) | (inf) yes | ✱ yes | none (✔ AE only) |
| Light / lens | Optical Flares | ? | no (inf) | ✱ optional GPU | ✱ yes | (inf) lights | (inf) | (inf) yes | ✱ yes | none now |
| Plexus-type 3D line/point systems | Plexus | ? | ? | ✱ OpenGL DoF | ✱ yes | ✱ camera, lights | (inf) | ? | ✱ incl. render-only SKU | none |
| Multi-effect suites | Sapphire, Continuum | ✱ yes | ? | ✱ CUDA / Metal | ✱ yes | (inf) little | (inf) some | ✱ (Mocha etc.) | ✱ yes | ✔ OFX, Avid |
| Look / finishing suites | Magic Bullet, Universe | ? | ? | ✱ OpenGL / OpenCL / CUDA | ? | (inf) little | (inf) | ✱ Looks UI, dashboard | ✱ yes | ✔ FCP, Resolve |
| Glows / stylise | Deep Glow, Saber | ✱ 32 bpc (Deep Glow) | ? | ✱ GPU required (Deep Glow) | ✱ yes | (inf) masks/text (Saber) | (inf) | no (inf) | ✱ (Deep Glow) | none |
| Transitions / glitch | Twitch | ? | ? | ? | ? | (inf) layer params | (inf) | no | ✱ key file | none |
| Scripts | Duik | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | out of scope |

**Per-class evidence and host consequences:**

| Class | Evidence (public) | Host consequence |
|---|---|---|
| Trapcode Particular / Form | AE **lights** can be emitters. Lights are found by **layer-name prefix**, and particle colour can come from the light ([Web: Maxon help, Light emitter](https://help.maxon.net/rg/en-us/Content/html/Particles-and-3D/Trapcode%20Particular/emitter-light-emitter.html)). A separate **Designer** with blocks ([Web: Maxon help, Designer blocks](https://help.maxon.net/rg/en-us/Content/html/Particles-and-3D/Trapcode%20Particular/blocks-in-designer.html)). "All Trapcode tools" support MFR from Trapcode 17, with more MFR optimisation in 2024.1 ([Web: Trapcode 17](https://support.maxon.net/hc/en-us/articles/4406575504274-Trapcode-17-September-14-2021), [Web: Trapcode 2024.1.0](https://support.maxon.net/hc/en-us/articles/14857843795228-Trapcode-2024-1-0-January-17-2024)). Needs an NVIDIA Maxwell+ GPU or a discrete GPU with OpenCL ([Web: Maxon minimum requirements](https://support.maxon.net/hc/en-us/articles/23987275556764)). Licensing goes through the Maxon App with a background service and an offline grace period ([Web: Maxon App licences](https://help.maxon.net/mxa/en-us/Content/html/my_licenses_tab.html), [Web: Maxon error 11025](https://support.maxon.net/hc/en-us/articles/5468542050844-Maxon-App-Error-11025-Can-not-connect-to-Red-Giant-Service)). Unregistered legacy serials render red-X watermarks ([Web: Toolfarm on Maxon licensing](https://toolfarm.com/tutorial/red-giant-licenses)). All via search. | Comp-wide layer enumeration with names and types; light and camera parameters at arbitrary times; read-dependency tracking (R4); a UI-capable helper for Designer (R6); helper access to network and local IPC (R9). Using the comp camera is (inf). |
| Element 3D | GPU **mandatory**, AE CS5+, "only compatible with Adobe After Effects" ([Web: Video Copilot compatibility](https://www.videocopilot.net/support/category/compatibility)). Uses a **custom OpenGL render engine** ([Web: B&H listing](https://www.bhphotovideo.com/c/product/892486-REG/video_copilot_e3d_element_3d.html), via search). Updated for MFR (v2.2.3) ([Web: Video Copilot AE 2022 update](https://videocopilot.net/products/ae22)). Adobe's KB lists GPU-using plug-ins such as Element 3D as crash-prone when AE's own panel hardware acceleration is on ([Web: Adobe KB, OpenGL plug-ins crash](https://helpx.adobe.com/after-effects/desktop/kb/OpenGL-plug-ins-crash.html), via search). Forum reports say it scales poorly under MFR (via search). | Plug-in-owned GL contexts share the driver with BAE's wgpu device. Prefer out-of-process. Cap per-module concurrency even when the plug-in is MFR-flagged (inf). Camera and light reads (inf). |
| Optical Flares | GPU optional. Mac Intel-only in the old table, with later M1-native updates ([Web: Video Copilot compatibility](https://www.videocopilot.net/support/category/compatibility), [Web: Toolfarm, VC M1 ready](https://www.toolfarm.com/?p=80809), via search). MFR update v1.3.8 ([Web: Video Copilot AE 2022 update](https://videocopilot.net/products/ae22)). Users work around crashes by turning off GPU inside the plug-in's own UI (forum, via search). | Expect a separate design window (inf) and an internal GPU switch. Host GPU interop is not needed. |
| Plexus | Reacts to the comp **camera and lights** and uses nulls ([Web: 80.lv](https://80.lv/articles/rowbyte-reveals-plexus-3)). OpenGL-accelerated depth of field ([Web: Toolfarm Plexus](https://www.toolfarm.com/?p=5983)). MFR support, plus a **render-only licence** for headless `aerender` machines ([Web: aescripts Plexus 3 render-only](https://aescripts.com/plexus-3-render-only/)). All via search. | Scene reads (R4). BAE headless or NAS rendering raises a licence-classification question (L-32). |
| Sapphire / Continuum | Hosts: AE, Premiere, Avid, and OFX hosts ([Web: Sapphire supported hosts](https://borisfx.com/products/sapphire/supported-hosts/)). GPU via CUDA (Windows/Linux) and Metal (macOS) with CPU fallback ([Web: Sapphire 2021 Metal](https://blog.borisfx.com/press/sapphire-2021-adds-metal-gpu-acceleration-and-must-have-lighting-vfx)). MFR since 2021.5 ([Web: Boris FX MFR](https://borisfx.com/news/boris-fx-supports-adobe-multi-frame-rendering-delivers-new-mocha-ae/)). "Cross host" licences exist ([Web: Boris FX EULA](https://support.borisfx.com/hc/en-us/articles/10006419267597-End-User-License-Agreement)). All via search. | Covered by an OFX host as well (16.2). For the AE bridge, Tier C is optional because CPU fallback exists. |
| Magic Bullet / Universe | Per-host tool subsets: AE all tools, Premiere a subset, Resolve and FCP builds ([Web: Red Giant 2026.2.0](https://support.maxon.net/hc/en-us/articles/24114684657692-Red-Giant-2026-2-0-December-3-2025), [Web: Universe 2026.0.0](https://support.maxon.net/hc/en-us/articles/23369159655708-Universe-2026-0-0-September-10-2025)). Looks can use OpenGL, OpenCL or CUDA. Universe does not support VMs or multi-GPU ([Web: Red Giant requirements](https://maxon.net/requirements/red-giant-requirements)). Universe dashboard ([Web: No Film School, Universe 3](https://nofilmschool.com/Red-Giant-Universe-3-Updates)). All via search. | UI-capable helper (R6). The same physical GPU as BAE (R11). Vendors already maintain non-AE builds, so outreach is plausible (L-34). |
| Deep Glow | Tagged "GPU accelerated, 32BPC, MFR", and a GPU is required ([Web: aescripts Deep Glow](https://aescripts.com/deep-glow/), via search). | Tier A/B candidate if its GPU use is internal (inf). |
| Saber | Free. MFR update v1.0.40 ([Web: Video Copilot AE 2022 update](https://videocopilot.net/products/ae22)). Masks and text as the core source (inf). | Path/mask access ([SDK: Effect Details › Working With Paths](https://ae-plugins.docsforadobe.dev/effect-details/working-with-paths/)); text outlines via AEGP (inf). A good free test subject (L-14). |
| Twitch | The key file must sit directly in AE's Plug-ins folder ([Web: Video Copilot plug-in support](https://www.videocopilot.net/support/category/plugins), via search). | Load in place and report true paths (E12). The licence is tied to AE's folder layout. |
| Duik | An ExtendScript tool, not an effect ([SDK: Introduction › Other Integration Possibilities](https://ae-plugins.docsforadobe.dev/intro/other-integration-possibilities/)). | Out of scope. It goes through BAE's script API (ARCHITECTURE §14). |

### 16.4 Cross-cutting expectations and host obligations

| # | Plug-in expects / does | Host MUST / SHOULD / MAY | BAE mapping | P3 risk |
|---|---|---|---|---|
| E1 | Reads `appl_id` and the effect API `version` and branches on them. Some plug-ins also run a script for exact AE version checks (`AEGP_ExecuteScript`) ([SDK: Introduction › Compatibility Across Multiple Versions?](https://ae-plugins.docsforadobe.dev/intro/compatibility-across-multiple-versions/)). | The host MUST report one coherent identity and API version, consistent with the suites it really implements. Choosing that identity is gated on legal review (L-12). The host SHOULD answer script-execution requests with an explicit "unsupported" error. | 20 (P3 host config). Records which identity was presented. | Reporting an honest BAE id may drive plug-ins to a legacy or refusal path. Reporting AE's id raises legal questions. |
| E2 | Uses SmartFX to get 32 bpc. In AE, SmartFX is "the way" to implement 32 bpc ([SDK: SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). | The host MUST implement SmartFX pre-render/render with checkouts, plus 8/16/32 bpc worlds. | 04 ROI negotiation; CPU node conversion | Without SmartFX most modern plug-ins degrade to 8 bpc or refuse (inf). |
| E3 | Thread-safe plug-ins opt into MFR. Non-safe ones get serialised entry, and AE shows a warning icon in the ECW. Sequence data is read-only during render unless the "slower" opt-in flag is set. A compute cache is available ([SDK: Effect Details › Multi-Frame Rendering in AE](https://ae-plugins.docsforadobe.dev/effect-details/multi-frame-rendering-in-ae/), [SDK: What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | The host MUST serialise non-MFR modules (crate `Concurrency::SerialPerModule`). It SHOULD badge them like AE does. It MUST give per-thread sequence-data copies for the mutable opt-in. | 04 scheduler; 11/12 badge | Plug-ins with their own OpenGL contexts may be MFR-flagged yet scale poorly (Element 3D reports). Concurrency caps per module may be needed (inf). |
| E4 | Since AE 13.5 the render thread and UI thread are separate, and selectors arrive on the matching thread ([SDK: What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | The host MUST keep UI-class selectors (sequence setup, user-changed param, dialog, draw) on one designated UI thread per module. Render selectors go elsewhere. | 19 lifecycle threading contract | Out-of-process mode must reproduce the two-thread model in the helper. |
| E5 | GPU path: `PF_Cmd_GPU_DEVICE_SETUP` / `SMART_RENDER_GPU` with host-chosen framework and device. Frameworks seen are CUDA, Metal, OpenCL and DirectX. DirectX effects load shader assets at runtime from a folder shipped next to the binary. Match names using the Adobe prefix are excluded from GPU rendering ([SDK: Effect Basics › Command Selectors](https://ae-plugins.docsforadobe.dev/effect-basics/command-selectors/), [SDK: Introduction › GPU-Accelerated Effect Build Instructions](https://ae-plugins.docsforadobe.dev/intro/gpu-build-instructions/), [SDK: What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | The host MAY start CPU-only: effects that reject or never get a GPU device fall back to CPU render. To offer GPU, it MUST hand over a native device and buffers in the framework the plug-in accepts. | 01 native handles; 04 GPU nodes | CUDA is unavailable on macOS. Cross-process GPU sharing needs exportable textures (R11). |
| E6 | Plug-in-owned GPU contexts (OpenGL, OpenCL, CUDA) are used internally, with CPU worlds as input and output. Adobe published a sample on correct OpenGL context management ([SDK: What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). | The helper process MUST have normal access to the GPU driver: no headless or VM-only sandbox, and the same physical adapter as BAE where possible. | 01 adapter identity | OpenGL is deprecated on macOS. Driver state is shared with BAE's wgpu device when in-process (inf). |
| E7 | Custom ECW and comp-panel UI through Drawbot and events. Drawing happens only during the draw event ([SDK: Effect UI & Events › Custom UI & Drawbot](https://ae-plugins.docsforadobe.dev/effect-ui-events/custom-ui-and-drawbot/)). | The host MUST provide a Drawbot-equivalent surface in the ECW and comp viewer. Out of process, it SHOULD use a record/replay display list. | 11/12 custom-draw region + overlay; 19 | Interactive latency across the process boundary. |
| E8 | Modal dialogs and floating windows: options dialogs, Designer/Looks, licence activation. They get the host main window through `PF_GET_PLATFORM_DATA` ([SDK: Effect UI & Events › UI Callbacks](https://ae-plugins.docsforadobe.dev/effect-ui-events/ui-callbacks/), [SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). | The host MUST provide an owner-window handle and a modal-session model. A sandbox helper MUST be able to show native windows. | 19 out-of-process UI (R6); 11/12 | macOS cannot parent windows across processes (inf). The dialog will be a separate top-level window. |
| E9 | Arbitrary-data params with flatten/unflatten, interpolation callbacks and re-entrancy ([SDK: Effect Details › Arbitrary Data Parameters](https://ae-plugins.docsforadobe.dev/effect-details/arbitrary-data-parameters/)). | The host MUST store arb blobs verbatim and interpolate them by calling the plug-in. | 02 binary param | Interpolation calls cross the process boundary on every keyframe evaluation. |
| E10 | AEGP from effects: own layer and effect refs, camera and lights, comp parent and background colour. The guide warns that AE does **not** track these dependencies; effects must cache the query in sequence data and force a re-render ([SDK: AEGPs › Cheating Effect Usage of AEGP Suites](https://ae-plugins.docsforadobe.dev/aegps/cheating-effect-usage-of-aegp-suites/), [SDK: Effect Details › Accessing Camera & Light Information](https://ae-plugins.docsforadobe.dev/effect-details/accessing-camera-light-information/)). | The host MUST offer synchronous comp-scene reads at arbitrary times: layer list with names and types, transforms, camera, lights. It SHOULD track those reads for cache invalidation, which would be better than AE. | 02 Stage read; 04 deps (R4) | Particular finds its lights by layer-name prefix, so every rename can change the output. |
| E11 | Licensing: trial watermark flag, which can change dynamically. The guide warns of cache races if state changes between query and render ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/)). Plug-ins also read host `serial_num` and the user's registration info (`PF_GetPersonalInfo`) ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/), [SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). | The host MUST fold the watermark state into cache keys and export pre-flight (R3). It MUST return non-identifying values for serial and personal info unless the user consents (L-41). | 04, 11/12 | Activation daemons, network checks and offline grace periods (Maxon App) need network and local IPC from the helper (R9). |
| E12 | Install paths and resources: key files and assets sit next to the binary (Twitch key, DirectX assets). `PF_GET_PLATFORM_DATA` reports the executable and resource paths ([SDK: Effect UI & Events › UI Callbacks](https://ae-plugins.docsforadobe.dev/effect-ui-events/ui-callbacks/)). | The host MUST load binaries **in place** (never copy them to a cache) and report their true paths. It SHOULD use user-registered folders, EDIUS-style, rather than silently scanning Adobe folders. | 19 registry | Adobe's own `.aex` files share those folders (L-30). |
| E13 | Lifecycle: AE never unloads SmartFX plug-ins ([SDK: SmartFX](https://ae-plugins.docsforadobe.dev/smartfx/smartfx/)). Premiere caches capability results across launches, and plug-ins can opt out of that cache ([SDK: Premiere Pro & Other Hosts › Plug-Ins... Reloaded](https://ae-plugins.docsforadobe.dev/ppro/plug-ins-reloaded/)). | The host MUST NOT unload a module during a session (R8). It MAY cache PiPL/global-setup results keyed by binary digest. | 19 | Restarting a helper re-runs global setup, which may re-trigger licence checks. |
| E14 | Binaries: universal macOS bundles and native ARM64 Windows `.aex`. They must be signed on macOS 15+ ([SDK: Introduction › Apple Silicon Support](https://ae-plugins.docsforadobe.dev/intro/apple-silicon-support/), [SDK: Introduction › Debugging Plug-ins](https://ae-plugins.docsforadobe.dev/intro/debugging-plug-ins/)). | The host MUST run foreign-arch slices only out of process. The macOS translated path ends with Rosetta's general support after macOS 27 ([Web: Apple developer news on Rosetta](https://developer.apple.com/news/?id=w5ngl9k2)). | 01 helpers (R10) | Intel-only plug-ins are expected to stop working on macOS from version 28 on. |

### 16.5 Compatibility tiers (prioritised)

The usefulness shares are **inference**: rough guesses about third-party effect instances in typical motion-design projects. They must be checked against a survey or an opt-in project scan before planning P3 (open question Q5). Each tier includes the ones above it. Cumulative shares follow the table order, not the recommended build order below.

| Tier | Plug-ins in it | Host must have (in addition to lower tiers) | Example products | Est. cumulative usefulness | P3 cost / risk |
|---|---|---|---|---|---|
| **A: Self-contained CPU** | Standard params only. Classic `PF_Cmd_RENDER` or SmartFX on CPU, 8/16/32 bpc. Any GPU use is internal, with CPU worlds in and out. No AEGP, no custom UI. | PiPL scan; core selectors; param model and supervision; sequence data with flatten; T0/T1 suites (iterate, fill, copy, sampling, handles, world, pixel format); SmartFX checkouts; serialised non-MFR scheduling; out-of-process frames in shared memory. | Simple colour or blur tools, Heat Distortion (GPU not required), CPU-mode Sapphire/BCC effects (inf) | ~15–25% | Lowest. Mostly the 04 CPU-node round trip. |
| **B: Interactive / stateful** | Tier A plus custom ECW and comp overlays (Drawbot, events), arb data, options dialogs and floating windows, licence-activation UIs, MFR with private sequence data or the compute cache. | Drawbot-equivalent surfaces; event routing; modal sessions and owner window; arb-data interpolation callbacks; watermark in cache key; helper network and IPC access. | Deep Glow, Saber, Twitch, Optical Flares (UI part), Magic Bullet Looks (UI) (inf) | ~40–55% | UI across processes; licensing behaviour in a non-AE host. |
| **C: Host GPU** | Tier B plus GPU device setup and smart-render-GPU on host-provided Metal / CUDA / OpenCL / DirectX buffers. | Native device handles; exportable textures or buffers for cross-process zero-copy; per-device setup and teardown; the same physical adapter. | Sapphire / Continuum GPU paths, Universe (inf) | ~60–70% | Per-framework interop. CUDA only on Windows/Linux. Many such suites also ship OFX builds (16.2). |
| **D: Scene-aware / own 3D** | Tier C plus AEGP from effects: camera, lights, layer enumeration by name, comp settings, streams/keyframes, script execution. Also renderers with their own OpenGL contexts. | Comp-scene read API at arbitrary times with dependency tracking; a camera/light model matching AE semantics; matrix conventions; tolerance of exclusive GL contexts (concurrency cap). | Trapcode Particular / Form, Element 3D, Plexus, Optical Flares with light tracking (inf) | ~85–90% | Highest. Wide AEGP surface, AE-exact 3D semantics, and these are the most AE-locked vendors. |
| **Never (out of scope)** | AEGP-only tools, AEIO, Artisans, BlitHook ([SDK: Introduction › What Can I Do With This SDK?](https://ae-plugins.docsforadobe.dev/intro/what-can-i-do/)) and ExtendScript tools (Duik). | n/a. Scripts go through BAE's script API (ARCHITECTURE §14). | Duik, panels | — | — |

Recommendation (inference): reserve everything Tier D needs in the core now, because those are the reservations that are hard to add later (R4, R6, R11). Implement the tiers in order A → B → D, with C last, unless the vendor suites in C lack OFX builds.

### 16.6 Host behaviours plug-ins rely on beyond the API

These come from Adobe's own list of differences between its two hosts. Plug-ins that only ever ran in AE will rely on AE's side of each difference (inference).

| Behaviour | AE behaviour (what plug-ins assume) | Source | Host obligation | BAE mapping |
|---|---|---|---|---|
| Time values | Plug-ins must use the *ratio* of time fields, not a hard-coded scale. Premiere already uses different scales. | [SDK: Premiere Pro & Other Hosts › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/) | The host MUST give an exact ratio from flicks. It SHOULD pick AE-like scales for common rates, to protect plug-ins that hard-code them anyway (inf). | crate `time.rs`, 04 |
| Render requests | AE renders on demand. Premiere adds low-then-high-quality pairs, speculative look-ahead and field-by-field renders. | same | The host MAY prefetch, but SHOULD NOT send field renders. It MUST treat a quality change as a new request. | 04 scheduler |
| Quality field | In AE, quality is the *layer* quality setting. In Premiere it is the monitor quality. | same | The host MUST map it from the layer quality switch, not the viewer resolution. | 02 layer property, 04 |
| Pixel aspect / frame size | AE passes the layer's real size and PAR, unlike Premiere's adjusted values. | same | The host MUST pass true layer dimensions plus PAR. | 04 |
| Param UI refresh | Premiere skips UI-update and draw calls when the time needle moves on an un-keyframed effect, unless non-param-vary is set. AE does not skip them. | same | The host SHOULD send UI updates on time change for effects that draw time-dependent UI (histograms). | 11/12 |
| Twirl state | AE honours start-collapsed groups; Premiere does not. | same | The host SHOULD honour it. | 11/12 |
| Missing suites | Premiere gives macro equivalents for some missing suites, and plug-ins check host and version to choose. | same | Any suite the host advertises MUST be complete. A partially implemented suite is worse than an absent one (inf). | crate `suites.rs` |
| Instances and threads | AE: multiple instances, shared globals, per-sequence data. Premiere: per-instance critical sections, but concurrent instances. | [SDK: Introduction › What Can I Do With This SDK?](https://ae-plugins.docsforadobe.dev/intro/what-can-i-do/), [SDK: Premiere Pro & Other Hosts › Basic Host Differences](https://ae-plugins.docsforadobe.dev/ppro/basic-host-differences/) | The host MUST serialise per module unless MFR is declared (E3). | 04, 19 |
| Sequence-data cloning | A Premiere-only opt-out of cloning sequence data for render "causes parameter UI problems". | [SDK: Premiere Pro & Other Hosts › Multithreading](https://ae-plugins.docsforadobe.dev/ppro/multithreading/) | The host SHOULD always clone or flatten sequence data for render threads, as AE does. | crate `sequence.rs` |
| Presets | Premiere uses its own preset format; AE has its own. | [SDK: Premiere Pro & Other Hosts › Plug-Ins... Reloaded](https://ae-plugins.docsforadobe.dev/ppro/plug-ins-reloaded/) | Vendor preset files (`.ffx`) are AE project fragments, so importing them is 15's job (inf). | 15 |
| Icons / previews | Premiere Elements expects per-effect preview icons named after the match name. Newer PiPLs add search keywords, a description and preview media (Premiere only). | [SDK: Premiere Pro & Other Hosts › Premiere Elements](https://ae-plugins.docsforadobe.dev/ppro/premiere-elements/), [SDK: Version History](https://ae-plugins.docsforadobe.dev/history/) | The host MAY show PiPL description and keywords in the effect browser. | 11/12 |

### 16.7 P3 risk register (ecosystem)

| Risk | Likelihood (inf) | Impact | Mitigation now (P1) | Mitigation in P3 |
|---|---|---|---|---|
| Top AE-only plug-ins refuse a non-AE `appl_id` | medium | Tier B–D unusable | Keep the identity a configurable policy in the crate; legal item L-12 | Test with vendor consent; vendor outreach (L-34) |
| Licence systems break under sandboxing or restarts | high | User loses activations | R9 permissions; R8 lifecycle contract | Licence-aware restart; one helper per vendor |
| Scene-aware plug-ins render stale frames | high | Wrong output, silently | R4 read-dependency recording | Fall back to a coarse "scene dirty" invalidation |
| GPU-context plug-ins crash next to wgpu | medium | App crash | Out-of-process by default (crate `process.rs`) | Per-module concurrency caps; blocklist |
| Intel-only plug-ins on macOS | certain after macOS 27 | Plug-ins lost | Do not invest in Rosetta beyond a flag | Show "needs native build" in UI |
| Collaborators render different implementations | high in teams | Divergent output | R2 provider pinning; R3 provenance in cache keys | Badge mismatches (R12) |
| Adobe first-party binaries get loaded | medium if folders are auto-scanned | Legal | `resolve.rs` denylist (crate discrepancy) | Explicit user-registered folders only |
| Vendors move to OFX, so AE bridge value falls | medium | Wasted effort | Keep the bridge optional and isolated | Ship an OFX host first (16.2) |

### 16.8 Reservations derived from this section

Each must exist in the core **now**. The structured result carries the full semantics.

| ID | To | Reservation (short) | Driven by |
|---|---|---|---|
| R1 | 02 | Foreign effect identity: an effect type id namespace for AE match names (opaque, byte-exact), plus verbatim retention of unknown params, arb blobs and flat state | E9, 16.2 |
| R2 | 02 | Per-instance provider preference persisted in the doc (`auto` / `native` / `bae-plugin:<id>` / `original`), with a project-level default | 16.7 collaboration risk |
| R3 | 04 | Provenance in cache keys (provider kind, binary digest, effect version, output-quality state) and an export pre-flight query | E11, L-47 |
| R4 | 02 + 04 | Recording Stage-read context: scene reads during node evaluation become the node's dynamic dependency set | E10, Particular |
| R5 | 04 | Per-node failure isolation: the node renders a placeholder with a status, and the graph keeps going | 16.1 blacklisting, EDIUS |
| R6 | 19 | UI-capable out-of-process helpers: owner-window token, modal sessions, helper UI thread | E4, E8 |
| R7 | 19 | Trust registry: binary digest, signer and arch slices; no load without consent | L-43, E14 |
| R8 | 19 | Module pinning (never unloaded) and a re-initialisation contract (global init may re-run in a fresh process) | E13 |
| R9 | 19 | Manifest permissions for network, local IPC and file system, inherited by helpers | E11, L-42 |
| R10 | 01 | Helper executables with separate signing and entitlements; universal and ARM64EC/x64 helpers | E14, L-36 |
| R11 | 01 | GPU adapter identity and exportable textures/buffers for cross-process zero-copy | E5, E6 |
| R12 | 11/12 | Effect status badges (watermarked, not MFR-safe, failed, provider differs) and an Options/About entry | E3, E8, E11 |
| R13 | 15 | Import keeps the original AE identity (match name, version, raw streams) even when a native effect is mapped; Adobe-prefixed effects never route to a binary | L-30 |

### 16.9 Notes on the crate skeleton (`resolve.rs`, `process.rs`)

- `resolve()` is static and machine-local. It cannot express a runtime refusal (plug-in rejects the host, lacks a licence, or crashes during global setup). It ignores effect versions and duplicate installs. Its global `compat_enabled` flag stands in for per-binary consent. It does not exclude Adobe first-party effects. See crate_discrepancies.
- `process.rs` has no UI or windowing channel (E8). It does not specify a network or IPC policy for helpers (E11). It treats Rosetta translation as durable (E14). Its restart policy re-runs global setup without considering licensing. It encodes the plug-in success code numerically in a doc comment, against crate rule 3.

### 16.10 Host obligation summary (ecosystem view)

1. The host MUST report one consistent host identity and API version, consistent with the suites it really implements. Which identity it reports waits on L-12 (E1).
2. The host MUST answer "absent" for any suite it does not fully implement, and never advertise a partial suite (16.1, 16.6).
3. The host MUST implement SmartFX with 8/16/32 bpc worlds before claiming support for any commercial plug-in (E2).
4. The host MUST serialise modules that are not MFR-safe and SHOULD badge them as AE does (E3).
5. The host MUST keep a UI thread separate from render threads per module, including inside helpers (E4).
6. The host MAY start CPU-only: a GPU-capable effect then falls back to its CPU path (E5).
7. The host MUST give helpers ordinary GPU-driver access on the same adapter as BAE (E6, R11).
8. The host MUST provide Drawbot-equivalent drawing for the ECW and comp overlays (E7).
9. The host MUST support plug-in dialogs and windows: an owner window and modal sessions (E8, R6).
10. The host MUST store arb data and flat sequence data verbatim, even when the plug-in is missing (E9, R1).
11. The host MUST support comp-scene reads at arbitrary times and track them for invalidation (E10, R4).
12. The host MUST fold the watermark/licence state into cache keys and gate export on it (E11, R3).
13. The host MUST NOT pass personal or registration data to plug-ins without consent (E11, L-41).
14. The host MUST load binaries in place, from user-registered folders, and never load Adobe first-party binaries (E12, L-30).
15. The host MUST NOT unload modules during a session, and MUST re-run global setup after a helper restart in a licence-aware way (E13, R8).
16. The host MUST NOT load an unapproved binary just because a project references it (L-43, R7).
17. The host MUST NOT modify or re-sign plug-in files (L-37).

## 21. Legal and compliance checklist (flag, do not conclude)

This is a checklist of questions to answer **before** any P3 work that loads, links or imitates the AE effect ABI. It records no conclusions. Owners: **LC** = legal counsel (US / EU / PRC as noted), **ENG** = engineering, **PM** = product and brand, **SEC** = security, **REL** = release engineering. Items are numbered L-xx so other documents can reference them.

### 21.1 Adobe SDK and developer terms

| ID | Question | Why it matters | Who | Evidence to collect |
|---|---|---|---|---|
| L-01 | Which terms govern the AE SDK today? Is it the Adobe Developer Terms of Use (the 2023 version supersedes the older "Additional Terms"), a licence file bundled in the SDK, or both? | Every other SDK question depends on it. | LC | The exact terms shown at SDK download, any bundled licence text, and the version and date. Public copy: [Web: Adobe Developer Terms of Use, 22 Aug 2023](https://wwwimages2.adobe.com/content/dam/cc/en/legal/servicetou/Adobe-Developer-Additional-Terms_en-US_20230822.pdf). |
| L-02 | Does the licence allow using headers or samples to build a **host**? The 2023 terms define "Developer Tools" to include SDK files, headers and sample code, provided for interoperating with or adding to *Adobe* software. The licence covers developing "Developer Software", defined as software meant to work with Adobe software or services (§2.22, §2.23, §4.1). | A host for third-party plug-ins is probably not "Developer Software". If so, using the headers to build one may fall outside the licence entirely. | LC | Section-by-section reading of §2, §4 and §6; whether the AE SDK carries extra terms. |
| L-03 | Can any SDK material be redistributed? The terms allow distribution only "in object code form" inside *approved* Developer Software (§4.1), forbid unbundling (§6.3) and forbid sublicensing (§6.8). An answer marked correct in an Adobe community thread says plug-ins may be distributed, but not the headers or sources themselves. | BAE is open source (Apache-2.0), so anything that ships is source. | LC | [Web: Adobe community thread on the AE SDK licence](https://community.adobe.com/t5/after-effects-discussions/after-effects-sdk-license/m-p/14348525/highlight/true). The answering poster is labelled "community Legend", and his name matches the long-time SDK documentor in [SDK: Version History](https://ae-plugins.docsforadobe.dev/history/). |
| L-04 | §6.1 bans reverse engineering, explicitly including monitoring inputs and outputs to recreate a system. It says any statutory interoperability right may be used only **after asking Adobe first**, and Adobe may set conditions and fees. Should BAE send that request? Does it bind people who accepted the terms? | It decides who may work on P3, and whether observing plug-in ↔ AE traffic is allowed. | LC | Which contributors have accepted the Developer Terms (Adobe ID developer accounts). A draft interoperability-information request to Adobe. Adobe's public stance: [SDK: Premiere Pro & Other Hosts › Other Hosts](https://ae-plugins.docsforadobe.dev/ppro/other-hosts/). |
| L-05 | §6.6 forbids combining Developer Tools with open-source software in a way that would force them into source form. Does the Apache-2.0 BAE repository create exposure if any SDK-derived artefact enters it? | Repository hygiene. CI must block header contents. | LC, ENG | A CI scan for SDK identifiers inside code (only allowed in docs). Contributor attestations. |
| L-06 | §6.12 includes a non-assert of the developer's IP against Adobe. Does accepting the terms (for example, to test BAE's own AE-side plug-ins) affect BAE's own IP? | Side effects on BAE's patents and copyright. | LC | List the BAE work that needs Adobe developer accounts (15 importer tests, Ai/AE bridge plug-ins). |
| L-07 | The guide used as the primary source is community-hosted, "for educational purposes only", and its content is © Adobe. Is paraphrasing it into `docs/spec/aex-compat.md` and the crate docs acceptable? Should quoting be limited further? | In SAS v WPL the CJEU said reproducing elements of a *manual* can infringe if they are the author's own intellectual creation. | LC | The repo's readme statement (local clone `readme.md`, "Licensing & Ownership"). [Web: Out-Law on SAS v WPL](https://www.pinsentmasons.com/out-law/news/computer-program-functionality-not-copyrightable-but-programming-languages-and-file-formats-may-be-protected-by-the-copyright-directive-ecj-rules). A diff check proving the spec has no long passages. |

### 21.2 Clean-room process

| ID | Question | Why it matters | Who | Evidence to collect |
|---|---|---|---|---|
| L-10 | Team partition: who may read what? A "spec team" reads only the public guide and vendor docs and writes a BAE-vocabulary spec. An "implementation team" reads only that spec. Nobody on either team has read Adobe headers or SDK samples, or accepted the Developer Terms (L-04). | A clean-room record is the main defence against copying claims. | LC, ENG | Written roles, reading logs, attestations, separate repos or branches for the spec. |
| L-11 | Where do **numeric ABI values** (selector numbers, flag bits, struct layouts, error codes, suite names and versions) come from? The guide shows struct shapes but not every value. Options: (a) headers, which hits L-02/L-03; (b) observing licensed plug-in binaries, which is reverse engineering and depends on L-04 and L-15/L-16/L-18; (c) asking Adobe, as §6.1 requires before decompiling; (d) a third party's clean documentation. | The P3 FFI shim cannot work without them. The crate deliberately holds none (crate rule 3). | LC, ENG | A per-value provenance log (value → source → who obtained it). Counsel's opinion on each option per jurisdiction. |
| L-12 | Host identity: may BAE report AE's creator code in `appl_id`, or an AE-like API version, so that plug-ins take their AE code path? The guide presents `appl_id` as a licensing check ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/)). | Possible issues: impersonation or passing off, breach of plug-in EULAs, and circumvention (L-17). The alternative, an honest BAE identity, may make plug-ins refuse to run. | LC, PM | A survey of how top plug-ins behave under an unknown `appl_id` (ENG test with vendor permission). Vendor statements. |
| L-13 | Are short identifiers (`PF_*`, `AEGP_*` names) used as documentation cross-references acceptable? Should BAE's public API avoid them entirely? | Google v. Oracle held reimplementing declaring code was fair use on those facts. It assumed, without deciding, that the code was copyrightable. | LC | [Web: US Copyright Office summary, Google v. Oracle (2021)](https://www.copyright.gov/fair-use/summaries/google-llc-oracle-am-inc-2021.pdf). An inventory of AE identifiers in BAE docs and code. |
| L-14 | Test corpus: may BAE test with commercially licensed plug-ins? Do trial licences permit testing in a non-Adobe host? Free plug-ins (e.g. Saber) are candidates. | Testing is needed for any compatibility claim (L-20). | LC, ENG | The EULA of each test plug-in (L-31). Written vendor permission where possible. |

### 21.3 Copyright and interoperability by jurisdiction

| ID | Question | Why it matters | Who | Evidence to collect |
|---|---|---|---|---|
| L-15 | **US**: does Google v. Oracle's fair-use reasoning cover a reimplemented *host side* of a plug-in API, or does it stay limited to its facts? Also check older interoperability cases (e.g. Sega v. Accolade, Sony v. Connectix; not researched here). | Copyright exposure for the ABI and its names. | LC (US) | [Web: Copyright Office summary](https://www.copyright.gov/fair-use/summaries/google-llc-oracle-am-inc-2021.pdf), [Web: CRS legal sidebar](https://www.congress.gov/crs_external_products/LSB/PDF/LSB10597/LSB10597.1.pdf). |
| L-16 | **EU**: does Directive 2009/24/EC Art. 6 (decompilation where indispensable for interoperability of an *independently created* program, with limits on how the information is used) apply? Does Art. 5(3) (observing, studying and testing while using a program) apply? Does Art. 8 void contract clauses that override them (and so the terms' §6.1 "ask Adobe first")? SAS v WPL (C-406/10) held that functionality, programming languages and data file formats are not protected as program expression. | EU contributors and users. Adobe's terms are under Irish law outside North America (§1.2). | LC (EU) | [Web: Directive 2009/24/EC Art. 6](https://www.legislation.gov.uk/eudr/2009/24/article/6), [Web: EUR-Lex 2009/24/EC](https://eur-lex.europa.eu/eli/dir/2009/24), [Web: Wikipedia, SAS v WPL](https://en.wikipedia.org/wiki/SAS_Institute_Inc_v_World_Programming_Ltd). Does Art. 6(2)(c) ("substantially similar" program) affect a *host*? |
| L-17 | **US DMCA §1201**: if a plug-in's host check or licence check counts as a technological protection measure, would presenting a spoofed identity be circumvention? Does §1201(f) (reverse engineering for interoperability by a lawful user) cover it? | It ties L-12 to statutory liability. | LC (US) | [Web: 17 U.S.C. §1201 (Cornell LII)](https://www.law.cornell.edu/uscode/text/17/1201). An analysis of whether `appl_id` checks "control access". |
| L-18 | **PRC**: Regulations on Computer Software Protection (2013 rev.) Art. 6 (no protection for ideas, processes, operating methods) and Art. 17 (use for studying design ideas and principles by installing, displaying, transmitting or storing, without permission). Is there an explicit decompile-for-interoperability exception? How do the Anti-Unfair-Competition Law and trade secrets (reverse engineering) interact? (Not researched here.) | Main team and users are in China. | LC (PRC) | [Web: State Council text, 2013 revision](https://www.gov.cn/zhengce/2013-02/08/content_2602615.htm). Relevant SPC judicial interpretations. |
| L-19 | Which law governs: Adobe's terms (California / Ireland, §1.2), each plug-in EULA, and the users' location? | Conflicts of law across the items above. | LC | A matrix of contributors, users and vendors. |

### 21.4 Trademarks and naming

| ID | Question | Why it matters | Who | Evidence to collect |
|---|---|---|---|---|
| L-20 | Is "compatible with Adobe After Effects plug-ins" (or similar) allowed? Adobe's guidelines allow referential phrases ("for use with", "compatible with") only if they are **true** and not confusing, require the full product name, forbid abbreviations, and require an attribution statement. | Marketing copy, UI strings ("AE 插件"), docs. | LC, PM | [Web: Adobe trademark guidelines](https://www.adobe.com/legal/permissions/trademarks.html). A tested-plug-in list to back any claim (16.2 lesson). |
| L-21 | The repository and codename are **"betterAfterEffect"**. Adobe's guidelines forbid Adobe marks in product, company or domain names, and names "confusingly similar" to them. Does the internal codename, repo name or any public artefact need renaming before release? | Brand risk that is independent of P3. | LC, PM | Every public use of the name (repo, crates, binaries, docs). The `bae-` crate prefix is neutral. |
| L-22 | Should the UI use the term "AE" at all? The guidelines say marks should not be abbreviated. Can `.aex` be shown to users? (It is a file extension; inference.) | i18n strings in 11/12. | LC, PM | String inventory. |
| L-23 | Must Adobe icons, logos and the "Adobe Clean" UI font be avoided in plug-in hosting UI? The guide says that font is proprietary and not licensed for plug-in UIs ([SDK: What's New](https://ae-plugins.docsforadobe.dev/intro/whats-new/)). AGENTS.md already bans copying Adobe icons. | Drawbot text drawing and font fallbacks. | ENG, PM | The font-substitution plan for `PF_GetFontStyleSheet`-style queries ([SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)). |
| L-24 | BAE will show third-party product names (e.g. "Particular", "Element 3D") in the effect browser and in compatibility lists. Is that permissible nominative use? Is a disclaimer needed that vendors do not endorse BAE? | Vendor trademarks, not only Adobe's. | LC, PM | Vendor trademark policies; draft compatibility-list wording. |

### 21.5 Patents (flag only)

| ID | Question | Why it matters | Who | Evidence to collect |
|---|---|---|---|---|
| L-25 | Are there Adobe (or third-party) patents on two-phase effect rendering with input region negotiation (SmartFX), multi-frame rendering of plug-in effects with per-thread state, the compute cache, or GPU effect device setup? A cursory web search found none; that is **not** a clearance. | ROI negotiation is also BAE-native (04), so this is not only a compat issue. | LC | A freedom-to-operate search by assignee and claims. Prior-art dates: SmartFX documented in the AE 7.0 SDK (2005) ([SDK: Version History](https://ae-plugins.docsforadobe.dev/history/)); OpenFX ROI action ([Web: OpenFX](https://openfx.readthedocs.io/en/main/Reference/ofxProcessingArch.html)). |
| L-26 | Do any plug-in vendors hold patents that would be implicated by BAE *hosting* (as opposed to running) their effects? | Unlikely but unknown. | LC | Vendor patent notices. |

### 21.6 Third-party plug-in EULAs, activation and anti-tamper

| ID | Question | Why it matters | Who | Evidence to collect |
|---|---|---|---|---|
| L-30 | Should BAE scan or load anything from Adobe's own plug-in folders? Those folders also hold **Adobe's first-party `.aex` effects**, whose use is governed by the AE EULA ([SDK: Introduction › Where Installers Should Put Plug-ins](https://ae-plugins.docsforadobe.dev/intro/where-installers-should-put-plug-ins/)). | Running Adobe's own effects in BAE is a separate and much higher-risk act. | LC, ENG | AE EULA terms on components. A denylist design: Adobe-prefixed match names and the AE install tree (crate discrepancy on `resolve.rs`). |
| L-31 | For each top plug-in, does its EULA limit use to particular host applications, count machines or seats (e.g. Element 3D, two machines, via search), restrict headless or render-only use (Plexus: `aerender` render-only licence), or forbid pre-rendering into stock (Video Copilot, via Storyblocks summary)? | The user may breach their licence, and BAE may be accused of inducing it. | LC | EULA texts: [Web: Boris FX EULA](https://support.borisfx.com/hc/en-us/articles/10006419267597-End-User-License-Agreement) ("cross host" licences), [Web: aescripts Plexus render-only](https://aescripts.com/plexus-3-render-only/), [Web: Toolfarm on VC machine limits](https://www.toolfarm.com/knowledge-base/product-questions/video-copilot-troubleshooting/), [Web: Storyblocks accepted plug-ins](https://contribute-faq.storyblocks.com/en/articles/4543862-what-are-the-accepted-plug-ins-for-after-effects-templates). |
| L-32 | Licence daemons and seats: does one helper process per module (crate `process.rs`), or a helper restart, consume extra floating seats or re-trigger activation? Does BAE headless or NAS rendering count as "render-only" use? | Users' licence compliance and support load. | ENG, LC | Vendor docs (Maxon App offline grace, RLM floating licences: [Web: Toolfarm on Maxon licensing](https://toolfarm.com/tutorial/red-giant-licenses)). Lab tests with vendor consent. |
| L-33 | Anti-tamper and anti-debug: would BAE's sandboxing, shared-memory worlds or syscall filtering trip a plug-in's tamper detection? Could that be construed as tampering? | Users could lose activations. | ENG, LC | Vendor contact. Crash and telemetry from opt-in beta testers. |
| L-34 | Vendor outreach: should BAE seek written permission or a partnership from the key AE-only vendors (Video Copilot, Maxon/Red Giant, Rowbyte, aescripts) before claiming support? | It reduces L-12, L-31 and L-33 risk, and vendors may prefer to ship BAE-native or OFX builds. | PM, LC | Contact log and responses. |
| L-35 | Vendor content packs (preset libraries, 3D model packs, LUTs) loaded through a hosted plug-in, or imported from `.ffx` presets: do their content licences limit use to AE? | Content licences are separate from plug-in EULAs. | LC | Content-licence texts of the main packs. |

### 21.7 Code signing, notarisation and OS policy

| ID | Question | Why it matters | Who | Evidence to collect |
|---|---|---|---|---|
| L-36 | On macOS, the hardened runtime blocks loading code from other Team IDs. Apple's guidance is to disable library validation only when loading third-party plug-ins, and that weakens Gatekeeper assessment. Should only a separate helper carry that entitlement, so the main app keeps library validation? | Notarisation, and the security posture of the main app. | REL, SEC | [Web: Apple, disable-library-validation entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.cs.disable-library-validation). Notarisation test of a helper. |
| L-37 | macOS 15+ refuses unsigned plug-ins ([SDK: Introduction › Debugging Plug-ins](https://ae-plugins.docsforadobe.dev/intro/debugging-plug-ins/)). Should BAE ever ad-hoc sign or re-sign a user's plug-in? | Re-signing modifies a third-party binary, which may be an EULA or anti-tamper issue (L-33). | LC, REL | Policy: never modify plug-in files. |
| L-38 | Rosetta-based hosting of Intel-only plug-ins: is it worth building, given general Rosetta support ends after macOS 27? | Wasted effort; user expectations. | PM, ENG | [Web: Apple developer news on Rosetta](https://developer.apple.com/news/?id=w5ngl9k2). |
| L-39 | Windows: do Smart App Control or WDAC policies block unsigned `.aex` loads in a BAE helper (inference, not researched)? Does an app store distribution channel forbid loading third-party native code? | Distribution channels. | REL | Microsoft docs; store policies. |

### 21.8 User data and privacy

| ID | Question | Why it matters | Who | Evidence to collect |
|---|---|---|---|---|
| L-41 | What does BAE return for host `serial_num` and for registration info (`PF_GetPersonalInfo`: user name, organisation, serial) ([SDK: Effect Basics › PF_InData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_InData/), [SDK: Effect Details › Useful Utility Functions](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/))? | Handing personal data to third-party code is a GDPR / PIPL processing question. The guide jokes about putting it into watermarks. | LC, ENG | Data-flow inventory. Default: non-identifying placeholder values. Consent UX. |
| L-42 | Plug-ins phone home (activation, update checks, telemetry). Should BAE disclose this, allow or block network access per module, and log it? What does PRC PIPL say about cross-border transfer initiated by third-party code running inside BAE? | Privacy notices; enterprise and offline deployments. | LC, SEC | Network-permission design (R9). Vendor privacy policies. |
| L-43 | Opening a shared or collaborative project that references AE plug-ins must not cause BAE to load native code without consent. Is per-binary trust (R7) enough? | A project file becomes a code-execution trigger. | SEC | Threat model. Consent UX (`resolve.rs` discrepancy). |
| L-44 | Crash reports from helpers contain third-party code and memory, which may include user media and licence tokens. May they be uploaded to BAE, or forwarded to vendors? | Privacy, and vendor confidentiality. | LC, SEC | Crash-reporting design; opt-in consent text. |

### 21.9 Liability, security and support

| ID | Question | Why it matters | Who | Evidence to collect |
|---|---|---|---|---|
| L-45 | Who is liable when a hosted plug-in crashes, corrupts a project or renders wrongly? What disclaimers apply (EDIUS: operation "not guaranteed")? | Support and warranty scope. | LC, PM | EULA and ToS draft; precedent wording ([Web: EDIUS 7 manual](https://wwwapps.grassvalley.com/manuals/EDIUS7_USER_EN/l06/l0_6_l1_15_l2_3.html)). |
| L-46 | Is out-of-process hosting a *security* boundary or only a *stability* boundary? Without an OS sandbox, plug-ins keep the user's full rights. An OS sandbox may break licensing (L-32). | How it is described to users and enterprises. | SEC | Threat model; per-platform sandbox feasibility. |
| L-47 | Watermarked or trial output: must export warn or block when any node reports a watermarked state ([SDK: Effect Basics › PF_OutData](https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/))? | Users could ship trial-watermarked deliverables, or BAE could be accused of stripping watermarks. | PM, ENG | Export pre-flight design (R3). |

### 21.10 Open-source licensing of BAE's compat host

| ID | Question | Why it matters | Who | Evidence to collect |
|---|---|---|---|---|
| L-50 | Can the compat host be Apache-2.0 like the rest of BAE? Or should it be a separately licensed or separately distributed optional component (e.g. the downloadable-plug-in pattern ARCHITECTURE §1 uses for FFmpeg)? | It isolates risk and lets the core ship even if P3 is blocked. | LC, PM | Packaging options. ARCHITECTURE §14 says licensing is "not yet decided". |
| L-51 | Contributor provenance: do contributors need a CLA or DCO with an extra attestation that they have not viewed Adobe SDK headers or samples and have not accepted the Developer Terms (for P3 code only)? | It supports the clean-room record (L-10). | LC | Attestation template; CI checks. |
| L-52 | Is the BAE-vocabulary spec itself (`docs/spec/aex-compat.md`, the crate docs) publishable under Apache-2.0, given L-07? | Public docs. | LC | Final spec review against the guide (no long passages, paraphrase only). |

### 21.11 Decision gates (when each item must be answered)

| Gate | Engineering step it unlocks | Items that must be answered first |
|---|---|---|
| G0: now (P1) | BAE-vocabulary docs and the crate skeleton (no ABI values, no headers) | L-07, L-13, L-21, L-52. Keep L-05 hygiene in CI. |
| G1 | Any contributor reads AE SDK headers or samples, or accepts the Developer Terms for BAE work | L-01–L-04, L-06, L-10, L-51 |
| G2 | Obtaining numeric ABI values; building the P3 FFI shim | L-11, L-15–L-19 |
| G3 | Loading a third-party plug-in in a lab (tests) | L-14, L-30, L-31, L-33, L-36, L-37 |
| G4 | Choosing the reported host identity | L-12, L-17, L-34 |
| G5 | Public release of the compat host or any "compatible" claim | L-20, L-22, L-23, L-25, L-32, L-38–L-47, L-50 |

### 21.12 Current P1 practice (facts to hand to counsel, not conclusions)

| Practice | Where | Relevant items |
|---|---|---|
| No Adobe headers, samples or binaries in the repository. Nothing is loaded or linked. | `crates/bae-aex-host/src/lib.rs` (status and rule 3); ARCHITECTURE §15 "合规" | L-03, L-05 |
| No numeric ABI values: selector numbers, flag bits, error codes and suite names are deliberately absent | crate rule 3; this document's authoring rules | L-11 |
| SDK identifiers appear only as cross-references in docs and comments. Types are BAE-named and not layout-compatible. | crate rule 2 | L-13 |
| Facts come from the public guide (community copy, © Adobe), paraphrased and cited per page | this spec | L-07, L-52 |
| The repository is Apache-2.0 | `LICENSE` | L-05, L-50 |
| The working name contains "AfterEffect" | repository and Orca workspace names | L-21 |

### 21.13 Evidence pack to assemble before G1

| Evidence | Owner | Used by |
|---|---|---|
| Archived copies (PDF with date) of the Adobe Developer Terms, the AE SDK download page and any bundled SDK licence | LC | L-01–L-06 |
| Archived copy of the community guide repo readme ("educational purposes only"; © Adobe) and the commit hash used for the spec | ENG | L-07, L-52 |
| Per-contributor attestations: Adobe developer account yes/no; headers or samples read yes/no | LC | L-10, L-51 |
| ABI value provenance log (empty in P1) | ENG | L-11 |
| EULAs of the top 15 plug-ins (from the 16.3 list), with host, seat, render-only and anti-tamper clauses highlighted | LC | L-31–L-33 |
| Vendor correspondence log | PM | L-34 |
| Trademark inventory: every public string containing Adobe marks or abbreviations | PM | L-20–L-23 |
| Freedom-to-operate memo request (ROI negotiation, MFR, GPU effect interop) | LC | L-25, L-26 |
| Threat model for project-triggered native-code loading and helper sandboxing | SEC | L-43, L-46 |
