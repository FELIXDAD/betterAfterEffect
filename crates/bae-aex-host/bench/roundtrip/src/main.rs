//! Round-trip cost of hosting a CPU AE effect inside BAE's GPU render graph.
//!
//! For each resolution (1080p, 2160p) and AE world depth (8/16/32 bpc) this
//! measures, on the local GPU (Metal on macOS, D3D12 on Windows):
//!
//! * **GPU-convert path** (recommended design, request R04-02): a compute
//!   pass converts the RGBA16F working texture into a packed AE world in a
//!   storage buffer → copy to a MAP_READ staging buffer → map (the plug-in
//!   reads this memory in place) → "plug-in" (pass-through memcpy from input
//!   world to output world) → output world staged in a MAP_WRITE buffer →
//!   copy + compute decode back into an RGBA16F texture.
//! * **UMA zero-copy variant** (if `MAPPABLE_PRIMARY_BUFFERS` is available,
//!   e.g. Apple Silicon): the compute pass writes straight into the mappable
//!   buffer and decodes straight from it; no buffer-to-buffer copies.
//! * **CPU-convert path** (naive design): raw RGBA16F texture readback →
//!   CPU conversion with `bae_aex_host::pixel` (1 and N threads) → plug-in →
//!   CPU conversion back → `write_texture`.
//! * **Out-of-process overhead**: a child process maps the same shared
//!   memory file; measures the IPC round trip (pipe ping-pong) and the extra
//!   copies into/out of shared memory, plus the child's memcpy "plug-in".
//!
//! It also validates the WGSL converter against the CPU reference kernels.
//!
//! Run: `cargo run --release --manifest-path crates/bae-aex-host/bench/roundtrip/Cargo.toml -- [--quick]`

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use bae_aex_host::geometry::LRect;
use bae_aex_host::pixel::{
    self, ChannelOrder, F16, FloatPolicy, PixelDepth, WorldBuf, WorldBufMut, WorldDesc,
};
use bae_aex_host::shaders::{self, CONVERT_WGSL, ConvertParams};

// ---------------------------------------------------------------------------
// small utilities
// ---------------------------------------------------------------------------

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

#[derive(Default, Clone)]
struct Samples(Vec<f64>);

impl Samples {
    fn push(&mut self, v: f64) {
        self.0.push(v);
    }
    fn median(&self) -> f64 {
        let mut v = self.0.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if v.is_empty() {
            return f64::NAN;
        }
        let n = v.len();
        if n % 2 == 1 {
            v[n / 2]
        } else {
            (v[n / 2 - 1] + v[n / 2]) / 2.0
        }
    }
    fn p90(&self) -> f64 {
        let mut v = self.0.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if v.is_empty() {
            return f64::NAN;
        }
        v[((v.len() as f64 * 0.9).ceil() as usize)
            .saturating_sub(1)
            .min(v.len() - 1)]
    }
}

fn depth_name(d: PixelDepth) -> &'static str {
    match d {
        PixelDepth::U8 => "8 bpc",
        PixelDepth::U16 => "16 bpc",
        PixelDepth::F32 => "32 bpc",
    }
}

/// Deterministic premultiplied test pattern with some over-range values.
fn pattern_f16(w: u32, h: u32, nan: bool) -> Vec<F16> {
    let mut v = Vec::with_capacity((w * h * 4) as usize);
    let mut s: u32 = 0x1234_5678;
    for y in 0..h {
        for x in 0..w {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let a = ((s >> 8) & 0xff) as f32 / 255.0;
            let r = a * (x % 97) as f32 / 96.0;
            let g = a * (y % 89) as f32 / 88.0;
            let mut b = a * ((s >> 20) & 0x3ff) as f32 / 700.0; // up to ~1.46 (over-range)
            if nan && (x + y) % 211 == 0 {
                b = f32::NAN;
            }
            v.extend_from_slice(&[
                F16(pixel::f32_to_f16(r)),
                F16(pixel::f32_to_f16(g)),
                F16(pixel::f32_to_f16(b)),
                F16(pixel::f32_to_f16(a)),
            ]);
        }
    }
    v
}

fn f16_bytes(v: &[F16]) -> Vec<u8> {
    v.iter().flat_map(|h| h.0.to_le_bytes()).collect()
}

// SAFETY helpers for reinterpreting mapped byte ranges. Mapped wgpu ranges and
// Vec<u8> allocations used here are at least 4-byte aligned (checked).
fn as_u16(b: &[u8]) -> &[u16] {
    assert_eq!(b.as_ptr() as usize % 2, 0);
    unsafe { std::slice::from_raw_parts(b.as_ptr() as *const u16, b.len() / 2) }
}
fn as_f32(b: &[u8]) -> &[f32] {
    assert_eq!(b.as_ptr() as usize % 4, 0);
    unsafe { std::slice::from_raw_parts(b.as_ptr() as *const f32, b.len() / 4) }
}
fn as_f16(b: &[u8]) -> &[F16] {
    assert_eq!(b.as_ptr() as usize % 2, 0);
    unsafe { std::slice::from_raw_parts(b.as_ptr() as *const F16, b.len() / 2) }
}
fn as_u16_mut(b: &mut [u8]) -> &mut [u16] {
    assert_eq!(b.as_ptr() as usize % 2, 0);
    unsafe { std::slice::from_raw_parts_mut(b.as_mut_ptr() as *mut u16, b.len() / 2) }
}
fn as_f32_mut(b: &mut [u8]) -> &mut [f32] {
    assert_eq!(b.as_ptr() as usize % 4, 0);
    unsafe { std::slice::from_raw_parts_mut(b.as_mut_ptr() as *mut f32, b.len() / 4) }
}
fn as_f16_mut(b: &mut [u8]) -> &mut [F16] {
    assert_eq!(b.as_ptr() as usize % 2, 0);
    unsafe { std::slice::from_raw_parts_mut(b.as_mut_ptr() as *mut F16, b.len() / 2) }
}

/// A 16-byte aligned byte buffer (Vec<u128> backing).
struct AlignedBytes {
    words: Vec<u128>,
    len: usize,
}
impl AlignedBytes {
    fn zeroed(len: usize) -> Self {
        AlignedBytes {
            words: vec![0u128; len.div_ceil(16)],
            len,
        }
    }
    fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.words.as_ptr() as *const u8, self.len) }
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.words.as_mut_ptr() as *mut u8, self.len) }
    }
}

fn world_mut<'a>(depth: PixelDepth, b: &'a mut [u8]) -> WorldBufMut<'a> {
    match depth {
        PixelDepth::U8 => WorldBufMut::U8(b),
        PixelDepth::U16 => WorldBufMut::U16(as_u16_mut(b)),
        PixelDepth::F32 => WorldBufMut::F32(as_f32_mut(b)),
    }
}
fn world_ref(depth: PixelDepth, b: &[u8]) -> WorldBuf<'_> {
    match depth {
        PixelDepth::U8 => WorldBuf::U8(b),
        PixelDepth::U16 => WorldBuf::U16(as_u16(b)),
        PixelDepth::F32 => WorldBuf::F32(as_f32(b)),
    }
}

// ---------------------------------------------------------------------------
// GPU setup
// ---------------------------------------------------------------------------

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
    uma: bool,
    module: wgpu::ShaderModule,
}

fn init_gpu() -> Gpu {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        apply_limit_buckets: false,
    }))
    .expect("no GPU adapter");
    let info = adapter.get_info();
    let uma = adapter
        .features()
        .contains(wgpu::Features::MAPPABLE_PRIMARY_BUFFERS)
        && info.device_type == wgpu::DeviceType::IntegratedGpu;
    let mut features = wgpu::Features::empty();
    if uma {
        features |= wgpu::Features::MAPPABLE_PRIMARY_BUFFERS;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("aex-roundtrip"),
        required_features: features,
        required_limits: adapter.limits(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .expect("device");
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("convert.wgsl"),
        source: wgpu::ShaderSource::Wgsl(CONVERT_WGSL.into()),
    });
    Gpu {
        device,
        queue,
        info,
        uma,
        module,
    }
}

fn wait(gpu: &Gpu) {
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");
}

fn pipeline(gpu: &Gpu, entry: &str) -> wgpu::ComputePipeline {
    gpu.device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(entry),
            layout: None,
            module: &gpu.module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        })
}

fn texture(gpu: &Gpu, w: u32, h: u32, usage: wgpu::TextureUsages) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage,
        view_formats: &[],
    })
}

fn buffer(gpu: &Gpu, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage,
        mapped_at_creation: false,
    })
}

fn upload_f16(gpu: &Gpu, tex: &wgpu::Texture, w: u32, h: u32, data: &[F16]) {
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &f16_bytes(data),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 8),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
}

fn uniform(gpu: &Gpu, w: u32, h: u32, rowbytes: usize) -> wgpu::Buffer {
    let b = buffer(
        gpu,
        32,
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    );
    let p = ConvertParams {
        origin: [0, 0],
        size: [w, h],
        row_words: (rowbytes / 4) as u32,
    };
    gpu.queue.write_buffer(&b, 0, &p.to_bytes());
    b
}

fn bind_encode(
    gpu: &Gpu,
    pl: &wgpu::ComputePipeline,
    src: &wgpu::TextureView,
    out: &wgpu::Buffer,
    u: &wgpu::Buffer,
) -> wgpu::BindGroup {
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pl.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(src),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: out.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: u.as_entire_binding(),
            },
        ],
    })
}

fn bind_decode(
    gpu: &Gpu,
    pl: &wgpu::ComputePipeline,
    dst: &wgpu::TextureView,
    inp: &wgpu::Buffer,
    u: &wgpu::Buffer,
) -> wgpu::BindGroup {
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pl.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(dst),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: inp.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: u.as_entire_binding(),
            },
        ],
    })
}

fn dispatch(
    enc: &mut wgpu::CommandEncoder,
    pl: &wgpu::ComputePipeline,
    bg: &wgpu::BindGroup,
    w: u32,
    h: u32,
) {
    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: None,
        timestamp_writes: None,
    });
    pass.set_pipeline(pl);
    pass.set_bind_group(0, bg, &[]);
    let (x, y) = shaders::dispatch_size(w, h);
    pass.dispatch_workgroups(x, y, 1);
}

fn map_and_wait(gpu: &Gpu, bufs: &[(&wgpu::Buffer, wgpu::MapMode)]) {
    for (b, mode) in bufs {
        b.slice(..).map_async(*mode, |r| r.expect("map failed"));
    }
    wait(gpu);
}

fn read_texture_f16(gpu: &Gpu, tex: &wgpu::Texture, w: u32, h: u32) -> Vec<F16> {
    let bpr = (w * 8).div_ceil(256) * 256;
    let rb = buffer(
        gpu,
        bpr as u64 * h as u64,
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &rb,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bpr),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([enc.finish()]);
    map_and_wait(gpu, &[(&rb, wgpu::MapMode::Read)]);
    let view = rb.slice(..).get_mapped_range().unwrap();
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        let row = &view[y * bpr as usize..y * bpr as usize + w as usize * 8];
        out.extend(
            row.chunks_exact(2)
                .map(|c| F16(u16::from_le_bytes([c[0], c[1]]))),
        );
    }
    drop(view);
    rb.unmap();
    out
}

// ---------------------------------------------------------------------------
// 1. shader validation against the CPU reference
// ---------------------------------------------------------------------------

fn validate(gpu: &Gpu) -> Vec<String> {
    let (w, h) = (67u32, 45u32);
    let src = pattern_f16(w, h, true);
    let tex = texture(
        gpu,
        w,
        h,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    upload_f16(gpu, &tex, w, h, &src);
    let view = tex.create_view(&Default::default());
    let mut report = Vec::new();
    for (depth, order) in [
        (PixelDepth::U8, ChannelOrder::Argb),
        (PixelDepth::U16, ChannelOrder::Argb),
        (PixelDepth::F32, ChannelOrder::Argb),
        (PixelDepth::F32, ChannelOrder::Bgra),
    ] {
        let (e, d) = shaders::entry_points(depth, order).unwrap();
        let mut desc = WorldDesc::for_rect(LRect::from_xywh(0, 0, w as i32, h as i32), depth, true);
        desc.order = order;
        let size = (desc.rowbytes * h as usize) as u64;
        // GPU encode
        let pe = pipeline(gpu, e);
        let u = uniform(gpu, w, h, desc.rowbytes);
        let out = buffer(
            gpu,
            size,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let rb = buffer(
            gpu,
            size,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        let bg = bind_encode(gpu, &pe, &view, &out, &u);
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        dispatch(&mut enc, &pe, &bg, w, h);
        enc.copy_buffer_to_buffer(&out, 0, &rb, 0, size);
        gpu.queue.submit([enc.finish()]);
        map_and_wait(gpu, &[(&rb, wgpu::MapMode::Read)]);
        let gpu_world = rb.slice(..).get_mapped_range().unwrap().to_vec();
        rb.unmap();
        // CPU encode
        let mut cpu_world = AlignedBytes::zeroed(size as usize);
        pixel::encode(
            &src,
            4 * w as usize,
            &desc,
            world_mut(depth, cpu_world.bytes_mut()),
            FloatPolicy::ZeroNan,
        )
        .unwrap();
        let mut gw = AlignedBytes::zeroed(size as usize);
        gw.bytes_mut().copy_from_slice(&gpu_world);
        let (mism, max_diff) = compare_worlds(&desc, gw.bytes(), cpu_world.bytes());
        report.push(format!(
            "encode {e}: {mism} mismatching channels of {} (max |diff| {max_diff})",
            w * h * 4
        ));
        // GPU decode of the CPU world vs CPU decode
        let pd = pipeline(gpu, d);
        let inbuf = buffer(
            gpu,
            size,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        gpu.queue.write_buffer(&inbuf, 0, cpu_world.bytes());
        let dst = texture(
            gpu,
            w,
            h,
            wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        );
        let dview = dst.create_view(&Default::default());
        let bgd = bind_decode(gpu, &pd, &dview, &inbuf, &u);
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        dispatch(&mut enc, &pd, &bgd, w, h);
        gpu.queue.submit([enc.finish()]);
        let gpu_tex = read_texture_f16(gpu, &dst, w, h);
        let mut cpu_tex = vec![F16(0); (w * h * 4) as usize];
        pixel::decode(
            world_ref(depth, cpu_world.bytes()),
            &desc,
            &mut cpu_tex,
            4 * w as usize,
        )
        .unwrap();
        let dm = gpu_tex
            .iter()
            .zip(&cpu_tex)
            .filter(|(a, b)| {
                a.0 != b.0 && !(pixel::f16_to_f32(a.0).is_nan() && pixel::f16_to_f32(b.0).is_nan())
            })
            .count();
        let max_ulp = gpu_tex
            .iter()
            .zip(&cpu_tex)
            .map(|(a, b)| (a.0 as i32 - b.0 as i32).unsigned_abs())
            .max()
            .unwrap_or(0);
        report.push(format!(
            "decode {d}: {dm} mismatching half-float channels of {} (max {max_ulp} ulp)",
            w * h * 4
        ));
        // The property that matters for the plug-in: world -> GPU decode ->
        // CPU encode must reproduce the world exactly (no drift across
        // repeated round trips through an AE node).
        let mut again = AlignedBytes::zeroed(size as usize);
        pixel::encode(
            &gpu_tex,
            4 * w as usize,
            &desc,
            world_mut(depth, again.bytes_mut()),
            FloatPolicy::ZeroNan,
        )
        .unwrap();
        let (rt, _) = compare_worlds(&desc, again.bytes(), cpu_world.bytes());
        report.push(format!(
            "round trip {d} -> {e}: {rt} channels differ from the original world"
        ));
    }
    report
}

fn compare_worlds(desc: &WorldDesc, a: &[u8], b: &[u8]) -> (usize, f64) {
    let w = desc.width as usize * 4;
    let mut mism = 0;
    let mut maxd = 0f64;
    for y in 0..desc.height as usize {
        let ra = &a[y * desc.rowbytes..];
        let rb = &b[y * desc.rowbytes..];
        for i in 0..w {
            let (va, vb) = match desc.depth {
                PixelDepth::U8 => (ra[i] as f64, rb[i] as f64),
                PixelDepth::U16 => (
                    u16::from_le_bytes([ra[2 * i], ra[2 * i + 1]]) as f64,
                    u16::from_le_bytes([rb[2 * i], rb[2 * i + 1]]) as f64,
                ),
                PixelDepth::F32 => (
                    f32::from_le_bytes(ra[4 * i..4 * i + 4].try_into().unwrap()) as f64,
                    f32::from_le_bytes(rb[4 * i..4 * i + 4].try_into().unwrap()) as f64,
                ),
            };
            if va != vb && !(va.is_nan() && vb.is_nan()) {
                mism += 1;
                maxd = maxd.max((va - vb).abs());
            }
        }
    }
    (mism, maxd)
}

// ---------------------------------------------------------------------------
// 2. GPU-convert path
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct PathTimes {
    readback: Samples,
    convert_in: Samples,
    plugin: Samples,
    convert_out: Samples,
    upload: Samples,
    total: Samples,
}

fn bench_gpu_path(
    gpu: &Gpu,
    w: u32,
    h: u32,
    depth: PixelDepth,
    iters: usize,
    zero_copy: bool,
) -> PathTimes {
    let (e, d) = shaders::entry_points(depth, ChannelOrder::Argb).unwrap();
    let rowbytes = pixel::aligned_rowbytes(w, depth, pixel::HOST_ROW_ALIGN);
    let size = (rowbytes * h as usize) as u64;
    let src = texture(
        gpu,
        w,
        h,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    upload_f16(gpu, &src, w, h, &pattern_f16(w, h, false));
    let dst = texture(gpu, w, h, wgpu::TextureUsages::STORAGE_BINDING);
    let (sv, dv) = (
        src.create_view(&Default::default()),
        dst.create_view(&Default::default()),
    );
    let u = uniform(gpu, w, h, rowbytes);
    let (pe, pd) = (pipeline(gpu, e), pipeline(gpu, d));
    use wgpu::BufferUsages as B;
    let (enc_buf, read_buf, write_buf, dec_buf);
    if zero_copy {
        read_buf = buffer(gpu, size, B::STORAGE | B::MAP_READ);
        write_buf = buffer(gpu, size, B::STORAGE | B::MAP_WRITE);
        enc_buf = None;
        dec_buf = None;
    } else {
        read_buf = buffer(gpu, size, B::MAP_READ | B::COPY_DST);
        write_buf = buffer(gpu, size, B::MAP_WRITE | B::COPY_SRC);
        enc_buf = Some(buffer(gpu, size, B::STORAGE | B::COPY_SRC));
        dec_buf = Some(buffer(gpu, size, B::STORAGE | B::COPY_DST));
    }
    let bge = bind_encode(gpu, &pe, &sv, enc_buf.as_ref().unwrap_or(&read_buf), &u);
    let bgd = bind_decode(gpu, &pd, &dv, dec_buf.as_ref().unwrap_or(&write_buf), &u);
    let mut t = PathTimes::default();
    for i in 0..iters + 2 {
        let t0 = Instant::now();
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        dispatch(&mut enc, &pe, &bge, w, h);
        if let Some(eb) = &enc_buf {
            enc.copy_buffer_to_buffer(eb, 0, &read_buf, 0, size);
        }
        gpu.queue.submit([enc.finish()]);
        map_and_wait(
            gpu,
            &[
                (&read_buf, wgpu::MapMode::Read),
                (&write_buf, wgpu::MapMode::Write),
            ],
        );
        let t1 = Instant::now();
        {
            // The "plug-in": reads its input world in place from the mapped
            // staging buffer and writes its output world (pass-through).
            let input = read_buf.slice(..).get_mapped_range().unwrap();
            let mut output = write_buf.slice(..).get_mapped_range_mut().unwrap();
            output.copy_from_slice(&input);
        }
        let t2 = Instant::now();
        read_buf.unmap();
        write_buf.unmap();
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        if let Some(db) = &dec_buf {
            enc.copy_buffer_to_buffer(&write_buf, 0, db, 0, size);
        }
        dispatch(&mut enc, &pd, &bgd, w, h);
        gpu.queue.submit([enc.finish()]);
        wait(gpu);
        let t3 = Instant::now();
        if i >= 2 {
            t.readback.push(ms(t1 - t0));
            t.plugin.push(ms(t2 - t1));
            t.upload.push(ms(t3 - t2));
            t.total.push(ms(t3 - t0));
        }
    }
    t
}

// ---------------------------------------------------------------------------
// 3. CPU-convert path
// ---------------------------------------------------------------------------

fn bench_cpu_path(
    gpu: &Gpu,
    w: u32,
    h: u32,
    depth: PixelDepth,
    iters: usize,
    threads: usize,
) -> PathTimes {
    let bpr = (w * 8).div_ceil(256) * 256;
    let src = texture(
        gpu,
        w,
        h,
        wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
    );
    upload_f16(gpu, &src, w, h, &pattern_f16(w, h, false));
    let dst = texture(gpu, w, h, wgpu::TextureUsages::COPY_DST);
    let rb = buffer(
        gpu,
        bpr as u64 * h as u64,
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    let desc = WorldDesc::for_rect(LRect::from_xywh(0, 0, w as i32, h as i32), depth, true);
    let wsize = desc.rowbytes * h as usize;
    let mut world_in = AlignedBytes::zeroed(wsize);
    let mut world_out = AlignedBytes::zeroed(wsize);
    let mut back = AlignedBytes::zeroed(w as usize * 8 * h as usize);
    let mut t = PathTimes::default();
    for i in 0..iters + 2 {
        let t0 = Instant::now();
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &src,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &rb,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bpr),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        gpu.queue.submit([enc.finish()]);
        map_and_wait(gpu, &[(&rb, wgpu::MapMode::Read)]);
        let t1 = Instant::now();
        {
            let view = rb.slice(..).get_mapped_range().unwrap();
            pixel::encode_parallel(
                as_f16(&view),
                bpr as usize / 2,
                &desc,
                world_mut(depth, world_in.bytes_mut()),
                FloatPolicy::ZeroNan,
                threads,
            )
            .unwrap();
        }
        rb.unmap();
        let t2 = Instant::now();
        world_out.bytes_mut().copy_from_slice(world_in.bytes());
        let t3 = Instant::now();
        pixel::decode_parallel(
            world_ref(depth, world_out.bytes()),
            &desc,
            as_f16_mut(back.bytes_mut()),
            4 * w as usize,
            threads,
        )
        .unwrap();
        let t4 = Instant::now();
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &dst,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            back.bytes(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 8),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        gpu.queue.submit([]);
        wait(gpu);
        let t5 = Instant::now();
        if i >= 2 {
            t.readback.push(ms(t1 - t0));
            t.convert_in.push(ms(t2 - t1));
            t.plugin.push(ms(t3 - t2));
            t.convert_out.push(ms(t4 - t3));
            t.upload.push(ms(t5 - t4));
            t.total.push(ms(t5 - t0));
        }
    }
    t
}

// ---------------------------------------------------------------------------
// 4. out-of-process overhead (shared memory + pipe signalling)
// ---------------------------------------------------------------------------

struct OopTimes {
    ipc_rtt_us: Samples,
    copy_in: Samples,
    child_plugin: Samples,
    copy_out: Samples,
}

fn child_main(path: &str, len: usize) {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("open shm");
    // SAFETY: the parent keeps the file alive and synchronises access through
    // the pipe protocol (exactly one side touches the mapping at a time).
    let mut map = unsafe { memmap2::MmapMut::map_mut(&file) }.expect("map shm");
    let half = len / 2;
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut op = [0u8; 1];
    while stdin.read_exact(&mut op).is_ok() {
        match op[0] {
            0 => {}
            1 => {
                let (a, b) = map.split_at_mut(half);
                b[..half].copy_from_slice(&a[..half]);
            }
            _ => break,
        }
        stdout.write_all(&[1]).unwrap();
        stdout.flush().unwrap();
    }
}

fn bench_oop(world_bytes: usize, iters: usize) -> OopTimes {
    let dir = std::env::temp_dir().join(format!("bae-aex-shm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("frames.bin");
    let len = world_bytes * 2;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    file.set_len(len as u64).unwrap();
    // SAFETY: see child_main.
    let mut map = unsafe { memmap2::MmapMut::map_mut(&file) }.unwrap();
    let exe = std::env::current_exe().unwrap();
    let mut child = Command::new(exe)
        .arg("--child")
        .arg(&path)
        .arg(len.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn child");
    let mut to = child.stdin.take().unwrap();
    let mut from = child.stdout.take().unwrap();
    let mut ack = [0u8; 1];
    let gpu_staging = AlignedBytes::zeroed(world_bytes); // stands in for the mapped readback buffer
    let mut upload_staging = AlignedBytes::zeroed(world_bytes);
    let mut t = OopTimes {
        ipc_rtt_us: Samples::default(),
        copy_in: Samples::default(),
        child_plugin: Samples::default(),
        copy_out: Samples::default(),
    };
    // Touch the mapping once so page faults are not measured.
    map.fill(0);
    for _ in 0..200 {
        let a = Instant::now();
        to.write_all(&[0]).unwrap();
        to.flush().unwrap();
        from.read_exact(&mut ack).unwrap();
        t.ipc_rtt_us.push(a.elapsed().as_secs_f64() * 1e6);
    }
    for i in 0..iters + 2 {
        let a = Instant::now();
        map[..world_bytes].copy_from_slice(gpu_staging.bytes());
        let b = Instant::now();
        to.write_all(&[1]).unwrap();
        to.flush().unwrap();
        from.read_exact(&mut ack).unwrap();
        let c = Instant::now();
        upload_staging
            .bytes_mut()
            .copy_from_slice(&map[world_bytes..2 * world_bytes]);
        let d = Instant::now();
        if i >= 2 {
            t.copy_in.push(ms(b - a));
            t.child_plugin.push(ms(c - b));
            t.copy_out.push(ms(d - c));
        }
    }
    to.write_all(&[2]).ok();
    drop(to);
    child.wait().ok();
    drop(map);
    std::fs::remove_dir_all(&dir).ok();
    t
}

// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--child") {
        child_main(&args[2], args[3].parse().unwrap());
        return;
    }
    let quick = args.iter().any(|a| a == "--quick");
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let gpu = init_gpu();
    println!("# bae-aex-host round-trip benchmark\n");
    println!(
        "GPU: {} ({:?}, {:?}), unified-memory zero-copy variant: {}; CPU threads: {threads}\n",
        gpu.info.name, gpu.info.backend, gpu.info.device_type, gpu.uma
    );
    println!("## WGSL converter vs CPU reference\n");
    for line in validate(&gpu) {
        println!("- {line}");
    }
    println!();

    let sizes: &[(u32, u32, &str)] = &[(1920, 1080, "1080p"), (3840, 2160, "2160p")];
    println!("## Per-frame round trip (median ms, p90 in parentheses)\n");
    println!(
        "| frame | depth | world MB | GPU-convert total | readback+map | plug-in memcpy | upload+decode | UMA zero-copy total | CPU-convert total ({threads} thr) | CPU-convert total (1 thr) | CPU encode ({threads} thr) | CPU decode ({threads} thr) |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
    let mut oop_rows = Vec::new();
    for &(w, h, label) in sizes {
        let iters = if quick {
            5
        } else if w > 2000 {
            12
        } else {
            25
        };
        for depth in [PixelDepth::U8, PixelDepth::U16, PixelDepth::F32] {
            let world = pixel::aligned_rowbytes(w, depth, pixel::HOST_ROW_ALIGN) * h as usize;
            let g = bench_gpu_path(&gpu, w, h, depth, iters, false);
            let z = gpu
                .uma
                .then(|| bench_gpu_path(&gpu, w, h, depth, iters, true));
            let c = bench_cpu_path(&gpu, w, h, depth, iters, threads);
            let c1 = bench_cpu_path(&gpu, w, h, depth, (iters / 3).max(3), 1);
            let f = |s: &Samples| format!("{:.2} ({:.2})", s.median(), s.p90());
            println!(
                "| {label} | {} | {:.1} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                depth_name(depth),
                world as f64 / 1e6,
                f(&g.total),
                f(&g.readback),
                f(&g.plugin),
                f(&g.upload),
                z.as_ref().map_or("n/a".into(), |z| f(&z.total)),
                f(&c.total),
                f(&c1.total),
                f(&c.convert_in),
                f(&c.convert_out),
            );
            let o = bench_oop(world, iters);
            oop_rows.push((label, depth, world, o));
        }
    }
    println!("\n## Out-of-process overhead (shared memory + pipe signalling)\n");
    println!(
        "| frame | depth | IPC round trip (µs, median / p90) | copy into shm (ms) | child plug-in memcpy (ms) | copy out of shm (ms) |"
    );
    println!("|---|---|---|---|---|---|");
    for (label, depth, _world, o) in &oop_rows {
        println!(
            "| {label} | {} | {:.1} / {:.1} | {:.2} | {:.2} | {:.2} |",
            depth_name(*depth),
            o.ipc_rtt_us.median(),
            o.ipc_rtt_us.p90(),
            o.copy_in.median(),
            o.child_plugin.median(),
            o.copy_out.median()
        );
    }
}
