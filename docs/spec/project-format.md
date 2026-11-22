# 工程文件格式

> 状态：格式版本 1（工作流 02 的 PR C）。代码：`crates/bae-model/src/project.rs`、`asset.rs`。架构依据：`docs/ARCHITECTURE.md` 第 4.6、16 节。

## 1. 目录包 `.bae`

```text
MyFilm.bae/
  project.json                       工程清单
  sublayers/<工程层 ID>.loro          每个工程层一个 Loro 快照；ID 里的 / 变成子目录
                                     （anim/shot010 → sublayers/anim/shot010.loro）
  updates/<工程层>/<客户端>.log        共享目录协同的增量日志（09 负责，预留）
  assets/                            工程内素材（可选）
  cache/                             代理、缩略图、渲染缓存，可删，不进版本
```

- 保存时每个文件先写临时文件再改名，崩溃不会留下写了一半的文件；`project.json` 最后写。
- 工程层从层叠里删掉后，保存时会删除它残留的 `.loro`。

## 2. `project.json`

```json
{
  "format": "bae.project",
  "version": 1,
  "layer_stack": ["anim/shot010", "root"],
  "path_mappings": [{"from": "Z:/", "to": "/Volumes/NAS/"}]
}
```

- `layer_stack`：强 → 弱。
- `path_mappings`：团队的素材路径映射（见第 5 节），可省略。
- `view`：桌面应用保存时打开着的合成和各自的当前时间（`{"comps": [{"path": "/comps/Comp_1", "time": 0}], "active": "/comps/Comp_1"}`），打开工程时恢复（同 AE）。不是工程数据：不经过命令、不算未保存的修改，可省略；由 `apps/bae/src/view_state.rs` 读写。打包（`bae pack`、`package.pack`）时和其他字段一起带进 `.baez`，打开包时恢复（#131）。
- 不认识的字段原样保留，保存时写回（新版本加的字段不会被旧版本丢掉）。

### 版本与迁移

- `version` 比当前程序支持的新：拒绝打开，提示升级 BAE（不会尝试读取以免损坏）。
- 比当前旧：按 `MIGRATIONS` 依次升级（`v1 → v2 → …`），每一步是对 JSON 的一个函数。格式变化时：版本号 +1，加一个迁移函数和测试。

## 3. 单文件 `.baez`

- zip 容器，内部布局与目录包相同。`project.json` 和 `sublayers/` 用 deflate 压缩；`assets/` 下的文件**不压缩**（stored，可直接定位读取媒体），开启 zip64 支持大文件。
- `Project::open` 可以直接打开 `.baez`，不需要先解包；`extract_baez` 解包成普通目录（只解出安全的相对路径）。
- `Project::write_baez(path, assets)` 写出时附带素材；收集用到的素材、视频裁片由 21 packaging 负责，它调用这个接口。
- `.baez` 里相对路径（素材、跨工程引用）以 `.baez` 文件所在目录为基准；目录包以 `.bae` 目录本身为基准。

## 4. 跨工程引用

`Reference.project` 是被引用工程相对本工程基准目录的路径（如 `../Library.bae`）。`Project::open` 默认会打开这些工程并挂到 Stage 上（`OpenOptions::load_references`），嵌套最多 8 层，互相引用会被截断；打不开的引用不会让打开失败，`Stage::unresolved_references()` 会列出来。

## 5. 素材引用

`Asset` Prim 上的属性：

| 属性 | 类型 | 说明 |
|---|---|---|
| `file:relative` | string | 相对工程基准目录的路径，`/` 分隔（不在同一卷上时没有） |
| `file:original` | string | 导入时的绝对路径，`/` 分隔，保留 Windows 盘符（`Z:/footage/a.mov`） |
| `file:size` | int | 字节数 |
| `file:mtime` | int | 修改时间，Unix 毫秒 |
| `file:hash` | string | 内容的 BLAKE3（可选，大文件较慢） |

查找顺序（`AssetResolver::resolve`）：相对路径 → 原始绝对路径 → 依次套用路径映射 → 在工程目录和搜索目录里按文件名 + 指纹查找（大小一致；有哈希时哈希也要一致；默认深度 4 层，最多看 2 万个条目）→ 缺失。

重新链接：用户找到一个缺失素材后，`asset::infer_mapping(原路径, 新路径)` 取两条路径相同的尾部，得到前缀映射（如 `Z:/` → `/Volumes/NAS/`），用它可以自动找回同一批搬走的其他素材。`asset.set_file` 命令写入新的引用。

## 6. `bae export-json`

```text
bae export-json <工程>                    合成后的整个工程（所有层平铺、引用展开）
bae export-json <工程> --sublayer <ID>    某个工程层自己的意见
bae export-json <工程> --prim <路径>      合成后的某个 Prim 子树
            [-o 文件] [--compact]
```

输出 `{"format": "bae.export", "version": 1, ...}`，Prim 是 `PrimTree` 的 JSON（见 `data-model.md` 第 3 节）。示例工程：`cargo run -p bae-model --example sample_project -- Sample.bae`。
