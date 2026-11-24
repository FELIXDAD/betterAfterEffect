# 效果库早期进度

## 第一阶段清单

- [ ] 第一批效果（0.0.1 先合并第一阶段，#70）
  - [x] `crates/bae-effects`：注册接口 `register` / `builtins` / `get` / `catalog` / `prewarm`，`Standalone` 渲染目标（测试、基准用）
  - [x] 颜色校正（声明式 WGSL 内嵌包，走 19 的 `DeclarativeEffect::load_builtin`）：色调、三色调、色阶、色阶（单独控件）、色相/饱和度、亮度和对比度、曝光度
  - [x] 模糊（Rust 效果）：高斯模糊（含旧版）、快速方框模糊（含旧版 Box Blur）
  - [ ] 曲线、填充、梯度渐变、四色渐变、线性/径向擦除、百叶窗、分形杂色、定向/径向模糊、发光、投影、简单阻塞工具、变换、动态拼贴、湍流置换、置换图、毛边（下一阶段）
- [x] matchName 对照表 `docs/interop/ae-effects.md`（已实现的 11 个 + 计划中的第一批）
- [x] golden image 测试：AE 26.2.1 的 26 个用例入库（`crates/bae-effects/tests/ae/golden/`），每个用例限定最大差和平均差；偏差来源和修正见 `crates/bae-effects/tests/ae/README.md`
- [ ] 亮度和对比度新版曲线改用 AE 实测曲线表（Rust 效果 + 曲线纹理）
- [x] 应用接入：`bae_effects::EffectsModule` 把内置效果发布到 `Registry<EffectDef>` / `Registry<dyn Effect>` / `Registry<EffectInfo>`；查看器、渲染队列、`bae render` 从注册表同步效果库（`Renderer::sync_effects`）；调整图层、图层 fx 开关；`effect.*` 命令（bae-motion）
- [ ] 界面：效果菜单按 AE 目录列出、效果控件按参数定义画控件、效果和预设面板（PR-C）
- [ ] 第二批效果

### 0.0.1 记录（2026-11-21）

- 文档身份用 AE matchName（与 02 数据模型一致，AE 工程不用查表），参数 id = AE disk id；内置效果 `plugin = None`。
- 性能回退的原因：wgpu 默认清零 workgroup 内存，生成的 Metal 代码在每个工作组里串行清零，卷积核慢了约 100 倍。07 的核都先写后读，已关掉清零。另外每线程算 4 个输出、降采样展开读取、金字塔阈值 σ ≥ 6。
- 偶发失败的原因：uniform 环形缓冲新建时未初始化，wgpu 的延迟清零和其他线程写入的槽位冲突，整张输出变 0。改为创建时映射（整块视为已初始化）后稳定通过。
- 基准（M1 Pro，GPU 时间为主）：4K 高斯 模糊度 100 = 1.16 ms/帧、模糊度 25 = 2.48、模糊度 10 = 4.3；颜色校正 4K ≈ 0.9–1.1 ms。
- 已知：快速方框模糊大半径偏慢（4K r20×3 ≈ 23 ms）；19 的 `DeclarativeEffect` 管线缓存不区分设备（测试里每个测试二进制共用一个 `GpuContext` 绕过）。
- 体积：`bae-effects` 还没有链进应用，release 体积不变；根 `Cargo.toml` 只加了一行路径依赖。

### 2026-11-24 记录（G：效果接入应用，分支 `render/effects-app`）

- 在本机 macOS AE 26.2.1 上重跑第一次对照：与 Windows AE 26.0 的结果逐项相同，偏差全部来自算法。已按实测修正：色调 / 三色调用 Rec.601 权重；色阶"32 bpc 时关闭"在 8 / 16 bpc 等于开、剪切关闭时镜像延伸；曝光度用纯幂 2.4 转线性；高斯模糊在线性光里做、σ = 0.288 × 模糊度、核截在 3σ；旧版高斯按实测 σ 表；快速方框模糊默认重复边缘像素。
- `EffectParams.float`（32 bpc）和声明式插件的 `BAE_FRAME_FLOAT`：8 / 16 bpc 时像素式效果的输出截断到 [0, 1]。声明式效果的 `frame.flags` 现在带上 `BAE_FRAME_LINEAR` / `BAE_FRAME_FINAL`（原来固定为 HIGH_QUALITY）。

### 效果按分类登记（2026-11-24）

生成、杂色和过渡与六个颜色校正效果合在 #263。每个分类有独立的注册模块、基准文件和 AE 对照说明。外部渐变图层、颗粒蒙版、填充的蒙版路径由 04/19 在 #286 跟进，07 在接口就绪后接回。
