/*
 * abi_check.c - compile-time (and a little run-time) check of bae_plugin.h.
 *
 * Build: cc -std=c11 -Wall -Wextra -Wpedantic -Werror abi_check.c && ./a.out
 * (tests/check.sh does this for C11 and C17.)
 *
 * What breaks the build here is an ABI change:
 *   1. every struct's size, alignment and field offsets (64-bit targets, R1);
 *   2. the types of typedefs, of key fields and of the flag/version macros;
 *   3. the numeric values of all ABI constants (status codes, kinds, flags,
 *      pixel formats, ...) - renumbering any of them is an ABI break;
 *   4. every macro function (BAE_MAKE_VERSION, BAE_VERSION_MAJOR/MINOR,
 *      BAE_PIXEL_FORMAT*, BAE_GPU_FORMAT_BIT, BAE_HAS_FIELD);
 *   5. the signature of every plugin vtable callback and every host suite
 *      function: a dummy plugin exports one contribution of every kind and a
 *      dummy host implements every suite, all with designated initializers,
 *      so a renamed field or a changed signature does not compile.
 * main() then runs BAE_HAS_FIELD on truncated structs and performs a tiny
 * load handshake between the dummy host and the dummy plugin.
 *
 * When the header changes on purpose (minor ABI bump: appended fields), update
 * the expected numbers here in the same commit and say so in the PR.
 *
 * SPDX-License-Identifier: Apache-2.0
 */
#if !defined(__STDC_VERSION__) || __STDC_VERSION__ < 201112L
#error "abi_check.c needs C11 or later (_Static_assert, _Generic, _Alignof)"
#endif

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "../bae_plugin.h"

#if UINTPTR_MAX != UINT64_MAX
#error "BAE plugins are 64-bit only (rule R1)"
#endif

#define UNUSED(x) (void)(x)

#define SIZE(T, n) BAE_STATIC_ASSERT(sizeof(T) == (n), "sizeof(" #T ") != " #n)
#define ALIGN(T, n) BAE_STATIC_ASSERT(_Alignof(T) == (n), "_Alignof(" #T ") != " #n)
#define OFF(T, f, n) BAE_STATIC_ASSERT(offsetof(T, f) == (n), "offsetof(" #T ", " #f ") != " #n)
#define IS_TYPE(expr, T) \
    BAE_STATIC_ASSERT(_Generic((expr), T: 1, default: 0), "type of " #expr " is not " #T)
#define FIELD_TYPE(S, f, T) IS_TYPE(((S *)0)->f, T)
#define EQ(a, b) BAE_STATIC_ASSERT((a) == (b), #a " != " #b)

/* ======================================================================= */
/* 1. Versions (also usable in the preprocessor)                           */
/* ======================================================================= */

/* BAE_ABI_VERSION_MAJOR/MINOR are plain literals, so plugins can gate code on
   them with #if. (BAE_ABI_VERSION / BAE_MAKE_VERSION contain casts and are
   not usable in #if.) */
#if BAE_ABI_VERSION_MAJOR != 0u || BAE_ABI_VERSION_MINOR < 1u
#error "abi_check.c was written for ABI 0.1"
#endif

IS_TYPE(BAE_ABI_VERSION, uint32_t);
IS_TYPE(BAE_MAKE_VERSION(1, 2), uint32_t);
IS_TYPE(BAE_VERSION_MAJOR(0u), uint32_t);
IS_TYPE(BAE_VERSION_MINOR(0u), uint32_t);
EQ(BAE_ABI_VERSION, 0x00000001u);
EQ(BAE_MAKE_VERSION(0, 1), BAE_ABI_VERSION);
EQ(BAE_MAKE_VERSION(1, 0), 0x00010000u);
EQ(BAE_MAKE_VERSION(1, 2), 0x00010002u);
EQ(BAE_MAKE_VERSION(0xFFFF, 0xFFFF), 0xFFFFFFFFu);
EQ(BAE_MAKE_VERSION(0, 0x1FFFF), 0x0000FFFFu); /* minor is masked to 16 bits */
EQ(BAE_VERSION_MAJOR(BAE_MAKE_VERSION(3, 7)), 3u);
EQ(BAE_VERSION_MINOR(BAE_MAKE_VERSION(3, 7)), 7u);
EQ(BAE_VERSION_MAJOR(BAE_ABI_VERSION), BAE_ABI_VERSION_MAJOR);
EQ(BAE_VERSION_MINOR(BAE_ABI_VERSION), BAE_ABI_VERSION_MINOR);
/* Packed versions order like (major, minor) pairs. */
BAE_STATIC_ASSERT(BAE_MAKE_VERSION(1, 0) > BAE_MAKE_VERSION(0, 0xFFFF), "version ordering");
BAE_STATIC_ASSERT(BAE_MAKE_VERSION(2, 1) > BAE_MAKE_VERSION(2, 0), "version ordering");

/* ======================================================================= */
/* 2. Basic types and constant values                                      */
/* ======================================================================= */

IS_TYPE((BaeStatus)0, int32_t);
IS_TYPE((BaeBool)0, uint32_t);
IS_TYPE((BaeFlicks)0, int64_t);
IS_TYPE((BaePixelFormat)0, uint32_t);
IS_TYPE((BaeParamType)0, uint32_t);

/* Status codes. */
EQ(BAE_OK, 0);
EQ(BAE_STATUS_FALLBACK, 1);
EQ(BAE_ERR_GENERIC, -1);
EQ(BAE_ERR_INVALID_ARGUMENT, -2);
EQ(BAE_ERR_OUT_OF_MEMORY, -3);
EQ(BAE_ERR_UNSUPPORTED, -4);
EQ(BAE_ERR_VERSION_MISMATCH, -5);
EQ(BAE_ERR_NOT_FOUND, -6);
EQ(BAE_ERR_ABORTED, -7);
EQ(BAE_ERR_BAD_STATE, -8);
EQ(BAE_ERR_PERMISSION_DENIED, -9);
EQ(BAE_ERR_BUFFER_TOO_SMALL, -10);
EQ(BAE_ERR_IO, -11);
EQ(BAE_ERR_PARSE, -12);
EQ(BAE_ERR_GPU_DEVICE_LOST, -13);
EQ(BAE_ERR_GPU, -14);
EQ(BAE_ERR_PANIC, -15);
EQ(BAE_ERR_TIMEOUT, -16);
EQ(BAE_ERR_INTERNAL, -17);

EQ(BAE_FALSE, 0u);
EQ(BAE_TRUE, 1u);

/* Flicks: exact for common frame and sample rates, as the header promises. */
IS_TYPE(BAE_FLICKS_PER_SECOND, int64_t);
EQ(BAE_FLICKS_PER_SECOND, 705600000);
EQ(BAE_FLICKS_PER_SECOND % 24, 0);
EQ(BAE_FLICKS_PER_SECOND % 25, 0);
EQ(BAE_FLICKS_PER_SECOND % 30, 0);
EQ(BAE_FLICKS_PER_SECOND % 48, 0);
EQ(BAE_FLICKS_PER_SECOND % 50, 0);
EQ(BAE_FLICKS_PER_SECOND % 60, 0);
EQ(BAE_FLICKS_PER_SECOND % 120, 0);
EQ(BAE_FLICKS_PER_SECOND * 1001 % 24000, 0); /* 23.976 */
EQ(BAE_FLICKS_PER_SECOND * 1001 % 30000, 0); /* 29.97 */
EQ(BAE_FLICKS_PER_SECOND * 1001 % 60000, 0); /* 59.94 */
EQ(BAE_FLICKS_PER_SECOND % 44100, 0);
EQ(BAE_FLICKS_PER_SECOND % 48000, 0);
EQ(BAE_FLICKS_PER_SECOND % 96000, 0);

IS_TYPE(BAE_COORD_MIN, int32_t);
IS_TYPE(BAE_COORD_MAX, int32_t);
EQ(BAE_COORD_MIN, -1073741824);
EQ(BAE_COORD_MAX, 1073741824);

/* Host capabilities and contribution kinds. */
IS_TYPE(BAE_HOST_CAP_GPU_COMPUTE, uint64_t);
EQ(BAE_HOST_CAP_GPU_COMPUTE, 0x1u);
EQ(BAE_HOST_CAP_GPU_NATIVE, 0x2u);
EQ(BAE_HOST_CAP_OUT_OF_PROCESS, 0x4u);
EQ(BAE_HOST_CAP_HEADLESS, 0x8u);
EQ(BAE_KIND_EFFECT, 1u);
EQ(BAE_KIND_SHAPE_OPERATOR, 2u);
EQ(BAE_KIND_TEXT_SELECTOR, 3u);
EQ(BAE_KIND_IMPORTER, 4u);
EQ(BAE_KIND_EXPORTER, 5u);
EQ(BAE_KIND_DECODER, 6u);
EQ(BAE_KIND_ENCODER, 7u);
EQ(BAE_KIND_COMMAND, 8u);
EQ(BAE_KIND_PANEL, 9u);
EQ(BAE_KIND_EXPRESSION_LIBRARY, 10u);
EQ(BAE_KIND_AUDIO_EFFECT, 11u);

/* The exported symbol name and the entry point type. */
BAE_STATIC_ASSERT(sizeof BAE_PLUGIN_ENTRY_NAME == sizeof "bae_plugin_entry", "entry name");

/* ======================================================================= */
/* 3. Pixel formats                                                        */
/* ======================================================================= */

EQ(BAE_CHANNELS_RGBA, 1u);
EQ(BAE_CHANNELS_BGRA, 2u);
EQ(BAE_CHANNELS_ARGB, 3u);
EQ(BAE_COMPONENT_U8, 1u);
EQ(BAE_COMPONENT_U16, 2u);
EQ(BAE_COMPONENT_U16_15BIT, 3u);
EQ(BAE_COMPONENT_F16, 4u);
EQ(BAE_COMPONENT_F32, 5u);
EQ(BAE_ALPHA_PREMULTIPLIED, 1u);
EQ(BAE_ALPHA_STRAIGHT, 2u);

IS_TYPE(BAE_PIXEL_FORMAT(1u, 1u, 1u), BaePixelFormat);
EQ(BAE_PIXEL_FORMAT(BAE_CHANNELS_BGRA, BAE_COMPONENT_U16, BAE_ALPHA_STRAIGHT), 0x020202u);
EQ(BAE_PIXEL_FORMAT_RGBA_F16_PREMUL, 0x010401u);
EQ(BAE_PIXEL_FORMAT_RGBA_F32_PREMUL, 0x010501u);
EQ(BAE_PIXEL_FORMAT_RGBA_U8_PREMUL, 0x010101u);
EQ(BAE_PIXEL_FORMAT_BGRA_U8_PREMUL, 0x010102u);
EQ(BAE_PIXEL_FORMAT_ARGB_U8_PREMUL, 0x010103u);
EQ(BAE_PIXEL_FORMAT_ARGB_U16_15BIT_PREMUL, 0x010303u);
EQ(BAE_PIXEL_FORMAT_ARGB_F32_PREMUL, 0x010503u);

/* Round trip channels/component/alpha through every predefined format. */
#define PIXEL_FORMAT_ROUND_TRIP(f, ch, comp, al)              \
    EQ(BAE_PIXEL_FORMAT_CHANNELS(f), (ch));                   \
    EQ(BAE_PIXEL_FORMAT_COMPONENT(f), (comp));                \
    EQ(BAE_PIXEL_FORMAT_ALPHA(f), (al));                      \
    EQ(BAE_PIXEL_FORMAT((ch), (comp), (al)), (f))
PIXEL_FORMAT_ROUND_TRIP(BAE_PIXEL_FORMAT_RGBA_F16_PREMUL, BAE_CHANNELS_RGBA, BAE_COMPONENT_F16,
                        BAE_ALPHA_PREMULTIPLIED);
PIXEL_FORMAT_ROUND_TRIP(BAE_PIXEL_FORMAT_RGBA_F32_PREMUL, BAE_CHANNELS_RGBA, BAE_COMPONENT_F32,
                        BAE_ALPHA_PREMULTIPLIED);
PIXEL_FORMAT_ROUND_TRIP(BAE_PIXEL_FORMAT_RGBA_U8_PREMUL, BAE_CHANNELS_RGBA, BAE_COMPONENT_U8,
                        BAE_ALPHA_PREMULTIPLIED);
PIXEL_FORMAT_ROUND_TRIP(BAE_PIXEL_FORMAT_BGRA_U8_PREMUL, BAE_CHANNELS_BGRA, BAE_COMPONENT_U8,
                        BAE_ALPHA_PREMULTIPLIED);
PIXEL_FORMAT_ROUND_TRIP(BAE_PIXEL_FORMAT_ARGB_U8_PREMUL, BAE_CHANNELS_ARGB, BAE_COMPONENT_U8,
                        BAE_ALPHA_PREMULTIPLIED);
PIXEL_FORMAT_ROUND_TRIP(BAE_PIXEL_FORMAT_ARGB_U16_15BIT_PREMUL, BAE_CHANNELS_ARGB,
                        BAE_COMPONENT_U16_15BIT, BAE_ALPHA_PREMULTIPLIED);
PIXEL_FORMAT_ROUND_TRIP(BAE_PIXEL_FORMAT_ARGB_F32_PREMUL, BAE_CHANNELS_ARGB, BAE_COMPONENT_F32,
                        BAE_ALPHA_PREMULTIPLIED);
PIXEL_FORMAT_ROUND_TRIP(BAE_PIXEL_FORMAT(BAE_CHANNELS_RGBA, BAE_COMPONENT_U16,
                                         BAE_ALPHA_STRAIGHT),
                        BAE_CHANNELS_RGBA, BAE_COMPONENT_U16, BAE_ALPHA_STRAIGHT);

EQ(BAE_COLOR_SPACE_UNKNOWN, 0u);
EQ(BAE_COLOR_SPACE_SRGB, 1u);
EQ(BAE_COLOR_SPACE_LINEAR_SRGB, 2u);
EQ(BAE_COLOR_SPACE_ACESCG, 3u);
EQ(BAE_COLOR_SPACE_LINEAR_REC2020, 4u);
EQ(BAE_COLOR_SPACE_DISPLAY_P3, 5u);
EQ(BAE_IMAGE_FLAG_WRITABLE, 1u);

/* GPU formats and their mask bits (masks are uint32_t: formats must be < 32). */
EQ(BAE_GPU_FORMAT_RGBA16F, 1u);
EQ(BAE_GPU_FORMAT_RGBA32F, 2u);
EQ(BAE_GPU_FORMAT_RGBA8_UNORM, 3u);
EQ(BAE_GPU_FORMAT_BGRA8_UNORM, 4u);
EQ(BAE_GPU_FORMAT_R16F, 5u);
EQ(BAE_GPU_FORMAT_R32F, 6u);
EQ(BAE_GPU_FORMAT_RG16F, 7u);
EQ(BAE_GPU_FORMAT_RG32F, 8u);
IS_TYPE(BAE_GPU_FORMAT_BIT(BAE_GPU_FORMAT_RGBA16F), uint32_t);
EQ(BAE_GPU_FORMAT_BIT(BAE_GPU_FORMAT_RGBA16F), 0x002u);
EQ(BAE_GPU_FORMAT_BIT(BAE_GPU_FORMAT_RGBA32F), 0x004u);
EQ(BAE_GPU_FORMAT_BIT(BAE_GPU_FORMAT_RG32F), 0x100u);
EQ(BAE_GPU_FORMAT_BIT(BAE_GPU_FORMAT_RGBA16F) | BAE_GPU_FORMAT_BIT(BAE_GPU_FORMAT_RGBA32F),
   0x006u);
BAE_STATIC_ASSERT(BAE_GPU_FORMAT_RG32F < 32u, "GPU format masks are 32-bit");

/* ======================================================================= */
/* 4. Parameter, effect, render and suite constants                        */
/* ======================================================================= */

EQ(BAE_PARAM_FLOAT, 1u);
EQ(BAE_PARAM_INT, 2u);
EQ(BAE_PARAM_ANGLE, 3u);
EQ(BAE_PARAM_CHECKBOX, 4u);
EQ(BAE_PARAM_COLOR, 5u);
EQ(BAE_PARAM_POINT, 6u);
EQ(BAE_PARAM_POINT_3D, 7u);
EQ(BAE_PARAM_POPUP, 8u);
EQ(BAE_PARAM_LAYER, 9u);
EQ(BAE_PARAM_GROUP, 10u);
EQ(BAE_PARAM_ARBITRARY, 11u);
EQ(BAE_PARAM_PATH, 12u);
EQ(BAE_PARAM_BUTTON, 13u);
EQ(BAE_PARAM_STRING, 14u);
EQ(BAE_PARAM_ASSET, 15u);
EQ(BAE_PARAM_GRADIENT, 16u);
EQ(BAE_PARAM_CURVE, 17u);

IS_TYPE(BAE_PARAM_FLAG_HIDDEN, uint64_t);
IS_TYPE(BAE_PARAM_FLAG_RELATIVE_DEFAULT, uint64_t);
EQ(BAE_PARAM_FLAG_HIDDEN, 0x001u);
EQ(BAE_PARAM_FLAG_DISABLED, 0x002u);
EQ(BAE_PARAM_FLAG_SUPERVISE, 0x004u);
EQ(BAE_PARAM_FLAG_NOT_ANIMATABLE, 0x008u);
EQ(BAE_PARAM_FLAG_HOLD_INTERPOLATION, 0x010u);
EQ(BAE_PARAM_FLAG_START_COLLAPSED, 0x020u);
EQ(BAE_PARAM_FLAG_UI_ONLY, 0x040u);
EQ(BAE_PARAM_FLAG_NO_EXPRESSIONS, 0x080u);
EQ(BAE_PARAM_FLAG_TRANSITION_PROGRESS, 0x100u);
EQ(BAE_PARAM_FLAG_RELATIVE_DEFAULT, 0x200u);
EQ(BAE_DISPLAY_NUMBER, 0u);
EQ(BAE_DISPLAY_PERCENT, 1u);
EQ(BAE_DISPLAY_PIXELS, 2u);
EQ(BAE_DISPLAY_DEGREES, 3u);
EQ(BAE_DISPLAY_SECONDS, 4u);
EQ(BAE_DISPLAY_FRAMES, 5u);
EQ(BAE_LAYER_DEFAULT_NONE, 0u);
EQ(BAE_LAYER_DEFAULT_SELF, 1u);
EQ(BAE_COLOR_DEF_HAS_ALPHA, 1u);
EQ(BAE_CHOICE_FLAG_SEPARATOR, 1u);

EQ(BAE_EFFECT_CLASS_FILTER, 1u);
EQ(BAE_EFFECT_CLASS_GENERATOR, 2u);
EQ(BAE_EFFECT_CLASS_TRANSITION, 3u);
IS_TYPE(BAE_EFFECT_FLAG_PIXEL_INDEPENDENT, uint64_t);
IS_TYPE(BAE_EFFECT_FLAG_OPTIONS_DIALOG, uint64_t);
IS_TYPE(BAE_EFFECT_DYNAMIC_FLAGS_MASK, uint64_t);
EQ(BAE_EFFECT_FLAG_PIXEL_INDEPENDENT, 0x00001u);
EQ(BAE_EFFECT_FLAG_EXPANDS_BUFFER, 0x00002u);
EQ(BAE_EFFECT_FLAG_SHRINKS_BUFFER, 0x00004u);
EQ(BAE_EFFECT_FLAG_NON_PARAM_VARY, 0x00008u);
EQ(BAE_EFFECT_FLAG_WIDE_TIME_INPUT, 0x00010u);
EQ(BAE_EFFECT_FLAG_USES_SHUTTER, 0x00020u);
EQ(BAE_EFFECT_FLAG_USES_CAMERA, 0x00040u);
EQ(BAE_EFFECT_FLAG_USES_LIGHTS, 0x00080u);
EQ(BAE_EFFECT_FLAG_NON_DETERMINISTIC, 0x00100u);
EQ(BAE_EFFECT_FLAG_IN_PLACE, 0x00200u);
EQ(BAE_EFFECT_FLAG_STATEFUL, 0x00400u);
EQ(BAE_EFFECT_FLAG_SEND_UPDATE_PARAMS_UI, 0x00800u);
EQ(BAE_EFFECT_FLAG_DYNAMIC_FLAGS, 0x01000u);
EQ(BAE_EFFECT_FLAG_GPU_NATIVE, 0x02000u);
EQ(BAE_EFFECT_FLAG_EXTERNAL_DEPENDENCIES, 0x04000u);
EQ(BAE_EFFECT_FLAG_CUSTOM_UI, 0x08000u);
EQ(BAE_EFFECT_FLAG_OPTIONS_DIALOG, 0x10000u);
EQ(BAE_EFFECT_DYNAMIC_FLAGS_MASK, 0x001F9u);
EQ(BAE_THREADING_SERIAL_GLOBAL, 0u);
EQ(BAE_THREADING_SERIAL_INSTANCE, 1u);
EQ(BAE_THREADING_CONCURRENT, 2u);
EQ(BAE_DEVICE_CPU, 1u);
EQ(BAE_DEVICE_GPU, 2u);
EQ(BAE_INSTANCE_EDIT, 1u);
EQ(BAE_INSTANCE_RENDER, 2u);
EQ(BAE_QUALITY_DRAFT, 0u);
EQ(BAE_QUALITY_HIGH, 1u);
EQ(BAE_RENDER_FLAG_LINEAR_BLENDING, 1u);
EQ(BAE_RENDER_FLAG_FINAL, 2u);
EQ(BAE_RENDER_FLAG_PREVIEW, 4u);
EQ(BAE_RENDER_FLAG_HEADLESS, 8u);
EQ(BAE_INPUT_SELF, 0u);
EQ(BAE_INPUT_PARAM_LAYER, 1u);
EQ(BAE_INPUT_TRANSITION_TO, 2u);
EQ(BAE_INPUT_FLAG_WITHOUT_EFFECTS, 1u);
EQ(BAE_INPUT_INFO_EMPTY, 1u);
EQ(BAE_INPUT_INFO_SOLID, 2u);
EQ(BAE_PRERENDER_ARG_PRESERVE_RGB_OF_ZERO_ALPHA, 1u);
EQ(BAE_PRERENDER_SOLID, 1u);
EQ(BAE_PRERENDER_GPU_POSSIBLE, 2u);
EQ(BAE_PRERENDER_RETURNS_EXTRA_PIXELS, 4u);
EQ(BAE_PRERENDER_IDENTITY, 8u);
IS_TYPE(BAE_GPU_FEATURE_SHADER_F16, uint64_t);
EQ(BAE_GPU_FEATURE_SHADER_F16, 1u);
EQ(BAE_GPU_FEATURE_FLOAT32_FILTERABLE, 2u);
EQ(BAE_GPU_FEATURE_TIMESTAMP_QUERY, 4u);
EQ(BAE_GPU_BACKEND_NONE, 0u);
EQ(BAE_GPU_BACKEND_METAL, 1u);
EQ(BAE_GPU_BACKEND_D3D12, 2u);
EQ(BAE_GPU_BACKEND_VULKAN, 3u);
EQ(BAE_CHANGED_STATE, 1u);
EQ(BAE_CHANGED_RERENDER, 2u);
IS_TYPE(BAE_SHAPE_OP_FLAG_PER_PATH, uint64_t);
EQ(BAE_SHAPE_OP_FLAG_PER_PATH, 1u);
EQ(BAE_PLANAR_NONE, 0u);
EQ(BAE_PLANAR_NV12, 1u);
EQ(BAE_PLANAR_P010, 2u);
EQ(BAE_PLANAR_YUV420P, 3u);
EQ(BAE_PLANAR_YUV422P10, 4u);
EQ(BAE_PLANAR_YUV444P16, 5u);
EQ(BAE_MEDIA_HAS_VIDEO, 1u);
EQ(BAE_MEDIA_HAS_AUDIO, 2u);
EQ(BAE_LOG_ERROR, 1u);
EQ(BAE_LOG_WARN, 2u);
EQ(BAE_LOG_INFO, 3u);
EQ(BAE_LOG_DEBUG, 4u);
EQ(BAE_LOG_TRACE, 5u);
EQ(BAE_PATH_PLUGIN_DIR, 1u);
EQ(BAE_PATH_DATA_DIR, 2u);
EQ(BAE_PATH_CACHE_DIR, 3u);
EQ(BAE_PATH_TEMP_DIR, 4u);
EQ(BAE_BINDING_UNIFORM, 1u);
EQ(BAE_BINDING_SAMPLED_TEXTURE, 2u);
EQ(BAE_BINDING_STORAGE_TEXTURE_WRITE, 3u);
EQ(BAE_BINDING_STORAGE_BUFFER, 4u);
EQ(BAE_BINDING_STORAGE_BUFFER_READ, 5u);
EQ(BAE_BINDING_SAMPLER_LINEAR, 6u);
EQ(BAE_BINDING_SAMPLER_NEAREST, 7u);
EQ(BAE_GPU_USAGE_SAMPLED, 1u);
EQ(BAE_GPU_USAGE_STORAGE, 2u);

/* Suite (name, version) pairs are frozen once released (R4). */
EQ(BAE_SUITE_CORE_VERSION, 1u);
EQ(BAE_SUITE_PARAM_DEFINE_VERSION, 1u);
EQ(BAE_SUITE_PARAM_EDIT_VERSION, 1u);
EQ(BAE_SUITE_RENDER_VERSION, 1u);
EQ(BAE_SUITE_GPU_COMPUTE_VERSION, 1u);
EQ(BAE_SUITE_GPU_NATIVE_VERSION, 1u);
EQ(BAE_SUITE_PATH_VERSION, 1u);
EQ(BAE_SUITE_COMMAND_VERSION, 1u);

/* ======================================================================= */
/* 5. Layout: size, alignment, offset of every field                       */
/* ======================================================================= */

/* Basic types */
SIZE(BaeRational, 8); ALIGN(BaeRational, 4);
OFF(BaeRational, num, 0); OFF(BaeRational, den, 4);
FIELD_TYPE(BaeRational, num, int32_t);
SIZE(BaeRect, 16); ALIGN(BaeRect, 4);
OFF(BaeRect, x0, 0); OFF(BaeRect, y0, 4); OFF(BaeRect, x1, 8); OFF(BaeRect, y1, 12);
FIELD_TYPE(BaeRect, x0, int32_t);
SIZE(BaeStr, 16); OFF(BaeStr, ptr, 0); OFF(BaeStr, len, 8);
FIELD_TYPE(BaeStr, ptr, const char *); FIELD_TYPE(BaeStr, len, size_t);
SIZE(BaeBytes, 16); OFF(BaeBytes, ptr, 0); OFF(BaeBytes, len, 8);
SIZE(BaeWriter, 16); OFF(BaeWriter, opaque, 0); OFF(BaeWriter, write, 8);

/* Host and exports */
SIZE(BaeHost, 64); ALIGN(BaeHost, 8);
OFF(BaeHost, struct_size, 0); OFF(BaeHost, abi_version, 4); OFF(BaeHost, capabilities, 8);
OFF(BaeHost, host_name, 16); OFF(BaeHost, host_version, 24); OFF(BaeHost, plugin_id, 32);
OFF(BaeHost, plugin_dir, 40); OFF(BaeHost, handle, 48); OFF(BaeHost, get_suite, 56);
FIELD_TYPE(BaeHost, struct_size, uint32_t); FIELD_TYPE(BaeHost, capabilities, uint64_t);
SIZE(BaeContribution, 32);
OFF(BaeContribution, kind, 4); OFF(BaeContribution, id, 8);
OFF(BaeContribution, vtable_version, 16); OFF(BaeContribution, reserved, 20);
OFF(BaeContribution, vtable, 24);
SIZE(BaePluginExports, 72);
OFF(BaePluginExports, abi_built, 4); OFF(BaePluginExports, abi_required, 8);
OFF(BaePluginExports, reserved0, 12); OFF(BaePluginExports, plugin_id, 16);
OFF(BaePluginExports, plugin_version, 24); OFF(BaePluginExports, plugin_data, 32);
OFF(BaePluginExports, contribution_count, 40); OFF(BaePluginExports, reserved1, 44);
OFF(BaePluginExports, contributions, 48); OFF(BaePluginExports, shutdown, 56);
OFF(BaePluginExports, get_suite, 64);

/* Images */
SIZE(BaeCpuImage, 48);
OFF(BaeCpuImage, format, 4); OFF(BaeCpuImage, rect, 8); OFF(BaeCpuImage, row_bytes, 24);
OFF(BaeCpuImage, data, 32); OFF(BaeCpuImage, flags, 40); OFF(BaeCpuImage, reserved, 44);
FIELD_TYPE(BaeCpuImage, format, BaePixelFormat); FIELD_TYPE(BaeCpuImage, row_bytes, int64_t);
SIZE(BaeGpuImageInfo, 40);
OFF(BaeGpuImageInfo, gpu_format, 4); OFF(BaeGpuImageInfo, rect, 8);
OFF(BaeGpuImageInfo, width, 24); OFF(BaeGpuImageInfo, height, 28);
OFF(BaeGpuImageInfo, alpha_mode, 32); OFF(BaeGpuImageInfo, reserved, 36);

/* Parameter definitions: every union member must fit the fixed 128 bytes. */
SIZE(BaeParamFloatDef, 48);
OFF(BaeParamFloatDef, default_value, 0); OFF(BaeParamFloatDef, valid_min, 8);
OFF(BaeParamFloatDef, valid_max, 16); OFF(BaeParamFloatDef, slider_min, 24);
OFF(BaeParamFloatDef, slider_max, 32); OFF(BaeParamFloatDef, precision, 40);
OFF(BaeParamFloatDef, display, 44);
SIZE(BaeParamIntDef, 24);
OFF(BaeParamIntDef, valid_min, 4); OFF(BaeParamIntDef, valid_max, 8);
OFF(BaeParamIntDef, slider_min, 12); OFF(BaeParamIntDef, slider_max, 16);
OFF(BaeParamIntDef, display, 20);
SIZE(BaeParamAngleDef, 8);
SIZE(BaeParamCheckboxDef, 16); OFF(BaeParamCheckboxDef, label, 8);
SIZE(BaeParamColorDef, 24); OFF(BaeParamColorDef, flags, 16);
SIZE(BaeParamPointDef, 32); OFF(BaeParamPointDef, reserved, 24);
SIZE(BaePopupChoice, 24);
OFF(BaePopupChoice, flags, 4); OFF(BaePopupChoice, name, 8); OFF(BaePopupChoice, l10n_key, 16);
SIZE(BaeParamPopupDef, 16);
OFF(BaeParamPopupDef, choice_count, 4); OFF(BaeParamPopupDef, choices, 8);
SIZE(BaeParamLayerDef, 8);
SIZE(BaeArbitraryVTable1, 40);
OFF(BaeArbitraryVTable1, interpolate, 8); OFF(BaeArbitraryVTable1, compare, 16);
OFF(BaeArbitraryVTable1, to_text, 24); OFF(BaeArbitraryVTable1, from_text, 32);
SIZE(BaeParamArbitraryDef, 24); OFF(BaeParamArbitraryDef, default_value, 8);
SIZE(BaeParamStringDef, 16);
OFF(BaeParamStringDef, max_length, 8); OFF(BaeParamStringDef, multiline, 12);
SIZE(BaeParamButtonDef, 8);
SIZE(BaeParamAssetDef, 8);
SIZE(BaeParamDef, 176); ALIGN(BaeParamDef, 8);
OFF(BaeParamDef, type, 4); OFF(BaeParamDef, id, 8); OFF(BaeParamDef, group_id, 12);
OFF(BaeParamDef, key, 16); OFF(BaeParamDef, name, 24); OFF(BaeParamDef, l10n_key, 32);
OFF(BaeParamDef, flags, 40); OFF(BaeParamDef, u, 48);
OFF(BaeParamDef, u.float_def.display, 48 + 44);
FIELD_TYPE(BaeParamDef, flags, uint64_t); FIELD_TYPE(BaeParamDef, type, BaeParamType);
BAE_STATIC_ASSERT(sizeof(((BaeParamDef *)0)->u) == 128, "BaeParamDef.u is 128 bytes");
BAE_STATIC_ASSERT(sizeof(BaeParamFloatDef) <= 128 && sizeof(BaeParamIntDef) <= 128 &&
                      sizeof(BaeParamCheckboxDef) <= 128 && sizeof(BaeParamColorDef) <= 128 &&
                      sizeof(BaeParamPointDef) <= 128 && sizeof(BaeParamPopupDef) <= 128 &&
                      sizeof(BaeParamArbitraryDef) <= 128 && sizeof(BaeParamStringDef) <= 128,
                  "a param definition outgrew the BaeParamDef union");

/* Parameter values */
SIZE(BaeLayerRef, 16); OFF(BaeLayerRef, flags, 8);
SIZE(BaeParamValue, 40); OFF(BaeParamValue, type, 4); OFF(BaeParamValue, v, 8);
BAE_STATIC_ASSERT(sizeof(((BaeParamValue *)0)->v) == 32, "BaeParamValue.v is 32 bytes");
FIELD_TYPE(BaeParamValue, v.f, double); FIELD_TYPE(BaeParamValue, v.i, int32_t);
FIELD_TYPE(BaeParamValue, v.b, BaeBool); FIELD_TYPE(BaeParamValue, v.choice, uint32_t);
FIELD_TYPE(BaeParamValue, v.handle, uint64_t);
SIZE(BaeParamValues, 16); OFF(BaeParamValues, values, 8);

/* Effects */
SIZE(BaeEffectDescription, 40);
OFF(BaeEffectDescription, effect_class, 4); OFF(BaeEffectDescription, flags, 8);
OFF(BaeEffectDescription, threading, 16); OFF(BaeEffectDescription, devices, 20);
OFF(BaeEffectDescription, gpu_formats, 24); OFF(BaeEffectDescription, cpu_format_count, 28);
OFF(BaeEffectDescription, cpu_formats, 32);
FIELD_TYPE(BaeEffectDescription, flags, uint64_t);
SIZE(BaeGlobalSetupArgs, 32);
OFF(BaeGlobalSetupArgs, host, 8); OFF(BaeGlobalSetupArgs, contribution_id, 16);
OFF(BaeGlobalSetupArgs, locale, 24);
SIZE(BaeInstanceArgs, 40);
OFF(BaeInstanceArgs, purpose, 4); OFF(BaeInstanceArgs, state, 8);
OFF(BaeInstanceArgs, state_plugin_version, 24); OFF(BaeInstanceArgs, instance_uid, 32);
SIZE(BaeRenderInfo, 120); ALIGN(BaeRenderInfo, 8);
OFF(BaeRenderInfo, quality, 4); OFF(BaeRenderInfo, time, 8); OFF(BaeRenderInfo, time_step, 16);
OFF(BaeRenderInfo, frame_rate, 24); OFF(BaeRenderInfo, downsample_x, 32);
OFF(BaeRenderInfo, downsample_y, 40); OFF(BaeRenderInfo, pixel_aspect, 48);
OFF(BaeRenderInfo, shutter_angle, 56); OFF(BaeRenderInfo, shutter_phase, 60);
OFF(BaeRenderInfo, color_space, 64); OFF(BaeRenderInfo, flags, 68);
OFF(BaeRenderInfo, layer_bounds, 72); OFF(BaeRenderInfo, render_id, 88);
OFF(BaeRenderInfo, params, 96); OFF(BaeRenderInfo, layer_duration, 112);
FIELD_TYPE(BaeRenderInfo, time, BaeFlicks); FIELD_TYPE(BaeRenderInfo, shutter_angle, float);
FIELD_TYPE(BaeRenderInfo, layer_duration, BaeFlicks);
SIZE(BaeInputRequest, 48);
OFF(BaeInputRequest, source, 4); OFF(BaeInputRequest, param_id, 8);
OFF(BaeInputRequest, checkout_id, 12); OFF(BaeInputRequest, time, 16);
OFF(BaeInputRequest, rect, 24); OFF(BaeInputRequest, flags, 40);
SIZE(BaeInputInfo, 64);
OFF(BaeInputInfo, flags, 4); OFF(BaeInputInfo, result_rect, 8);
OFF(BaeInputInfo, max_result_rect, 24); OFF(BaeInputInfo, layer_bounds, 40);
OFF(BaeInputInfo, pixel_aspect, 56);
SIZE(BaePreRenderArgs, 48);
OFF(BaePreRenderArgs, device, 4); OFF(BaePreRenderArgs, info, 8); OFF(BaePreRenderArgs, ctx, 16);
OFF(BaePreRenderArgs, request_rect, 24); OFF(BaePreRenderArgs, flags, 40);
SIZE(BaePreRenderResult, 48);
OFF(BaePreRenderResult, flags, 4); OFF(BaePreRenderResult, result_rect, 8);
OFF(BaePreRenderResult, max_result_rect, 24); OFF(BaePreRenderResult, frame_data, 40);
SIZE(BaeCpuRenderArgs, 40);
OFF(BaeCpuRenderArgs, info, 8); OFF(BaeCpuRenderArgs, ctx, 16);
OFF(BaeCpuRenderArgs, frame_data, 24); OFF(BaeCpuRenderArgs, output, 32);
SIZE(BaeGpuRenderArgs, 40);
OFF(BaeGpuRenderArgs, info, 8); OFF(BaeGpuRenderArgs, ctx, 16);
OFF(BaeGpuRenderArgs, frame_data, 24); OFF(BaeGpuRenderArgs, output, 32);
SIZE(BaeGpuDeviceArgs, 32);
OFF(BaeGpuDeviceArgs, backend, 4); OFF(BaeGpuDeviceArgs, ctx, 8);
OFF(BaeGpuDeviceArgs, features, 16); OFF(BaeGpuDeviceArgs, device_index, 24);
SIZE(BaeParamChangedArgs, 32);
OFF(BaeParamChangedArgs, param_id, 4); OFF(BaeParamChangedArgs, time, 8);
OFF(BaeParamChangedArgs, values, 16); OFF(BaeParamChangedArgs, editor, 24);
SIZE(BaeParamUiArgs, 32);
OFF(BaeParamUiArgs, time, 8); OFF(BaeParamUiArgs, values, 16); OFF(BaeParamUiArgs, editor, 24);

/* Effect vtable: one pointer per callback, in this order, forever (R5). */
SIZE(BaeEffectVTable1, 144); ALIGN(BaeEffectVTable1, 8);
OFF(BaeEffectVTable1, struct_size, 0); OFF(BaeEffectVTable1, reserved, 4);
OFF(BaeEffectVTable1, global_setup, 8); OFF(BaeEffectVTable1, global_setdown, 16);
OFF(BaeEffectVTable1, define_params, 24); OFF(BaeEffectVTable1, gpu_device_setup, 32);
OFF(BaeEffectVTable1, gpu_device_setdown, 40); OFF(BaeEffectVTable1, instance_create, 48);
OFF(BaeEffectVTable1, instance_serialize, 56); OFF(BaeEffectVTable1, instance_destroy, 64);
OFF(BaeEffectVTable1, param_changed, 72); OFF(BaeEffectVTable1, update_params_ui, 80);
OFF(BaeEffectVTable1, query_dynamic_flags, 88); OFF(BaeEffectVTable1, pre_render, 96);
OFF(BaeEffectVTable1, render_cpu, 104); OFF(BaeEffectVTable1, render_gpu, 112);
OFF(BaeEffectVTable1, release_frame_data, 120); OFF(BaeEffectVTable1, get_dependencies, 128);
OFF(BaeEffectVTable1, message, 136);

/* Shape operators */
SIZE(BaePathVertex, 48);
OFF(BaePathVertex, y, 8); OFF(BaePathVertex, in_x, 16); OFF(BaePathVertex, in_y, 24);
OFF(BaePathVertex, out_x, 32); OFF(BaePathVertex, out_y, 40);
SIZE(BaePath, 16); OFF(BaePath, closed, 4); OFF(BaePath, vertices, 8);
SIZE(BaePathSet, 16); OFF(BaePathSet, paths, 8);
SIZE(BaeShapeApplyArgs, 56);
OFF(BaeShapeApplyArgs, time, 8); OFF(BaeShapeApplyArgs, params, 16);
OFF(BaeShapeApplyArgs, input, 32); OFF(BaeShapeApplyArgs, sink, 48);
SIZE(BaeShapeOperatorVTable1, 40);
OFF(BaeShapeOperatorVTable1, global_setup, 8); OFF(BaeShapeOperatorVTable1, global_setdown, 16);
OFF(BaeShapeOperatorVTable1, define_params, 24); OFF(BaeShapeOperatorVTable1, apply, 32);

/* Importers, exporters, commands */
SIZE(BaeImportArgs, 48);
OFF(BaeImportArgs, path, 8); OFF(BaeImportArgs, options_json, 24); OFF(BaeImportArgs, ctx, 40);
SIZE(BaeImporterVTable1, 24);
OFF(BaeImporterVTable1, probe, 8); OFF(BaeImporterVTable1, run, 16);
SIZE(BaeExportArgs, 64);
OFF(BaeExportArgs, path, 8); OFF(BaeExportArgs, options_json, 24);
OFF(BaeExportArgs, target_prim, 40); OFF(BaeExportArgs, ctx, 56);
SIZE(BaeExporterVTable1, 16); OFF(BaeExporterVTable1, run, 8);
SIZE(BaeCommandArgs, 40);
OFF(BaeCommandArgs, args_json, 8); OFF(BaeCommandArgs, ctx, 24);
OFF(BaeCommandArgs, result_json, 32);
SIZE(BaeCommandVTable1, 24);
OFF(BaeCommandVTable1, execute, 8); OFF(BaeCommandVTable1, can_execute, 16);

/* Codecs */
SIZE(BaeColorInfo, 16); ALIGN(BaeColorInfo, 4);
OFF(BaeColorInfo, transfer, 4); OFF(BaeColorInfo, matrix, 8); OFF(BaeColorInfo, full_range, 12);
SIZE(BaeVideoStreamInfo, 64);
OFF(BaeVideoStreamInfo, has_alpha, 4); OFF(BaeVideoStreamInfo, width, 8);
OFF(BaeVideoStreamInfo, height, 12); OFF(BaeVideoStreamInfo, frame_rate, 16);
OFF(BaeVideoStreamInfo, pixel_aspect, 24); OFF(BaeVideoStreamInfo, duration, 32);
OFF(BaeVideoStreamInfo, color, 40); OFF(BaeVideoStreamInfo, planar_layout, 56);
SIZE(BaeAudioStreamInfo, 24);
OFF(BaeAudioStreamInfo, channels, 4); OFF(BaeAudioStreamInfo, sample_rate, 8);
OFF(BaeAudioStreamInfo, frame_count, 16);
SIZE(BaeMediaInfo, 96);
OFF(BaeMediaInfo, flags, 4); OFF(BaeMediaInfo, video, 8); OFF(BaeMediaInfo, audio, 72);
SIZE(BaePlanarImage, 96);
OFF(BaePlanarImage, layout, 4); OFF(BaePlanarImage, width, 8); OFF(BaePlanarImage, height, 12);
OFF(BaePlanarImage, color, 16); OFF(BaePlanarImage, planes, 32);
OFF(BaePlanarImage, row_bytes, 64);
SIZE(BaeDecoderOpenArgs, 40);
OFF(BaeDecoderOpenArgs, path, 8); OFF(BaeDecoderOpenArgs, options_json, 24);
SIZE(BaeDecodeVideoArgs, 32);
OFF(BaeDecodeVideoArgs, time, 8); OFF(BaeDecodeVideoArgs, rgba, 16);
OFF(BaeDecodeVideoArgs, planar, 24);
SIZE(BaeDecoderVTable1, 56);
OFF(BaeDecoderVTable1, probe, 8); OFF(BaeDecoderVTable1, open, 16);
OFF(BaeDecoderVTable1, get_info, 24); OFF(BaeDecoderVTable1, decode_video, 32);
OFF(BaeDecoderVTable1, decode_audio, 40); OFF(BaeDecoderVTable1, close, 48);
SIZE(BaeEncoderOpenArgs, 128);
OFF(BaeEncoderOpenArgs, path, 8); OFF(BaeEncoderOpenArgs, settings_json, 24);
OFF(BaeEncoderOpenArgs, video, 40); OFF(BaeEncoderOpenArgs, audio, 104);
SIZE(BaeEncoderVTable1, 48);
OFF(BaeEncoderVTable1, open, 8); OFF(BaeEncoderVTable1, encode_video, 16);
OFF(BaeEncoderVTable1, encode_audio, 24); OFF(BaeEncoderVTable1, finish, 32);
OFF(BaeEncoderVTable1, close, 40);

/* Expression libraries */
SIZE(BaeExprValue, 40); OFF(BaeExprValue, v, 8);
SIZE(BaeExprFunction, 32);
OFF(BaeExprFunction, min_args, 8); OFF(BaeExprFunction, max_args, 12);
OFF(BaeExprFunction, flags, 16); OFF(BaeExprFunction, call, 24);
SIZE(BaeExpressionLibraryVTable1, 16);
OFF(BaeExpressionLibraryVTable1, function_count, 4);
OFF(BaeExpressionLibraryVTable1, functions, 8);

/* Host suites: plain tables of function pointers, frozen per version (R4). */
SIZE(BaeCoreSuite1, 48);
OFF(BaeCoreSuite1, log, 0); OFF(BaeCoreSuite1, set_error, 8); OFF(BaeCoreSuite1, alloc, 16);
OFF(BaeCoreSuite1, free, 24); OFF(BaeCoreSuite1, has_permission, 32);
OFF(BaeCoreSuite1, get_path, 40);
SIZE(BaeParamDefineSuite1, 8);
SIZE(BaeParamEditSuite1, 16); OFF(BaeParamEditSuite1, set_ui, 8);
SIZE(BaeRenderSuite1, 64);
OFF(BaeRenderSuite1, checkout_param, 8); OFF(BaeRenderSuite1, get_input_cpu, 16);
OFF(BaeRenderSuite1, get_input_gpu, 24); OFF(BaeRenderSuite1, is_aborted, 32);
OFF(BaeRenderSuite1, report_progress, 40); OFF(BaeRenderSuite1, alloc_cpu_image, 48);
OFF(BaeRenderSuite1, parallel_for, 56);
SIZE(BaeGpuBindingLayout, 16);
OFF(BaeGpuBindingLayout, type, 4); OFF(BaeGpuBindingLayout, gpu_format, 8);
SIZE(BaeComputePipelineDesc, 48);
OFF(BaeComputePipelineDesc, binding_count, 4); OFF(BaeComputePipelineDesc, wgsl_source, 8);
OFF(BaeComputePipelineDesc, entry_point, 24); OFF(BaeComputePipelineDesc, bindings, 32);
OFF(BaeComputePipelineDesc, label, 40);
SIZE(BaeGpuBinding, 40);
OFF(BaeGpuBinding, type, 4); OFF(BaeGpuBinding, image, 8); OFF(BaeGpuBinding, buffer, 16);
OFF(BaeGpuBinding, data, 24);
SIZE(BaeGpuComputeSuite1, 64);
OFF(BaeGpuComputeSuite1, destroy_pipeline, 8); OFF(BaeGpuComputeSuite1, get_image_info, 16);
OFF(BaeGpuComputeSuite1, acquire_image, 24); OFF(BaeGpuComputeSuite1, acquire_buffer, 32);
OFF(BaeGpuComputeSuite1, dispatch, 40); OFF(BaeGpuComputeSuite1, copy_image, 48);
OFF(BaeGpuComputeSuite1, clear_image, 56);
SIZE(BaeNativeGpuDevice, 56);
OFF(BaeNativeGpuDevice, backend, 4); OFF(BaeNativeGpuDevice, instance, 8);
OFF(BaeNativeGpuDevice, physical_device, 16); OFF(BaeNativeGpuDevice, device, 24);
OFF(BaeNativeGpuDevice, queue, 32); OFF(BaeNativeGpuDevice, queue_family_index, 40);
OFF(BaeNativeGpuDevice, device_index, 44); OFF(BaeNativeGpuDevice, adapter_luid, 48);
SIZE(BaeNativeImageRequest, 16);
OFF(BaeNativeImageRequest, usage, 8);
SIZE(BaeNativeGpuTexture, 64);
OFF(BaeNativeGpuTexture, backend, 4); OFF(BaeNativeGpuTexture, texture, 8);
OFF(BaeNativeGpuTexture, view, 16); OFF(BaeNativeGpuTexture, native_format, 24);
OFF(BaeNativeGpuTexture, state, 28); OFF(BaeNativeGpuTexture, width, 32);
OFF(BaeNativeGpuTexture, height, 36); OFF(BaeNativeGpuTexture, rect, 40);
OFF(BaeNativeGpuTexture, usage, 56);
SIZE(BaeNativeGpuScope, 96);
OFF(BaeNativeGpuScope, device, 8); OFF(BaeNativeGpuScope, wait_fence, 64);
OFF(BaeNativeGpuScope, wait_value, 72); OFF(BaeNativeGpuScope, signal_fence, 80);
OFF(BaeNativeGpuScope, signal_value, 88);
SIZE(BaeGpuNativeSuite1, 24);
OFF(BaeGpuNativeSuite1, begin, 8); OFF(BaeGpuNativeSuite1, end, 16);
SIZE(BaePathSuite1, 8);
SIZE(BaeCommandSuite1, 32);
OFF(BaeCommandSuite1, query, 8); OFF(BaeCommandSuite1, is_aborted, 16);
OFF(BaeCommandSuite1, report_progress, 24);

/* ======================================================================= */
/* 6. Dummy plugin: one contribution of every kind that has a vtable        */
/* ======================================================================= */
/* Every function below has exactly the signature of its vtable slot. The
   designated initializers fail to compile if a slot is renamed or its type
   changes (incompatible function pointer types are errors with -Werror). */

/* ---- writer used by the dummy callbacks ---- */
typedef struct ByteSink {
    char data[256];
    size_t len;
} ByteSink;

static BaeStatus sink_write(void *opaque, const void *data, size_t len)
{
    ByteSink *sink = (ByteSink *)opaque;
    if (len > sizeof sink->data - sink->len) {
        return BAE_ERR_BUFFER_TOO_SMALL;
    }
    memcpy(sink->data + sink->len, data, len);
    sink->len += len;
    return BAE_OK;
}

/* ---- ARBITRARY param callbacks ---- */
static BaeStatus arb_interpolate(void *global, BaeBytes a, BaeBytes b, double t, BaeWriter *out)
{
    UNUSED(global);
    UNUSED(b);
    UNUSED(t);
    return out->write(out->opaque, a.ptr, a.len);
}
static int32_t arb_compare(void *global, BaeBytes a, BaeBytes b)
{
    UNUSED(global);
    return a.len == b.len ? 0 : (a.len < b.len ? -1 : 1);
}
static BaeStatus arb_to_text(void *global, BaeBytes value, BaeWriter *out_utf8)
{
    UNUSED(global);
    UNUSED(value);
    return out_utf8->write(out_utf8->opaque, "[]", 2);
}
static BaeStatus arb_from_text(void *global, BaeStr text, BaeWriter *out_bytes)
{
    UNUSED(global);
    return out_bytes->write(out_bytes->opaque, text.ptr, text.len);
}
static const BaeArbitraryVTable1 k_arbitrary_vtable = {
    .struct_size = (uint32_t)sizeof(BaeArbitraryVTable1),
    .reserved = 0u,
    .interpolate = arb_interpolate,
    .compare = arb_compare,
    .to_text = arb_to_text,
    .from_text = arb_from_text,
};

/* ---- effect ---- */
static const BaePixelFormat k_formats[] = { BAE_PIXEL_FORMAT_RGBA_F32_PREMUL };

static BaeStatus fx_global_setup(void *plugin_data, const BaeGlobalSetupArgs *args,
                                 BaeEffectDescription *out_desc, void **out_global)
{
    UNUSED(args);
    if (!BAE_HAS_FIELD(out_desc, BaeEffectDescription, cpu_formats)) {
        return BAE_ERR_VERSION_MISMATCH;
    }
    out_desc->effect_class = BAE_EFFECT_CLASS_FILTER;
    out_desc->flags = BAE_EFFECT_FLAG_PIXEL_INDEPENDENT;
    out_desc->threading = BAE_THREADING_CONCURRENT;
    out_desc->devices = BAE_DEVICE_CPU | BAE_DEVICE_GPU;
    out_desc->gpu_formats = BAE_GPU_FORMAT_BIT(BAE_GPU_FORMAT_RGBA16F);
    out_desc->cpu_format_count = 1u;
    out_desc->cpu_formats = k_formats;
    *out_global = plugin_data;
    return BAE_OK;
}
static void fx_global_setdown(void *global) { UNUSED(global); }
static BaeStatus fx_define_params(void *global, BaeParamBuilder *builder)
{
    UNUSED(global);
    UNUSED(builder);
    return BAE_OK;
}
static BaeStatus fx_gpu_device_setup(void *global, const BaeGpuDeviceArgs *args,
                                     void **out_gpu_data)
{
    UNUSED(global);
    UNUSED(args);
    *out_gpu_data = NULL;
    return BAE_OK;
}
static void fx_gpu_device_setdown(void *global, const BaeGpuDeviceArgs *args, void *gpu_data)
{
    UNUSED(global);
    UNUSED(args);
    UNUSED(gpu_data);
}
static BaeStatus fx_instance_create(void *global, const BaeInstanceArgs *args,
                                    void **out_instance)
{
    UNUSED(global);
    UNUSED(args);
    *out_instance = NULL;
    return BAE_OK;
}
static BaeStatus fx_instance_serialize(void *global, const void *instance, BaeWriter *out)
{
    UNUSED(global);
    UNUSED(instance);
    UNUSED(out);
    return BAE_OK;
}
static void fx_instance_destroy(void *global, void *instance)
{
    UNUSED(global);
    UNUSED(instance);
}
static BaeStatus fx_param_changed(void *global, void *instance, const BaeParamChangedArgs *args,
                                  uint32_t *out_flags)
{
    UNUSED(global);
    UNUSED(instance);
    UNUSED(args);
    *out_flags |= BAE_CHANGED_RERENDER;
    return BAE_OK;
}
static BaeStatus fx_update_params_ui(void *global, const void *instance,
                                     const BaeParamUiArgs *args)
{
    UNUSED(global);
    UNUSED(instance);
    UNUSED(args);
    return BAE_OK;
}
static BaeStatus fx_query_dynamic_flags(void *global, const void *instance,
                                        const BaeParamValues *values, uint64_t *inout_flags)
{
    UNUSED(global);
    UNUSED(instance);
    UNUSED(values);
    *inout_flags &= ~(uint64_t)BAE_EFFECT_FLAG_NON_PARAM_VARY;
    return BAE_OK;
}
static BaeStatus fx_pre_render(void *global, const void *instance, const BaePreRenderArgs *args,
                               BaePreRenderResult *out_result)
{
    UNUSED(global);
    UNUSED(instance);
    out_result->flags = BAE_PRERENDER_IDENTITY;
    out_result->result_rect = args->request_rect;
    out_result->max_result_rect = args->request_rect;
    out_result->frame_data = NULL;
    return BAE_OK;
}
static BaeStatus fx_render_cpu(void *global, const void *instance, const BaeCpuRenderArgs *args)
{
    UNUSED(global);
    UNUSED(instance);
    UNUSED(args);
    return BAE_OK;
}
static BaeStatus fx_render_gpu(void *global, const void *instance, void *gpu_data,
                               const BaeGpuRenderArgs *args)
{
    UNUSED(global);
    UNUSED(instance);
    UNUSED(gpu_data);
    UNUSED(args);
    return BAE_STATUS_FALLBACK;
}
static void fx_release_frame_data(void *global, const void *instance, void *frame_data)
{
    UNUSED(global);
    UNUSED(instance);
    UNUSED(frame_data);
}
static BaeStatus fx_get_dependencies(void *global, const void *instance,
                                     const BaeParamValues *values, BaeWriter *out_json)
{
    UNUSED(global);
    UNUSED(instance);
    UNUSED(values);
    return out_json->write(out_json->opaque, "[]", 2);
}
static BaeStatus fx_message(void *global, void *instance, const char *selector, BaeBytes input,
                            BaeWriter *output)
{
    UNUSED(global);
    UNUSED(instance);
    UNUSED(input);
    if (strcmp(selector, BAE_MESSAGE_ABOUT) == 0) {
        return output->write(output->opaque, "abi_check", 9);
    }
    if (strcmp(selector, BAE_MESSAGE_OPTIONS_DIALOG) == 0 ||
        strcmp(selector, BAE_MESSAGE_MIGRATE) == 0) {
        return BAE_ERR_UNSUPPORTED;
    }
    return BAE_ERR_NOT_FOUND;
}
static const BaeEffectVTable1 k_effect_vtable = {
    .struct_size = (uint32_t)sizeof(BaeEffectVTable1),
    .reserved = 0u,
    .global_setup = fx_global_setup,
    .global_setdown = fx_global_setdown,
    .define_params = fx_define_params,
    .gpu_device_setup = fx_gpu_device_setup,
    .gpu_device_setdown = fx_gpu_device_setdown,
    .instance_create = fx_instance_create,
    .instance_serialize = fx_instance_serialize,
    .instance_destroy = fx_instance_destroy,
    .param_changed = fx_param_changed,
    .update_params_ui = fx_update_params_ui,
    .query_dynamic_flags = fx_query_dynamic_flags,
    .pre_render = fx_pre_render,
    .render_cpu = fx_render_cpu,
    .render_gpu = fx_render_gpu,
    .release_frame_data = fx_release_frame_data,
    .get_dependencies = fx_get_dependencies,
    .message = fx_message,
};

/* ---- shape operator ---- */
static BaeStatus so_global_setup(void *plugin_data, const BaeGlobalSetupArgs *args,
                                 uint64_t *out_flags, void **out_global)
{
    UNUSED(args);
    *out_flags = BAE_SHAPE_OP_FLAG_PER_PATH;
    *out_global = plugin_data;
    return BAE_OK;
}
static void so_global_setdown(void *global) { UNUSED(global); }
static BaeStatus so_define_params(void *global, BaeParamBuilder *builder)
{
    UNUSED(global);
    UNUSED(builder);
    return BAE_OK;
}
static BaeStatus so_apply(void *global, const BaeShapeApplyArgs *args)
{
    UNUSED(global);
    UNUSED(args);
    return BAE_OK;
}
static const BaeShapeOperatorVTable1 k_shape_vtable = {
    .struct_size = (uint32_t)sizeof(BaeShapeOperatorVTable1),
    .reserved = 0u,
    .global_setup = so_global_setup,
    .global_setdown = so_global_setdown,
    .define_params = so_define_params,
    .apply = so_apply,
};

/* ---- importer / exporter / command ---- */
static uint32_t imp_probe(void *plugin_data, BaeStr path, BaeBytes header)
{
    UNUSED(plugin_data);
    UNUSED(header);
    return path.len > 0u ? 50u : 0u;
}
static BaeStatus imp_run(void *plugin_data, const BaeImportArgs *args)
{
    UNUSED(plugin_data);
    UNUSED(args);
    return BAE_OK;
}
static const BaeImporterVTable1 k_importer_vtable = {
    .struct_size = (uint32_t)sizeof(BaeImporterVTable1),
    .reserved = 0u,
    .probe = imp_probe,
    .run = imp_run,
};
static BaeStatus exp_run(void *plugin_data, const BaeExportArgs *args)
{
    UNUSED(plugin_data);
    UNUSED(args);
    return BAE_OK;
}
static const BaeExporterVTable1 k_exporter_vtable = {
    .struct_size = (uint32_t)sizeof(BaeExporterVTable1),
    .reserved = 0u,
    .run = exp_run,
};
static BaeStatus cmd_execute(void *plugin_data, const BaeCommandArgs *args)
{
    UNUSED(plugin_data);
    return args->result_json->write(args->result_json->opaque, "{}", 2);
}
static BaeBool cmd_can_execute(void *plugin_data, BaeCommandCtx *ctx)
{
    UNUSED(plugin_data);
    UNUSED(ctx);
    return BAE_TRUE;
}
static const BaeCommandVTable1 k_command_vtable = {
    .struct_size = (uint32_t)sizeof(BaeCommandVTable1),
    .reserved = 0u,
    .execute = cmd_execute,
    .can_execute = cmd_can_execute,
};

/* ---- decoder / encoder ---- */
static uint32_t dec_probe(void *plugin_data, BaeStr path, BaeBytes header)
{
    UNUSED(plugin_data);
    UNUSED(path);
    return header.len >= 4u ? 10u : 0u;
}
static BaeStatus dec_open(void *plugin_data, const BaeDecoderOpenArgs *args, void **out_handle)
{
    UNUSED(args);
    *out_handle = plugin_data;
    return BAE_OK;
}
static BaeStatus dec_get_info(void *handle, BaeMediaInfo *out_info)
{
    UNUSED(handle);
    out_info->flags = BAE_MEDIA_HAS_VIDEO;
    return BAE_OK;
}
static BaeStatus dec_decode_video(void *handle, const BaeDecodeVideoArgs *args)
{
    UNUSED(handle);
    UNUSED(args);
    return BAE_OK;
}
static BaeStatus dec_decode_audio(void *handle, int64_t first_frame, uint32_t frame_count,
                                  float *out_samples, uint32_t *out_frames)
{
    UNUSED(handle);
    UNUSED(first_frame);
    UNUSED(frame_count);
    UNUSED(out_samples);
    *out_frames = 0u;
    return BAE_OK;
}
static void dec_close(void *handle) { UNUSED(handle); }
static const BaeDecoderVTable1 k_decoder_vtable = {
    .struct_size = (uint32_t)sizeof(BaeDecoderVTable1),
    .reserved = 0u,
    .probe = dec_probe,
    .open = dec_open,
    .get_info = dec_get_info,
    .decode_video = dec_decode_video,
    .decode_audio = dec_decode_audio,
    .close = dec_close,
};
static BaeStatus enc_open(void *plugin_data, const BaeEncoderOpenArgs *args,
                          BaePixelFormat *out_format, void **out_handle)
{
    UNUSED(args);
    *out_format = BAE_PIXEL_FORMAT_RGBA_U8_PREMUL;
    *out_handle = plugin_data;
    return BAE_OK;
}
static BaeStatus enc_encode_video(void *handle, const BaeCpuImage *frame, BaeFlicks pts)
{
    UNUSED(handle);
    UNUSED(frame);
    UNUSED(pts);
    return BAE_OK;
}
static BaeStatus enc_encode_audio(void *handle, const float *samples, uint32_t frame_count)
{
    UNUSED(handle);
    UNUSED(samples);
    UNUSED(frame_count);
    return BAE_OK;
}
static BaeStatus enc_finish(void *handle)
{
    UNUSED(handle);
    return BAE_OK;
}
static void enc_close(void *handle) { UNUSED(handle); }
static const BaeEncoderVTable1 k_encoder_vtable = {
    .struct_size = (uint32_t)sizeof(BaeEncoderVTable1),
    .reserved = 0u,
    .open = enc_open,
    .encode_video = enc_encode_video,
    .encode_audio = enc_encode_audio,
    .finish = enc_finish,
    .close = enc_close,
};

/* ---- expression library ---- */
static BaeStatus expr_lerp(void *plugin_data, const BaeExprValue *args, uint32_t arg_count,
                           BaeExprValue *out_result)
{
    UNUSED(plugin_data);
    if (arg_count != 3u) {
        return BAE_ERR_INVALID_ARGUMENT;
    }
    out_result->len = 1u;
    out_result->reserved = 0u;
    out_result->v[0] = args[0].v[0] + (args[1].v[0] - args[0].v[0]) * args[2].v[0];
    return BAE_OK;
}
static const BaeExprFunction k_expr_functions[] = {
    { .name = "lerp", .min_args = 3u, .max_args = 3u, .flags = 0u, .reserved = 0u,
      .call = expr_lerp },
};
static const BaeExpressionLibraryVTable1 k_expr_vtable = {
    .struct_size = (uint32_t)sizeof(BaeExpressionLibraryVTable1),
    .function_count = 1u,
    .functions = k_expr_functions,
};

/* ---- exports ---- */
#define CONTRIBUTION(kind_, id_, vtable_)                                                   \
    { .struct_size = (uint32_t)sizeof(BaeContribution), .kind = (kind_), .id = (id_),      \
      .vtable_version = 1u, .reserved = 0u, .vtable = (vtable_) }
static const BaeContribution k_contributions[] = {
    CONTRIBUTION(BAE_KIND_EFFECT, "effect", &k_effect_vtable),
    CONTRIBUTION(BAE_KIND_SHAPE_OPERATOR, "shape-operator", &k_shape_vtable),
    CONTRIBUTION(BAE_KIND_IMPORTER, "importer", &k_importer_vtable),
    CONTRIBUTION(BAE_KIND_EXPORTER, "exporter", &k_exporter_vtable),
    CONTRIBUTION(BAE_KIND_DECODER, "codec", &k_decoder_vtable),
    CONTRIBUTION(BAE_KIND_ENCODER, "codec", &k_encoder_vtable),
    CONTRIBUTION(BAE_KIND_COMMAND, "command", &k_command_vtable),
    CONTRIBUTION(BAE_KIND_EXPRESSION_LIBRARY, "expr-lib", &k_expr_vtable),
};
#define CONTRIBUTION_COUNT (sizeof k_contributions / sizeof k_contributions[0])

static int g_plugin_data;
static void plugin_shutdown(void *plugin_data) { UNUSED(plugin_data); }
static const void *plugin_get_suite(void *plugin_data, const char *name, uint32_t version)
{
    UNUSED(plugin_data);
    UNUSED(name);
    UNUSED(version);
    return NULL;
}

/* The entry point, through the header macro. Its type must be BaePluginEntryFn. */
BAE_PLUGIN_ENTRY
{
    if (!BAE_HAS_FIELD(host, BaeHost, get_suite) ||
        BAE_VERSION_MAJOR(host->abi_version) != BAE_ABI_VERSION_MAJOR ||
        BAE_VERSION_MINOR(host->abi_version) < BAE_ABI_VERSION_MINOR) {
        return BAE_ERR_VERSION_MISMATCH;
    }
    if (!BAE_HAS_FIELD(out_exports, BaePluginExports, contributions)) {
        return BAE_ERR_VERSION_MISMATCH;
    }
    out_exports->abi_built = BAE_ABI_VERSION;
    out_exports->abi_required = BAE_ABI_VERSION;
    out_exports->plugin_id = "com.example.abi-check";
    out_exports->plugin_version = "0.0.0";
    out_exports->plugin_data = &g_plugin_data;
    out_exports->contribution_count = (uint32_t)CONTRIBUTION_COUNT;
    out_exports->contributions = k_contributions;
    if (BAE_HAS_FIELD(out_exports, BaePluginExports, shutdown)) {
        out_exports->shutdown = plugin_shutdown;
    }
    if (BAE_HAS_FIELD(out_exports, BaePluginExports, get_suite)) {
        out_exports->get_suite = plugin_get_suite;
    }
    return BAE_OK;
}

/* ======================================================================= */
/* 7. Dummy host: every suite                                              */
/* ======================================================================= */

struct BaeHostHandle {
    int errors;
};

static void core_log(BaeHostHandle *host, uint32_t level, const char *message)
{
    UNUSED(host);
    UNUSED(level);
    UNUSED(message);
}
static void core_set_error(BaeHostHandle *host, const char *message)
{
    UNUSED(message);
    ++host->errors;
}
static void *core_alloc(BaeHostHandle *host, size_t size, size_t align)
{
    UNUSED(host);
    UNUSED(size);
    UNUSED(align);
    return NULL;
}
static void core_free(BaeHostHandle *host, void *ptr)
{
    UNUSED(host);
    UNUSED(ptr);
}
static BaeBool core_has_permission(BaeHostHandle *host, const char *permission)
{
    UNUSED(host);
    UNUSED(permission);
    return BAE_FALSE;
}
static BaeStatus core_get_path(BaeHostHandle *host, uint32_t which, BaeWriter *out_utf8)
{
    UNUSED(host);
    UNUSED(which);
    UNUSED(out_utf8);
    return BAE_ERR_UNSUPPORTED;
}
static const BaeCoreSuite1 k_core = {
    .log = core_log,
    .set_error = core_set_error,
    .alloc = core_alloc,
    .free = core_free,
    .has_permission = core_has_permission,
    .get_path = core_get_path,
};

static BaeStatus pd_add(BaeParamBuilder *builder, const BaeParamDef *def)
{
    UNUSED(builder);
    UNUSED(def);
    return BAE_OK;
}
static const BaeParamDefineSuite1 k_param_define = { .add = pd_add };

static BaeStatus pe_set_value(BaeParamEditor *editor, const BaeParamValue *value)
{
    UNUSED(editor);
    UNUSED(value);
    return BAE_OK;
}
static BaeStatus pe_set_ui(BaeParamEditor *editor, uint32_t param_id, uint64_t set_flags,
                           uint64_t clear_flags, const char *name)
{
    UNUSED(editor);
    UNUSED(param_id);
    UNUSED(set_flags);
    UNUSED(clear_flags);
    UNUSED(name);
    return BAE_OK;
}
static const BaeParamEditSuite1 k_param_edit = { .set_value = pe_set_value, .set_ui = pe_set_ui };

static BaeStatus r_checkout_input(BaeRenderCtx *ctx, const BaeInputRequest *request,
                                  BaeInputInfo *out_info)
{
    UNUSED(ctx);
    UNUSED(request);
    UNUSED(out_info);
    return BAE_ERR_BAD_STATE;
}
static BaeStatus r_checkout_param(BaeRenderCtx *ctx, uint32_t param_id, BaeFlicks time,
                                  BaeParamValue *out_value)
{
    UNUSED(ctx);
    UNUSED(param_id);
    UNUSED(time);
    UNUSED(out_value);
    return BAE_ERR_NOT_FOUND;
}
static BaeStatus r_get_input_cpu(BaeRenderCtx *ctx, uint32_t checkout_id, BaeCpuImage *out_image)
{
    UNUSED(ctx);
    UNUSED(checkout_id);
    UNUSED(out_image);
    return BAE_ERR_NOT_FOUND;
}
static BaeStatus r_get_input_gpu(BaeRenderCtx *ctx, uint32_t checkout_id, BaeGpuImage **out_image)
{
    UNUSED(ctx);
    UNUSED(checkout_id);
    *out_image = NULL;
    return BAE_ERR_NOT_FOUND;
}
static BaeBool r_is_aborted(BaeRenderCtx *ctx)
{
    UNUSED(ctx);
    return BAE_FALSE;
}
static void r_report_progress(BaeRenderCtx *ctx, uint64_t done, uint64_t total)
{
    UNUSED(ctx);
    UNUSED(done);
    UNUSED(total);
}
static BaeStatus r_alloc_cpu_image(BaeRenderCtx *ctx, BaePixelFormat format, BaeRect rect,
                                   BaeCpuImage *out_image)
{
    UNUSED(ctx);
    UNUSED(format);
    UNUSED(rect);
    UNUSED(out_image);
    return BAE_ERR_OUT_OF_MEMORY;
}
static BaeStatus r_parallel_for(BaeRenderCtx *ctx, uint32_t count, BaeParallelFn fn, void *user)
{
    uint32_t i;
    UNUSED(ctx);
    for (i = 0; i < count; ++i) {
        const BaeStatus status = fn(user, i, 0u);
        if (status != BAE_OK) {
            return status;
        }
    }
    return BAE_OK;
}
static const BaeRenderSuite1 k_render = {
    .checkout_input = r_checkout_input,
    .checkout_param = r_checkout_param,
    .get_input_cpu = r_get_input_cpu,
    .get_input_gpu = r_get_input_gpu,
    .is_aborted = r_is_aborted,
    .report_progress = r_report_progress,
    .alloc_cpu_image = r_alloc_cpu_image,
    .parallel_for = r_parallel_for,
};

static BaeStatus gc_create_pipeline(BaeGpuSetupCtx *ctx, const BaeComputePipelineDesc *desc,
                                    BaeGpuPipeline **out_pipeline)
{
    UNUSED(ctx);
    UNUSED(desc);
    *out_pipeline = NULL;
    return BAE_ERR_UNSUPPORTED;
}
static void gc_destroy_pipeline(BaeGpuSetupCtx *ctx, BaeGpuPipeline *pipeline)
{
    UNUSED(ctx);
    UNUSED(pipeline);
}
static BaeStatus gc_get_image_info(BaeRenderCtx *ctx, BaeGpuImage *image,
                                   BaeGpuImageInfo *out_info)
{
    UNUSED(ctx);
    UNUSED(image);
    UNUSED(out_info);
    return BAE_ERR_UNSUPPORTED;
}
static BaeStatus gc_acquire_image(BaeRenderCtx *ctx, uint32_t gpu_format, BaeRect rect,
                                  BaeGpuImage **out_image)
{
    UNUSED(ctx);
    UNUSED(gpu_format);
    UNUSED(rect);
    *out_image = NULL;
    return BAE_ERR_UNSUPPORTED;
}
static BaeStatus gc_acquire_buffer(BaeRenderCtx *ctx, uint64_t size, BaeGpuBuffer **out_buffer)
{
    UNUSED(ctx);
    UNUSED(size);
    *out_buffer = NULL;
    return BAE_ERR_UNSUPPORTED;
}
static BaeStatus gc_dispatch(BaeRenderCtx *ctx, BaeGpuPipeline *pipeline,
                             const BaeGpuBinding *bindings, uint32_t binding_count,
                             uint32_t groups_x, uint32_t groups_y, uint32_t groups_z)
{
    UNUSED(ctx);
    UNUSED(pipeline);
    UNUSED(bindings);
    UNUSED(binding_count);
    UNUSED(groups_x);
    UNUSED(groups_y);
    UNUSED(groups_z);
    return BAE_ERR_UNSUPPORTED;
}
static BaeStatus gc_copy_image(BaeRenderCtx *ctx, BaeGpuImage *src, BaeGpuImage *dst,
                               BaeRect rect)
{
    UNUSED(ctx);
    UNUSED(src);
    UNUSED(dst);
    UNUSED(rect);
    return BAE_ERR_UNSUPPORTED;
}
static BaeStatus gc_clear_image(BaeRenderCtx *ctx, BaeGpuImage *image, const float rgba[4])
{
    UNUSED(ctx);
    UNUSED(image);
    UNUSED(rgba);
    return BAE_ERR_UNSUPPORTED;
}
static const BaeGpuComputeSuite1 k_gpu_compute = {
    .create_pipeline = gc_create_pipeline,
    .destroy_pipeline = gc_destroy_pipeline,
    .get_image_info = gc_get_image_info,
    .acquire_image = gc_acquire_image,
    .acquire_buffer = gc_acquire_buffer,
    .dispatch = gc_dispatch,
    .copy_image = gc_copy_image,
    .clear_image = gc_clear_image,
};

static BaeStatus gn_get_device(BaeGpuSetupCtx *ctx, BaeNativeGpuDevice *out_device)
{
    UNUSED(ctx);
    UNUSED(out_device);
    return BAE_ERR_UNSUPPORTED;
}
static BaeStatus gn_begin(BaeRenderCtx *ctx, const BaeNativeImageRequest *images, uint32_t count,
                          BaeNativeGpuScope *out_scope, BaeNativeGpuTexture *out_textures)
{
    UNUSED(ctx);
    UNUSED(images);
    UNUSED(count);
    UNUSED(out_scope);
    UNUSED(out_textures);
    return BAE_ERR_UNSUPPORTED;
}
static BaeStatus gn_end(BaeRenderCtx *ctx)
{
    UNUSED(ctx);
    return BAE_ERR_BAD_STATE;
}
static const BaeGpuNativeSuite1 k_gpu_native = {
    .get_device = gn_get_device,
    .begin = gn_begin,
    .end = gn_end,
};

static BaeStatus path_add(BaePathSink *sink, const BaePath *path)
{
    UNUSED(sink);
    UNUSED(path);
    return BAE_OK;
}
static const BaePathSuite1 k_path = { .add_path = path_add };

static BaeStatus c_invoke(BaeCommandCtx *ctx, const char *command_id, BaeStr args_json,
                          BaeWriter *result_json)
{
    UNUSED(ctx);
    UNUSED(command_id);
    UNUSED(args_json);
    UNUSED(result_json);
    return BAE_ERR_NOT_FOUND;
}
static BaeStatus c_query(BaeCommandCtx *ctx, const char *query, BaeStr args_json,
                         BaeWriter *result_json)
{
    UNUSED(ctx);
    UNUSED(query);
    UNUSED(args_json);
    UNUSED(result_json);
    return BAE_ERR_NOT_FOUND;
}
static BaeBool c_is_aborted(BaeCommandCtx *ctx)
{
    UNUSED(ctx);
    return BAE_FALSE;
}
static void c_report_progress(BaeCommandCtx *ctx, uint64_t done, uint64_t total)
{
    UNUSED(ctx);
    UNUSED(done);
    UNUSED(total);
}
static const BaeCommandSuite1 k_command = {
    .invoke = c_invoke,
    .query = c_query,
    .is_aborted = c_is_aborted,
    .report_progress = c_report_progress,
};

static const void *host_get_suite(BaeHostHandle *host, const char *name, uint32_t version)
{
    static const struct {
        const char *name;
        uint32_t version;
        const void *table;
    } suites[] = {
        { BAE_SUITE_CORE, BAE_SUITE_CORE_VERSION, &k_core },
        { BAE_SUITE_PARAM_DEFINE, BAE_SUITE_PARAM_DEFINE_VERSION, &k_param_define },
        { BAE_SUITE_PARAM_EDIT, BAE_SUITE_PARAM_EDIT_VERSION, &k_param_edit },
        { BAE_SUITE_RENDER, BAE_SUITE_RENDER_VERSION, &k_render },
        { BAE_SUITE_GPU_COMPUTE, BAE_SUITE_GPU_COMPUTE_VERSION, &k_gpu_compute },
        { BAE_SUITE_GPU_NATIVE, BAE_SUITE_GPU_NATIVE_VERSION, &k_gpu_native },
        { BAE_SUITE_PATH, BAE_SUITE_PATH_VERSION, &k_path },
        { BAE_SUITE_COMMAND, BAE_SUITE_COMMAND_VERSION, &k_command },
    };
    size_t i;
    UNUSED(host);
    for (i = 0; i < sizeof suites / sizeof suites[0]; ++i) {
        if (strcmp(suites[i].name, name) == 0 && suites[i].version == version) {
            return suites[i].table;
        }
    }
    return NULL; /* unknown (name, version): optional feature absent (6.6 rule 4) */
}

/* ======================================================================= */
/* 8. Runtime checks                                                       */
/* ======================================================================= */

static int g_failures;
#define EXPECT(cond)                                                          \
    do {                                                                      \
        if (!(cond)) {                                                        \
            ++g_failures;                                                     \
            fprintf(stderr, "abi_check: FAIL line %d: %s\n", __LINE__, #cond); \
        }                                                                     \
    } while (0)

/* BAE_HAS_FIELD on producers that know fewer (or more) fields (R3). */
static void check_has_field(void)
{
    BaeRenderInfo info;
    BaeHost host;
    BaeParamDef def;
    const BaeRenderInfo *cinfo = &info;
    const size_t end_of_duration =
        offsetof(BaeRenderInfo, layer_duration) + sizeof info.layer_duration;

    memset(&info, 0, sizeof info);

    /* A producer from before layer_duration was appended. */
    info.struct_size = (uint32_t)offsetof(BaeRenderInfo, layer_duration);
    EXPECT(BAE_HAS_FIELD(&info, BaeRenderInfo, struct_size));
    EXPECT(BAE_HAS_FIELD(&info, BaeRenderInfo, params));
    EXPECT(!BAE_HAS_FIELD(&info, BaeRenderInfo, layer_duration));
    /* A field that is only partly covered is absent. */
    info.struct_size = (uint32_t)(end_of_duration - 1u);
    EXPECT(!BAE_HAS_FIELD(&info, BaeRenderInfo, layer_duration));
    /* Exactly covered, current size, and a newer (bigger) producer. */
    info.struct_size = (uint32_t)end_of_duration;
    EXPECT(BAE_HAS_FIELD(&info, BaeRenderInfo, layer_duration));
    info.struct_size = (uint32_t)sizeof info;
    EXPECT(BAE_HAS_FIELD(cinfo, BaeRenderInfo, layer_duration)); /* through a const pointer */
    info.struct_size = (uint32_t)sizeof info + 64u;
    EXPECT(BAE_HAS_FIELD(&info, BaeRenderInfo, layer_duration));
    /* struct_size 0 (e.g. "no video" in BaeEncoderOpenArgs) has nothing. */
    info.struct_size = 0u;
    EXPECT(!BAE_HAS_FIELD(&info, BaeRenderInfo, struct_size));

    memset(&host, 0, sizeof host);
    host.struct_size = (uint32_t)offsetof(BaeHost, get_suite);
    EXPECT(BAE_HAS_FIELD(&host, BaeHost, handle));
    EXPECT(!BAE_HAS_FIELD(&host, BaeHost, get_suite));

    /* Nested member designators work too. */
    memset(&def, 0, sizeof def);
    def.struct_size = (uint32_t)sizeof def;
    EXPECT(BAE_HAS_FIELD(&def, BaeParamDef, u.float_def.display));
    def.struct_size = (uint32_t)offsetof(BaeParamDef, u);
    EXPECT(!BAE_HAS_FIELD(&def, BaeParamDef, u.float_def.default_value));
}

/* Loads the dummy plugin into the dummy host and calls a few callbacks. */
static void check_handshake(void)
{
    const BaePluginEntryFn entry = bae_plugin_entry; /* signature check */
    struct BaeHostHandle handle = { 0 };
    BaeHost host;
    BaePluginExports exports;
    ByteSink sink;
    BaeWriter writer;
    uint32_t i;
    BaeStatus status;

    memset(&host, 0, sizeof host);
    host.struct_size = (uint32_t)sizeof host;
    host.abi_version = BAE_ABI_VERSION;
    host.capabilities = BAE_HOST_CAP_HEADLESS;
    host.host_name = "abi_check";
    host.host_version = "0.0.0";
    host.plugin_id = "com.example.abi-check";
    host.plugin_dir = ".";
    host.handle = &handle;
    host.get_suite = host_get_suite;

    memset(&sink, 0, sizeof sink);
    writer.opaque = &sink;
    writer.write = sink_write;

    /* Every suite of ABI 0.1 is served; an unknown version is not. */
    EXPECT(host.get_suite(host.handle, BAE_SUITE_CORE, BAE_SUITE_CORE_VERSION) == &k_core);
    EXPECT(host.get_suite(host.handle, BAE_SUITE_COMMAND, BAE_SUITE_COMMAND_VERSION) ==
           &k_command);
    EXPECT(host.get_suite(host.handle, BAE_SUITE_CORE, 2u) == NULL);
    ((const BaeCoreSuite1 *)host.get_suite(host.handle, BAE_SUITE_CORE, 1u))
        ->set_error(host.handle, "test");
    EXPECT(handle.errors == 1);

    /* A host of another major is refused by the plugin (spec 6.6 rule 3). */
    host.abi_version = BAE_MAKE_VERSION(BAE_ABI_VERSION_MAJOR + 1u, 0u);
    memset(&exports, 0, sizeof exports);
    exports.struct_size = (uint32_t)sizeof exports;
    EXPECT(entry(&host, &exports) == BAE_ERR_VERSION_MISMATCH);
    host.abi_version = BAE_ABI_VERSION;

    status = entry(&host, &exports);
    EXPECT(status == BAE_OK);
    EXPECT(exports.abi_built == BAE_ABI_VERSION);
    EXPECT(exports.contribution_count == CONTRIBUTION_COUNT);
    EXPECT(exports.shutdown == plugin_shutdown);
    EXPECT(exports.get_suite(exports.plugin_data, "com.example.none", 1u) == NULL);

    for (i = 0; i < exports.contribution_count; ++i) {
        const BaeContribution *c = &exports.contributions[i];
        EXPECT(c->struct_size == sizeof(BaeContribution));
        EXPECT(c->vtable_version == 1u);
        switch (c->kind) {
        case BAE_KIND_EFFECT: {
            const BaeEffectVTable1 *vt = (const BaeEffectVTable1 *)c->vtable;
            BaeEffectDescription desc;
            void *global = NULL;
            EXPECT(vt->struct_size == sizeof(BaeEffectVTable1));
            memset(&desc, 0, sizeof desc);
            desc.struct_size = (uint32_t)sizeof desc;
            EXPECT(vt->global_setup(exports.plugin_data, NULL, &desc, &global) == BAE_OK);
            EXPECT(desc.gpu_formats == BAE_GPU_FORMAT_BIT(BAE_GPU_FORMAT_RGBA16F));
            EXPECT(vt->message(global, NULL, BAE_MESSAGE_ABOUT, (BaeBytes){ NULL, 0u },
                               &writer) == BAE_OK);
            vt->global_setdown(global);
            break;
        }
        case BAE_KIND_SHAPE_OPERATOR:
            EXPECT(((const BaeShapeOperatorVTable1 *)c->vtable)->struct_size ==
                   sizeof(BaeShapeOperatorVTable1));
            break;
        case BAE_KIND_IMPORTER: {
            const BaeImporterVTable1 *vt = (const BaeImporterVTable1 *)c->vtable;
            const BaeStr path = { "a.json", 6u };
            EXPECT(vt->probe(exports.plugin_data, path, (BaeBytes){ NULL, 0u }) == 50u);
            break;
        }
        case BAE_KIND_EXPORTER:
            EXPECT(((const BaeExporterVTable1 *)c->vtable)->struct_size ==
                   sizeof(BaeExporterVTable1));
            break;
        case BAE_KIND_DECODER:
            EXPECT(((const BaeDecoderVTable1 *)c->vtable)->struct_size ==
                   sizeof(BaeDecoderVTable1));
            break;
        case BAE_KIND_ENCODER:
            EXPECT(((const BaeEncoderVTable1 *)c->vtable)->struct_size ==
                   sizeof(BaeEncoderVTable1));
            break;
        case BAE_KIND_COMMAND: {
            const BaeCommandVTable1 *vt = (const BaeCommandVTable1 *)c->vtable;
            BaeCommandArgs args;
            memset(&args, 0, sizeof args);
            args.struct_size = (uint32_t)sizeof args;
            args.result_json = &writer;
            EXPECT(vt->execute(exports.plugin_data, &args) == BAE_OK);
            break;
        }
        case BAE_KIND_EXPRESSION_LIBRARY: {
            const BaeExpressionLibraryVTable1 *vt =
                (const BaeExpressionLibraryVTable1 *)c->vtable;
            BaeExprValue args[3];
            BaeExprValue result;
            memset(args, 0, sizeof args);
            args[0].len = args[1].len = args[2].len = 1u;
            args[0].v[0] = 2.0;
            args[1].v[0] = 4.0;
            args[2].v[0] = 0.5;
            EXPECT(vt->function_count == 1u);
            EXPECT(vt->functions[0].call(exports.plugin_data, args, 3u, &result) == BAE_OK);
            EXPECT(result.len == 1u && result.v[0] == 3.0);
            break;
        }
        default:
            /* R9: unknown kinds are ignored, never rejected. */
            break;
        }
    }

    /* ARBITRARY param callbacks round-trip through a BaeWriter. */
    sink.len = 0u;
    EXPECT(k_arbitrary_vtable.to_text(NULL, (BaeBytes){ NULL, 0u }, &writer) == BAE_OK);
    EXPECT(sink.len == 2u && memcmp(sink.data, "[]", 2u) == 0);

    exports.shutdown(exports.plugin_data);
}

int main(void)
{
    check_has_field();
    check_handshake();
    if (g_failures != 0) {
        fprintf(stderr, "abi_check: %d runtime check(s) failed\n", g_failures);
        return 1;
    }
    printf("abi_check: OK (ABI %u.%u)\n", (unsigned)BAE_ABI_VERSION_MAJOR,
           (unsigned)BAE_ABI_VERSION_MINOR);
    return 0;
}
