# apps/

可执行程序。`apps/bae` 是桌面应用入口（只做组装，见 `docs/spec/app-shell.md`）。每个子目录是一个 Cargo crate，会被 `apps/*` 通配自动加入工作区。
