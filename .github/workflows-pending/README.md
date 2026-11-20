# 待启用的 CI

`ci.yml` 是本仓库的 CI（macOS arm64 + Windows x64：fmt、clippy、测试、单一 wgpu 检查、release 体积预算、无界面冒烟测试、打包产物）。

它暂时放在这里而不是 `.github/workflows/`，因为开发 Agent 使用的 GitHub 令牌没有 `workflow` 权限，GitHub 会拒绝任何修改 `.github/workflows/` 的推送。有 `workflow` 权限的人（例如先执行 `gh auth refresh -s workflow`）执行一次下面的命令即可启用：

```sh
git mv .github/workflows-pending/ci.yml .github/workflows/ci.yml
git rm .github/workflows-pending/README.md
git commit -m "ci: enable workflow"
```
