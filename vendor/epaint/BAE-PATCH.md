# vendor/epaint：为什么有这份拷贝

这里是 crates.io 上的 epaint 0.36.2（egui 提交 `49682f8b`），通过根 `Cargo.toml` 的 `[patch.crates-io]` 替换掉注册表里的 epaint。egui、egui-wgpu、egui-winit 等其余 egui crate 仍然从 crates.io 来。

## 为什么

egui 0.36.2 的 epaint 用 skrifa 0.44 + harfrust 0.12（+ read-fonts 0.41），vello 0.12 和 parley 0.13 用 skrifa 0.49 + harfrust 0.15（+ read-fonts 0.46）。两套字体栈同时链接进 `bae`，Windows 上 cargo-bloat 里 skrifa + harfrust + read_fonts 合计超过 1.8 MiB，其中一套是纯重复。

几条路的比较（2026-11-20）：

| 做法 | 结果 |
|---|---|
| 升级 egui 到 0.37.0（egui_dock 0.22 已支持） | 不解决：epaint 0.37.0 仍是 skrifa 0.44 / harfrust 0.12，还会多一份 vello_cpu 0.2 |
| vello 降到 0.11、parley/fontique 降到 0.12（它们用 skrifa 0.44） | 能统一，但丢掉 vello 0.12 修的 DX12 + FXC 多路径 GPU 挂起（vello#1992），以及 parley 0.13 的一批排版修复（空文本光标、正 descent 字体、`slnt` 斜体方向、`trak` 字距），bae-text 还要改 API。不可取 |
| 本补丁：把 egui 上游已合并的字体依赖升级（emilk/egui#8874，提交 `1a591f8e`）回移到 0.36.2 | 统一到 vello/parley 那一套，改动只有 epaint 文字排版里调用 harfrust 的几十行，egui 公开 API 不变 |

## 改了什么

和 crates.io 原版的差别只有：

- `Cargo.toml`：harfrust `0.12.0` → `0.15.0`，skrifa `0.44.0` → `0.49.0`；去掉 bench 目标和 dev-dependencies（没拷 `benches/`），`publish = false`。
- `src/text/font.rs`、`src/text/fonts.rs`、`src/text/text_layout.rs`：harfrust 0.15 的 API（`ShaperData` → `harfrust::Font`，`UnicodeBuffer`/`GlyphBuffer` → `Buffer`，`harfrust::shape`），逐行对应上游 `1a591f8e` 里同名函数的改动。

`git log -p -- vendor/epaint` 能看到：第一个提交是原样拷贝，第二个提交是回移的差异。

## 验证

用同一组字符串（中英混排、PingFang/SF、emoji、连字、可变字体 `wght`、自动换行，1x 和 2x）分别跑原版和补丁版 epaint：

- 每个字形的位置、advance、UV 完全一致；字形图集像素哈希一致。
- 不缓存的排版耗时：原版平均 0.58 ms/轮，补丁版 0.51 ms/轮（50 个排版任务，Apple M4），没有变慢。

## 什么时候删

egui 组升级到包含 #8874 的版本（预计 egui 0.38，且 egui_dock 跟上）时，删掉 `vendor/epaint` 和根 `Cargo.toml` 里的 `[patch.crates-io]`/`exclude` 两处。在那之前不要在这里做别的修改；需要改 epaint 行为的，先提到上游。
