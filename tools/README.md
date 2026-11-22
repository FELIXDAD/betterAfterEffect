# tools/

构建辅助工具与配置：

- `size-budget/budget.toml`：release 体积预算，`cargo xtask size` 和 CI 用它检查。
- `dep-probe/`：编译期检查 wgpu / vello / egui-wgpu / parley 是否共用同一套版本（不进入发布产物）。
