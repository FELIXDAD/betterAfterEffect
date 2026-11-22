# vendor/wgpu-hal：为什么有这份拷贝

这里是 crates.io 上的 wgpu-hal 30.0.2（wgpu 30.0.2 用的那个版本），通过根 `Cargo.toml` 的 `[patch.crates-io]` 替换掉注册表里的 wgpu-hal。wgpu、wgpu-core、wgpu-types、naga 等其余 wgpu crate 仍然从 crates.io 来。许可证（MIT 或 Apache-2.0）见同目录的 `LICENSE.MIT` 和 `LICENSE.APACHE`。

## 为什么（#194，最早在 #154 的 CI 取证里发现）

D3D12 后端用 FXC（`d3dcompiler_47.dll`）编译着色器时，得到的是一个 `ID3DBlob`。wgpu-hal 的每个设备都有一个着色器缓存（`src/dx12/device.rs` 的 `Device::load_shader`），机制如下：

- 缓存命中时，在缓存锁内把这个 blob clone 一份返回（COM `AddRef`）。
- 调用线程建完管线后，在锁外释放这份 clone（`Release`）。
- 多个线程在同一个设备上用同一个着色器建管线时，就会同时对同一个 blob 做 `AddRef` 和 `Release`。

问题出在 FXC 的 blob（`D3DX11Core::CBuffer`）不是自由线程对象。在 runner 的 d3dcompiler_47 10.0.26100.33828 上用 cdb 反汇编：

```
D3DX11Core::CBuffer::AddRef:
  mov     eax,dword ptr [rcx+8]
  inc     eax
  mov     dword ptr [rcx+8],eax
  ret
D3DX11Core::CBuffer::Release:
  mov     r8d,dword ptr [rcx+8]
  lea     ebx,[r8-1]
  mov     dword ptr [rcx+8],ebx      ; 没有 lock 前缀
  cmp     r8d,1
  je      → delete this
```

计数不是原子的，并发时会丢失更新，blob 被提前释放或重复释放，表现为堆损坏（`STATUS_HEAP_CORRUPTION` 0xc0000374，`ntdll!RtlpLogHeapFailure` ← `ucrtbase!free_base` ← `CBuffer::Release` ← `wgpu_hal::dx12::Device::create_render_pipeline`）。CI 转储里，崩溃时另外 3 个线程正好都在同一个设备的 `load_shader` 里。

这会影响产品：BAE 在 Windows 上一律用 FXC（`bae_gpu::dx12_compiler`），应用里所有部分共用一个设备，而预览渲染器、渲染队列线程、效果预热、多个 vello 渲染器会在不同线程上编译同样的着色器。和适配器无关，真显卡上一样会发生。DXC 的 blob 是原子计数，不受影响。

## 改了什么

第一个提交是 crates.io 上 30.0.2 的原样拷贝（只去掉了 `.cargo_vcs_info.json` 和 `Cargo.lock`）。第二个提交是补丁，只有两处：

- `src/dx12/shader_compilation.rs` 的 `compile_fxc`：编译成功后，把 blob 的字节拷进 `Vec<u8>`，在编译它的线程上立即释放 blob，返回 `CompiledShader::Precompiled(bytecode)`。缓存里存的和跨线程 clone 的都是 `Vec<u8>`，不再有共享的 COM 对象。`Precompiled` 原本就用于 HLSL 直通着色器，`create_native_shader` 已经支持。
- `src/dx12/mod.rs`：`CompiledShader::Fxc` 这个变体不再被构造，加 `#[allow(dead_code)]`，保持和上游的差异最小。

代价：每个 FXC 着色器编译后多一次字节拷贝（几 KB），缓存命中时 clone 的是 `Vec<u8>` 而不是 `AddRef`。发布版体积不变。

`git log -p -- vendor/wgpu-hal` 能看到原样拷贝和补丁这两个提交。

## 验证

`crates/bae-gpu/tests/concurrent_pipelines.rs`：4 个线程在同一个设备上反复用同一个着色器建计算管线。平时的 CI 每个线程跑 50 次；崩溃复现 workflow（`.github/workflows/win-crash-repro.yml`，分支 `ci/win-crash-repro`）把次数调大，在 windows-latest（WARP + FXC）上对比去掉补丁和打上补丁两种情况，结果记在 #194。

## 什么时候删掉

wgpu 升级时，看上游的 `wgpu-hal/src/dx12/device.rs` 的 `load_shader` 和 `shader_compilation.rs` 的 `compile_fxc`：

- 如果上游已经不再跨线程共享 FXC blob（比如改成存字节，或者 clone 和释放都放在锁内），删掉这个目录和根 `Cargo.toml` 里 `[patch.crates-io]` 的 `wgpu-hal` 一行，同时去掉 `[workspace] exclude` 里的 `vendor/wgpu-hal`。
- 如果还没改，把上面那处补丁带到新版本的拷贝上（同样分成原样拷贝和补丁两个提交），并重跑 `concurrent_pipelines` 的压力对比。
- 如果 BAE 改用 DXC（`dxcompiler.dll`），这个补丁就不需要了。
