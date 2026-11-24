# 界面图标

界面里的图标（工具栏、面板标签、时间轴开关、查看器按钮、走带控制等）从这里的 SVG 生成。应用运行时不解析 SVG，也不带 SVG 库。

| 路径 | 内容 |
|---|---|
| `manifest.json` | 图标清单：`Icon` 枚举的变体名 → SVG 文件（相对本目录）。只有清单里的图标会打包 |
| `spectrum/` | Adobe 以 Apache-2.0 开源的 Spectrum 1 图标（只放用到的），含许可证、版权声明、NOTICE 和来源说明（见 `spectrum/README.md`） |
| `bae/` | BAE 自己画的图标（AE 专有、Spectrum 里没有的：时间轴开关、坐标轴模式、窗格开关、走带控制、3D 形状等） |
| `icons.bin` | 生成物：所有图标的填充路径数据，zlib 压缩，`include_bytes!` 嵌进 `bae-ui`。**不要手改** |
| `app/` | 应用图标（另一套流程，见 `app/README.md`） |

## 重新生成

```sh
python3 tools/icons/fetch_spectrum.py                  # 更新 Spectrum 图标（npm 固定版本，校验 sha512）
python3 tools/icons/build_icons.py                    # 改了 SVG 或清单后重建 icons.bin
python3 tools/icons/build_icons.py --check            # 检查 icons.bin 是否和 SVG 一致
python3 tools/icons/build_icons.py --preview out.png  # 另存一张 16/18/32 px 的预览图
```

只需要 Python 3.9 以上的标准库。

## 工作方式

1. 构建前：脚本把每个 SVG 的形状（`path`、`rect`、`circle`、`ellipse`、`polygon`、`polyline`、`line`，含 `transform`）变成填充轮廓（直线、二次和三次曲线，非零环绕规则）。描边按线宽展开成填充；圆、椭圆、圆角矩形的描边是精确的圆环。`fill-rule="evenodd"` 按嵌套层数改写成非零规则。
2. 运行时（`crates/bae-ui/src/icons/vector.rs`）：某个图标第一次以某个像素尺寸出现时，用自带的精确面积覆盖率扫描填充光栅化一次（几十行代码），放进一张 512×512 的图集纹理；之后每次绘制只是一个带纹理的方块，顶点颜色就是主题色。所以 1×、125%、150%、2× 下都是按实际像素画的，不会因为缩放位图而发虚。小于 14 px 的图标会稍微加粗覆盖率，让细线看得清。
3. 每个 `Icon` 都必须在清单里（`cargo test -p bae-ui` 会检查）。

`fill` 不写或写 `currentColor` 表示跟随主题色（调用方传进来的颜色）；写具体颜色（如合成图标里的红、黄、蓝）表示这一层固定用这个颜色，只跟随调用方颜色的透明度。

## 自绘图标的规范

和 Spectrum 1 工作流图标保持一致：

- 36×36 的 `viewBox`（显示时 18 px 对应 1 个设计单位 = 0.5 px），主体落在 3–33 之间。
- 线宽 2（细节可用 1.5，强调可用 2.5），圆头圆角（`stroke-linecap="round"`、`stroke-linejoin="round"`）。
- 能用实心表达的用实心；空心形状里不要再塞太多细节，16 px 下要能一眼认出来。
- 下拉箭头统一放在右下角：`<path d="M27 30h8l-4 4Z"/>`。

## 来源

- `spectrum/`：Adobe 开源的 Spectrum 1 图标（`@adobe/spectrum-css-workflow-icons` 1.5.4、`@adobe/react-spectrum-ui` 1.2.1，Apache-2.0）。许可证文件随安装包发布。
- `bae/`：BAE 自己画的。
- 没有使用从 AE、Ai、Pr 安装包里提取的图标或图片，没有使用第三方的“Adobe 图标包”，不用 Adobe 的应用图标、启动画面和标志。
