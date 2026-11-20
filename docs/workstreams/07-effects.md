# 07 · 效果库

| 项 | 内容 |
|---|---|
| 分支 | `render/effects` |
| Orca 工作区 | `bae-07-effects` |
| 阶段 | P2 |
| 依赖 | 04（Effect trait）、06 |
| 负责目录 | `crates/bae-effects/` |

## 目标

按 AE 用户的使用频率，用 WGSL 实现内置效果。参数名、默认值、取值范围都与 AE 一致，方便 AE 工程导入后直接对上。

## 要做

- **第一批（M1）**：高斯模糊、快速方框模糊、方向模糊、径向模糊、发光、投影、填充、色调、三色调、曲线、色阶、色相/饱和度、曝光度、亮度和对比度、梯度渐变、四色渐变、线性擦除、径向擦除、百叶窗、变换、动态拼贴、分形杂色、湍流置换、置换图、简单阻塞工具、毛边。
- **第二批**：残影（Echo）、时间置换、CC 系列常用效果的等价实现、粒子（类似 CC Particle World 的基础版）、镜头光晕、锐化、杂色与颗粒。
- **每个效果**：
  - 记录对应的 AE matchName，供 15 做映射。
  - 参数定义与 AE 相同（名称、顺序、默认值、范围）。
  - golden image 测试：与 AE 渲染的参考图对比，并给出允许的误差。
  - 正确声明需要扩展的边界（例如模糊会让画面变大）。
- **通过插件接口注册**：内置效果和第三方插件效果走同一套接口（19），能用声明式 WGSL 插件形态实现的效果优先用这种形态。
- **分类元数据**：给"效果和预设"面板用的分类与搜索关键词（中英文）。

## 对外接口

- 通过 04 的注册机制注册所有效果。
- `docs/interop/ae-effects.md`：AE matchName → BAE 效果的对照表（与 15 共享）。

## 验收

- 第一批效果全部有 golden image 测试，并在 B1 基准中不拖慢实时预览。
- 4K 高斯模糊（半径 100）单帧 < 5 ms（M1 / RTX 3060 级别）。

## 不做

- 效果控件面板的界面（12）。

## 进度

- [ ] 第一批效果（进行中）
  - [x] `crates/bae-effects`：注册接口 `register`/`builtins`/`get`/`catalog`/`prewarm`，`Standalone` 渲染目标（测试/基准用）
  - [x] 颜色校正（声明式 WGSL 内嵌包，走 19 的 `DeclarativeEffect::load_builtin`）：色调、三色调、色阶、色阶（单独控件）、色相/饱和度、亮度和对比度、曝光度
  - [x] 模糊（Rust 效果）：高斯模糊（含旧版）、快速方框模糊（含旧版 Box Blur）；大 σ 走降采样金字塔
  - [ ] 曲线、填充、梯度渐变、四色渐变、线性/径向擦除、百叶窗、分形杂色、定向/径向模糊、发光、投影、简单阻塞工具、变换、动态拼贴、湍流置换、置换图、毛边
- [ ] matchName 对照表 `docs/interop/ae-effects.md`
- [ ] golden image 测试：AE 对照用例和脚本已写（`tests/ae/`），需要在装了 AE 的机器上跑
- [ ] 第二批效果

### 0.0.1 记录（2026-11-21，暂停）

- 文档身份用 AE matchName（与 02 数据模型一致，AE 工程不用查表），参数 id = AE disk id；内置效果 `plugin = None`。
- 已 rebase 到 dev（`EffectParams` 已有 R04-11 的字段，测试和基准改用 `EffectParams::new`）。
- **性能回退已查明并修复**：原因是 wgpu 默认把 workgroup 内存清零（生成的 Metal 代码里每个工作组串行清零），不是权重下标。`gpu.rs` 里关掉 `zero_initialize_workgroup_memory`（所有核都先写后读）。另外：卷积核每个线程算 4 个输出、权重放 workgroup 内存；降采样展开 4 个独立读取；金字塔阈值从 σ ≥ 8 改为 σ ≥ 6（层上 σ 在 3–6）。
- 基准（M1 Pro，dev profile、关闭 GPU 校验，GPU 时间为主）：4K 高斯 模糊度 100 = 1.16 ms/帧（原 2.81）、模糊度 25 = 2.48（原 10.5）、模糊度 10 = 4.3、模糊度 300 = 1.36；颜色校正 4K ≈ 0.9–1.1。金字塔误差 ≤ 0.0051（含 3.0 的 HDR 输入，约 0.4/255）。
- **已知问题（下一步先修）**：`tests/blur.rs::fast_box_blur` 偶发失败（6 次里 3 次）：`box r=4 n=1` 在 (9, -3) 得到 0、应为 0.9。关掉 workgroup 清零后新卷积核可能有未写先读或边界问题，需要先查清再推进；快速方框模糊大半径（4K r20×3 = 23 ms、r100×3 = 29 ms）还慢。
- 还没跑 `cargo clippy --workspace`、`cargo xtask size`；没请求 Windows 实机测试。
- 下一步：修上面的偶发失败 → 方框模糊优化 → 第一批剩下的 17 个效果 → `docs/interop/ae-effects.md`、README → clippy/size → Windows 实机基准（RTX 5090 D：`cargo bench -p bae-effects --bench effects -- 4K`、`cargo test -p bae-effects`）。

## 请求（其他工作流在这里留言）

## 请求（其他工作流在这里留言）

### 14 ui/edit → 07（2026-11-20，Pr 剪辑规划）

依据：`docs/spec/pr/`（Premiere Pro 基础功能清单）和 `docs/spec/pr/edit-model.md`（剪辑数据模型），PR #73。

1. **过渡**：按 02 新增的 `EffectDef` "过渡"种类（两个输入 + 进度）注册内置过渡。M2 先要交叉溶解（`bae.cross_dissolve`）和渐隐为黑色（`bae.dip_to_black`）；M4 再加渐隐为白色、渐隐为颜色、胶片溶解、叠加溶解、推、滑动、拆分、线性擦除、径向擦除、时钟式擦除、交叉缩放、急摇，以及音频的恒定功率 / 恒定增益 / 指数淡化（音频归 08 处理也可以，只要注册在同一个表里）。清单见 `docs/spec/pr/effects-transitions.md` 附录。
2. **Pr 的 matchName 别名**：Pr 里从 AE 移植的效果 matchName 是 `AE.` 加 AE 的 matchName（例如 `AE.ADBE Gaussian Blur 2`）。导入 Pr 工程 / XML 时去掉前缀查 07 的实现即可；请在 `EffectRegistry` 查找时支持这个别名（或者在 07 注册时一并登记），Pr 独有的效果（Lumetri、极致键……）先登记为已知 ID，缺实现时走占位。
3. **剪辑常用效果的优先顺序**（供排期参考，PR-FX-013）：高斯模糊、裁剪、变换、投影、亮度和对比度、Lumetri 颜色（与 06 协作）、极致键、Alpha 调整、轨道遮罩键、黑白、色调、边角定位、镜像、偏移、基本 3D。
4. （M4）生成器：彩条和色调、黑场视频、透明视频、通用倒计时片头（Asset `kind = generator`，PR-PRJ-020）。
