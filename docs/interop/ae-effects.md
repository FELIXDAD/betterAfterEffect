# AE 效果 → BAE 效果对照表

与 15（AE 互转）共用，由 07 维护。BAE 内置效果的文档身份就是 AE matchName，参数 `param:NNNN` 的 NNNN 等于 AE 参数 matchName 末尾的序号（disk id，参数组开始/结束各占一个），所以导入时效果和参数都不用查表；这张表记录每个 AE 效果在 BAE 里的支持程度和已知差异。

状态：**已实现** / 计划（第一批，07 任务书）/ 不支持（导入为"缺失效果"占位，参数原样保留，导回 AE 时还原）。

| AE matchName | 名称（中文） | 状态 | BAE 实现 | 说明 |
|---|---|---|---|---|
| `ADBE Gaussian Blur 2` | 高斯模糊 | 已实现 | `bae.effects/gaussian-blur`（Rust） | σ = 0.3 × 模糊度（待 AE 实测）；默认值按 AE 26（模糊度 25、重复边缘像素开） |
| `ADBE Gaussian Blur` | 高斯模糊（旧版） | 已实现 | `bae.effects/gaussian-blur-legacy` | 与新版同一个核；AE 旧版核宽度略有不同，待实测 |
| `ADBE Box Blur2` | 快速方框模糊 | 已实现 | `bae.effects/fast-box-blur`（Rust） | 小数半径时最外两个采样按小数部分加权（待实测） |
| `ADBE Box Blur` | 方框模糊（旧 matchName） | 已实现 | `bae.effects/box-blur` | 同上，默认迭代 1 |
| `ADBE Tint` | 色调 | 已实现 | 声明式 | 亮度权重用 Rec.709（待实测）；参数 4（交换颜色按钮）无值 |
| `ADBE Tritone` | 三色调 | 已实现 | 声明式 | 同上 |
| `ADBE Easy Levels2` | 色阶 | 已实现 | 声明式 | 参数 2（直方图，任意数据）不声明，导入值原样保留；五个数值作用于"通道"选中的通道（AE 里各通道的值存在直方图数据里，尚未解码） |
| `ADBE Pro Levels2` | 色阶（单独控件） | 已实现 | 声明式 | 先单通道再 RGB 合成（顺序待实测） |
| `ADBE HUE SATURATION` | 色相/饱和度 | 已实现 | 声明式 | 主色相/饱和度/亮度和彩色化；"通道控制"选红色等时把主值限定到对应色相范围（AE 的各范围值存在参数 3 的任意数据里，尚未解码） |
| `ADBE Brightness & Contrast 2` | 亮度和对比度 | 已实现 | 声明式 | 新版曲线是拟合公式（待实测）；旧版为线性 |
| `ADBE Exposure2` | 曝光度 | 已实现 | 声明式 | 非线性工作空间按 sRGB 曲线转线性光 |
| `ADBE CurvesCustom` | 曲线 | 计划 | | |
| `ADBE Fill` | 填充 | 计划 | | |
| `ADBE Ramp` | 梯度渐变 | 计划 | | |
| `ADBE 4ColorGradient` | 四色渐变 | 计划 | | |
| `ADBE Linear Wipe` | 线性擦除 | 计划 | | |
| `ADBE Radial Wipe` | 径向擦除 | 计划 | | |
| `ADBE Venetian Blinds` | 百叶窗 | 计划 | | |
| `ADBE Fractal Noise` | 分形杂色 | 计划 | | |
| `ADBE Motion Blur` | 定向模糊 | 计划 | | |
| `ADBE Radial Blur` | 径向模糊 | 计划 | | |
| `ADBE Glo2` | 发光 | 计划 | | |
| `ADBE Drop Shadow` | 投影 | 计划 | | |
| `ADBE Simple Choker` | 简单阻塞工具 | 计划 | | |
| `ADBE Geometry2` | 变换 | 计划 | | |
| `ADBE Tile` | 动态拼贴 | 计划 | | |
| `ADBE Turbulent Displace` | 湍流置换 | 计划 | | |
| `ADBE Displacement Map` | 置换图 | 计划 | | |
| `ADBE Roughen Edges` | 毛边 | 计划 | | |

其他 AE 效果目前都按"缺失效果"占位处理。规格来源：`docs/spec/ae/_parts/effects--fx-*.md`。与 AE 的逐像素对照用例在 `crates/bae-effects/tests/ae/`（需要装了 AE 的机器跑）。
