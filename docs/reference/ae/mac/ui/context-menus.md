# After Effects 26.2.1（macOS）右键菜单

`context-menus.json` 是在本机 Mac 版 AE 26.2.1（英文界面）里实际右键、用辅助功能接口读出的原生菜单：条目文字、分隔线、子菜单、置灰、勾选和快捷键，只记结构和文字。读法见 `tools/ae/reference/dump_context_menu.swift`（AE 不把打开的菜单列在辅助功能树里，要在点击位置附近做命中测试找到菜单）。

测试工程：一个 1920×1080 合成，含活动摄像机、带蒙版的纯色、形状、文字、预合成、素材、空对象、灯光、调整图层。

| 键 | 位置 |
|---|---|
| `viewer.blank` | 合成查看器里没有图层的地方 |
| `viewer.layer.solid` | 合成查看器里的纯色图层上（末尾的"选择 ▸"是指针下的图层列表，没记） |
| `timeline.blank` | 时间轴图层区下面的空白处（和 `viewer.blank` 一样，只是"重命名"可用） |
| `timeline.header` | 时间轴列标题 |
| `project.blank` | 项目面板空白处 |
| `project.item.comp` / `.footage` / `.folder` | 项目面板里的合成 / 素材 / 文件夹上（三者条目相同，置灰不同） |
| `project.header` | 项目面板列标题 |

AE 按工程生成的条目照原样记：`Active Camera (Camera 1)`、`Reset Camera 1`、`Camera 1 Settings...`。效果 ▸ 下各类别里的效果列表就是效果菜单（见 `docs/reference/ae/effects.md`），没重复记。

图层菜单对纯色、形状、预合成、蒙版路径上、多选时条目都一样（只有置灰不同）；摄像机的"变换 ▸"里是"目标点…"而不是"锚点…"。Mac 上时间轴的图层右键菜单和查看器的一样。

还没读到的位置：时间标尺、工作区域条（试的位置没有弹出菜单）、时间轴里属性名和属性值上。中文名以 AE 包里的 zh_CN 词典为准（`$$$/AE/MenuID/0239/…` 是图层右键菜单）。
