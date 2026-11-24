//! A device created while another one is in use is reported (#154); a lost device
//! does not count. Alone in its test binary so no other test creates devices.

use bae_gpu::{GpuContext, GpuContextDescriptor, GpuError, wgpu};

fn create() -> Option<std::sync::Arc<GpuContext>> {
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

#[test]
fn second_device_in_use_is_reported_lost_ones_are_not() {
    let Some(first) = create() else { return };
    assert_eq!(GpuContext::devices_in_use(), 1);
    assert_eq!(GpuContext::extra_devices_created(), 0);

    // Created one after the other on this thread: no shader work runs meanwhile.
    let second = create().unwrap();
    assert_eq!(GpuContext::devices_in_use(), 2);
    assert_eq!(GpuContext::extra_devices_created(), 1);
    drop(second);
    assert_eq!(GpuContext::live_count(), 1);

    first.device().destroy();
    let _ = first.device().poll(wgpu::PollType::Poll);
    assert!(first.is_lost());
    assert_eq!(GpuContext::devices_in_use(), 0);
    assert_eq!(GpuContext::live_count(), 1);

    // The app's recovery after a device loss: a new device while the lost one may
    // still be referenced somewhere.
    let replacement = create().unwrap();
    assert_eq!(GpuContext::extra_devices_created(), 1);
    assert_eq!(GpuContext::devices_in_use(), 1);
    drop(replacement);
}
