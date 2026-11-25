# duotone-rs：Rust 原生效果示例

用 `bae-plugin-sdk` 写的原生 CPU 效果：把每个像素的亮度映射到“暗部颜色”和“亮部颜色”之间的渐变，
再按“数量”与原图混合。`src/lib.rs` 只描述参数、处理像素；C ABI 的入口函数、vtable 和参数定义由
`bae_plugin_sdk::export_plugin!` 生成，回调里的 panic 会变成 `BAE_ERR_PANIC`（规则 R8），宿主据此
禁用插件，不会崩溃。

清单里没有 `[[effect.param]]`：宿主直接用 `define_params` 的结果（来自 `Duotone::PARAMS`）。清单
声明 `isolation = "out-of-process"`，插件在单独的进程里运行（规格 §15）。

## 构建

```sh
cargo build --release
```

把产物复制到 `bae-plugin.toml` 里 `[native.libraries]` 写的位置：

| 平台 | 产物 | 放到 |
|---|---|---|
| macOS（Apple Silicon） | `target/release/libduotone_rs.dylib` | `bin/macos-aarch64/` |
| Windows（x64） | `target/release/duotone_rs.dll` | `bin/windows-x86_64/` |
| Linux | `target/release/libduotone_rs.so` | `bin/linux-x86_64/` |

然后把整个目录放进用户插件目录（或用 `BAE_PLUGIN_PATH` 指向它），重启 BAE 或在插件管理里重新加载；
效果出现在“效果 ▸ 颜色校正 ▸ 双色调”。`bae plugin check <目录>` 检查清单。

## 测试

`cargo test -p bae-plugin --test native` 会构建这个示例，用宿主的同一套代码分别在进程内和进程外
加载、渲染，并核对像素。
