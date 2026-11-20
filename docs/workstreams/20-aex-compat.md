# 20 · AE 插件兼容（预留）

| 项 | 内容 |
|---|---|
| 分支 | `interop/aex-compat` |
| Orca 工作区 | `bae-20-aex-compat` |
| 阶段 | P1 只做设计与接口预留；兼容层实现是 P3 |
| 依赖 | 02、04、19 |
| 负责目录 | `crates/bae-aex-host/`、`docs/spec/aex-compat.md` |

## 目标

现在就把"将来能运行 AE 插件"需要的底层能力想清楚，并确保 02（效果参数模型）、04（渲染图）、19（插件接口）的设计不会挡路。现阶段不要求能运行任何 .aex。背景见 `docs/ARCHITECTURE.md` 第 15 节。

## 要做（现阶段）

- **调研**：基于 Adobe 公开的 AE SDK 文档，整理效果插件和 AEGP 插件的工作方式：入口与命令选择器、PiPL 资源、参数类型与标志、像素世界（PF_EffectWorld）与位深、SmartFX、序列数据、图层 checkout、多帧渲染线程安全、GPU 渲染、自定义界面（Drawbot / 事件）、常用 suite 列表、macOS .plugin 与 Windows .aex 的打包差异。写入 `docs/spec/aex-compat.md`。
- **差距评审**：逐项对照 02/04/19 的接口，列出"已能映射 / 需要预留 / 暂不考虑"，并把需要预留的点作为请求写进对应任务书的"请求"一节。
- **骨架**：`crates/bae-aex-host` 只放接口和数据结构的翻译层设计（不含 Adobe 头文件、不加载任何插件），编译通过即可。
- **性能评估**：估算 CPU 往返（GPU→CPU→插件→GPU）对单个效果节点的耗时，说明不会影响不使用 AE 插件的工程。
- **合规评估**：列出实现兼容层需要确认的许可与法律问题，供以后决策。

## 验收

- `docs/spec/aex-compat.md` 覆盖上面的调研项，并有完整的映射表。
- 02/04/19 的负责人确认已收到并处理预留请求。

## 不做

- 现阶段不加载、不运行任何 AE 插件；不分发 Adobe SDK 头文件或代码。

## 进度

- [ ] SDK 调研文档
- [ ] 对 02/04/19 的差距评审与请求
- [x] crates/bae-aex-host 骨架（PR 1）：只有数据结构和纯函数，不加载任何插件、没有 FFI、没有 Adobe 头文件或 ABI 数值。
  - 已加入工作区，继承工作区的 package 字段和 lints；不被 `apps/bae` 依赖，release 体积增加 0。
  - `selector::LifecycleHook` 按 19 的 C 回调名（`BaeEffectVTable1`）一一对应，测试直接读取 `sdk/c/bae_plugin.h` 核对，19 的接口有增减时测试会失败（回应 19 的请求 R20-1）。
  - `caps::Concurrency` 对应 `BAE_THREADING_*`。
  - 新增 `model` 模块：把 AE 的参数和效果描述转换成已合并的 `bae_model::EffectDef`，02 目前表达不了的地方列为 `ModelGap`，并注明由哪条预留请求解决。
  - `bench/roundtrip`：GPU↔CPU 往返基准（独立 package，wgpu 不进入主工作区）。
- [ ] 性能与合规评估

## 请求（其他工作流在这里留言）
