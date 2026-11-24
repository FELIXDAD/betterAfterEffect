# 与 After Effects 对照（效果库）

| 文件 | 用途 |
|---|---|
| `cases.json` | 用例（两边共用）：输入图、效果 matchName、参数（按 AE 参数序号）、`tolerance`（允许的最大单通道差）、`mean`（允许的平均差），单位都是 8 位色阶。 |
| `render_cases.jsx` | 在 AE 里逐个建合成（输入图尺寸、用例位深（默认 8 bpc）、不线性混合、无色彩管理、图层质量"最佳"），按 matchName 设参数，`saveFrameToPng` 输出第 0 帧。 |
| `harness.rs` | 生成输入图（不提交二进制素材），用 BAE 渲染同样的用例（用例指定的位深）并比较。 |
| `golden/` | AE 26.2.1（macOS）的输出，`tests/ae_golden.rs` 在 CI 里用它比较，不需要 AE。软件 / 虚拟适配器（CI 的 WARP）上最大差加 2、平均差加 0.5。 |

重新生成：

```sh
cargo run -p bae-effects --example ae_compare -- prepare           # 输入图 + cases.json → /private/tmp/bae-07-ae
tools/ae/aectl lock -- tools/ae/aectl run crates/bae-effects/tests/ae/render_cases.jsx    # AE → /private/tmp/bae-07-ae/ae
cargo run -p bae-effects --example ae_compare -- compare           # BAE 渲染 + 对比表
cargo run -p bae-effects --example ae_compare -- golden            # 把 AE 输出拷进 golden/
```

探针（拟合核和曲线用，不入库）：另建一个目录放 `inputs/` 和自己的 `cases.json`，用包装脚本 `var BAE_07_ROOT = "<目录>"; $.evalFile("<仓库>/crates/bae-effects/tests/ae/render_cases.jsx");` 交给 `aectl run`，再 `BAE_07_ROOT=<目录> cargo run -p bae-effects --example ae_compare -- compare`。输入图除了 `chart`、`soft` 还有 `step`（竖直硬边）、`line`（1 像素白线）、`lineg`（灰底白线，能看到负旁瓣）、`ramp`（灰阶，下半是半幅）。

注意：`saveFrameToPng` 返回时文件还没写完，要用新的 `File` 对象轮询（同一个对象会缓存 `exists`/`length`）；`app.newProject()` 遇到没保存的工程会弹保存框，脚本先 `close(DO_NOT_SAVE_CHANGES)`。

## 分类对照

**偏差来自算法，不是 AE 版本或项目设置。** 07 第一次对照（Windows AE 26.0，`docs/test-reports/win5090/effects-ae-compare.txt`）和本机 macOS AE 26.2.1 用同一个脚本渲出来的结果与 BAE 的差逐项相同（到平均差小数点后三位）。两边的项目设置都是 8 bpc、不线性混合、无色彩管理。

所有效果的输出在 8 / 16 bpc 下按 AE 整数缓冲的方式截断到 [0, 1]（`EffectParams.float = false`，声明式插件的 `BAE_FRAME_FLOAT` 标志），32 bpc 保留超出范围的值。

- [模糊和锐化](categories/blur-sharpen.md)
- [通道](categories/channel.md)
- [颜色校正](categories/color.md)
- [扭曲](categories/distort.md)
- [表达式控制](categories/controls.md)
- [生成](categories/generate.md)
- [遮罩](categories/matte.md)
- [抠像](categories/keying.md)
- [杂色和颗粒](categories/noise-grain.md)
- [透视](categories/perspective.md)
- [风格化](categories/stylize.md)
- [过渡](categories/transition.md)
