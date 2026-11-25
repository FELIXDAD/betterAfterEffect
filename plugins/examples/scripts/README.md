# 示例脚本

AE 风格的脚本（`.jsx` / `.js`），用 BAE 的“文件 ▸ 脚本 ▸ 运行脚本文件…”运行，或放进脚本文件夹后出现在“文件 ▸ 脚本”菜单里。API 和注意事项见 [`docs/plugins/scripting.md`](../../../docs/plugins/scripting.md)。

| 文件 | 内容 |
|---|---|
| `lower-third.jsx` | 新建合成，色条 + 标题 + 控制空对象，空对象 0.5 秒滑入 |
| `bounce-selected.jsx` | 给选中图层在当前时间加缩放弹跳关键帧 |
| `rename-layers.jsx` | 按图层顺序把选中图层（或全部图层）重命名为 Shot 01、Shot 02… |
| `align-to-grid.jsx` | 把图层位置对齐到 10 像素网格（有关键帧时在当前时间加关键帧） |

这些脚本都有自动测试（`cargo test -p bae-script`）。
