# bae-package：工程与素材打包

工作流 21。设计依据：`docs/ARCHITECTURE.md` 第 16 节、`docs/spec/project-format.md` 第 3、5 节。

不依赖 GPU、窗口和平台 API，命令可以在无界面环境（Agent、CI、Hub）里运行。

## 能做什么

| 功能 | 接口 | 命令 |
|---|---|---|
| 检查素材：哪些用到、哪些没用到、哪些缺失、用了哪些字体 | `scan`、`collect::collect` | `package.scan` |
| 打包成单文件 `.baez` | `pack` | `package.pack`；命令行 `bae pack` |
| 收集文件到文件夹（AE 的"收集文件"） | `collect_files` | `package.collect` |
| 解包成普通 `.bae` 目录 | `unpack` | `package.unpack` |
| 直接打开 `.baez`，素材从包内读取 | `open_project`、`PackageReader` | — |
| 重新链接一个素材，并按同样的目录关系找回其他缺失素材 | `plan_relink` | `asset.relink` |
| 一次设置多个素材的文件引用（一个撤销步骤） | `commands::write_asset_file` | `asset.set_files` |

## 素材收集

- 遍历工程里所有 `Asset` Prim。一个素材"被用到"指：某个 Prim 的属性（静态值或关键帧）用 `asset_ref` / `prim_ref` 指向它，例如图层的 `source`、剪辑片段的 `source`。
- 不指定范围时，整个工程里的合成、图层、片段都算（对应 AE"收集文件"的"所有合成"）。
- 指定范围（`roots`，通常是选中的合成）时，只算这些合成以及它们引用的预合成，递归到底。
- 同时记录文字图层用到的字体（PostScript 名）和引用的其他工程（跨工程引用不会被打进包里，报告里会提示）。

## 查找素材文件

依次查找，先找到先用：

1. 工程是从 `.baez` 直接打开的：包内的 `assets/…` 条目；
2. 相对工程的路径（`file:relative`）；
3. 原始绝对路径（`file:original`）；
4. 依次套用路径映射（工程里的 `path_mappings` 加上调用方给的），例如 `Z:/` → `/Volumes/NAS/`、`//server/share/` → `/Volumes/share/`；
5. 在工程目录和搜索目录里按文件名 + 指纹（大小，记录了哈希时还要哈希一致）查找；
6. 缺失。

**重新链接**：用户找到一个缺失素材后，`asset.relink` 用新旧两条路径相同的尾部推出目录映射（如 `Z:/` → `/Volumes/NAS/`），再用这个映射和"同一个文件夹"两种方式找回其他缺失素材，全部写入作为一个撤销步骤，并返回推出的映射（可以加进工程的路径映射）和仍然缺失的素材。

## 打包

- `.baez` 是 zip：`project.json` 和工程层数据压缩存放；素材放在 `assets/` 下、**不压缩**（可以直接定位读取），大文件用 zip64。
- 包内的工程是一份副本：被打包素材的 `file:relative` 改成 `assets/…`；原始路径、大小、修改时间、哈希保留，用于追溯。**原工程和原始素材文件不做任何修改**（有测试检查内容和修改时间）。
- 素材在包里的名字：普通文件放在 `assets/<文件名>`，重名时加 `_2`、`_3`；图片序列放在 `assets/<序列名>/`，保留原帧文件名。多个素材引用同一个文件时只打包一次。
- 图片序列打包它的全部帧（只打包用到的帧和视频裁片在后续阶段做）。
- 选项（`PackOptions`）：
  - `roots`：打包范围，空表示整个工程；
  - `include_unused`：是否也打包没用到的素材（默认是）。不打包时这些素材保留原来的外部引用；
  - `fail_on_missing`：有缺失素材时直接失败（默认否：照常打包并在报告里列出）；
  - `search_dirs`、`path_mappings`：额外的搜索目录和路径映射；
  - `fonts`：`record`（默认，只记录字体名）或 `embed`（打包字体文件，需注意字体许可；**暂未实现**，会提示并只记录字体名）；
  - `trim`：视频裁片参数（前后余量、直拷/重编码、目标编码），**已预留，暂未实现**，会提示并打包完整文件。
- 输出已存在时默认拒绝（可以选择覆盖）；不允许输出到工程自身或任何素材文件上。"收集文件"和"解包"的目标目录必须不存在或为空。

## 直接打开 `.baez`

`open_project(path)` 打开 `.bae` 目录或 `.baez` 文件，返回工程和 `AssetLocator`。`locate` 得到素材位置：

- `AssetLocation::Packaged { entry }`：在包内。`PackageReader::entry` 给出字节偏移和长度（素材不压缩，就是 `.baez` 文件里的一段）；`open_entry` 返回可 `Read + Seek` 的读取器；需要文件路径的解码器用 `materialize` 解出到缓存目录。
- `AssetLocation::File { path, via }`：在磁盘上，以及是怎么找到的。

## 报告

`PackageReport`（可以序列化成 JSON）：每个素材的状态（已打包 / 缺失 / 未用到而排除 / 无文件）、来源位置、在包里的路径、文件数和字节数；用到的字体；引用的其他工程；警告（带稳定的 `code`，界面据此做中英文提示）；统计（素材数、打包文件数、素材字节数、工程数据字节数、输出大小）。

## 打包后的 `project.json`

包内工程的 `project.json` 保留原工程清单的其他字段（路径映射、桌面应用保存的视图 `view` 等；命令只拿到工程层时从 `project` 位置读清单，`read_manifest`），并多一个 `package` 字段，记录打包工具版本、用到的字体、是否裁片、打包范围：

```json
"package": { "tool": "bae-package 0.1.0", "fonts": ["Inter-Regular"], "trimmed": false }
```

旧版本程序会原样保留这个字段。
