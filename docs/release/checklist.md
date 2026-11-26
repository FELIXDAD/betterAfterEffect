# BAE 0.1.0 发布前检查清单

发布负责人对最终候选版本逐项确认。运行记录应指向同一版代码；候选版本变化后，更新受影响的记录。

## 打标签前

- [ ] **版本和说明**。Cargo 工作区和“关于 BAE”显示 0.1.0，macOS 的两个 Bundle 版本为 0.1.0，Windows 文件版本和产品版本为 0.1.0.0。用 `gh pr list --base dev --state merged` 核对从 v0.0.1 到候选版本的合入项，补齐最后一批功能与已知问题。[发布说明](0.1.0.md)
- [ ] **完整 CI**。候选版本的 `CI full` 中，macOS arm64 和 Windows x64 的 `full` 作业均为 success，记录运行链接。仅打包的 `package` 作业用于试包。[完整 CI 运行记录](https://github.com/FELIXDAD/betterAfterEffects/actions/workflows/ci.yml)
- [ ] **P0 清零**。同时检查开放问题的标题和标签。逐项核对修复、复测和关闭状态；在结果为零时记录查询日期。[标题含 P0](https://github.com/FELIXDAD/betterAfterEffects/issues?q=is%3Aissue+is%3Aopen+P0+in%3Atitle) · [标签为 P0](https://github.com/FELIXDAD/betterAfterEffects/issues?q=is%3Aissue+is%3Aopen+label%3AP0)
- [ ] **看板完成度**。在 0.1.0 里程碑核对 M1、常用效果、音频播放、粒子、绘画和 release 各项的验收状态；记录完成项数、总项数和阻塞项。[BAE 进度看板](https://board.rimlight.studio/)
- [ ] **测试员复测**。用候选 DMG 和 ZIP 安装并打开应用，核对“关于”版本，验证工程重开、预览带音频、效果、粒子、绘画和视频导出。Windows、macOS 各填写一行，附已修复 P0 的复测结论。[Windows 测试通道](https://github.com/FELIXDAD/betterAfterEffects/issues/253)
- [ ] **包与体积**。下载双平台构件，检查下表中的文件、第三方许可证、签名状态和各自的 `size-report.json`。可执行文件预算为 Windows 50 MiB、macOS 38 MiB。[仓库体积预算](../../tools/size-budget/budget.toml)

| 测试平台 | 候选包 / 来源运行 | 测试员、日期 | 复测结果与记录链接 |
| --- | --- | --- | --- |
| Apple Silicon Mac | 待填 | 待填 | 待复测 |
| Windows x64 | 待填 | 待填 | 待复测 |

准备阶段在 Windows x64 上运行 `cargo xtask size`，以 dev 合入 #656 后的代码构建。0.1.0 release 版 `bae.exe` 为 **42,910,208 字节（40.92 MiB）**，距 50 MiB 上限还有 **9.08 MiB**。Windows ZIP 为 **15.79 MiB**。正式发布时以 tag 对应的双平台报告为准。

## 生成与核对发布包

在候选分支上可手动运行 `CI full` 并勾选 `packages_only`，生成与 tag 相同格式的试包。在对应平台本地可运行 `cargo xtask size --json size-report.json`，随后运行 `cargo xtask dist --no-build`。安装文件位于工作树的 `target/dist/`。

| 构件 | 校验对象 |
| --- | --- |
| `BAE-0.1.0-aarch64-macos.dmg` | 从映像复制出 `BAE.app`，验证可执行权限、Info.plist 版本和整个应用的 ad-hoc 签名。记录本次未做 Developer ID 签名及公证。 |
| `BAE-0.1.0-x86_64-windows.zip` | 解压并验证 `bae.exe` 的版本资源、产品名称与启动结果，检查 LICENSE 和 ThirdPartyNotices。 |

## 负责人确认后

- [ ] 负责人确认发布范围及上述记录，确定发布提交并创建 `v0.1.0`。tag 触发两个平台的完整 CI，工作流检查 tag 与应用版本一致后打包。
- [ ] 取该 tag 的成功运行中两个 `bae-<target>` 构件内的 DMG 和 Windows ZIP，由负责人附到 GitHub Release 并发布中文说明。核对公开附件可下载、文件名和版本正确，回填发布链接及看板状态。
