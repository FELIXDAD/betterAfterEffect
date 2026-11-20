# benches/

性能基准工程和历史结果（工作流 04）。测量方法见 `docs/perf.md`。

| 基准 | 内容 | 状态 |
|---|---|---|
| B1 动效图形 | 1080p30，10 秒，50 个形状图层 + 10 个文字图层 + 模糊和发光 | 已有（形状、文字用纯色和纹理代替；模糊、发光等 07） |
| B2 素材合成 | 4K ProRes 素材 + 调色 + 遮罩 | 未做（等遮罩和 06 的调色） |
| B3 压力测试 | 500 个图层，多层预合成嵌套 | 未做 |

- 基准程序：`crates/bae-render/benches/*.rs`，运行 `cargo xtask bench -p bae-render`。
- 场景定义：`bae_render::fixture`（B1 是 `fixture::b1`），通过 bae-model 命令搭建，和真实工程一样经过 schema 校验。也可以存成工程文件给 `bae render` 和 AE 对照用：`cargo run -p bae-render --example b1_project -- <目录>`。
- 每次运行把 JSON 报告写到 `target/bench/`（或 `BAE_BENCH_OUT`）。值得记录的结果手动追加到 `benches/results/`（文件名 `<日期>-<机器>.json`），CI 接入后自动记录趋势。
