# AE 效果 → BAE 效果对照表

与 15（AE 互转）共用，由 07 维护。BAE 内置效果的文档身份就是 AE matchName，参数 `param:NNNN` 的 NNNN 等于 AE 参数 matchName 末尾的序号（disk id，参数组开始/结束各占一个），所以导入时效果和参数都不用查表；这张表记录每个 AE 效果在 BAE 里的支持程度和已知差异。

状态：**已实现** / 计划（第一批，07 任务书）/ 不支持（导入为"缺失效果"占位，参数原样保留，导回 AE 时还原）。

| AE matchName | 名称（中文） | 状态 | BAE 实现 | 说明 |
|---|---|---|---|---|
| `ADBE Linear Color Key2` | 线性颜色键 | 已实现 | 声明式 | 叠加“保留颜色”恢复栈前颜色等待渲染图输入 |
| `ADBE Color Range` / `ADBE Color Difference Key` | 颜色范围 / 颜色差值键 | 已实现 | 声明式 | 可用参数编辑，自定义预览和吸管尚未接入 |
| `ADBE Spill2` | 高级溢出抑制器 | 已实现 | 声明式 | 标准最大差 1；极致模式为近似 |
| `ADBE Matte Choker` | 遮罩阻塞工具 | 已实现 | Rust | 两阶段遮罩卷积、灰度柔和度及迭代 |
| `ADBE PS Median` / `ADBE Dust & Scratches` | 中间值 / 蒙尘与划痕 | 已实现 | 声明式计算 | 方形中值邻域、可处理 alpha |
| `ADBE Checkerboard` / `ADBE Circle` / `ADBE ELLIPSE` | 棋盘 / 圆形 / 椭圆 | 已实现 | 声明式 | 棋盘与圆形对照最大差 2，椭圆硬边的亚像素覆盖有差异 |
| `ADBE Cell Pattern` | 单元格图案 | 已实现 | 声明式计算 | 同种子的站点布局与 AE 不同，支持平铺与循环演化 |
| `ADBE Gaussian Blur 2` | 高斯模糊 | 已实现 | `bae.effects/gaussian-blur`（Rust） | 线性光（纯幂 2.4）里模糊，σ = 0.288 × 模糊度，核截在 3σ（AE 26 实测）；AE 核的微小负旁瓣未复现（暗通道紧邻亮边处差异，平均差 ≤ 3.7 级）；默认值按 AE 26（模糊度 25、重复边缘像素开） |
| `ADBE Gaussian Blur` | 高斯模糊（旧版） | 已实现 | `bae.effects/gaussian-blur-legacy` | 不转线性光，σ 按 AE 26 实测表（与模糊度不成正比），最大差 ≤ 4 级 |
| `ADBE Box Blur2` | 快速方框模糊 | 已实现 | `bae.effects/fast-box-blur`（Rust） | 小数半径时最外两个采样按小数部分加权（AE 实测一致）；默认重复边缘像素开（AE 26） |
| `ADBE Box Blur` | 方框模糊（旧 matchName） | 已实现 | `bae.effects/box-blur` | 同上，默认迭代 1 |
| `ADBE Tint` | 色调 | 已实现 | 声明式 | 亮度权重 Rec.601（AE 26 实测，差 ≤ 1 级）；参数 4（交换颜色按钮）无值 |
| `ADBE Tritone` | 三色调 | 已实现 | 声明式 | 亮度权重 Rec.601（AE 26 实测） |
| `ADBE Easy Levels2` | 色阶 | 已实现 | 声明式 | 参数 2（直方图，任意数据）不声明，导入值原样保留；五个数值作用于"通道"选中的通道（AE 里各通道的值存在直方图数据里，尚未解码） |
| `ADBE Pro Levels2` | 色阶（单独控件） | 已实现 | 声明式 | 先单通道再 RGB 合成（顺序待实测） |
| `ADBE HUE SATURATION` | 色相/饱和度 | 已实现 | 声明式 | 主色相/饱和度/亮度和彩色化；"通道控制"选红色等时把主值限定到对应色相范围（AE 的各范围值存在参数 3 的任意数据里，尚未解码） |
| `ADBE Brightness & Contrast 2` | 亮度和对比度 | 已实现 | 声明式 | 新版曲线是近似公式，与 AE 26 差 ≤ 8 级（改用实测曲线表，进行中）；旧版为线性（差 0） |
| `ADBE Exposure2` | 曝光度 | 已实现 | 声明式 | 非线性工作空间按纯幂 2.4 转线性光（AE 26 实测，差 0） |
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
