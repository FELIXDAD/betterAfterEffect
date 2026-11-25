/*
 * Test fixture: a native effect that misbehaves on purpose (crash isolation
 * tests in crates/bae-plugin/tests/native.rs). Param "mode": 0 copies the
 * input, 1 dereferences NULL, 2 returns an error with a message, 3 aborts.
 *
 * SPDX-License-Identifier: Apache-2.0
 */
#include <stdlib.h>
#include <string.h>

#include "bae_plugin.h"

static BaeHostHandle *g_host;
static const BaeCoreSuite1 *g_core;
static const BaeParamDefineSuite1 *g_define;
static const BaeRenderSuite1 *g_render;
static const BaePixelFormat k_formats[] = {BAE_PIXEL_FORMAT_RGBA_F32_PREMUL};

static BaeStatus crash_global_setup(void *plugin_data, const BaeGlobalSetupArgs *args,
                                    BaeEffectDescription *out_desc, void **out_global)
{
    (void)args;
    out_desc->effect_class = BAE_EFFECT_CLASS_FILTER;
    out_desc->flags = 0u;
    out_desc->threading = BAE_THREADING_SERIAL_GLOBAL;
    out_desc->devices = BAE_DEVICE_CPU;
    out_desc->gpu_formats = 0u;
    out_desc->cpu_format_count = 1u;
    out_desc->cpu_formats = k_formats;
    *out_global = plugin_data;
    return BAE_OK;
}

static BaeStatus crash_define_params(void *global, BaeParamBuilder *builder)
{
    BaeParamDef def;
    (void)global;
    memset(&def, 0, sizeof def);
    def.struct_size = (uint32_t)sizeof def;
    def.type = BAE_PARAM_INT;
    def.id = 1u;
    def.key = "mode";
    def.name = "Mode";
    def.u.int_def.default_value = 0;
    def.u.int_def.valid_min = 0;
    def.u.int_def.valid_max = 3;
    def.u.int_def.slider_min = 0;
    def.u.int_def.slider_max = 3;
    return g_define->add(builder, &def);
}

static BaeStatus crash_render_cpu(void *global, const void *instance, const BaeCpuRenderArgs *args)
{
    BaeCpuImage in;
    int32_t mode = args->info->params.count > 0u ? args->info->params.values[0].v.i : 0;
    int32_t y;
    (void)global;
    (void)instance;
    if (mode == 1) {
        volatile int *null_pointer = NULL;
        *null_pointer = 42;
    } else if (mode == 2) {
        g_core->set_error(g_host, "mode 2 always fails");
        return BAE_ERR_GENERIC;
    } else if (mode == 3) {
        abort();
    }
    memset(&in, 0, sizeof in);
    in.struct_size = (uint32_t)sizeof in;
    if (g_render->get_input_cpu(args->ctx, 0u, &in) != BAE_OK) {
        return BAE_ERR_NOT_FOUND;
    }
    for (y = args->output->rect.y0; y < args->output->rect.y1; ++y) {
        memcpy((char *)args->output->data + (int64_t)(y - args->output->rect.y0) * args->output->row_bytes,
               (const char *)in.data + (int64_t)(y - in.rect.y0) * in.row_bytes +
                   (int64_t)(args->output->rect.x0 - in.rect.x0) * 16,
               (size_t)(args->output->rect.x1 - args->output->rect.x0) * 16u);
    }
    return BAE_OK;
}

static const BaeEffectVTable1 k_vtable = {
    .struct_size = (uint32_t)sizeof(BaeEffectVTable1),
    .global_setup = crash_global_setup,
    .define_params = crash_define_params,
    .render_cpu = crash_render_cpu,
};

static const BaeContribution k_contributions[] = {
    {(uint32_t)sizeof(BaeContribution), BAE_KIND_EFFECT, "crash", 1u, 0u, &k_vtable},
};

static int g_plugin_data;

BAE_PLUGIN_ENTRY
{
    g_host = host->handle;
    g_core = (const BaeCoreSuite1 *)host->get_suite(host->handle, BAE_SUITE_CORE, 1u);
    g_define = (const BaeParamDefineSuite1 *)host->get_suite(host->handle, BAE_SUITE_PARAM_DEFINE, 1u);
    g_render = (const BaeRenderSuite1 *)host->get_suite(host->handle, BAE_SUITE_RENDER, 1u);
    out_exports->abi_built = BAE_ABI_VERSION;
    out_exports->abi_required = BAE_MAKE_VERSION(1, 0);
    out_exports->plugin_id = "com.example.crash";
    out_exports->plugin_version = "0.1.0";
    out_exports->plugin_data = &g_plugin_data;
    out_exports->contribution_count = 1u;
    out_exports->contributions = k_contributions;
    return BAE_OK;
}
