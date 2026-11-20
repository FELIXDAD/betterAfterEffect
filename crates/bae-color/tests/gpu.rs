//! GPU tests: every WGSL snippet must match the f64 CPU reference. They skip
//! (pass with a note) when the machine has no usable adapter, like bae-gpu's
//! tests; set `BAE_REQUIRE_GPU=1` to make a missing adapter a failure.

use std::sync::Arc;

use bae_color::{
    ColorOp, ColorSpace, ColorTransform, GpuSnippet, Interpolation, Lut1d, Lut3d, LutDimension,
    TransferFunction, builtin,
};
use bae_gpu::wgpu;
use bae_gpu::{GpuContext, GpuContextDescriptor, GpuError};
use wgpu::util::DeviceExt;

fn context() -> Option<Arc<GpuContext>> {
    match GpuContext::new_headless(GpuContextDescriptor::default().with_env()) {
        Ok(ctx) => Some(ctx),
        Err(error @ GpuError::NoAdapter { .. }) => {
            if std::env::var_os("BAE_REQUIRE_GPU").is_some_and(|v| v == "1") {
                panic!("BAE_REQUIRE_GPU=1 but {error}");
            }
            eprintln!("skipping GPU test: {error}");
            None
        }
        Err(error) => panic!("GPU context creation failed: {error}"),
    }
}

/// Runs `snippet` over `input` (RGBA) in a compute shader.
fn run(ctx: &GpuContext, snippet: &GpuSnippet, input: &[[f32; 4]], premul: bool) -> Vec<[f32; 4]> {
    let dev = ctx.device();
    let mut src = snippet.wgsl.clone();
    src.push_str(&format!(
        "@group(0) @binding(0) var<uniform> params: {};\n\
         @group(0) @binding(1) var<storage, read> inp: array<vec4<f32>>;\n\
         @group(0) @binding(2) var<storage, read_write> outp: array<vec4<f32>>;\n",
        snippet.params_type()
    ));
    let mut luts = Vec::new();
    for (i, t) in snippet.textures.iter().enumerate() {
        let dim = match t.dimension {
            LutDimension::D2 => "2d",
            LutDimension::D3 => "3d",
        };
        src.push_str(&format!(
            "@group(0) @binding({}) var lut{i}: texture_{dim}<f32>;\n",
            3 + i
        ));
        luts.push(format!("lut{i}"));
    }
    let lut_refs: Vec<&str> = luts.iter().map(String::as_str).collect();
    let expr = if premul {
        snippet.call("c", "params", &lut_refs, true)
    } else {
        format!(
            "vec4<f32>({}, c.w)",
            snippet.call("c.xyz", "params", &lut_refs, false)
        )
    };
    src.push_str(&format!(
        "@compute @workgroup_size(64)\nfn main(@builtin(global_invocation_id) id: vec3<u32>) {{\n    \
         let i = id.x;\n    if (i >= arrayLength(&inp)) {{ return; }}\n    let c = inp[i];\n    outp[i] = {expr};\n}}\n"
    ));

    let scope = dev.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = dev.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bae-color test"),
        source: wgpu::ShaderSource::Wgsl(src.clone().into()),
    });
    let mut entries = vec![
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ];
    for (i, t) in snippet.textures.iter().enumerate() {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 3 + i as u32,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: match t.dimension {
                    LutDimension::D2 => wgpu::TextureViewDimension::D2,
                    LutDimension::D3 => wgpu::TextureViewDimension::D3,
                },
                multisampled: false,
            },
            count: None,
        });
    }
    let bgl = dev.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &entries,
    });
    let layout = dev.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipeline = dev.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&layout),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    if let Some(err) = pollster::block_on(scope.pop()) {
        panic!("WGSL error: {err}\n--- shader ---\n{src}");
    }

    let bytes: Vec<u8> = input
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let size = bytes.len() as u64;
    let params = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &snippet.params_bytes(),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let inp = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &bytes,
        usage: wgpu::BufferUsages::STORAGE,
    });
    let outp = dev.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let staging = dev.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut views = Vec::new();
    for t in &snippet.textures {
        let size3 = wgpu::Extent3d {
            width: t.width,
            height: t.height,
            depth_or_array_layers: t.depth,
        };
        let tex = dev.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: size3,
            mip_level_count: 1,
            sample_count: 1,
            dimension: match t.dimension {
                LutDimension::D2 => wgpu::TextureDimension::D2,
                LutDimension::D3 => wgpu::TextureDimension::D3,
            },
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        ctx.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &t.bytes(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(t.width * 16),
                rows_per_image: Some(t.height),
            },
            size3,
        );
        views.push(tex.create_view(&wgpu::TextureViewDescriptor::default()));
    }
    let mut bind = vec![
        wgpu::BindGroupEntry {
            binding: 0,
            resource: params.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: inp.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 2,
            resource: outp.as_entire_binding(),
        },
    ];
    for (i, v) in views.iter().enumerate() {
        bind.push(wgpu::BindGroupEntry {
            binding: 3 + i as u32,
            resource: wgpu::BindingResource::TextureView(v),
        });
    }
    let bg = dev.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bgl,
        entries: &bind,
    });
    let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups((input.len() as u32).div_ceil(64), 1, 1);
    }
    enc.copy_buffer_to_buffer(&outp, 0, &staging, 0, Some(size));
    ctx.queue().submit([enc.finish()]);
    staging.map_async(wgpu::MapMode::Read, .., |r| r.expect("map"));
    ctx.wait_idle().expect("poll");
    let out: Vec<[f32; 4]> = {
        let view = staging.get_mapped_range(..).expect("mapped");
        view.as_chunks::<16>()
            .0
            .iter()
            .map(|c| {
                let f = |i: usize| f32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
                [f(0), f(4), f(8), f(12)]
            })
            .collect()
    };
    staging.unmap();
    out
}

/// Inputs spanning `[lo, hi]` per channel with decorrelated channels.
fn samples(lo: f64, hi: f64, n: usize) -> Vec<[f32; 4]> {
    (0..n)
        .map(|i| {
            let t = |k: usize| lo + (hi - lo) * (((i * k) % n) as f64 / (n - 1) as f64);
            [t(1) as f32, t(7) as f32, t(13) as f32, 1.0]
        })
        .collect()
}

/// Max error metric: |gpu - cpu| / max(1, |cpu|).
fn compare(name: &str, t: &ColorTransform, input: &[[f32; 4]], out: &[[f32; 4]], tol: f64) -> f64 {
    let mut worst = 0.0f64;
    for (i, o) in input.iter().zip(out) {
        let cpu = t.apply_f64([i[0] as f64, i[1] as f64, i[2] as f64]);
        for c in 0..3 {
            let err = (o[c] as f64 - cpu[c]).abs() / cpu[c].abs().max(1.0);
            assert!(
                err <= tol,
                "{name}: input {i:?} channel {c}: gpu {} cpu {} (err {err:e})",
                o[c],
                cpu[c]
            );
            worst = worst.max(err);
        }
    }
    worst
}

fn check(ctx: &GpuContext, name: &str, t: &ColorTransform, lo: f64, hi: f64, tol: f64) {
    let input = samples(lo, hi, 509);
    let out = run(ctx, &t.to_wgsl("cc"), &input, false);
    let worst = compare(name, t, &input, &out, tol);
    eprintln!("{name:<40} max rel err {worst:.2e}");
}

#[test]
fn snippets_match_cpu_reference() {
    let Some(ctx) = context() else { return };
    eprintln!("GPU: {}", ctx.summary());

    // Every curve, both directions. PQ amplifies f32 rounding (the
    // `E^(1/m2) - c1` step cancels digits), so it gets a looser bound.
    for tf in TransferFunction::ALL {
        let tol = if tf == TransferFunction::Pq {
            1e-4
        } else {
            2e-5
        };
        let (lo, hi) = tf.encoded_test_range();
        let dec = ColorTransform::from_ops([ColorOp::Decode(tf)]);
        check(&ctx, &format!("decode {tf:?}"), &dec, lo, hi, tol);
        let (llo, lhi) = (tf.decode(lo), tf.decode(hi));
        let enc = ColorTransform::from_ops([ColorOp::Encode(tf)]);
        check(
            &ctx,
            &format!("encode {tf:?}"),
            &enc,
            llo.max(-1.0),
            lhi.min(200.0),
            tol,
        );
    }

    // Space conversions (merged chains).
    let pairs: &[(ColorSpace, ColorSpace)] = &[
        (builtin::SLOG3_SGAMUT3_CINE, builtin::SRGB),
        (builtin::LOGC3_AWG3, builtin::ACESCG),
        (builtin::LOGC4_AWG4, builtin::REC2100_PQ),
        (builtin::VLOG_VGAMUT, builtin::DISPLAY_P3),
        (builtin::CLOG3_CINEMA_GAMUT, builtin::REC709),
        (builtin::ACESCCT, builtin::SRGB),
        (builtin::ACESCC, builtin::ACESCG),
        (builtin::REC2100_HLG, builtin::REC2100_PQ),
        (builtin::SRGB, builtin::REC709_GAMMA24),
        (builtin::ADOBE_RGB, builtin::GAMMA22),
        (builtin::SRGB, builtin::ACESCG),
        (builtin::DCI_P3, builtin::REC2020),
    ];
    for (a, b) in pairs {
        let t = a.transform_to(b).unwrap();
        let (lo, hi) = a.transfer().unwrap().encoded_test_range();
        let pq = [a, b]
            .iter()
            .any(|s| s.transfer() == Some(TransferFunction::Pq));
        let tol = if pq { 1e-4 } else { 2e-5 };
        check(
            &ctx,
            &format!("{} -> {}", a.id, b.id),
            &t,
            lo.max(0.0),
            hi.min(1.0),
            tol,
        );
    }

    // Clamp + matrix with offset.
    let t = ColorTransform::from_ops([
        ColorOp::Matrix(bae_color::Affine {
            m: [[1.2, -0.1, 0.0], [0.05, 0.9, 0.05], [0.0, 0.2, 0.8]],
            offset: [0.01, -0.02, 0.03],
        }),
        ColorOp::Clamp { min: 0.0, max: 1.0 },
    ]);
    check(&ctx, "matrix+offset, clamp", &t, -0.5, 1.5, 1e-6);

    // 1D LUT long enough to wrap rows, with a non-unit domain.
    let mut l1 = Lut1d::identity(5000);
    for e in &mut l1.entries {
        *e = [e[0].powf(2.2), e[1].powf(1.8), e[2].sqrt()];
    }
    l1.domain_min = [-0.25; 3];
    l1.domain_max = [1.5; 3];
    let t = ColorTransform::from_ops([ColorOp::Lut1d(Arc::new(l1))]);
    check(&ctx, "lut1d 5000", &t, -0.5, 2.0, 2e-6);

    // 3D LUTs, both interpolations, with a non-separable function.
    for interp in [Interpolation::Linear, Interpolation::Tetrahedral] {
        let mut l3 = Lut3d::from_fn(17, |c| {
            [
                c[0] * c[1],
                (c[1] + c[2]) * 0.5,
                c[2].powf(2.0) + 0.1 * c[0],
            ]
        });
        l3.interpolation = interp;
        let t = ColorTransform::from_ops([
            ColorOp::Decode(TransferFunction::Srgb),
            ColorOp::Lut3d(Arc::new(l3)),
            ColorOp::Encode(TransferFunction::Gamma(2.4)),
        ]);
        check(&ctx, &format!("lut3d 17 {interp:?}"), &t, -0.1, 1.1, 5e-6);
    }
}

#[test]
fn premultiplied_entry_point() {
    let Some(ctx) = context() else { return };
    let t = builtin::SLOG3_SGAMUT3.transform_to(&builtin::SRGB).unwrap();
    let input = vec![
        [0.2, 0.3, 0.1, 0.5],
        [0.0, 0.0, 0.0, 0.0],
        [0.4, 0.4, 0.4, 1.0],
    ];
    let out = run(&ctx, &t.to_wgsl("pm"), &input, true);
    let mut cpu = input.clone();
    t.apply_premultiplied(&mut cpu);
    for (g, c) in out.iter().zip(&cpu) {
        for k in 0..4 {
            assert!((g[k] - c[k]).abs() < 1e-5, "{out:?} vs {cpu:?}");
        }
    }
    // Pure matrices skip the unpremultiply.
    let m = builtin::SRGB_LINEAR.transform_to(&builtin::ACESCG).unwrap();
    assert!(m.is_linear());
    let out = run(&ctx, &m.to_wgsl("pl"), &input, true);
    let mut cpu = input.clone();
    m.apply_premultiplied(&mut cpu);
    for (g, c) in out.iter().zip(&cpu) {
        for k in 0..4 {
            assert!((g[k] - c[k]).abs() < 1e-6);
        }
    }
}

#[test]
fn two_snippets_in_one_shader() {
    let Some(ctx) = context() else { return };
    // Two prefixes must coexist (input transform + display transform).
    let a = builtin::SRGB
        .transform_to(&builtin::ACESCG)
        .unwrap()
        .to_wgsl("inp");
    let b = builtin::ACESCG
        .transform_to(&builtin::SRGB)
        .unwrap()
        .to_wgsl("disp");
    let module = format!(
        "{}\n{}\n@compute @workgroup_size(1) fn main() {{}}",
        a.wgsl, b.wgsl
    );
    let scope = ctx.device().push_error_scope(wgpu::ErrorFilter::Validation);
    let _m = ctx
        .device()
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(module.into()),
        });
    assert!(pollster::block_on(scope.pop()).is_none());
}
