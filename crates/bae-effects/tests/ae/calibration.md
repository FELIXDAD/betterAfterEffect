# 空间效果的 AE 26.2.1 网格校准

在 Apple M4 上将 BAE 的 GPU 输出与 AE 的 256 × 128 PNG 对齐。每个像素取预乘 RGBA 四通道的最大绝对差，再计算一张图的峰值和平均差。峰值和平均差分别取所有 Best 图中最差的一张，单位为 1/255。Draft 用于区分图层质量对采样的影响。

各分类记录网格范围、误差和采样结论。[扭曲](categories/distort.md#采样网格校准) · [风格化](categories/stylize.md#马赛克采样网格校准) · [模糊和锐化](categories/blur-sharpen.md#径向模糊和钝化蒙版复测)

另按 8、16、32 bpc 各渲染 30 张 SDR 输入图，覆盖旋转扭曲、凸出、波纹、光学补偿和马赛克、三种输入和开启 / 关闭状态。读取 PNG 原始通道，再折算为 8 位色阶，三档的峰值均为 1/255。32 bpc 的 PNG 输出把输入白色编码为 0.099992，按该档无效果白图归一化 RGB，Alpha 保留原值。HDR 超白和半透明采样由 `tests/distort.rs` 在 RGBA16F、RGBA32F 工作缓冲上另测。

## 复现

在仓库根目录执行以下命令。主网格包含 387 对 Best / Draft，另有 192 张 Best 验证图。光学补偿另外测量了 144 张条纹探针，条纹周期为 4、9 和 19 像素，每种周期的亮条宽度取半周期向下取整，六个相位偏移为 0、1、2、3、4 和 7 像素。日常 GPU 回归从这些测试中选取 16 张 AE 输出，保存在 `golden/cal_*.png`。

```sh
export BAE_07_ROOT="$(mktemp -d)"
cargo run -p bae-effects --example ae_compare -- prepare
python3 crates/bae-effects/tests/ae/calibration_grid.py "$BAE_07_ROOT"
tools/ae/aectl lock -- tools/ae/aectl run "$BAE_07_ROOT/run.jsx" --timeout 570
python3 crates/bae-effects/tests/ae/calibration_grid.py "$BAE_07_ROOT" --quality best
cargo run -p bae-effects --example ae_compare -- compare
```

第一批 AE 命令在 774 张 PNG 均已写出后恢复了一次启动弹框，退出码为 3。所有 PNG 均通过解码检查。补充批次使用同一把锁。最终 16 张回归图在一次持锁调用中重新生成，与选入仓库的像素逐字节相同。
