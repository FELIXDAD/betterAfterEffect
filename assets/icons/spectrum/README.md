# Adobe Spectrum 图标（开源）

BAE 界面里的通用图标来自 Adobe 以 Apache-2.0 开源发布的 **Spectrum 1** 图标。这里只放 BAE 实际用到的那些（不是整包），由 `tools/icons/fetch_spectrum.py` 从 npm 下载固定版本、校验 sha512 后复制过来；再由 `tools/icons/build_icons.py` 转成压缩的填充路径数据嵌进程序。

| 目录 | 来源 | 处理 |
|---|---|---|
| `workflow/` | `@adobe/spectrum-css-workflow-icons` **1.5.4**，`dist/18/*.svg`（18 px 工作流图标，36×36 viewBox） | 原样复制，未修改 |
| `ui/` | `@adobe/react-spectrum-ui` **1.2.1**，`src/*.js` 里 medium 档的路径 | **已修改**：路径数据放进 18×18 的 SVG，字形居中（文件里有注释） |
| `modified/` | 上面的 `VectorDraw`、`Text`、`PinOn` | **已修改**：原图缩到 75% 放左上角，右下角加 BAE 自己画的小标记，做 AE 工具组里的变体（添加/删除/转换顶点、蒙版羽化、直排文字、Puppet 的各种控点）；由 `fetch_spectrum.py` 生成，文件里有注释 |

- 来源：<https://registry.npmjs.org/@adobe/spectrum-css-workflow-icons/-/spectrum-css-workflow-icons-1.5.4.tgz>（<https://github.com/adobe/spectrum-css-workflow-icons>），<https://registry.npmjs.org/@adobe/react-spectrum-ui/-/react-spectrum-ui-1.2.1.tgz>（<https://github.com/adobe/react-spectrum>）。
- 许可证：Apache License 2.0，全文见 `LICENSE`（取自 workflow 包；UI 包的 LICENSE 是同一份许可证，只有附录里的示例版权行不同）。
- 版权：`COPYRIGHT` 是 workflow 包里的原文；UI 包源文件头写的是 "Copyright 2024 Adobe. All rights reserved."。
- `NOTICE`：来源、版本、许可证和改动的英文汇总。
- `LICENSE`、`COPYRIGHT`、`NOTICE`、本文件随安装包一起发：macOS 在 `BAE.app/Contents/Resources/ThirdPartyNotices/Spectrum-Icons/`，Windows 在 zip 里的 `ThirdPartyNotices/Spectrum-Icons/`（`xtask/src/dist.rs`）。
- 只用 Spectrum 1，不混用 Spectrum 2。不含从 AE、Ai、Pr 安装包提取的任何图标或图片，不用 Adobe 的应用图标、启动画面和标志。Apache-2.0 不授予商标权，BAE 不使用 Adobe 的名称或商标做宣传。

更新：改 `tools/icons/fetch_spectrum.py` 里的清单，运行它，再运行 `python3 tools/icons/build_icons.py`。
