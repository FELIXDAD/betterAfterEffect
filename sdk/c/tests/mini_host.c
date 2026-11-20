/*
 * mini_host.c - a tiny test host for native BAE plugins, written against
 * bae_plugin.h only. It loads a plugin library the way the real host does
 * (spec 7.2: dlopen(RTLD_NOW | RTLD_LOCAL) / LoadLibraryExW), negotiates the
 * API (spec 6.6), validates the exports against the manifest values given on
 * the command line, and then drives the "invert" example effect
 * (sdk/c/examples/invert) through global_setup, define_params and render_cpu.
 *
 * Usage: mini_host <library> <plugin-id> <plugin-version> <api> <effect-id>
 *        (tests/check.sh extracts the last four from bae-plugin.toml)
 *
 * The export/negotiation checks are generic; the parameter and pixel checks
 * are specific to the invert example. The host deliberately hands the plugin
 * truncated out-structs surrounded by canary bytes to verify rule R3 (never
 * write past struct_size).
 *
 * Note: bae_plugin_entry is normally called once per load. This harness calls
 * it several times (negative negotiation tests first); the invert example
 * tolerates that because its entry point only fills static data.
 *
 * SPDX-License-Identifier: Apache-2.0
 */
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "../bae_plugin.h"

#if defined(_WIN32)
#include <windows.h>
#else
#include <dlfcn.h>
#endif

/* ----------------------------------------------------------------------- */
/* Check bookkeeping                                                       */
/* ----------------------------------------------------------------------- */

static int g_checks;
static int g_failures;

static void check_impl(int ok, int line, const char *fmt, ...)
{
    ++g_checks;
    if (!ok) {
        va_list ap;
        ++g_failures;
        fprintf(stderr, "mini_host: FAIL (line %d): ", line);
        va_start(ap, fmt);
        vfprintf(stderr, fmt, ap);
        va_end(ap);
        fputc('\n', stderr);
    }
}
#define CHECK(cond, ...) check_impl((cond) ? 1 : 0, __LINE__, __VA_ARGS__)

#define CANARY_BYTE 0xA5u
#define PAD_BYTE 0x5Au

/* True if bytes [from, size) of buf all equal `value`. */
static int bytes_equal(const unsigned char *buf, size_t from, size_t size, unsigned value)
{
    size_t i;
    for (i = from; i < size; ++i) {
        if (buf[i] != value) {
            return 0;
        }
    }
    return 1;
}

/* ----------------------------------------------------------------------- */
/* Host objects (the opaque types of the header, completed by the host)    */
/* ----------------------------------------------------------------------- */

struct BaeHostHandle {
    char last_error[256];
    int error_count;
    int unknown_suite_requests;
};

#define MAX_PARAMS 8
struct BaeParamBuilder {
    BaeParamDef defs[MAX_PARAMS];
    uint32_t count;
};

struct BaeRenderCtx {
    const BaeCpuImage *input; /* the implicit self input (checkout id 0) */
    BaeBool aborted;
    uint32_t abort_polls;
};

/* ---- bae.core ---- */

static void core_log(BaeHostHandle *host, uint32_t level, const char *message)
{
    (void)host;
    if (getenv("BAE_MINI_HOST_VERBOSE") != NULL) {
        fprintf(stderr, "plugin log [%u]: %s\n", (unsigned)level, message ? message : "");
    }
}

static void core_set_error(BaeHostHandle *host, const char *message)
{
    snprintf(host->last_error, sizeof host->last_error, "%s", message ? message : "");
    ++host->error_count;
}

static void *core_alloc(BaeHostHandle *host, size_t size, size_t align)
{
    (void)host;
    /* Enough for this harness: malloc already satisfies small alignments. */
    return align <= 16u ? malloc(size) : NULL;
}

static void core_free(BaeHostHandle *host, void *ptr)
{
    (void)host;
    free(ptr);
}

static BaeBool core_has_permission(BaeHostHandle *host, const char *permission)
{
    (void)host;
    (void)permission;
    return BAE_FALSE;
}

static BaeStatus core_get_path(BaeHostHandle *host, uint32_t which, BaeWriter *out_utf8)
{
    (void)host;
    (void)which;
    (void)out_utf8;
    return BAE_ERR_UNSUPPORTED;
}

static const BaeCoreSuite1 k_core_suite = {
    core_log, core_set_error, core_alloc, core_free, core_has_permission, core_get_path,
};

/* ---- bae.param.define ---- */

static BaeStatus param_add(BaeParamBuilder *builder, const BaeParamDef *def)
{
    if (def == NULL || def->struct_size < offsetof(BaeParamDef, u) + sizeof def->u) {
        return BAE_ERR_INVALID_ARGUMENT;
    }
    if (builder->count >= MAX_PARAMS) {
        return BAE_ERR_BUFFER_TOO_SMALL;
    }
    builder->defs[builder->count++] = *def; /* the def is copied */
    return BAE_OK;
}

static const BaeParamDefineSuite1 k_param_define_suite = { param_add };

/* ---- bae.render ---- */

static BaeStatus render_checkout_input(BaeRenderCtx *ctx, const BaeInputRequest *request,
                                       BaeInputInfo *out_info)
{
    (void)ctx;
    (void)request;
    (void)out_info;
    return BAE_ERR_BAD_STATE; /* pre_render only; this harness never calls it */
}

static BaeStatus render_checkout_param(BaeRenderCtx *ctx, uint32_t param_id, BaeFlicks time,
                                       BaeParamValue *out_value)
{
    (void)ctx;
    (void)param_id;
    (void)time;
    (void)out_value;
    return BAE_ERR_NOT_FOUND;
}

static BaeStatus render_get_input_cpu(BaeRenderCtx *ctx, uint32_t checkout_id,
                                      BaeCpuImage *out_image)
{
    uint32_t size;
    if (checkout_id != 0u || ctx->input == NULL) {
        return BAE_ERR_NOT_FOUND;
    }
    if (out_image == NULL || !BAE_HAS_FIELD(out_image, BaeCpuImage, flags)) {
        return BAE_ERR_INVALID_ARGUMENT;
    }
    /* The caller sized the out-struct: copy at most struct_size bytes. */
    size = out_image->struct_size;
    memcpy(out_image, ctx->input, size < sizeof(BaeCpuImage) ? size : sizeof(BaeCpuImage));
    out_image->struct_size = size;
    return BAE_OK;
}

static BaeStatus render_get_input_gpu(BaeRenderCtx *ctx, uint32_t checkout_id,
                                      BaeGpuImage **out_image)
{
    (void)ctx;
    (void)checkout_id;
    (void)out_image;
    return BAE_ERR_UNSUPPORTED;
}

static BaeBool render_is_aborted(BaeRenderCtx *ctx)
{
    ++ctx->abort_polls;
    return ctx->aborted;
}

static void render_report_progress(BaeRenderCtx *ctx, uint64_t done, uint64_t total)
{
    (void)ctx;
    (void)done;
    (void)total;
}

static BaeStatus render_alloc_cpu_image(BaeRenderCtx *ctx, BaePixelFormat format, BaeRect rect,
                                        BaeCpuImage *out_image)
{
    (void)ctx;
    (void)format;
    (void)rect;
    (void)out_image;
    return BAE_ERR_UNSUPPORTED;
}

static BaeStatus render_parallel_for(BaeRenderCtx *ctx, uint32_t count, BaeParallelFn fn,
                                     void *user)
{
    uint32_t i;
    (void)ctx;
    for (i = 0; i < count; ++i) {
        BaeStatus status = fn(user, i, 0u);
        if (status != BAE_OK) {
            return status;
        }
    }
    return BAE_OK;
}

static const BaeRenderSuite1 k_render_suite = {
    render_checkout_input,  render_checkout_param,  render_get_input_cpu,
    render_get_input_gpu,   render_is_aborted,      render_report_progress,
    render_alloc_cpu_image, render_parallel_for,
};

static const void *host_get_suite(BaeHostHandle *host, const char *name, uint32_t version)
{
    if (strcmp(name, BAE_SUITE_CORE) == 0 && version == BAE_SUITE_CORE_VERSION) {
        return &k_core_suite;
    }
    if (strcmp(name, BAE_SUITE_PARAM_DEFINE) == 0 && version == BAE_SUITE_PARAM_DEFINE_VERSION) {
        return &k_param_define_suite;
    }
    if (strcmp(name, BAE_SUITE_RENDER) == 0 && version == BAE_SUITE_RENDER_VERSION) {
        return &k_render_suite;
    }
    ++host->unknown_suite_requests;
    return NULL;
}

/* ----------------------------------------------------------------------- */
/* Library loading                                                         */
/* ----------------------------------------------------------------------- */

#if defined(_WIN32)
typedef HMODULE LibHandle;
static LibHandle lib_open(const char *path)
{
    /* Spec 7.2: search the plugin's own directory for its dependencies
       (needs an absolute path). */
    return LoadLibraryExA(path, NULL,
                          LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
}
static BaePluginEntryFn lib_entry(LibHandle lib)
{
    FARPROC proc = GetProcAddress(lib, BAE_PLUGIN_ENTRY_NAME);
    BaePluginEntryFn fn;
    memcpy(&fn, &proc, sizeof fn);
    return fn;
}
static void lib_close(LibHandle lib) { FreeLibrary(lib); }
static const char *lib_error(void) { return "LoadLibraryExA failed"; }
#else
typedef void *LibHandle;
static LibHandle lib_open(const char *path) { return dlopen(path, RTLD_NOW | RTLD_LOCAL); }
static BaePluginEntryFn lib_entry(LibHandle lib)
{
    /* ISO C has no object-to-function pointer conversion; POSIX guarantees
       the representation, so copy the bits. */
    void *sym = dlsym(lib, BAE_PLUGIN_ENTRY_NAME);
    BaePluginEntryFn fn;
    BAE_STATIC_ASSERT(sizeof fn == sizeof sym, "function and object pointers differ in size");
    memcpy(&fn, &sym, sizeof fn);
    return fn;
}
static void lib_close(LibHandle lib) { dlclose(lib); }
static const char *lib_error(void)
{
    const char *e = dlerror();
    return e ? e : "unknown dlopen error";
}
#endif

/* ----------------------------------------------------------------------- */
/* Test images                                                             */
/* ----------------------------------------------------------------------- */

typedef struct TestImage {
    BaeCpuImage image;
    unsigned char *storage;
    size_t storage_size;
    size_t pixel_bytes;
} TestImage;

static size_t pixel_bytes_of(BaePixelFormat format)
{
    switch (BAE_PIXEL_FORMAT_COMPONENT(format)) {
    case BAE_COMPONENT_U8:
        return 4u;
    case BAE_COMPONENT_U16:
    case BAE_COMPONENT_U16_15BIT:
    case BAE_COMPONENT_F16:
        return 8u;
    case BAE_COMPONENT_F32:
        return 16u;
    default:
        return 0u;
    }
}

/* Allocates an image whose rows have 16+ bytes of padding filled with PAD_BYTE. */
static int image_init(TestImage *t, BaePixelFormat format, BaeRect rect, int writable)
{
    const size_t width = (size_t)(rect.x1 - rect.x0);
    const size_t height = (size_t)(rect.y1 - rect.y0);
    size_t row_bytes;

    memset(t, 0, sizeof *t);
    t->pixel_bytes = pixel_bytes_of(format);
    row_bytes = (width * t->pixel_bytes + 15u) / 16u * 16u + 16u;
    t->storage_size = row_bytes * height;
    t->storage = (unsigned char *)malloc(t->storage_size);
    if (t->storage == NULL) {
        return 0;
    }
    memset(t->storage, PAD_BYTE, t->storage_size);
    t->image.struct_size = (uint32_t)sizeof t->image;
    t->image.format = format;
    t->image.rect = rect;
    t->image.row_bytes = (int64_t)row_bytes;
    t->image.data = t->storage;
    t->image.flags = writable ? BAE_IMAGE_FLAG_WRITABLE : 0u;
    return 1;
}

static void image_free(TestImage *t) { free(t->storage); }

static unsigned char *image_pixel(const TestImage *t, int32_t x, int32_t y)
{
    return t->storage + (size_t)(y - t->image.rect.y0) * (size_t)t->image.row_bytes +
           (size_t)(x - t->image.rect.x0) * t->pixel_bytes;
}

/* True if every row's padding still holds PAD_BYTE (no write past the rect). */
static int image_padding_intact(const TestImage *t)
{
    const size_t used = (size_t)(t->image.rect.x1 - t->image.rect.x0) * t->pixel_bytes;
    int32_t y;
    for (y = t->image.rect.y0; y < t->image.rect.y1; ++y) {
        const unsigned char *row = image_pixel(t, t->image.rect.x0, y);
        if (!bytes_equal(row, used, (size_t)t->image.row_bytes, PAD_BYTE)) {
            return 0;
        }
    }
    return 1;
}

/* Deterministic premultiplied test pattern; channel k of pixel (x, y). */
static float pattern(int32_t x, int32_t y, int k)
{
    const float alpha = 0.25f + 0.125f * (float)((x + 8 * 4) % 6);
    if (k == 3) {
        return alpha;
    }
    return alpha * (float)((x + 2 * y + k + 64) % 5) / 4.0f;
}

static void fill_pattern(TestImage *t)
{
    int32_t x;
    int32_t y;
    int k;
    for (y = t->image.rect.y0; y < t->image.rect.y1; ++y) {
        for (x = t->image.rect.x0; x < t->image.rect.x1; ++x) {
            unsigned char *p = image_pixel(t, x, y);
            for (k = 0; k < 4; ++k) {
                const float v = pattern(x, y, k);
                if (BAE_PIXEL_FORMAT_COMPONENT(t->image.format) == BAE_COMPONENT_F32) {
                    memcpy(p + 4 * k, &v, sizeof v);
                } else {
                    p[k] = (unsigned char)(v * 255.0f + 0.5f);
                }
            }
        }
    }
}

static int contains(BaeRect r, int32_t x, int32_t y)
{
    return x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1;
}

/* ----------------------------------------------------------------------- */
/* Effect driving                                                          */
/* ----------------------------------------------------------------------- */

typedef struct Effect {
    const BaeEffectVTable1 *vt;
    void *global;
    BaeHost *host;
    BaeHostHandle *handle;
} Effect;

static BaeStatus run_render(const Effect *fx, TestImage *in, TestImage *out, double amount,
                            BaeBool abort, uint32_t *out_polls)
{
    BaeParamValue value;
    BaeRenderInfo info;
    BaeRenderCtx ctx;
    BaeCpuRenderArgs args;
    BaeStatus status;

    memset(&value, 0, sizeof value);
    value.id = 1u;
    value.type = BAE_PARAM_FLOAT;
    value.v.f = amount;

    memset(&info, 0, sizeof info);
    info.struct_size = (uint32_t)sizeof info;
    info.quality = BAE_QUALITY_HIGH;
    info.time = 0;
    info.time_step = BAE_FLICKS_PER_SECOND / 30;
    info.frame_rate.num = 30;
    info.frame_rate.den = 1;
    info.downsample_x.num = info.downsample_x.den = 1;
    info.downsample_y.num = info.downsample_y.den = 1;
    info.pixel_aspect.num = info.pixel_aspect.den = 1;
    info.color_space = BAE_COLOR_SPACE_SRGB;
    info.flags = BAE_RENDER_FLAG_HEADLESS;
    info.layer_bounds = in->image.rect;
    info.render_id = 1u;
    info.params.count = 1u;
    info.params.values = &value;
    info.layer_duration = BAE_FLICKS_PER_SECOND;

    memset(&ctx, 0, sizeof ctx);
    ctx.input = &in->image;
    ctx.aborted = abort;

    memset(&args, 0, sizeof args);
    args.struct_size = (uint32_t)sizeof args;
    args.info = &info;
    args.ctx = &ctx;
    args.frame_data = NULL;
    args.output = &out->image;

    status = fx->vt->render_cpu(fx->global, NULL, &args);
    if (out_polls != NULL) {
        *out_polls = ctx.abort_polls;
    }
    return status;
}

/* Renders and compares every output pixel with the premultiplied invert. */
static void test_render(const Effect *fx, BaePixelFormat format, BaeRect in_rect,
                        BaeRect out_rect, double amount, const char *label)
{
    const int is_f32 = BAE_PIXEL_FORMAT_COMPONENT(format) == BAE_COMPONENT_F32;
    const float mix = (float)(amount / 100.0);
    TestImage in;
    TestImage out;
    unsigned char *in_copy;
    BaeStatus status;
    uint32_t polls = 0;
    int mismatches = 0;
    int32_t x;
    int32_t y;
    int k;

    if (!image_init(&in, format, in_rect, 0) || !image_init(&out, format, out_rect, 1)) {
        CHECK(0, "%s: out of memory", label);
        return;
    }
    fill_pattern(&in);
    in_copy = (unsigned char *)malloc(in.storage_size);
    if (in_copy != NULL) {
        memcpy(in_copy, in.storage, in.storage_size);
    }

    status = run_render(fx, &in, &out, amount, BAE_FALSE, &polls);
    CHECK(status == BAE_OK, "%s: render_cpu returned %d", label, (int)status);
    CHECK(polls >= (uint32_t)(out_rect.y1 - out_rect.y0),
          "%s: is_aborted polled %u times for %d rows", label, (unsigned)polls,
          (int)(out_rect.y1 - out_rect.y0));
    CHECK(image_padding_intact(&out), "%s: plugin wrote past the end of an output row", label);
    CHECK(in_copy != NULL && memcmp(in_copy, in.storage, in.storage_size) == 0,
          "%s: plugin modified its read-only input", label);

    for (y = out_rect.y0; y < out_rect.y1; ++y) {
        for (x = out_rect.x0; x < out_rect.x1; ++x) {
            const unsigned char *p = image_pixel(&out, x, y);
            const int inside = contains(in_rect, x, y);
            for (k = 0; k < 4; ++k) {
                float expected = 0.0f;
                if (inside) {
                    const float a = pattern(x, y, 3);
                    const float c = pattern(x, y, k);
                    expected = k == 3 ? a : c + mix * (a - 2.0f * c);
                }
                if (is_f32) {
                    float got;
                    memcpy(&got, p + 4 * k, sizeof got);
                    if (got < expected - 1e-5f || got > expected + 1e-5f) {
                        ++mismatches;
                    }
                } else {
                    /* The input was quantized to 8 bits: allow +-1 LSB per step. */
                    const float want = expected * 255.0f;
                    const float got = (float)p[k];
                    if (got < want - 1.6f || got > want + 1.6f) {
                        ++mismatches;
                    }
                }
            }
        }
    }
    CHECK(mismatches == 0, "%s: %d channel values differ from the expected invert", label,
          mismatches);

    free(in_copy);
    image_free(&in);
    image_free(&out);
}

static void test_render_errors(const Effect *fx)
{
    const BaeRect rect = { 0, 0, 4, 2 };
    TestImage in;
    TestImage out;
    BaeStatus status;
    int errors_before;

    /* Abort: the plugin must notice is_aborted() and return BAE_ERR_ABORTED. */
    if (image_init(&in, BAE_PIXEL_FORMAT_RGBA_U8_PREMUL, rect, 0) &&
        image_init(&out, BAE_PIXEL_FORMAT_RGBA_U8_PREMUL, rect, 1)) {
        status = run_render(fx, &in, &out, 100.0, BAE_TRUE, NULL);
        CHECK(status == BAE_ERR_ABORTED, "abort: render_cpu returned %d", (int)status);
    } else {
        CHECK(0, "abort: out of memory");
    }
    image_free(&in);
    image_free(&out);

    /* A format the effect did not declare: error + message via set_error. */
    if (image_init(&in, BAE_PIXEL_FORMAT_BGRA_U8_PREMUL, rect, 0) &&
        image_init(&out, BAE_PIXEL_FORMAT_BGRA_U8_PREMUL, rect, 1)) {
        errors_before = fx->handle->error_count;
        status = run_render(fx, &in, &out, 100.0, BAE_FALSE, NULL);
        CHECK(status == BAE_ERR_UNSUPPORTED, "BGRA: render_cpu returned %d", (int)status);
        CHECK(fx->handle->error_count > errors_before && fx->handle->last_error[0] != '\0',
              "BGRA: set_error was not called");
    } else {
        CHECK(0, "BGRA: out of memory");
    }
    image_free(&in);
    image_free(&out);

    /* Input and output formats differ. */
    if (image_init(&in, BAE_PIXEL_FORMAT_RGBA_F32_PREMUL, rect, 0) &&
        image_init(&out, BAE_PIXEL_FORMAT_RGBA_U8_PREMUL, rect, 1)) {
        status = run_render(fx, &in, &out, 100.0, BAE_FALSE, NULL);
        CHECK(status == BAE_ERR_UNSUPPORTED, "mixed formats: render_cpu returned %d",
              (int)status);
    } else {
        CHECK(0, "mixed formats: out of memory");
    }
    image_free(&in);
    image_free(&out);
}

/* ----------------------------------------------------------------------- */
/* Main                                                                    */
/* ----------------------------------------------------------------------- */

typedef union ExportsBuffer {
    BaePluginExports exports;
    unsigned char bytes[sizeof(BaePluginExports) + 64];
} ExportsBuffer;

typedef union DescBuffer {
    BaeEffectDescription desc;
    unsigned char bytes[sizeof(BaeEffectDescription) + 64];
} DescBuffer;

static BaeStatus call_entry(BaePluginEntryFn entry, const BaeHost *host, ExportsBuffer *buf,
                            uint32_t struct_size)
{
    memset(buf->bytes, CANARY_BYTE, sizeof buf->bytes);
    buf->exports.struct_size = struct_size;
    return entry(host, &buf->exports);
}

static int parse_api(const char *text, uint32_t *out_version)
{
    char *end;
    unsigned long major = strtoul(text, &end, 10);
    unsigned long minor;
    if (end == text || *end != '.') {
        return 0;
    }
    text = end + 1;
    minor = strtoul(text, &end, 10);
    if (end == text || *end != '\0' || major > 0xFFFFul || minor > 0xFFFFul) {
        return 0;
    }
    *out_version = BAE_MAKE_VERSION(major, minor);
    return 1;
}

int main(int argc, char **argv)
{
    struct BaeHostHandle handle;
    BaeHost host;
    LibHandle lib;
    BaePluginEntryFn entry;
    ExportsBuffer xbuf;
    BaePluginExports exports;
    const BaeContribution *effect = NULL;
    uint32_t api = 0;
    uint32_t i;
    BaeStatus status;
    Effect fx;

    if (argc != 6 || !parse_api(argv[4], &api)) {
        fprintf(stderr, "usage: %s <library> <plugin-id> <plugin-version> <api> <effect-id>\n",
                argv[0]);
        return 2;
    }

    lib = lib_open(argv[1]);
    if (lib == NULL) {
        fprintf(stderr, "mini_host: cannot load %s: %s\n", argv[1], lib_error());
        return 1;
    }
    entry = lib_entry(lib);
    if (entry == NULL) {
        fprintf(stderr, "mini_host: %s does not export %s\n", argv[1], BAE_PLUGIN_ENTRY_NAME);
        lib_close(lib);
        return 1;
    }

    memset(&handle, 0, sizeof handle);
    memset(&host, 0, sizeof host);
    host.struct_size = (uint32_t)sizeof host;
    host.capabilities = BAE_HOST_CAP_HEADLESS;
    host.host_name = "BAE";
    host.host_version = "0.0.0-mini-host";
    host.plugin_id = argv[2];
    host.plugin_dir = ".";
    host.handle = &handle;
    host.get_suite = host_get_suite;

    /* ---- 1. API negotiation (spec 6.6 rule 3): the plugin must refuse. ---- */
    host.abi_version = BAE_MAKE_VERSION(BAE_VERSION_MAJOR(api) + 1u, BAE_VERSION_MINOR(api));
    status = call_entry(entry, &host, &xbuf, (uint32_t)sizeof(BaePluginExports));
    CHECK(status == BAE_ERR_VERSION_MISMATCH, "host of another major: entry returned %d",
          (int)status);
    if (BAE_VERSION_MINOR(api) > 0u) {
        host.abi_version = BAE_MAKE_VERSION(BAE_VERSION_MAJOR(api), BAE_VERSION_MINOR(api) - 1u);
        status = call_entry(entry, &host, &xbuf, (uint32_t)sizeof(BaePluginExports));
        CHECK(status == BAE_ERR_VERSION_MISMATCH, "host of an older minor: entry returned %d",
              (int)status);
    }
    host.abi_version = BAE_ABI_VERSION;
    host.struct_size = (uint32_t)offsetof(BaeHost, get_suite);
    status = call_entry(entry, &host, &xbuf, (uint32_t)sizeof(BaePluginExports));
    CHECK(status == BAE_ERR_VERSION_MISMATCH, "BaeHost without get_suite: entry returned %d",
          (int)status);
    host.struct_size = (uint32_t)sizeof host;

    /* ---- 2. Rule R3 on the exports out-struct. ---- */
    status = call_entry(entry, &host, &xbuf, (uint32_t)offsetof(BaePluginExports, shutdown));
    CHECK(status == BAE_OK, "exports without shutdown/get_suite: entry returned %d", (int)status);
    CHECK(bytes_equal(xbuf.bytes, offsetof(BaePluginExports, shutdown), sizeof xbuf.bytes,
                      CANARY_BYTE),
          "entry wrote past a truncated BaePluginExports.struct_size");

    status = call_entry(entry, &host, &xbuf, (uint32_t)sizeof(BaePluginExports));
    CHECK(status == BAE_OK, "entry returned %d (%s)", (int)status, handle.last_error);
    CHECK(bytes_equal(xbuf.bytes, sizeof(BaePluginExports), sizeof xbuf.bytes, CANARY_BYTE),
          "entry wrote past BaePluginExports");
    CHECK(xbuf.exports.struct_size == sizeof(BaePluginExports),
          "entry changed the host-set struct_size");
    if (status != BAE_OK) {
        lib_close(lib);
        fprintf(stderr, "mini_host: %d of %d checks failed\n", g_failures, g_checks);
        return 1;
    }
    exports = xbuf.exports;

    /* ---- 3. Exports vs manifest (spec 6.6 rule 2, 7.2 step 4). ---- */
    CHECK(exports.plugin_id != NULL && strcmp(exports.plugin_id, argv[2]) == 0,
          "plugin_id '%s' != manifest id '%s'", exports.plugin_id ? exports.plugin_id : "(null)",
          argv[2]);
    CHECK(exports.plugin_version != NULL && strcmp(exports.plugin_version, argv[3]) == 0,
          "plugin_version '%s' != manifest version '%s'",
          exports.plugin_version ? exports.plugin_version : "(null)", argv[3]);
    CHECK(exports.abi_required == api, "abi_required 0x%08x != manifest api %s (0x%08x)",
          (unsigned)exports.abi_required, argv[4], (unsigned)api);
    CHECK(BAE_VERSION_MAJOR(exports.abi_built) == BAE_ABI_VERSION_MAJOR,
          "abi_built 0x%08x has another major", (unsigned)exports.abi_built);
    CHECK(handle.unknown_suite_requests == 0, "plugin asked for %d unknown suites",
          handle.unknown_suite_requests);
    CHECK(exports.contribution_count > 0u && exports.contributions != NULL,
          "plugin exports no contributions");
    for (i = 0; i < exports.contribution_count && exports.contributions != NULL; ++i) {
        const BaeContribution *c = &exports.contributions[i];
        CHECK(c->struct_size >= sizeof(BaeContribution), "contribution %u: struct_size %u",
              (unsigned)i, (unsigned)c->struct_size);
        CHECK(c->vtable != NULL && c->vtable_version == 1u, "contribution %u: bad vtable",
              (unsigned)i);
        if (c->kind == BAE_KIND_EFFECT && c->id != NULL && strcmp(c->id, argv[5]) == 0) {
            CHECK(effect == NULL, "effect '%s' exported twice", argv[5]);
            effect = c;
        }
    }
    CHECK(effect != NULL, "no effect contribution with id '%s'", argv[5]);
    if (effect == NULL) {
        lib_close(lib);
        fprintf(stderr, "mini_host: %d of %d checks failed\n", g_failures, g_checks);
        return 1;
    }

    memset(&fx, 0, sizeof fx);
    fx.vt = (const BaeEffectVTable1 *)effect->vtable;
    fx.host = &host;
    fx.handle = &handle;
    CHECK(BAE_HAS_FIELD(fx.vt, BaeEffectVTable1, render_cpu), "effect vtable too small (%u)",
          (unsigned)fx.vt->struct_size);
    CHECK(fx.vt->global_setup != NULL && fx.vt->render_cpu != NULL,
          "effect lacks global_setup or render_cpu");

    /* ---- 4. global_setup and the effect description. ---- */
    {
        BaeGlobalSetupArgs args;
        DescBuffer dbuf;
        void *global = NULL;
        uint32_t f;
        int has_f32 = 0;
        int has_u8 = 0;

        memset(&args, 0, sizeof args);
        args.struct_size = (uint32_t)sizeof args;
        args.host = &host;
        args.contribution_id = effect->id;
        args.locale = "zh-CN";

        /* Truncated description: must fail cleanly without writing past it. */
        memset(dbuf.bytes, CANARY_BYTE, sizeof dbuf.bytes);
        dbuf.desc.struct_size = (uint32_t)offsetof(BaeEffectDescription, cpu_formats);
        status = fx.vt->global_setup(exports.plugin_data, &args, &dbuf.desc, &global);
        CHECK(status == BAE_ERR_VERSION_MISMATCH, "truncated description: global_setup -> %d",
              (int)status);
        CHECK(bytes_equal(dbuf.bytes, offsetof(BaeEffectDescription, cpu_formats),
                          sizeof dbuf.bytes, CANARY_BYTE),
              "global_setup wrote past a truncated BaeEffectDescription");

        memset(dbuf.bytes, CANARY_BYTE, sizeof dbuf.bytes);
        dbuf.desc.struct_size = (uint32_t)sizeof dbuf.desc;
        status = fx.vt->global_setup(exports.plugin_data, &args, &dbuf.desc, &global);
        CHECK(status == BAE_OK, "global_setup returned %d (%s)", (int)status, handle.last_error);
        CHECK(bytes_equal(dbuf.bytes, sizeof dbuf.desc, sizeof dbuf.bytes, CANARY_BYTE),
              "global_setup wrote past BaeEffectDescription");
        CHECK(dbuf.desc.effect_class == BAE_EFFECT_CLASS_FILTER, "effect_class %u",
              (unsigned)dbuf.desc.effect_class);
        CHECK((dbuf.desc.flags & BAE_EFFECT_FLAG_PIXEL_INDEPENDENT) != 0u,
              "PIXEL_INDEPENDENT not set (manifest says pixel-independent)");
        CHECK(dbuf.desc.threading == BAE_THREADING_CONCURRENT,
              "threading %u (manifest says concurrent)", (unsigned)dbuf.desc.threading);
        CHECK(dbuf.desc.devices == BAE_DEVICE_CPU, "devices %u (manifest says cpu)",
              (unsigned)dbuf.desc.devices);
        for (f = 0; f < dbuf.desc.cpu_format_count && dbuf.desc.cpu_formats != NULL; ++f) {
            has_f32 |= dbuf.desc.cpu_formats[f] == BAE_PIXEL_FORMAT_RGBA_F32_PREMUL;
            has_u8 |= dbuf.desc.cpu_formats[f] == BAE_PIXEL_FORMAT_RGBA_U8_PREMUL;
        }
        CHECK(has_f32 && has_u8, "cpu_formats lacks RGBA F32 or RGBA U8 premultiplied");
        fx.global = global;
    }

    /* ---- 5. define_params vs the manifest listing copy. ---- */
    if (fx.vt->define_params != NULL) {
        struct BaeParamBuilder builder;
        memset(&builder, 0, sizeof builder);
        status = fx.vt->define_params(fx.global, &builder);
        CHECK(status == BAE_OK, "define_params returned %d", (int)status);
        CHECK(builder.count == 1u, "expected 1 param, got %u", (unsigned)builder.count);
        if (builder.count >= 1u) {
            const BaeParamDef *d = &builder.defs[0];
            CHECK(d->type == BAE_PARAM_FLOAT && d->id == 1u, "param 0: type %u id %u",
                  (unsigned)d->type, (unsigned)d->id);
            CHECK(d->key != NULL && strcmp(d->key, "amount") == 0, "param 0: key");
            CHECK(d->u.float_def.default_value == 100.0 && d->u.float_def.valid_min == 0.0 &&
                      d->u.float_def.valid_max == 100.0 && d->u.float_def.slider_min == 0.0 &&
                      d->u.float_def.slider_max == 100.0 && d->u.float_def.precision == 1u &&
                      d->u.float_def.display == BAE_DISPLAY_PERCENT,
                  "param 0: float definition differs from bae-plugin.toml");
        }
    } else {
        CHECK(0, "define_params is NULL");
    }

    /* ---- 6. render_cpu. ---- */
    {
        const BaeRect in_a = { 0, 0, 5, 3 };
        const BaeRect out_a = { 1, 1, 5, 3 };    /* inside the input */
        const BaeRect in_b = { -2, -1, 3, 2 };
        const BaeRect out_b = { -3, -2, 4, 3 };  /* larger than the input on all sides */
        test_render(&fx, BAE_PIXEL_FORMAT_RGBA_F32_PREMUL, in_a, out_a, 100.0, "f32 100%");
        test_render(&fx, BAE_PIXEL_FORMAT_RGBA_F32_PREMUL, in_b, out_b, 25.0, "f32 25%");
        test_render(&fx, BAE_PIXEL_FORMAT_RGBA_U8_PREMUL, in_a, out_a, 50.0, "u8 50%");
        test_render(&fx, BAE_PIXEL_FORMAT_RGBA_U8_PREMUL, in_b, out_b, 100.0, "u8 100%");
        test_render(&fx, BAE_PIXEL_FORMAT_RGBA_U8_PREMUL, in_a, in_a, 0.0, "u8 0%");
        test_render_errors(&fx);
    }

    /* ---- 7. Teardown in reverse order (spec 7.4 step 4). ---- */
    if (fx.vt->global_setdown != NULL) {
        fx.vt->global_setdown(fx.global);
    }
    if (BAE_HAS_FIELD(&exports, BaePluginExports, shutdown) && exports.shutdown != NULL) {
        exports.shutdown(exports.plugin_data);
    }
    lib_close(lib);

    if (g_failures != 0) {
        fprintf(stderr, "mini_host: %d of %d checks failed\n", g_failures, g_checks);
        return 1;
    }
    printf("mini_host: %s/%s %s: %d checks passed\n", argv[2], argv[5], argv[3], g_checks);
    return 0;
}
