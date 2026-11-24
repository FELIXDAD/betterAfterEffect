# 实用工具

## 应用颜色 LUT

`ADBE Apply Color LUT2` 使用 `bae-color` 的 1D、3D 和组合 LUT 读取器，支持 `.cube`、`.spi1d`、`.spi3d`。文件名和内容保存在效果参数 `param:0001`。渲染时在 GPU 上按直通颜色查表，再乘回 Alpha。曲线、组合 LUT 和两种 Sony 格式另有 GPU 与 CPU 数值对照。

AE 26.2.1 添加该效果时会打开原生文件选择框。复现 `cases-color-3.json` 时选取仓库的 `crates/bae-color/tests/data/ocio/look3d.cube`。脚本在同一合成上切换位深，一次选择生成三张参考图。8、16、32 bpc 的最大通道差均为 1/255，平均差依次为 0.010、0.018、0.089 个 8 位色阶。

BAE 面板的「选择文件…」按钮由效果控件工作流接入。调用方用 `bae_color::lutfile::embed(path)` 校验文件，再用 `attr.set_at` 将 `Value::Bytes` 写入参数。按钮旁用 `embedded_name(&bytes)` 显示原文件名，返回 `None` 时显示 `<LUT not set>`。
