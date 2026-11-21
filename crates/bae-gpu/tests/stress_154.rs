//! Crash-repro stress for #154 (only on the `ci/win-crash-repro` branch): several
//! threads create GPU contexts at the same moment and build pipelines, the pattern
//! that crashed in dxilconv.dll on WARP before device creation was serialized.

use std::sync::{Arc, Barrier};

use bae_gpu::{GpuContext, GpuContextDescriptor, GpuError, wgpu};

const THREADS: usize = 4;

fn create() -> Option<Arc<GpuContext>> {
    match GpuContext::new_headless(GpuContextDescriptor::default().with_env()) {
        Ok(ctx) => Some(ctx),
        Err(GpuError::NoAdapter { .. }) => None,
        Err(error) => panic!("GPU context creation failed: {error}"),
    }
}

/// A compute pipeline with its own shader text, so D3D12 converts a new shader.
fn pipeline(ctx: &GpuContext, salt: usize) {
    let module = ctx
        .device()
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stress"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "@group(0) @binding(0) var<storage, read_write> data: array<u32>;\n\
                     @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{\n\
                         data[id.x] = data[id.x] * {}u + {}u;\n\
                     }}",
                    salt + 3,
                    salt + 1
                )
                .into(),
            ),
        });
    let _ = ctx
        .device()
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("stress"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
}

fn concurrent_creation() {
    let barrier = Arc::new(Barrier::new(THREADS));
    let threads: Vec<_> = (0..THREADS)
        .map(|t| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let ctx = create()?;
                pipeline(&ctx, t);
                Some(ctx)
            })
        })
        .collect();
    let contexts: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    drop(contexts);
}

fn churn() {
    let barrier = Arc::new(Barrier::new(THREADS));
    let threads: Vec<_> = (0..THREADS)
        .map(|t| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                for i in 0..4 {
                    let Some(ctx) = create() else { return };
                    pipeline(&ctx, 100 + t * 10 + i);
                    drop(ctx);
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
}

#[test]
fn contexts_created_concurrently_and_churned() {
    concurrent_creation();
    churn();
}
