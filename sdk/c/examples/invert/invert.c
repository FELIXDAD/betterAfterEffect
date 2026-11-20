/*
 * invert.c - minimal native CPU effect for BAE, written in plain C against
 * bae_plugin.h only (no SDK, no other dependency).
 *
 * One effect, "invert": inverts the color channels of its input and mixes the
 * result with the original by the "Amount" parameter (0..100 %, id 1).
 *
 * What this example demonstrates:
 *   - the single exported entry point and the plugin-side API check (spec 6.6);
 *   - extensible structs (rule R3): never read or write past struct_size;
 *   - acquiring frozen host suites by (name, version) (rule R4);
 *   - global_setup / define_params / render_cpu of BaeEffectVTable1;
 *   - pre_render == NULL, i.e. the identity ROI on the self input;
 *   - CPU images: rect, row_bytes and pixel formats (RGBA F32 / U8, premultiplied);
 *   - polling is_aborted and reporting errors with bae.core set_error.
 *
 * Build commands for macOS / Windows / Linux: see README.md next to this file.
 * Manifest: bae-plugin.toml (its [[effect.param]] table is a listing copy of
 * what define_params declares and must match it).
 *
 * SPDX-License-Identifier: Apache-2.0
 */
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "bae_plugin.h"

/* Must equal [plugin] id / version in bae-plugin.toml (checked by the host). */
#define INVERT_PLUGIN_ID "com.example.invert-c"
#define INVERT_PLUGIN_VERSION "0.1.0"
/* Lowest host API this plugin runs on. Must equal `api` in bae-plugin.toml. */
#define INVERT_ABI_REQUIRED BAE_MAKE_VERSION(0, 1)

#define INVERT_EFFECT_ID "invert"
#define INVERT_PARAM_AMOUNT 1u /* stable param id: never reuse or retype */

/* Writes `value` only if the host-sized struct has room for `field` (R3). */
#define SET_IF_PRESENT(ptr, type, field, value)  \
    do {                                         \
        if (BAE_HAS_FIELD(ptr, type, field)) {   \
            (ptr)->field = (value);              \
        }                                        \
    } while (0)

/* Plugin-wide data: the host handle and the suites acquired at load time.
   Written once in bae_plugin_entry, read-only afterwards, so render threads
   may read it concurrently (BAE_THREADING_CONCURRENT). */
typedef struct InvertPlugin {
    BaeHostHandle *host;
    const BaeCoreSuite1 *core;
    const BaeParamDefineSuite1 *param_define;
    const BaeRenderSuite1 *render;
} InvertPlugin;

static InvertPlugin g_plugin;

/* Pixel formats render_cpu accepts, in preference order. */
static const BaePixelFormat k_cpu_formats[] = {
    BAE_PIXEL_FORMAT_RGBA_F32_PREMUL,
    BAE_PIXEL_FORMAT_RGBA_U8_PREMUL,
};

/* Attaches `message` to the error being returned (per thread) and returns it. */
static BaeStatus fail(const InvertPlugin *plugin, BaeStatus status, const char *message)
{
    if (plugin != NULL && plugin->core != NULL && plugin->core->set_error != NULL) {
        plugin->core->set_error(plugin->host, message);
    }
    return status;
}

/* ----------------------------------------------------------------------- */
/* Effect callbacks                                                        */
/* ----------------------------------------------------------------------- */

static BaeStatus invert_global_setup(void *plugin_data, const BaeGlobalSetupArgs *args,
                                     BaeEffectDescription *out_desc, void **out_global)
{
    InvertPlugin *plugin = (InvertPlugin *)plugin_data;
    (void)args; /* contribution_id / locale are not needed here */

    if (out_desc == NULL || out_global == NULL) {
        return fail(plugin, BAE_ERR_INVALID_ARGUMENT, "invert: global_setup got NULL outputs");
    }
    /* The host set out_desc->struct_size (R3). Every field up to cpu_formats is
       needed to describe this effect; a host without them is too old. */
    if (!BAE_HAS_FIELD(out_desc, BaeEffectDescription, cpu_formats)) {
        return fail(plugin, BAE_ERR_VERSION_MISMATCH,
                    "invert: host BaeEffectDescription is too small");
    }
    out_desc->effect_class = BAE_EFFECT_CLASS_FILTER;
    out_desc->flags = BAE_EFFECT_FLAG_PIXEL_INDEPENDENT;
    out_desc->threading = BAE_THREADING_CONCURRENT;
    out_desc->devices = BAE_DEVICE_CPU;
    out_desc->gpu_formats = 0u;
    out_desc->cpu_format_count = (uint32_t)(sizeof k_cpu_formats / sizeof k_cpu_formats[0]);
    out_desc->cpu_formats = k_cpu_formats; /* static: valid until global_setdown */

    /* No per-effect allocation: the global is the plugin-wide data. */
    *out_global = plugin;
    return BAE_OK;
}

static BaeStatus invert_define_params(void *global, BaeParamBuilder *builder)
{
    const InvertPlugin *plugin = (const InvertPlugin *)global;
    BaeParamDef def;

    memset(&def, 0, sizeof def);
    def.struct_size = (uint32_t)sizeof def;
    def.type = BAE_PARAM_FLOAT;
    def.id = INVERT_PARAM_AMOUNT;
    def.group_id = 0u;          /* top level */
    def.key = "amount";         /* stable machine key (scripts, expressions) */
    def.name = "Amount";        /* English default; translated in locales/ */
    def.l10n_key = "amount";    /* looks up [effect.invert.params] amount */
    def.flags = 0u;
    def.u.float_def.default_value = 100.0;
    def.u.float_def.valid_min = 0.0;
    def.u.float_def.valid_max = 100.0;
    def.u.float_def.slider_min = 0.0;
    def.u.float_def.slider_max = 100.0;
    def.u.float_def.precision = 1u;
    def.u.float_def.display = BAE_DISPLAY_PERCENT;

    /* The host copies the definition; `def` may live on the stack. */
    return plugin->param_define->add(builder, &def);
}

/* Value of the Amount param as a 0..1 mix factor. */
static float amount_from_params(const BaeParamValues *params)
{
    double amount = 100.0; /* default if the host did not send the value */
    uint32_t i;

    for (i = 0; i < params->count; ++i) {
        const BaeParamValue *value = &params->values[i];
        if (value->id == INVERT_PARAM_AMOUNT && value->type == BAE_PARAM_FLOAT) {
            amount = value->v.f;
            break;
        }
    }
    if (amount < 0.0) {
        amount = 0.0;
    } else if (amount > 100.0) {
        amount = 100.0;
    }
    return (float)(amount / 100.0);
}

/*
 * Premultiplied invert: with straight color c and alpha a, the inverted
 * straight color is 1 - c, so the premultiplied result is (1 - c) * a, which
 * is a - c_premul. No division by alpha is needed. The result is then mixed
 * with the original: out = c + amount * ((a - c) - c). Alpha is unchanged.
 */
static void invert_row_f32(float *dst, const float *src, int32_t count, float amount)
{
    int32_t i;
    int c;

    for (i = 0; i < count; ++i) {
        const float *s = src + 4 * i;
        float *d = dst + 4 * i;
        const float alpha = s[3];
        for (c = 0; c < 3; ++c) {
            d[c] = s[c] + amount * (alpha - 2.0f * s[c]);
        }
        d[3] = alpha;
    }
}

static void invert_row_u8(uint8_t *dst, const uint8_t *src, int32_t count, float amount)
{
    int32_t i;
    int c;

    for (i = 0; i < count; ++i) {
        const uint8_t *s = src + 4 * i;
        uint8_t *d = dst + 4 * i;
        const int alpha = s[3];
        for (c = 0; c < 3; ++c) {
            const int color = s[c];
            /* Clamp at 0 in case the input is not validly premultiplied. */
            const int inverted = alpha > color ? alpha - color : 0;
            float v = (float)color + amount * (float)(inverted - color) + 0.5f;
            if (v < 0.0f) {
                v = 0.0f;
            } else if (v > 255.0f) {
                v = 255.0f;
            }
            d[c] = (uint8_t)v;
        }
        d[3] = (uint8_t)alpha;
    }
}

/* Address of pixel (x, y) of a CPU image (see BaeCpuImage in bae_plugin.h). */
static uint8_t *pixel_address(const BaeCpuImage *image, int32_t x, int32_t y, size_t pixel_bytes)
{
    return (uint8_t *)image->data + (int64_t)(y - image->rect.y0) * image->row_bytes +
           (int64_t)(x - image->rect.x0) * (int64_t)pixel_bytes;
}

static BaeStatus invert_render_cpu(void *global, const void *instance, const BaeCpuRenderArgs *args)
{
    const InvertPlugin *plugin = (const InvertPlugin *)global;
    const BaeCpuImage *out;
    BaeCpuImage in;
    BaeStatus status;
    uint32_t component;
    size_t pixel_bytes;
    float amount;
    int32_t y;

    (void)instance; /* stateless effect: no instance_create, instance is NULL */

    if (args == NULL || args->info == NULL || args->output == NULL) {
        return fail(plugin, BAE_ERR_INVALID_ARGUMENT, "invert: render_cpu got NULL arguments");
    }
    out = args->output;

    /* Fetch the self input. We are the caller, so we size the out-struct. */
    memset(&in, 0, sizeof in);
    in.struct_size = (uint32_t)sizeof in;
    status = plugin->render->get_input_cpu(args->ctx, 0u /* implicit self input */, &in);
    if (status != BAE_OK) {
        return fail(plugin, status, "invert: could not get the source input");
    }

    /* We only declared RGBA premultiplied U8 / F32, but never trust that
       blindly: the host may evolve, and unknown values must not crash us. */
    if ((out->flags & BAE_IMAGE_FLAG_WRITABLE) == 0u) {
        return fail(plugin, BAE_ERR_INVALID_ARGUMENT, "invert: output image is not writable");
    }
    if (in.format != out->format ||
        BAE_PIXEL_FORMAT_CHANNELS(out->format) != BAE_CHANNELS_RGBA ||
        BAE_PIXEL_FORMAT_ALPHA(out->format) != BAE_ALPHA_PREMULTIPLIED) {
        return fail(plugin, BAE_ERR_UNSUPPORTED, "invert: unsupported pixel format");
    }
    component = BAE_PIXEL_FORMAT_COMPONENT(out->format);
    if (component == BAE_COMPONENT_F32) {
        pixel_bytes = 4u * sizeof(float);
    } else if (component == BAE_COMPONENT_U8) {
        pixel_bytes = 4u;
    } else {
        return fail(plugin, BAE_ERR_UNSUPPORTED, "invert: unsupported pixel component type");
    }

    amount = amount_from_params(&args->info->params);

    for (y = out->rect.y0; y < out->rect.y1; ++y) {
        uint8_t *row = pixel_address(out, out->rect.x0, y, pixel_bytes);
        int32_t x0;
        int32_t x1;

        /* Poll once per row; return promptly when the host aborts. */
        if (plugin->render->is_aborted(args->ctx)) {
            return BAE_ERR_ABORTED;
        }

        /* Pixels of the output that the input does not cover are transparent. */
        x0 = out->rect.x0 > in.rect.x0 ? out->rect.x0 : in.rect.x0;
        x1 = out->rect.x1 < in.rect.x1 ? out->rect.x1 : in.rect.x1;
        if (y < in.rect.y0 || y >= in.rect.y1 || x1 <= x0) {
            memset(row, 0, (size_t)(out->rect.x1 - out->rect.x0) * pixel_bytes);
            continue;
        }
        memset(row, 0, (size_t)(x0 - out->rect.x0) * pixel_bytes);
        memset(pixel_address(out, x1, y, pixel_bytes), 0,
               (size_t)(out->rect.x1 - x1) * pixel_bytes);

        if (component == BAE_COMPONENT_F32) {
            invert_row_f32((float *)(void *)pixel_address(out, x0, y, pixel_bytes),
                           (const float *)(const void *)pixel_address(&in, x0, y, pixel_bytes),
                           x1 - x0, amount);
        } else {
            invert_row_u8(pixel_address(out, x0, y, pixel_bytes),
                          pixel_address(&in, x0, y, pixel_bytes), x1 - x0, amount);
        }
    }
    return BAE_OK;
}

/* ----------------------------------------------------------------------- */
/* Exports                                                                 */
/* ----------------------------------------------------------------------- */

/* Everything not listed is NULL, which means "use the documented default"
   (R5): no global_setdown (nothing to free), no instances (stateless),
   pre_render NULL = identity ROI on the self input, no GPU path. */
static const BaeEffectVTable1 k_invert_vtable = {
    .struct_size = (uint32_t)sizeof(BaeEffectVTable1),
    .global_setup = invert_global_setup,
    .define_params = invert_define_params,
    .pre_render = NULL,
    .render_cpu = invert_render_cpu,
};

static const BaeContribution k_contributions[] = {
    {
        .struct_size = (uint32_t)sizeof(BaeContribution),
        .kind = BAE_KIND_EFFECT,
        .id = INVERT_EFFECT_ID, /* must match [[effect]] id in the manifest */
        .vtable_version = 1u,
        .vtable = &k_invert_vtable,
    },
};

static void invert_shutdown(void *plugin_data)
{
    memset(plugin_data, 0, sizeof(InvertPlugin));
}

BAE_PLUGIN_ENTRY
{
    InvertPlugin plugin;

    if (host == NULL || out_exports == NULL) {
        return BAE_ERR_INVALID_ARGUMENT;
    }
    /* Extensible struct written by the host: check before reading (R3). */
    if (!BAE_HAS_FIELD(host, BaeHost, get_suite) || host->get_suite == NULL) {
        return BAE_ERR_VERSION_MISMATCH;
    }
    /* Spec 6.6 rule 3: refuse a different major or an older minor. */
    if (BAE_VERSION_MAJOR(host->abi_version) != BAE_VERSION_MAJOR(INVERT_ABI_REQUIRED) ||
        BAE_VERSION_MINOR(host->abi_version) < BAE_VERSION_MINOR(INVERT_ABI_REQUIRED)) {
        return BAE_ERR_VERSION_MISMATCH;
    }

    /* Suites are frozen per (name, version) and stay valid until shutdown. */
    memset(&plugin, 0, sizeof plugin);
    plugin.host = host->handle;
    plugin.core = (const BaeCoreSuite1 *)host->get_suite(host->handle, BAE_SUITE_CORE,
                                                         BAE_SUITE_CORE_VERSION);
    plugin.param_define = (const BaeParamDefineSuite1 *)host->get_suite(
        host->handle, BAE_SUITE_PARAM_DEFINE, BAE_SUITE_PARAM_DEFINE_VERSION);
    plugin.render = (const BaeRenderSuite1 *)host->get_suite(host->handle, BAE_SUITE_RENDER,
                                                             BAE_SUITE_RENDER_VERSION);
    if (plugin.core == NULL || plugin.param_define == NULL || plugin.render == NULL) {
        return fail(&plugin, BAE_ERR_UNSUPPORTED, "invert: a required host suite is missing");
    }

    /* The host set out_exports->struct_size; fields up to `contributions`
       are required, later ones are written only if present. */
    if (!BAE_HAS_FIELD(out_exports, BaePluginExports, contributions)) {
        return fail(&plugin, BAE_ERR_VERSION_MISMATCH, "invert: host BaePluginExports is too small");
    }
    g_plugin = plugin;

    out_exports->abi_built = BAE_ABI_VERSION;
    out_exports->abi_required = INVERT_ABI_REQUIRED;
    out_exports->reserved0 = 0u;
    out_exports->plugin_id = INVERT_PLUGIN_ID;
    out_exports->plugin_version = INVERT_PLUGIN_VERSION;
    out_exports->plugin_data = &g_plugin;
    out_exports->contribution_count =
        (uint32_t)(sizeof k_contributions / sizeof k_contributions[0]);
    out_exports->reserved1 = 0u;
    out_exports->contributions = k_contributions;
    SET_IF_PRESENT(out_exports, BaePluginExports, shutdown, invert_shutdown);
    SET_IF_PRESENT(out_exports, BaePluginExports, get_suite, NULL);
    return BAE_OK;
}
