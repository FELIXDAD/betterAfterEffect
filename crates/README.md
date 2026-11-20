# crates/

库 crate，统一用 `bae-` 前缀。每个子目录都必须有 `Cargo.toml`（`crates/*` 通配会把所有子目录当成工作区成员）。依赖版本统一写在根 `Cargo.toml` 的 `[workspace.dependencies]`，crate 里用 `xxx.workspace = true` 引用，见 `docs/spec/dependencies.md`。
