/*
 * bae_plugin.h - BAE native plugin C ABI.
 *
 * Status: DRAFT v0.1 (design phase). ABI major 0 carries no stability promise;
 * the first frozen ABI is 1.0 (milestone M1). See docs/spec/plugin-architecture.md.
 *
 * This header is hand-written for the design phase. It will later be generated
 * by cbindgen from `bae_plugin_sdk::abi` (crates/bae-plugin-sdk/src/abi.rs);
 * until then this file is authoritative and the Rust mirror is checked against
 * it by a layout test.
 *
 * SPDX-License-Identifier: Apache-2.0
 *
 * -------------------------------------------------------------------------
 * ABI rules (normative; details in plugin-architecture.md section "C ABI")
 * -------------------------------------------------------------------------
 *  R1  64-bit targets only (aarch64/x86_64). Default C calling convention.
 *  R2  Fixed-width integer types only. Enumerations are uint32_t/int32_t
 *      fields plus #define constants (never C `enum` types in structs).
 *  R3  Every extensible struct starts with `uint32_t struct_size`, set by the
 *      producer to sizeof(struct) as the producer knows it. Consumers read a
 *      field only if BAE_HAS_FIELD() says it is present; missing fields are
 *      treated as zero. New fields are only ever appended (minor ABI bump).
 *      For out-structs the HOST sets struct_size before the call and the
 *      plugin must not write past it.
 *  R4  Host suites are acquired by (name, version) and are FROZEN: a released
 *      suite version never changes. New functions mean a new suite version.
 *  R5  Plugin vtables are extensible structs (R3): optional callbacks are
 *      appended; a NULL callback means "use the documented default".
 *  R6  Strings are UTF-8. `const char*` fields are NUL-terminated; BaeStr is
 *      (pointer, length) and not necessarily NUL-terminated.
 *  R7  Ownership: whoever allocates frees. Data crossing the boundary is
 *      either borrowed for the duration of the call, or written through a
 *      host-provided BaeWriter, or allocated with the bae.core allocator.
 *  R8  No unwinding across the boundary. C++ exceptions and Rust panics must
 *      be caught inside the plugin and reported as BAE_ERR_PANIC.
 *  R9  Unknown enum values and unknown flag bits must be tolerated (ignored /
 *      treated as the default), never rejected.
 *  R10 Pointers handed to a callback are valid only until that callback
 *      returns unless stated otherwise.
 */
#ifndef BAE_PLUGIN_H
#define BAE_PLUGIN_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ======================================================================= */
/* 1. Versioning                                                           */
/* ======================================================================= */

#define BAE_ABI_VERSION_MAJOR 0u
#define BAE_ABI_VERSION_MINOR 1u

/* A version is packed as (major << 16) | minor. */
#define BAE_MAKE_VERSION(major, minor) \
    ((uint32_t)((((uint32_t)(major)) << 16) | (((uint32_t)(minor)) & 0xFFFFu)))
#define BAE_VERSION_MAJOR(v) ((uint32_t)(v) >> 16)
#define BAE_VERSION_MINOR(v) ((uint32_t)(v) & 0xFFFFu)
#define BAE_ABI_VERSION BAE_MAKE_VERSION(BAE_ABI_VERSION_MAJOR, BAE_ABI_VERSION_MINOR)

/* True if an extensible struct (rule R3) carries `field`. */
#define BAE_HAS_FIELD(ptr, type, field) \
    ((size_t)(ptr)->struct_size >= offsetof(type, field) + sizeof(((type *)0)->field))

/* ======================================================================= */
/* 2. Platform macros                                                      */
/* ======================================================================= */

#if defined(_WIN32)
#define BAE_EXPORT __declspec(dllexport)
#else
#define BAE_EXPORT __attribute__((visibility("default")))
#endif

#if defined(__cplusplus)
#define BAE_STATIC_ASSERT(cond, msg) static_assert(cond, msg)
#else
#define BAE_STATIC_ASSERT(cond, msg) _Static_assert(cond, msg)
#endif

/* ======================================================================= */
/* 3. Status codes                                                         */
/* ======================================================================= */

typedef int32_t BaeStatus;

#define BAE_OK 0
/* Informational (> 0). */
#define BAE_STATUS_FALLBACK 1 /* render_gpu: do this frame on the CPU instead */
/* Errors (< 0). Attach a message with BaeCoreSuite1.set_error. */
#define BAE_ERR_GENERIC (-1)
#define BAE_ERR_INVALID_ARGUMENT (-2)
#define BAE_ERR_OUT_OF_MEMORY (-3)
#define BAE_ERR_UNSUPPORTED (-4)
#define BAE_ERR_VERSION_MISMATCH (-5)
#define BAE_ERR_NOT_FOUND (-6)
#define BAE_ERR_ABORTED (-7) /* host asked to abort (is_aborted() returned true) */
#define BAE_ERR_BAD_STATE (-8)
#define BAE_ERR_PERMISSION_DENIED (-9)
#define BAE_ERR_BUFFER_TOO_SMALL (-10)
#define BAE_ERR_IO (-11)
#define BAE_ERR_PARSE (-12) /* malformed input, e.g. an unreadable state blob */
#define BAE_ERR_GPU_DEVICE_LOST (-13)
#define BAE_ERR_GPU (-14)
#define BAE_ERR_PANIC (-15) /* a panic/exception was caught at the boundary */
#define BAE_ERR_TIMEOUT (-16)
#define BAE_ERR_INTERNAL (-17)

/* ======================================================================= */
/* 4. Basic types                                                          */
/* ======================================================================= */

typedef uint32_t BaeBool;
#define BAE_FALSE 0u
#define BAE_TRUE 1u

/* Time in flicks: 1 second = 705,600,000 flicks (exact for all common rates). */
typedef int64_t BaeFlicks;
#define BAE_FLICKS_PER_SECOND INT64_C(705600000)

/* den > 0. */
typedef struct BaeRational {
    int32_t num;
    int32_t den;
} BaeRational;

/*
 * Half-open pixel rectangle [x0, x1) x [y0, y1). x grows right, y grows down.
 * Unless stated otherwise coordinates are in the layer's *render pixel space*:
 * full-resolution layer pixels multiplied by the request's downsample factor.
 * Empty when x1 <= x0 or y1 <= y0.
 */
typedef struct BaeRect {
    int32_t x0;
    int32_t y0;
    int32_t x1;
    int32_t y1;
} BaeRect;

/* Bounds used to express "unbounded" extents (e.g. infinite generators). */
#define BAE_COORD_MIN (-(INT32_C(1) << 30))
#define BAE_COORD_MAX (INT32_C(1) << 30)

typedef struct BaeStr {
    const char *ptr; /* UTF-8, not necessarily NUL-terminated */
    size_t len;
} BaeStr;

typedef struct BaeBytes {
    const void *ptr;
    size_t len;
} BaeBytes;

/* Host-provided sink for variable-size output (state blobs, JSON, paths). */
typedef struct BaeWriter {
    void *opaque;
    BaeStatus (*write)(void *opaque, const void *data, size_t len);
} BaeWriter;

/* Opaque host objects. */
typedef struct BaeHostHandle BaeHostHandle;
typedef struct BaeParamBuilder BaeParamBuilder;
typedef struct BaeParamEditor BaeParamEditor;
typedef struct BaeRenderCtx BaeRenderCtx;
typedef struct BaeGpuSetupCtx BaeGpuSetupCtx;
typedef struct BaeGpuImage BaeGpuImage;
typedef struct BaeGpuBuffer BaeGpuBuffer;
typedef struct BaeGpuPipeline BaeGpuPipeline;
typedef struct BaePathSink BaePathSink;
typedef struct BaeCommandCtx BaeCommandCtx;

/* ======================================================================= */
/* 5. Host, entry point, exports                                           */
/* ======================================================================= */

/* Host capability bits (BaeHost.capabilities). */
#define BAE_HOST_CAP_GPU_COMPUTE (UINT64_C(1) << 0)    /* bae.gpu.compute available */
#define BAE_HOST_CAP_GPU_NATIVE (UINT64_C(1) << 1)     /* bae.gpu.native available */
#define BAE_HOST_CAP_OUT_OF_PROCESS (UINT64_C(1) << 2) /* running inside bae-plugin-runner */
#define BAE_HOST_CAP_HEADLESS (UINT64_C(1) << 3)       /* no UI (bae --headless / bae render) */

typedef struct BaeHost {
    uint32_t struct_size;
    uint32_t abi_version;    /* BAE_MAKE_VERSION of the host ABI */
    uint64_t capabilities;   /* BAE_HOST_CAP_* */
    const char *host_name;   /* "BAE" */
    const char *host_version;/* application semver, e.g. "0.3.1" */
    const char *plugin_id;   /* id of the plugin being loaded (from its manifest) */
    const char *plugin_dir;  /* absolute path of the installed package root */
    BaeHostHandle *handle;   /* pass back to suite functions */
    /* Returns a frozen suite table or NULL if (name, version) is unsupported.
       Thread-safe. The table stays valid until the plugin's shutdown returns. */
    const void *(*get_suite)(BaeHostHandle *host, const char *name, uint32_t version);
} BaeHost;

/* Contribution kinds (manifest kinds that can have a native implementation). */
#define BAE_KIND_EFFECT 1u             /* effects, generators, transitions */
#define BAE_KIND_SHAPE_OPERATOR 2u
#define BAE_KIND_TEXT_SELECTOR 3u      /* reserved, no vtable yet */
#define BAE_KIND_IMPORTER 4u
#define BAE_KIND_EXPORTER 5u
#define BAE_KIND_DECODER 6u            /* manifest [[codec]] with decode role */
#define BAE_KIND_ENCODER 7u            /* manifest [[codec]] with encode role */
#define BAE_KIND_COMMAND 8u
#define BAE_KIND_PANEL 9u              /* reserved for native panels */
#define BAE_KIND_EXPRESSION_LIBRARY 10u
#define BAE_KIND_AUDIO_EFFECT 11u      /* reserved: audio effects (AE AUDIO_* selectors) */

typedef struct BaeContribution {
    uint32_t struct_size;
    uint32_t kind;           /* BAE_KIND_* */
    const char *id;          /* local id; must match the manifest entry */
    uint32_t vtable_version; /* 1 for all v0 vtables */
    uint32_t reserved;
    const void *vtable;      /* e.g. const BaeEffectVTable1*; static lifetime */
} BaeContribution;

typedef struct BaePluginExports {
    uint32_t struct_size;
    uint32_t abi_built;      /* BAE_ABI_VERSION the plugin was compiled against */
    uint32_t abi_required;   /* lowest host ABI the plugin can run on */
    uint32_t reserved0;
    const char *plugin_id;   /* must equal the manifest id */
    const char *plugin_version; /* must equal the manifest version */
    void *plugin_data;       /* plugin-wide pointer handed back to callbacks */
    uint32_t contribution_count;
    uint32_t reserved1;
    const BaeContribution *contributions; /* static lifetime */
    /* Called once, last, after every contribution has been torn down. May be NULL. */
    void (*shutdown)(void *plugin_data);
    /* Plugin-provided extension suites (cross-plugin services). May be NULL. */
    const void *(*get_suite)(void *plugin_data, const char *name, uint32_t version);
} BaePluginExports;

/*
 * The single exported symbol. Called once per load on the host's plugin
 * thread. The host sets out_exports->struct_size; the plugin fills the rest.
 * Return BAE_ERR_VERSION_MISMATCH if the host ABI is too old.
 */
#define BAE_PLUGIN_ENTRY_NAME "bae_plugin_entry"
typedef BaeStatus (*BaePluginEntryFn)(const BaeHost *host, BaePluginExports *out_exports);
#define BAE_PLUGIN_ENTRY \
    BAE_EXPORT BaeStatus bae_plugin_entry(const BaeHost *host, BaePluginExports *out_exports)

/* ======================================================================= */
/* 6. Pixels, images, color                                                */
/* ======================================================================= */

/* BaePixelFormat = channels | component << 8 | alpha << 16. */
typedef uint32_t BaePixelFormat;
#define BAE_CHANNELS_RGBA 1u
#define BAE_CHANNELS_BGRA 2u
#define BAE_CHANNELS_ARGB 3u          /* AE PF_Pixel memory order */
#define BAE_COMPONENT_U8 1u           /* 0..255 */
#define BAE_COMPONENT_U16 2u          /* 0..65535 */
#define BAE_COMPONENT_U16_15BIT 3u    /* 0..32768 (AE "16 bpc") */
#define BAE_COMPONENT_F16 4u
#define BAE_COMPONENT_F32 5u
#define BAE_ALPHA_PREMULTIPLIED 1u
#define BAE_ALPHA_STRAIGHT 2u
#define BAE_PIXEL_FORMAT(channels, component, alpha) \
    ((BaePixelFormat)((channels) | ((component) << 8) | ((alpha) << 16)))
#define BAE_PIXEL_FORMAT_CHANNELS(f) ((f) & 0xFFu)
#define BAE_PIXEL_FORMAT_COMPONENT(f) (((f) >> 8) & 0xFFu)
#define BAE_PIXEL_FORMAT_ALPHA(f) (((f) >> 16) & 0xFFu)

/* The host's native working format: zero conversion cost besides transfer. */
#define BAE_PIXEL_FORMAT_RGBA_F16_PREMUL \
    BAE_PIXEL_FORMAT(BAE_CHANNELS_RGBA, BAE_COMPONENT_F16, BAE_ALPHA_PREMULTIPLIED)
#define BAE_PIXEL_FORMAT_RGBA_F32_PREMUL \
    BAE_PIXEL_FORMAT(BAE_CHANNELS_RGBA, BAE_COMPONENT_F32, BAE_ALPHA_PREMULTIPLIED)
#define BAE_PIXEL_FORMAT_RGBA_U8_PREMUL \
    BAE_PIXEL_FORMAT(BAE_CHANNELS_RGBA, BAE_COMPONENT_U8, BAE_ALPHA_PREMULTIPLIED)
#define BAE_PIXEL_FORMAT_BGRA_U8_PREMUL \
    BAE_PIXEL_FORMAT(BAE_CHANNELS_BGRA, BAE_COMPONENT_U8, BAE_ALPHA_PREMULTIPLIED)
/* AE-compatible formats (used by the future bae-aex-host). */
#define BAE_PIXEL_FORMAT_ARGB_U8_PREMUL \
    BAE_PIXEL_FORMAT(BAE_CHANNELS_ARGB, BAE_COMPONENT_U8, BAE_ALPHA_PREMULTIPLIED)
#define BAE_PIXEL_FORMAT_ARGB_U16_15BIT_PREMUL \
    BAE_PIXEL_FORMAT(BAE_CHANNELS_ARGB, BAE_COMPONENT_U16_15BIT, BAE_ALPHA_PREMULTIPLIED)
#define BAE_PIXEL_FORMAT_ARGB_F32_PREMUL \
    BAE_PIXEL_FORMAT(BAE_CHANNELS_ARGB, BAE_COMPONENT_F32, BAE_ALPHA_PREMULTIPLIED)

/* Working color spaces (BaeRenderInfo.color_space). Pixel buffers AND color
   parameter values are both expressed in this space and encoding. */
#define BAE_COLOR_SPACE_UNKNOWN 0u
#define BAE_COLOR_SPACE_SRGB 1u           /* non-linear sRGB (AE default) */
#define BAE_COLOR_SPACE_LINEAR_SRGB 2u
#define BAE_COLOR_SPACE_ACESCG 3u
#define BAE_COLOR_SPACE_LINEAR_REC2020 4u
#define BAE_COLOR_SPACE_DISPLAY_P3 5u

/* CPU image. Pixel (x, y) lives at
   (uint8_t*)data + (y - rect.y0) * row_bytes + (x - rect.x0) * bytes_per_pixel. */
#define BAE_IMAGE_FLAG_WRITABLE 1u
typedef struct BaeCpuImage {
    uint32_t struct_size;
    BaePixelFormat format;
    BaeRect rect;          /* render pixel space covered by this buffer */
    int64_t row_bytes;     /* > 0, multiple of 16; rows 64-byte aligned by the host */
    void *data;            /* first pixel (rect.x0, rect.y0); read-only unless WRITABLE */
    uint32_t flags;        /* BAE_IMAGE_FLAG_* */
    uint32_t reserved;
} BaeCpuImage;

/* GPU texture formats; masks use BAE_GPU_FORMAT_BIT(f). */
#define BAE_GPU_FORMAT_RGBA16F 1u  /* host working format */
#define BAE_GPU_FORMAT_RGBA32F 2u
#define BAE_GPU_FORMAT_RGBA8_UNORM 3u
#define BAE_GPU_FORMAT_BGRA8_UNORM 4u
#define BAE_GPU_FORMAT_R16F 5u
#define BAE_GPU_FORMAT_R32F 6u
#define BAE_GPU_FORMAT_RG16F 7u
#define BAE_GPU_FORMAT_RG32F 8u
#define BAE_GPU_FORMAT_BIT(f) (1u << (f))

typedef struct BaeGpuImageInfo {
    uint32_t struct_size;
    uint32_t gpu_format;   /* BAE_GPU_FORMAT_* */
    BaeRect rect;          /* render pixel space; texel (0,0) is (rect.x0, rect.y0) */
    uint32_t width;        /* allocated texture size; may exceed the rect (pooled) */
    uint32_t height;
    uint32_t alpha_mode;   /* BAE_ALPHA_* */
    uint32_t reserved;
} BaeGpuImageInfo;

/* ======================================================================= */
/* 7. Parameters                                                           */
/* ======================================================================= */

typedef uint32_t BaeParamType;
#define BAE_PARAM_FLOAT 1u      /* double; AE float slider / fixed slider */
#define BAE_PARAM_INT 2u        /* int32; AE (integer) slider */
#define BAE_PARAM_ANGLE 3u      /* double, degrees */
#define BAE_PARAM_CHECKBOX 4u   /* BaeBool */
#define BAE_PARAM_COLOR 5u      /* float RGBA, straight alpha, working space */
#define BAE_PARAM_POINT 6u      /* double x, y */
#define BAE_PARAM_POINT_3D 7u   /* double x, y, z */
#define BAE_PARAM_POPUP 8u      /* uint32 choice id (stable, not an index) */
#define BAE_PARAM_LAYER 9u      /* reference to a layer in the same comp, or none */
#define BAE_PARAM_GROUP 10u     /* group header (AE topic); children set group_id */
#define BAE_PARAM_ARBITRARY 11u /* opaque bytes with optional callbacks */
#define BAE_PARAM_PATH 12u      /* reference to a mask/path on the layer */
#define BAE_PARAM_BUTTON 13u    /* momentary; only triggers param_changed */
#define BAE_PARAM_STRING 14u    /* UTF-8 text (BAE extension) */
#define BAE_PARAM_ASSET 15u     /* reference to a project asset, e.g. a LUT (BAE extension) */
#define BAE_PARAM_GRADIENT 16u  /* reserved: color ramp (BAE extension) */
#define BAE_PARAM_CURVE 17u     /* reserved: curves (BAE extension) */

/* Parameter flags (BaeParamDef.flags). */
#define BAE_PARAM_FLAG_HIDDEN (UINT64_C(1) << 0)          /* not shown in UI */
#define BAE_PARAM_FLAG_DISABLED (UINT64_C(1) << 1)        /* shown, not editable */
#define BAE_PARAM_FLAG_SUPERVISE (UINT64_C(1) << 2)       /* call param_changed */
#define BAE_PARAM_FLAG_NOT_ANIMATABLE (UINT64_C(1) << 3)  /* no keyframes */
#define BAE_PARAM_FLAG_HOLD_INTERPOLATION (UINT64_C(1) << 4) /* keyframes never interpolate */
#define BAE_PARAM_FLAG_START_COLLAPSED (UINT64_C(1) << 5) /* groups / twirlies */
#define BAE_PARAM_FLAG_UI_ONLY (UINT64_C(1) << 6)         /* never affects pixels: excluded from cache keys */
#define BAE_PARAM_FLAG_NO_EXPRESSIONS (UINT64_C(1) << 7)
#define BAE_PARAM_FLAG_TRANSITION_PROGRESS (UINT64_C(1) << 8) /* the 0..1 progress of a transition */
#define BAE_PARAM_FLAG_RELATIVE_DEFAULT (UINT64_C(1) << 9) /* point default is a fraction of layer size */

/* Display units for numeric parameters. */
#define BAE_DISPLAY_NUMBER 0u
#define BAE_DISPLAY_PERCENT 1u
#define BAE_DISPLAY_PIXELS 2u
#define BAE_DISPLAY_DEGREES 3u
#define BAE_DISPLAY_SECONDS 4u
#define BAE_DISPLAY_FRAMES 5u

/* Layer param default (BaeParamLayerDef.default_mode). */
#define BAE_LAYER_DEFAULT_NONE 0u
#define BAE_LAYER_DEFAULT_SELF 1u

typedef struct BaeParamFloatDef {
    double default_value;
    double valid_min;   /* hard limits */
    double valid_max;
    double slider_min;  /* UI slider range */
    double slider_max;
    uint32_t precision; /* decimal places shown */
    uint32_t display;   /* BAE_DISPLAY_* */
} BaeParamFloatDef;

typedef struct BaeParamIntDef {
    int32_t default_value;
    int32_t valid_min;
    int32_t valid_max;
    int32_t slider_min;
    int32_t slider_max;
    uint32_t display;
} BaeParamIntDef;

typedef struct BaeParamAngleDef {
    double default_degrees;
} BaeParamAngleDef;

typedef struct BaeParamCheckboxDef {
    BaeBool default_value;
    uint32_t reserved;
    const char *label;  /* optional text next to the box */
} BaeParamCheckboxDef;

#define BAE_COLOR_DEF_HAS_ALPHA 1u
typedef struct BaeParamColorDef {
    float default_rgba[4];
    uint32_t flags;     /* BAE_COLOR_DEF_* */
    uint32_t reserved;
} BaeParamColorDef;

/* Used for both POINT (z ignored) and POINT_3D. With RELATIVE_DEFAULT the
   default is a fraction of the layer size (0.5, 0.5 = centre); otherwise it is
   in full-resolution layer pixels. */
typedef struct BaeParamPointDef {
    double default_xyz[3];
    uint32_t reserved[2];
} BaeParamPointDef;

#define BAE_CHOICE_FLAG_SEPARATOR 1u
typedef struct BaePopupChoice {
    uint32_t id;        /* stable value; never reuse */
    uint32_t flags;     /* BAE_CHOICE_FLAG_* */
    const char *name;
    const char *l10n_key; /* may be NULL */
} BaePopupChoice;

typedef struct BaeParamPopupDef {
    uint32_t default_id;
    uint32_t choice_count;
    const BaePopupChoice *choices;
} BaeParamPopupDef;

typedef struct BaeParamLayerDef {
    uint32_t default_mode; /* BAE_LAYER_DEFAULT_* */
    uint32_t reserved;
} BaeParamLayerDef;

/* Callbacks for ARBITRARY params (AE ARBITRARY_CALLBACK equivalents). All
   optional; `global` is the effect's global pointer. Called on any thread. */
typedef struct BaeArbitraryVTable1 {
    uint32_t struct_size;
    uint32_t reserved;
    /* NULL = hold interpolation. */
    BaeStatus (*interpolate)(void *global, BaeBytes a, BaeBytes b, double t, BaeWriter *out);
    /* NULL = bytewise comparison. Returns <0, 0, >0. */
    int32_t (*compare)(void *global, BaeBytes a, BaeBytes b);
    /* Text forms for JSON export, scripting and diffing. */
    BaeStatus (*to_text)(void *global, BaeBytes value, BaeWriter *out_utf8);
    BaeStatus (*from_text)(void *global, BaeStr text, BaeWriter *out_bytes);
} BaeArbitraryVTable1;

typedef struct BaeParamArbitraryDef {
    const BaeArbitraryVTable1 *vtable; /* may be NULL; static lifetime */
    BaeBytes default_value;
} BaeParamArbitraryDef;

typedef struct BaeParamStringDef {
    const char *default_value;
    uint32_t max_length;  /* bytes; 0 = host default */
    BaeBool multiline;
} BaeParamStringDef;

typedef struct BaeParamButtonDef {
    const char *label;
} BaeParamButtonDef;

typedef struct BaeParamAssetDef {
    const char *extensions; /* e.g. ".cube;.3dl" */
} BaeParamAssetDef;

typedef struct BaeParamDef {
    uint32_t struct_size;
    BaeParamType type;
    uint32_t id;           /* stable, > 0, unique in the effect, never reused (AE disk id) */
    uint32_t group_id;     /* 0 = top level, else id of a GROUP param defined earlier */
    const char *key;       /* stable machine key [a-z][a-z0-9_]*, unique; used by WGSL/scripts */
    const char *name;      /* default (English) display name */
    const char *l10n_key;  /* optional key into the plugin's locale files */
    uint64_t flags;        /* BAE_PARAM_FLAG_* */
    union {
        BaeParamFloatDef float_def;
        BaeParamIntDef int_def;
        BaeParamAngleDef angle_def;
        BaeParamCheckboxDef checkbox_def;
        BaeParamColorDef color_def;
        BaeParamPointDef point_def;
        BaeParamPopupDef popup_def;
        BaeParamLayerDef layer_def;
        BaeParamArbitraryDef arbitrary_def;
        BaeParamStringDef string_def;
        BaeParamButtonDef button_def;
        BaeParamAssetDef asset_def;
        uint8_t reserved[128]; /* fixes the union size for ABI stability */
    } u;
} BaeParamDef;

/* Layer reference value. handle == 0 means "none". */
typedef struct BaeLayerRef {
    uint64_t handle;
    uint32_t flags;
    uint32_t reserved;
} BaeLayerRef;

/* One evaluated parameter value. Points are in render pixel space (downsample
   applied); all other values are as stored. Borrowed until the callback returns. */
typedef struct BaeParamValue {
    uint32_t id;
    BaeParamType type;
    union {
        double f;               /* FLOAT, ANGLE */
        int32_t i;              /* INT */
        BaeBool b;              /* CHECKBOX, BUTTON (always false outside param_changed) */
        float color[4];         /* COLOR */
        double point[3];        /* POINT (z = 0), POINT_3D */
        uint32_t choice;        /* POPUP */
        BaeLayerRef layer;      /* LAYER */
        BaeBytes bytes;         /* ARBITRARY */
        BaeStr str;             /* STRING */
        uint64_t handle;        /* PATH, ASSET (opaque host handles) */
        uint8_t reserved[32];
    } v;
} BaeParamValue;

/* Values in definition order (index i = i-th defined param). */
typedef struct BaeParamValues {
    uint32_t count;
    uint32_t reserved;
    const BaeParamValue *values;
} BaeParamValues;

/* ======================================================================= */
/* 8. Effects                                                              */
/* ======================================================================= */

#define BAE_EFFECT_CLASS_FILTER 1u     /* one source input */
#define BAE_EFFECT_CLASS_GENERATOR 2u  /* source input optional */
#define BAE_EFFECT_CLASS_TRANSITION 3u /* inputs "from" (self) and "to" */

/* Static effect flags (BaeEffectDescription.flags). AE equivalents in the spec. */
#define BAE_EFFECT_FLAG_PIXEL_INDEPENDENT (UINT64_C(1) << 0) /* out(p) depends only on in(p) */
#define BAE_EFFECT_FLAG_EXPANDS_BUFFER (UINT64_C(1) << 1)    /* result may exceed input */
#define BAE_EFFECT_FLAG_SHRINKS_BUFFER (UINT64_C(1) << 2)
#define BAE_EFFECT_FLAG_NON_PARAM_VARY (UINT64_C(1) << 3)    /* varies with time at constant params */
#define BAE_EFFECT_FLAG_WIDE_TIME_INPUT (UINT64_C(1) << 4)   /* reads inputs/params at other times */
#define BAE_EFFECT_FLAG_USES_SHUTTER (UINT64_C(1) << 5)
#define BAE_EFFECT_FLAG_USES_CAMERA (UINT64_C(1) << 6)
#define BAE_EFFECT_FLAG_USES_LIGHTS (UINT64_C(1) << 7)
#define BAE_EFFECT_FLAG_NON_DETERMINISTIC (UINT64_C(1) << 8) /* never cache */
#define BAE_EFFECT_FLAG_IN_PLACE (UINT64_C(1) << 9)          /* CPU output may alias input */
#define BAE_EFFECT_FLAG_STATEFUL (UINT64_C(1) << 10)         /* has instance state (instance_* called) */
#define BAE_EFFECT_FLAG_SEND_UPDATE_PARAMS_UI (UINT64_C(1) << 11)
#define BAE_EFFECT_FLAG_DYNAMIC_FLAGS (UINT64_C(1) << 12)    /* call query_dynamic_flags */
#define BAE_EFFECT_FLAG_GPU_NATIVE (UINT64_C(1) << 13)       /* uses bae.gpu.native */
#define BAE_EFFECT_FLAG_EXTERNAL_DEPENDENCIES (UINT64_C(1) << 14) /* call get_dependencies */
#define BAE_EFFECT_FLAG_CUSTOM_UI (UINT64_C(1) << 15)  /* reserved (ABI 0.2): ui_event + bae.draw */
#define BAE_EFFECT_FLAG_OPTIONS_DIALOG (UINT64_C(1) << 16) /* has an "Options..." action: message(BAE_MESSAGE_OPTIONS_DIALOG) */
/* Flags query_dynamic_flags may change. */
#define BAE_EFFECT_DYNAMIC_FLAGS_MASK                                                       \
    (BAE_EFFECT_FLAG_PIXEL_INDEPENDENT | BAE_EFFECT_FLAG_NON_PARAM_VARY |                  \
     BAE_EFFECT_FLAG_WIDE_TIME_INPUT | BAE_EFFECT_FLAG_USES_SHUTTER |                      \
     BAE_EFFECT_FLAG_USES_CAMERA | BAE_EFFECT_FLAG_USES_LIGHTS)

/* Threading contract for render-side callbacks (pre_render, render_*,
   release_frame_data, RENDER-purpose instance_create/destroy). */
#define BAE_THREADING_SERIAL_GLOBAL 0u   /* host serializes ALL calls into this effect */
#define BAE_THREADING_SERIAL_INSTANCE 1u /* concurrent across render instances only */
#define BAE_THREADING_CONCURRENT 2u      /* fully concurrent (AE MFR-safe) */

#define BAE_DEVICE_CPU 1u
#define BAE_DEVICE_GPU 2u

typedef struct BaeEffectDescription {
    uint32_t struct_size;
    uint32_t effect_class;      /* BAE_EFFECT_CLASS_* */
    uint64_t flags;             /* BAE_EFFECT_FLAG_* */
    uint32_t threading;         /* BAE_THREADING_* */
    uint32_t devices;           /* BAE_DEVICE_* mask; CPU is required unless GPU-only is declared */
    uint32_t gpu_formats;       /* BAE_GPU_FORMAT_BIT mask accepted by render_gpu */
    uint32_t cpu_format_count;
    const BaePixelFormat *cpu_formats; /* preference order; valid until global_setdown */
} BaeEffectDescription;

typedef struct BaeGlobalSetupArgs {
    uint32_t struct_size;
    uint32_t reserved;
    const BaeHost *host;
    const char *contribution_id;
    const char *locale;         /* UI locale, e.g. "zh-CN" */
} BaeGlobalSetupArgs;

#define BAE_INSTANCE_EDIT 1u    /* lives on the model thread, may be mutated */
#define BAE_INSTANCE_RENDER 2u  /* immutable after creation, shared by render threads */

typedef struct BaeInstanceArgs {
    uint32_t struct_size;
    uint32_t purpose;           /* BAE_INSTANCE_* */
    BaeBytes state;             /* serialized state; len == 0 for a brand-new instance */
    const char *state_plugin_version; /* version that wrote `state`; NULL if empty */
    uint64_t instance_uid;      /* stable id of the effect in the document */
} BaeInstanceArgs;

#define BAE_QUALITY_DRAFT 0u
#define BAE_QUALITY_HIGH 1u

#define BAE_RENDER_FLAG_LINEAR_BLENDING 1u /* working space is linearized */
#define BAE_RENDER_FLAG_FINAL 2u           /* render queue / bae render */
#define BAE_RENDER_FLAG_PREVIEW 4u
#define BAE_RENDER_FLAG_HEADLESS 8u

/* Common description of one frame request. */
typedef struct BaeRenderInfo {
    uint32_t struct_size;
    uint32_t quality;           /* BAE_QUALITY_* */
    BaeFlicks time;             /* evaluation time (layer time) */
    BaeFlicks time_step;        /* one frame at the comp rate; negative when reversed */
    BaeRational frame_rate;     /* comp frame rate */
    BaeRational downsample_x;   /* 1/1 full, 1/2 half, ... */
    BaeRational downsample_y;
    BaeRational pixel_aspect;   /* layer pixel aspect ratio */
    float shutter_angle;        /* degrees */
    float shutter_phase;        /* degrees */
    uint32_t color_space;       /* BAE_COLOR_SPACE_* of pixels and color params */
    uint32_t flags;             /* BAE_RENDER_FLAG_* */
    BaeRect layer_bounds;       /* the layer's own bounds in render pixel space */
    uint64_t render_id;         /* unique per frame request; for logs */
    BaeParamValues params;      /* values at `time` */
    BaeFlicks layer_duration;   /* duration of the layer (AE in_data total_time) */
} BaeRenderInfo;

/* Input sources for checkout_input. */
#define BAE_INPUT_SELF 0u        /* the effect's input (layer pixels after previous effects) */
#define BAE_INPUT_PARAM_LAYER 1u /* the layer referenced by a LAYER param */
#define BAE_INPUT_TRANSITION_TO 2u
#define BAE_INPUT_FLAG_WITHOUT_EFFECTS 1u /* layer source without masks/effects */

typedef struct BaeInputRequest {
    uint32_t struct_size;
    uint32_t source;            /* BAE_INPUT_* */
    uint32_t param_id;          /* for BAE_INPUT_PARAM_LAYER */
    uint32_t checkout_id;       /* plugin-chosen, unique per pre_render, > 0 (0 = implicit self input) */
    BaeFlicks time;             /* may differ from info->time (needs WIDE_TIME_INPUT) */
    BaeRect rect;               /* requested rect in that input's render pixel space */
    uint32_t flags;             /* BAE_INPUT_FLAG_* */
    uint32_t reserved;
} BaeInputRequest;

#define BAE_INPUT_INFO_EMPTY 1u /* nothing to render (e.g. layer param is none) */
#define BAE_INPUT_INFO_SOLID 2u /* fully opaque within result_rect */

typedef struct BaeInputInfo {
    uint32_t struct_size;
    uint32_t flags;             /* BAE_INPUT_INFO_* */
    BaeRect result_rect;        /* what the input will actually provide */
    BaeRect max_result_rect;    /* largest it could ever provide */
    BaeRect layer_bounds;
    BaeRational pixel_aspect;
} BaeInputInfo;

#define BAE_PRERENDER_ARG_PRESERVE_RGB_OF_ZERO_ALPHA 1u

typedef struct BaePreRenderArgs {
    uint32_t struct_size;
    uint32_t device;            /* BAE_DEVICE_GPU if the host plans a GPU render */
    const BaeRenderInfo *info;
    BaeRenderCtx *ctx;          /* for bae.render: checkout_input, checkout_param */
    BaeRect request_rect;       /* region of the output the host needs */
    uint32_t flags;             /* BAE_PRERENDER_ARG_* */
    uint32_t reserved;
} BaePreRenderArgs;

#define BAE_PRERENDER_SOLID 1u                /* result fully opaque */
#define BAE_PRERENDER_GPU_POSSIBLE 2u         /* preset by host when GPU is planned; clear to force CPU */
#define BAE_PRERENDER_RETURNS_EXTRA_PIXELS 4u /* may write outside request_rect */
#define BAE_PRERENDER_IDENTITY 8u             /* output == self input: render is skipped */

typedef struct BaePreRenderResult {
    uint32_t struct_size;
    uint32_t flags;             /* BAE_PRERENDER_* */
    BaeRect result_rect;
    BaeRect max_result_rect;
    void *frame_data;           /* handed to render_*; freed with release_frame_data */
} BaePreRenderResult;

typedef struct BaeCpuRenderArgs {
    uint32_t struct_size;
    uint32_t reserved;
    const BaeRenderInfo *info;
    BaeRenderCtx *ctx;          /* for bae.render: inputs, abort, progress, parallel_for */
    void *frame_data;
    BaeCpuImage *output;        /* writable; covers result_rect clipped to request */
} BaeCpuRenderArgs;

typedef struct BaeGpuRenderArgs {
    uint32_t struct_size;
    uint32_t reserved;
    const BaeRenderInfo *info;
    BaeRenderCtx *ctx;          /* for bae.render, bae.gpu.compute, bae.gpu.native */
    void *frame_data;
    BaeGpuImage *output;
} BaeGpuRenderArgs;

/* GPU features (BaeGpuDeviceArgs.features). */
#define BAE_GPU_FEATURE_SHADER_F16 (UINT64_C(1) << 0)
#define BAE_GPU_FEATURE_FLOAT32_FILTERABLE (UINT64_C(1) << 1)
#define BAE_GPU_FEATURE_TIMESTAMP_QUERY (UINT64_C(1) << 2)

#define BAE_GPU_BACKEND_NONE 0u
#define BAE_GPU_BACKEND_METAL 1u
#define BAE_GPU_BACKEND_D3D12 2u
#define BAE_GPU_BACKEND_VULKAN 3u

typedef struct BaeGpuDeviceArgs {
    uint32_t struct_size;
    uint32_t backend;           /* BAE_GPU_BACKEND_* */
    BaeGpuSetupCtx *ctx;        /* for bae.gpu.compute create_pipeline / bae.gpu.native */
    uint64_t features;          /* BAE_GPU_FEATURE_* */
    uint32_t device_index;
    uint32_t reserved;
} BaeGpuDeviceArgs;

#define BAE_CHANGED_STATE 1u     /* instance state changed: host re-serializes it */
#define BAE_CHANGED_RERENDER 2u  /* force a re-render */

typedef struct BaeParamChangedArgs {
    uint32_t struct_size;
    uint32_t param_id;
    BaeFlicks time;             /* current time indicator */
    const BaeParamValues *values; /* at `time`, after the change */
    BaeParamEditor *editor;     /* bae.param.edit; edits join the same undo step */
} BaeParamChangedArgs;

typedef struct BaeParamUiArgs {
    uint32_t struct_size;
    uint32_t reserved;
    BaeFlicks time;
    const BaeParamValues *values;
    BaeParamEditor *editor;     /* only set_ui is allowed here */
} BaeParamUiArgs;

/*
 * Effect vtable (kind BAE_KIND_EFFECT, vtable_version 1). Required:
 * global_setup and at least one of render_cpu / render_gpu. Everything else
 * may be NULL. Threads: L = plugin/load thread, M = model thread,
 * R = render threads (see BAE_THREADING_*), G = GPU setup thread.
 * AE selector equivalents are listed for bae-aex-host (workstream 20).
 */
typedef struct BaeEffectVTable1 {
    uint32_t struct_size;
    uint32_t reserved;
    /* L. AE: GLOBAL_SETUP (+ PiPL flags). Fill the description; set *out_global. */
    BaeStatus (*global_setup)(void *plugin_data, const BaeGlobalSetupArgs *args,
                              BaeEffectDescription *out_desc, void **out_global);
    /* L. AE: GLOBAL_SETDOWN. */
    void (*global_setdown)(void *global);
    /* L. AE: PARAMS_SETUP. Add params via bae.param.define. Called once; cached. */
    BaeStatus (*define_params)(void *global, BaeParamBuilder *builder);
    /* G. AE: GPU_DEVICE_SETUP / GPU_DEVICE_SETDOWN. Per device; create pipelines here. */
    BaeStatus (*gpu_device_setup)(void *global, const BaeGpuDeviceArgs *args, void **out_gpu_data);
    void (*gpu_device_setdown)(void *global, const BaeGpuDeviceArgs *args, void *gpu_data);
    /* M (EDIT) or R (RENDER). AE: SEQUENCE_SETUP (state empty) / SEQUENCE_RESETUP. */
    BaeStatus (*instance_create)(void *global, const BaeInstanceArgs *args, void **out_instance);
    /* M or R. AE: SEQUENCE_FLATTEN / GET_FLATTENED_SEQUENCE_DATA. Non-destructive. */
    BaeStatus (*instance_serialize)(void *global, const void *instance, BaeWriter *out);
    /* M or R. AE: SEQUENCE_SETDOWN. */
    void (*instance_destroy)(void *global, void *instance);
    /* M. AE: USER_CHANGED_PARAM. *out_flags |= BAE_CHANGED_*. */
    BaeStatus (*param_changed)(void *global, void *instance, const BaeParamChangedArgs *args,
                               uint32_t *out_flags);
    /* M. AE: UPDATE_PARAMS_UI (needs SEND_UPDATE_PARAMS_UI). */
    BaeStatus (*update_params_ui)(void *global, const void *instance, const BaeParamUiArgs *args);
    /* M. AE: QUERY_DYNAMIC_FLAGS (needs DYNAMIC_FLAGS). May change DYNAMIC_FLAGS_MASK bits. */
    BaeStatus (*query_dynamic_flags)(void *global, const void *instance,
                                     const BaeParamValues *values, uint64_t *inout_flags);
    /* R. AE: SMART_PRE_RENDER (and FRAME_SETUP). NULL = identity ROI on self input. */
    BaeStatus (*pre_render)(void *global, const void *instance, const BaePreRenderArgs *args,
                            BaePreRenderResult *out_result);
    /* R. AE: SMART_RENDER / RENDER. */
    BaeStatus (*render_cpu)(void *global, const void *instance, const BaeCpuRenderArgs *args);
    /* R. AE: SMART_RENDER_GPU. May return BAE_STATUS_FALLBACK. */
    BaeStatus (*render_gpu)(void *global, const void *instance, void *gpu_data,
                            const BaeGpuRenderArgs *args);
    /* R. AE: FRAME_SETDOWN / SmartFX delete_pre_render_data. */
    void (*release_frame_data)(void *global, const void *instance, void *frame_data);
    /* M. AE: GET_EXTERNAL_DEPENDENCIES. JSON array of {"kind","path"|"name"}. */
    BaeStatus (*get_dependencies)(void *global, const void *instance,
                                  const BaeParamValues *values, BaeWriter *out_json);
    /* M. AE: COMPLETELY_GENERAL. Generic extension point, also reachable from scripts. */
    BaeStatus (*message)(void *global, void *instance, const char *selector, BaeBytes input,
                         BaeWriter *output);
} BaeEffectVTable1;

/*
 * Well-known `message` selectors (BaeEffectVTable1.message). Selectors starting
 * with "bae." are reserved for the host; plugins use their own reverse-DNS prefix.
 *   bae.about           input empty; output UTF-8 about text (AE ABOUT).
 *   bae.options_dialog  M thread; input empty; output JSON {"changed":bool}.
 *                       The plugin edits values only through bae.param.edit
 *                       (AE DO_DIALOG; needs BAE_EFFECT_FLAG_OPTIONS_DIALOG).
 *   bae.migrate         M thread; input JSON {"from_version","params":[...],"state_b64"};
 *                       output the same shape for the current version (spec 6.5).
 */
#define BAE_MESSAGE_ABOUT "bae.about"
#define BAE_MESSAGE_OPTIONS_DIALOG "bae.options_dialog"
#define BAE_MESSAGE_MIGRATE "bae.migrate"

/* ======================================================================= */
/* 9. Shape operators                                                      */
/* ======================================================================= */

/* Cubic Bezier vertex; tangents relative to the vertex (AE / Lottie style). */
typedef struct BaePathVertex {
    double x, y;
    double in_x, in_y;
    double out_x, out_y;
} BaePathVertex;

typedef struct BaePath {
    uint32_t vertex_count;
    BaeBool closed;
    const BaePathVertex *vertices;
} BaePath;

typedef struct BaePathSet {
    uint32_t path_count;
    uint32_t reserved;
    const BaePath *paths;
} BaePathSet;

#define BAE_SHAPE_OP_FLAG_PER_PATH (UINT64_C(1) << 0) /* each path independent: host may parallelize */

typedef struct BaeShapeApplyArgs {
    uint32_t struct_size;
    uint32_t reserved;
    BaeFlicks time;
    BaeParamValues params;
    BaePathSet input;           /* layer space, full resolution */
    BaePathSink *sink;          /* bae.path add_path */
} BaeShapeApplyArgs;

typedef struct BaeShapeOperatorVTable1 {
    uint32_t struct_size;
    uint32_t reserved;
    /* L. *out_flags = BAE_SHAPE_OP_FLAG_* ; threading follows BAE_THREADING_CONCURRENT. */
    BaeStatus (*global_setup)(void *plugin_data, const BaeGlobalSetupArgs *args,
                              uint64_t *out_flags, void **out_global);
    void (*global_setdown)(void *global);
    BaeStatus (*define_params)(void *global, BaeParamBuilder *builder);
    /* Any thread, concurrently. */
    BaeStatus (*apply)(void *global, const BaeShapeApplyArgs *args);
} BaeShapeOperatorVTable1;

/* ======================================================================= */
/* 10. Importers, exporters, commands                                      */
/* ======================================================================= */

/* Importers turn a file into command invocations (one undoable transaction). */
typedef struct BaeImportArgs {
    uint32_t struct_size;
    uint32_t reserved;
    BaeStr path;
    BaeStr options_json;
    BaeCommandCtx *ctx;         /* bae.command: invoke, query, progress */
} BaeImportArgs;

typedef struct BaeImporterVTable1 {
    uint32_t struct_size;
    uint32_t reserved;
    /* Any thread. 0 = cannot import, 1..100 = confidence. `header` = first bytes. */
    uint32_t (*probe)(void *plugin_data, BaeStr path, BaeBytes header);
    BaeStatus (*run)(void *plugin_data, const BaeImportArgs *args);
} BaeImporterVTable1;

typedef struct BaeExportArgs {
    uint32_t struct_size;
    uint32_t reserved;
    BaeStr path;
    BaeStr options_json;
    BaeStr target_prim;         /* prim path to export, e.g. "/comps/main" */
    BaeCommandCtx *ctx;         /* read-only: only query is allowed */
} BaeExportArgs;

typedef struct BaeExporterVTable1 {
    uint32_t struct_size;
    uint32_t reserved;
    BaeStatus (*run)(void *plugin_data, const BaeExportArgs *args);
} BaeExporterVTable1;

typedef struct BaeCommandArgs {
    uint32_t struct_size;
    uint32_t reserved;
    BaeStr args_json;           /* validated against the manifest schema */
    BaeCommandCtx *ctx;
    BaeWriter *result_json;
} BaeCommandArgs;

typedef struct BaeCommandVTable1 {
    uint32_t struct_size;
    uint32_t reserved;
    /* Model thread, inside one transaction (one undo step). */
    BaeStatus (*execute)(void *plugin_data, const BaeCommandArgs *args);
    /* Optional: enablement for menus. NULL = always enabled. */
    BaeBool (*can_execute)(void *plugin_data, BaeCommandCtx *ctx);
} BaeCommandVTable1;

/* ======================================================================= */
/* 11. Codecs (decoders / encoders)                                        */
/* ======================================================================= */

/* Color metadata as ITU-T H.273 (CICP) code points. */
typedef struct BaeColorInfo {
    uint32_t primaries;
    uint32_t transfer;
    uint32_t matrix;
    uint32_t full_range;        /* BaeBool */
} BaeColorInfo;

#define BAE_PLANAR_NONE 0u      /* packed RGBA-family BaeCpuImage */
#define BAE_PLANAR_NV12 1u
#define BAE_PLANAR_P010 2u
#define BAE_PLANAR_YUV420P 3u
#define BAE_PLANAR_YUV422P10 4u
#define BAE_PLANAR_YUV444P16 5u

typedef struct BaeVideoStreamInfo {
    uint32_t struct_size;
    BaeBool has_alpha;
    uint32_t width;
    uint32_t height;
    BaeRational frame_rate;
    BaeRational pixel_aspect;
    BaeFlicks duration;
    BaeColorInfo color;
    uint32_t planar_layout;     /* BAE_PLANAR_*: the decoder's native output */
    uint32_t reserved;
} BaeVideoStreamInfo;

typedef struct BaeAudioStreamInfo {
    uint32_t struct_size;
    uint32_t channels;
    uint32_t sample_rate;
    uint32_t reserved;
    int64_t frame_count;
} BaeAudioStreamInfo;

#define BAE_MEDIA_HAS_VIDEO 1u
#define BAE_MEDIA_HAS_AUDIO 2u

typedef struct BaeMediaInfo {
    uint32_t struct_size;
    uint32_t flags;             /* BAE_MEDIA_* */
    BaeVideoStreamInfo video;
    BaeAudioStreamInfo audio;
} BaeMediaInfo;

typedef struct BaePlanarImage {
    uint32_t struct_size;
    uint32_t layout;            /* BAE_PLANAR_* */
    uint32_t width;
    uint32_t height;
    BaeColorInfo color;
    void *planes[4];
    int64_t row_bytes[4];
} BaePlanarImage;

typedef struct BaeDecoderOpenArgs {
    uint32_t struct_size;
    uint32_t reserved;
    BaeStr path;
    BaeStr options_json;
} BaeDecoderOpenArgs;

typedef struct BaeDecodeVideoArgs {
    uint32_t struct_size;
    uint32_t reserved;
    BaeFlicks time;             /* media time */
    BaeCpuImage *rgba;          /* non-NULL when planar_layout == BAE_PLANAR_NONE */
    BaePlanarImage *planar;     /* otherwise; host-allocated in the native layout */
} BaeDecodeVideoArgs;

/* Handles are per open file; calls on one handle are serialized by the host,
   different handles may be used concurrently. */
typedef struct BaeDecoderVTable1 {
    uint32_t struct_size;
    uint32_t reserved;
    uint32_t (*probe)(void *plugin_data, BaeStr path, BaeBytes header);
    BaeStatus (*open)(void *plugin_data, const BaeDecoderOpenArgs *args, void **out_handle);
    BaeStatus (*get_info)(void *handle, BaeMediaInfo *out_info);
    BaeStatus (*decode_video)(void *handle, const BaeDecodeVideoArgs *args);
    /* Interleaved float samples; *out_frames = frames written. */
    BaeStatus (*decode_audio)(void *handle, int64_t first_frame, uint32_t frame_count,
                              float *out_samples, uint32_t *out_frames);
    void (*close)(void *handle);
} BaeDecoderVTable1;

typedef struct BaeEncoderOpenArgs {
    uint32_t struct_size;
    uint32_t reserved;
    BaeStr path;
    BaeStr settings_json;
    BaeVideoStreamInfo video;   /* struct_size 0 = no video */
    BaeAudioStreamInfo audio;   /* struct_size 0 = no audio */
} BaeEncoderOpenArgs;

typedef struct BaeEncoderVTable1 {
    uint32_t struct_size;
    uint32_t reserved;
    /* *out_format: the BaePixelFormat encode_video expects. */
    BaeStatus (*open)(void *plugin_data, const BaeEncoderOpenArgs *args,
                      BaePixelFormat *out_format, void **out_handle);
    BaeStatus (*encode_video)(void *handle, const BaeCpuImage *frame, BaeFlicks pts);
    BaeStatus (*encode_audio)(void *handle, const float *samples, uint32_t frame_count);
    BaeStatus (*finish)(void *handle);
    void (*close)(void *handle);
} BaeEncoderVTable1;

/* ======================================================================= */
/* 12. Expression function libraries (native, numeric, pure)               */
/* ======================================================================= */

/* Scalar (len 1) up to vec4 (len 4). */
typedef struct BaeExprValue {
    uint32_t len;
    uint32_t reserved;
    double v[4];
} BaeExprValue;

typedef struct BaeExprFunction {
    const char *name;           /* exposed as <namespace>.<name> */
    uint32_t min_args;
    uint32_t max_args;
    uint32_t flags;             /* reserved, 0 */
    uint32_t reserved;
    /* Pure, deterministic, thread-safe, no allocation. */
    BaeStatus (*call)(void *plugin_data, const BaeExprValue *args, uint32_t arg_count,
                      BaeExprValue *out_result);
} BaeExprFunction;

typedef struct BaeExpressionLibraryVTable1 {
    uint32_t struct_size;
    uint32_t function_count;
    const BaeExprFunction *functions; /* static lifetime */
} BaeExpressionLibraryVTable1;

/* ======================================================================= */
/* 13. Host suites                                                         */
/* ======================================================================= */

/* ---- bae.core v1: logging, errors, memory, permissions, paths ---- */
#define BAE_SUITE_CORE "bae.core"
#define BAE_SUITE_CORE_VERSION 1u

#define BAE_LOG_ERROR 1u
#define BAE_LOG_WARN 2u
#define BAE_LOG_INFO 3u
#define BAE_LOG_DEBUG 4u
#define BAE_LOG_TRACE 5u

#define BAE_PATH_PLUGIN_DIR 1u /* read-only package root */
#define BAE_PATH_DATA_DIR 2u   /* persistent, per plugin, writable */
#define BAE_PATH_CACHE_DIR 3u  /* purgeable, per plugin */
#define BAE_PATH_TEMP_DIR 4u

typedef struct BaeCoreSuite1 {
    void (*log)(BaeHostHandle *host, uint32_t level, const char *message);
    /* Attach a message to the error the current callback is about to return
       (per thread). */
    void (*set_error)(BaeHostHandle *host, const char *message);
    void *(*alloc)(BaeHostHandle *host, size_t size, size_t align);
    void (*free)(BaeHostHandle *host, void *ptr);
    /* e.g. "filesystem:read:project", "network:https://api.example.com". */
    BaeBool (*has_permission)(BaeHostHandle *host, const char *permission);
    BaeStatus (*get_path)(BaeHostHandle *host, uint32_t which, BaeWriter *out_utf8);
} BaeCoreSuite1;

/* ---- bae.param.define v1: used inside define_params ---- */
#define BAE_SUITE_PARAM_DEFINE "bae.param.define"
#define BAE_SUITE_PARAM_DEFINE_VERSION 1u

typedef struct BaeParamDefineSuite1 {
    /* Params are added in display order; the def is copied. */
    BaeStatus (*add)(BaeParamBuilder *builder, const BaeParamDef *def);
} BaeParamDefineSuite1;

/* ---- bae.param.edit v1: used inside param_changed / update_params_ui ---- */
#define BAE_SUITE_PARAM_EDIT "bae.param.edit"
#define BAE_SUITE_PARAM_EDIT_VERSION 1u

typedef struct BaeParamEditSuite1 {
    /* Sets a value (a keyframe at the current time if animated). Becomes an
       op in the same command as the user's change. Not allowed in update_params_ui. */
    BaeStatus (*set_value)(BaeParamEditor *editor, const BaeParamValue *value);
    /* UI-only state: set/clear HIDDEN and DISABLED, optionally rename. Not undoable. */
    BaeStatus (*set_ui)(BaeParamEditor *editor, uint32_t param_id, uint64_t set_flags,
                        uint64_t clear_flags, const char *name);
} BaeParamEditSuite1;

/* ---- bae.render v1: inputs, params at other times, abort, CPU helpers ---- */
#define BAE_SUITE_RENDER "bae.render"
#define BAE_SUITE_RENDER_VERSION 1u

typedef BaeStatus (*BaeParallelFn)(void *user, uint32_t index, uint32_t thread_index);

typedef struct BaeRenderSuite1 {
    /* pre_render only. Declares a dependency and returns what the input provides. */
    BaeStatus (*checkout_input)(BaeRenderCtx *ctx, const BaeInputRequest *request,
                                BaeInputInfo *out_info);
    /* pre_render or render. Value of a param at another time (dependency is recorded). */
    BaeStatus (*checkout_param)(BaeRenderCtx *ctx, uint32_t param_id, BaeFlicks time,
                                BaeParamValue *out_value);
    /* render_cpu only. checkout_id 0 = the implicit self input. */
    BaeStatus (*get_input_cpu)(BaeRenderCtx *ctx, uint32_t checkout_id, BaeCpuImage *out_image);
    /* render_gpu only. */
    BaeStatus (*get_input_gpu)(BaeRenderCtx *ctx, uint32_t checkout_id, BaeGpuImage **out_image);
    /* Poll regularly; on true return BAE_ERR_ABORTED promptly. */
    BaeBool (*is_aborted)(BaeRenderCtx *ctx);
    void (*report_progress)(BaeRenderCtx *ctx, uint64_t done, uint64_t total);
    /* render_cpu only. Temporary image released when the callback returns. */
    BaeStatus (*alloc_cpu_image)(BaeRenderCtx *ctx, BaePixelFormat format, BaeRect rect,
                                 BaeCpuImage *out_image);
    /* render_cpu only. Runs fn(user, i, thread) for i in [0, count) on the host pool;
       returns the first error. Must not be nested. */
    BaeStatus (*parallel_for)(BaeRenderCtx *ctx, uint32_t count, BaeParallelFn fn, void *user);
} BaeRenderSuite1;

/* ---- bae.gpu.compute v1: host-encoded WGSL compute (portable, no sync cost) ---- */
#define BAE_SUITE_GPU_COMPUTE "bae.gpu.compute"
#define BAE_SUITE_GPU_COMPUTE_VERSION 1u

#define BAE_BINDING_UNIFORM 1u              /* bytes copied into the host uniform ring */
#define BAE_BINDING_SAMPLED_TEXTURE 2u      /* texture_2d<f32> */
#define BAE_BINDING_STORAGE_TEXTURE_WRITE 3u/* texture_storage_2d<format, write> */
#define BAE_BINDING_STORAGE_BUFFER 4u       /* var<storage, read_write> */
#define BAE_BINDING_STORAGE_BUFFER_READ 5u  /* var<storage, read> */
#define BAE_BINDING_SAMPLER_LINEAR 6u       /* bound by the host automatically */
#define BAE_BINDING_SAMPLER_NEAREST 7u

typedef struct BaeGpuBindingLayout {
    uint32_t binding;           /* @group(0) @binding(n) */
    uint32_t type;              /* BAE_BINDING_* */
    uint32_t gpu_format;        /* storage textures only */
    uint32_t reserved;
} BaeGpuBindingLayout;

typedef struct BaeComputePipelineDesc {
    uint32_t struct_size;
    uint32_t binding_count;
    BaeStr wgsl_source;
    const char *entry_point;
    const BaeGpuBindingLayout *bindings;
    const char *label;          /* for diagnostics */
} BaeComputePipelineDesc;

typedef struct BaeGpuBinding {
    uint32_t binding;
    uint32_t type;              /* must match the layout */
    BaeGpuImage *image;         /* textures */
    BaeGpuBuffer *buffer;       /* storage buffers */
    BaeBytes data;              /* uniforms */
} BaeGpuBinding;

typedef struct BaeGpuComputeSuite1 {
    /* gpu_device_setup only. Compile errors are reported through set_error/log. */
    BaeStatus (*create_pipeline)(BaeGpuSetupCtx *ctx, const BaeComputePipelineDesc *desc,
                                 BaeGpuPipeline **out_pipeline);
    void (*destroy_pipeline)(BaeGpuSetupCtx *ctx, BaeGpuPipeline *pipeline);
    BaeStatus (*get_image_info)(BaeRenderCtx *ctx, BaeGpuImage *image, BaeGpuImageInfo *out_info);
    /* Temporaries from the host pool; released when render_gpu returns. */
    BaeStatus (*acquire_image)(BaeRenderCtx *ctx, uint32_t gpu_format, BaeRect rect,
                               BaeGpuImage **out_image);
    BaeStatus (*acquire_buffer)(BaeRenderCtx *ctx, uint64_t size, BaeGpuBuffer **out_buffer);
    BaeStatus (*dispatch)(BaeRenderCtx *ctx, BaeGpuPipeline *pipeline,
                          const BaeGpuBinding *bindings, uint32_t binding_count,
                          uint32_t groups_x, uint32_t groups_y, uint32_t groups_z);
    BaeStatus (*copy_image)(BaeRenderCtx *ctx, BaeGpuImage *src, BaeGpuImage *dst, BaeRect rect);
    BaeStatus (*clear_image)(BaeRenderCtx *ctx, BaeGpuImage *image, const float rgba[4]);
} BaeGpuComputeSuite1;

/* ---- bae.gpu.native v1: raw Metal / D3D12 / Vulkan interop ---- */
#define BAE_SUITE_GPU_NATIVE "bae.gpu.native"
#define BAE_SUITE_GPU_NATIVE_VERSION 1u

/*
 * Handle meanings (all stored as uint64_t):
 *   Metal : device = id<MTLDevice>, queue = id<MTLCommandQueue>,
 *           texture = id<MTLTexture>, fence = id<MTLSharedEvent>
 *   D3D12 : device = ID3D12Device*, queue = ID3D12CommandQueue*,
 *           physical_device = IDXGIAdapter*, texture = ID3D12Resource*,
 *           fence = ID3D12Fence*, state = D3D12_RESOURCE_STATES
 *   Vulkan: instance = VkInstance, physical_device = VkPhysicalDevice,
 *           device = VkDevice, queue = VkQueue, texture = VkImage,
 *           view = VkImageView, fence = VkSemaphore (timeline), state = VkImageLayout
 * The host owns every handle. Plugins may retain (ObjC retain / AddRef) the
 * device and queue in gpu_device_setup and must release them in setdown.
 */
typedef struct BaeNativeGpuDevice {
    uint32_t struct_size;
    uint32_t backend;           /* BAE_GPU_BACKEND_* */
    uint64_t instance;
    uint64_t physical_device;
    uint64_t device;
    uint64_t queue;
    uint32_t queue_family_index;
    uint32_t device_index;
    uint64_t adapter_luid;
} BaeNativeGpuDevice;

#define BAE_GPU_USAGE_SAMPLED 1u  /* D3D12: PIXEL|NON_PIXEL_SHADER_RESOURCE; VK: SHADER_READ_ONLY_OPTIMAL */
#define BAE_GPU_USAGE_STORAGE 2u  /* D3D12: UNORDERED_ACCESS; VK: GENERAL */

typedef struct BaeNativeImageRequest {
    BaeGpuImage *image;
    uint32_t usage;             /* BAE_GPU_USAGE_* */
    uint32_t reserved;
} BaeNativeImageRequest;

typedef struct BaeNativeGpuTexture {
    uint32_t struct_size;
    uint32_t backend;
    uint64_t texture;
    uint64_t view;              /* Vulkan only, else 0 */
    uint32_t native_format;     /* MTLPixelFormat / DXGI_FORMAT / VkFormat */
    uint32_t state;             /* entry state; MUST be restored before end() */
    uint32_t width;
    uint32_t height;
    BaeRect rect;
    uint32_t usage;
    uint32_t reserved;
} BaeNativeGpuTexture;

typedef struct BaeNativeGpuScope {
    uint32_t struct_size;
    uint32_t reserved;
    BaeNativeGpuDevice device;
    uint64_t wait_fence;        /* 0 = queue order is sufficient */
    uint64_t wait_value;
    uint64_t signal_fence;      /* 0 = nothing to signal */
    uint64_t signal_value;
} BaeNativeGpuScope;

typedef struct BaeGpuNativeSuite1 {
    /* gpu_device_setup: the device the pipelines are for. */
    BaeStatus (*get_device)(BaeGpuSetupCtx *ctx, BaeNativeGpuDevice *out_device);
    /* render_gpu: transitions the images, submits all pending host work and
       opens a native scope. out_textures has `count` entries. */
    BaeStatus (*begin)(BaeRenderCtx *ctx, const BaeNativeImageRequest *images, uint32_t count,
                       BaeNativeGpuScope *out_scope, BaeNativeGpuTexture *out_textures);
    /* The plugin has committed all its work to scope.device.queue (and
       signalled signal_fence if non-zero). It must not wait for completion. */
    BaeStatus (*end)(BaeRenderCtx *ctx);
} BaeGpuNativeSuite1;

/* ---- bae.path v1: output sink for shape operators ---- */
#define BAE_SUITE_PATH "bae.path"
#define BAE_SUITE_PATH_VERSION 1u

typedef struct BaePathSuite1 {
    BaeStatus (*add_path)(BaePathSink *sink, const BaePath *path); /* path is copied */
} BaePathSuite1;

/* ---- bae.command v1: commands, importers, exporters ---- */
#define BAE_SUITE_COMMAND "bae.command"
#define BAE_SUITE_COMMAND_VERSION 1u

typedef struct BaeCommandSuite1 {
    /* Runs another registered command inside the current transaction. */
    BaeStatus (*invoke)(BaeCommandCtx *ctx, const char *command_id, BaeStr args_json,
                        BaeWriter *result_json);
    /* Read-only queries shared with MCP: "stage.describe", "prim.get", "prim.find",
       "selection.get". */
    BaeStatus (*query)(BaeCommandCtx *ctx, const char *query, BaeStr args_json,
                       BaeWriter *result_json);
    BaeBool (*is_aborted)(BaeCommandCtx *ctx);
    void (*report_progress)(BaeCommandCtx *ctx, uint64_t done, uint64_t total);
} BaeCommandSuite1;

/* ======================================================================= */
/* 14. Layout guarantees (checked on every supported 64-bit target)        */
/* ======================================================================= */

#if UINTPTR_MAX == UINT64_MAX
BAE_STATIC_ASSERT(sizeof(BaeRational) == 8, "BaeRational");
BAE_STATIC_ASSERT(sizeof(BaeRect) == 16, "BaeRect");
BAE_STATIC_ASSERT(sizeof(BaeStr) == 16, "BaeStr");
BAE_STATIC_ASSERT(sizeof(BaeBytes) == 16, "BaeBytes");
BAE_STATIC_ASSERT(sizeof(BaeWriter) == 16, "BaeWriter");
BAE_STATIC_ASSERT(sizeof(BaeHost) == 64, "BaeHost");
BAE_STATIC_ASSERT(sizeof(BaeContribution) == 32, "BaeContribution");
BAE_STATIC_ASSERT(sizeof(BaePluginExports) == 72, "BaePluginExports");
BAE_STATIC_ASSERT(sizeof(BaeCpuImage) == 48, "BaeCpuImage");
BAE_STATIC_ASSERT(sizeof(BaeGpuImageInfo) == 40, "BaeGpuImageInfo");
BAE_STATIC_ASSERT(sizeof(BaeParamDef) == 176, "BaeParamDef");
BAE_STATIC_ASSERT(offsetof(BaeParamDef, u) == 48, "BaeParamDef.u");
BAE_STATIC_ASSERT(sizeof(BaeParamValue) == 40, "BaeParamValue");
BAE_STATIC_ASSERT(sizeof(BaeParamValues) == 16, "BaeParamValues");
BAE_STATIC_ASSERT(sizeof(BaeEffectDescription) == 40, "BaeEffectDescription");
BAE_STATIC_ASSERT(sizeof(BaeRenderInfo) == 120, "BaeRenderInfo");
BAE_STATIC_ASSERT(sizeof(BaeInputRequest) == 48, "BaeInputRequest");
BAE_STATIC_ASSERT(sizeof(BaeInputInfo) == 64, "BaeInputInfo");
BAE_STATIC_ASSERT(sizeof(BaePreRenderArgs) == 48, "BaePreRenderArgs");
BAE_STATIC_ASSERT(sizeof(BaePreRenderResult) == 48, "BaePreRenderResult");
BAE_STATIC_ASSERT(sizeof(BaeEffectVTable1) == 144, "BaeEffectVTable1");
BAE_STATIC_ASSERT(sizeof(BaeNativeGpuDevice) == 56, "BaeNativeGpuDevice");
BAE_STATIC_ASSERT(sizeof(BaeNativeGpuTexture) == 64, "BaeNativeGpuTexture");
BAE_STATIC_ASSERT(sizeof(BaeNativeGpuScope) == 96, "BaeNativeGpuScope");
#endif

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* BAE_PLUGIN_H */
