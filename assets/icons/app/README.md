# 应用图标

BAE 自己设计的应用图标：深色圆角方块上，一条缓动曲线连着两个关键帧（菱形），带图表编辑器式的手柄。没有使用、也没有模仿任何 Adobe 的图标。

这里的文件都由 `cargo xtask icons` 生成（图形定义在 `xtask/src/icon.rs`），**不要手改**；改图形就改代码，再重新生成。`cargo test -p xtask` 会检查这里的文件和代码是否一致。

| 文件 | 用途 | 大小 |
|---|---|---|
| `bae.icns` | macOS 应用包图标（`Contents/Resources/bae.icns`，Info.plist 的 `CFBundleIconFile`）；16、32 px 用 `ARGB`，其余是 PNG，类型和 `iconutil` 写的一致 | 约 53 KB |
| `bae.ico` | Windows 图标，16 到 256 px（PNG） | 约 13 KB |
| `bae.res` | 同一套 Windows 图标编译成的资源文件（图标组 1）。`apps/bae/build.rs` 把它交给 MSVC 链接器，`bae.exe` 因此带图标，不需要 rc.exe 或额外依赖 | 约 14 KB |
| `bae.svg` | 矢量预览，构建不使用 | 1 KB |

- 文档图标（`.bae`、`.baez`）由 macOS 根据应用图标自动生成（`CFBundleTypeIconSystemGenerated`）。
- 小尺寸（32 px 及以下）去掉手柄，线条和关键帧保持最小屏幕尺寸，保证能看清。
- macOS 用 Apple 图标网格（1024 画布里 824 的圆角方块），Windows 的方块更满。
