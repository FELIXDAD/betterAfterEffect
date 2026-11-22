# After Effects 图层属性树总览

> 来源：Adobe After Effects 26.0x67 (zh_CN) | 平台：Windows 11 Pro (10.0.26200, x64) | 提取日期：2026-11-21

## 图层类型索引

| 图层类型 | 说明 | 文件 | 顶层属性组数 | 叶子属性总数 |
|---|---|---|---:|---:|
| `solid` | 纯色图层（2D、3D+遮罩及 3 种 3D 渲染器几何/材质选项） | `layers/solid.json` | 30 | 278 |
| `text` | 文字图层（含文本动画器、3 种选择器及全部动画属性） | `layers/text.json` | 10 | 276 |
| `shape` | 形状图层（含组、4 种路径、4 种描边/填充及 10 种操作器） | `layers/shape.json` | 10 | 332 |
| `camera` | 摄像机图层（双节点与单节点） | `layers/camera.json` | 6 | 52 |
| `light` | 灯光图层（平行光、聚光、点光、环境光、Environment 光） | `layers/light.json` | 15 | 130 |
| `null` | 空对象图层 | `layers/null.json` | 15 | 137 |
| `adjustment` | 调整图层 | `layers/adjustment.json` | 15 | 137 |
| `footage` | 素材图层（静态图像素材与音频素材） | `layers/footage.json` | 30 | 274 |
| `precomp` | 预合成图层（含时间重映射与音频） | `layers/precomp.json` | 15 | 137 |

## 各图层顶层属性组明细

### `solid` — 纯色图层（2D、3D+遮罩及 3 种 3D 渲染器几何/材质选项）

**变体 `solid2D`**（`matchName`: `ADBE AV Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 时间重映射 | `ADBE Time Remapping` | `PROPERTY` | 1 |
| 动态跟踪器 | `ADBE MTrackers` | `INDEXED_GROUP` | 0 |
| 蒙版 | `ADBE Mask Parade` | `INDEXED_GROUP` | 0 |
| 效果 | `ADBE Effect Parade` | `INDEXED_GROUP` | 0 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 图层样式 | `ADBE Layer Styles` | `NAMED_GROUP` | 97 |
| 几何选项 | `ADBE Plane Options Group` | `NAMED_GROUP` | 2 |
| 几何选项 | `ADBE Extrsn Options Group` | `NAMED_GROUP` | 5 |
| 材质选项 | `ADBE Material Options Group` | `NAMED_GROUP` | 17 |
| 音频 | `ADBE Audio Group` | `NAMED_GROUP` | 1 |
| 数据 | `ADBE Data Group` | `INDEXED_GROUP` | 0 |
| 基本属性 | `ADBE Layer Overrides` | `INDEXED_GROUP` | 0 |
| 集 | `ADBE Layer Sets` | `INDEXED_GROUP` | 0 |
| 替换源 | `ADBE Source Options Group` | `NAMED_GROUP` | 1 |

**变体 `solid3DWithMask`**（`matchName`: `ADBE AV Layer`, 3D: `True`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 时间重映射 | `ADBE Time Remapping` | `PROPERTY` | 1 |
| 动态跟踪器 | `ADBE MTrackers` | `INDEXED_GROUP` | 0 |
| 蒙版 | `ADBE Mask Parade` | `INDEXED_GROUP` | 4 |
| 效果 | `ADBE Effect Parade` | `INDEXED_GROUP` | 0 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 图层样式 | `ADBE Layer Styles` | `NAMED_GROUP` | 97 |
| 几何选项 | `ADBE Plane Options Group` | `NAMED_GROUP` | 2 |
| 几何选项 | `ADBE Extrsn Options Group` | `NAMED_GROUP` | 5 |
| 材质选项 | `ADBE Material Options Group` | `NAMED_GROUP` | 17 |
| 音频 | `ADBE Audio Group` | `NAMED_GROUP` | 1 |
| 数据 | `ADBE Data Group` | `INDEXED_GROUP` | 0 |
| 基本属性 | `ADBE Layer Overrides` | `INDEXED_GROUP` | 0 |
| 集 | `ADBE Layer Sets` | `INDEXED_GROUP` | 0 |
| 替换源 | `ADBE Source Options Group` | `NAMED_GROUP` | 1 |

### `text` — 文字图层（含文本动画器、3 种选择器及全部动画属性）

**变体 `text`**（`matchName`: `ADBE Text Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 文本 | `ADBE Text Properties` | `NAMED_GROUP` | 143 |
| 蒙版 | `ADBE Mask Parade` | `INDEXED_GROUP` | 0 |
| 效果 | `ADBE Effect Parade` | `INDEXED_GROUP` | 0 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 图层样式 | `ADBE Layer Styles` | `NAMED_GROUP` | 97 |
| 几何选项 | `ADBE Extrsn Options Group` | `NAMED_GROUP` | 5 |
| 材质选项 | `ADBE Material Options Group` | `NAMED_GROUP` | 17 |
| 音频 | `ADBE Audio Group` | `NAMED_GROUP` | 1 |
| 集 | `ADBE Layer Sets` | `INDEXED_GROUP` | 0 |

### `shape` — 形状图层（含组、4 种路径、4 种描边/填充及 10 种操作器）

**变体 `shape`**（`matchName`: `ADBE Vector Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 内容 | `ADBE Root Vectors Group` | `INDEXED_GROUP` | 199 |
| 蒙版 | `ADBE Mask Parade` | `INDEXED_GROUP` | 0 |
| 效果 | `ADBE Effect Parade` | `INDEXED_GROUP` | 0 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 图层样式 | `ADBE Layer Styles` | `NAMED_GROUP` | 97 |
| 几何选项 | `ADBE Extrsn Options Group` | `NAMED_GROUP` | 5 |
| 材质选项 | `ADBE Material Options Group` | `NAMED_GROUP` | 17 |
| 音频 | `ADBE Audio Group` | `NAMED_GROUP` | 1 |
| 集 | `ADBE Layer Sets` | `INDEXED_GROUP` | 0 |

### `camera` — 摄像机图层（双节点与单节点）

**变体 `twoNode`**（`matchName`: `ADBE Camera Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 摄像机选项 | `ADBE Camera Options Group` | `NAMED_GROUP` | 13 |

**变体 `oneNode`**（`matchName`: `ADBE Camera Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 摄像机选项 | `ADBE Camera Options Group` | `NAMED_GROUP` | 13 |

### `light` — 灯光图层（平行光、聚光、点光、环境光、Environment 光）

**变体 `light_parallel`**（`matchName`: `ADBE Light Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 灯光选项 | `ADBE Light Options Group` | `NAMED_GROUP` | 13 |

**变体 `light_spot`**（`matchName`: `ADBE Light Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 灯光选项 | `ADBE Light Options Group` | `NAMED_GROUP` | 13 |

**变体 `light_point`**（`matchName`: `ADBE Light Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 灯光选项 | `ADBE Light Options Group` | `NAMED_GROUP` | 13 |

**变体 `light_ambient`**（`matchName`: `ADBE Light Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 灯光选项 | `ADBE Light Options Group` | `NAMED_GROUP` | 13 |

**变体 `light_environment`**（`matchName`: `ADBE Light Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 灯光选项 | `ADBE Light Options Group` | `NAMED_GROUP` | 13 |

### `null` — 空对象图层

**变体 `null`**（`matchName`: `ADBE AV Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 时间重映射 | `ADBE Time Remapping` | `PROPERTY` | 1 |
| 动态跟踪器 | `ADBE MTrackers` | `INDEXED_GROUP` | 0 |
| 蒙版 | `ADBE Mask Parade` | `INDEXED_GROUP` | 0 |
| 效果 | `ADBE Effect Parade` | `INDEXED_GROUP` | 0 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 图层样式 | `ADBE Layer Styles` | `NAMED_GROUP` | 97 |
| 几何选项 | `ADBE Plane Options Group` | `NAMED_GROUP` | 2 |
| 几何选项 | `ADBE Extrsn Options Group` | `NAMED_GROUP` | 5 |
| 材质选项 | `ADBE Material Options Group` | `NAMED_GROUP` | 17 |
| 音频 | `ADBE Audio Group` | `NAMED_GROUP` | 1 |
| 数据 | `ADBE Data Group` | `INDEXED_GROUP` | 0 |
| 基本属性 | `ADBE Layer Overrides` | `INDEXED_GROUP` | 0 |
| 集 | `ADBE Layer Sets` | `INDEXED_GROUP` | 0 |
| 替换源 | `ADBE Source Options Group` | `NAMED_GROUP` | 1 |

### `adjustment` — 调整图层

**变体 `adjustment`**（`matchName`: `ADBE AV Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 时间重映射 | `ADBE Time Remapping` | `PROPERTY` | 1 |
| 动态跟踪器 | `ADBE MTrackers` | `INDEXED_GROUP` | 0 |
| 蒙版 | `ADBE Mask Parade` | `INDEXED_GROUP` | 0 |
| 效果 | `ADBE Effect Parade` | `INDEXED_GROUP` | 0 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 图层样式 | `ADBE Layer Styles` | `NAMED_GROUP` | 97 |
| 几何选项 | `ADBE Plane Options Group` | `NAMED_GROUP` | 2 |
| 几何选项 | `ADBE Extrsn Options Group` | `NAMED_GROUP` | 5 |
| 材质选项 | `ADBE Material Options Group` | `NAMED_GROUP` | 17 |
| 音频 | `ADBE Audio Group` | `NAMED_GROUP` | 1 |
| 数据 | `ADBE Data Group` | `INDEXED_GROUP` | 0 |
| 基本属性 | `ADBE Layer Overrides` | `INDEXED_GROUP` | 0 |
| 集 | `ADBE Layer Sets` | `INDEXED_GROUP` | 0 |
| 替换源 | `ADBE Source Options Group` | `NAMED_GROUP` | 1 |

### `footage` — 素材图层（静态图像素材与音频素材）

**变体 `image`**（`matchName`: `ADBE AV Layer`, 3D: `False`, 音频: `False`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 时间重映射 | `ADBE Time Remapping` | `PROPERTY` | 1 |
| 动态跟踪器 | `ADBE MTrackers` | `INDEXED_GROUP` | 0 |
| 蒙版 | `ADBE Mask Parade` | `INDEXED_GROUP` | 0 |
| 效果 | `ADBE Effect Parade` | `INDEXED_GROUP` | 0 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 图层样式 | `ADBE Layer Styles` | `NAMED_GROUP` | 97 |
| 几何选项 | `ADBE Plane Options Group` | `NAMED_GROUP` | 2 |
| 几何选项 | `ADBE Extrsn Options Group` | `NAMED_GROUP` | 5 |
| 材质选项 | `ADBE Material Options Group` | `NAMED_GROUP` | 17 |
| 音频 | `ADBE Audio Group` | `NAMED_GROUP` | 1 |
| 数据 | `ADBE Data Group` | `INDEXED_GROUP` | 0 |
| 基本属性 | `ADBE Layer Overrides` | `INDEXED_GROUP` | 0 |
| 集 | `ADBE Layer Sets` | `INDEXED_GROUP` | 0 |
| 替换源 | `ADBE Source Options Group` | `NAMED_GROUP` | 1 |

**变体 `audio`**（`matchName`: `ADBE AV Layer`, 3D: `False`, 音频: `True`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 时间重映射 | `ADBE Time Remapping` | `PROPERTY` | 1 |
| 动态跟踪器 | `ADBE MTrackers` | `INDEXED_GROUP` | 0 |
| 蒙版 | `ADBE Mask Parade` | `INDEXED_GROUP` | 0 |
| 效果 | `ADBE Effect Parade` | `INDEXED_GROUP` | 0 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 图层样式 | `ADBE Layer Styles` | `NAMED_GROUP` | 97 |
| 几何选项 | `ADBE Plane Options Group` | `NAMED_GROUP` | 2 |
| 几何选项 | `ADBE Extrsn Options Group` | `NAMED_GROUP` | 5 |
| 材质选项 | `ADBE Material Options Group` | `NAMED_GROUP` | 17 |
| 音频 | `ADBE Audio Group` | `NAMED_GROUP` | 1 |
| 数据 | `ADBE Data Group` | `INDEXED_GROUP` | 0 |
| 基本属性 | `ADBE Layer Overrides` | `INDEXED_GROUP` | 0 |
| 集 | `ADBE Layer Sets` | `INDEXED_GROUP` | 0 |
| 替换源 | `ADBE Source Options Group` | `NAMED_GROUP` | 1 |

### `precomp` — 预合成图层（含时间重映射与音频）

**变体 `precomp`**（`matchName`: `ADBE AV Layer`, 3D: `False`, 音频: `True`）

| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |
|---|---|---|---:|
| 标记 | `ADBE Marker` | `PROPERTY` | 1 |
| 时间重映射 | `ADBE Time Remapping` | `PROPERTY` | 1 |
| 动态跟踪器 | `ADBE MTrackers` | `INDEXED_GROUP` | 0 |
| 蒙版 | `ADBE Mask Parade` | `INDEXED_GROUP` | 0 |
| 效果 | `ADBE Effect Parade` | `INDEXED_GROUP` | 0 |
| 变换 | `ADBE Transform Group` | `NAMED_GROUP` | 12 |
| 图层样式 | `ADBE Layer Styles` | `NAMED_GROUP` | 97 |
| 几何选项 | `ADBE Plane Options Group` | `NAMED_GROUP` | 2 |
| 几何选项 | `ADBE Extrsn Options Group` | `NAMED_GROUP` | 5 |
| 材质选项 | `ADBE Material Options Group` | `NAMED_GROUP` | 17 |
| 音频 | `ADBE Audio Group` | `NAMED_GROUP` | 1 |
| 数据 | `ADBE Data Group` | `INDEXED_GROUP` | 0 |
| 基本属性 | `ADBE Layer Overrides` | `INDEXED_GROUP` | 0 |
| 集 | `ADBE Layer Sets` | `INDEXED_GROUP` | 0 |
| 替换源 | `ADBE Source Options Group` | `NAMED_GROUP` | 1 |

## 形状图层已展开内容（`ADBE Root Vectors Group`）

`ADBE Vector Group`, `ADBE Vector Shape - Rect`, `ADBE Vector Shape - Ellipse`, `ADBE Vector Shape - Star`, `ADBE Vector Shape - Group`, `ADBE Vector Graphic - Fill`, `ADBE Vector Graphic - Stroke`, `ADBE Vector Stroke Dash 1`, `ADBE Vector Stroke Gap 1`, `ADBE Vector Stroke Offset`, `ADBE Vector Graphic - G-Fill`, `ADBE Vector Graphic - G-Stroke`, `ADBE Vector Stroke Dash 1`, `ADBE Vector Stroke Gap 1`, `ADBE Vector Stroke Offset`, `ADBE Vector Filter - Merge`, `ADBE Vector Filter - Offset`, `ADBE Vector Filter - PB`, `ADBE Vector Filter - Repeater`, `ADBE Vector Filter - RC`, `ADBE Vector Filter - Trim`, `ADBE Vector Filter - Twist`, `ADBE Vector Filter - Roughen`, `ADBE Vector Filter - Wiggler`, `ADBE Vector Filter - Zigzag`

## 文字动画器已展开属性与选择器

`ADBE Text Animator`, `ADBE Text Selector`, `ADBE Text Wiggly Selector`, `ADBE Text Expressible Selector`, `ADBE Text Anchor Point 3D`, `ADBE Text Position 3D`, `ADBE Text Scale 3D`, `ADBE Text Skew`, `ADBE Text Skew Axis`, `ADBE Text Rotation`, `ADBE Text Rotation X`, `ADBE Text Rotation Y`, `ADBE Text Opacity`, `ADBE Text Fill Color`, `ADBE Text Fill Hue`, `ADBE Text Fill Saturation`, `ADBE Text Fill Brightness`, `ADBE Text Fill Opacity`, `ADBE Text Stroke Color`, `ADBE Text Stroke Hue`, `ADBE Text Stroke Saturation`, `ADBE Text Stroke Brightness`, `ADBE Text Stroke Opacity`, `ADBE Text Stroke Width`, `ADBE Text Line Anchor`, `ADBE Text Line Spacing`, `ADBE Text Track Type`, `ADBE Text Tracking Amount`, `ADBE Text Character Change Type`, `ADBE Text Character Range`, `ADBE Text Character Offset`, `ADBE Text Blur`
