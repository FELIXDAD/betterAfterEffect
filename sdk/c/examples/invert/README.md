# invert：纯 C 原生效果示例

最小的原生 CPU 效果：反转图层颜色，再按"数量"（0–100 %）与原图混合。只依赖
`sdk/c/bae_plugin.h`，不需要 Rust SDK。

演示的内容：

- 唯一导出的入口函数 `bae_plugin_entry` 和插件侧的 API 版本检查（规格 §6.6）；
- 可扩展结构体：读写都不超过 `struct_size`（§14.6 规则 R3）；
- 按（名字，版本）获取冻结的宿主套件（规则 R4）；
- `BaeEffectVTable1` 的 `global_setup` / `define_params` / `render_cpu`；
- `pre_render` 为 `NULL`，即输入区域等于输出区域；
- CPU 图像的矩形、行跨度和像素格式（RGBA F32 / U8，预乘）；
- 轮询 `is_aborted`，用 `bae.core` 的 `set_error` 报告错误。

## 构建

产物放在 `bin/<平台>/`，与 `bae-plugin.toml` 里 `[native.libraries]` 的路径一致。二进制不要提交。

macOS（Apple Silicon）：

```sh
mkdir -p bin/macos-aarch64
cc -std=c11 -O2 -fvisibility=hidden -dynamiclib -I../.. invert.c -o bin/macos-aarch64/libinvert.dylib
```

Windows（x64，MSVC 开发者命令行）：

```bat
mkdir bin\windows-x86_64
cl /O2 /LD /I..\.. invert.c /Fe:bin\windows-x86_64\invert.dll
```

Linux（无界面渲染 / CI）：

```sh
mkdir -p bin/linux-x86_64
cc -std=c11 -O2 -fvisibility=hidden -shared -fPIC -I../.. invert.c -o bin/linux-x86_64/libinvert.so
```

## 测试

`sdk/c/tests/check.sh` 会编译本示例，并用 `sdk/c/tests/mini_host.c`（一个只依赖头文件的迷你宿主）
加载它：检查导出、版本协商、参数定义，再用 F32 和 U8 两种格式渲染并核对像素。

```sh
sdk/c/tests/check.sh
```
