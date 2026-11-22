//! Several threads create pipelines from the same shader on one device (#194).
//!
//! wgpu-hal's D3D12 backend caches compiled shaders per device and hands every
//! thread that loads one a reference to the same compiled blob. FXC's blobs count
//! references without atomics, so this pattern corrupted the heap
//! (STATUS_HEAP_CORRUPTION, 0xc0000374) until vendor/wgpu-hal copied FXC output
//! into owned bytes. The app does the same thing: the preview renderer, the render
//! queue thread and effect prewarming build the same pipelines on the one shared
//! device.
//!
//! `BAE_STRESS_ITERATIONS` (default 50) sets the pipelines per thread; the crash
//! repro workflow raises it to make the race likely without the patch.

use std::sync::{Arc, Barrier};

use bae_gpu::{GpuContext, GpuContextDescriptor, GpuError, wgpu};

const THREADS: usize = 4;

const SHADER: &str = "
@group(0) @binding(0) var<storage, read_write> data: array<u32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    data[id.x] = data[id.x] * 3u + 1u;
}
";

#[test]
fn threads_build_pipelines_from_one_shader() {
    let gpu = match GpuContext::new_headless(GpuContextDescriptor::default().with_env()) {
        Ok(gpu) => gpu,
        Err(error @ GpuError::NoAdapter { .. }) => {
            if std::env::var_os("BAE_REQUIRE_GPU").is_some_and(|v| v == "1") {
                panic!("BAE_REQUIRE_GPU=1 but {error}");
            }
            eprintln!("skipping GPU test: {error}");
            return;
        }
        Err(error) => panic!("GPU context creation failed: {error}"),
    };
    let iterations: usize = std::env::var("BAE_STRESS_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let module = Arc::new(
        gpu.device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("concurrent pipelines"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            }),
    );
    let barrier = Arc::new(Barrier::new(THREADS));
    let threads: Vec<_> = (0..THREADS)
        .map(|_| {
            let (gpu, module, barrier) = (gpu.clone(), module.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..iterations {
                    // Each pipeline loads the cached shader: a new reference to the
                    // same compiled bytecode, released when creation ends.
                    let _pipeline =
                        gpu.device()
                            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                                label: Some("concurrent pipelines"),
                                layout: None,
                                module: &module,
                                entry_point: Some("main"),
                                compilation_options: Default::default(),
                                cache: None,
                            });
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(gpu.uncaptured_error_count(), 0);
}
