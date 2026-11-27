# 示例插件

这些示例与设计文档 `docs/spec/plugin-architecture.md`（英文，规范性文本）一一对应，用来说明三种插件形态怎么写。它们都有自动测试：`cargo test -p bae-plugin` 会用宿主的同一套代码加载每个示例包，检查清单，并把 WGSL 和宿主生成的前导代码（prelude）一起用 naga 编译、校验；两个原生示例由 `cargo test -p bae-plugin --test native` 编译后加载、渲染并核对像素；纯 C 示例还由 `sdk/c/tests/check.sh` 用只依赖头文件的迷你宿主跑通 C ABI 调用流程。

| 目录 | 形态 | 演示内容 | 规范章节 |
|---|---|---|---|
| `posterize/` | 声明式 WGSL，像素式 | 最小的完整效果：整数、复选框参数；`identity_when`；快速开始文档 `docs/plugins/quickstart.md` 用的就是它 | §5.3、§11.4 |
| `vignette/` | 声明式 WGSL，像素式（pixel style） | 浮点、点、弹出菜单、颜色参数；`identity_when`；线性光混合；可以与相邻像素式效果融合成一个内核 | §5.3.6、§11.3、§11.4、§10.9 |
| `box-blur/` | 声明式 WGSL，计算式（compute style） | 两个 pass + 中间纹理；ROI（`read`、`grows`）；随降采样缩放半径 | §11.5、§11.6、§11.7 |
| `composition-report/` | 脚本插件 | 只读菜单与 MCP 自动化，返回合成摘要 | §17 |
| `fade-selected/` | 脚本插件 | 以播放头为起点给选中图层添加淡入关键帧，支持一次撤销 | §17 |
| `align-to-grid/` | 脚本插件（JavaScript） | 命令（菜单、快捷键、MCP 工具）+ 自动化脚本（`[[automation]]`）；参数 JSON Schema；权限；一次调用 = 一个撤销步骤 | §5.3.8、§17 |
| `../../sdk/c/examples/invert/` | 原生插件（纯 C，只依赖 `bae_plugin.h`） | 不用任何 SDK，直接实现 C ABI；进程内运行 | §14 |
| `duotone-rs/` | 原生插件（Rust，`bae-plugin-sdk`） | `CpuEffect` + `export_plugin!`；参数来自 `define_params`（清单不写参数表）；进程外运行 | §8、§14、§15 |

后续阶段还会补充：原生 GPU 互操作效果（`bae.gpu.native`）、导入器、编解码器示例。

## 目录约定

每个示例都是一个完整的插件包（package），根目录有 `bae-plugin.toml`，可以直接打包成 `.baeplugin`（zip）：

```
<插件目录>/
  bae-plugin.toml      清单（必需）
  shaders/*.wgsl       声明式效果的着色器
  scripts/*.js         脚本入口模块
  schemas/*.json       命令参数的 JSON Schema
  locales/zh-CN.toml   中文翻译（清单里的英文是默认值）
```
