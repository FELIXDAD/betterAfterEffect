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

## 请求（其他工作流在这里留言）

### 14 ui/edit → 07（2026-11-20，Pr 剪辑规划）

依据：`docs/spec/pr/`（Premiere Pro 基础功能清单）和 `docs/spec/pr/edit-model.md`（剪辑数据模型），PR #73。

1. **过渡**：按 02 新增的 `EffectDef` "过渡"种类（两个输入 + 进度）注册内置过渡。M2 先要交叉溶解（`bae.cross_dissolve`）和渐隐为黑色（`bae.dip_to_black`）；M4 再加渐隐为白色、渐隐为颜色、胶片溶解、叠加溶解、推、滑动、拆分、线性擦除、径向擦除、时钟式擦除、交叉缩放、急摇，以及音频的恒定功率 / 恒定增益 / 指数淡化（音频归 08 处理也可以，只要注册在同一个表里）。清单见 `docs/spec/pr/effects-transitions.md` 附录。
2. **Pr 的 matchName 别名**：Pr 里从 AE 移植的效果 matchName 是 `AE.` 加 AE 的 matchName（例如 `AE.ADBE Gaussian Blur 2`）。导入 Pr 工程 / XML 时去掉前缀查 07 的实现即可；请在 `EffectRegistry` 查找时支持这个别名（或者在 07 注册时一并登记），Pr 独有的效果（Lumetri、极致键……）先登记为已知 ID，缺实现时走占位。
3. **剪辑常用效果的优先顺序**（供排期参考，PR-FX-013）：高斯模糊、裁剪、变换、投影、亮度和对比度、Lumetri 颜色（与 06 协作）、极致键、Alpha 调整、轨道遮罩键、黑白、色调、边角定位、镜像、偏移、基本 3D。
4. （M4）生成器：彩条和色调、黑场视频、透明视频、通用倒计时片头（Asset `kind = generator`，PR-PRJ-020）。

## 进度

[第一阶段清单、基准和接入记录](07-effects/history.md)

- 应用界面（G，#271）：效果菜单按 AE 目录列出 299 个效果（可用 / 置灰实时读效果注册表）、效果控件按参数定义画控件、效果和预设面板（搜索、分类、双击 / 拖放应用）、时间轴 fx 开关列。下拉中文选项走 `EffectInfo.param_options_zh`（包的 `choices` 表；Rust 效果的 `options_zh` 查表在 `rust_options_zh()` 接入）。

- [模糊和锐化](../../crates/bae-effects/tests/ae/categories/blur-sharpen.md#工作流进度)
- [通道](../../crates/bae-effects/tests/ae/categories/channel.md#工作流进度)
- [颜色校正](../../crates/bae-effects/tests/ae/categories/color.md#工作流进度)
- [表达式控制](../../crates/bae-effects/tests/ae/categories/controls.md#工作流进度)
- [扭曲](../../crates/bae-effects/tests/ae/categories/distort.md#工作流进度)
- [生成](../../crates/bae-effects/tests/ae/categories/generate.md#工作流进度)
- [抠像](../../crates/bae-effects/tests/ae/categories/keying.md#工作流进度)
- [遮罩](../../crates/bae-effects/tests/ae/categories/matte.md#工作流进度)
- [杂色和颗粒](../../crates/bae-effects/tests/ae/categories/noise-grain.md#工作流进度)
- [透视](../../crates/bae-effects/tests/ae/categories/perspective.md#工作流进度)
- [风格化](../../crates/bae-effects/tests/ae/categories/stylize.md#工作流进度)
- [过渡](../../crates/bae-effects/tests/ae/categories/transition.md#工作流进度)

### AE 过时效果核查（2026-11-25）

核对 `obsolete.json` 的 10 项和 `obsolete-hidden.json` 的 39 项。旧高斯模糊、方框模糊、阈值已注册；绘画由 #347 直接读取原 matchName。

- #393 补齐 7 项颜色别名。亮度和对比度、曝光度、照片滤镜、色阶、独立色阶、两个 Alpha 色阶版本共用各自的现代内核。
- #394 补齐 6 项空间与抠像别名。快速模糊、钝化蒙版、变换、设置遮罩、Photoshop 纯色填充、溢出抑制保留旧参数。
- 杂色、杂色 Alpha、杂色 HLS、杂色 HLS 自动、Cineon 转换器、CC Composite 共 6 项。后五项随 #351、#358、#365 的现代实现注册。旧 `CC Composite` 读取当前层源图。
- 余下 26 项跳过。基本文字、路径文本、应用颜色 LUT 的模态数据缺少可靠参数记录。基本 3D、颜色键、亮度键、闪光、减少交错闪烁、颜色平衡、三向颜色校正器、调整遮罩和五项 Photoshop 图层样式（斜面和浮雕、投影、内发光、内阴影、外发光）缺少可确认的完整参数映射。操控、Roto 笔刷和调整边缘、两个 CC PS Classic、CC Rain、CC Snow、两个 CC Time Blend 缺少已验证的变形、粒子或时序映射。3D 眼镜和 mocha shape 按标签跳过。

### 颜色校正第三批（2026-11-24）

完成应用颜色 LUT、色光、灰度系数 / 基值 / 增益、保留颜色、可选颜色、自动色阶、自动对比度、自动颜色、色调均化、广播颜色。四个整层直方图效果在 GPU 上归约。131 张 AE 26.2.1 参考图覆盖三个工程位深。自动颜色的“对齐中性中间调”经 2×2 色块和裁剪扫描校准，原近中性色卡平均差从 22.491 降至 2.297/255。其余误差见 #301 及效果库分类说明。

LUT 字节和文件名随项目保存。效果面板的文件选择按钮交由 G 接入。三个自动校正效果的瞬时平滑和场景检测等待 #286 提供跨帧输入。
