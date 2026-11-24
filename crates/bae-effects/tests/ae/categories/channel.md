# 通道与 AE 26 的像素对照

## 已有效果

| AE matchName | 名称 | 实现 |
|---|---|---|
| `ADBE Invert` | 反转 | 声明式像素 |
| `ADBE Minimax` | 最小/最大 | Rust |
| `ADBE Set Matte3` | 设置遮罩 | Rust |
| `ADBE Shift Channels` | 转换通道 | 声明式像素 |

`cases-controls-channel.json` 的 39 个用例由一次 `aectl run` 批量生成。AE 的启动提示框由 `aectl` 恢复后，批次正常完成。输入采用不透明色块与羽化 alpha 圆盘，PNG 输出是 8 bpc。对照按预乘 RGBA 比较，使用每像素四通道中的最大差，再计算整图平均。

| 效果 | 最大差 / 平均差（8 位色阶） | 观察 |
|---|---:|---|
| Invert | ≤1 / ≤0.775 | HLS 色相按半圈镜像，反转饱和度后限制最大色度，整数项目在混合前截断反转结果。 |
| Shift Channels | ≤1 / ≤0.012 | 饱和度取 RGB 极差乘以同亮度可用的最大色度。 |
| Set Matte | ≤1 / ≤0.033 | AE 用例取自身源，另有 GPU 测试覆盖隐藏图层作为源、居中和半分辨率。 |
| Minimax | 0 / 0 | 测试覆盖最小、最大、先最小再最大，以及透明边缘。 |

Invert Alpha、Shift Channels 和 Set Matte 在 AE 中会重新显露直通素材全透明像素的 RGB。声明式像素效果可通过 `bae_input_straight` 从源纹理读取该值，已经预乘且丢失 RGB 的输入仍只能使用可恢复的信息。

重跑该批次时设置独立工作目录。用 `aectl lock --` 包住整批 AE 操作，由工具获取和释放共享锁。

```sh
export BAE_07_ROOT=/private/tmp/bae-controls-channel-ae
export BAE_07_CASES="$PWD/crates/bae-effects/tests/ae/cases-controls-channel.json"
cargo run -p bae-effects --example ae_compare -- prepare
cat > "$BAE_07_ROOT/run.jsx" <<EOF_JS
var BAE_07_ROOT = "$BAE_07_ROOT";
\$.evalFile("$PWD/crates/bae-effects/tests/ae/render_cases.jsx");
EOF_JS
tools/ae/aectl lock -- tools/ae/aectl run "$BAE_07_ROOT/run.jsx"
cargo run -p bae-effects --example ae_compare -- compare
cargo run -p bae-effects --example ae_compare -- golden
```

## 工作流进度

2026-11-24，`effects/controls-channel` 完成通道效果，与表达式控制、颜色校正和遮罩一起生成 39 个 AE 26.2.1 用例。Set Matte 支持当前帧的源阶段图层取样，界面上的源阶段切换由效果控件工作流接入。
